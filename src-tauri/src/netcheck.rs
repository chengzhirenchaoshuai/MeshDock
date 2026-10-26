//! 本机网络体检：把 netcheck 结果翻译成面向普通人的结论
use crate::classify::Regions;
use crate::model::{Netcheck, non_empty};
use serde::Serialize;

/// 结论的级别，前端据此选颜色和图标
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Level {
    Good,
    Warn,
    Bad,
    Info,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub level: Level,
    pub text: String,
}

/// 一个中继区域的延迟
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RegionLatency {
    pub id: String,
    pub name: String,
    pub ms: f64,
    pub preferred: bool,
    pub self_hosted: bool,
}

/// 体检报告
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub findings: Vec<Finding>,
    /// 公网 IPv4 出口（含端口）
    pub global_v4: Option<String>,
    /// 公网 IPv6 出口（含端口）
    pub global_v6: Option<String>,
    /// 支持的端口映射协议
    pub port_mapping: Vec<String>,
    /// 按延迟从低到高排列
    pub regions: Vec<RegionLatency>,
    pub at_ms: i64,
}

fn finding(level: Level, text: impl Into<String>) -> Finding {
    Finding {
        level,
        text: text.into(),
    }
}

/// 纳秒 → 毫秒，保留一位小数
fn ns_to_ms(ns: u64) -> f64 {
    (ns as f64 / 100_000.0).round() / 10.0
}

pub fn report(n: &Netcheck, regions: &Regions, at_ms: i64) -> Report {
    let mut findings = Vec::new();

    // UDP：打洞的前提
    match n.udp {
        Some(true) => findings.push(finding(Level::Good, "UDP 可用，具备直连的条件")),
        Some(false) => findings.push(finding(
            Level::Bad,
            "UDP 被阻断，所有连接都只能走中继，速度会受限",
        )),
        None => findings.push(finding(Level::Info, "没能确认 UDP 是否可用")),
    }

    // IPv6：家里和手机热点都有 IPv6，有它基本就能直连
    match (n.ipv6, n.os_has_ipv6) {
        (Some(true), _) => findings.push(finding(
            Level::Good,
            "当前网络有 IPv6，可以和同样有 IPv6 的设备（比如家里）直连",
        )),
        (_, Some(true)) => findings.push(finding(
            Level::Warn,
            "系统支持 IPv6，但当前网络没有 IPv6 公网地址，只能靠 IPv4 打洞或中继",
        )),
        _ => findings.push(finding(
            Level::Warn,
            "当前网络没有 IPv6，只能靠 IPv4 打洞或中继",
        )),
    }

    // IPv4 与 NAT 类型
    match (n.ipv4, n.mapping_varies_by_dest_ip) {
        (Some(false), _) => findings.push(finding(Level::Warn, "没有检测到 IPv4 公网出口")),
        (_, Some(true)) => findings.push(finding(
            Level::Bad,
            "你的 IPv4 是对称型 NAT，和外面的设备基本打不通，需要靠 IPv6 或中继",
        )),
        (_, Some(false)) => findings.push(finding(
            Level::Good,
            "IPv4 不是对称型 NAT，和外面的设备打洞成功率较高",
        )),
        _ => {}
    }

    // 端口映射
    let port_mapping: Vec<String> = [(n.upnp, "UPnP"), (n.pmp, "NAT-PMP"), (n.pcp, "PCP")]
        .iter()
        .filter(|(on, _)| *on == Some(true))
        .map(|(_, name)| name.to_string())
        .collect();
    if port_mapping.is_empty() {
        findings.push(finding(
            Level::Info,
            "路由器没有开启端口映射（UPnP / NAT-PMP / PCP），不影响使用，只是直连机会略少",
        ));
    } else {
        findings.push(finding(
            Level::Good,
            format!(
                "路由器支持端口映射（{}），有助于直连",
                port_mapping.join(" / ")
            ),
        ));
    }

    if n.captive_portal == Some(true) {
        findings.push(finding(
            Level::Warn,
            "当前网络需要先在网页上登录（酒店、机场 Wi-Fi 常见），登录前组网可能不通",
        ));
    }

    // 中继延迟
    let preferred = n.preferred_derp.map(|id| id.to_string());
    let mut list: Vec<RegionLatency> = n
        .region_latency
        .iter()
        .flatten()
        .map(|(id, ns)| RegionLatency {
            id: id.clone(),
            name: regions.name_by_id(id),
            ms: ns_to_ms(*ns),
            preferred: preferred.as_deref() == Some(id.as_str()),
            self_hosted: regions.is_self_hosted_id(id),
        })
        .collect();
    list.sort_by(|a, b| a.ms.total_cmp(&b.ms));

    match list.iter().find(|r| r.self_hosted) {
        Some(r) => findings.push(finding(
            Level::Info,
            format!("到{}中继的延迟 {} ms", r.name, fmt_ms(r.ms)),
        )),
        None if regions.has_self_hosted() => findings.push(finding(
            Level::Warn,
            format!(
                "连不上{}中继，直连失败时设备之间可能不通",
                regions.self_hosted_name()
            ),
        )),
        // 组网只用官方中继，没有自建中继可测
        None => {}
    }
    if let Some(p) = list.iter().find(|r| r.preferred && !r.self_hosted) {
        findings.push(finding(
            Level::Info,
            format!("当前首选的中继是 {}（{} ms）", p.name, fmt_ms(p.ms)),
        ));
    }

    Report {
        findings,
        global_v4: non_empty(&n.global_v4).map(str::to_string),
        global_v6: non_empty(&n.global_v6).map(str::to_string),
        port_mapping,
        regions: list,
        at_ms,
    }
}

