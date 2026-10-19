//! tailscale 命令输出的宽松解析：所有字段都可缺省，未知字段忽略，避免官方升级改字段后崩溃
use chrono::DateTime;
use serde::Deserialize;
use std::collections::HashMap;

/// `tailscale status --json`
#[derive(Deserialize, Default, Debug)]
#[serde(default, rename_all = "PascalCase")]
pub struct Status {
    pub version: Option<String>,
    /// Running / Stopped / NeedsLogin / Starting 等
    pub backend_state: Option<String>,
    /// 健康告警（英文原文）
    pub health: Option<Vec<String>>,
    #[serde(rename = "Self")]
    pub self_node: Option<PeerStatus>,
    pub peer: Option<HashMap<String, PeerStatus>>,
}

/// status 中的一台设备（本机或对端）
#[derive(Deserialize, Default, Debug, Clone)]
#[serde(default, rename_all = "PascalCase")]
pub struct PeerStatus {
    #[serde(rename = "ID")]
    pub id: Option<String>,
    /// 节点公钥（nodekey:…），用来和 netmap 中的设备对应
    pub public_key: Option<String>,
    pub host_name: Option<String>,
    #[serde(rename = "DNSName")]
    pub dns_name: Option<String>,
    #[serde(rename = "OS")]
    pub os: Option<String>,
    #[serde(rename = "TailscaleIPs")]
    pub tailscale_ips: Option<Vec<String>>,
    pub online: Option<bool>,
    pub active: Option<bool>,
    /// 当前直连地址，空字符串表示没有直连
    pub cur_addr: Option<String>,
    /// 归属的中继区域代码（总是有值，不代表正在走中继）
    pub relay: Option<String>,
    /// 经另一台节点中转时的信息，目前实测为空
    pub peer_relay: Option<String>,
    pub last_seen: Option<String>,
    pub last_handshake: Option<String>,
    pub rx_bytes: Option<u64>,
    pub tx_bytes: Option<u64>,
    /// 这台设备对外的各个地址（ip:端口），本机的用于识别局域网 IPv6 前缀
    pub addrs: Option<Vec<String>>,
    /// 这台设备发布并已生效的子网路由，例如 phone-router 的 192.168.66.0/24
    pub primary_routes: Option<Vec<String>>,
    /// 本机正在用这台设备当出口
    pub exit_node: Option<bool>,
    /// 这台设备可以当出口（已发布 0.0.0.0/0 且控制端已批准）
    pub exit_node_option: Option<bool>,
}

/// `tailscale debug prefs`，只取用得到的字段
#[derive(Deserialize, Default, Debug)]
#[serde(default, rename_all = "PascalCase")]
pub struct Prefs {
    #[serde(rename = "ControlURL")]
    pub control_url: Option<String>,
    /// 即 accept-routes
    pub route_all: Option<bool>,
    /// 选用的出口设备（status 里的设备 ID），空字符串表示不用出口
    #[serde(rename = "ExitNodeID")]
    pub exit_node_id: Option<String>,
    /// 走出口时仍可直接访问本机所在的局域网
    #[serde(rename = "ExitNodeAllowLANAccess")]
    pub exit_node_allow_lan_access: Option<bool>,
    /// 即 accept-dns：使用组网下发的 DNS
    #[serde(rename = "CorpDNS")]
    pub corp_dns: Option<bool>,
    /// 挡住所有传入连接（官方的“Allow incoming connections”取反）
    pub shields_up: Option<bool>,
    /// 即 unattended：没有用户登录时也保持连接
    pub force_daemon: Option<bool>,
    pub want_running: Option<bool>,
}

/// 本机的组网选项，给前端展示用（字段为 None 表示读取不到）
#[derive(serde::Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LocalPrefs {
    pub accept_routes: Option<bool>,
    /// 选用的出口设备 ID，None 表示不用出口
    pub exit_node_id: Option<String>,
    pub exit_node_allow_lan: Option<bool>,
    pub accept_dns: Option<bool>,
    pub shields_up: Option<bool>,
    pub unattended: Option<bool>,
}

