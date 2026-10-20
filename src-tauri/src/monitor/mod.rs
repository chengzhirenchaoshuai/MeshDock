//! 后台定时读取组网状态：整理成快照推给前端，并更新托盘
mod alerts;
mod cache;
mod snapshot;
mod tasks;

pub use snapshot::{Device, Location, Overview, Snapshot, SubnetRoute};
pub use tasks::record_history;

use crate::classify::{self, Conn, ConnKind, LocalNet, Regions};
use crate::cli;
use crate::gateway;
use crate::launch;
use crate::model::{Hostinfo, LocalPrefs, NetInfo, PeerStatus, Status};
use crate::notify::{MismatchWatcher, PathWatcher};
use crate::router;
use crate::settings::{Settings, SettingsStore};
use crate::tray::{self, TrayState};
use cache::Ttl;
use snapshot::{device, peer_order};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Notify;

/// 推给前端的快照事件
pub const SNAPSHOT_EVENT: &str = "snapshot";

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

/// 后台监视器，作为 Tauri 托管状态
#[derive(Clone)]
pub struct Monitor {
    inner: Arc<Inner>,
}

struct Inner {
    latest: Mutex<Option<Snapshot>>,
    regions: Mutex<Option<Arc<Regions>>>,
    /// 中继区域表最近一次读取失败的时间，用于暂缓重试
    regions_failed_at: Mutex<Option<Instant>>,
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
    /// 唤醒后台测速循环（测速间隔改了时）
    history_wake: Notify,
    /// 刷新排队执行：后台循环、手动刷新、连接 / 断开、切换模式可能同时发起刷新，
    /// 不加锁时先开始的刷新可能后结束，用旧快照覆盖新快照，通知判断也会被重复推进
    refreshing: tokio::sync::Mutex<()>,
    /// ~/.ssh/config 的解析结果，按修改时间缓存
    ssh_hosts: launch::SshHostsCache,
    /// 本机是否装了 RustDesk
    rustdesk: Ttl<bool>,
    /// 本机的组网选项（只缓存读取成功的结果）
    prefs: Ttl<LocalPrefs>,
    /// 上一轮选用的出口设备是否离线，用于只在刚离线时发一次通知
    exit_offline: Mutex<bool>,
    /// 官方客户端的新版本号（后台定期检查）
    client_update: Mutex<Option<String>>,
}

/// RustDesk 是否安装，多久重新检测一次
const RUSTDESK_TTL: Duration = Duration::from_secs(60);
/// 本机组网选项多久重新读取一次。默认刷新间隔（10 秒）下每 3 轮读一次；
/// 本程序切换模式、出口或其他选项、连接、断开后立即重新读取
const PREFS_TTL: Duration = Duration::from_secs(30);

/// netmap 中的网络检查结论：本机的，以及按节点公钥索引的各设备的
#[derive(Default)]
struct NetInfos {
    me: Option<NetInfo>,
    /// 节点公钥 → 设备上报的主机信息
    peers: HashMap<String, Hostinfo>,
    /// 节点公钥 → 设备的局域网 IPv4，用来对应 ~/.ssh/config 里写局域网地址的别名
    lan_ips: HashMap<String, Vec<String>>,
}

/// 中继区域表读取失败后，多久再重试
const REGIONS_RETRY: Duration = Duration::from_secs(60);

/// netmap 缓存多久，避免每次刷新都读取（约 34 KB）
const NET_INFO_TTL: Duration = Duration::from_secs(30);