/// 毫秒的展示，与前端 formatLatency 一致：10 ms 以上取整，以下保留一位小数
fn fmt_ms(ms: f64) -> String {
    if ms >= 10.0 || ms.fract() == 0.0 {
        format!("{ms:.0}")
    } else {
        format!("{ms:.1}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DerpMap, parse_netcheck};

    fn regions() -> Regions {
        let map: DerpMap = serde_json::from_str(include_str!("../fixtures/derpmap.json")).unwrap();
        Regions::from_derp_map(&map, "广州")
    }

    fn texts(r: &Report) -> Vec<String> {
        r.findings.iter().map(|f| f.text.clone()).collect()
    }

    #[test]
    fn 验收场景_家里网络体检() {
        // 本机真实输出：有 IPv6、IPv4 不是对称型 NAT、到广州约 18 ms
        let n = parse_netcheck(include_str!("../fixtures/netcheck.txt")).unwrap();
        let r = report(&n, &regions(), 0);
        let t = texts(&r);
        assert!(t.iter().any(|s| s.starts_with("当前网络有 IPv6")));
        assert!(t.iter().any(|s| s.starts_with("IPv4 不是对称型 NAT")));
        assert!(t.contains(&"到广州中继的延迟 18 ms".to_string()), "{t:?}");
        assert!(r.findings.iter().all(|f| f.level != Level::Bad));
        // 区域按延迟升序，首个是首选的广州
        assert_eq!(r.regions.len(), 29);
        assert!(r.regions[0].preferred && r.regions[0].self_hosted);
        assert!(r.regions.windows(2).all(|w| w[0].ms <= w[1].ms));
    }

    #[test]
    fn 对称型_nat_与无_ipv6() {
        let n = Netcheck {
            udp: Some(true),
            ipv4: Some(true),
            ipv6: Some(false),
            os_has_ipv6: Some(true),
            mapping_varies_by_dest_ip: Some(true),
            ..Default::default()
        };
        let r = report(&n, &regions(), 0);
        let t = texts(&r);
        assert!(
            t.iter()
                .any(|s| s.contains("对称型 NAT，和外面的设备基本打不通"))
        );
        assert!(
            t.iter()
                .any(|s| s.starts_with("系统支持 IPv6，但当前网络没有"))
        );
        assert!(t.contains(&"连不上广州中继，直连失败时设备之间可能不通".to_string()));
    }

    #[test]
    fn udp_被阻断() {
        let n = Netcheck {
            udp: Some(false),
            ..Default::default()
        };
        let r = report(&n, &regions(), 0);
        assert_eq!(r.findings[0].level, Level::Bad);
    }

    #[test]
    fn 纳秒换算() {
        assert_eq!(ns_to_ms(17_914_600), 17.9);
        assert_eq!(fmt_ms(20.0), "20");
        assert_eq!(fmt_ms(17.9), "18");
        assert_eq!(fmt_ms(2.46), "2.5");
    }
}
