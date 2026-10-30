//! 子网路由器的健康状态：出门时访问家里局域网全靠它
use crate::classify::{ConnKind, Regions};
use crate::model::Hostinfo;
use crate::netcheck::{Finding, Level};
use serde::Serialize;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RouterHealth {
    pub name: String,
    pub online: bool,
    pub routes: Vec<String>,
    /// 最严重的结论级别，决定卡片颜色
    pub level: Level,
    pub findings: Vec<Finding>,
    /// 到自建中继的延迟（毫秒）
    pub relay_ms: Option<f64>,
    /// 自建中继的显示名，如“自建”或设置里起的“广州”
    pub relay_name: String,
    /// Tailscale 版本（只取 1.102.4 这样的前半段）
    pub version: Option<String>,
    pub os_version: Option<String>,
    pub device_model: Option<String>,
    pub last_seen_ms: Option<i64>,
}

fn finding(level: Level, text: impl Into<String>) -> Finding {
    Finding {
        level,
        text: text.into(),
    }
}

fn severity(level: Level) -> u8 {
    match level {
        Level::Good => 0,
        Level::Info => 1,
        Level::Warn => 2,
        Level::Bad => 3,
    }
}

/// 路由器没有 IPv6 时的结论：直连还能靠 IPv4 打洞，所以先看实际路径，再看它是不是对称型 NAT
fn no_ipv6_finding(direct_v4: bool, symmetric: Option<bool>) -> Finding {
    if direct_v4 {
        return finding(Level::Info, "当前没有 IPv6，但本机现在经 IPv4 和它直连，不受影响");
    }
    match symmetric {
        Some(true) => finding(
            Level::Warn,
            "当前没有 IPv6：在外面经它访问家里时，多半只能走中继（约 500 KB/s）",
        ),
        _ => finding(
            Level::Info,
            "当前没有 IPv6：在外面经它访问家里时要靠 IPv4 打洞直连，打不通才走中继（约 500 KB/s）",
        ),
    }
}

