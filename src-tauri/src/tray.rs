//! 托盘：图标右下角的圆点反映整体状态；左键单击打开主界面，右键弹出中文菜单
use crate::classify::ConnKind;
use crate::monitor::{Monitor, Overview};
use crate::window;
use std::sync::Mutex;
use tauri::{
    AppHandle, Emitter, Manager, Wry,
    image::Image,
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use tauri_plugin_notification::NotificationExt;

const TRAY_ID: &str = "main";

/// 托盘菜单请求切换模式时发给前端的事件，前端收到后弹出确认框
pub const MODE_REQUEST_EVENT: &str = "mode-request";

/// 托盘里的两个模式菜单项，以及当前的实际值（点击会自动改变勾选，需要按实际值改回）
struct ModeItems {
    home: CheckMenuItem<Wry>,
    away: CheckMenuItem<Wry>,
    current: Mutex<Option<bool>>,
}

/// 快捷操作子菜单里的一项（出口节点、SSH 连接）
#[derive(Clone, Debug, PartialEq)]
struct QuickItem {
    id: String,
    text: String,
    enabled: bool,
    /// None 为普通菜单项，Some 为带勾选的菜单项
    checked: Option<bool>,
}

/// 托盘里的两个快捷操作子菜单，以及上次生成时的内容（内容不变就不重建，避免每轮刷新都重建菜单）
struct QuickMenus {
    exit: Submenu<Wry>,
    ssh: Submenu<Wry>,
    last: Mutex<Option<(Vec<QuickItem>, Vec<QuickItem>)>>,
}

/// 菜单项 id 前缀：后面接设备 id 或 SSH 别名
const EXIT_PREFIX: &str = "exit:";
const EXIT_NONE: &str = "exit-none";
const SSH_PREFIX: &str = "ssh:";

/// 托盘发起、等待前端确认的模式切换：Some(true) 表示请求切到出门模式。
/// 窗口可能刚被创建、还没来得及监听事件，所以先存起来，由前端主动取走
#[derive(Default)]
pub struct PendingMode(Mutex<Option<bool>>);

impl PendingMode {
    pub fn take(&self) -> Option<bool> {
        self.0.lock().unwrap().take()
    }
}

/// 托盘底图：256×256 托盘专用图标（由 tray-icon.svg 渲染），按系统托盘尺寸缩小后再画状态圆点
const BASE_ICON: &[u8] = include_bytes!("../icons/tray-256.png");

/// 系统托盘图标的实际像素尺寸，随显示缩放变化（100% 为 16，125% 为 20，150% 为 24）。
/// 直接按这个尺寸生成图标，Windows 不再二次缩放，边缘清晰
#[cfg(windows)]
fn tray_size() -> u32 {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSMICON};
    // SAFETY: 只读取系统度量值，没有指针参数
    let n = unsafe { GetSystemMetrics(SM_CXSMICON) };
    if (16..=256).contains(&n) {
        n as u32
    } else {
        32
    }
}

#[cfg(not(windows))]
fn tray_size() -> u32 {
    32
}

/// 组网的整体状态，决定托盘圆点颜色和悬停提示
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TrayState {
    /// 刚启动、还没拿到状态：不画圆点
    Unknown,
    /// 已连接组网
    Connected,
    /// 组网已断开或服务没运行
    Disconnected,
    /// 本机正常，但连不上控制端（Headscale）
    ControlUnreachable,
}

