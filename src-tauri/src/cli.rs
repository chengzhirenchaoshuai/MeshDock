//! 调用官方 tailscale.exe：统一超时、隐藏黑框、把失败归类成带中文说明的错误
use crate::classify::{LocalNet, PingResult, Regions, parse_ping};
use crate::model::{DerpMap, NetMap, Netcheck, Prefs, Status, parse_netcheck, parse_netmap};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;

/// 单条命令的硬超时
pub const TIMEOUT: Duration = Duration::from_secs(10);

/// 可用环境变量指定 tailscale.exe 的位置（非默认安装目录或测试时用）
const PATH_ENV: &str = "MESHDOCK_TAILSCALE";

/// 不弹出控制台窗口，避免每次刷新闪一下黑框
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorKind {
    /// 找不到 tailscale.exe
    NotInstalled,
    /// Tailscale 服务没运行
    ServiceDown,
    Timeout,
    /// 权限不足
    AccessDenied,
    /// 其他失败（含输出无法解析）
    Failed,
}

/// 给前端看的错误：中文说明 + 原始输出（可折叠查看）
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CliError {
    pub kind: ErrorKind,
    pub message: String,
    pub raw: String,
}

impl CliError {
    fn new(kind: ErrorKind, raw: impl Into<String>) -> Self {
        let message = match kind {
            ErrorKind::NotInstalled => {
                "没有找到 Tailscale 官方客户端（tailscale.exe），请确认已经安装"
            }
            ErrorKind::ServiceDown => {
                "Tailscale 服务没有运行，请打开官方客户端，或在 Windows“服务”中启动 Tailscale"
            }
            ErrorKind::Timeout => "Tailscale 超过 10 秒没有响应，请稍后再试",
            ErrorKind::AccessDenied => "权限不足，这项操作需要以管理员身份运行",
            ErrorKind::Failed => "Tailscale 命令执行失败",
        };
        Self {
            kind,
            message: message.into(),
            raw: raw.into(),
        }
    }
}

/// 命令执行完毕后的输出
pub struct Output {
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
}

impl Output {
    /// stdout 与 stderr 合并，用于展示原文
    pub fn text(&self) -> String {
        format!("{}\n{}", self.stdout.trim(), self.stderr.trim())
            .trim()
            .to_string()
    }
}

/// 查找 tailscale.exe：环境变量 → 默认安装目录 → PATH。每次都重新查找，装好之后无需重启本程序
pub fn exe_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os(PATH_ENV) {
        return Some(PathBuf::from(p));
    }
    let program_files =
        std::env::var_os("ProgramFiles").unwrap_or_else(|| r"C:\Program Files".into());
    let default = PathBuf::from(program_files).join(r"Tailscale\tailscale.exe");
    if default.is_file() {
        return Some(default);
    }
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join("tailscale.exe"))
        .find(|p| p.is_file())
}

/// 从输出文本识别服务没运行、权限不足（文案取自 tailscale.exe 1.102.4）
fn known_failure(text: &str) -> Option<ErrorKind> {
    let lower = text.to_lowercase();
    if lower.contains("failed to connect to local tailscaled") {
        Some(ErrorKind::ServiceDown)
    } else if lower.contains("access denied") || lower.contains("access is denied") {
        Some(ErrorKind::AccessDenied)
    } else {
        None
    }
}

/// 执行命令。只要进程正常结束就返回输出（不论退出码），由调用方决定是否算失败；
/// 但服务没运行、权限不足这类情况统一当作错误
pub async fn run(args: &[&str]) -> Result<Output, CliError> {
    let exe = exe_path().ok_or_else(|| CliError::new(ErrorKind::NotInstalled, ""))?;
    let mut cmd = Command::new(&exe);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true); // 超时后丢弃时结束子进程
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let out = match tokio::time::timeout(TIMEOUT, cmd.output()).await {
        Err(_) => {
            return Err(CliError::new(
                ErrorKind::Timeout,
                format!("tailscale {}", args.join(" ")),
            ));
        }
        Ok(Err(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(CliError::new(
                ErrorKind::NotInstalled,
                exe.display().to_string(),
            ));
        }
        Ok(Err(e)) => return Err(CliError::new(ErrorKind::Failed, e.to_string())),
        Ok(Ok(out)) => out,
    };
    let output = Output {
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        success: out.status.success(),
    };
    match known_failure(&output.text()) {
        Some(kind) => Err(CliError::new(kind, output.text())),
        None => Ok(output),
    }
}

/// 执行命令，退出码非 0 视为失败
pub async fn run_checked(args: &[&str]) -> Result<Output, CliError> {
    let out = run(args).await?;
    if out.success {
        Ok(out)
    } else {
        Err(CliError::new(ErrorKind::Failed, out.text()))
    }
}

