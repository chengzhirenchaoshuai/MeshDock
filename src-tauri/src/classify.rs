//! 连接方式判断（实施计划第 3 节）与 ping 结果解析，全部是纯函数
use crate::model::{DerpMap, PeerStatus, Status, non_empty};
use serde::Serialize;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

/// 自建中继的区域代码，界面上显示为“广州”
const SELF_HOSTED_REGION: &str = "headscale";
const SELF_HOSTED_NAME: &str = "广州";

/// 变为活跃后多久内仍视为“正在打洞”
pub const PUNCH_WINDOW: Duration = Duration::from_secs(15);

/// 连接方式，前端据此选择颜色
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ConnKind {
    /// 本机
    SelfNode,
    Offline,
    Lan,
    Ipv6,
    Public,
    PeerRelay,
    Punching,
    Relay,
    /// 在线但当前没有通信，路径未知
    Idle,
    /// ping 返回了无法识别的路径
    Unknown,
}

/// 连接方式及其中文说明
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Conn {
    pub kind: ConnKind,
    pub label: String,
    /// 补充说明，例如中继的速度上限
    pub hint: Option<String>,
}

impl Conn {
    fn new(kind: ConnKind, label: impl Into<String>, hint: Option<String>) -> Self {
        Self {
            kind,
            label: label.into(),
            hint,
        }
    }
}

/// 中继区域代码 → 显示名称
#[derive(Default, Debug)]
pub struct Regions {
    by_code: HashMap<String, String>,
    by_id: HashMap<String, String>,
}

impl Regions {
    pub fn from_derp_map(map: &DerpMap) -> Self {
        let mut regions = Self::default();
        for (key, r) in map.regions.iter().flatten() {
            let name = if r.region_code.as_deref() == Some(SELF_HOSTED_REGION) {
                SELF_HOSTED_NAME.into()
            } else {
                r.region_name.clone().unwrap_or_else(|| key.clone())
            };
            if let Some(code) = &r.region_code {
                regions.by_code.insert(code.clone(), name.clone());
            }
            regions.by_id.insert(key.clone(), name);
        }
        regions
    }

    /// 按区域代码取名称：自建中继显示“广州”，其余用 derp-map 中的名称，查不到时原样显示
    pub fn name(&self, code: &str) -> String {
        match self.by_code.get(code) {
            Some(name) => name.clone(),
            // derp-map 还没读到时，自建中继也要显示“广州”
            None if code == SELF_HOSTED_REGION => SELF_HOSTED_NAME.into(),
            None => code.into(),
        }
    }

    /// 按区域编号取名称（netcheck 的延迟表用编号）
    pub fn name_by_id(&self, id: &str) -> String {
        self.by_id
            .get(id)
            .cloned()
            .unwrap_or_else(|| format!("区域 {id}"))
    }
}

/// 中继的提示：自建中继受服务器带宽限制
fn relay_hint(code: &str) -> Option<String> {
    if code == SELF_HOSTED_REGION {
        Some("速度受广州服务器 4 Mbps 带宽限制，约 500 KB/s".into())
    } else {
        Some("经官方中继，速度和延迟可能较差".into())
    }
}

fn relay_conn(code: &str, regions: &Regions) -> Conn {
    Conn::new(
        ConnKind::Relay,
        format!("中继（经 {}）", regions.name(code)),
        relay_hint(code),
    )
}

/// 按直连地址判断：局域网 / IPv6 / 公网
fn direct_conn(addr: &str) -> Conn {
    let ip = addr
        .parse::<SocketAddr>()
        .map(|a| a.ip())
        .or_else(|_| addr.parse::<IpAddr>());
    let kind = match ip {
        Ok(IpAddr::V6(_)) => ConnKind::Ipv6,
        Ok(IpAddr::V4(v4)) if v4.is_private() => ConnKind::Lan,
        Ok(IpAddr::V4(_)) => ConnKind::Public,
        Err(_) if addr.starts_with('[') => ConnKind::Ipv6,
        Err(_) => ConnKind::Public,
    };
    let label = match kind {
        ConnKind::Lan => "局域网直连",
        ConnKind::Ipv6 => "IPv6 直连",
        _ => "公网直连",
    };
    Conn::new(kind, label, None)
}