impl TrayState {
    /// 圆点颜色（RGB）；托盘不随主题变化，固定取中等亮度，浅色、深色任务栏都看得清
    fn dot_color(self) -> Option<[u8; 3]> {
        match self {
            Self::Unknown => None,
            Self::Connected => Some([0x22, 0xc5, 0x5e]),
            Self::Disconnected => Some([0x9c, 0xa3, 0xaf]),
            Self::ControlUnreachable => Some([0xef, 0x44, 0x44]),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Unknown => "正在获取状态…",
            Self::Connected => "已连接",
            Self::Disconnected => "未连接",
            Self::ControlUnreachable => "控制端连不上",
        }
    }
}

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    crate::applog::info(format!("托盘图标尺寸：{} 像素", tray_size()));
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon_for(TrayState::Unknown)?)
        .tooltip(tooltip_for(TrayState::Unknown))
        .menu(&build_menu(app)?)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => window::show(app),
            "refresh" => app.state::<Monitor>().wake(),
            "quit" => app.exit(0),
            "mode-home" => request_mode(app, false),
            "mode-away" => request_mode(app, true),
            EXIT_NONE => pick_exit(app, None),
            id => {
                if let Some(dev) = id.strip_prefix(EXIT_PREFIX) {
                    pick_exit(app, Some(dev.to_string()));
                } else if let Some(alias) = id.strip_prefix(SSH_PREFIX)
                    && let Err(e) = crate::launch::open_ssh(alias)
                {
                    show_notice(app, "打开 SSH 失败", &e);
                }
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                window::show(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

/// 移除托盘图标：程序被安装程序直接结束前调用，避免任务栏留下残影
pub fn remove(app: &AppHandle) {
    if let Some(tray) = app.remove_tray_by_id(TRAY_ID) {
        let _ = tray.set_visible(false);
    }
}

/// 更新托盘图标和悬停提示
pub fn set_state(app: &AppHandle, state: TrayState) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    match icon_for(state) {
        Ok(icon) => {
            let _ = tray.set_icon(Some(icon));
        }
        Err(e) => crate::applog::error(format!("生成托盘图标失败：{e}")),
    }
    let _ = tray.set_tooltip(Some(tooltip_for(state)));
}

/// 按 accept-routes 的实际值勾选模式菜单项；读不到时两项都不勾
pub fn set_mode(app: &AppHandle, accept_routes: Option<bool>) {
    let Some(items) = app.try_state::<ModeItems>() else {
        return;
    };
    *items.current.lock().unwrap() = accept_routes;
    let _ = items.home.set_checked(accept_routes == Some(false));
    let _ = items.away.set_checked(accept_routes == Some(true));
}

/// 按最新的组网概况更新“出口节点”“SSH 连接”子菜单；概况读不到或组网没在运行时禁用
pub fn set_quick(app: &AppHandle, overview: Option<&Overview>) {
    let Some(menus) = app.try_state::<QuickMenus>() else {
        return;
    };
    let running = overview.filter(|o| o.backend_state.as_deref() == Some("Running"));
    let next = (exit_items(running), ssh_items(running));
    let mut last = menus.last.lock().unwrap();
    if last.as_ref() == Some(&next) {
        return;
    }
    let exit_text = if running.is_some() && next.0.is_empty() {
        "出口节点（组网里没有设备提供）"
    } else {
        "出口节点"
    };
    let ssh_text = if running.is_some() && next.1.is_empty() {
        "SSH 连接（ssh 配置里没有对应的设备）"
    } else {
        "SSH 连接"
    };
    if let Err(e) = fill(app, &menus.exit, exit_text, &next.0)
        .and_then(|_| fill(app, &menus.ssh, ssh_text, &next.1))
    {
        crate::applog::error(format!("更新托盘菜单失败：{e}"));
        return;
    }
    *last = Some(next);
}

/// 清空子菜单后按 items 重新填充；没有内容时禁用整个子菜单
fn fill(app: &AppHandle, menu: &Submenu<Wry>, text: &str, items: &[QuickItem]) -> tauri::Result<()> {
    while menu.remove_at(0)?.is_some() {}
    for item in items {
        // Windows 菜单把 & 当作快捷键标记，名称里的 & 要写成 &&
        let label = item.text.replace('&', "&&");
        match item.checked {
            Some(checked) => menu.append(&CheckMenuItem::with_id(
                app,
                &item.id,
                label,
                item.enabled,
                checked,
                None::<&str>,
            )?)?,
            None => menu.append(&MenuItem::with_id(
                app,
                &item.id,
                label,
                item.enabled,
                None::<&str>,
            )?)?,
        }
    }
    menu.set_text(text)?;
    menu.set_enabled(!items.is_empty())
}

/// 出口节点子菜单：“不使用出口”加上每台能当出口的设备，勾选当前选用的；离线的设备置灰
fn exit_items(overview: Option<&Overview>) -> Vec<QuickItem> {
    let Some(o) = overview else {
        return Vec::new();
    };
    let options: Vec<_> = o.peers.iter().filter(|d| d.exit_option).collect();
    if options.is_empty() {
        return Vec::new();
    }
    // 读不到组网选项时不勾选任何一项
    let current = o.prefs.as_ref().map(|p| p.exit_node_id.as_deref());
    let mut items = vec![QuickItem {
        id: EXIT_NONE.into(),
        text: "不使用出口".into(),
        enabled: true,
        checked: Some(current == Some(None)),
    }];
    items.extend(options.into_iter().map(|d| {
        let offline = d.conn.kind == ConnKind::Offline;
        QuickItem {
            id: format!("{EXIT_PREFIX}{}", d.id),
            text: if offline {
                format!("{}（离线）", d.name)
            } else {
                d.name.clone()
            },
            enabled: !offline,
            checked: Some(current == Some(Some(d.id.as_str()))),
        }
    }));
    items
}

/// SSH 连接子菜单：ssh 配置里有别名的设备，离线的置灰
fn ssh_items(overview: Option<&Overview>) -> Vec<QuickItem> {
    let Some(o) = overview else {
        return Vec::new();
    };
    o.peers
        .iter()
        .filter_map(|d| {
            let alias = d.ssh_alias.as_ref()?;
            let offline = d.conn.kind == ConnKind::Offline;
            Some(QuickItem {
                id: format!("{SSH_PREFIX}{alias}"),
                text: format!("{}（ssh {alias}）{}", d.name, if offline { "离线" } else { "" }),
                enabled: !offline,
                checked: None,
            })
        })
        .collect()
}

/// 托盘选了出口：点击会自动改变勾选，先按实际值改回，再在后台执行，结果用通知告知
fn pick_exit(app: &AppHandle, id: Option<String>) {
    if let Some(menus) = app.try_state::<QuickMenus>() {
        *menus.last.lock().unwrap() = None;
    }
    let monitor = app.state::<Monitor>().inner().clone();
    let overview = monitor.latest().and_then(|s| s.overview);
    set_quick(app, overview.as_ref());
    let current = overview
        .as_ref()
        .and_then(|o| o.prefs.as_ref())
        .map(|p| p.exit_node_id.clone());
    if current == Some(id.clone()) {
        return; // 已经是这个选择
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        match crate::commands::apply_exit_node(&app, &monitor, id).await {
            Ok((done, _)) => show_notice(&app, &done, "可在 MeshDock 设备页的“出口节点”中查看。"),
            Err(e) => show_notice(&app, "切换出口失败", &e.message),
        }
    });
}

