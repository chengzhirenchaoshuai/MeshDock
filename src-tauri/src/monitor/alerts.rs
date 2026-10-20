//! 需要发通知的变化：路径在直连和中继之间变化、持续走中继、出口设备离线、模式开错
use super::{Monitor, Overview, snapshot::exit_offline};
use crate::notify::{Mismatch, PathClass};
use crate::settings::Settings;
use std::time::Instant;
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

impl Monitor {
    /// 检查路径变化和模式开错，需要时发通知。关闭通知时仍然更新判断状态，重新打开后不会补发旧变化
    pub(super) fn notify_changes(&self, app: &AppHandle, overview: &Overview, settings: &Settings) {
        let now = Instant::now();
        let mut messages = Vec::new();
        {
            let mut paths = self.inner.paths.lock().unwrap();
            for d in &overview.peers {
                let Some(change) = paths.observe(&d.id, PathClass::of(d.conn.kind), now) else {
                    continue;
                };
                let title = match change.to {
                    PathClass::Relay => format!("{} 从直连变成了中继", d.name),
                    PathClass::Direct => format!("{} 恢复直连", d.name),
                };
                let body = match &d.conn.hint {
                    Some(hint) => format!("{}。{hint}", d.conn.label),
                    None => d.conn.label.clone(),
                };
                messages.push((title, body));
            }
        }
        if let Some(limit) = settings.relay_alert() {
            let mut relay = self.inner.relay.lock().unwrap();
            for d in &overview.peers {
                let on_relay = PathClass::of(d.conn.kind) == Some(PathClass::Relay);
                let Some(elapsed) = relay.observe(&d.id, on_relay, now, limit) else {
                    continue;
                };
                let body = match &d.conn.hint {
                    Some(hint) => format!("{}。{hint}", d.conn.label),
                    None => d.conn.label.clone(),
                };
                messages.push((
                    format!("{} 已经走中继 {} 分钟", d.name, elapsed.as_secs() / 60),
                    body,
                ));
            }
        }
        if let Some(name) = self.exit_went_offline(overview) {
            messages.push((
                format!("出口设备 {name} 离线了"),
                "本机的上网流量都经它转发，现在可能上不了网。打开 MeshDock 换一台出口或停用出口。".into(),
            ));
        }
        let mismatch = Mismatch::of(overview.location.at_home, overview.accept_routes);
        if let Some(m) = self.inner.mismatch.lock().unwrap().observe(mismatch) {
            let (title, body) = m.message();
            messages.push((title.into(), body.into()));
        }
        if settings.notify {
            send_all(app, messages);
        }
    }

    /// 选用的出口设备刚变成离线（或已不在列表里）时返回它的名字，持续离线不重复
    fn exit_went_offline(&self, overview: &Overview) -> Option<String> {
        let offline = exit_offline(overview.prefs.as_ref(), &overview.peers);
        let mut last = self.inner.exit_offline.lock().unwrap();
        let was = std::mem::replace(&mut *last, offline.is_some());
        offline.filter(|_| !was)
    }
}

/// 逐条发送 Windows 通知，失败记入日志
pub(super) fn send_all(app: &AppHandle, messages: Vec<(String, String)>) {
    for (title, body) in messages {
        if let Err(e) = app.notification().builder().title(title).body(body).show() {
            crate::applog::error(format!("发送通知失败：{e}"));
        }
    }
}