/// 根据在线状态、本机到它的实际路径和路由器自己上报的网络检查结论，给出人话结论
pub fn health(
    name: &str,
    online: bool,
    conn: Option<&ConnKind>,
    routes: Vec<String>,
    last_seen_ms: Option<i64>,
    info: Option<&Hostinfo>,
    regions: &Regions,
) -> RouterHealth {
    let routes_text = routes.join("、");
    let mut findings = Vec::new();
    let net = info.and_then(|h| h.net_info.as_ref());
    let direct_v4 = conn == Some(&ConnKind::Public);
    if !online {
        findings.push(finding(
            Level::Bad,
            format!("离线：出门模式下访问不到家里局域网（{routes_text}）"),
        ));
    } else {
        findings.push(finding(
            Level::Good,
            format!("在线，正在为组网提供家里局域网（{routes_text}）"),
        ));
    }
    match net {
        None => findings.push(finding(Level::Info, "没有读到它上报的网络情况")),
        Some(n) => {
            if n.working_udp == Some(false) {
                findings.push(finding(
                    Level::Bad,
                    "UDP 不通：其他设备只能经中继访问它，速度受限",
                ));
            }
            match n.working_ipv6 {
                Some(true) => findings.push(finding(
                    Level::Good,
                    "有 IPv6：在外面连着有 IPv6 的网络时，可以和它直连",
                )),
                Some(false) => findings.push(no_ipv6_finding(direct_v4, n.mapping_varies_by_dest_ip)),
                None => {}
            }
            // 已经打通了，就不再说打洞基本不通
            if n.mapping_varies_by_dest_ip == Some(true) && !direct_v4 {
                findings.push(finding(
                    Level::Info,
                    "IPv4 是对称型 NAT：IPv4 打洞基本不通，直连主要靠 IPv6",
                ));
            }
        }
    }
    let relay_ms = net.and_then(|n| {
        regions
            .self_hosted_ids()
            .iter()
            .find_map(|id| n.region_latency_ms(id))
    });
    let level = findings
        .iter()
        .map(|f| f.level)
        .max_by_key(|l| severity(*l))
        .unwrap_or(Level::Info);
    RouterHealth {
        name: name.to_string(),
        online,
        routes,
        level,
        findings,
        relay_ms,
        relay_name: regions.self_hosted_name(),
        version: info
            .and_then(|h| h.ipn_version.as_deref())
            .map(|v| v.split('-').next().unwrap_or(v).to_string()),
        os_version: info.and_then(|h| h.os_version.clone()),
        device_model: info.and_then(|h| h.device_model.clone()),
        last_seen_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{NetInfo, Status, parse_netmap};

    #[test]
    fn 真实样本中的手机路由器() {
        let s: Status = serde_json::from_str(include_str!("../fixtures/status.json")).unwrap();
        let n = parse_netmap(include_str!("../fixtures/netmap.json")).unwrap();
        let p = s
            .peers()
            .into_iter()
            .find(|p| p.host_name.as_deref() == Some("phone-router"))
            .unwrap();
        let infos = n.peer_hostinfo();
        let h = health(
            "phone-router",
            true,
            None,
            p.primary_routes.clone().unwrap(),
            None,
            infos.get(p.public_key.as_ref().unwrap()),
            &Regions::default(),
        );
        assert_eq!(h.routes, ["192.168.66.0/24"]);
        assert_eq!(h.level, Level::Info); // 有 IPv6，对称型 NAT 只是说明
        assert_eq!(h.version.as_deref(), Some("1.102.4"));
        assert!((h.relay_ms.unwrap() - 19.1).abs() < 0.1);
        assert_eq!(h.relay_name, "自建");
        assert!(h.findings.iter().any(|f| f.text.starts_with("有 IPv6")));
    }

    #[test]
    fn 离线和没有_ipv6() {
        let info = Hostinfo {
            net_info: Some(NetInfo {
                working_ipv6: Some(false),
                working_udp: Some(true),
                mapping_varies_by_dest_ip: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        };
        let h = health(
            "phone-router",
            false,
            None,
            vec!["192.168.66.0/24".into()],
            None,
            Some(&info),
            &Regions::default(),
        );
        assert_eq!(h.level, Level::Bad);
        assert!(
            h.findings[0]
                .text
                .starts_with("离线：出门模式下访问不到家里局域网")
        );
        assert!(
            h.findings
                .iter()
                .any(|f| f.level == Level::Warn && f.text.contains("没有 IPv6"))
        );
    }

    fn 没有_ipv6_的结论(conn: Option<&ConnKind>, symmetric: Option<bool>) -> RouterHealth {
        let info = Hostinfo {
            net_info: Some(NetInfo {
                working_ipv6: Some(false),
                working_udp: Some(true),
                mapping_varies_by_dest_ip: symmetric,
                ..Default::default()
            }),
            ..Default::default()
        };
        health("r", true, conn, vec!["10.0.0.0/24".into()], None, Some(&info), &Regions::default())
    }

    #[test]
    fn 没有_ipv6_但已经_ipv4_直连时不说走中继() {
        for symmetric in [Some(false), Some(true), None] {
            let h = 没有_ipv6_的结论(Some(&ConnKind::Public), symmetric);
            assert_eq!(h.level, Level::Info);
            assert!(h.findings.iter().any(|f| f.text.contains("和它直连")));
            assert!(!h.findings.iter().any(|f| f.text.contains("多半只能走中继")));
            assert!(!h.findings.iter().any(|f| f.text.contains("打洞基本不通")));
        }
    }

    #[test]
    fn 没有_ipv6_时按_nat_类型判断在外面能否直连() {
        // 不是对称型 NAT，或不知道：还能打洞，不下“只能走中继”的结论
        for symmetric in [Some(false), None] {
            let h = 没有_ipv6_的结论(Some(&ConnKind::Idle), symmetric);
            assert_eq!(h.level, Level::Info);
            assert!(h.findings.iter().any(|f| f.text.contains("打不通才走中继")));
        }
        // 对称型 NAT：确实多半只能走中继，在家走局域网也不改变在外面的判断
        let h = 没有_ipv6_的结论(Some(&ConnKind::Lan), Some(true));
        assert_eq!(h.level, Level::Warn);
        assert!(h.findings.iter().any(|f| f.text.contains("多半只能走中继")));
    }

    #[test]
    fn 读不到上报信息() {
        let h = health("r", true, None, vec!["10.0.0.0/24".into()], None, None, &Regions::default());
        assert!(
            h.findings
                .iter()
                .any(|f| f.text == "没有读到它上报的网络情况")
        );
        assert_eq!(h.relay_ms, None);
    }
}
