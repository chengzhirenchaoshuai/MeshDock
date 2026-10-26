//! MeshDock 自身的新版本检查与自动更新：用 Tauri 官方更新插件读取公开仓库最新发布里的 latest.json。
//! 安装包打包时用私钥签名，程序里只放公钥（tauri.conf.json），签名校验通过才会安装。
//! 安装版一键下载并运行安装程序；免安装版下载签名过的新 exe，替换自己后重新启动。
//! 发布里没有免安装版下载项时（0.6.0 及更早），免安装版只提示新版本并打开下载页
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Url};
use tauri_plugin_updater::{Updater, UpdaterExt};

/// 发布 MeshDock 的仓库
const REPO: &str = "chengzhirenchaoshuai/MeshDock";
/// 检查和下载的超时（含联网）。安装包约 5 MB，按慢速网络留足时间
const TIMEOUT: Duration = Duration::from_secs(120);
/// 下载进度推给前端的事件
pub const PROGRESS_EVENT: &str = "app-update-progress";
/// latest.json 里免安装版的平台键（安装版用插件默认的 windows-x86_64）
const PORTABLE_TARGET: &str = "windows-x86_64-portable";
/// 免安装版换成新文件后，新程序带这个参数启动：先等旧程序（这个进程号）退出，
/// 否则单实例插件会以为程序已在运行，让新程序直接退出
const WAIT_ARG: &str = "--wait-exit=";
/// 新程序最多等旧程序多久
const WAIT_TIMEOUT: Duration = Duration::from_secs(10);

/// 有新版本时的信息
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdate {
    /// 新版本号，不带 v，如 0.6.0
    pub version: String,
    /// 发布页地址
    pub url: String,
    /// 更新内容（latest.json 里的 notes），可能没有
    pub notes: Option<String>,
    /// 能否一键更新：安装版总能；免安装版要发布里有免安装版的下载项
    pub installable: bool,
}

/// 检查结果，显示在设置页“关于”里
#[derive(Serialize, Clone, Debug, PartialEq, Default)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum UpdateState {
    /// 还没查过
    #[default]
    Unknown,
    /// 已是最新版
    Latest,
    /// 有新版本
    Available(AppUpdate),
    /// 查不到（断网、GitHub 访问不了等）
    Unavailable { reason: String },
}

/// 下载进度
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Progress {
    downloaded: u64,
    /// 安装包总大小；服务器没给出时为 None
    total: Option<u64>,
}

/// 某个版本的发布页
fn release_url(version: &str) -> String {
    format!("https://github.com/{REPO}/releases/tag/v{version}")
}

/// 是不是安装版：NSIS 安装程序会在程序旁放 uninstall.exe，免安装版没有
pub fn installed() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.join("uninstall.exe").is_file()))
        .unwrap_or(false)
}

fn updater(app: &AppHandle) -> Result<Updater, String> {
    let handle = app.clone();
    #[allow(unused_mut)]
    let mut builder = app.updater_builder();
    // 仅开发版：测试时用 MESHDOCK_DEBUG_UPDATE_URL 指向本机的 latest.json
    // （http 地址还需在 --config 里打开 plugins.updater.dangerousInsecureTransportProtocol）
    #[cfg(debug_assertions)]
    if let Ok(url) = std::env::var("MESHDOCK_DEBUG_UPDATE_URL") {
        let url = Url::parse(&url).map_err(|e| format!("测试更新地址不对：{e}"))?;
        builder = builder
            .endpoints(vec![url])
            .map_err(|e| format!("测试更新地址不对：{e}"))?;
    }
    builder
        .timeout(TIMEOUT)
        // Windows 上开始安装时程序会被直接结束，先移除托盘图标，免得任务栏留下残影
        .on_before_exit(move || {
            crate::applog::info("开始安装新版本，程序退出");
            crate::tray::remove(&handle);
        })
        .build()
        .map_err(|e| format!("初始化更新检查失败：{e}"))
}

/// 查询最新正式版。Ok(None) 表示已是最新；断网等查不到时返回 Err（不提示，稍后重试）
pub async fn check(app: &AppHandle) -> Result<Option<AppUpdate>, String> {
    let update = updater(app)?
        .check()
        .await
        .map_err(|e| format!("检查新版本失败：{e}"))?;
    Ok(update.map(|u| AppUpdate {
        url: release_url(&u.version),
        notes: u.body.clone().filter(|n| !n.trim().is_empty()),
        installable: installed() || portable_entry(&u.raw_json).is_some(),
        version: u.version,
    }))
}