/// 判断对端设备的连接方式。`active_for` 是设备持续活跃的时长（由调用方记录），用于识别“正在打洞”
pub fn classify(p: &PeerStatus, active_for: Option<Duration>, regions: &Regions) -> Conn {
    // 1. 离线
    if p.online == Some(false) {
        return Conn::new(ConnKind::Offline, "离线", None);
    }
    // 2~4. 有直连地址
    if let Some(addr) = non_empty(&p.cur_addr) {
        return direct_conn(addr);
    }
    // 5. 经另一台节点中转
    if let Some(via) = non_empty(&p.peer_relay) {
        return Conn::new(ConnKind::PeerRelay, format!("中继（经 {via}）"), None);
    }
    if p.active == Some(true) {
        // 6. 刚开始通信，可能还在打洞
        if active_for.is_some_and(|d| d < PUNCH_WINDOW) {
            return Conn::new(
                ConnKind::Punching,
                "正在打洞",
                Some("先走中继、几秒内变成直连属于正常现象".into()),
            );
        }
        // 7. 在走中继
        let code = non_empty(&p.relay).unwrap_or(SELF_HOSTED_REGION);
        return relay_conn(code, regions);
    }
    // 8. 在线但没有通信
    Conn::new(
        ConnKind::Idle,
        "在线·空闲",
        Some("当前没有通信，路径未知，点“测试”查看实际路径".into()),
    )
}

/// 从健康告警判断是否连不上控制端（文案取自 tailscale.exe 1.102.4）：
/// “Unable to connect to the Tailscale coordination server …”
/// “Tailscale hasn't received a network map from the coordination server in …”
pub fn control_unreachable(status: &Status) -> bool {
    status.health.iter().flatten().any(|h| {
        let h = h.to_lowercase();
        h.contains("coordination server")
            && (h.contains("unable to connect") || h.contains("hasn't received a network map"))
    })
}

/// 本机卡片的连接方式
pub fn self_conn() -> Conn {
    Conn::new(ConnKind::SelfNode, "本机", None)
}

/// 一次 `tailscale ping` 的结果
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PingResult {
    /// 收到回包时的实测路径；没收到为 None
    pub conn: Option<Conn>,
    pub latency_ms: Option<f64>,
    /// 没收到回包时的中文原因
    pub error: Option<String>,
    /// 命令原始输出，前端可折叠查看
    pub raw: String,
}

/// 解析 ping 输出。走中继时命令退出码是 1 但仍有回包，所以只看文本，不看退出码
pub fn parse_ping(output: &str, regions: &Regions) -> PingResult {
    let raw = output.trim().to_string();
    let pong = raw.lines().find_map(|l| {
        let rest = l.trim().strip_prefix("pong from ")?;
        let (_, after_via) = rest.split_once(" via ")?;
        let (via, latency) = after_via.rsplit_once(" in ")?;
        Some((via.trim().to_string(), parse_duration_ms(latency.trim())))
    });
    if let Some((via, latency_ms)) = pong {
        let conn = if let Some(code) = via.strip_prefix("DERP(").and_then(|s| s.strip_suffix(')')) {
            relay_conn(code, regions)
        } else if via.to_lowercase().contains("relay") {
            Conn::new(ConnKind::PeerRelay, format!("中继（{via}）"), None)
        } else if via.parse::<SocketAddr>().is_ok() {
            direct_conn(&via)
        } else {
            Conn::new(ConnKind::Unknown, format!("路径：{via}"), None)
        };
        return PingResult {
            conn: Some(conn),
            latency_ms,
            error: None,
            raw,
        };
    }
    let error = if raw.contains("timed out") || raw.contains("no reply") {
        "没有回应，设备可能离线或网络不通"
    } else if raw.contains("no matching peer") {
        "组网中找不到这台设备"
    } else {
        "测试失败"
    };
    PingResult {
        conn: None,
        latency_ms: None,
        error: Some(error.into()),
        raw,
    }
}

