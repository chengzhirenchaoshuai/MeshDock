//! 诊断报告：汇总状态、体检、设置，方便发给运维。导出前把公网地址和密钥打码
use crate::monitor::Overview;
use crate::netcheck;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// 公网 IPv4 只保留第一段，如 198.*.*.*；局域网、组网（100.64/10）、回环等内网地址原样保留
fn mask_v4(ip: Ipv4Addr) -> Option<String> {
    let o = ip.octets();
    let internal = ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_unspecified()
        || (o[0] == 100 && (64..128).contains(&o[1])) // 组网地址 100.64.0.0/10
        || (o[0] == 198 && (18..20).contains(&o[1])); // 198.18.0.0/15，代理软件虚拟网卡常用
    (!internal).then(|| format!("{}.*.*.*", o[0]))
}

/// 公网 IPv6（2000::/3）只保留前两段；fd7a: 等唯一本地地址、链路本地原样保留
fn mask_v6(ip: Ipv6Addr) -> Option<String> {
    let s = ip.segments();
    ((s[0] & 0xe000) == 0x2000).then(|| format!("{:x}:{:x}:*", s[0], s[1]))
}

/// 把一段由数字、十六进制字母、冒号、点组成的文字中的公网地址打码
fn mask_token(token: &str) -> Option<String> {
    if let Ok(ip) = token.parse::<IpAddr>() {
        return match ip {
            IpAddr::V4(v4) => mask_v4(v4),
            IpAddr::V6(v6) => mask_v6(v6),
        };
    }
    // IPv4 带端口：198.51.100.27:36637
    let (host, port) = token.rsplit_once(':')?;
    let v4: Ipv4Addr = host.parse().ok()?;
    port.parse::<u16>().ok()?;
    Some(format!("{}:{port}", mask_v4(v4)?))
}

/// 节点密钥只保留前 8 位
fn mask_keys(text: &str) -> String {
    let mut out = text.to_string();
    for prefix in [
        "nodekey:",
        "discokey:",
        "mkey:",
        "nlpub:",
        "privkey:",
        "nlpriv:",
    ] {
        let mut result = String::with_capacity(out.len());
        let mut rest = out.as_str();
        while let Some(i) = rest.find(prefix) {
            let start = i + prefix.len();
            result.push_str(&rest[..start]);
            let hex_len = rest[start..]
                .find(|c: char| !c.is_ascii_hexdigit())
                .unwrap_or(rest.len() - start);
            let keep = hex_len.min(8);
            result.push_str(&rest[start..start + keep]);
            if hex_len > keep {
                result.push('…');
            }
            rest = &rest[start + hex_len..];
        }
        result.push_str(rest);
        out = result;
    }
    out
}

/// 生成报告所需的材料；原始输出读取失败时放错误说明
pub struct Input<'a> {
    pub version: &'a str,
    pub generated_at: String,
    pub overview: Option<&'a Overview>,
    pub snapshot_error: Option<&'a str>,
    pub netcheck: Option<&'a netcheck::Report>,
    pub health: Vec<String>,
    pub raw_status: String,
    pub raw_prefs: String,
    pub raw_netcheck: String,
    /// 最近的 MeshDock 日志
    pub recent_log: String,
}

fn level_text(level: netcheck::Level) -> &'static str {
    match level {
        netcheck::Level::Good => "正常",
        netcheck::Level::Warn => "注意",
        netcheck::Level::Bad => "问题",
        netcheck::Level::Info => "说明",
    }
}

