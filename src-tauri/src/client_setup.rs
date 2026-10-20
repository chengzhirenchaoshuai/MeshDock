//! 官方客户端的“无人值守”和“托盘开机自启”：本机直接执行 PowerShell，其他 Windows 设备经 ssh 执行。
//!
//! 托盘自启是“所有用户”启动文件夹里的 Tailscale.lnk，开关方式与任务管理器“启动”页相同：
//! 写 HKLM\…\StartupApproved\StartupFolder 里同名的值，首字节为偶数是启用、奇数是禁用，快捷方式本身保留。
//! 写 HKLM 需要管理员权限：本机经 UAC 提权执行，远端要求 ssh 登录的账户是管理员。
use serde::{Deserialize, Serialize};
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;

/// 本机读取的超时（启动 PowerShell 加读设置）
const LOCAL_TIMEOUT: Duration = Duration::from_secs(20);
/// 本机提权修改的超时：包含用户在 UAC 弹窗上考虑的时间
const ELEVATED_TIMEOUT: Duration = Duration::from_secs(120);
/// 经 ssh 执行的超时
const SSH_TIMEOUT: Duration = Duration::from_secs(25);
/// UAC 弹窗被取消时外层脚本的退出码（ERROR_CANCELLED）
const UAC_CANCELLED: i32 = 1223;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 官方客户端的启动方式
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientSetup {
    /// 无人值守（ForceDaemon）：没有用户登录、托盘退出时组网仍保持连接
    pub unattended: bool,
    /// 启动文件夹里有官方托盘的快捷方式
    pub shortcut: bool,
    /// 开机时启动官方托盘
    pub tray_autostart: bool,
    /// 执行脚本的账户是管理员（远端时即 ssh 登录的账户）
    pub admin: bool,
}

/// 读取并按需修改的脚本，只用 ASCII，避免各种代码页下中文乱码。
/// 输出一行 `OK:<json>`，出错时输出 `ERR:<代码> <详情>` 并以非 0 退出
const SCRIPT: &str = r#"
try { [Console]::OutputEncoding = [Text.Encoding]::UTF8 } catch {}
$U = __U__
$T = __T__
$ts = Join-Path $env:ProgramFiles 'Tailscale\tailscale.exe'
if (-not (Test-Path $ts)) { 'ERR:NOTS'; exit 2 }
$lnk = Join-Path $env:ProgramData 'Microsoft\Windows\Start Menu\Programs\StartUp\Tailscale.lnk'
$key = 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\StartupFolder'
$admin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if ($null -ne $T) {
  if (-not $admin) { 'ERR:ADMIN'; exit 3 }
  if (-not (Test-Path $lnk)) { 'ERR:NOLNK'; exit 4 }
  if ($T) { $v = [byte[]](2,0,0,0,0,0,0,0,0,0,0,0) }
  else { $v = [byte[]](@(3,0,0,0) + [BitConverter]::GetBytes([DateTime]::Now.ToFileTime())) }
  try {
    if (-not (Test-Path $key)) { New-Item -Path $key -Force -ErrorAction Stop | Out-Null }
    Set-ItemProperty -Path $key -Name 'Tailscale.lnk' -Value $v -Type Binary -ErrorAction Stop
  } catch { 'ERR:REG ' + $_.Exception.Message; exit 6 }
}
if ($null -ne $U) {
  $out = & $ts set "--unattended=$(([string]$U).ToLower())" 2>&1
  if ($LASTEXITCODE -ne 0) { 'ERR:SET ' + ($out -join ' '); exit 5 }
}
$raw = & $ts debug prefs 2>$null
if ($LASTEXITCODE -ne 0) { 'ERR:PREFS'; exit 7 }
$prefs = ($raw -join "`n") | ConvertFrom-Json
$a = (Get-ItemProperty -Path $key -ErrorAction SilentlyContinue).'Tailscale.lnk'
$shortcut = Test-Path $lnk
$on = $shortcut -and (($null -eq $a) -or (($a[0] -band 1) -eq 0))
'OK:' + (@{ unattended = [bool]$prefs.ForceDaemon; shortcut = $shortcut; trayAutostart = $on; admin = $admin } | ConvertTo-Json -Compress)
"#;

