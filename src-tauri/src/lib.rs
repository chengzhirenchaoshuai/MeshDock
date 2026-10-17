mod classify;
// M6 起才用到 prefs、set 等接口
#[allow(dead_code)]
mod cli;
mod commands;
#[allow(dead_code)]
mod model;
mod monitor;
mod netcheck;
mod tray;
mod window;

use monitor::Monitor;
use tauri::{Manager, RunEvent, WindowEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // 单实例插件必须最先注册：再次打开程序时只唤出已有窗口
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            window::show(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let monitor = Monitor::new();
            app.manage(monitor.clone());
            tray::setup(app.handle())?;
            monitor.start(app.handle().clone());
            window::show(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::refresh,
            commands::ping,
            commands::run_netcheck,
        ])
        // 点击关闭按钮时只隐藏到托盘，真正退出走托盘菜单
        .on_window_event(|win, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                window::hide(win);
            }
        })
        .build(tauri::generate_context!())
        .expect("启动 MeshDock 失败")
        .run(|_app, event| {
            // 主窗口被销毁时不退出程序；只有托盘“退出”（app.exit 带退出码）才真正退出
            if let RunEvent::ExitRequested {
                code: None, api, ..
            } = event
            {
                api.prevent_exit();
            }
        });
}
