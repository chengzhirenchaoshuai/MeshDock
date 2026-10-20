//! 路径变化、模式开错、延迟偏高、持续中继的通知：判断逻辑是纯函数，发送通知由调用方完成
use crate::classify::ConnKind;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// 新路径持续多久才通知，避免“先中继、几秒后直连”的正常打洞过程误报
pub const STABLE_FOR: Duration = Duration::from_secs(30);

/// 只关心直连和中继两类；离线、空闲、打洞中等不参与判断
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathClass {
    Direct,
    Relay,
}

impl PathClass {
    pub fn of(kind: ConnKind) -> Option<Self> {
        match kind {
            ConnKind::Lan | ConnKind::Ipv6 | ConnKind::Public => Some(Self::Direct),
            ConnKind::Relay | ConnKind::PeerRelay => Some(Self::Relay),
            _ => None,
        }
    }
}

/// 一次需要通知的路径变化
#[derive(Debug, PartialEq, Eq)]
pub struct PathChange {
    pub id: String,
    pub from: PathClass,
    pub to: PathClass,
}

/// 记录每台设备“稳定的”路径，新路径持续 STABLE_FOR 以上才算变化
#[derive(Default)]
pub struct PathWatcher {
    stable: HashMap<String, PathClass>,
    /// 设备 id → (候选的新路径, 首次看到的时间)
    pending: HashMap<String, (PathClass, Instant)>,
}

impl PathWatcher {
    /// 记录一次观察结果，返回需要通知的变化。第一次看到的设备只记录、不通知
    pub fn observe(
        &mut self,
        id: &str,
        class: Option<PathClass>,
        now: Instant,
    ) -> Option<PathChange> {
        let Some(class) = class else {
            // 离线、空闲等：不改变稳定路径，候选作废
            self.pending.remove(id);
            return None;
        };
        let Some(&stable) = self.stable.get(id) else {
            self.stable.insert(id.to_string(), class);
            return None;
        };
        if class == stable {
            self.pending.remove(id);
            return None;
        }
        let since = match self.pending.get(id) {
            Some(&(c, t)) if c == class => t,
            _ => {
                self.pending.insert(id.to_string(), (class, now));
                now
            }
        };
        if now.duration_since(since) < STABLE_FOR {
            return None;
        }
        self.pending.remove(id);
        self.stable.insert(id.to_string(), class);
        Some(PathChange {
            id: id.to_string(),
            from: stable,
            to: class,
        })
    }
}

/// 模式与位置不一致的情况
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mismatch {
    /// 在家却开着出门模式（本地流量会绕进隧道）
    HomeButAway,
    /// 在外面却没开出门模式（访问不到家里局域网）
    AwayButHome,
}

impl Mismatch {
    pub fn of(at_home: Option<bool>, accept_routes: Option<bool>) -> Option<Self> {
        match (at_home, accept_routes) {
            (Some(true), Some(true)) => Some(Self::HomeButAway),
            (Some(false), Some(false)) => Some(Self::AwayButHome),
            _ => None,
        }
    }

    pub fn message(self) -> (&'static str, &'static str) {
        match self {
            Self::HomeButAway => (
                "你在家，却开着出门模式",
                "家里局域网的流量会绕进隧道，可能变慢甚至断网。打开 MeshDock 一键改回在家模式。",
            ),
            Self::AwayButHome => (
                "你在外面，但没开出门模式",
                "现在访问不到家里局域网的设备。需要时打开 MeshDock 切换到出门模式。",
            ),
        }
    }
}

/// 模式开错的通知只在“刚出现”时发一次，持续不重复
#[derive(Default)]
pub struct MismatchWatcher {
    last: Option<Mismatch>,
}

impl MismatchWatcher {
    pub fn observe(&mut self, current: Option<Mismatch>) -> Option<Mismatch> {
        let changed = current != self.last;
        self.last = current;
        if changed { current } else { None }
    }
}

/// 延迟连续多少次后台测速超过阈值才提醒，避免偶尔一次抖动就打扰
pub const LATENCY_STREAK: usize = 3;

/// 延迟持续偏高：连续 LATENCY_STREAK 次超过阈值时提醒一次，回落到阈值以下后重新计数。
/// 没有回应的测量不计入也不清零（离线由路径和出口通知负责）
#[derive(Default)]
pub struct LatencyWatcher {
    streak: HashMap<String, usize>,
    alerted: HashSet<String>,
}

impl LatencyWatcher {
    /// 记录一次测量，需要提醒时返回 true
    pub fn observe(&mut self, id: &str, ms: Option<f64>, threshold: f64) -> bool {
        let Some(ms) = ms else {
            return false;
        };
        if ms <= threshold {
            self.streak.remove(id);
            self.alerted.remove(id);
            return false;
        }
        let n = self.streak.entry(id.to_string()).or_default();
        *n += 1;
        *n >= LATENCY_STREAK && self.alerted.insert(id.to_string())
    }
}

