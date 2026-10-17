//! 快捷连接：SSH（按 ~/.ssh/config 的别名打开终端）和 RustDesk（用组网 IP 直连）
use std::path::PathBuf;
use std::process::Command;

/// ~/.ssh/config 中的一条主机：别名 → HostName
#[derive(Debug, PartialEq)]
pub struct SshHost {
    pub alias: String,
    pub host_name: String,
}

/// 解析 ssh 配置，只取 Host 别名和 HostName；带通配符的 Host 忽略，Include 暂不展开
pub fn parse_ssh_config(text: &str) -> Vec<SshHost> {
    let mut hosts = Vec::new();
    let mut aliases: Vec<String> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // 关键字与值之间可以是空格或等号
        let (key, value) = match line.split_once(|c: char| c.is_whitespace() || c == '=') {
            Some((k, v)) => (
                k.to_ascii_lowercase(),
                v.trim_start_matches(['=', ' ', '\t']).trim(),
            ),
            None => continue,
        };
        match key.as_str() {
            "host" => {
                aliases = value
                    .split_whitespace()
                    .filter(|a| !a.contains(['*', '?', '!']))
                    .map(str::to_string)
                    .collect();
            }
            "hostname" => {
                for alias in aliases.drain(..) {
                    hosts.push(SshHost {
                        alias,
                        host_name: value.to_string(),
                    });
                }
            }
            _ => {}
        }
    }
    hosts
}

/// 找设备对应的 SSH 别名：HostName 等于设备的组网 IP、名称或 MagicDNS 名
pub fn alias_for<'a>(hosts: &'a [SshHost], candidates: &[&str]) -> Option<&'a str> {
    hosts
        .iter()
        .find(|h| {
            candidates
                .iter()
                .any(|c| h.host_name.eq_ignore_ascii_case(c))
        })
        .map(|h| h.alias.as_str())
}

/// 读取当前用户的 ~/.ssh/config；没有时返回空
pub fn load_ssh_hosts() -> Vec<SshHost> {
    let Some(home) = std::env::var_os("USERPROFILE") else {
        return Vec::new();
    };
    let path = PathBuf::from(home).join(".ssh").join("config");
    std::fs::read_to_string(path)
        .map(|t| parse_ssh_config(&t))
        .unwrap_or_default()
}

/// 别名只允许常见字符，避免被终端当成命令分隔符或选项
fn valid_alias(alias: &str) -> bool {
    !alias.is_empty()
        && !alias.starts_with('-')
        && alias
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
}

/// 在新终端中执行 `ssh <别名>`：有 Windows Terminal 就用它，否则开一个新的控制台窗口
pub fn open_ssh(alias: &str) -> Result<(), String> {
    if !valid_alias(alias) {
        return Err(format!("SSH 别名含有不支持的字符：{alias}"));
    }
    let wt = std::env::var_os("PATH").and_then(|p| {
        std::env::split_paths(&p)
            .map(|d| d.join("wt.exe"))
            .find(|f| f.is_file())
    });
    let result = match wt {
        Some(wt) => Command::new(wt)
            .args(["new-tab", "--title", &format!("SSH {alias}"), "ssh", alias])
            .spawn(),
        None => {
            let mut cmd = Command::new("ssh");
            cmd.arg(alias);
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
                cmd.creation_flags(CREATE_NEW_CONSOLE);
            }
            cmd.spawn()
        }
    };
    result.map(|_| ()).map_err(|e| format!("打开终端失败：{e}"))
}

/// 本机是否装了 RustDesk：看 rustdesk:// 链接协议有没有注册
#[cfg(windows)]
pub fn rustdesk_installed() -> bool {
    use windows_sys::Win32::System::Registry::{HKEY_CLASSES_ROOT, RRF_RT_REG_SZ, RegGetValueW};
    let subkey: Vec<u16> = "rustdesk\\shell\\open\\command\0".encode_utf16().collect();
    let mut size = 0u32;
    // SAFETY: 只查询默认值是否存在，不读取内容（数据指针为空，仅返回所需大小）
    let status = unsafe {
        RegGetValueW(
            HKEY_CLASSES_ROOT,
            subkey.as_ptr(),
            std::ptr::null(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut size,
        )
    };
    status == 0 && size > 0
}

#[cfg(not(windows))]
pub fn rustdesk_installed() -> bool {
    false
}

/// RustDesk 直连链接；`RustDesk.exe --connect <id>` 在内部也是转成这个链接（见 RustDesk 源码 core_main.rs）
pub fn rustdesk_url(ip: &str) -> Result<String, String> {
    let ip: std::net::IpAddr = ip.parse().map_err(|_| format!("IP 地址格式不正确：{ip}"))?;
    Ok(format!("rustdesk://connect/{ip}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &str = "\
# 注释
Host gz
    HostName 203.0.113.1
Host ming
    HostName 100.64.0.3
    User ming
Host peng-work laptop
  HostName=100.64.0.4
Host *.example.com
    HostName 10.0.0.1
";

    #[test]
    fn 解析别名与主机() {
        let hosts = parse_ssh_config(CONFIG);
        let pairs: Vec<_> = hosts
            .iter()
            .map(|h| (h.alias.as_str(), h.host_name.as_str()))
            .collect();
        assert_eq!(
            pairs,
            [
                ("gz", "203.0.113.1"),
                ("ming", "100.64.0.3"),
                ("peng-work", "100.64.0.4"),
                ("laptop", "100.64.0.4"),
            ]
        );
    }

    #[test]
    fn 按组网_ip_或名称找别名() {
        let hosts = parse_ssh_config(CONFIG);
        assert_eq!(alias_for(&hosts, &["100.64.0.3", "ming-pc"]), Some("ming"));
        assert_eq!(alias_for(&hosts, &["100.64.0.2", "phone-router"]), None);
        let by_name = parse_ssh_config("Host m\n  HostName MING-PC\n");
        assert_eq!(alias_for(&by_name, &["100.64.0.9", "ming-pc"]), Some("m"));
    }

    #[test]
    fn 别名与链接校验() {
        assert!(valid_alias("peng-work"));
        assert!(!valid_alias("-oProxyCommand=x"));
        assert!(!valid_alias("a;calc"));
        assert_eq!(
            rustdesk_url("100.64.0.3").unwrap(),
            "rustdesk://connect/100.64.0.3"
        );
        assert!(rustdesk_url("--help").is_err());
    }

    /// 读取本机注册表，需手动运行：cargo test -- --ignored
    #[test]
    #[ignore]
    fn 实机_检测_rustdesk() {
        assert!(rustdesk_installed());
        assert!(!load_ssh_hosts().is_empty());
    }
}