/// latest.json 里免安装版的下载地址和签名；没有这一项（旧发布）或内容不对时为 None
fn portable_entry(raw: &serde_json::Value) -> Option<(Url, String)> {
    let p = raw.get("platforms")?.get(PORTABLE_TARGET)?;
    let url = Url::parse(p.get("url")?.as_str()?).ok()?;
    let signature = p.get("signature")?.as_str()?.trim();
    (!signature.is_empty()).then(|| (url, signature.to_string()))
}

/// 下载并安装新版本：下载时推送进度，签名校验通过才继续。
/// 安装版运行安装程序，程序随之退出，装好后自动重新打开；免安装版替换程序文件后重新启动
pub async fn install(app: &AppHandle) -> Result<(), String> {
    let Some(mut update) = updater(app)?
        .check()
        .await
        .map_err(|e| format!("检查新版本失败：{e}"))?
    else {
        return Err("已是最新版".into());
    };
    crate::applog::info(format!("开始下载新版本 {}", update.version));
    let mut downloaded = 0u64;
    let on_chunk = |chunk: usize, total: Option<u64>| {
        downloaded += chunk as u64;
        let _ = app.emit(PROGRESS_EVENT, Progress { downloaded, total });
    };
    let on_finish = || crate::applog::info("新版本下载完成");
    if installed() {
        return update
            .download_and_install(on_chunk, on_finish)
            .await
            .map_err(|e| format!("更新失败：{e}"));
    }
    let Some((url, signature)) = portable_entry(&update.raw_json) else {
        return Err("这个版本没有提供免安装版的自动更新，请打开下载页下载新版本".into());
    };
    (update.download_url, update.signature) = (url, signature);
    // download 内部用公钥校验签名（并核对签名里的版本号），不通过就返回错误
    let bytes = update
        .download(on_chunk, on_finish)
        .await
        .map_err(|e| format!("更新失败：{e}"))?;
    let exe = std::env::current_exe().map_err(|e| format!("找不到程序文件：{e}"))?;
    replace_exe(&exe, &bytes)?;
    crate::applog::info(format!("免安装版已替换为 {}，重新启动", update.version));
    restart(app, &exe, &update.version)
}

/// 程序文件旁的同名文件加后缀，如 MeshDock.exe.old
fn sibling(exe: &Path, suffix: &str) -> PathBuf {
    let mut name = exe.as_os_str().to_owned();
    name.push(format!(".{suffix}"));
    PathBuf::from(name)
}

/// 用新程序替换正在运行的程序文件。Windows 不能覆盖运行中的 exe，但允许改名，
/// 所以先写出 .new，再把旧文件改名为 .old、把 .new 改成原名（文件名不变，开机自启和快捷方式照常可用）；
/// 中途失败就还原。.old 由新程序启动后删除
fn replace_exe(exe: &Path, bytes: &[u8]) -> Result<(), String> {
    let new = sibling(exe, "new");
    let old = sibling(exe, "old");
    std::fs::write(&new, bytes).map_err(|e| {
        let _ = std::fs::remove_file(&new);
        format!("程序所在的文件夹不能写入（{e}），请打开下载页手动下载替换")
    })?;
    let _ = std::fs::remove_file(&old);
    if let Err(e) = std::fs::rename(exe, &old) {
        let _ = std::fs::remove_file(&new);
        return Err(format!("替换程序文件失败：{e}"));
    }
    if let Err(e) = std::fs::rename(&new, exe) {
        let _ = std::fs::rename(&old, exe);
        let _ = std::fs::remove_file(&new);
        return Err(format!("替换程序文件失败：{e}"));
    }
    Ok(())
}

/// 启动新程序并退出自己；新程序会等这个进程退出后再继续启动
fn restart(app: &AppHandle, exe: &Path, version: &str) -> Result<(), String> {
    std::process::Command::new(exe)
        .arg(format!("{WAIT_ARG}{}", std::process::id()))
        .spawn()
        .map_err(|e| format!("已更新到 {version}，但重新启动失败（{e}），请手动重新打开 MeshDock"))?;
    crate::tray::remove(app);
    app.exit(0);
    Ok(())
}

