//! 后台任务：延迟历史采样、官方客户端新版本检查
use super::{Monitor, now_ms};
use crate::classify::ConnKind;
use crate::cli;
use crate::history::{HistoryStore, Sample};
use crate::settings::SettingsStore;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

/// 延迟历史更新时推给前端的事件
const HISTORY_EVENT: &str = "history";

/// 记入历史并推给前端（后台测量和手动测试共用）
pub fn record_history(app: &AppHandle, samples: Vec<(String, Sample)>) {
    if samples.is_empty() {
        return;
    }
    let store = app.state::<HistoryStore>();
    store.add(samples, now_ms());
    let _ = app.emit(HISTORY_EVENT, store.get());
}

/// 启动后多久第一次检查官方客户端新版本（留几秒给开机时的网络），之后多久检查一次
pub(super) const UPDATE_FIRST_DELAY: Duration = Duration::from_secs(5);
pub(super) const UPDATE_INTERVAL: Duration = Duration::from_secs(6 * 3600);
/// 检查失败（如开机时还没联网）后，多久重试
pub(super) const UPDATE_RETRY: Duration = Duration::from_secs(10 * 60);

impl Monitor {
    /// 检查官方客户端有没有新版本；发现新的版本号时发一次通知。查不到（如断网）时保持原结果并返回 false
    pub(super) async fn check_client_update(&self, app: &AppHandle) -> bool {
        let Ok(info) = cli::version_upstream().await else {
            return false;
        };
        let newer = info.newer_upstream();
        let previous = std::mem::replace(
            &mut *self.inner.client_update.lock().unwrap(),
            newer.clone(),
        );
        let Some(v) = newer.filter(|v| previous.as_ref() != Some(v)) else {
            return true;
        };
        // 立即刷新一次，让本机卡片马上标出新版本
        self.wake();
        crate::applog::info(format!("Tailscale 官方客户端有新版本 {v}"));
        if app.state::<SettingsStore>().get().notify
            && let Err(e) = app
                .notification()
                .builder()
                .title(format!("Tailscale 有新版本 {v}"))
                .body("可在官方客户端的托盘菜单中更新，或到 tailscale.com 下载。")
                .show()
        {
            crate::applog::error(format!("发送通知失败：{e}"));
        }
        true
    }

    /// 并发 ping 所有在线设备，结果记入历史
    pub(super) async fn sample_history(&self, app: &AppHandle) {
        let targets: Vec<(String, String)> = self
            .latest()
            .and_then(|s| s.overview)
            // 组网断开时设备列表是断开前的旧信息，测出来的“无回应”会把小图画错，跳过
            .filter(|o| o.backend_state.as_deref() == Some("Running"))
            .map(|o| o.peers)
            .unwrap_or_default()
            .into_iter()
            .filter(|d| d.conn.kind != ConnKind::Offline)
            .filter_map(|d| {
                let ip = d.ips.iter().find(|a| !a.contains(':'))?.clone();
                Some((d.id, ip))
            })
            .collect();
        if targets.is_empty() {
            return;
        }
        let regions = self.regions().await;
        let local = self.local();
        let mut set = tokio::task::JoinSet::new();
        for (id, ip) in targets {
            let (regions, local) = (regions.clone(), local.clone());
            set.spawn(async move { (id, cli::ping(&ip, &regions, &local).await) });
        }
        let mut samples = Vec::new();
        while let Some(Ok((id, result))) = set.join_next().await {
            // 命令本身出错（如服务没运行）不记录，避免把故障画成“无回应”
            if let Ok(r) = result {
                samples.push((id, Sample::from_ping(now_ms(), &r)));
            }
        }
        record_history(app, samples);
    }
}
