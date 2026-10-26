//! 自动切换在家 / 出门模式（设置中开启，默认关闭）：位置按网关识别变化后，模式不对就切换一次
use super::{Monitor, Overview};
use crate::cli;
use crate::settings::Settings;
use tauri::AppHandle;

impl Monitor {
    /// 按本轮的位置和模式判断要不要自动切换；要切时在后台执行（等本轮刷新结束后再重新读取），
    /// 返回 true 表示已开始切换，本轮不再发“模式开错”的通知
    pub(super) fn auto_mode(&self, app: &AppHandle, overview: &Overview, settings: &Settings) -> bool {
        let mut auto = self.inner.auto_mode.lock().unwrap();
        // 只在组网运行、有子网路由器、并且位置是按网关自动识别（不是手动标记）时处理
        let running = overview.backend_state.as_deref() == Some("Running");
        if !settings.auto_mode || !running || overview.subnet_routes.is_empty() {
            if !settings.auto_mode {
                auto.reset();
            }
            return false;
        }
        let at_home = overview.location.auto.then_some(overview.location.at_home).flatten();
        let Some(target) = auto.observe(at_home, overview.accept_routes) else {
            return false;
        };
        drop(auto);
        let mode = if target { "出门模式" } else { "在家模式" };
        let reason = if target {
            "默认网关不是家里路由器，判断你出门了".to_string()
        } else {
            format!("默认网关是家里路由器 {}，判断你在家", overview.location.home_gateway)
        };
        crate::applog::info(format!("自动切换到{mode}：{reason}"));
        let (this, app, notify) = (self.clone(), app.clone(), settings.notify);
        tauri::async_runtime::spawn(async move {
            let message = match cli::set_accept_routes(target).await {
                Ok(()) => {
                    // 等本轮刷新放开锁后重新读取，界面和托盘随之更新
                    this.refresh_reread(&app).await;
                    (format!("已自动切换到{mode}"), format!("{reason}。可在设置中关闭自动切换。"))
                }
                Err(e) => {
                    crate::applog::error(format!("自动切换到{mode}失败：{}\n{}", e.message, e.raw));
                    (format!("自动切换到{mode}失败"), e.message)
                }
            };
            if notify {
                super::alerts::send_all(&app, vec![message]);
            }
        });
        true
    }
}
