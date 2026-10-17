//! 路径变化与模式开错的通知：判断逻辑是纯函数，发送通知由调用方完成
use crate::classify::ConnKind;
use std::collections::HashMap;
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

#[cfg(test)]
mod tests {
    use super::*;

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
