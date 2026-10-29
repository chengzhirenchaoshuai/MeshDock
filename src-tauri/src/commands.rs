//! 暴露给前端的命令
use crate::applog;
use crate::classify::PingResult;
use crate::cli::{self, CliError, ErrorKind};
use crate::history::{DAY_MS, History, HistoryStore, Sample};
use crate::monitor::{Monitor, Snapshot};
use crate::netcheck::{self, Report};
use crate::paths::AppPaths;
use crate::report;
use crate::settings::{Settings, SettingsStore};
use crate::tray::PendingMode;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, State};

/// 取最近一次快照；还没有时立即读取一次
#[tauri::command]
pub async fn get_snapshot(app: AppHandle, monitor: State<'_, Monitor>) -> Result<Snapshot, ()> {
    match monitor.latest() {
        Some(s) => Ok(s),
        None => Ok(monitor.refresh(&app).await),
    }
}

/// 立即刷新并返回新快照
#[tauri::command]
pub async fn refresh(app: AppHandle, monitor: State<'_, Monitor>) -> Result<Snapshot, ()> {
    Ok(monitor.refresh(&app).await)
}

/// 测试到某台设备的实际路径和延迟。“全部测试”由前端并发调用
#[tauri::command]
pub async fn ping(
    ip: String,
    app: AppHandle,
    monitor: State<'_, Monitor>,
) -> Result<PingResult, CliError> {
    let regions = monitor.regions().await;
    let mut result = cli::ping(&ip, &regions, &monitor.local()).await?;
    // 手动测试的结果也记入延迟历史
    if let Some(id) = monitor.device_id_for_ip(&ip) {
        crate::monitor::record_history(
            &app,
            vec![(id, Sample::from_ping(crate::monitor::now_ms(), &result))],
        );
    }
    if let Some(conn) = result.conn.take() {
        result.conn = Some(monitor.explain_ping(&ip, conn).await);
    }
    Ok(result)
}

/// 全部设备最近 24 小时的延迟历史（设备卡片的小图）
#[tauri::command]
pub fn get_history(store: State<'_, HistoryStore>) -> History {
    store.recent(crate::monitor::now_ms() - DAY_MS)
}

/// 一台设备最近 7 天的延迟历史（详情弹窗用）
#[tauri::command]
pub fn get_device_history(id: String, store: State<'_, HistoryStore>) -> Vec<Sample> {
    store.device(&id)
}

