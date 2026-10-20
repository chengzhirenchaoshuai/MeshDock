//! 系统信息：经 ssh 到 Linux 设备上只读采集运行时长、负载、内存、磁盘、IPv6、电池、监听端口
use serde::Serialize;
use std::time::Duration;

/// 整条 ssh 命令的硬超时（含建立连接）
const TIMEOUT: Duration = Duration::from_secs(15);

/// 远端执行的命令：只读，每段前面打 @@ 标记，某段失败不影响其他段。
/// 写成一行，避免 Windows 传参时换行出问题
const SCRIPT: &str = "echo @@uptime; cat /proc/uptime 2>/dev/null; \
echo @@load; cat /proc/loadavg 2>/dev/null; \
echo @@nproc; nproc 2>/dev/null; \
echo @@mem; grep -E '^(MemTotal|MemAvailable|SwapTotal|SwapFree):' /proc/meminfo 2>/dev/null; \
echo @@df; df -P -k / /home /data 2>/dev/null; \
echo @@ipv6; ip -6 addr show scope global 2>/dev/null; \
echo @@battery; for f in capacity temp status; do printf '%s=' $f; cat /sys/class/power_supply/battery/$f 2>/dev/null || echo; done; \
echo @@ports; ss -ltn 2>/dev/null || netstat -ltn 2>/dev/null";

/// 一个分区的用量
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Disk {
    pub mount: String,
    pub total_kb: u64,
    pub used_kb: u64,
}

/// 电池（安卓手机的 Linux 环境能读到 /sys/class/power_supply/battery）
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Battery {
    pub percent: Option<u8>,
    /// 摄氏度；内核给的是 0.1 ℃ 为单位
    pub temp_c: Option<f64>,
    /// 内核原文：Charging、Discharging、Full、Not charging
    pub status: Option<String>,
}

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SysInfo {
    pub uptime_secs: Option<u64>,
    /// 1、5、15 分钟平均负载
    pub load: Option<[f64; 3]>,
    pub cpus: Option<u32>,
    pub mem_total_kb: Option<u64>,
    pub mem_available_kb: Option<u64>,
    pub swap_total_kb: Option<u64>,
    pub swap_free_kb: Option<u64>,
    pub disks: Vec<Disk>,
    /// 全局 IPv6 地址（不含链路本地）；为空说明拿不到 IPv6，直连可能只能靠 IPv4 打洞或中继
    pub ipv6: Vec<String>,
    pub battery: Option<Battery>,
    /// 对外监听的 TCP 端口（只监听 127.0.0.1 / ::1 的不算），从小到大
    pub ports: Vec<u16>,
}

/// 把输出按 @@ 标记切成段：(段名, 段内容)
fn sections(text: &str) -> Vec<(&str, Vec<&str>)> {
    let mut out: Vec<(&str, Vec<&str>)> = Vec::new();
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(name) = line.strip_prefix("@@") {
            out.push((name.trim(), Vec::new()));
        } else if let Some((_, lines)) = out.last_mut() {
            lines.push(line);
        }
    }
    out
}

/// 解析采集输出；没有任何 @@ 标记时返回 None（多半不是 Linux，或命令没执行）
pub fn parse(text: &str) -> Option<SysInfo> {
    let secs = sections(text);
    if secs.is_empty() {
        return None;
    }
    let mut info = SysInfo::default();
    for (name, lines) in secs {
        let first = lines.first().copied().unwrap_or("");
        match name {
            "uptime" => {
                info.uptime_secs = first
                    .split_whitespace()
                    .next()
                    .and_then(|s| s.parse::<f64>().ok())
                    .map(|s| s as u64);
            }
            "load" => {
                let v: Vec<f64> = first
                    .split_whitespace()
                    .take(3)
                    .filter_map(|s| s.parse().ok())
                    .collect();
                info.load = <[f64; 3]>::try_from(v).ok();
            }
            "nproc" => info.cpus = first.trim().parse().ok(),
            "mem" => {
                for l in &lines {
                    let Some((key, rest)) = l.split_once(':') else {
                        continue;
                    };
                    let kb = rest.split_whitespace().next().and_then(|s| s.parse().ok());
                    match key {
                        "MemTotal" => info.mem_total_kb = kb,
                        "MemAvailable" => info.mem_available_kb = kb,
                        "SwapTotal" => info.swap_total_kb = kb,
                        "SwapFree" => info.swap_free_kb = kb,
                        _ => {}
                    }
                }
            }
            "df" => info.disks = parse_df(&lines),
            "ipv6" => {
                info.ipv6 = lines
                    .iter()
                    .filter_map(|l| l.trim().strip_prefix("inet6 "))
                    .filter_map(|rest| rest.split('/').next())
                    .map(str::to_string)
                    .collect();
            }
            "battery" => info.battery = parse_battery(&lines),
            "ports" => info.ports = parse_ports(&lines),
            _ => {}
        }
    }
    Some(info)
}

