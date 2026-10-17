//! 托盘：图标右下角的圆点反映整体状态；左键单击打开主界面，右键弹出中文菜单
use crate::monitor::Monitor;
use crate::window;
use std::sync::Mutex;
use tauri::{
    AppHandle, Emitter, Manager, Wry,
    image::Image,
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

const TRAY_ID: &str = "main";

/// 托盘菜单请求切换模式时发给前端的事件，前端收到后弹出确认框
pub const MODE_REQUEST_EVENT: &str = "mode-request";

/// 托盘里的两个模式菜单项，以及当前的实际值（点击会自动改变勾选，需要按实际值改回）
struct ModeItems {
    home: CheckMenuItem<Wry>,
    away: CheckMenuItem<Wry>,
    current: Mutex<Option<bool>>,
}

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
            _ => {}
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