impl From<Prefs> for LocalPrefs {
    fn from(p: Prefs) -> Self {
        Self {
            accept_routes: p.route_all,
            exit_node_id: non_empty(&p.exit_node_id).map(str::to_string),
            exit_node_allow_lan: p.exit_node_allow_lan_access,
            accept_dns: p.corp_dns,
            shields_up: p.shields_up,
            unattended: p.force_daemon,
        }
    }
}

/// `tailscale version --upstream --json`
#[derive(Deserialize, Default, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct VersionInfo {
    /// 本机版本，如 1.102.4
    pub short: Option<String>,
    /// 官方最新稳定版，如 1.104.1
    pub upstream: Option<String>,
}

impl VersionInfo {
    /// 官方有更新的版本时返回它
    pub fn newer_upstream(&self) -> Option<String> {
        let up = self.upstream.as_deref()?;
        (version_key(up)? > version_key(self.short.as_deref()?)?).then(|| up.to_string())
    }
}

/// "1.102.4" → (1, 102, 4)；格式不对时返回 None
fn version_key(v: &str) -> Option<(u32, u32, u32)> {
    let mut it = v.trim().split('.').map(|p| p.parse::<u32>().ok());
    Some((it.next()??, it.next()??, it.next().flatten().unwrap_or(0)))
}

/// `tailscale serve status --json`：没有任何规则时是 `{}`
#[derive(Deserialize, Default, Debug)]
#[serde(default, rename_all = "PascalCase")]
pub struct ServeStatus {
    /// 对外端口 → 处理方式
    #[serde(rename = "TCP")]
    pub tcp: Option<HashMap<String, ServeTcp>>,
    /// "主机名:端口" → 各路径的处理方式（HTTP / HTTPS 规则）
    pub web: Option<HashMap<String, ServeWeb>>,
}

#[derive(Deserialize, Default, Debug)]
#[serde(default, rename_all = "PascalCase")]
pub struct ServeTcp {
    /// TCP 转发的目标，如 "127.0.0.1:45999"
    #[serde(rename = "TCPForward")]
    pub tcp_forward: Option<String>,
    #[serde(rename = "HTTP")]
    pub http: Option<bool>,
    #[serde(rename = "HTTPS")]
    pub https: Option<bool>,
}

#[derive(Deserialize, Default, Debug)]
#[serde(default, rename_all = "PascalCase")]
pub struct ServeWeb {
    pub handlers: Option<HashMap<String, ServeHandler>>,
}

#[derive(Deserialize, Default, Debug)]
#[serde(default, rename_all = "PascalCase")]
pub struct ServeHandler {
    pub proxy: Option<String>,
    pub path: Option<String>,
    pub text: Option<String>,
}

/// 一条共享规则，给前端展示用
#[derive(serde::Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ServeRule {
    /// "http"、"https" 或 "tcp"
    pub kind: String,
    /// 组网里对外的端口
    pub port: u16,
    /// 转发到哪里（本机地址、目录或文字）
    pub target: String,
}

impl ServeStatus {
    /// 整理成规则列表，按端口排序
    pub fn rules(&self) -> Vec<ServeRule> {
        let mut rules: Vec<ServeRule> = self
            .tcp
            .iter()
            .flatten()
            .filter_map(|(port, h)| {
                let port: u16 = port.parse().ok()?;
                let (kind, target) = if let Some(t) = non_empty(&h.tcp_forward) {
                    ("tcp", t.to_string())
                } else {
                    let kind = if h.https == Some(true) { "https" } else { "http" };
                    (kind, self.web_target(port).unwrap_or_default())
                };
                Some(ServeRule {
                    kind: kind.into(),
                    port,
                    target,
                })
            })
            .collect();
        rules.sort_by_key(|r| r.port);
        rules
    }

