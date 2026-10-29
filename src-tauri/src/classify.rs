//! 连接方式判断（实施计划第 3 节）与 ping 结果解析，全部是纯函数
use crate::model::{DerpMap, NetInfo, PeerStatus, Status, non_empty};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv6Addr, SocketAddr};
use std::time::Duration;

/// 自建中继的区域编号下限：Tailscale 把 900～999 留给自建 DERP，Headscale 内置中继默认用 999
const SELF_HOSTED_MIN_ID: i64 = 900;
/// Headscale 内置中继的区域代码和编号；derp-map 还没读到时据此认出自建中继
const HEADSCALE_REGION_CODE: &str = "headscale";
const HEADSCALE_REGION_ID: &str = "999";
/// Headscale 内置中继在 derp-map 里的默认名称，不适合直接显示
const HEADSCALE_REGION_NAME: &str = "Headscale Embedded DERP";
/// 设置里没起名、derp-map 里也没有合适名称时，自建中继的显示名
pub const DEFAULT_SELF_HOSTED_NAME: &str = "自建";

/// 变为活跃后多久内仍视为“正在打洞”
pub const PUNCH_WINDOW: Duration = Duration::from_secs(15);

/// 连接方式，前端据此选择颜色
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
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

/// 中继区域代码 → 显示名称，并记下哪些区域是自建中继
#[derive(Debug)]
pub struct Regions {
    by_code: HashMap<String, String>,
    by_id: HashMap<String, String>,
    /// 自建中继的区域代码和编号（编号按数字升序）
    self_codes: HashSet<String>,
    self_ids: Vec<String>,
    /// 设置里的自建中继显示名，空时为“自建”
    self_name: String,
    /// 是否读到了 derp-map；没读到时按 Headscale 的默认代码和编号认自建中继
    loaded: bool,
}

impl Default for Regions {
    fn default() -> Self {
        Self::named("")
    }
}

/// 设置里的显示名，空时用默认名
fn self_display_name(name: &str) -> String {
    match name.trim() {
        "" => DEFAULT_SELF_HOSTED_NAME.into(),
        n => n.into(),
    }
}

impl Regions {
    /// 还没读到 derp-map 时用：只知道自建中继的显示名
    pub fn named(self_name: &str) -> Self {
        Self {
            by_code: HashMap::new(),
            by_id: HashMap::new(),
            self_codes: HashSet::new(),
            self_ids: Vec::new(),
            self_name: self_display_name(self_name),
            loaded: false,
        }
    }

    /// self_name：设置里的自建中继显示名，空时用 derp-map 里的名称（Headscale 内置中继的默认名称除外），再没有就叫“自建”
    pub fn from_derp_map(map: &DerpMap, self_name: &str) -> Self {
        let mut regions = Self::named(self_name);
        regions.loaded = true;
        let mut self_ids: Vec<(i64, String)> = Vec::new();
        for (key, r) in map.regions.iter().flatten() {
            let code = r.region_code.as_deref();
            let id = r.region_id.or_else(|| key.parse().ok());
            let self_hosted = id.is_some_and(|id| id >= SELF_HOSTED_MIN_ID);
            let name = if self_hosted {
                let own = r
                    .region_name
                    .as_deref()
                    .map(str::trim)
                    .filter(|n| !n.is_empty() && *n != HEADSCALE_REGION_NAME);
                match (self_name.trim(), own) {
                    ("", Some(own)) => own.to_string(),
                    _ => regions.self_name.clone(),
                }
            } else if let Some(zh) = code.and_then(zh_region_name) {
                zh.into()
            } else {
                // 表里没有的新区域保留官方英文名
                r.region_name.clone().unwrap_or_else(|| key.clone())
            };
            if let Some(code) = &r.region_code {
                regions.by_code.insert(code.clone(), name.clone());
                if self_hosted {
                    regions.self_codes.insert(code.clone());
                }
            }
            if self_hosted {
                self_ids.push((id.unwrap_or_default(), key.clone()));
            }
            regions.by_id.insert(key.clone(), name);
        }
        self_ids.sort();
        regions.self_ids = self_ids.into_iter().map(|(_, key)| key).collect();
        regions
    }

