//! 调用官方 tailscale.exe：统一超时、隐藏黑框、把失败归类成带中文说明的错误
use crate::classify::{LocalNet, PingResult, Regions, parse_ping};
use crate::model::{
    DerpMap, NetMap, Netcheck, Prefs, Status, VersionInfo, parse_netcheck, parse_netmap,
};
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
    run_timeout(args, TIMEOUT).await
}

/// 同 run，但可指定超时（发送文件等耗时操作用）
async fn run_timeout(args: &[&str], timeout: Duration) -> Result<Output, CliError> {
    let exe = exe_path().ok_or_else(|| CliError::new(ErrorKind::NotInstalled, ""))?;
    let mut cmd = Command::new(&exe);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true); // 超时后丢弃时结束子进程
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let out = match tokio::time::timeout(timeout, cmd.output()).await {
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

/// 本机版本和官方最新版本（需要访问官方服务器，约 2 秒）
pub async fn version_upstream() -> Result<VersionInfo, CliError> {
    run_json(&["version", "--upstream", "--json"]).await
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

/// `tailscale up` 因为会改掉非默认设置而拒绝执行时，从提示中取出它给出的完整命令的参数。
/// 提示格式（tailscale.exe 1.102.4 原文）：
/// "Error: changing settings via 'tailscale up' requires mentioning all\nnon-default flags. ...\n\n\ttailscale up --flag=... ..."
/// 只接受全部以 `--` 开头的参数，避免执行意外内容
pub fn suggested_up_args(text: &str) -> Option<Vec<String>> {
    if !text.contains("requires mentioning all") {
        return None;
    }
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("tailscale up"))?;
    let args = split_args(line.strip_prefix("tailscale up")?)?;
    args.iter().all(|a| a.starts_with("--")).then_some(args)
}

/// 按空白拆分参数，支持单引号和双引号包住的值；引号不成对时返回 None
fn split_args(text: &str) -> Option<Vec<String>> {
    let mut args = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut has_token = false;
    for c in text.chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => cur.push(c),
            None if c == '"' || c == '\'' => {
                quote = Some(c);
                has_token = true;
            }
            None if c.is_whitespace() => {
                if has_token {
                    args.push(std::mem::take(&mut cur));
                    has_token = false;
                }
            }
            None => {
                cur.push(c);
                has_token = true;
            }
        }
    }
    if quote.is_some() {
        return None;
    }
    if has_token {
        args.push(cur);
    }
    Some(args)
}

/// 需要重新登录时的提示（tailscale.exe 1.102.4 原文为 "To authenticate, visit:"）
fn needs_login(text: &str) -> bool {
    text.contains("To authenticate, visit")
}

fn login_error(raw: String) -> CliError {
    CliError {
        kind: ErrorKind::Failed,
        message: "需要重新登录：请打开 Tailscale 官方客户端完成登录".into(),
        raw,
    }
}

/// 重试用的参数：去掉建议命令里已有的 --timeout（重复会报 "flag provided multiple times"），再统一追加
fn retry_up_args(suggested: &[String]) -> Vec<String> {
    let mut args: Vec<String> = suggested
        .iter()
        .filter(|a| !a.starts_with("--timeout"))
        .cloned()
        .collect();
    args.push("--timeout=8s".into());
    args
}

/// 连接组网。按官方说明，不带任何参数的 `tailscale up` 只让组网上线、不改设置；
/// 一旦带了参数（哪怕只是 --timeout）就会被当成“完整设置”，触发非默认设置检查，所以第一步必须不带参数。
/// 若仍被拒绝，按它给出的完整命令重新执行
pub async fn up() -> Result<(), CliError> {
    let out = match run(&["up"]).await {
        Ok(out) => out,
        // 不带参数时遇到需要登录会一直等待，只能靠 10 秒硬超时结束
        Err(e) if e.kind == ErrorKind::Timeout => {
            return Err(CliError {
                kind: ErrorKind::Timeout,
                message:
                    "连接超过 10 秒没有完成，可能需要重新登录：请打开 Tailscale 官方客户端查看"
                        .into(),
                raw: e.raw,
            });
        }
        Err(e) => return Err(e),
    };
    if out.success {
        return Ok(());
    }
    let text = out.text();
    if needs_login(&text) {
        return Err(login_error(text));
    }
    let Some(args) = suggested_up_args(&text) else {
        return Err(CliError::new(ErrorKind::Failed, text));
    };
    let args = retry_up_args(&args);
    let mut full: Vec<&str> = vec!["up"];
    full.extend(args.iter().map(String::as_str));
    let out = run(&full).await?;
    if out.success {
        Ok(())
    } else if needs_login(&out.text()) {
        Err(login_error(out.text()))
    } else {
        Err(CliError::new(ErrorKind::Failed, out.text()))
    }
}