/// 生成脚本：unattended / tray 为 None 表示不改，只读取
fn script(unattended: Option<bool>, tray: Option<bool>) -> String {
    let ps = |v: Option<bool>| match v {
        None => "$null",
        Some(true) => "$true",
        Some(false) => "$false",
    };
    SCRIPT
        .replace("__U__", ps(unattended))
        .replace("__T__", ps(tray))
}

/// PowerShell -EncodedCommand 的参数：UTF-16LE 再 Base64，不受引号和代码页影响
fn encode(script: &str) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, &b)| n | (b as u32) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(TABLE[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// 错误代码 → 中文原因；where_ 是“本机”或“对方”
fn explain(code: &str, detail: &str, where_: &str) -> String {
    match code {
        "NOTS" => format!("{where_}没有在默认位置安装 Tailscale 官方客户端"),
        "ADMIN" => "修改托盘自启需要管理员权限，SSH 登录的账户不是管理员".into(),
        "NOLNK" => format!(
            "{where_}启动文件夹里没有官方托盘的快捷方式（Tailscale.lnk），可能被删掉了，重新安装官方客户端可恢复"
        ),
        "REG" => format!("写入启动项失败：{detail}"),
        "SET" => format!("设置无人值守失败：{detail}"),
        "PREFS" => format!("读取{where_}官方客户端的设置失败，Tailscale 服务可能没有运行"),
        _ => format!("执行失败：{code} {detail}"),
    }
}

/// 解析脚本输出
fn parse(stdout: &str, where_: &str) -> Result<ClientSetup, String> {
    for line in stdout.lines().map(str::trim) {
        if let Some(json) = line.strip_prefix("OK:") {
            return serde_json::from_str(json).map_err(|e| format!("解析结果失败：{e}"));
        }
        if let Some(err) = line.strip_prefix("ERR:") {
            let (code, detail) = err.split_once(' ').unwrap_or((err, ""));
            return Err(explain(code, detail.trim(), where_));
        }
    }
    let text = stdout.trim();
    Err(if text.is_empty() {
        "没有返回结果，对方可能不是 Windows，或 SSH 默认的命令行不是 cmd / PowerShell".into()
    } else {
        format!("返回了无法识别的结果：{}", text.chars().take(200).collect::<String>())
    })
}

/// 在本机启动 PowerShell 执行 -EncodedCommand，返回（退出码，标准输出）
async fn powershell(encoded: &str, timeout: Duration) -> Result<(Option<i32>, String), String> {
    let mut cmd = Command::new("powershell.exe");
    cmd.args(["-NoProfile", "-NonInteractive", "-EncodedCommand", encoded])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let out = match tokio::time::timeout(timeout, cmd.output()).await {
        Err(_) => return Err(format!("PowerShell 超过 {} 秒没有返回", timeout.as_secs())),
        Ok(Err(e)) => return Err(format!("启动 PowerShell 失败：{e}")),
        Ok(Ok(out)) => out,
    };
    Ok((out.status.code(), String::from_utf8_lossy(&out.stdout).into_owned()))
}

/// 读取：alias 为 None 时读本机，否则经 ssh 读对方
pub async fn get(alias: Option<&str>) -> Result<ClientSetup, String> {
    let encoded = encode(&script(None, None));
    match alias {
        None => parse(&powershell(&encoded, LOCAL_TIMEOUT).await?.1, "本机"),
        Some(a) => remote(a, &encoded).await,
    }
}

/// 经 ssh 在对方执行；远端默认命令行是 cmd 或 PowerShell 都能直接运行这条命令
async fn remote(alias: &str, encoded: &str) -> Result<ClientSetup, String> {
    let command = format!("powershell -NoProfile -NonInteractive -EncodedCommand {encoded}");
    let stdout = crate::sysinfo::ssh_output(alias, &command, SSH_TIMEOUT).await?;
    parse(&stdout, "对方")
}

/// 修改对方的无人值守 / 托盘自启（None 表示不改），返回修改后的状态
pub async fn set_remote(
    alias: &str,
    unattended: Option<bool>,
    tray: Option<bool>,
) -> Result<ClientSetup, String> {
    remote(alias, &encode(&script(unattended, tray))).await
}

/// 修改本机的托盘自启：经 UAC 提权执行（会弹出系统的确认框），完成后重新读取
pub async fn set_local_tray(on: bool) -> Result<ClientSetup, String> {
    let inner = encode(&script(None, Some(on)));
    // 外层脚本只负责提权启动并等待；提权后的窗口隐藏，输出拿不到，按退出码判断
    let outer = format!(
        "try {{ $p = Start-Process -FilePath powershell.exe -Verb RunAs -Wait -PassThru -WindowStyle Hidden \
         -ArgumentList '-NoProfile','-NonInteractive','-EncodedCommand','{inner}' -ErrorAction Stop; exit $p.ExitCode }} \
         catch {{ exit {UAC_CANCELLED} }}"
    );
    let (code, _) = powershell(&encode(&outer), ELEVATED_TIMEOUT).await?;
    match code {
        Some(0) => get(None).await,
        Some(UAC_CANCELLED) => Err("已取消：没有在系统弹出的确认框里允许".into()),
        Some(2) => Err(explain("NOTS", "", "本机")),
        Some(4) => Err(explain("NOLNK", "", "本机")),
        Some(c) => Err(format!("修改失败（退出码 {c}），可以在任务管理器的“启动”页手动修改")),
        None => Err("修改被中断".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 编码与_powershell_一致() {
        // [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes('dir')) 的结果
        assert_eq!(encode("dir"), "ZABpAHIA");
        // 长度不是 3 的倍数时补等号
        assert_eq!(encode("ab"), "YQBiAA==");
        assert_eq!(encode("中"), "LU4=");
    }

    #[test]
    fn 脚本只含_ascii_并替换参数() {
        let s = script(Some(true), None);
        assert!(s.is_ascii());
        assert!(s.contains("$U = $true"));
        assert!(s.contains("$T = $null"));
        assert!(!s.contains("__"));
        // 编码后要能放进 Windows 命令行（cmd 上限 8191 个字符）
        assert!(encode(&script(Some(false), Some(false))).len() < 7000);
    }

    #[test]
    fn 解析成功结果() {
        let out = "\r\nOK:{\"trayAutostart\":false,\"admin\":true,\"shortcut\":true,\"unattended\":true}\r\n";
        assert_eq!(
            parse(out, "对方").unwrap(),
            ClientSetup {
                unattended: true,
                shortcut: true,
                tray_autostart: false,
                admin: true
            }
        );
    }

    #[test]
    fn 解析错误代码() {
        assert!(parse("ERR:ADMIN\n", "对方").unwrap_err().contains("管理员"));
        assert!(parse("ERR:SET access denied\n", "对方").unwrap_err().contains("access denied"));
        assert!(parse("", "对方").unwrap_err().contains("不是 Windows"));
    }

    /// 实机读取本机（只读）：cargo test -- --ignored 实机_读取本机
    #[tokio::test]
    #[ignore]
    async fn 实机_读取本机() {
        let s = get(None).await.unwrap();
        println!("{s:?}");
    }

    /// 实机经 ssh 读取对方（只读）：设置 MESHDOCK_TEST_ALIAS=别名 后 cargo test -- --ignored 实机_读取对方
    #[tokio::test]
    #[ignore]
    async fn 实机_读取对方() {
        let alias = std::env::var("MESHDOCK_TEST_ALIAS").expect("需要设置 MESHDOCK_TEST_ALIAS");
        let s = get(Some(&alias)).await.unwrap();
        println!("{s:?}");
    }

    /// 实机经 ssh 把对方的两项写成它现在的值（走完整的修改流程，但不改变状态）：
    /// 设置 MESHDOCK_TEST_ALIAS=别名 后 cargo test -- --ignored 实机_对方写回原值
    #[tokio::test]
    #[ignore]
    async fn 实机_对方写回原值() {
        let alias = std::env::var("MESHDOCK_TEST_ALIAS").expect("需要设置 MESHDOCK_TEST_ALIAS");
        let before = get(Some(&alias)).await.unwrap();
        let after = set_remote(&alias, Some(before.unattended), Some(before.tray_autostart))
            .await
            .unwrap();
        println!("{after:?}");
        assert_eq!(before, after);
    }
}
