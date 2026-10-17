//! 暴露给前端的命令
use crate::classify::PingResult;
use crate::cli::{self, CliError, ErrorKind};
use crate::monitor::{Monitor, Snapshot};
use crate::netcheck::{self, Report};
use crate::settings::{Settings, SettingsStore};
use crate::tray::PendingMode;
use tauri::{AppHandle, State};

/// 取最近一次快照；还没有时立即读取一次
#[tauri::command]
pub async fn get_snapshot(app: AppHandle, monitor: State<'_, Monitor>) -> Result<Snapshot, ()> {
    match monitor.latest() {
        Some(s) => Ok(s),
        None => Ok(monitor.refresh(&app).await),
    }
}

/// 立即刷新并返回新快照
#[tauri::command]
pub async fn refresh(app: AppHandle, monitor: State<'_, Monitor>) -> Result<Snapshot, ()> {
    Ok(monitor.refresh(&app).await)
}

/// 测试到某台设备的实际路径和延迟。“全部测试”由前端并发调用
#[tauri::command]
pub async fn ping(ip: String, monitor: State<'_, Monitor>) -> Result<PingResult, CliError> {
    let regions = monitor.regions().await;
    cli::ping(&ip, &regions, &monitor.local()).await
}

/// 本机网络体检，约 4 秒
#[tauri::command]
pub async fn run_netcheck(monitor: State<'_, Monitor>) -> Result<Report, CliError> {
    let regions = monitor.regions().await;
    let n = cli::netcheck().await?;
    Ok(netcheck::report(&n, &regions, crate::monitor::now_ms()))
}

/// 读取本机设置
#[tauri::command]
pub fn get_settings(store: State<'_, SettingsStore>) -> Settings {
    store.get()
}

/// 手动标记是否在家
#[tauri::command]
pub fn set_at_home(
    at_home: Option<bool>,
    store: State<'_, SettingsStore>,
) -> Result<Settings, String> {
    store.update(|s| s.at_home = at_home)
}

/// 取走托盘发起的模式切换请求（没有时返回 None）
#[tauri::command]
pub fn take_mode_request(pending: State<'_, PendingMode>) -> Option<bool> {
    pending.take()
}

/// 切换在家 / 出门模式，切换后重新读取设置确认已生效
#[tauri::command]
pub async fn set_accept_routes(
    on: bool,
    app: AppHandle,
    monitor: State<'_, Monitor>,
) -> Result<Snapshot, CliError> {
    cli::set_accept_routes(on).await?;
    let snapshot = monitor.refresh(&app).await;
    let actual = snapshot.overview.as_ref().and_then(|o| o.accept_routes);
    if actual != Some(on) {
        return Err(CliError {
            kind: ErrorKind::Failed,
            message: "已执行切换，但重新读取到的设置没有变化，请稍后刷新再看".into(),
            raw: format!("期望 accept-routes={on}，读取到 {actual:?}"),
        });
    }
    Ok(snapshot)
}