    /// 按区域代码取名称：查不到时原样显示（derp-map 还没读到时，Headscale 内置中继显示自建中继的名称）
    pub fn name(&self, code: &str) -> String {
        match self.by_code.get(code) {
            Some(name) => name.clone(),
            None if self.is_self_hosted_code(code) => self.self_name.clone(),
            None => code.into(),
        }
    }

    /// 按区域编号取名称（netcheck 的延迟表用编号）
    pub fn name_by_id(&self, id: &str) -> String {
        match self.by_id.get(id) {
            Some(name) => name.clone(),
            None if self.is_self_hosted_id(id) => self.self_name.clone(),
            None => format!("区域 {id}"),
        }
    }

    /// 自建中继的显示名（多个自建中继时取编号最小的那个）
    pub fn self_hosted_name(&self) -> String {
        self.self_ids
            .first()
            .map(|id| self.name_by_id(id))
            .unwrap_or_else(|| self.self_name.clone())
    }

    /// 区域代码是不是自建中继
    pub fn is_self_hosted_code(&self, code: &str) -> bool {
        if self.loaded {
            self.self_codes.contains(code)
        } else {
            code == HEADSCALE_REGION_CODE
        }
    }

    /// 区域编号是不是自建中继
    pub fn is_self_hosted_id(&self, id: &str) -> bool {
        if self.loaded {
            self.self_ids.iter().any(|s| s == id)
        } else {
            id.parse::<i64>().is_ok_and(|n| n >= SELF_HOSTED_MIN_ID)
        }
    }

    /// 自建中继的区域编号；derp-map 还没读到时按 Headscale 的默认编号猜
    pub fn self_hosted_ids(&self) -> Vec<String> {
        if self.loaded {
            self.self_ids.clone()
        } else {
            vec![HEADSCALE_REGION_ID.into()]
        }
    }

    /// 组网里有没有自建中继；derp-map 还没读到时无法判断，视为有
    pub fn has_self_hosted(&self) -> bool {
        !self.loaded || !self.self_ids.is_empty()
    }
}

/// 官方中继区域代码 → 中文城市名（取自 tailscale 1.102.4 的 derp-map，共 28 个）
fn zh_region_name(code: &str) -> Option<&'static str> {
    Some(match code {
        "nyc" => "纽约",
        "sfo" => "旧金山",
        "sin" => "新加坡",
        "fra" => "法兰克福",
        "syd" => "悉尼",
        "blr" => "班加罗尔",
        "tok" => "东京",
        "lhr" => "伦敦",
        "dfw" => "达拉斯",
        "sea" => "西雅图",
        "sao" => "圣保罗",
        "ord" => "芝加哥",
        "den" => "丹佛",
        "ams" => "阿姆斯特丹",
        "jnb" => "约翰内斯堡",
        "mia" => "迈阿密",
        "lax" => "洛杉矶",
        "par" => "巴黎",
        "mad" => "马德里",
        "hkg" => "香港",
        "tor" => "多伦多",
        "waw" => "华沙",
        "dbi" => "迪拜",
        "hnl" => "檀香山",
        "nai" => "内罗毕",
        "nue" => "纽伦堡",
        "iad" => "阿什本",
        "hel" => "赫尔辛基",
        _ => return None,
    })
}

/// 中继的提示：自建中继受服务器带宽限制
fn relay_hint(code: &str, regions: &Regions) -> Option<String> {
    if regions.is_self_hosted_code(code) {
        Some("经自建中继，速度受中继服务器的带宽限制".into())
    } else {
        Some("经官方中继，速度和延迟可能较差".into())
    }
}

fn relay_conn(code: &str, regions: &Regions) -> Conn {
    Conn::new(
        ConnKind::Relay,
        format!("中继（经 {}）", regions.name(code)),
        relay_hint(code, regions),
    )
}

