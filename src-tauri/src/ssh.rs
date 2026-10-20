//! 经 ssh 在其他设备上执行命令：别名校验和带超时的非交互执行，系统信息、官方客户端设置等共用
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;

/// 不弹出控制台窗口
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 别名只允许常见字符，避免被终端当成命令分隔符或选项
pub fn valid_alias(alias: &str) -> bool {
    !alias.is_empty()
        && !alias.starts_with('-')
        && alias
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
}

/// 经 ssh <别名> 在远端执行一条命令，返回标准输出。
/// BatchMode：需要密码或确认主机指纹时直接失败，不会卡住等输入；ssh 自己失败（退出码 255）时报错，
/// 远端命令的退出码不算失败，由调用方按输出判断
pub async fn output(alias: &str, remote: &str, timeout: Duration) -> Result<String, String> {
    if !valid_alias(alias) {
        return Err(format!("SSH 别名含有不支持的字符：{alias}"));
    }
    let mut cmd = Command::new("ssh");
    cmd.args([
        "-T",
        "-o",
        "BatchMode=yes",
        "-o",
        "ConnectTimeout=5",
        alias,
        remote,
    ])
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let out = match tokio::time::timeout(timeout, cmd.output()).await {
        Err(_) => return Err(format!("ssh {alias} 超过 {} 秒没有返回", timeout.as_secs())),
        Ok(Err(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err("找不到 ssh 命令，请安装 Windows 自带的 OpenSSH 客户端".into());
        }
        Ok(Err(e)) => return Err(format!("执行 ssh 失败：{e}")),
        Ok(Ok(out)) => out,
    };
    // 退出码 255 是 ssh 自己失败（连不上、认证失败），其余是远端命令的退出码，不影响已采到的内容
    if out.status.code() == Some(255) {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("ssh {alias} 连接失败：{}", err.trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 别名校验() {
        assert!(valid_alias("peng-work"));
        assert!(!valid_alias("-oProxyCommand=x"));
        assert!(!valid_alias("a;calc"));
    }
}
