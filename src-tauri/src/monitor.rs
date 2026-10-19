//! 后台定时读取组网状态：整理成快照推给前端，并更新托盘
use crate::classify::{self, Conn, ConnKind, LocalNet, Regions};
use crate::cli::{self, CliError};
use crate::gateway;
use crate::history::{HistoryStore, Sample};
use crate::launch;
use crate::model::{Hostinfo, LocalPrefs, NetInfo, PeerStatus, Status, non_empty, time_ms};
use crate::notify::{Mismatch, MismatchWatcher, PathClass, PathWatcher};
use crate::router::{self, RouterHealth};
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
    /// 这台设备可以当出口
    pub exit_option: bool,
    /// 本机正在用这台设备当出口
    pub exit_active: bool,
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
    /// 本机的组网选项（出口、DNS、传入连接等）；读取失败为 None
    pub prefs: Option<LocalPrefs>,
    /// 组网里发布了子网路由的设备
    pub subnet_routes: Vec<SubnetRoute>,
    /// 本机在不在家
    pub location: Location,
    /// 本机是否装了 RustDesk（决定是否显示 RustDesk 按钮）
    pub rustdesk: bool,
    /// 子网路由器的健康状态
    pub routers: Vec<RouterHealth>,
    /// 官方客户端有新版本时为新版本号
    pub client_update: Option<String>,
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

/// 带有效期的缓存值
struct Ttl<T> {
    ttl: Duration,
    value: Mutex<Option<(Instant, T)>>,
}

impl<T: Clone> Ttl<T> {
    fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            value: Mutex::new(None),
        }
    }

    /// 未过期时返回缓存值
    fn get(&self) -> Option<T> {
        let value = self.value.lock().unwrap();
        let (at, v) = value.as_ref()?;
        (at.elapsed() < self.ttl).then(|| v.clone())
    }

    fn set(&self, v: T) {
        *self.value.lock().unwrap() = Some((Instant::now(), v));
    }

    fn clear(&self) {
        *self.value.lock().unwrap() = None;
    }

    /// 未过期时用缓存，否则重新计算并缓存
    fn get_or(&self, f: impl FnOnce() -> T) -> T {
        self.get().unwrap_or_else(|| {
            let v = f();
            self.set(v.clone());
            v
        })
    }
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