/// 托盘操作的结果：主窗口可能没打开，用 Windows 通知告知（是用户主动操作的反馈，不受通知开关影响）
fn show_notice(app: &AppHandle, title: &str, body: &str) {
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        crate::applog::error(format!("发送通知失败：{e}"));
    }
}

/// 托盘点了某个模式：先把勾选改回实际值，再打开主界面由用户确认
fn request_mode(app: &AppHandle, accept_routes: bool) {
    let current = app
        .try_state::<ModeItems>()
        .and_then(|items| *items.current.lock().unwrap());
    set_mode(app, current);
    if current == Some(accept_routes) {
        return; // 已经是这个模式
    }
    *app.state::<PendingMode>().0.lock().unwrap() = Some(accept_routes);
    window::show(app);
    let _ = app.emit(MODE_REQUEST_EVENT, ());
}

fn tooltip_for(state: TrayState) -> String {
    format!("MeshDock · {}", state.label())
}

fn build_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let menu = Menu::new(app)?;
    menu.append(&MenuItem::with_id(
        app,
        "show",
        "打开主界面",
        true,
        None::<&str>,
    )?)?;
    menu.append(&MenuItem::with_id(
        app,
        "refresh",
        "刷新",
        true,
        None::<&str>,
    )?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    // 勾选状态由 set_mode 按实际设置更新
    let home = CheckMenuItem::with_id(app, "mode-home", "在家模式", true, false, None::<&str>)?;
    let away = CheckMenuItem::with_id(app, "mode-away", "出门模式", true, false, None::<&str>)?;
    menu.append(&home)?;
    menu.append(&away)?;
    app.manage(ModeItems {
        home,
        away,
        current: Mutex::new(None),
    });
    // 快捷操作：内容由 set_quick 按每轮刷新的结果填充，拿到状态前禁用
    let exit = Submenu::with_id(app, "exit-menu", "出口节点", false)?;
    let ssh = Submenu::with_id(app, "ssh-menu", "SSH 连接", false)?;
    menu.append(&exit)?;
    menu.append(&ssh)?;
    app.manage(QuickMenus {
        exit,
        ssh,
        last: Mutex::new(None),
    });
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?)?;
    Ok(menu)
}