/// 拼出报告全文并打码
pub fn compose(input: &Input) -> String {
    let mut t = String::new();
    let mut line = |s: String| {
        t.push_str(&s);
        t.push('\n');
    };
    line("MeshDock 诊断报告".into());
    line(format!("生成时间：{}", input.generated_at));
    line(format!("MeshDock 版本：{}", input.version));
    line("说明：公网 IP 只保留第一段，节点密钥只保留前 8 位；局域网和组网地址保留原样。".into());

    line(String::new());
    line("## 概况".into());
    match input.overview {
        Some(o) => {
            line(format!(
                "组网状态：{}",
                o.backend_state.as_deref().unwrap_or("未知")
            ));
            line(format!(
                "控制端：{}",
                if o.control_unreachable {
                    "连不上"
                } else {
                    "正常"
                }
            ));
            line(format!(
                "在家 / 出门模式：{}",
                match o.accept_routes {
                    Some(true) => "出门模式（accept-routes 已开启）",
                    Some(false) => "在家模式（accept-routes 已关闭）",
                    None => "读取不到",
                }
            ));
            if let Some(p) = &o.prefs {
                let exit = match &p.exit_node_id {
                    None => "不用".to_string(),
                    Some(id) => o
                        .peers
                        .iter()
                        .find(|d| &d.id == id)
                        .map_or_else(|| format!("{id}（不在设备列表中）"), |d| d.name.clone()),
                };
                let yn = |v: Option<bool>| match v {
                    Some(true) => "开",
                    Some(false) => "关",
                    None => "读取不到",
                };
                line(format!(
                    "出口：{exit}（走出口时访问本地局域网：{}）",
                    yn(p.exit_node_allow_lan)
                ));
                line(format!(
                    "组网 DNS：{}；挡住传入连接：{}；无人登录时保持连接：{}",
                    yn(p.accept_dns),
                    yn(p.shields_up),
                    yn(p.unattended)
                ));
            }
            let l = &o.location;
            line(format!(
                "位置：{}（{}；默认网关：{}；家里网关：{}）",
                match l.at_home {
                    Some(true) => "在家",
                    Some(false) => "在外面",
                    None => "无法判断",
                },
                if l.auto {
                    "自动识别"
                } else {
                    "手动标记"
                },
                if l.gateways.is_empty() {
                    "无".into()
                } else {
                    l.gateways.join("、")
                },
                l.home_gateway
            ));
            line(String::new());
            line("## 设备".into());
            if let Some(d) = &o.self_device {
                line(format!(
                    "- {}（本机）{} {}",
                    d.name,
                    d.ips.join(" "),
                    d.os.as_deref().unwrap_or("")
                ));
            }
            for d in &o.peers {
                let hint = d
                    .conn
                    .hint
                    .as_deref()
                    .map(|h| format!("；{h}"))
                    .unwrap_or_default();
                line(format!(
                    "- {} {} {}：{}{hint}",
                    d.name,
                    d.ips.join(" "),
                    d.os.as_deref().unwrap_or(""),
                    d.conn.label
                ));
            }
            for r in &o.routers {
                line(String::new());
                line(format!(
                    "## 子网路由器：{}（{}）",
                    r.name,
                    r.routes.join("、")
                ));
                for f in &r.findings {
                    line(format!("- [{}] {}", level_text(f.level), f.text));
                }
                if let Some(ms) = r.relay_ms {
                    line(format!("- 到广州中继 {ms:.0} ms"));
                }
                let model = [
                    r.device_model.as_deref(),
                    r.os_version.as_deref(),
                    r.version.as_deref(),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join("，");
                if !model.is_empty() {
                    line(format!("- 设备：{model}"));
                }
            }
        }
        None => line(format!(
            "读取组网状态失败：{}",
            input.snapshot_error.unwrap_or("未知原因")
        )),
    }

    line(String::new());
    line("## 本机网络体检".into());
    match input.netcheck {
        Some(r) => {
            for f in &r.findings {
                line(format!("- [{}] {}", level_text(f.level), f.text));
            }
        }
        None => line("体检失败，见下方原始输出".into()),
    }

    line(String::new());
    line("## 官方客户端告警".into());
    if input.health.is_empty() {
        line("无".into());
    }
    for h in &input.health {
        line(format!("- {h}"));
    }

    for (title, raw) in [
        ("tailscale status --json", &input.raw_status),
        (
            "tailscale debug prefs（已去掉含密钥的 Config 部分）",
            &input.raw_prefs,
        ),
        ("tailscale netcheck --format=json", &input.raw_netcheck),
    ] {
        line(String::new());
        line(format!("## 原始输出：{title}"));
        line(raw.trim_end().to_string());
    }
    line(String::new());
    line("## MeshDock 最近日志".into());
    line(match input.recent_log.trim_end() {
        "" => "无".into(),
        log => log.to_string(),
    });
    mask(&t)
}

/// prefs 原文去掉 Config（含密钥占位与登录信息）后重新排版；解析失败时原样返回
pub fn strip_prefs(raw: &str) -> String {
    match serde_json::from_str::<serde_json::Value>(raw) {
        Ok(mut v) => {
            if let Some(obj) = v.as_object_mut() {
                obj.remove("Config");
            }
            serde_json::to_string_pretty(&v).unwrap_or_else(|_| raw.to_string())
        }
        Err(_) => raw.to_string(),
    }
}

/// 打码：公网 IP 和节点密钥
pub fn mask(text: &str) -> String {
    let text = mask_keys(text);
    let is_ip_char = |c: char| c.is_ascii_hexdigit() || c == ':' || c == '.';
    let mut out = String::with_capacity(text.len());
    let mut run = String::new();
    let flush = |run: &mut String, out: &mut String| {
        if !run.is_empty() {
            match mask_token(run) {
                Some(masked) => out.push_str(&masked),
                None => out.push_str(run),
            }
            run.clear();
        }
    };
    for c in text.chars() {
        if is_ip_char(c) {
            run.push(c);
        } else {
            flush(&mut run, &mut out);
            out.push(c);
        }
    }
    flush(&mut run, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 公网地址打码_内网地址保留() {
        assert_eq!(
            mask("GlobalV4: 198.51.100.27:36637"),
            "GlobalV4: 198.*.*.*:36637"
        );
        assert_eq!(
            mask("\"[2001:db8:4e9e:32c5:9550:b39b:9cb2:84df]:61165\""),
            "\"[2001:db8:*]:61165\""
        );
        assert_eq!(
            mask("控制端 https://203.0.113.1:8443"),
            "控制端 https://203.*.*.*:8443"
        );
        for keep in [
            "192.168.66.233:41641",
            "100.64.0.3",
            "fd7a:115c:a1e0::3",
            "198.18.0.2",
            "127.0.0.1",
            "[fe80::1]",
        ] {
            assert_eq!(mask(keep), keep);
        }
    }

    #[test]
    fn 不误伤时间和版本号() {
        let t = "\"Now\": \"2026-10-17T06:50:30.2414716Z\", 版本 1.102.4, 延迟 17914600";
        assert_eq!(mask(t), t);
    }

    #[test]
    fn 密钥只留前_8_位() {
        assert_eq!(
            mask("\"PublicKey\": \"nodekey:ca71619149f13167c3d336c4d910bd24\""),
            "\"PublicKey\": \"nodekey:ca716191…\""
        );
        assert_eq!(mask("nodekey:abc"), "nodekey:abc");
    }

    #[test]
    fn 设置原文去掉_config() {
        let raw = r#"{"ControlURL":"https://203.0.113.1:8443","RouteAll":false,"Config":{"PrivateNodeKey":"privkey:00"}}"#;
        let out = strip_prefs(raw);
        assert!(!out.contains("Config") && !out.contains("privkey"), "{out}");
        assert!(out.contains("RouteAll"));
    }

    #[test]
    fn 报告全文已打码() {
        let input = Input {
            version: "0.2.0",
            generated_at: "2026-10-17 16:30:00".into(),
            overview: None,
            snapshot_error: Some("服务没运行"),
            netcheck: None,
            health: vec!["Some peers are advertising routes but --accept-routes is false".into()],
            raw_status: include_str!("../fixtures/status.json").into(),
            raw_prefs: strip_prefs(include_str!("../fixtures/prefs.json")),
            raw_netcheck: include_str!("../fixtures/netcheck.txt").into(),
            recent_log: "2026-10-17 16:29:00 [错误] 连接组网失败：dial 203.0.113.77:443".into(),
        };
        let out = compose(&input);
        assert!(out.contains("读取组网状态失败：服务没运行"));
        assert!(out.contains("## 原始输出：tailscale netcheck --format=json"));
        assert!(out.contains("## MeshDock 最近日志"));
        assert!(
            out.contains("连接组网失败：dial 203.*.*.*:443"),
            "日志应附上并打码"
        );
        assert!(
            !out.contains("203.0.113.10") && !out.contains("2001:db8:1::"),
            "未打码"
        );
        assert!(out.contains("100.64.0.3"), "组网地址应保留");
    }

    /// 用本机真实输出生成报告，确认真实的公网出口地址没有出现在报告里。需手动运行：cargo test -- --ignored
    #[tokio::test]
    #[ignore]
    async fn 实机_报告不泄露公网地址() {
        let nc = crate::cli::run_checked(&["netcheck", "--format=json"])
            .await
            .unwrap();
        let n = crate::model::parse_netcheck(&nc.stdout).unwrap();
        let status = crate::cli::run_checked(&["status", "--json"])
            .await
            .unwrap();
        let input = Input {
            version: "test",
            generated_at: String::new(),
            overview: None,
            snapshot_error: None,
            netcheck: None,
            health: vec![],
            raw_status: status.stdout,
            raw_prefs: String::new(),
            raw_netcheck: nc.text(),
            recent_log: String::new(),
        };
        let out = compose(&input);
        for addr in [n.global_v4, n.global_v6].into_iter().flatten() {
            // 去掉端口和方括号，只比较地址本身
            let host = addr.rsplit_once(':').map_or(addr.as_str(), |(h, _)| h);
            let host = host.trim_matches(['[', ']']);
            assert!(!out.contains(host), "报告中出现了未打码的公网地址");
        }
    }

    #[test]
    fn 真实样本打码后不含原始公网地址() {
        // 样本已脱敏为文档地址（203.0.113.x / 2001:db8::），文档地址也属于公网段，应被打码
        let out = mask(include_str!("../fixtures/netcheck.txt"));
        assert!(!out.contains("203.0.113.10"), "{out}");
        assert!(!out.contains("2001:db8:1::"), "{out}");
        assert!(out.contains("203.*.*.*"));
    }
}
