//! 主窗口：按需创建；关闭后先隐藏，一段时间内未再打开则销毁窗口和 WebView 以释放内存
use crate::monitor::Monitor;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Manager, WebviewWindowBuilder, Window};

const MAIN: &str = "main";

/// 隐藏后多久销毁 WebView；期间再次打开可瞬间恢复
const DESTROY_AFTER: Duration = Duration::from_secs(30);

/// 每次显示或隐藏都递增，用于作废之前排定的销毁
static HIDE_SEQ: AtomicU64 = AtomicU64::new(0);

/// 显示并激活主窗口；窗口已被销毁时重新创建
pub fn show(app: &AppHandle) {
    HIDE_SEQ.fetch_add(1, Ordering::SeqCst);
    // 隐藏期间刷新较慢，重新显示时补一次
    app.state::<Monitor>().wake_if_stale();
    if let Some(window) = app.get_webview_window(MAIN) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    if let Err(e) = create(app) {
        eprintln!("创建主窗口失败：{e}");
    }
}

/// 按 tauri.conf.json 中的窗口配置创建主窗口
fn create(app: &AppHandle) -> Result<(), String> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == MAIN)
        .cloned()
        .ok_or("缺少主窗口配置")?;
    let window = WebviewWindowBuilder::from_config(app, &config)
        .map_err(|e| e.to_string())?
        .visible(true)
        .build()
        .map_err(|e| e.to_string())?;
    let _ = window.set_focus();
    Ok(())
}

/// 点击关闭按钮：隐藏到托盘，DESTROY_AFTER 后仍未显示则销毁
pub fn hide(window: &Window) {
    let _ = window.hide();
    let seq = HIDE_SEQ.fetch_add(1, Ordering::SeqCst) + 1;
    let app = window.app_handle().clone();
    thread::spawn(move || {
        thread::sleep(DESTROY_AFTER);
        if HIDE_SEQ.load(Ordering::SeqCst) != seq {
            return;
        }
        if let Some(window) = app.get_webview_window(MAIN)
            && !window.is_visible().unwrap_or(true)
        {
            let _ = window.destroy();
        }
    });
}
