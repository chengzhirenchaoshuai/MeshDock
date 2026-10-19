//! 暴露给前端的命令
use crate::applog;
use crate::classify::PingResult;
use crate::cli::{self, CliError, ErrorKind};
use crate::history::{History, HistoryStore, Sample};
use crate::monitor::{Monitor, Snapshot};
use crate::netcheck::{self, Report};
use crate::paths::AppPaths;
use crate::report;
use crate::settings::{Settings, SettingsStore};
use crate::tray::PendingMode;
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

/// 全部设备最近 24 小时的延迟历史
#[tauri::command]
pub fn get_history(store: State<'_, HistoryStore>) -> History {
    store.get()
}

/// 本机网络体检，约 4 秒
#[tauri::command]
pub async fn run_netcheck(monitor: State<'_, Monitor>) -> Result<Report, CliError> {
    let regions = monitor.regions().await;
    let n = cli::netcheck().await?;
    Ok(netcheck::report(&n, &regions, crate::monitor::now_ms()))
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
    if saved.refresh_secs != old.refresh_secs || saved.sample_minutes != old.sample_minutes {
        // 间隔改了，立即按新间隔重新计时
        monitor.intervals_changed();
    } else if saved.home_gateway != old.home_gateway || saved.at_home != old.at_home {
        // 家里网关或手动标记变了，立即重新识别位置
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
    /// 开发版：开机自启不会真正写入
    debug: bool,
}

#[tauri::command]
pub fn get_app_info(app: AppHandle, paths: State<'_, AppPaths>) -> AppInfo {
    AppInfo {
        version: app.package_info().version.to_string(),
        settings_file: paths.settings_file.display().to_string(),
        portable: paths.webview_dir.is_some(),
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
        .map(|n| netcheck::report(&n, &regions, crate::monitor::now_ms()));
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