/// 启动参数里要等待退出的旧程序进程号
fn wait_pid(mut args: impl Iterator<Item = String>) -> Option<u32> {
    args.find_map(|a| a.strip_prefix(WAIT_ARG)?.parse().ok())
}

/// 程序启动时最先调用（在单实例插件之前）：免安装版更新后先等旧程序退出，再删掉留下的旧程序文件
pub fn after_portable_update() {
    if let Some(pid) = wait_pid(std::env::args()) {
        wait_exit(pid, WAIT_TIMEOUT);
    }
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let old = sibling(&exe, "old");
    // 旧程序刚退出时文件可能还被占用一小会儿，稍等重试
    for _ in 0..10 {
        if !old.exists() || std::fs::remove_file(&old).is_ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// 等指定进程退出，最多等 timeout；进程已不存在时立即返回
#[cfg(windows)]
fn wait_exit(pid: u32, timeout: Duration) {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
    };
    // SAFETY: 打开失败（返回空句柄）时直接返回；打开成功的句柄用完即关闭
    unsafe {
        let handle = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
        if handle.is_null() {
            return;
        }
        WaitForSingleObject(handle, timeout.as_millis() as u32);
        CloseHandle(handle);
    }
}

#[cfg(not(windows))]
fn wait_exit(_pid: u32, _timeout: Duration) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 发布页地址() {
        assert_eq!(
            release_url("0.6.0"),
            "https://github.com/chengzhirenchaoshuai/MeshDock/releases/tag/v0.6.0"
        );
    }

    #[test]
    fn 读取免安装版下载项() {
        let raw = serde_json::json!({
            "version": "0.6.1",
            "platforms": {
                "windows-x86_64": { "url": "https://example.com/setup.exe", "signature": "aaa" },
                "windows-x86_64-portable": { "url": "https://example.com/portable.exe", "signature": "bbb\n" }
            }
        });
        let (url, sig) = portable_entry(&raw).unwrap();
        assert_eq!(url.as_str(), "https://example.com/portable.exe");
        assert_eq!(sig, "bbb");
    }

    #[test]
    fn 旧发布没有免安装版下载项() {
        let raw = serde_json::json!({
            "platforms": { "windows-x86_64": { "url": "https://example.com/setup.exe", "signature": "aaa" } }
        });
        assert!(portable_entry(&raw).is_none());
        let empty_sig = serde_json::json!({
            "platforms": { "windows-x86_64-portable": { "url": "https://example.com/p.exe", "signature": " " } }
        });
        assert!(portable_entry(&empty_sig).is_none());
    }

    #[test]
    fn 读取等待的进程号() {
        let args = ["meshdock.exe", "--autostart", "--wait-exit=1234"].map(String::from);
        assert_eq!(wait_pid(args.into_iter()), Some(1234));
        assert_eq!(wait_pid(["meshdock.exe".to_string()].into_iter()), None);
        assert_eq!(wait_pid(["--wait-exit=abc".to_string()].into_iter()), None);
    }

    /// 每个测试用自己的临时目录
    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("meshdock-update-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn 替换程序文件() {
        let dir = temp_dir("ok");
        let exe = dir.join("MeshDock.exe");
        std::fs::write(&exe, b"old").unwrap();
        // 上次更新没删掉的 .old 不影响这次
        std::fs::write(sibling(&exe, "old"), b"older").unwrap();
        replace_exe(&exe, b"new").unwrap();
        assert_eq!(std::fs::read(&exe).unwrap(), b"new");
        assert_eq!(std::fs::read(sibling(&exe, "old")).unwrap(), b"old");
        assert!(!sibling(&exe, "new").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn 替换失败时不留下新文件() {
        let dir = temp_dir("fail");
        // 程序文件不存在，改名失败
        let exe = dir.join("MeshDock.exe");
        assert!(replace_exe(&exe, b"new").is_err());
        assert!(!sibling(&exe, "new").exists());
        assert!(!exe.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn 测试程序不是安装版() {
        // cargo test 的可执行文件旁没有 uninstall.exe
        assert!(!installed());
    }
}