/// 把一台设备最近 7 天的延迟历史导出成 CSV，保存到“下载”文件夹并在资源管理器中选中，返回文件路径
#[tauri::command]
pub fn export_history(
    id: String,
    app: AppHandle,
    store: State<'_, HistoryStore>,
    monitor: State<'_, Monitor>,
) -> Result<String, String> {
    use tauri_plugin_opener::OpenerExt;
    let samples = store.device(&id);
    if samples.is_empty() {
        return Err("这台设备还没有延迟历史".into());
    }
    let csv = crate::history::to_csv(&samples, |ms| {
        chrono::DateTime::from_timestamp_millis(ms)
            .map(|t| t.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_default()
    });
    // 文件名用设备名，去掉 Windows 文件名不允许的字符
    let name: String = monitor
        .device_name(&id)
        .unwrap_or(id)
        .chars()
        .map(|c| if r#"\/:*?"<>|"#.contains(c) { '_' } else { c })
        .collect();
    let dir = app
        .path()
        .download_dir()
        .map_err(|e| format!("找不到下载文件夹：{e}"))?;
    let path = dir.join(format!(
        "MeshDock延迟历史-{name}-{}.csv",
        chrono::Local::now().format("%Y%m%d-%H%M%S")
    ));
    std::fs::write(&path, csv).map_err(|e| {
        applog::error(format!("导出延迟历史失败：{e}"));
        format!("保存失败：{e}")
    })?;
    let _ = app.opener().reveal_item_in_dir(&path);
    Ok(path.display().to_string())
}

/// 本机网络体检，约 4 秒
#[tauri::command]
pub async fn run_netcheck(monitor: State<'_, Monitor>) -> Result<Report, CliError> {
    let regions = monitor.regions().await;
    let n = cli::netcheck().await?;
    Ok(netcheck::report(
        &n,
        &regions,
        &monitor.local(),
        crate::monitor::now_ms(),
    ))
}

/// 读取本机设置
#[tauri::command]
pub fn get_settings(store: State<'_, SettingsStore>) -> Settings {
    store.get()
}

/// 取走托盘发起的模式切换请求（没有时返回 None）
#[tauri::command]
pub fn take_mode_request(pending: State<'_, PendingMode>) -> Option<bool> {
    pending.take()
}

/// 切换在家 / 出门模式，切换后重新读取设置确认已生效
#[tauri::command]
pub async fn set_accept_routes(
    on: bool,
    app: AppHandle,
    monitor: State<'_, Monitor>,
) -> Result<Snapshot, CliError> {
    let mode = if on { "出门模式" } else { "在家模式" };
    if let Err(e) = cli::set_accept_routes(on).await {
        applog::error(format!("切换到{mode}失败：{}\n{}", e.message, e.raw));
        return Err(e);
    }
    applog::info(format!("已执行切换到{mode}"));
    let snapshot = monitor.refresh_reread(&app).await;
    let actual = snapshot.overview.as_ref().and_then(|o| o.accept_routes);
    if actual != Some(on) {
        applog::error(format!("切换到{mode}后读取到 accept-routes={actual:?}"));
        return Err(CliError {
            kind: ErrorKind::Failed,
            message: "已执行切换，但重新读取到的设置没有变化，请稍后刷新再看".into(),
            raw: format!("期望 accept-routes={on}，读取到 {actual:?}"),
        });
    }
    Ok(snapshot)
}

/// 选用出口设备（传设备 id），None 表示停用出口；执行后重新读取确认已生效
#[tauri::command]
pub async fn set_exit_node(
    id: Option<String>,
    app: AppHandle,
    monitor: State<'_, Monitor>,
) -> Result<Snapshot, CliError> {
    apply_exit_node(&app, &monitor, id).await.map(|(_, s)| s)
}

/// 选用或停用出口（界面和托盘菜单共用），成功时返回给用户看的结果和新快照
pub async fn apply_exit_node(
    app: &AppHandle,
    monitor: &Monitor,
    id: Option<String>,
) -> Result<(String, Snapshot), CliError> {
    let target = match &id {
        Some(id) => Some(monitor.exit_candidate(id).ok_or_else(|| CliError {
            kind: ErrorKind::Failed,
            message: "这台设备现在不能当出口，请刷新后再试".into(),
            raw: id.clone(),
        })?),
        None => None,
    };
    let (what, done) = match &target {
        Some((name, _)) => (format!("选用出口 {name}"), format!("已改为经 {name} 上网")),
        None => ("停用出口".to_string(), "已停用出口".to_string()),
    };
    if let Err(e) = cli::set_exit_node(target.as_ref().map(|(_, ip)| ip.as_str())).await {
        applog::error(format!("{what}失败：{}\n{}", e.message, e.raw));
        return Err(e);
    }
    applog::info(format!("已执行{what}"));
    let snapshot = monitor.refresh_reread(app).await;
    let actual = snapshot
        .overview
        .as_ref()
        .and_then(|o| o.prefs.as_ref())
        .map(|p| p.exit_node_id.clone());
    if actual != Some(id.clone()) {
        applog::error(format!("{what}后读取到出口为 {actual:?}"));
        return Err(CliError {
            kind: ErrorKind::Failed,
            message: "已执行，但重新读取到的出口没有变化，请稍后刷新再看".into(),
            raw: format!("期望 {id:?}，读取到 {actual:?}"),
        });
    }
    Ok((done, snapshot))
}

/// 开关本机组网选项（出口时访问局域网、组网 DNS、挡住传入连接、无人登录时保持连接）
#[tauri::command]
pub async fn set_pref_switch(
    switch: cli::PrefSwitch,
    on: bool,
    app: AppHandle,
    monitor: State<'_, Monitor>,
) -> Result<Snapshot, CliError> {
    let what = format!("{}“{}”", if on { "开启" } else { "关闭" }, switch.label());
    if let Err(e) = cli::set_switch(switch, on).await {
        applog::error(format!("{what}失败：{}\n{}", e.message, e.raw));
        return Err(e);
    }
    applog::info(format!("已执行{what}"));
    let snapshot = monitor.refresh_reread(&app).await;
    let actual = snapshot
        .overview
        .as_ref()
        .and_then(|o| o.prefs.as_ref())
        .and_then(|p| switch.read(p));
    if actual != Some(on) {
        applog::error(format!("{what}后读取到 {actual:?}"));
        return Err(CliError {
            kind: ErrorKind::Failed,
            message: "已执行，但重新读取到的设置没有变化，请稍后刷新再看".into(),
            raw: format!("期望 {on}，读取到 {actual:?}"),
        });
    }
    Ok(snapshot)
}

/// 发送文件的结果；用户取消选择时命令返回 None
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendResult {
    device: String,
    /// 发出的文件名（不含路径）
    files: Vec<String>,
}

/// 按设备 id 找到在线设备，返回名称和组网 IP；离线或不在组网中时报错
fn send_target(monitor: &Monitor, id: &str) -> Result<(String, String), CliError> {
    monitor.online_peer(id).ok_or_else(|| CliError {
        kind: ErrorKind::Failed,
        message: "设备离线或已不在组网中，不能发送".into(),
        raw: id.to_string(),
    })
}

/// 用 Taildrop 发送文件并记日志，返回发出的文件名
async fn send_paths(name: String, ip: &str, files: &[PathBuf]) -> Result<SendResult, CliError> {
    let names: Vec<String> = files
        .iter()
        .filter_map(|f| f.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .collect();
    applog::info(format!("开始向 {name} 发送 {} 个文件", files.len()));
    cli::send_files(files, ip).await.inspect_err(|e| {
        applog::error(format!("向 {name} 发送文件失败：{}\n{}", e.message, e.raw))
    })?;
    applog::info(format!("已向 {name} 发送：{}", names.join("、")));
    Ok(SendResult {
        device: name,
        files: names,
    })
}

/// 拖进窗口的文件多久内有效：前端收到拖放事件后马上会来取，过期的不再发送
const DROP_TTL: Duration = Duration::from_secs(10);

/// 最近一次拖进窗口的文件。拖放事件由后端记下路径，前端只告诉后端落在哪台设备上，文件路径不经过前端
#[derive(Default)]
pub struct DroppedFiles(Mutex<Option<(Instant, Vec<PathBuf>)>>);

impl DroppedFiles {
    pub fn set(&self, paths: Vec<PathBuf>) {
        *self.0.lock().unwrap() = Some((Instant::now(), paths));
    }

    /// 取走未过期的文件；取过一次就清空，避免重复发送
    fn take(&self) -> Option<Vec<PathBuf>> {
        let (at, paths) = self.0.lock().unwrap().take()?;
        (at.elapsed() < DROP_TTL).then_some(paths)
    }
}

/// Taildrop 只能发文件：拖进来的有文件夹（或已不存在）时报错
fn check_dropped(paths: &[PathBuf]) -> Result<(), String> {
    if paths.is_empty() {
        return Err("没有拖入文件".into());
    }
    match paths.iter().find(|p| !p.is_file()) {
        Some(p) if p.is_dir() => Err(format!(
            "只能发送文件，不能发送文件夹（{}）",
            p.file_name().unwrap_or_default().to_string_lossy()
        )),
        Some(p) => Err(format!("找不到文件 {}", p.display())),
        None => Ok(()),
    }
}

/// 把刚拖到设备卡片上的文件用 Taildrop 发给这台设备
#[tauri::command]
pub async fn send_dropped(
    id: String,
    monitor: State<'_, Monitor>,
    dropped: State<'_, DroppedFiles>,
) -> Result<SendResult, CliError> {
    let files = dropped.take().ok_or_else(|| CliError {
        kind: ErrorKind::Failed,
        message: "没有找到刚拖入的文件，请重新拖一次".into(),
        raw: String::new(),
    })?;
    check_dropped(&files).map_err(|message| CliError {
        kind: ErrorKind::Failed,
        message,
        raw: String::new(),
    })?;
    let (name, ip) = send_target(&monitor, &id)?;
    send_paths(name, &ip, &files).await
}

/// 弹出选文件对话框，把选中的文件用 Taildrop 发给设备。
/// 文件路径只在后端取得，不经过前端
#[tauri::command]
pub async fn send_files(
    id: String,
    app: AppHandle,
    monitor: State<'_, Monitor>,
) -> Result<Option<SendResult>, CliError> {
    use tauri_plugin_dialog::DialogExt;
    let (name, ip) = send_target(&monitor, &id)?;
    let (tx, rx) = tokio::sync::oneshot::channel();
    let mut dialog = app.dialog().file().set_title(format!("选择要发给 {name} 的文件"));
    if let Some(window) = app.get_webview_window("main") {
        dialog = dialog.set_parent(&window);
    }
    dialog.pick_files(move |picked| {
        let _ = tx.send(picked);
    });
    let Some(picked) = rx.await.ok().flatten() else {
        return Ok(None);
    };
    let files: Vec<PathBuf> = picked
        .into_iter()
        .filter_map(|f| f.into_path().ok())
        .collect();
    send_paths(name, &ip, &files).await.map(Some)
}

/// 本机当前的共享规则（tailscale serve）
#[tauri::command]
pub async fn get_serve_rules() -> Result<Vec<crate::model::ServeRule>, CliError> {
    cli::serve_rules().await
}

/// 新增共享规则，返回新的规则列表
#[tauri::command]
pub async fn add_serve_rule(
    kind: cli::ServeKind,
    port: u16,
    local_port: u16,
) -> Result<Vec<crate::model::ServeRule>, CliError> {
    let what = format!("共享本机端口 {local_port}（组网端口 {port}，{kind:?}）");
    cli::serve_add(kind, port, local_port)
        .await
        .inspect_err(|e| applog::error(format!("{what}失败：{}\n{}", e.message, e.raw)))?;
    applog::info(format!("已{what}"));
    cli::serve_rules().await
}

/// 删除共享规则，返回新的规则列表
#[tauri::command]
pub async fn remove_serve_rule(
    kind: String,
    port: u16,
) -> Result<Vec<crate::model::ServeRule>, CliError> {
    cli::serve_remove(&kind, port).await.inspect_err(|e| {
        applog::error(format!("停止共享 {kind} {port} 失败：{}\n{}", e.message, e.raw))
    })?;
    applog::info(format!("已停止共享 {kind} {port}"));
    cli::serve_rules().await
}

/// 已登录的账户列表
#[tauri::command]
pub async fn get_profiles() -> Result<Vec<cli::Profile>, CliError> {
    cli::profiles().await
}

/// 切换到另一个已登录的账户，返回切换后的快照。前端已做二次确认
#[tauri::command]
pub async fn switch_profile(
    id: String,
    app: AppHandle,
    monitor: State<'_, Monitor>,
) -> Result<Snapshot, CliError> {
    cli::switch_profile(&id)
        .await
        .inspect_err(|e| applog::error(format!("切换账户失败：{}\n{}", e.message, e.raw)))?;
    applog::info(format!("已切换到账户 {id}"));
    Ok(monitor.refresh_reread(&app).await)
}

/// 保存设置并立即生效：主题应用到窗口，开机自启同步到注册表
#[tauri::command]
pub fn save_settings(
    settings: Settings,
    app: AppHandle,
    store: State<'_, SettingsStore>,
) -> Result<Settings, String> {
    let old = store.get();
    if settings.launch_on_boot != old.launch_on_boot {
        // 先改注册表，失败就不保存这一项
        crate::sync_autostart(&app, settings.launch_on_boot).inspect_err(|e| applog::error(e))?;
    }
    let saved = store
        .update(|s| *s = settings)
        .inspect_err(|e| applog::error(e))?;
    let monitor = app.state::<Monitor>();
    monitor.set_relay_name(&saved.relay_name);
    if saved.refresh_secs != old.refresh_secs || saved.sample_minutes != old.sample_minutes {
        // 间隔改了，立即按新间隔重新计时
        monitor.intervals_changed();
    } else if saved.home_gateway != old.home_gateway
        || saved.at_home != old.at_home
        || saved.auto_mode != old.auto_mode
        || saved.relay_name != old.relay_name
    {
        // 家里网关、手动标记、自动切换或中继显示名变了，立即重新识别位置、刷新界面
        monitor.wake();
    }
    if saved.theme != old.theme
        && let Some(window) = app.get_webview_window("main")
    {
        let _ = window.set_theme(saved.theme.window_theme());
    }
    Ok(saved)
}

/// 设置页“关于”用的信息
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    version: String,
    settings_file: String,
    /// 程序旁有 data 目录（便携模式）
    portable: bool,
    /// 安装版（可自动更新）；免安装版只能到下载页下载
    installed: bool,
    /// 开发版：开机自启不会真正写入
    debug: bool,
}

#[tauri::command]
pub fn get_app_info(app: AppHandle, paths: State<'_, AppPaths>) -> AppInfo {
    AppInfo {
        version: app.package_info().version.to_string(),
        settings_file: paths.settings_file.display().to_string(),
        portable: paths.webview_dir.is_some(),
        installed: crate::app_update::installed(),
        debug: cfg!(debug_assertions),
    }
}

/// 在新终端中执行 `ssh <别名>`
#[tauri::command]
pub fn open_ssh(alias: String) -> Result<(), String> {
    crate::launch::open_ssh(&alias)
}

/// 经 ssh 只读采集 Linux 设备的系统信息（运行时长、内存、磁盘、IPv6、电池、端口）
#[tauri::command]
pub async fn device_sysinfo(alias: String) -> Result<crate::sysinfo::SysInfo, String> {
    crate::sysinfo::fetch(&alias).await
}

/// 读取官方客户端的无人值守和托盘自启：alias 为 None 读本机，否则经 ssh 读对方（Windows）
#[tauri::command]
pub async fn get_client_setup(
    alias: Option<String>,
) -> Result<crate::client_setup::ClientSetup, String> {
    crate::client_setup::get(alias.as_deref()).await
}

/// 开关官方托盘的开机自启：本机经 UAC 提权，对方经 ssh（需管理员账户）
#[tauri::command]
pub async fn set_tray_autostart(
    alias: Option<String>,
    on: bool,
) -> Result<crate::client_setup::ClientSetup, String> {
    let who = alias.as_deref().unwrap_or("本机");
    let result = match alias.as_deref() {
        None => crate::client_setup::set_local_tray(on).await,
        Some(a) => crate::client_setup::set_remote(a, None, Some(on)).await,
    };
    match &result {
        Ok(_) => crate::applog::info(format!("{who}：官方托盘开机自启已{}", on_off(on))),
        Err(e) => crate::applog::error(format!("{who}：修改官方托盘开机自启失败：{e}")),
    }
    result
}

/// 经 ssh 开关对方的无人值守（本机用组网选项里的开关）
#[tauri::command]
pub async fn set_remote_unattended(
    alias: String,
    on: bool,
) -> Result<crate::client_setup::ClientSetup, String> {
    let result = crate::client_setup::set_remote(&alias, Some(on), None).await;
    match &result {
        Ok(_) => crate::applog::info(format!("{alias}：无人值守已{}", on_off(on))),
        Err(e) => crate::applog::error(format!("{alias}：修改无人值守失败：{e}")),
    }
    result
}

fn on_off(on: bool) -> &'static str {
    if on { "开启" } else { "关闭" }
}

/// 在浏览器中打开 MeshDock 新版本的发布页（地址取自后端的检查结果，不由前端传入）
#[tauri::command]
pub fn open_app_release(app: AppHandle, monitor: State<'_, Monitor>) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let crate::app_update::UpdateState::Available(u) = monitor.app_update() else {
        return Err("现在没有可用的新版本".into());
    };
    app.opener()
        .open_url(u.url, None::<&str>)
        .map_err(|e| format!("打开发布页失败：{e}"))
}

