//! 后台任务：延迟历史采样（及延迟偏高提醒）、官方客户端和 MeshDock 自身的新版本检查
use super::{Monitor, now_ms};
use crate::classify::ConnKind;
use crate::app_update::UpdateState;
use crate::cli;
use crate::history::{DAY_MS, HistoryStore, Sample};
use crate::notify::LATENCY_STREAK;
use crate::settings::SettingsStore;
use std::collections::HashMap;
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
    let _ = app.emit(HISTORY_EVENT, store.recent(now_ms() - DAY_MS));
}

/// 启动后多久第一次检查官方客户端新版本（留几秒给开机时的网络），之后多久检查一次
pub(super) const UPDATE_FIRST_DELAY: Duration = Duration::from_secs(5);
pub(super) const UPDATE_INTERVAL: Duration = Duration::from_secs(6 * 3600);
/// 检查失败（如开机时还没联网）后，多久重试
pub(super) const UPDATE_RETRY: Duration = Duration::from_secs(10 * 60);

impl Monitor {
    /// 检查 MeshDock 自身有没有新版本；发现新的版本号时发一次通知。查不到时记下原因（设置页显示），只在原因变化时记日志
    pub(super) async fn check_app_update(&self, app: &AppHandle) {
        let current = app.package_info().version.to_string();
        let next = match crate::app_update::check(&current).await {
            Ok(Some(u)) => UpdateState::Available(u),
            Ok(None) => UpdateState::Latest,
            Err(reason) => UpdateState::Unavailable { reason },
        };
        let previous = std::mem::replace(&mut *self.inner.app_update.lock().unwrap(), next.clone());
        if previous == next {
            return;
        }
        self.wake();
        match &next {
            UpdateState::Available(u) => {
                crate::applog::info(format!("MeshDock 有新版本 {}", u.version));
                if app.state::<SettingsStore>().get().notify {
                    super::alerts::send_all(
                        app,
                        vec![(
                            format!("MeshDock 有新版本 {}", u.version),
                            "打开 MeshDock 的“设置”页，在最下面点“打开下载页”。".into(),
                        )],
                    );
                }
            }
            UpdateState::Unavailable { reason } => {
                crate::applog::info(format!("无法检查 MeshDock 新版本：{reason}"));
            }
            _ => {}
        }
    }

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
        let targets: Vec<(String, String, String)> = self
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
                Some((d.id, d.name, ip))
            })
            .collect();
        if targets.is_empty() {
            return;
        }
        let regions = self.regions().await;
        let local = self.local();
        let mut set = tokio::task::JoinSet::new();
        let names: HashMap<String, String> =
            targets.iter().map(|(id, name, _)| (id.clone(), name.clone())).collect();
        for (id, _, ip) in targets {
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
        self.check_latency(app, &samples, &names);
        record_history(app, samples);
    }

    /// 后台测速后检查延迟是否持续偏高，需要时发通知。关闭通知时仍然更新判断状态
    fn check_latency(
        &self,
        app: &AppHandle,
        samples: &[(String, Sample)],
        names: &HashMap<String, String>,
    ) {
        let settings = app.state::<SettingsStore>().get();
        let Some(threshold) = settings.latency_alert() else {
            return;
        };
        let mut watcher = self.inner.latency.lock().unwrap();
        let messages: Vec<(String, String)> = samples
            .iter()
            .filter(|(id, s)| watcher.observe(id, s.ms, threshold))
            .map(|(id, s)| {
                let name = names.get(id).map_or("未知设备", String::as_str);
                let path = s.kind.map(kind_label).unwrap_or("路径未知");
                (
                    format!("{name} 延迟持续偏高"),
                    format!(
                        "最近 {LATENCY_STREAK} 次测速都超过 {threshold} ms，最近一次 {:.0} ms（{path}）。",
                        s.ms.unwrap_or_default()
                    ),
                )
            })
            .collect();
        drop(watcher);
        if settings.notify {
            super::alerts::send_all(app, messages);
        }
    }
}

/// 实测路径的简短中文（延迟历史只记了路径类型）
fn kind_label(kind: ConnKind) -> &'static str {
    match kind {
        ConnKind::Lan => "局域网直连",
        ConnKind::Ipv6 => "IPv6 直连",
        ConnKind::Public => "公网直连",
        ConnKind::Relay | ConnKind::PeerRelay => "中继",
        _ => "路径未知",
    }
}