/// 执行命令并把 stdout 解析成 JSON
async fn run_json<T: DeserializeOwned>(args: &[&str]) -> Result<T, CliError> {
    let out = run_checked(args).await?;
    serde_json::from_str(&out.stdout).map_err(|e| {
        CliError::new(
            ErrorKind::Failed,
            format!("无法解析输出：{e}\n{}", out.stdout),
        )
    })
}

pub async fn status() -> Result<Status, CliError> {
    run_json(&["status", "--json"]).await
}

pub async fn prefs() -> Result<Prefs, CliError> {
    run_json(&["debug", "prefs"]).await
}

pub async fn derp_map() -> Result<DerpMap, CliError> {
    run_json(&["debug", "derp-map"]).await
}

/// 各设备上报的网络检查结论（用于解释中继原因）
pub async fn netmap() -> Result<NetMap, CliError> {
    let out = run_checked(&["debug", "netmap"]).await?;
    parse_netmap(&out.stdout).map_err(|e| {
        CliError::new(
            ErrorKind::Failed,
            format!("无法解析输出：{e}\n{}", out.stderr),
        )
    })
}

/// 本机网络体检，约 4 秒
pub async fn netcheck() -> Result<Netcheck, CliError> {
    let out = run_checked(&["netcheck", "--format=json"]).await?;
    parse_netcheck(&out.stdout).map_err(|e| {
        CliError::new(
            ErrorKind::Failed,
            format!("无法解析输出：{e}\n{}", out.text()),
        )
    })
}

/// 测一次路径和延迟；离线设备 3 秒内返回。
/// 只接受合法 IP，避免把以 `-` 开头的文本当成命令选项传给 tailscale
pub async fn ping(ip: &str, regions: &Regions, local: &LocalNet) -> Result<PingResult, CliError> {
    let ip: std::net::IpAddr = ip.parse().map_err(|_| CliError {
        kind: ErrorKind::Failed,
        message: "IP 地址格式不正确".into(),
        raw: ip.into(),
    })?;
    let ip = ip.to_string();
    let out = run(&["ping", "-c", "1", "--timeout", "3s", &ip]).await?;
    Ok(parse_ping(&out.text(), regions, local))
}

/// 在家 / 出门模式：accept-routes 开关
pub async fn set_accept_routes(on: bool) -> Result<(), CliError> {
    let flag = format!("--accept-routes={on}");
    run_checked(&["set", &flag]).await.map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 识别服务没运行() {
        // tailscale.exe 1.102.4 中的原文
        let text =
            "failed to connect to local tailscaled process; is the Tailscale service running?";
        assert_eq!(known_failure(text), Some(ErrorKind::ServiceDown));
    }

    #[tokio::test]
    async fn 拒绝非法_ip() {
        let e = ping("--help", &Regions::default(), &LocalNet::default())
            .await
            .unwrap_err();
        assert_eq!(e.message, "IP 地址格式不正确");
    }

    #[test]
    fn 识别权限不足() {
        assert_eq!(
            known_failure("Access denied: prefs write access denied"),
            Some(ErrorKind::AccessDenied)
        );
        assert_eq!(known_failure("pong from ming-pc"), None);
    }

    /// 以下测试调用本机真实的 tailscale.exe，需手动运行：cargo test -- --ignored
    #[tokio::test]
    #[ignore]
    async fn 实机_读取状态和设置() {
        let s = status().await.unwrap();
        assert!(s.self_node.is_some());
        let p = prefs().await.unwrap();
        assert!(p.route_all.is_some());
        let d = derp_map().await.unwrap();
        assert!(d.regions.is_some());
        let m = netmap().await.unwrap();
        assert!(m.self_net_info().is_some());
        assert!(!m.peer_net_info().is_empty());
    }

    #[tokio::test]
    #[ignore]
    async fn 实机_体检和测速() {
        let n = netcheck().await.unwrap();
        assert!(n.udp.is_some());
        let r = ping("100.64.0.255", &Regions::default(), &LocalNet::default())
            .await
            .unwrap();
        assert!(r.conn.is_none(), "{r:?}");
    }

    #[tokio::test]
    #[ignore]
    async fn 实机_程序路径错误() {
        // SAFETY: 测试单线程设置环境变量，其他实机测试应单独运行
        unsafe { std::env::set_var(PATH_ENV, r"C:\不存在\tailscale.exe") };
        let e = status().await.unwrap_err();
        unsafe { std::env::remove_var(PATH_ENV) };
        assert_eq!(e.kind, ErrorKind::NotInstalled);
    }
}
