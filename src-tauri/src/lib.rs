mod applog;
mod aumid;
mod classify;
mod cli;
mod client_setup;
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
mod sysinfo;
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
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let paths = AppPaths::resolve(app.handle())?;
            applog::init(paths.log_file.clone());
            applog::info(format!(
                "MeshDock {} 启动{}",
                app.package_info().version,
                if std::env::args().any(|a| a == AUTOSTART_ARG) {
                    "（开机自启）"
                } else {
                    ""
                }
            ));
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
                applog::error(e);
            }
            // 免安装版登记通知的显示名和图标（图标放在设置文件旁）
            if let Some(dir) = app.state::<AppPaths>().settings_file.parent()
                && let Err(e) = aumid::register_if_portable(&app.config().identifier, dir)
            {
                applog::error(format!("登记通知身份失败：{e}"));
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
            commands::set_exit_node,
            commands::set_pref_switch,
            commands::send_files,
            commands::get_profiles,
            commands::get_serve_rules,
            commands::add_serve_rule,
            commands::remove_serve_rule,
            commands::switch_profile,
            commands::save_settings,
            commands::get_app_info,
            commands::open_ssh,
            commands::open_rustdesk,
            commands::device_sysinfo,
            commands::get_client_setup,
            commands::set_tray_autostart,
            commands::set_remote_unattended,
        ])
        // 点击关闭按钮：默认只隐藏到托盘（真正退出走托盘菜单）；设置中关闭了该项时直接退出
        .on_window_event(|win, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let app = win.app_handle();
                if app.state::<SettingsStore>().get().close_to_tray {
                    api.prevent_close();
                    window::hide(win);
                } else {
                    applog::info("关闭窗口，退出程序");
                    app.exit(0);
                }
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