/// 下载并安装新版本：成功时程序会退出并重新打开（安装版由安装程序打开，免安装版替换文件后自己重启），所以只有失败才会返回
#[tauri::command]
pub async fn install_app_update(app: AppHandle) -> Result<(), String> {
    crate::app_update::install(&app)
        .await
        .inspect_err(|e| applog::error(format!("自动更新失败：{e}")))
}

/// 用 RustDesk 直连设备的组网 IP（对方需在 RustDesk 中允许 IP 直接访问）
#[tauri::command]
pub fn open_rustdesk(ip: String, app: AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let url = crate::launch::rustdesk_url(&ip)?;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| format!("打开 RustDesk 失败：{e}"))
}

/// 诊断报告附带的日志行数
const REPORT_LOG_LINES: usize = 50;

/// 生成诊断报告（已打码）：先刷新状态，再读取 status、prefs、netcheck 的原始输出
#[tauri::command]
pub async fn build_report(app: AppHandle, monitor: State<'_, Monitor>) -> Result<String, String> {
    let snapshot = monitor.refresh(&app).await;
    let regions = monitor.regions().await;
    let (status, prefs, nc) = tokio::join!(
        cli::run_checked(&["status", "--json"]),
        cli::run_checked(&["debug", "prefs"]),
        cli::run_checked(&["netcheck", "--format=json"]),
    );
    let failed = |e: &CliError| format!("读取失败：{}\n{}", e.message, e.raw);
    let health = status
        .as_ref()
        .ok()
        .and_then(|o| serde_json::from_str::<crate::model::Status>(&o.stdout).ok())
        .and_then(|s| s.health)
        .unwrap_or_default();
    let nc_report = nc
        .as_ref()
        .ok()
        .and_then(|o| crate::model::parse_netcheck(&o.stdout).ok())
        .map(|n| netcheck::report(&n, &regions, &monitor.local(), crate::monitor::now_ms()));
    let input = report::Input {
        version: &app.package_info().version.to_string(),
        generated_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        overview: snapshot.overview.as_ref(),
        snapshot_error: snapshot.error.as_ref().map(|e| e.message.as_str()),
        netcheck: nc_report.as_ref(),
        health,
        raw_status: status.as_ref().map_or_else(failed, |o| o.stdout.clone()),
        raw_prefs: prefs
            .as_ref()
            .map_or_else(failed, |o| report::strip_prefs(&o.stdout)),
        raw_netcheck: nc.as_ref().map_or_else(failed, |o| o.text()),
        recent_log: applog::tail(REPORT_LOG_LINES),
    };
    Ok(report::compose(&input))
}

