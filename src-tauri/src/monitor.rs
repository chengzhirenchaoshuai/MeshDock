//! 后台定时读取组网状态：整理成快照推给前端，并更新托盘
use crate::classify::{self, Conn, ConnKind, LocalNet, Regions};
use crate::cli::{self, CliError};
use crate::gateway;
use crate::history::{HistoryStore, Sample};
use crate::launch;
use crate::model::{NetInfo, PeerStatus, Status, non_empty, time_ms};
use crate::notify::{Mismatch, MismatchWatcher, PathClass, PathWatcher};
use crate::settings::{Settings, SettingsStore};
use crate::tray::{self, TrayState};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;
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
    /// 节点公钥，只在后端用来对应 netmap，不发给前端
    #[serde(skip)]
    pub key: Option<String>,
    /// ~/.ssh/config 中对应这台设备的别名，没有则不显示 SSH 按钮
    pub ssh_alias: Option<String>,
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
    /// 本机在不在家
    pub location: Location,
    /// 本机是否装了 RustDesk（决定是否显示 RustDesk 按钮）
    pub rustdesk: bool,
}

/// 在不在家：优先按默认网关自动识别，读不到网关时用手动标记
#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    /// None 表示无法判断（读不到网关且没有手动标记）
    pub at_home: Option<bool>,
    /// true 为按网关自动识别，false 为手动标记
    pub auto: bool,
    /// 当前所有默认网关
    pub gateways: Vec<String>,
    /// 设置中的家里网关
    pub home_gateway: String,
}

/// 延迟历史更新时推给前端的事件
pub const HISTORY_EVENT: &str = "history";
const HISTORY_FIRST_DELAY: Duration = Duration::from_secs(30);
const HISTORY_INTERVAL: Duration = Duration::from_secs(300);

/// 记入历史并推给前端（后台测量和手动测试共用）
pub fn record_history(app: &AppHandle, samples: Vec<(String, Sample)>) {
    if samples.is_empty() {
        return;
    }
    let store = app.state::<HistoryStore>();
    store.add(samples, now_ms());
    let _ = app.emit(HISTORY_EVENT, store.get());
}

/// 读取默认网关并判断位置
fn locate(settings: &Settings) -> Location {
    let gateways = gateway::default_gateways();
    let auto = gateway::at_home(&gateways, &settings.home_gateway);
    Location {
        at_home: auto.or(settings.at_home),
        auto: auto.is_some(),
        gateways: gateways.iter().map(ToString::to_string).collect(),
        home_gateway: settings.home_gateway.clone(),
    }
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
    /// 各设备上报的网络检查结论及读取时间，只在有设备走中继时才读取
    net_infos: Mutex<Option<(Instant, Arc<NetInfos>)>>,
    /// 路径变化与模式开错的判断状态（用于发通知）
    paths: Mutex<PathWatcher>,
    mismatch: Mutex<MismatchWatcher>,
    wake: Notify,
}

/// netmap 中的网络检查结论：本机的，以及按节点公钥索引的各设备的
#[derive(Default)]
struct NetInfos {
    me: Option<NetInfo>,
    peers: HashMap<String, NetInfo>,
}

/// netmap 缓存多久，避免每次刷新都读取（约 34 KB）
const NET_INFO_TTL: Duration = Duration::from_secs(30);

