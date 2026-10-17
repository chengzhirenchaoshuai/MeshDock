//! 暴露给前端的命令
use crate::monitor::{Monitor, Snapshot};
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