/// 解析 Go 的时长文本（如 82ms、1.5s、850µs），统一换算成毫秒
fn parse_duration_ms(text: &str) -> Option<f64> {
    let units = [
        ("ms", 1.0),
        ("µs", 0.001),
        ("us", 0.001),
        ("ns", 0.000_001),
        ("s", 1000.0),
    ];
    units.iter().find_map(|(unit, factor)| {
        let num = text.strip_suffix(unit)?;
        num.parse::<f64>().ok().map(|v| v * factor)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Status;

    fn regions() -> Regions {
        let map: DerpMap = serde_json::from_str(include_str!("../fixtures/derpmap.json")).unwrap();
        Regions::from_derp_map(&map)
    }

    fn peer(online: bool, active: bool, cur_addr: &str) -> PeerStatus {
        PeerStatus {
            online: Some(online),
            active: Some(active),
            cur_addr: Some(cur_addr.into()),
            relay: Some("headscale".into()),
            peer_relay: Some(String::new()),
            ..Default::default()
        }
    }

    fn kind(p: &PeerStatus, active_for: Option<Duration>) -> ConnKind {
        classify(p, active_for, &regions()).kind
    }

    #[test]
    fn 真实样本中的三台设备() {
        let s: Status = serde_json::from_str(include_str!("../fixtures/status.json")).unwrap();
        let r = regions();
        let got: Vec<_> = s
            .peers()
            .iter()
            .map(|p| (p.host_name.clone().unwrap(), classify(p, None, &r).kind))
            .collect();
        assert_eq!(
            got,
            [
                ("ming-pc".into(), ConnKind::Ipv6),
                ("phone-router".into(), ConnKind::Idle),
                ("work-laptop".into(), ConnKind::Offline),
            ]
        );
    }

    #[test]
    fn 验收场景_局域网直连() {
        assert_eq!(
            kind(&peer(true, true, "192.168.66.233:41641"), None),
            ConnKind::Lan
        );
        assert_eq!(
            kind(&peer(true, true, "10.0.0.5:41641"), None),
            ConnKind::Lan
        );
        assert_eq!(
            kind(&peer(true, true, "172.20.1.1:41641"), None),
            ConnKind::Lan
        );
    }

    #[test]
    fn 公网直连与非私有的_172_段() {
        assert_eq!(
            kind(&peer(true, true, "203.0.113.10:41641"), None),
            ConnKind::Public
        );
        assert_eq!(
            kind(&peer(true, true, "172.32.0.1:41641"), None),
            ConnKind::Public
        );
    }

    #[test]
    fn 验收场景_先打洞后中继() {
        let p = peer(true, true, "");
        assert_eq!(kind(&p, Some(Duration::from_secs(3))), ConnKind::Punching);
        let c = classify(&p, Some(Duration::from_secs(60)), &regions());
        assert_eq!(c.kind, ConnKind::Relay);
        assert_eq!(c.label, "中继（经 广州）");
        assert!(c.hint.unwrap().contains("500 KB/s"));
    }

    #[test]
    fn 离线优先于其他字段() {
        assert_eq!(
            kind(&peer(false, true, "192.168.1.2:41641"), None),
            ConnKind::Offline
        );
    }

    #[test]
    fn 经节点中转() {
        let mut p = peer(true, true, "");
        p.peer_relay = Some("phone-router".into());
        assert_eq!(kind(&p, None), ConnKind::PeerRelay);
    }

    #[test]
    fn 官方中继显示区域名() {
        let mut p = peer(true, true, "");
        p.relay = Some("sin".into());
        let c = classify(&p, None, &regions());
        assert_eq!(c.label, "中继（经 Singapore）");
    }

    #[test]
    fn 没有_derp_map_时自建中继仍显示广州() {
        assert_eq!(Regions::default().name("headscale"), "广州");
        assert_eq!(Regions::default().name("sin"), "sin");
    }

    #[test]
    fn 区域编号取名() {
        let r = regions();
        assert_eq!(r.name_by_id("999"), "广州");
        assert_eq!(r.name_by_id("1"), "New York City");
        assert_eq!(r.name_by_id("12345"), "区域 12345");
    }

    #[test]
    fn 解析真实_ping_输出() {
        let r = regions();
        let direct = parse_ping(include_str!("../fixtures/ping-direct-ipv6.txt"), &r);
        assert_eq!(direct.conn.unwrap().kind, ConnKind::Ipv6);
        assert_eq!(direct.latency_ms, Some(1.0));

        let derp = parse_ping(include_str!("../fixtures/ping-derp.txt"), &r);
        assert_eq!(derp.conn.unwrap().label, "中继（经 广州）");
        assert_eq!(derp.latency_ms, Some(82.0));

        let timeout = parse_ping(include_str!("../fixtures/ping-timeout.txt"), &r);
        assert!(timeout.conn.is_none());
        assert!(timeout.error.unwrap().contains("没有回应"));

        let none = parse_ping("no matching peer", &r);
        assert_eq!(none.error.as_deref(), Some("组网中找不到这台设备"));
    }

    #[test]
    fn 识别控制端连不上() {
        let mut s: Status = serde_json::from_str(include_str!("../fixtures/status.json")).unwrap();
        // 真实样本里只有“子网路由未开启”的告警，不算连不上
        assert!(!control_unreachable(&s));
        s.health = Some(vec![
            "Unable to connect to the Tailscale coordination server to synchronize the state of your tailnet. Peer reachability might degrade over time.".into(),
        ]);
        assert!(control_unreachable(&s));
    }

    #[test]
    fn 时长单位换算() {
        assert_eq!(parse_duration_ms("82ms"), Some(82.0));
        assert_eq!(parse_duration_ms("1.5s"), Some(1500.0));
        assert_eq!(parse_duration_ms("850µs"), Some(0.85));
        assert_eq!(parse_duration_ms("abc"), None);
    }
}