/// 本机所在网络：本机各公网 IPv6 地址的 /64 前缀。
/// 家里的设备往往都有同一前缀的公网 IPv6，Tailscale 会优先用它直连，这其实是局域网内直连
#[derive(Default, Debug)]
pub struct LocalNet {
    v6_prefixes: Vec<[u16; 4]>,
    /// 本机网卡上有公网 IPv6 地址（2000::/3）
    pub has_global_v6: bool,
    /// 本机有代理软件的 TUN 虚拟网卡：Clash / Mihomo / sing-box 的 TUN 默认用 198.18.0.0/15 的地址
    pub proxy_tun: bool,
}

impl LocalNet {
    /// 从 status 中本机的 Addrs（本机对外的各个地址）提取前缀，并识别公网 IPv6 和代理 TUN 网卡
    pub fn from_status(status: &Status) -> Self {
        let ips: Vec<IpAddr> = status
            .self_node
            .iter()
            .flat_map(|s| s.addrs.iter().flatten())
            .filter_map(|a| Some(a.parse::<SocketAddr>().ok()?.ip()))
            .collect();
        let mut v6_prefixes: Vec<[u16; 4]> = ips
            .iter()
            .filter_map(|ip| match ip {
                IpAddr::V6(v6) => Some(prefix64(v6)),
                IpAddr::V4(_) => None,
            })
            .collect();
        v6_prefixes.sort_unstable();
        v6_prefixes.dedup();
        let has_global_v6 = ips
            .iter()
            .any(|ip| matches!(ip, IpAddr::V6(v6) if (v6.segments()[0] & 0xe000) == 0x2000));
        let proxy_tun = ips.iter().any(|ip| {
            matches!(ip, IpAddr::V4(v4) if v4.octets()[0] == 198 && (v4.octets()[1] & 0xfe) == 18)
        });
        Self {
            v6_prefixes,
            has_global_v6,
            proxy_tun,
        }
    }

    /// 本机网卡有公网 IPv6 地址、实测却用不了时的说明：有代理 TUN 网卡时点明是它
    pub fn broken_v6_text(&self) -> &'static str {
        if self.proxy_tun {
            "本机有 IPv6 公网地址，但 IPv6 实测不通：检测到代理软件的 TUN 虚拟网卡（198.18.x.x），IPv6 流量多半被它接管了。请在代理软件里让 IPv6 绕过 TUN（如关闭 TUN 的 IPv6），否则只能靠 IPv4 打洞或中继"
        } else {
            "本机有 IPv6 公网地址，但 IPv6 实测不通，可能被防火墙或代理软件拦截了，只能靠 IPv4 打洞或中继"
        }
    }

    /// 链路本地（fe80::/10）、唯一本地（fc00::/7）或与本机同一 /64 前缀，都算局域网
    fn is_lan_v6(&self, ip: &Ipv6Addr) -> bool {
        let first = ip.segments()[0];
        (first & 0xffc0) == 0xfe80
            || (first & 0xfe00) == 0xfc00
            || self.v6_prefixes.contains(&prefix64(ip))
    }
}

fn prefix64(ip: &Ipv6Addr) -> [u16; 4] {
    let s = ip.segments();
    [s[0], s[1], s[2], s[3]]
}