/// 把报告保存到“下载”文件夹并在资源管理器中选中，返回文件路径
#[tauri::command]
pub fn save_report(text: String, app: AppHandle) -> Result<String, String> {
    use tauri_plugin_opener::OpenerExt;
    let dir = app
        .path()
        .download_dir()
        .map_err(|e| format!("找不到下载文件夹：{e}"))?;
    let name = format!(
        "MeshDock诊断报告-{}.txt",
        chrono::Local::now().format("%Y%m%d-%H%M%S")
    );
    let path = dir.join(name);
    std::fs::write(&path, text).map_err(|e| {
        applog::error(format!("保存诊断报告失败：{e}"));
        format!("保存失败：{e}")
    })?;
    let _ = app.opener().reveal_item_in_dir(&path);
    Ok(path.display().to_string())
}

/// 连接组网（tailscale up，不改动现有设置），返回刷新后的快照
#[tauri::command]
pub async fn connect(app: AppHandle, monitor: State<'_, Monitor>) -> Result<Snapshot, CliError> {
    cli::up()
        .await
        .inspect_err(|e| applog::error(format!("连接组网失败：{}\n{}", e.message, e.raw)))?;
    applog::info("已连接组网");
    Ok(monitor.refresh_reread(&app).await)
}

/// 断开组网（tailscale down），返回刷新后的快照。前端已做二次确认
#[tauri::command]
pub async fn disconnect(app: AppHandle, monitor: State<'_, Monitor>) -> Result<Snapshot, CliError> {
    cli::down()
        .await
        .inspect_err(|e| applog::error(format!("断开组网失败：{}\n{}", e.message, e.raw)))?;
    applog::info("已断开组网");
    Ok(monitor.refresh_reread(&app).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 拖入的只能是文件() {
        let dir = std::env::temp_dir().join(format!("meshdock-drop-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.txt");
        std::fs::write(&file, "x").unwrap();
        assert!(check_dropped(std::slice::from_ref(&file)).is_ok());
        assert!(check_dropped(&[]).is_err());
        let err = check_dropped(&[file.clone(), dir.clone()]).unwrap_err();
        assert!(err.contains("文件夹"), "{err}");
        assert!(check_dropped(&[dir.join("没有这个文件")]).unwrap_err().contains("找不到"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn 拖入的文件只能取一次() {
        let dropped = DroppedFiles::default();
        assert_eq!(dropped.take(), None);
        dropped.set(vec![PathBuf::from("a")]);
        assert_eq!(dropped.take(), Some(vec![PathBuf::from("a")]));
        assert_eq!(dropped.take(), None);
    }
}