/// 启动后多久第一次检查官方客户端新版本（留几秒给开机时的网络），之后多久检查一次
const UPDATE_FIRST_DELAY: Duration = Duration::from_secs(5);
const UPDATE_INTERVAL: Duration = Duration::from_secs(6 * 3600);
/// 检查失败（如开机时还没联网）后，多久重试
const UPDATE_RETRY: Duration = Duration::from_secs(10 * 60);

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
            tokio::time::sleep(UPDATE_FIRST_DELAY).await;
            loop {
                let wait = if this.check_client_update(&update_app).await {
                    UPDATE_INTERVAL
                } else {
                    UPDATE_RETRY
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

    /// 检查官方客户端有没有新版本；发现新的版本号时发一次通知。查不到（如断网）时保持原结果并返回 false
    async fn check_client_update(&self, app: &AppHandle) -> bool {
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

    /// 刷新或测速间隔改了：立即按新间隔重新计时
    pub fn intervals_changed(&self) {
        self.inner.wake.notify_one();
        self.inner.history_wake.notify_one();
    }

    /// 并发 ping 所有在线设备，结果记入历史
    async fn sample_history(&self, app: &AppHandle) {
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
        if !enabled {
            return;
        }
        for (title, body) in messages {
            if let Err(e) = app.notification().builder().title(title).body(body).show() {
                crate::applog::error(format!("发送通知失败：{e}"));
            }
        }
    }

    /// 选用的出口设备刚变成离线（或已不在列表里）时返回它的名字，持续离线不重复
    fn exit_went_offline(&self, overview: &Overview) -> Option<String> {
        let offline = exit_offline(overview.prefs.as_ref(), &overview.peers);
        let mut last = self.inner.exit_offline.lock().unwrap();
        let was = std::mem::replace(&mut *last, offline.is_some());
        offline.filter(|_| !was)
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

impl Device {
    /// 组网 IP，优先 IPv4
    fn main_ip(&self) -> Option<String> {
        self.ips
            .iter()
            .find(|a| !a.contains(':'))
            .or(self.ips.first())
            .cloned()
    }
}

fn device(p: &PeerStatus, conn: Conn) -> Device {
    Device {
        id: p.id.clone().unwrap_or_default(),
        name: p.display_name().unwrap_or("未知设备").to_string(),
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
        exit_option: p.exit_node_option == Some(true),
        exit_active: p.exit_node == Some(true),
    }
}

/// 选用的出口设备离线或已不在组网中时返回它的名字；没选出口返回 None
fn exit_offline(prefs: Option<&LocalPrefs>, peers: &[Device]) -> Option<String> {
    let id = prefs?.exit_node_id.as_ref()?;
    match peers.iter().find(|d| &d.id == id) {
        Some(d) => (d.conn.kind == ConnKind::Offline).then(|| d.name.clone()),
        None => Some("（已不在组网中）".into()),
    }
}

/// 设备卡片的排序键：在线的在前、离线的在后；在线里子网路由器在前；同组按名称（不分大小写）。
/// 不按连接方式或延迟排，避免卡片随路径变化来回跳
fn peer_order(offline: bool, router: bool, name: &str) -> (bool, bool, String) {
    (offline, !router, name.to_lowercase())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 缓存未过期时不重新计算_作废后重新计算() {
        let cache = Ttl::new(Duration::from_secs(60));
        let mut calls = 0;
        assert!(cache.get_or(|| {
            calls += 1;
            true
        }));
        assert!(cache.get_or(|| {
            calls += 1;
            false
        }));
        assert_eq!(calls, 1);
        cache.clear();
        assert_eq!(cache.get(), None);
        // 有效期为 0 时总是过期
        let expired = Ttl::new(Duration::ZERO);
        expired.set(Some(true));
        assert_eq!(expired.get(), None);
    }

    fn dev(id: &str, kind: ConnKind) -> Device {
        Device {
            id: id.into(),
            name: format!("{id}-名"),
            dns_name: None,
            ips: vec![],
            os: None,
            online: None,
            conn: Conn { kind, label: String::new(), hint: None },
            rx_bytes: None,
            tx_bytes: None,
            last_seen_ms: None,
            last_handshake_ms: None,
            key: None,
            ssh_alias: None,
            exit_option: true,
            exit_active: false,
        }
    }

    #[test]
    fn 出口设备离线判断() {
        let peers = [dev("a", ConnKind::Lan), dev("b", ConnKind::Offline)];
        let pick = |id: Option<&str>| LocalPrefs {
            exit_node_id: id.map(str::to_string),
            ..Default::default()
        };
        assert_eq!(exit_offline(None, &peers), None);
        assert_eq!(exit_offline(Some(&pick(None)), &peers), None);
        assert_eq!(exit_offline(Some(&pick(Some("a"))), &peers), None);
        assert_eq!(exit_offline(Some(&pick(Some("b"))), &peers).as_deref(), Some("b-名"));
        assert_eq!(
            exit_offline(Some(&pick(Some("x"))), &peers).as_deref(),
            Some("（已不在组网中）")
        );
    }

    #[test]
    fn 设备排序_在线在前_路由器在前_再按名称() {
        let mut list = [
            ("ming-pc", true, false),
            ("peng-work", false, false),
            ("Alpha", false, false),
            ("phone-router", false, true),
            ("box", true, false),
        ];
        list.sort_by_key(|&(name, offline, router)| peer_order(offline, router, name));
        let names: Vec<_> = list.iter().map(|t| t.0).collect();
        assert_eq!(names, ["phone-router", "Alpha", "peng-work", "box", "ming-pc"]);
    }
}
