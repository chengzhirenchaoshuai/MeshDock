//! MeshDock 自身的新版本检查与自动更新：用 Tauri 官方更新插件读取公开仓库最新发布里的 latest.json。
//! 安装包打包时用私钥签名，程序里只放公钥（tauri.conf.json），签名校验通过才会安装。
//! 安装版可在设置页一键下载安装；免安装版没有安装程序可替换，只提示新版本并打开下载页
use serde::Serialize;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::{Updater, UpdaterExt};

/// 发布 MeshDock 的仓库
const REPO: &str = "chengzhirenchaoshuai/MeshDock";
/// 检查和下载的超时（含联网）。安装包约 5 MB，按慢速网络留足时间
const TIMEOUT: Duration = Duration::from_secs(120);
/// 下载进度推给前端的事件
pub const PROGRESS_EVENT: &str = "app-update-progress";

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
    app.updater_builder()
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
        version: u.version,
    }))
}

/// 下载并安装新版本（只用于安装版）：下载时推送进度，校验签名后运行安装程序，程序随之退出，装好后自动重新打开
pub async fn install(app: &AppHandle) -> Result<(), String> {
    if !installed() {
        return Err("免安装版不能自动更新，请打开下载页下载新版本".into());
    }
    let Some(update) = updater(app)?
        .check()
        .await
        .map_err(|e| format!("检查新版本失败：{e}"))?
    else {
        return Err("已是最新版".into());
    };
    crate::applog::info(format!("开始下载新版本 {}", update.version));
    let mut downloaded = 0u64;
    update
        .download_and_install(
            |chunk, total| {
                downloaded += chunk as u64;
                let _ = app.emit(PROGRESS_EVENT, Progress { downloaded, total });
            },
            || crate::applog::info("新版本下载完成，签名校验通过"),
        )
        .await
        .map_err(|e| format!("更新失败：{e}"))
}

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
    fn 测试程序不是安装版() {
        // cargo test 的可执行文件旁没有 uninstall.exe
        assert!(!installed());
    }
}