/// 断开组网；本来就已断开也算成功
pub async fn down() -> Result<(), CliError> {
    let out = run(&["down"]).await?;
    if out.success || out.text().contains("already stopped") {
        Ok(())
    } else {
        Err(CliError::new(ErrorKind::Failed, out.text()))
    }
}

/// 在家 / 出门模式：accept-routes 开关
pub async fn set_accept_routes(on: bool) -> Result<(), CliError> {
    let flag = format!("--accept-routes={on}");
    run_checked(&["set", &flag]).await.map(|_| ())
}

/// 选用出口设备（传组网 IP），None 表示停用出口。
/// 只接受合法 IP，避免把以 `-` 开头的文本当成命令选项
pub async fn set_exit_node(ip: Option<&str>) -> Result<(), CliError> {
    let value = match ip {
        Some(ip) => ip
            .parse::<std::net::IpAddr>()
            .map_err(|_| CliError {
                kind: ErrorKind::Failed,
                message: "IP 地址格式不正确".into(),
                raw: ip.into(),
            })?
            .to_string(),
        None => String::new(),
    };
    let flag = format!("--exit-node={value}");
    run_checked(&["set", &flag]).await.map(|_| ())
}

/// 已登录的一个账户（`tailscale switch --list --json` 的一项）
#[derive(serde::Deserialize, Serialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Profile {
    pub id: String,
    pub nickname: String,
    /// 组网名；Headscale 下是控制端地址
    pub tailnet: String,
    pub account: String,
    pub selected: bool,
}

pub async fn profiles() -> Result<Vec<Profile>, CliError> {
    run_json(&["switch", "--list", "--json"]).await
}

/// 切换账户的超时：要断开当前组网再连上另一个
const SWITCH_TIMEOUT: Duration = Duration::from_secs(30);

/// 切换到另一个已登录的账户。id 只接受字母和数字（实机形如 "aa49"），避免被当成选项
pub async fn switch_profile(id: &str) -> Result<(), CliError> {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(CliError {
            kind: ErrorKind::Failed,
            message: "账户编号格式不正确".into(),
            raw: id.into(),
        });
    }
    let out = run_timeout(&["switch", id], SWITCH_TIMEOUT).await?;
    if out.success {
        Ok(())
    } else {
        Err(CliError::new(ErrorKind::Failed, out.text()))
    }
}

/// 共享本机服务（tailscale serve）的方式。HTTPS 需要控制端签发证书，Headscale 不支持，不提供
#[derive(serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ServeKind {
    Http,
    Tcp,
}

impl ServeKind {
    fn flag(self, port: u16) -> String {
        match self {
            Self::Http => format!("--http={port}"),
            Self::Tcp => format!("--tcp={port}"),
        }
    }

    /// 本机目标地址，只转发到 127.0.0.1
    fn target(self, local_port: u16) -> String {
        match self {
            Self::Http => format!("http://127.0.0.1:{local_port}"),
            Self::Tcp => format!("tcp://127.0.0.1:{local_port}"),
        }
    }
}

/// 当前的共享规则
pub async fn serve_rules() -> Result<Vec<crate::model::ServeRule>, CliError> {
    let s: crate::model::ServeStatus = run_json(&["serve", "status", "--json"]).await?;
    Ok(s.rules())
}

fn check_port(port: u16) -> Result<(), CliError> {
    if port == 0 {
        return Err(CliError {
            kind: ErrorKind::Failed,
            message: "端口要在 1～65535 之间".into(),
            raw: port.to_string(),
        });
    }
    Ok(())
}

/// 在组网的 port 端口共享本机的 local_port（后台运行，重启后仍保留）
pub async fn serve_add(kind: ServeKind, port: u16, local_port: u16) -> Result<(), CliError> {
    check_port(port)?;
    check_port(local_port)?;
    let flag = kind.flag(port);
    let target = kind.target(local_port);
    run_checked(&["serve", "--bg", "--yes", &flag, &target])
        .await
        .map(|_| ())
}

/// 删除一条共享规则。kind 为 "https" 的规则（官方客户端或命令行加的）也能删
pub async fn serve_remove(kind: &str, port: u16) -> Result<(), CliError> {
    check_port(port)?;
    let flag = match kind {
        "http" => format!("--http={port}"),
        "https" => format!("--https={port}"),
        "tcp" => format!("--tcp={port}"),
        _ => {
            return Err(CliError {
                kind: ErrorKind::Failed,
                message: "不认识的共享方式".into(),
                raw: kind.into(),
            });
        }
    };
    run_checked(&["serve", &flag, "off"]).await.map(|_| ())
}

