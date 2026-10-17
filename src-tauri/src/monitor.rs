//! 后台定时读取组网状态：整理成快照推给前端，并更新托盘
use crate::classify::{self, Conn, LocalNet, Regions};
use crate::cli::{self, CliError};
use crate::model::{PeerStatus, Status, non_empty, time_ms};
use crate::tray::{self, TrayState};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Notify;

/// 推给前端的快照事件
pub const SNAPSHOT_EVENT: &str = "snapshot";

/// 窗口可见时的刷新间隔
const INTERVAL_VISIBLE: Duration = Duration::from_secs(10);
/// 窗口隐藏时只需更新托盘，降低频率
const INTERVAL_HIDDEN: Duration = Duration::from_secs(30);

/// 一台设备，给前端展示用
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub id: String,
    pub name: String,
    pub dns_name: Option<String>,
    pub ips: Vec<String>,
    pub os: Option<String>,
    pub online: Option<bool>,
    pub conn: Conn,
    pub rx_bytes: Option<u64>,
    pub tx_bytes: Option<u64>,
    pub last_seen_ms: Option<i64>,
    pub last_handshake_ms: Option<i64>,
}

/// 组网整体情况
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub backend_state: Option<String>,
    pub control_unreachable: bool,
    pub version: Option<String>,
    pub self_device: Option<Device>,
    pub peers: Vec<Device>,
    /// 本机 accept-routes 的实际值（读取 debug prefs）；读取失败为 None
    pub accept_routes: Option<bool>,
    /// 组网里发布了子网路由的设备
    pub subnet_routes: Vec<SubnetRoute>,
}

/// 某台设备发布的子网路由
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SubnetRoute {
    pub device: String,
    pub routes: Vec<String>,
}

/// 一次刷新的结果：成功时有 overview，失败时有 error
#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub overview: Option<Overview>,
    pub error: Option<CliError>,
    pub updated_at_ms: i64,
}

/// 后台监视器，作为 Tauri 托管状态
#[derive(Clone)]
pub struct Monitor {
    inner: Arc<Inner>,
}

struct Inner {
    latest: Mutex<Option<Snapshot>>,
    regions: Mutex<Option<Arc<Regions>>>,
    /// 最近一次读到的本机网络信息，测速时判断局域网 IPv6 也要用
    local: Mutex<Arc<LocalNet>>,
    /// 设备 id → 本轮连续活跃的起始时间；None 表示还没观察过
    active_since: Mutex<Option<HashMap<String, Instant>>>,
    wake: Notify,
}