    /// 某个端口上 HTTP 规则的目标：各路径的目标，多个时用“；”连接
    fn web_target(&self, port: u16) -> Option<String> {
        let suffix = format!(":{port}");
        let (_, web) = self.web.iter().flatten().find(|(k, _)| k.ends_with(&suffix))?;
        let mut parts: Vec<String> = web
            .handlers
            .iter()
            .flatten()
            .map(|(path, h)| {
                let to = non_empty(&h.proxy)
                    .or(non_empty(&h.path))
                    .map(str::to_string)
                    .or_else(|| non_empty(&h.text).map(|_| "固定文字".into()))
                    .unwrap_or_default();
                if path == "/" { to } else { format!("{path} → {to}") }
            })
            .collect();
        parts.sort();
        Some(parts.join("；"))
    }
}

/// `tailscale netcheck --format=json`
#[derive(Deserialize, Default, Debug)]
#[serde(default, rename_all = "PascalCase")]
pub struct Netcheck {
    #[serde(rename = "UDP")]
    pub udp: Option<bool>,
    #[serde(rename = "IPv4")]
    pub ipv4: Option<bool>,
    #[serde(rename = "IPv6")]
    pub ipv6: Option<bool>,
    #[serde(rename = "OSHasIPv6")]
    pub os_has_ipv6: Option<bool>,
    /// true 表示对称型 NAT
    #[serde(rename = "MappingVariesByDestIP")]
    pub mapping_varies_by_dest_ip: Option<bool>,
    #[serde(rename = "UPnP")]
    pub upnp: Option<bool>,
    #[serde(rename = "PMP")]
    pub pmp: Option<bool>,
    #[serde(rename = "PCP")]
    pub pcp: Option<bool>,
    #[serde(rename = "PreferredDERP")]
    pub preferred_derp: Option<i64>,
    /// 区域编号 → 延迟（纳秒）
    pub region_latency: Option<HashMap<String, u64>>,
    #[serde(rename = "GlobalV4")]
    pub global_v4: Option<String>,
    #[serde(rename = "GlobalV6")]
    pub global_v6: Option<String>,
    pub captive_portal: Option<bool>,
}

/// `tailscale debug derp-map`
#[derive(Deserialize, Default, Debug)]
#[serde(default, rename_all = "PascalCase")]
pub struct DerpMap {
    pub regions: Option<HashMap<String, DerpRegion>>,
}

#[derive(Deserialize, Default, Debug, Clone)]
#[serde(default, rename_all = "PascalCase")]
pub struct DerpRegion {
    #[serde(rename = "RegionID")]
    pub region_id: Option<i64>,
    pub region_code: Option<String>,
    pub region_name: Option<String>,
}

/// `tailscale debug netmap`：只取每台设备自己上报的网络检查结论，用于解释为什么走中继
#[derive(Deserialize, Default, Debug)]
#[serde(default, rename_all = "PascalCase")]
pub struct NetMap {
    pub self_node: Option<NetMapNode>,
    pub peers: Option<Vec<NetMapNode>>,
}

#[derive(Deserialize, Default, Debug)]
#[serde(default, rename_all = "PascalCase")]
pub struct NetMapNode {
    /// 节点公钥，与 status 的 PublicKey 相同
    pub key: Option<String>,
    pub hostinfo: Option<Hostinfo>,
    /// 设备上报的可达地址，形如 "192.168.66.80:41641"、"[2001:db8::1]:41641"
    pub endpoints: Option<Vec<String>>,
}

#[derive(Deserialize, Default, Debug, Clone)]
#[serde(default, rename_all = "PascalCase")]
pub struct Hostinfo {
    pub net_info: Option<NetInfo>,
    /// Tailscale 版本，如 1.102.4-t3caf7d9e7-g084ee3b64
    #[serde(rename = "IPNVersion")]
    pub ipn_version: Option<String>,
    #[serde(rename = "OSVersion")]
    pub os_version: Option<String>,
    pub device_model: Option<String>,
}