/// 持续走中继：连续走中继超过设定时长时提醒一次，离开中继（直连、离线、空闲）后重新计时
#[derive(Default)]
pub struct RelayWatcher {
    since: HashMap<String, Instant>,
    alerted: HashSet<String>,
}

impl RelayWatcher {
    /// 记录一次观察，需要提醒时返回已走中继的时长
    pub fn observe(&mut self, id: &str, relay: bool, now: Instant, limit: Duration) -> Option<Duration> {
        if !relay {
            self.since.remove(id);
            self.alerted.remove(id);
            return None;
        }
        let since = *self.since.entry(id.to_string()).or_insert(now);
        let elapsed = now.duration_since(since);
        (elapsed >= limit && self.alerted.insert(id.to_string())).then_some(elapsed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 延迟连续三次超标才提醒_回落后重新计数() {
        let mut w = LatencyWatcher::default();
        assert!(!w.observe("a", Some(300.0), 200.0));
        assert!(!w.observe("a", None, 200.0)); // 没有回应：不计入也不清零
        assert!(!w.observe("a", Some(250.0), 200.0));
        assert!(w.observe("a", Some(260.0), 200.0));
        assert!(!w.observe("a", Some(400.0), 200.0), "持续超标不重复提醒");
        assert!(!w.observe("a", Some(20.0), 200.0));
        assert!(!w.observe("a", Some(300.0), 200.0));
        assert!(!w.observe("a", Some(300.0), 200.0));
        assert!(w.observe("a", Some(300.0), 200.0), "回落后再次连续超标会重新提醒");
        assert!(!w.observe("b", Some(200.0), 200.0), "等于阈值不算超标");
    }

    #[test]
    fn 持续中继超过时长提醒一次() {
        let t0 = Instant::now();
        let at = |s: u64| t0 + Duration::from_secs(s);
        let limit = Duration::from_secs(600);
        let mut w = RelayWatcher::default();
        assert_eq!(w.observe("a", true, at(0), limit), None);
        assert_eq!(w.observe("a", true, at(599), limit), None);
        assert_eq!(w.observe("a", true, at(601), limit), Some(Duration::from_secs(601)));
        assert_eq!(w.observe("a", true, at(9999), limit), None);
        assert_eq!(w.observe("a", false, at(10000), limit), None);
        assert_eq!(w.observe("a", true, at(10001), limit), None, "离开中继后重新计时");
    }

    const D: Option<PathClass> = Some(PathClass::Direct);
    const R: Option<PathClass> = Some(PathClass::Relay);

    #[test]
    fn 第一次看到不通知_持续_30_秒才通知() {
        let t0 = Instant::now();
        let at = |s: u64| t0 + Duration::from_secs(s);
        let mut w = PathWatcher::default();
        assert_eq!(w.observe("a", D, at(0)), None);
        assert_eq!(w.observe("a", R, at(10)), None);
        assert_eq!(w.observe("a", R, at(30)), None);
        assert_eq!(
            w.observe("a", R, at(40)),
            Some(PathChange {
                id: "a".into(),
                from: PathClass::Direct,
                to: PathClass::Relay
            })
        );
        // 已经通知过，持续中继不再重复
        assert_eq!(w.observe("a", R, at(100)), None);
    }

    #[test]
    fn 打洞时短暂中继不通知() {
        let t0 = Instant::now();
        let at = |s: u64| t0 + Duration::from_secs(s);
        let mut w = PathWatcher::default();
        w.observe("a", D, at(0));
        assert_eq!(w.observe("a", R, at(10)), None);
        assert_eq!(w.observe("a", D, at(20)), None); // 回到直连，候选作废
        assert_eq!(w.observe("a", R, at(50)), None); // 重新计时
        assert_eq!(w.observe("a", R, at(70)), None);
    }

    #[test]
    fn 离线和空闲不算变化() {
        let t0 = Instant::now();
        let at = |s: u64| t0 + Duration::from_secs(s);
        let mut w = PathWatcher::default();
        w.observe("a", R, at(0));
        assert_eq!(w.observe("a", None, at(100)), None);
        // 空闲后重新直连，与稳定路径（中继）不同，持续后通知
        assert_eq!(w.observe("a", D, at(200)), None);
        assert_eq!(
            w.observe("a", D, at(240)).map(|c| c.to),
            Some(PathClass::Direct)
        );
    }

    #[test]
    fn 模式开错只在刚出现时通知() {
        let mut w = MismatchWatcher::default();
        assert_eq!(w.observe(None), None);
        let m = Mismatch::of(Some(true), Some(true));
        assert_eq!(w.observe(m), Some(Mismatch::HomeButAway));
        assert_eq!(w.observe(m), None);
        assert_eq!(w.observe(None), None);
        assert_eq!(w.observe(m), Some(Mismatch::HomeButAway));
        assert_eq!(Mismatch::of(None, Some(true)), None);
        assert_eq!(
            Mismatch::of(Some(false), Some(false)),
            Some(Mismatch::AwayButHome)
        );
    }
}