/// 发送文件的超时：大文件经中继可能要很久，只防止永远卡住
const SEND_TIMEOUT: Duration = Duration::from_secs(60 * 60);

/// 用 Taildrop 把文件发给某台设备（`tailscale file cp <文件…> <IP>:`）。
/// 文件必须是绝对路径（不会以 `-` 开头被当成选项），IP 必须合法
pub async fn send_files(files: &[std::path::PathBuf], ip: &str) -> Result<(), CliError> {
    let bad = |raw: String, message: &str| CliError {
        kind: ErrorKind::Failed,
        message: message.into(),
        raw,
    };
    let ip: std::net::IpAddr = ip.parse().map_err(|_| bad(ip.into(), "IP 地址格式不正确"))?;
    if files.is_empty() {
        return Err(bad(String::new(), "没有选择文件"));
    }
    let files: Vec<String> = files
        .iter()
        .map(|f| {
            f.is_absolute()
                .then(|| f.display().to_string())
                .ok_or_else(|| bad(f.display().to_string(), "文件路径不正确"))
        })
        .collect::<Result<_, _>>()?;
    let target = format!("{ip}:");
    let mut args = vec!["file", "cp", "--update-interval=0"];
    args.extend(files.iter().map(String::as_str));
    args.push(&target);
    let out = match run_timeout(&args, SEND_TIMEOUT).await {
        Err(e) if e.kind == ErrorKind::Timeout => {
            return Err(CliError {
                message: "发送超过 1 小时还没完成，已停止".into(),
                ..e
            });
        }
        r => r?,
    };
    if out.success {
        Ok(())
    } else {
        Err(CliError {
            kind: ErrorKind::Failed,
            message: send_failure(&out.text()).into(),
            raw: out.text(),
        })
    }
}

/// 发送失败的中文原因（按 tailscale.exe 的英文提示判断，认不出时给通用说明）
fn send_failure(text: &str) -> &'static str {
    let lower = text.to_lowercase();
    if lower.contains("offline") {
        "对方设备离线，发送失败"
    } else if lower.contains("not a valid target") || lower.contains("no matching peer") {
        "对方不接收文件：通常是对方不属于同一个用户，或控制端没有开放文件传输"
    } else {
        "发送失败，可展开查看原始输出"
    }
}

/// 可在界面上开关的本机组网选项（都用 `tailscale set --<参数>=true/false`）
#[derive(serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PrefSwitch {
    /// 走出口时仍可访问本机所在局域网
    ExitNodeAllowLan,
    AcceptDns,
    ShieldsUp,
    Unattended,
}

impl PrefSwitch {
    pub fn flag(self) -> &'static str {
        match self {
            Self::ExitNodeAllowLan => "--exit-node-allow-lan-access",
            Self::AcceptDns => "--accept-dns",
            Self::ShieldsUp => "--shields-up",
            Self::Unattended => "--unattended",
        }
    }

    /// 日志和提示里用的中文名
    pub fn label(self) -> &'static str {
        match self {
            Self::ExitNodeAllowLan => "走出口时访问本地局域网",
            Self::AcceptDns => "使用组网 DNS",
            Self::ShieldsUp => "挡住传入连接",
            Self::Unattended => "无人登录时保持连接",
        }
    }

    /// 从读到的本机选项里取出这一项的值
    pub fn read(self, p: &crate::model::LocalPrefs) -> Option<bool> {
        match self {
            Self::ExitNodeAllowLan => p.exit_node_allow_lan,
            Self::AcceptDns => p.accept_dns,
            Self::ShieldsUp => p.shields_up,
            Self::Unattended => p.unattended,
        }
    }
}