/// 设备自己网络检查的结论（各设备定期上报给控制端）
#[derive(Deserialize, Default, Debug, Clone, PartialEq)]
#[serde(default, rename_all = "PascalCase")]
pub struct NetInfo {
    #[serde(rename = "WorkingIPv6")]
    pub working_ipv6: Option<bool>,
    #[serde(rename = "WorkingUDP")]
    pub working_udp: Option<bool>,
    /// true 表示 IPv4 是对称型 NAT
    #[serde(rename = "MappingVariesByDestIP")]
    pub mapping_varies_by_dest_ip: Option<bool>,
    /// 中继区域延迟（秒），键如 "999-v4"、"999-v6"
    #[serde(rename = "DERPLatency")]
    pub derp_latency: Option<HashMap<String, f64>>,
}

impl NetInfo {
    /// 到某个中继区域的延迟（毫秒），IPv4 / IPv6 取较小值
    pub fn region_latency_ms(&self, region_id: &str) -> Option<f64> {
        let map = self.derp_latency.as_ref()?;
        ["v4", "v6"]
            .iter()
            .filter_map(|f| map.get(&format!("{region_id}-{f}")))
            .map(|s| s * 1000.0)
            .reduce(f64::min)
    }
}

impl NetMapNode {
    fn net_info(&self) -> Option<NetInfo> {
        self.hostinfo.as_ref()?.net_info.clone()
    }
}

impl NetMap {
    pub fn self_net_info(&self) -> Option<NetInfo> {
        self.self_node.as_ref()?.net_info()
    }

    /// 节点公钥 → 设备上报的主机信息（版本、型号、网络检查结论）
    pub fn peer_hostinfo(&self) -> HashMap<String, Hostinfo> {
        self.peers
            .iter()
            .flatten()
            .filter_map(|p| Some((p.key.clone()?, p.hostinfo.clone()?)))
            .collect()
    }

    /// 节点公钥 → 设备的局域网 IPv4（从上报地址里取私有地址）。
    /// 公网地址不要：同一个家里的设备共用一个公网出口，没法区分是哪台
    pub fn peer_lan_ips(&self) -> HashMap<String, Vec<String>> {
        self.peers
            .iter()
            .flatten()
            .filter_map(|p| {
                let key = p.key.clone()?;
                let ips: Vec<String> = p
                    .endpoints
                    .iter()
                    .flatten()
                    .filter_map(|e| e.parse::<std::net::SocketAddrV4>().ok())
                    .map(|a| *a.ip())
                    .filter(|ip| ip.is_private())
                    .map(|ip| ip.to_string())
                    .collect();
                (!ips.is_empty()).then_some((key, ips))
            })
            .collect()
    }
}

impl PeerStatus {
    /// 显示名：优先用控制端里的名字（MagicDNS 名第一段，在 Headscale 里改名后会跟着变），
    /// 取不到再用设备自己上报的主机名
    pub fn display_name(&self) -> Option<&str> {
        self.dns_name
            .as_deref()
            .and_then(|d| d.split('.').next())
            .filter(|n| !n.is_empty())
            .or_else(|| self.host_name.as_deref().filter(|n| !n.is_empty()))
    }
}

impl Status {
    /// 对端设备列表（不含本机），按显示名排序，保证界面顺序稳定
    pub fn peers(&self) -> Vec<&PeerStatus> {
        let mut list: Vec<_> = self.peer.iter().flat_map(|m| m.values()).collect();
        list.sort_by_key(|p| p.display_name().unwrap_or_default().to_lowercase());
        list
    }
}

/// 解析 netcheck 输出：前面可能有代理日志和 `# Warning` 行，从第一个 `{` 开始解析
pub fn parse_netcheck(text: &str) -> Result<Netcheck, serde_json::Error> {
    let start = text.find('{').unwrap_or(0);
    serde_json::from_str(&text[start..])
}