impl Monitor {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                latest: Mutex::new(None),
                regions: Mutex::new(None),
                local: Mutex::new(Arc::default()),
                active_since: Mutex::new(None),
                wake: Notify::new(),
            }),
        }
    }

    /// 启动后台循环
    pub fn start(&self, app: AppHandle) {
        let this = self.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                this.refresh(&app).await;
                let interval = if window_visible(&app) {
                    INTERVAL_VISIBLE
                } else {
                    INTERVAL_HIDDEN
                };
                // 到时间或被要求立即刷新，二者先到为准
                tokio::select! {
                    _ = tokio::time::sleep(interval) => {}
                    _ = this.inner.wake.notified() => {}
                }
            }
        });
    }

    /// 要求后台立即刷新一次（托盘“刷新”、窗口重新显示时）
    pub fn wake(&self) {
        self.inner.wake.notify_one();
    }

    /// 窗口重新显示时调用：快照超过 3 秒才刷新；还没有快照说明后台首轮正在进行，不重复触发
    pub fn wake_if_stale(&self) {
        let stale = self
            .latest()
            .is_some_and(|s| now_ms() - s.updated_at_ms > 3000);
        if stale {
            self.wake();
        }
    }

    pub fn local(&self) -> Arc<LocalNet> {
        self.inner.local.lock().unwrap().clone()
    }

    pub fn latest(&self) -> Option<Snapshot> {
        self.inner.latest.lock().unwrap().clone()
    }

    /// 中继区域名称表；首次成功读取后缓存，失败时下次再试
    pub async fn regions(&self) -> Arc<Regions> {
        if let Some(r) = self.inner.regions.lock().unwrap().clone() {
            return r;
        }
        match cli::derp_map().await {
            Ok(map) => {
                let r = Arc::new(Regions::from_derp_map(&map));
                *self.inner.regions.lock().unwrap() = Some(r.clone());
                r
            }
            Err(_) => Arc::new(Regions::default()),
        }
    }

    /// 读取一次状态，保存快照、推给前端、更新托盘
    pub async fn refresh(&self, app: &AppHandle) -> Snapshot {
        let regions = self.regions().await;
        // 状态和设置同时读取；设置读不到不影响设备列表
        let (status, prefs) = tokio::join!(cli::status(), cli::prefs());
        let accept_routes = prefs.ok().and_then(|p| p.route_all);
        let snapshot = match status {
            Ok(status) => Snapshot {
                overview: Some(self.overview(&status, &regions, accept_routes)),
                error: None,
                updated_at_ms: now_ms(),
            },
            Err(e) => Snapshot {
                overview: None,
                error: Some(e),
                updated_at_ms: now_ms(),
            },
        };
        tray::set_state(app, tray_state(&snapshot));
        tray::set_mode(app, accept_routes);
        *self.inner.latest.lock().unwrap() = Some(snapshot.clone());
        let _ = app.emit(SNAPSHOT_EVENT, &snapshot);
        snapshot
    }

    fn overview(
        &self,
        status: &Status,
        regions: &Regions,
        accept_routes: Option<bool>,
    ) -> Overview {
        let local = Arc::new(LocalNet::from_status(status));
        *self.inner.local.lock().unwrap() = local.clone();
        let subnet_routes = status
            .peers()
            .iter()
            .filter_map(|p| {
                let routes = p.primary_routes.clone().filter(|r| !r.is_empty())?;
                Some(SubnetRoute {
                    device: non_empty(&p.host_name).unwrap_or("未知设备").to_string(),
                    routes,
                })
            })
            .collect();
        let peers = status.peers();
        self.track_active(&peers);
        let since = self.inner.active_since.lock().unwrap();
        let peers = peers
            .into_iter()
            .map(|p| {
                let active_for =
                    p.id.as_ref()
                        .and_then(|id| since.as_ref()?.get(id))
                        .map(Instant::elapsed);
                device(p, classify::classify(p, active_for, regions, &local))
            })
            .collect();
        Overview {
            backend_state: status.backend_state.clone(),
            control_unreachable: classify::control_unreachable(status),
            version: status.version.clone(),
            self_device: status
                .self_node
                .as_ref()
                .map(|p| device(p, classify::self_conn())),
            peers,
            accept_routes,
            subnet_routes,
        }
    }

    /// 记录每台设备从不活跃变为活跃的时刻，用于判断“正在打洞”。
    /// 程序启动后第一次看到的活跃设备视为早已活跃，避免误报“正在打洞”
    fn track_active(&self, peers: &[&PeerStatus]) {
        let mut since = self.inner.active_since.lock().unwrap();
        let now = Instant::now();
        let first_seen = match &*since {
            Some(_) => now,
            None => now.checked_sub(classify::PUNCH_WINDOW).unwrap_or(now),
        };
        let mut seen = HashMap::new();
        for p in peers {
            let Some(id) = &p.id else { continue };
            if p.active == Some(true) {
                let start = since.as_ref().and_then(|m| m.get(id).copied());
                seen.insert(id.clone(), start.unwrap_or(first_seen));
            }
        }
        *since = Some(seen);
    }
}

fn device(p: &PeerStatus, conn: Conn) -> Device {
    Device {
        id: p.id.clone().unwrap_or_default(),
        name: non_empty(&p.host_name).unwrap_or("未知设备").to_string(),
        dns_name: non_empty(&p.dns_name).map(|s| s.trim_end_matches('.').to_string()),
        ips: p.tailscale_ips.clone().unwrap_or_default(),
        os: non_empty(&p.os).map(str::to_string),
        online: p.online,
        conn,
        rx_bytes: p.rx_bytes,
        tx_bytes: p.tx_bytes,
        last_seen_ms: time_ms(p.last_seen.as_deref()),
        last_handshake_ms: time_ms(p.last_handshake.as_deref()),
    }
}

/// 快照 → 托盘状态
fn tray_state(s: &Snapshot) -> TrayState {
    match &s.overview {
        None => TrayState::Disconnected,
        Some(o) if o.backend_state.as_deref() != Some("Running") => TrayState::Disconnected,
        Some(o) if o.control_unreachable => TrayState::ControlUnreachable,
        Some(_) => TrayState::Connected,
    }
}

fn window_visible(app: &AppHandle) -> bool {
    app.get_webview_window("main")
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false)
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
