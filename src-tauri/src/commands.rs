//! 暴露给前端的命令
use crate::classify::PingResult;
use crate::cli::{self, CliError};
use crate::monitor::{Monitor, Snapshot};
use crate::netcheck::{self, Report};
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
    cli::ping(&ip, &regions).await
}

/// 本机网络体检，约 4 秒
#[tauri::command]
pub async fn run_netcheck(monitor: State<'_, Monitor>) -> Result<Report, CliError> {
    let regions = monitor.regions().await;
    let n = cli::netcheck().await?;
    Ok(netcheck::report(&n, &regions, crate::monitor::now_ms()))
}