/// 解析 netmap 输出，与 netcheck 一样从第一个 `{` 开始，容忍前面的日志行
pub fn parse_netmap(text: &str) -> Result<NetMap, serde_json::Error> {
    let start = text.find('{').unwrap_or(0);
    serde_json::from_str(&text[start..])
}

/// 把 RFC 3339 时间转成毫秒时间戳；零值（0001-01-01）、空值或无法解析时返回 None
pub fn time_ms(text: Option<&str>) -> Option<i64> {
    let t = DateTime::parse_from_rfc3339(text?).ok()?;
    let ms = t.timestamp_millis();
    // 零值年份是 1 年，任何早于 2000 年的都视为“无”
    (ms > 946_684_800_000).then_some(ms)
}

/// 去掉空字符串，统一成 None
pub fn non_empty(text: &Option<String>) -> Option<&str> {
    text.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATUS: &str = include_str!("../fixtures/status.json");

    #[test]
    fn 解析真实_status() {
        let s: Status = serde_json::from_str(STATUS).unwrap();
        assert_eq!(s.backend_state.as_deref(), Some("Running"));
        assert_eq!(
            s.self_node.as_ref().unwrap().host_name.as_deref(),
            Some("lan-desktop")
        );
        let names: Vec<_> = s
            .peers()
            .iter()
            .map(|p| p.host_name.clone().unwrap())
            .collect();
        // 按显示名排序：work-laptop 的显示名是 peng-work，排在 phone-router 前面
        assert_eq!(names, ["ming-pc", "work-laptop", "phone-router"]);
    }

    #[test]
    fn 缺字段和多字段都不报错() {
        let s: Status =
            serde_json::from_str(r#"{"Peer":null,"新字段":1,"Self":{"Online":true}}"#).unwrap();
        assert!(s.peers().is_empty());
        assert_eq!(s.self_node.unwrap().online, Some(true));
    }

    #[test]
    fn 解析带前缀的_netcheck() {
        let n = parse_netcheck(include_str!("../fixtures/netcheck.txt")).unwrap();
        assert_eq!(n.udp, Some(true));
        assert_eq!(n.mapping_varies_by_dest_ip, Some(false));
        assert_eq!(n.preferred_derp, Some(999));
        assert!(n.region_latency.unwrap().contains_key("999"));
    }

    #[test]
    fn 解析_prefs_和_derp_map() {
        let p: Prefs = serde_json::from_str(include_str!("../fixtures/prefs.json")).unwrap();
        assert_eq!(p.route_all, Some(false));
        let l = LocalPrefs::from(p);
        // 实机未选出口：ExitNodeID 为空字符串，转成 None
        assert_eq!(
            l,
            LocalPrefs {
                accept_routes: Some(false),
                exit_node_id: None,
                exit_node_allow_lan: Some(false),
                accept_dns: Some(false),
                shields_up: Some(false),
                unattended: Some(true),
            }
        );
        let d: DerpMap = serde_json::from_str(include_str!("../fixtures/derpmap.json")).unwrap();
        let regions = d.regions.unwrap();
        assert_eq!(regions.len(), 29);
        assert_eq!(regions["999"].region_code.as_deref(), Some("headscale"));
    }

    #[test]
    fn 解析_netmap_并按公钥对应() {
        let n = parse_netmap(include_str!("../fixtures/netmap.json")).unwrap();
        assert_eq!(n.self_net_info().unwrap().working_ipv6, Some(true));
        // 工作笔记本：最近一次上报时没有 IPv6，IPv4 是对称型 NAT
        let s: Status = serde_json::from_str(STATUS).unwrap();
        let laptop = s
            .peers()
            .into_iter()
            .find(|p| p.host_name.as_deref() == Some("work-laptop"))
            .unwrap();
        let hostinfo = n.peer_hostinfo();
        let info = hostinfo[laptop.public_key.as_ref().unwrap()]
            .net_info
            .as_ref()
            .unwrap();
        assert_eq!(info.working_ipv6, Some(false));
        assert_eq!(info.mapping_varies_by_dest_ip, Some(true));
    }

    #[test]
    fn 解析共享规则() {
        // 2026-10-19 实机输出：先后加了 TCP 45999 和 HTTP 45998（主机名已替换）
        let s: ServeStatus = serde_json::from_str(
            r#"{"TCP":{"45999":{"TCPForward":"127.0.0.1:45999"},"45998":{"HTTP":true}},
                "Web":{"peng-home.example.com:45998":{"Handlers":{"/":{"Proxy":"http://127.0.0.1:45998"}}}}}"#,
        )
        .unwrap();
        assert_eq!(
            s.rules(),
            [
                ServeRule { kind: "http".into(), port: 45998, target: "http://127.0.0.1:45998".into() },
                ServeRule { kind: "tcp".into(), port: 45999, target: "127.0.0.1:45999".into() },
            ]
        );
        let empty: ServeStatus = serde_json::from_str("{}").unwrap();
        assert!(empty.rules().is_empty());
    }

    #[test]
    fn 比较官方新版本() {
        // 2026-10-19 实机输出（节选）
        let v: VersionInfo = serde_json::from_str(
            r#"{"majorMinorPatch":"1.102.4","short":"1.102.4","cap":142,"upstream":"1.104.1"}"#,
        )
        .unwrap();
        assert_eq!(v.newer_upstream().as_deref(), Some("1.104.1"));
        let same = VersionInfo { short: Some("1.104.1".into()), upstream: Some("1.104.1".into()) };
        assert_eq!(same.newer_upstream(), None);
        // 按数字比较，不按字符串：1.99 比 1.102 旧
        let old = VersionInfo { short: Some("1.102.0".into()), upstream: Some("1.99.9".into()) };
        assert_eq!(old.newer_upstream(), None);
        let bad = VersionInfo { short: Some("1.102.4".into()), upstream: Some("".into()) };
        assert_eq!(bad.newer_upstream(), None);
    }

    #[test]
    fn 时间零值视为无() {
        assert_eq!(time_ms(Some("0001-01-01T00:00:00Z")), None);
        assert_eq!(time_ms(Some("")), None);
        assert_eq!(time_ms(None), None);
        assert_eq!(
            time_ms(Some("2026-10-16T18:17:15.250730444+08:00")),
            Some(1_792_145_835_250)
        );
    }

    #[test]
    fn netmap_只取私有_ipv4_作为局域网地址() {
        let m: NetMap = serde_json::from_str(
            r#"{"Peers":[
                {"Key":"k1","Endpoints":["198.51.100.27:41641","[2001:db8:8256::1]:41641","192.168.66.80:41641"]},
                {"Key":"k2","Endpoints":["198.51.100.112:41641"]},
                {"Key":"k3"}
            ]}"#,
        )
        .unwrap();
        let lan = m.peer_lan_ips();
        assert_eq!(lan.get("k1"), Some(&vec!["192.168.66.80".to_string()]));
        assert_eq!(lan.len(), 1);
    }

    #[test]
    fn 显示名优先用控制端的名字() {
        let s: Status = serde_json::from_str(STATUS).unwrap();
        let names: Vec<_> = s.peers().iter().map(|p| p.display_name().unwrap()).collect();
        // work-laptop 在 Headscale 里改名为 peng-work，排序也按新名字
        assert_eq!(names, ["ming-pc", "peng-work", "phone-router"]);
        let bare = PeerStatus { host_name: Some("box".into()), dns_name: Some("".into()), ..Default::default() };
        assert_eq!(bare.display_name(), Some("box"));
        assert_eq!(PeerStatus::default().display_name(), None);
    }
}