fn icon_for(state: TrayState) -> tauri::Result<Image<'static>> {
    let base = Image::from_bytes(BASE_ICON)?;
    let n = tray_size();
    let small = downscale(base.rgba(), base.width(), base.height(), n, n);
    let rgba = match state.dot_color() {
        Some(color) => draw_dot(&small, n, n, color),
        None => small,
    };
    Ok(Image::new_owned(rgba, n, n))
}

/// 面积平均缩小：每个目标像素取它覆盖的源像素按面积加权的平均值。
/// 颜色按 alpha 预乘后再平均，避免透明边缘发黑
fn downscale(src: &[u8], sw: u32, sh: u32, dw: u32, dh: u32) -> Vec<u8> {
    let (fx, fy) = (sw as f32 / dw as f32, sh as f32 / dh as f32);
    // 源像素 [p, p+1) 与区间 [lo, hi) 的重叠长度
    let overlap = |p: u32, lo: f32, hi: f32| (hi.min(p as f32 + 1.0) - lo.max(p as f32)).max(0.0);
    let mut out = vec![0u8; (dw * dh * 4) as usize];
    for dy in 0..dh {
        let (y0, y1) = (dy as f32 * fy, (dy + 1) as f32 * fy);
        for dx in 0..dw {
            let (x0, x1) = (dx as f32 * fx, (dx + 1) as f32 * fx);
            let mut acc = [0f32; 4];
            let mut area = 0f32;
            for sy in y0.floor() as u32..(y1.ceil() as u32).min(sh) {
                let wy = overlap(sy, y0, y1);
                for sx in x0.floor() as u32..(x1.ceil() as u32).min(sw) {
                    let w = wy * overlap(sx, x0, x1);
                    let i = ((sy * sw + sx) * 4) as usize;
                    let a = src[i + 3] as f32 / 255.0;
                    for c in 0..3 {
                        acc[c] += src[i + c] as f32 * a * w;
                    }
                    acc[3] += a * w;
                    area += w;
                }
            }
            let o = ((dy * dw + dx) * 4) as usize;
            if area > 0.0 && acc[3] > 0.0 {
                for c in 0..3 {
                    out[o + c] = (acc[c] / acc[3]).round() as u8;
                }
                out[o + 3] = (acc[3] / area * 255.0).round() as u8;
            }
        }
    }
    out
}