impl Monitor {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                latest: Mutex::new(None),
                regions: Mutex::new(None),
                local: Mutex::new(Arc::default()),
                active_since: Mutex::new(None),
                net_infos: Mutex::new(None),
                paths: Mutex::new(PathWatcher::default()),
                mismatch: Mutex::new(MismatchWatcher::default()),
                wake: Notify::new(),
            }),
        }
    }

    /// 启动后台循环
    pub fn start(&self, app: AppHandle) {
        let this = self.clone();
        let refresh_app = app.clone();
        tauri::async_runtime::spawn(async move {
            let app = refresh_app;
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
        // 延迟历史：启动 30 秒后开始，之后每 5 分钟测一次在线设备
        let this = self.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(HISTORY_FIRST_DELAY).await;
            loop {
                this.sample_history(&app).await;
                tokio::time::sleep(HISTORY_INTERVAL).await;
            }
        });
    }

    /// 并发 ping 所有在线设备，结果记入历史
    async fn sample_history(&self, app: &AppHandle) {
        let targets: Vec<(String, String)> = self
            .latest()
            .and_then(|s| s.overview)
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

    /// 各设备上报的网络检查结论；缓存 30 秒，读取失败时沿用旧的
    async fn net_infos(&self) -> Option<Arc<NetInfos>> {
        let cached = self.inner.net_infos.lock().unwrap().clone();
        if let Some((at, infos)) = &cached
            && at.elapsed() < NET_INFO_TTL
        {
            return Some(infos.clone());
        }
        match cli::netmap().await {
            Ok(map) => {
                let infos = Arc::new(NetInfos {
                    me: map.self_net_info(),
                    peers: map.peer_net_info(),
                });
                *self.inner.net_infos.lock().unwrap() = Some((Instant::now(), infos.clone()));
                Some(infos)
            }
            Err(_) => cached.map(|(_, infos)| infos),
        }
    }

    /// 给走中继的设备补上可能的原因；没有设备走中继时不读取 netmap
    async fn explain_relays(&self, peers: &mut [Device]) {
        if !peers.iter().any(|d| d.conn.kind == ConnKind::Relay) {
            return;
        }
        let Some(infos) = self.net_infos().await else {
            return;
        };
        for d in peers.iter_mut() {
            let peer = d.key.as_ref().and_then(|k| infos.peers.get(k));
            let reason = classify::relay_reason(infos.me.as_ref(), peer);
            d.conn = classify::with_relay_reason(d.conn.clone(), reason);
        }
    }

    /// 按组网 IP 在最近的快照里找到设备 id
    pub fn device_id_for_ip(&self, ip: &str) -> Option<String> {
        self.latest()?
            .overview?
            .peers
            .into_iter()
            .find(|d| d.ips.iter().any(|a| a == ip))
            .map(|d| d.id)
    }

    /// 测速结果是中继时补上原因：按 IP 在最近的快照里找到设备
    pub async fn explain_ping(&self, ip: &str, conn: Conn) -> Conn {
        if conn.kind != ConnKind::Relay {
            return conn;
        }
        let key = self.latest().and_then(|s| {
            s.overview?
                .peers
                .into_iter()
                .find(|d| d.ips.iter().any(|a| a == ip))?
                .key
        });
        let Some(infos) = self.net_infos().await else {
            return conn;
        };
        let peer = key.as_ref().and_then(|k| infos.peers.get(k));
        classify::with_relay_reason(conn, classify::relay_reason(infos.me.as_ref(), peer))
    }

    /// 检查路径变化和模式开错，需要时发通知。关闭通知时仍然更新判断状态，重新打开后不会补发旧变化
    fn notify_changes(&self, app: &AppHandle, overview: &Overview, enabled: bool) {
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
        let mismatch = Mismatch::of(overview.location.at_home, overview.accept_routes);
        if let Some(m) = self.inner.mismatch.lock().unwrap().observe(mismatch) {
            let (title, body) = m.message();
            messages.push((title.into(), body.into()));
        }
        if !enabled {
            return;
        }
        for (title, body) in messages {
            if let Err(e) = app.notification().builder().title(title).body(body).show() {
                eprintln!("发送通知失败：{e}");
            }
        }
    }

    /// 读取一次状态，保存快照、推给前端、更新托盘
    pub async fn refresh(&self, app: &AppHandle) -> Snapshot {
        let regions = self.regions().await;
        // 状态和设置同时读取；设置读不到不影响设备列表
        let (status, prefs) = tokio::join!(cli::status(), cli::prefs());
        let accept_routes = prefs.ok().and_then(|p| p.route_all);
        let snapshot = match status {
            Ok(status) => {
                let mut overview = self.overview(&status, &regions, accept_routes);
                self.explain_relays(&mut overview.peers).await;
                let settings = app.state::<SettingsStore>().get();
                overview.location = locate(&settings);
                self.notify_changes(app, &overview, settings.notify);
                Snapshot {
                    overview: Some(overview),
                    error: None,
                    updated_at_ms: now_ms(),
                }
            }
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
        // 仅开发版：测试时把快照写到 MESHDOCK_DEBUG_DUMP 指定的文件，便于不看界面也能核对数据
        #[cfg(debug_assertions)]
        if let Some(path) = std::env::var_os("MESHDOCK_DEBUG_DUMP")
            && let Ok(text) = serde_json::to_string_pretty(&snapshot)
        {
            let _ = std::fs::write(path, text);
        }
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
            .collect::<Vec<_>>();
        // SSH 别名：按组网 IP、名称或 MagicDNS 名对应 ~/.ssh/config 中的 HostName
        let ssh_hosts = launch::load_ssh_hosts();
        let peers = peers
            .into_iter()
            .map(|mut d| {
                let mut candidates: Vec<&str> = d.ips.iter().map(String::as_str).collect();
                candidates.push(&d.name);
                if let Some(dns) = &d.dns_name {
                    candidates.push(dns);
                }
                d.ssh_alias = launch::alias_for(&ssh_hosts, &candidates).map(str::to_string);
                d
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
            location: Location::default(),
            rustdesk: launch::rustdesk_installed(),
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
        key: p.public_key.clone(),
        ssh_alias: None,
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