impl Monitor {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                latest: Mutex::new(None),
                regions: Mutex::new(None),
                regions_failed_at: Mutex::new(None),
                local: Mutex::new(Arc::default()),
                active_since: Mutex::new(None),
                net_infos: Mutex::new(None),
                paths: Mutex::new(PathWatcher::default()),
                mismatch: Mutex::new(MismatchWatcher::default()),
                wake: Notify::new(),
                history_wake: Notify::new(),
                refreshing: tokio::sync::Mutex::new(()),
                ssh_hosts: launch::SshHostsCache::default(),
                rustdesk: Ttl::new(RUSTDESK_TTL),
                prefs: Ttl::new(PREFS_TTL),
                exit_offline: Mutex::new(false),
                client_update: Mutex::new(None),
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
                let interval = app
                    .state::<SettingsStore>()
                    .get()
                    .refresh_interval(window_visible(&app));
                // 到时间或被要求立即刷新，二者先到为准
                tokio::select! {
                    _ = tokio::time::sleep(interval) => {}
                    _ = this.inner.wake.notified() => {}
                }
            }
        });
        // 官方客户端新版本：启动后立即检查，之后每 6 小时一次；查不到时 10 分钟后重试
        let this = self.clone();
        let update_app = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(tasks::UPDATE_FIRST_DELAY).await;
            loop {
                let wait = if this.check_client_update(&update_app).await {
                    tasks::UPDATE_INTERVAL
                } else {
                    tasks::UPDATE_RETRY
                };
                tokio::time::sleep(wait).await;
            }
        });
        // 延迟历史：启动时前端会测一遍全部设备并记入历史，后台从下一个间隔开始测
        let this = self.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                let last = Instant::now();
                // 等到下一次测速；期间改了间隔会被唤醒，按新间隔重新计算剩余时间
                loop {
                    let interval = app.state::<SettingsStore>().get().sample_interval();
                    let Some(left) = interval.checked_sub(last.elapsed()) else {
                        break;
                    };
                    tokio::select! {
                        _ = tokio::time::sleep(left) => {}
                        _ = this.inner.history_wake.notified() => {}
                    }
                }
                this.sample_history(&app).await;
            }
        });
    }

    /// 刷新或测速间隔改了：立即按新间隔重新计时
    pub fn intervals_changed(&self) {
        self.inner.wake.notify_one();
        self.inner.history_wake.notify_one();
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

    /// 中继区域名称表；首次成功读取后缓存。读取失败后 REGIONS_RETRY 内不再重试，
    /// 避免服务没运行时每次刷新都多起一个注定失败的进程
    pub async fn regions(&self) -> Arc<Regions> {
        if let Some(r) = self.inner.regions.lock().unwrap().clone() {
            return r;
        }
        let failed_recently = self
            .inner
            .regions_failed_at
            .lock()
            .unwrap()
            .is_some_and(|at| at.elapsed() < REGIONS_RETRY);
        if failed_recently {
            return Arc::new(Regions::default());
        }
        match cli::derp_map().await {
            Ok(map) => {
                let r = Arc::new(Regions::from_derp_map(&map));
                *self.inner.regions.lock().unwrap() = Some(r.clone());
                r
            }
            Err(_) => {
                *self.inner.regions_failed_at.lock().unwrap() = Some(Instant::now());
                Arc::new(Regions::default())
            }
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
                    peers: map.peer_hostinfo(),
                    lan_ips: map.peer_lan_ips(),
                });
                *self.inner.net_infos.lock().unwrap() = Some((Instant::now(), infos.clone()));
                Some(infos)
            }
            Err(_) => cached.map(|(_, infos)| infos),
        }
    }

    /// 用 netmap 补充信息：给走中继的设备补上可能的原因，按局域网地址补 SSH 别名，生成子网路由器的健康状态。
    /// 既没有设备走中继、也没有子网路由器、也没有缺 SSH 别名的设备时不读取 netmap
    async fn enrich(&self, overview: &mut Overview) {
        let has_relay = overview
            .peers
            .iter()
            .any(|d| d.conn.kind == ConnKind::Relay);
        let ssh_hosts = self.inner.ssh_hosts.get();
        let missing_alias =
            !ssh_hosts.is_empty() && overview.peers.iter().any(|d| d.ssh_alias.is_none());
        if !has_relay && overview.subnet_routes.is_empty() && !missing_alias {
            return;
        }
        let infos = self.net_infos().await;
        let hostinfo = |key: &Option<String>| {
            let infos = infos.as_ref()?;
            infos.peers.get(key.as_ref()?)
        };
        for d in overview.peers.iter_mut() {
            let peer = hostinfo(&d.key).and_then(|h| h.net_info.as_ref());
            let me = infos.as_ref().and_then(|i| i.me.as_ref());
            let reason = classify::relay_reason(me, peer);
            d.conn = classify::with_relay_reason(d.conn.clone(), reason);
            // 组网 IP 和名称都对不上时，再看 ssh 配置里有没有写这台设备局域网地址的别名（如手机的 phone）
            if d.ssh_alias.is_none()
                && let Some(lan) = infos.as_ref().zip(d.key.as_ref()).and_then(|(i, k)| i.lan_ips.get(k))
            {
                let candidates: Vec<&str> = lan.iter().map(String::as_str).collect();
                d.ssh_alias = launch::alias_for(&ssh_hosts, &candidates).map(str::to_string);
            }
        }
        overview.routers = overview
            .subnet_routes
            .iter()
            .map(|r| {
                let device = overview.peers.iter().find(|d| d.name == r.device);
                router::health(
                    &r.device,
                    device.is_some_and(|d| d.conn.kind != ConnKind::Offline),
                    r.routes.clone(),
                    device.and_then(|d| d.last_seen_ms),
                    device.and_then(|d| hostinfo(&d.key)),
                )
            })
            .collect();
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

    /// 按设备 id 在最近的快照里找到设备
    fn peer(&self, id: &str) -> Option<Device> {
        self.latest()?.overview?.peers.into_iter().find(|d| d.id == id)
    }

    /// 按设备 id 找到可当出口的设备，返回名称和组网 IP
    pub fn exit_candidate(&self, id: &str) -> Option<(String, String)> {
        let d = self.peer(id).filter(|d| d.exit_option)?;
        Some((d.name.clone(), d.main_ip()?))
    }

    /// 按设备 id 找到在线设备，返回名称和组网 IP（发送文件用）
    pub fn online_peer(&self, id: &str) -> Option<(String, String)> {
        let d = self.peer(id).filter(|d| d.conn.kind != ConnKind::Offline)?;
        Some((d.name.clone(), d.main_ip()?))
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
        let peer = key
            .as_ref()
            .and_then(|k| infos.peers.get(k))
            .and_then(|h| h.net_info.as_ref());
        classify::with_relay_reason(conn, classify::relay_reason(infos.me.as_ref(), peer))
    }

    /// 本机组网选项：缓存未过期时直接用，否则读取 debug prefs；读取失败不缓存
    async fn read_prefs(&self) -> Option<LocalPrefs> {
        if let Some(v) = self.inner.prefs.get() {
            return Some(v);
        }
        let v = LocalPrefs::from(cli::prefs().await.ok()?);
        self.inner.prefs.set(v.clone());
        Some(v)
    }

    fn invalidate_prefs(&self) {
        self.inner.prefs.clear();
    }

    /// 读取一次状态，保存快照、推给前端、更新托盘
    pub async fn refresh(&self, app: &AppHandle) -> Snapshot {
        self.refresh_with(app, false).await
    }

    /// 不用缓存、重新读取本机组网选项后刷新（切换模式、出口或其他选项、连接、断开后调用）
    pub async fn refresh_reread(&self, app: &AppHandle) -> Snapshot {
        self.refresh_with(app, true).await
    }

    async fn refresh_with(&self, app: &AppHandle, reread_prefs: bool) -> Snapshot {
        let _guard = self.inner.refreshing.lock().await;
        // 必须在拿到锁之后再作废：否则正在进行的刷新可能把切换前读到的旧值写回缓存
        if reread_prefs {
            self.invalidate_prefs();
        }
        let regions = self.regions().await;
        // 状态和设置同时读取；设置读不到不影响设备列表
        let (status, prefs) = tokio::join!(cli::status(), self.read_prefs());
        // 状态读不到（如服务没运行）时不沿用缓存的模式，托盘也不勾选
        let prefs = if status.is_ok() {
            prefs
        } else {
            self.invalidate_prefs();
            None
        };
        let accept_routes = prefs.as_ref().and_then(|p| p.accept_routes);
        let snapshot = match status {
            Ok(status) => {
                let mut overview = self.overview(&status, &regions, prefs);
                self.enrich(&mut overview).await;
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
        tray::set_quick(app, snapshot.overview.as_ref());
        let previous = self.inner.latest.lock().unwrap().replace(snapshot.clone());
        log_error_change(previous.as_ref(), &snapshot);
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
        prefs: Option<LocalPrefs>,
    ) -> Overview {
        let local = Arc::new(LocalNet::from_status(status));
        *self.inner.local.lock().unwrap() = local.clone();
        let subnet_routes = status
            .peers()
            .iter()
            .filter_map(|p| {
                let routes = p.primary_routes.clone().filter(|r| !r.is_empty())?;
                Some(SubnetRoute {
                    device: p.display_name().unwrap_or("未知设备").to_string(),
                    routes,
                })
            })
            .collect::<Vec<_>>();
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
        let ssh_hosts = self.inner.ssh_hosts.get();
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
            .collect::<Vec<_>>();
        // 界面顺序：在线在前，其中子网路由器在前，再按名称
        let mut peers = peers;
        peers.sort_by_cached_key(|d| {
            let router = subnet_routes.iter().any(|r| r.device == d.name);
            peer_order(d.conn.kind == ConnKind::Offline, router, &d.name)
        });
        Overview {
            backend_state: status.backend_state.clone(),
            control_unreachable: classify::control_unreachable(status),
            version: status.version.clone(),
            self_device: status
                .self_node
                .as_ref()
                .map(|p| device(p, classify::self_conn())),
            peers,
            accept_routes: prefs.as_ref().and_then(|p| p.accept_routes),
            prefs,
            subnet_routes,
            location: Location::default(),
            rustdesk: self.inner.rustdesk.get_or(launch::rustdesk_installed),
            routers: Vec::new(),
            client_update: self.inner.client_update.lock().unwrap().clone(),
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

/// 读取状态出错、恢复或错误类型变化时记入日志；状态不变时不重复记录
fn log_error_change(previous: Option<&Snapshot>, current: &Snapshot) {
    let before = previous.and_then(|s| s.error.as_ref()).map(|e| e.kind);
    match (&current.error, before) {
        (Some(e), b) if b != Some(e.kind) => {
            crate::applog::error(format!("读取组网状态失败：{}\n{}", e.message, e.raw));
        }
        (None, Some(_)) => crate::applog::info("读取组网状态已恢复"),
        _ => {}
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