/// 在右下角画状态圆点，并在圆点外挖出一圈透明边，让圆点和底图分开（边缘做抗锯齿）
fn draw_dot(base: &[u8], w: u32, h: u32, color: [u8; 3]) -> Vec<u8> {
    let size = w.min(h) as f32;
    let r = size * 0.2; // 圆点半径：20 像素的托盘图标上直径 8 像素
    let gap = size * 0.06; // 透明边宽度
    let (cx, cy) = (w as f32 - r, h as f32 - r);
    let mut out = base.to_vec();
    for y in 0..h {
        for x in 0..w {
            let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
            let dot = (r - d + 0.5).clamp(0.0, 1.0);
            let cut = (r + gap - d + 0.5).clamp(0.0, 1.0);
            if cut == 0.0 {
                continue;
            }
            let i = ((y * w + x) * 4) as usize;
            // 底图先按透明边淡出，再把圆点按覆盖率叠上去（非预乘 alpha 合成）
            let a = out[i + 3] as f32 / 255.0 * (1.0 - cut);
            let out_a = dot + a * (1.0 - dot);
            for c in 0..3 {
                out[i + c] = if out_a > 0.0 {
                    ((color[c] as f32 * dot + out[i + c] as f32 * a * (1.0 - dot)) / out_a).round()
                        as u8
                } else {
                    0
                };
            }
            out[i + 3] = (out_a * 255.0).round() as u8;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 圆点中心为状态色且不透明() {
        let base = vec![255u8; 32 * 32 * 4];
        let out = draw_dot(&base, 32, 32, [1, 2, 3]);
        let i = ((27 * 32 + 27) * 4) as usize;
        assert_eq!(&out[i..i + 4], &[1, 2, 3, 255]);
    }

    #[test]
    fn 左上角保持原样() {
        let base: Vec<u8> = (0..32 * 32 * 4).map(|v| (v % 251) as u8).collect();
        let out = draw_dot(&base, 32, 32, [1, 2, 3]);
        assert_eq!(&out[..64], &base[..64]);
    }

    #[test]
    fn 缩小后纯色保持不变() {
        let base: Vec<u8> = [10u8, 120, 200, 255].repeat(256 * 256);
        let out = downscale(&base, 256, 256, 20, 20);
        assert_eq!(out.len(), 20 * 20 * 4);
        assert!(out.chunks(4).all(|p| p == [10, 120, 200, 255]));
    }

    #[test]
    fn 缩小时透明像素不把颜色拉黑() {
        // 2×1：左边不透明红色，右边全透明黑色 → 1×1 应为半透明的红色，而不是暗红
        let base = vec![255, 0, 0, 255, 0, 0, 0, 0];
        assert_eq!(downscale(&base, 2, 1, 1, 1), vec![255, 0, 0, 128]);
    }

    fn dev(id: &str, kind: ConnKind, exit: bool, alias: Option<&str>) -> crate::monitor::Device {
        crate::monitor::Device {
            id: id.into(),
            name: format!("{id}-名"),
            dns_name: None,
            ips: vec![],
            os: None,
            online: None,
            conn: crate::classify::Conn { kind, label: String::new(), hint: None },
            rx_bytes: None,
            tx_bytes: None,
            last_seen_ms: None,
            last_handshake_ms: None,
            key: None,
            ssh_alias: alias.map(str::to_string),
            exit_option: exit,
            exit_active: false,
        }
    }

    fn overview(peers: Vec<crate::monitor::Device>, exit: Option<Option<&str>>) -> Overview {
        Overview {
            backend_state: Some("Running".into()),
            control_unreachable: false,
            version: None,
            self_device: None,
            peers,
            accept_routes: None,
            prefs: exit.map(|id| crate::model::LocalPrefs {
                exit_node_id: id.map(str::to_string),
                ..Default::default()
            }),
            subnet_routes: vec![],
            location: Default::default(),
            rustdesk: false,
            routers: vec![],
            client_update: None,
            app_update: Default::default(),
        }
    }

    #[test]
    fn 出口子菜单_勾选当前出口_离线置灰() {
        let peers = vec![
            dev("a", ConnKind::Lan, true, None),
            dev("b", ConnKind::Offline, true, None),
            dev("c", ConnKind::Lan, false, None),
        ];
        let items = exit_items(Some(&overview(peers.clone(), Some(Some("a")))));
        let brief: Vec<_> = items
            .iter()
            .map(|i| (i.id.as_str(), i.enabled, i.checked))
            .collect();
        assert_eq!(
            brief,
            [
                ("exit-none", true, Some(false)),
                ("exit:a", true, Some(true)),
                ("exit:b", false, Some(false)),
            ]
        );
        assert_eq!(items[2].text, "b-名（离线）");
        // 没用出口时勾“不使用出口”；读不到组网选项时都不勾
        assert_eq!(exit_items(Some(&overview(peers.clone(), Some(None))))[0].checked, Some(true));
        assert!(exit_items(Some(&overview(peers, None))).iter().all(|i| i.checked == Some(false)));
        // 没有设备能当出口时整个子菜单为空
        assert!(exit_items(Some(&overview(vec![dev("c", ConnKind::Lan, false, None)], None))).is_empty());
    }

    #[test]
    fn ssh_子菜单只列有别名的设备() {
        let peers = vec![
            dev("a", ConnKind::Lan, false, Some("ming")),
            dev("b", ConnKind::Offline, false, Some("box")),
            dev("c", ConnKind::Lan, false, None),
        ];
        let items = ssh_items(Some(&overview(peers, None)));
        let brief: Vec<_> = items.iter().map(|i| (i.id.as_str(), i.enabled)).collect();
        assert_eq!(brief, [("ssh:ming", true), ("ssh:box", false)]);
        assert_eq!(items[1].text, "b-名（ssh box）离线");
        assert!(ssh_items(None).is_empty());
    }

    #[test]
    fn 每种状态都能生成图标() {
        for s in [
            TrayState::Unknown,
            TrayState::Connected,
            TrayState::Disconnected,
            TrayState::ControlUnreachable,
        ] {
            let icon = icon_for(s).unwrap();
            assert_eq!(
                icon.rgba().len(),
                (icon.width() * icon.height() * 4) as usize
            );
        }
    }
}