/// 按直连地址判断：局域网 / IPv6 / 公网
fn direct_conn(addr: &str, local: &LocalNet) -> Conn {
    let ip = addr
        .parse::<SocketAddr>()
        .map(|a| a.ip())
        .or_else(|_| addr.parse::<IpAddr>());
    let kind = match ip {
        Ok(IpAddr::V6(v6)) if local.is_lan_v6(&v6) => ConnKind::Lan,
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
pub fn classify(
    p: &PeerStatus,
    active_for: Option<Duration>,
    regions: &Regions,
    local: &LocalNet,
) -> Conn {
    // 1. 离线
    if p.online == Some(false) {
        return Conn::new(ConnKind::Offline, "离线", None);
    }
    // 2~4. 有直连地址
    if let Some(addr) = non_empty(&p.cur_addr) {
        return direct_conn(addr, local);
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
        return match non_empty(&p.relay) {
            Some(code) => relay_conn(code, regions),
            // 没写明经哪个中继：组网有自建中继时多半是它
            None if regions.has_self_hosted() => Conn::new(
                ConnKind::Relay,
                format!("中继（经 {}）", regions.self_hosted_name()),
                Some("经自建中继，速度受中继服务器的带宽限制".into()),
            ),
            None => Conn::new(ConnKind::Relay, "中继", Some("经中继转发，速度和延迟可能较差".into())),
        };
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

/// 解释为什么走中继：对比本机和对方各自上报的网络检查结论。
/// 直连需要双方都有 IPv6，或 IPv4 能打洞（对称型 NAT 基本打不通），UDP 被阻断则都不行
pub fn relay_reason(me: Option<&NetInfo>, peer: Option<&NetInfo>) -> Option<String> {
    let mut reasons = Vec::new();
    for (who, info) in [("对方", peer), ("本机", me)] {
        let Some(info) = info else { continue };
        if info.working_udp == Some(false) {
            reasons.push(format!("{who}网络的 UDP 被阻断"));
        }
    }
    for (who, info) in [("对方", peer), ("本机", me)] {
        let Some(info) = info else { continue };
        if info.working_ipv6 == Some(false) {
            reasons.push(format!("{who}当前网络没有 IPv6"));
        }
    }
    for (who, info) in [("对方", peer), ("本机", me)] {
        let Some(info) = info else { continue };
        if info.mapping_varies_by_dest_ip == Some(true) {
            reasons.push(format!("{who} IPv4 是对称型 NAT"));
        }
    }
    (!reasons.is_empty()).then(|| format!("可能的原因：{}，打不通直连", reasons.join("、")))
}

/// 中继连接补上原因（接在速度提示后面）；不是中继或找不到原因时原样返回
pub fn with_relay_reason(mut conn: Conn, reason: Option<String>) -> Conn {
    if conn.kind == ConnKind::Relay
        && let Some(reason) = reason
    {
        conn.hint = Some(match conn.hint {
            Some(h) => format!("{h}。{reason}"),
            None => reason,
        });
    }
    conn
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
pub fn parse_ping(output: &str, regions: &Regions, local: &LocalNet) -> PingResult {
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
            direct_conn(&via, local)
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
        // 测试数据是 Headscale 内置中继，设置里起名“广州”
        Regions::from_derp_map(&map, "广州")
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

    fn status() -> Status {
        serde_json::from_str(include_str!("../fixtures/status.json")).unwrap()
    }

    /// 本机网络取自真实样本：家里的 IPv6 前缀为 2001:db8:1::/64（脱敏后）
    fn home() -> LocalNet {
        LocalNet::from_status(&status())
    }

    fn kind(p: &PeerStatus, active_for: Option<Duration>) -> ConnKind {
        classify(p, active_for, &regions(), &home()).kind
    }

    #[test]
    fn 验收场景_同在家里时显示局域网直连() {
        // 真实样本：ming-pc 与本机同在家里，Tailscale 用同前缀的公网 IPv6 直连
        let s = status();
        let (r, local) = (regions(), home());
        let got: Vec<_> = s
            .peers()
            .iter()
            .map(|p| {
                (
                    p.host_name.clone().unwrap(),
                    classify(p, None, &r, &local).kind,
                )
            })
            .collect();
        assert_eq!(
            got,
            [
                ("ming-pc".into(), ConnKind::Lan),
                ("work-laptop".into(), ConnKind::Offline),
                ("phone-router".into(), ConnKind::Idle),
            ]
        );
    }

    #[test]
    fn 不同前缀的_ipv6_是_ipv6_直连() {
        // 例如 ming-pc 连着手机热点
        assert_eq!(
            kind(&peer(true, true, "[2001:db8:2::5]:41641"), None),
            ConnKind::Ipv6
        );
        // 没有本机信息时也按 IPv6 直连处理
        let p = peer(true, true, "[2001:db8:1::13]:41641");
        assert_eq!(
            classify(&p, None, &regions(), &LocalNet::default()).kind,
            ConnKind::Ipv6
        );
    }

    #[test]
    fn 链路本地和唯一本地_ipv6_算局域网() {
        assert_eq!(
            kind(&peer(true, true, "[fe80::1]:41641"), None),
            ConnKind::Lan
        );
        assert_eq!(
            kind(&peer(true, true, "[fd00::1]:41641"), None),
            ConnKind::Lan
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
        let c = classify(&p, Some(Duration::from_secs(60)), &regions(), &home());
        assert_eq!(c.kind, ConnKind::Relay);
        assert_eq!(c.label, "中继（经 广州）");
        assert!(c.hint.unwrap().contains("自建中继"));
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
        let c = classify(&p, None, &regions(), &home());
        assert_eq!(c.label, "中继（经 新加坡）");
    }

    #[test]
    fn 没有_derp_map_时按_headscale_默认值认自建中继() {
        assert_eq!(Regions::default().name("headscale"), "自建");
        assert_eq!(Regions::named("广州").name("headscale"), "广州");
        assert_eq!(Regions::default().name("sin"), "sin");
        assert_eq!(Regions::default().name_by_id("999"), "自建");
        assert_eq!(Regions::default().self_hosted_ids(), ["999"]);
        assert!(Regions::default().has_self_hosted());
    }

    #[test]
    fn 自建中继的显示名() {
        let map: DerpMap = serde_json::from_str(include_str!("../fixtures/derpmap.json")).unwrap();
        // 没起名时，Headscale 内置中继的默认名称不直接显示
        let r = Regions::from_derp_map(&map, "");
        assert_eq!(r.name("headscale"), "自建");
        assert_eq!(r.self_hosted_ids(), ["999"]);
        assert!(r.is_self_hosted_id("999") && !r.is_self_hosted_id("1"));
        // 自建 DERP 在 derp-map 里写了名称时用它，设置里起了名时以设置为准
        let custom = DerpMap {
            regions: Some(HashMap::from([(
                "901".to_string(),
                crate::model::DerpRegion {
                    region_id: Some(901),
                    region_code: Some("sh".into()),
                    region_name: Some("上海".into()),
                },
            )])),
        };
        assert_eq!(Regions::from_derp_map(&custom, "").name("sh"), "上海");
        assert_eq!(Regions::from_derp_map(&custom, " 公司 ").name("sh"), "公司");
        assert!(Regions::from_derp_map(&custom, "").is_self_hosted_code("sh"));
    }

    #[test]
    fn 只用官方中继时没有自建中继() {
        let official = DerpMap {
            regions: Some(HashMap::from([(
                "1".to_string(),
                crate::model::DerpRegion {
                    region_id: Some(1),
                    region_code: Some("nyc".into()),
                    region_name: Some("New York City".into()),
                },
            )])),
        };
        let r = Regions::from_derp_map(&official, "");
        assert!(!r.has_self_hosted());
        assert!(!r.is_self_hosted_code("headscale"));
        let mut p = peer(true, true, "");
        p.relay = None;
        assert_eq!(classify(&p, None, &r, &home()).label, "中继");
    }

    #[test]
    fn 全部区域都有中文名() {
        let map: DerpMap = serde_json::from_str(include_str!("../fixtures/derpmap.json")).unwrap();
        let r = Regions::from_derp_map(&map, "广州");
        for id in map.regions.as_ref().unwrap().keys() {
            let name = r.name_by_id(id);
            assert!(!name.is_ascii(), "区域 {id} 没有中文名：{name}");
        }
        // 表里没有的新区域保留英文名
        let extra = DerpMap {
            regions: Some(HashMap::from([(
                "30".to_string(),
                crate::model::DerpRegion {
                    region_id: Some(30),
                    region_code: Some("xyz".into()),
                    region_name: Some("Newtown".into()),
                },
            )])),
        };
        assert_eq!(Regions::from_derp_map(&extra, "").name("xyz"), "Newtown");
    }

    #[test]
    fn 区域编号取名() {
        let r = regions();
        assert_eq!(r.name_by_id("999"), "广州");
        assert_eq!(r.name_by_id("1"), "纽约");
        assert_eq!(r.name_by_id("12345"), "区域 12345");
    }

    #[test]
    fn 解析真实_ping_输出() {
        let (r, local) = (regions(), home());
        let direct = parse_ping(include_str!("../fixtures/ping-direct-ipv6.txt"), &r, &local);
        assert_eq!(direct.conn.unwrap().kind, ConnKind::Lan);
        assert_eq!(direct.latency_ms, Some(1.0));

        let derp = parse_ping(include_str!("../fixtures/ping-derp.txt"), &r, &local);
        assert_eq!(derp.conn.unwrap().label, "中继（经 广州）");
        assert_eq!(derp.latency_ms, Some(82.0));

        let timeout = parse_ping(include_str!("../fixtures/ping-timeout.txt"), &r, &local);
        assert!(timeout.conn.is_none());
        assert!(timeout.error.unwrap().contains("没有回应"));

        let none = parse_ping("no matching peer", &r, &local);
        assert_eq!(none.error.as_deref(), Some("组网中找不到这台设备"));
    }

    #[test]
    fn 识别控制端连不上() {
        let mut s = status();
        // 真实样本里只有“子网路由未开启”的告警，不算连不上
        assert!(!control_unreachable(&s));
        s.health = Some(vec![
            "Unable to connect to the Tailscale coordination server to synchronize the state of your tailnet. Peer reachability might degrade over time.".into(),
        ]);
        assert!(control_unreachable(&s));
    }

    #[test]
    fn 验收场景_随身_wifi_走中继并说明原因() {
        // 真实样本：工作笔记本最近一次上报时没有 IPv6、IPv4 是对称型 NAT
        let n = crate::model::parse_netmap(include_str!("../fixtures/netmap.json")).unwrap();
        let s = status();
        let laptop = s
            .peers()
            .into_iter()
            .find(|p| p.host_name.as_deref() == Some("work-laptop"))
            .unwrap();
        let peers = n.peer_hostinfo();
        let reason = relay_reason(
            n.self_net_info().as_ref(),
            peers[laptop.public_key.as_ref().unwrap()].net_info.as_ref(),
        );
        assert_eq!(
            reason.as_deref(),
            Some("可能的原因：对方当前网络没有 IPv6、对方 IPv4 是对称型 NAT，打不通直连")
        );
        // 接在速度提示后面
        let c = with_relay_reason(
            classify(&peer(true, true, ""), None, &regions(), &home()),
            reason,
        );
        let hint = c.hint.unwrap();
        assert!(
            hint.contains("自建中继") && hint.contains("没有 IPv6"),
            "{hint}"
        );
    }

    #[test]
    fn 双方网络都正常时不编造原因() {
        let ok = NetInfo {
            working_ipv6: Some(true),
            working_udp: Some(true),
            mapping_varies_by_dest_ip: Some(false),
            ..Default::default()
        };
        assert_eq!(relay_reason(Some(&ok), Some(&ok)), None);
        assert_eq!(relay_reason(None, None), None);
        // 非中继连接不追加
        let lan = with_relay_reason(direct_conn("192.168.1.2:41641", &home()), Some("x".into()));
        assert_eq!(lan.hint, None);
    }

    #[test]
    fn 本机_udp_被阻断() {
        let me = NetInfo {
            working_udp: Some(false),
            ..Default::default()
        };
        assert_eq!(
            relay_reason(Some(&me), None).as_deref(),
            Some("可能的原因：本机网络的 UDP 被阻断，打不通直连")
        );
    }

    #[test]
    fn 时长单位换算() {
        assert_eq!(parse_duration_ms("82ms"), Some(82.0));
        assert_eq!(parse_duration_ms("1.5s"), Some(1500.0));
        assert_eq!(parse_duration_ms("850µs"), Some(0.85));
        assert_eq!(parse_duration_ms("abc"), None);
    }
}
