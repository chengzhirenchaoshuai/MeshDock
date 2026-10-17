mod classify;
mod cli;
mod commands;
mod gateway;
mod history;
mod launch;
mod model;
mod monitor;
mod netcheck;
mod notify;
mod paths;
mod report;
mod router;
mod settings;
mod tray;
mod window;

use history::HistoryStore;
use monitor::Monitor;
use paths::AppPaths;
use settings::SettingsStore;
use tauri::{AppHandle, Manager, RunEvent, WindowEvent};
use tauri_plugin_autostart::ManagerExt;
use tray::PendingMode;

/// 开机自启时附带的命令行参数，用于区分“开机启动”和“手动打开”
const AUTOSTART_ARG: &str = "--autostart";

/// 让注册表中的开机自启项与设置保持一致。
/// 开发版不写入，避免把 target\debug 下的程序注册为开机启动
pub(crate) fn sync_autostart(app: &AppHandle, enabled: bool) -> Result<(), String> {
    if cfg!(debug_assertions) {
        return Ok(());
    }
    let launcher = app.autolaunch();
    let result = match (enabled, launcher.is_enabled().unwrap_or(false)) {
        (true, _) => launcher.enable(), // 总是重写一次，免安装版挪了位置后也能更新路径
        (false, true) => launcher.disable(),
        (false, false) => Ok(()),
    };
    result.map_err(|e| format!("设置开机自启失败：{e}"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // 单实例插件必须最先注册：再次打开程序时只唤出已有窗口
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            window::show(app);
        }))
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .app_name("MeshDock")
                .arg(AUTOSTART_ARG)
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let paths = AppPaths::resolve(app.handle())?;
            let store = SettingsStore::load(paths.settings_file.clone());
            let settings = store.get();
            app.manage(store);
            app.manage(HistoryStore::load(
                paths.history_file.clone(),
                monitor::now_ms(),
            ));
            app.manage(paths);
            app.manage(PendingMode::default());
            if let Err(e) = sync_autostart(app.handle(), settings.launch_on_boot) {
                eprintln!("{e}");
            }
            let monitor = Monitor::new();
            app.manage(monitor.clone());
            tray::setup(app.handle())?;
            monitor.start(app.handle().clone());
            // 开机自启时只待在托盘，手动打开时显示窗口
            if !std::env::args().any(|a| a == AUTOSTART_ARG) {
                window::show(app.handle());
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::refresh,
            commands::ping,
            commands::get_history,
            commands::build_report,
            commands::save_report,
            commands::connect,
            commands::disconnect,
            commands::run_netcheck,
            commands::get_settings,
            commands::take_mode_request,
            commands::set_accept_routes,
            commands::save_settings,
            commands::get_app_info,
            commands::open_ssh,
            commands::open_rustdesk,
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
