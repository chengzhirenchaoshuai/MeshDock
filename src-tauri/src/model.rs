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
}

/// `tailscale debug prefs`，只取用得到的字段
#[derive(Deserialize, Default, Debug)]
#[serde(default, rename_all = "PascalCase")]
pub struct Prefs {
    #[serde(rename = "ControlURL")]
    pub control_url: Option<String>,
    /// 即 accept-routes
    pub route_all: Option<bool>,
    #[serde(rename = "ExitNodeID")]
    pub exit_node_id: Option<String>,
    pub want_running: Option<bool>,
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
}

impl Status {
    /// 对端设备列表（不含本机），按名称排序，保证界面顺序稳定
    pub fn peers(&self) -> Vec<&PeerStatus> {
        let mut list: Vec<_> = self.peer.iter().flat_map(|m| m.values()).collect();
        list.sort_by_key(|p| p.host_name.clone().unwrap_or_default().to_lowercase());
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
        assert_eq!(names, ["ming-pc", "phone-router", "work-laptop"]);
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
    fn 时间零值视为无() {
        assert_eq!(time_ms(Some("0001-01-01T00:00:00Z")), None);
        assert_eq!(time_ms(Some("")), None);
        assert_eq!(time_ms(None), None);
        assert_eq!(
            time_ms(Some("2026-10-16T18:17:15.250730444+08:00")),
            Some(1_792_145_835_250)
        );
    }
}