/// df -P -k：同一个文件系统挂在多处（如 /home 就在 / 上）时只留第一个
fn parse_df(lines: &[&str]) -> Vec<Disk> {
    let mut seen = Vec::new();
    let mut disks = Vec::new();
    for l in lines.iter().skip_while(|l| !l.starts_with("Filesystem")).skip(1) {
        let cols: Vec<&str> = l.split_whitespace().collect();
        let [fs, total, used, _avail, _pct, mount] = cols[..] else {
            continue;
        };
        let (Ok(total_kb), Ok(used_kb)) = (total.parse(), used.parse()) else {
            continue;
        };
        if seen.contains(&fs) {
            continue;
        }
        seen.push(fs);
        disks.push(Disk {
            mount: mount.to_string(),
            total_kb,
            used_kb,
        });
    }
    disks
}

/// 读不到任何一项时视为没有电池
fn parse_battery(lines: &[&str]) -> Option<Battery> {
    let get = |key: &str| {
        lines
            .iter()
            .find_map(|l| l.strip_prefix(key)?.strip_prefix('='))
            .map(str::trim)
            .filter(|v| !v.is_empty())
    };
    let b = Battery {
        percent: get("capacity").and_then(|v| v.parse().ok()),
        temp_c: get("temp")
            .and_then(|v| v.parse::<f64>().ok())
            .map(|t| t / 10.0),
        status: get("status").map(str::to_string),
    };
    (b.percent.is_some() || b.temp_c.is_some() || b.status.is_some()).then_some(b)
}

/// ss -ltn / netstat -ltn：每行第一个带冒号的字段是本地地址，取最后一个冒号后的端口
fn parse_ports(lines: &[&str]) -> Vec<u16> {
    let mut ports: Vec<u16> = lines
        .iter()
        .filter(|l| l.contains("LISTEN"))
        .filter_map(|l| l.split_whitespace().find(|c| c.contains(':')))
        .filter(|addr| !addr.starts_with("127.") && !addr.starts_with("[::1]") && !addr.starts_with("::1:"))
        .filter_map(|addr| addr.rsplit(':').next()?.parse().ok())
        .collect();
    ports.sort_unstable();
    ports.dedup();
    ports
}

/// 经 ssh 采集。BatchMode：需要密码或确认主机指纹时直接失败，不会卡住等输入
pub async fn fetch(alias: &str) -> Result<SysInfo, String> {
    let stdout = crate::ssh::output(alias, SCRIPT, TIMEOUT).await?;
    parse(&stdout).ok_or_else(|| "没有采集到信息，对方可能不是 Linux".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-10-18 在手机 Ubuntu 层（phone）执行 SCRIPT 的真实输出，IPv6 地址已替换为文档地址
    const PHONE: &str = include_str!("../fixtures/sysinfo-phone.txt");

    #[test]
    fn 解析手机的真实输出() {
        let i = parse(PHONE).unwrap();
        assert_eq!(i.uptime_secs, Some(2_380_130));
        assert_eq!(i.load, Some([1.23, 1.34, 1.42]));
        assert_eq!(i.cpus, Some(8));
        assert_eq!(i.mem_total_kb, Some(11_877_508));
        assert_eq!(i.mem_available_kb, Some(8_322_152));
        assert_eq!(i.swap_total_kb, Some(6_291_452));
        assert_eq!(i.swap_free_kb, Some(4_413_124));
        assert_eq!(
            i.disks,
            vec![
                Disk { mount: "/".into(), total_kb: 16_356_264, used_kb: 5_194_584 },
                Disk { mount: "/data".into(), total_kb: 487_085_036, used_kb: 187_887_432 },
            ]
        );
        assert_eq!(i.ipv6, vec!["2001:db8:1::a861", "2001:db8:1::ab59"]);
        assert_eq!(
            i.battery,
            Some(Battery { percent: Some(100), temp_c: Some(30.5), status: Some("Full".into()) })
        );
        assert_eq!(i.ports, vec![139, 445, 2222, 2223, 8090, 46590]);
    }

    #[test]
    fn 读不到的段留空() {
        let i = parse("@@uptime\n@@battery\ncapacity=\ntemp=\nstatus=\n@@ipv6\n@@ports\n").unwrap();
        assert_eq!(i.uptime_secs, None);
        assert_eq!(i.battery, None);
        assert!(i.ipv6.is_empty());
        assert!(i.ports.is_empty());
    }

    #[test]
    fn 没有标记视为失败() {
        assert_eq!(parse("'echo' is not recognized as an internal command\r\n"), None);
    }

    #[test]
    fn 只监听本机回环的端口不算() {
        let lines = [
            "LISTEN 0 128 127.0.0.1:631 0.0.0.0:*",
            "LISTEN 0 128 [::1]:631 [::]:*",
            "tcp 0 0 0.0.0.0:22 0.0.0.0:* LISTEN",
        ];
        assert_eq!(parse_ports(&lines), vec![22]);
    }

    #[test]
    fn 同一文件系统只留一个挂载点() {
        let lines = [
            "Filesystem 1024-blocks Used Available Capacity Mounted on",
            "/dev/sda1 100 40 60 40% /",
            "/dev/sda1 100 40 60 40% /",
        ];
        assert_eq!(parse_df(&lines).len(), 1);
    }

    /// 实机：经 ~/.ssh/config 里的 phone 采集（cargo test -- --ignored）
    #[tokio::test]
    #[ignore]
    async fn 实机_采集手机() {
        let i = fetch("phone").await.unwrap();
        assert!(i.uptime_secs.is_some() && i.mem_total_kb.is_some());
        println!("{i:#?}");
    }
}
