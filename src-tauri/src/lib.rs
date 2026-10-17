// M3 接入界面前，部分接口暂未使用
#[allow(dead_code)]
mod classify;
#[allow(dead_code)]
mod cli;
#[allow(dead_code)]
mod model;
mod tray;
mod window;

use tauri::{RunEvent, WindowEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // 单实例插件必须最先注册：再次打开程序时只唤出已有窗口
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            window::show(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            tray::setup(app.handle())?;
            window::show(app.handle());
            Ok(())
        })
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