pub async fn set_switch(switch: PrefSwitch, on: bool) -> Result<(), CliError> {
    let flag = format!("{}={on}", switch.flag());
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
        let e = set_exit_node(Some("--reset")).await.unwrap_err();
        assert_eq!(e.message, "IP 地址格式不正确");
    }

    #[tokio::test]
    async fn 发送文件拒绝非法参数() {
        let abs = [std::path::PathBuf::from(r"C:\a.txt")];
        let e = send_files(&abs, "--help").await.unwrap_err();
        assert_eq!(e.message, "IP 地址格式不正确");
        let rel = [std::path::PathBuf::from("-x")];
        let e = send_files(&rel, "100.64.0.3").await.unwrap_err();
        assert_eq!(e.message, "文件路径不正确");
        let e = send_files(&[], "100.64.0.3").await.unwrap_err();
        assert_eq!(e.message, "没有选择文件");
    }

    #[test]
    fn 共享只转发到本机() {
        assert_eq!(ServeKind::Http.flag(8080), "--http=8080");
        assert_eq!(ServeKind::Tcp.target(22), "tcp://127.0.0.1:22");
        assert!(serde_json::from_str::<ServeKind>(r#""https""#).is_err());
    }

    #[tokio::test]
    async fn 共享拒绝非法参数() {
        let e = serve_add(ServeKind::Http, 0, 80).await.unwrap_err();
        assert_eq!(e.message, "端口要在 1～65535 之间");
        let e = serve_remove("--reset", 80).await.unwrap_err();
        assert_eq!(e.message, "不认识的共享方式");
    }

    #[test]
    fn 解析账户列表() {
        // 2026-10-19 实机输出（控制端地址已替换）
        let list: Vec<Profile> = serde_json::from_str(
            r#"[{"id":"aa49","nickname":"peng","tailnet":"203.0.113.1","account":"peng","selected":true}]"#,
        )
        .unwrap();
        assert_eq!(list.len(), 1);
        assert!(list[0].selected);
        assert_eq!(list[0].id, "aa49");
    }

    #[tokio::test]
    async fn 切换账户拒绝非法编号() {
        for id in ["", "--remove", "a b"] {
            let e = switch_profile(id).await.unwrap_err();
            assert_eq!(e.message, "账户编号格式不正确");
        }
    }

    #[test]
    fn 开关选项按名称反序列化() {
        let s: PrefSwitch = serde_json::from_str(r#""shields-up""#).unwrap();
        assert_eq!(s.flag(), "--shields-up");
        assert!(serde_json::from_str::<PrefSwitch>(r#""reset""#).is_err());
    }

    /// 按 tailscale.exe 1.102.4 中的提示原文构造
    const UP_REFUSED: &str = "Error: changing settings via 'tailscale up' requires mentioning all\n\
non-default flags. To proceed, either re-run your command with --reset or\n\
use the command below to explicitly mention the current value of\n\
all non-default settings:\n\n\
\ttailscale up --accept-dns=false --login-server=https://203.0.113.1:8443 --unattended\n";

    /// 2026-10-17 实机输出（控制端地址已替换）：第一步带了 --timeout，建议命令里也原样带上了它
    const UP_REFUSED_REAL: &str = "Error: changing settings via 'tailscale up' requires mentioning all\n\
non-default flags. To proceed, either re-run your command with --reset or\n\
use the command below to explicitly mention the current value of\n\
all non-default settings:\n\n\
\ttailscale up --timeout=8s --accept-dns=false --accept-routes=false --hostname=lan-desktop --login-server=https://203.0.113.1:8443 --unattended\n";

    #[test]
    fn 重试参数中只有一个_timeout() {
        let args = retry_up_args(&suggested_up_args(UP_REFUSED_REAL).unwrap());
        assert_eq!(
            args.iter().filter(|a| a.starts_with("--timeout")).count(),
            1
        );
        assert_eq!(
            args,
            [
                "--accept-dns=false",
                "--accept-routes=false",
                "--hostname=lan-desktop",
                "--login-server=https://203.0.113.1:8443",
                "--unattended",
                "--timeout=8s",
            ]
        );
    }

    #[test]
    fn 取出_up_建议的完整参数() {
        assert_eq!(
            suggested_up_args(UP_REFUSED).unwrap(),
            [
                "--accept-dns=false",
                "--login-server=https://203.0.113.1:8443",
                "--unattended"
            ]
        );
        assert_eq!(suggested_up_args("Success."), None);
    }

    #[test]
    fn 建议命令中的引号与可疑参数() {
        let quoted = UP_REFUSED.replace("--unattended", "--hostname='my pc'");
        assert_eq!(suggested_up_args(&quoted).unwrap()[2], "--hostname=my pc");
        // 混入不以 -- 开头的内容时拒绝执行
        let bad = UP_REFUSED.replace("--unattended", "--unattended ; calc");
        assert_eq!(suggested_up_args(&bad), None);
        // 引号不成对
        let broken = UP_REFUSED.replace("--unattended", "--hostname='x");
        assert_eq!(suggested_up_args(&broken), None);
    }

    #[test]
    fn 识别需要重新登录() {
        assert!(needs_login(
            "\nTo authenticate, visit:\n\n\thttps://203.0.113.1:8443/register/abc\n"
        ));
        assert!(!needs_login(UP_REFUSED));
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
        assert!(!m.peer_hostinfo().is_empty());
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
