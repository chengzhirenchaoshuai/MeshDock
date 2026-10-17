//! 本机设置：保存在应用配置目录的 settings.json，只存界面偏好，不存任何密钥
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// 手动标记“我现在在家”：只在读不到网关、无法自动识别时使用
    pub at_home: Option<bool>,
    /// 家里路由器的地址：默认网关等于它即为在家
    pub home_gateway: String,
    /// 外观：跟随系统 / 浅色 / 深色
    pub theme: ThemeMode,
    /// 开机自启，默认关闭
    pub launch_on_boot: bool,
    /// 路径变化、模式开错时发 Windows 通知，默认开启
    pub notify: bool,
    /// 点窗口关闭按钮时最小化到托盘（默认开启）；关闭则直接退出程序
    pub close_to_tray: bool,
    /// 窗口打开时的状态刷新间隔（秒），只能取 REFRESH_CHOICES 中的值
    pub refresh_secs: u64,
    /// 后台测速间隔（分钟），只能取 SAMPLE_CHOICES 中的值
    pub sample_minutes: u64,
}

/// 状态刷新间隔的可选值（秒）
pub const REFRESH_CHOICES: [u64; 4] = [5, 10, 30, 60];
/// 后台测速间隔的可选值（分钟）。最长 10 分钟：延迟小图把相隔超过 15 分钟的两次测量画成断开
pub const SAMPLE_CHOICES: [u64; 4] = [1, 2, 5, 10];
/// 窗口隐藏时只需更新托盘，刷新间隔至少这么长
const HIDDEN_MIN_SECS: u64 = 30;

impl Default for Settings {
    fn default() -> Self {
        Self {
            at_home: None,
            home_gateway: "192.168.66.66".into(),
            theme: ThemeMode::default(),
            launch_on_boot: false,
            notify: true,
            close_to_tray: true,
            refresh_secs: 10,
            sample_minutes: 5,
        }
    }
}

impl Settings {
    /// 把不在可选范围内的间隔改回默认值（设置文件被手动改过、或前端传来意外的值时）
    pub fn sanitized(mut self) -> Self {
        let default = Self::default();
        if !REFRESH_CHOICES.contains(&self.refresh_secs) {
            self.refresh_secs = default.refresh_secs;
        }
        if !SAMPLE_CHOICES.contains(&self.sample_minutes) {
            self.sample_minutes = default.sample_minutes;
        }
        self
    }

    /// 状态刷新间隔：窗口打开时按设置，隐藏时至少 30 秒
    pub fn refresh_interval(&self, window_visible: bool) -> Duration {
        let secs = if window_visible {
            self.refresh_secs
        } else {
            self.refresh_secs.max(HIDDEN_MIN_SECS)
        };
        Duration::from_secs(secs)
    }

    /// 后台测速间隔
    pub fn sample_interval(&self) -> Duration {
        Duration::from_secs(self.sample_minutes * 60)
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeMode {
    /// 对应的窗口主题；跟随系统时返回 None，由系统决定（网页的 prefers-color-scheme 也随之变化）
    pub fn window_theme(self) -> Option<tauri::Theme> {
        match self {
            Self::System => None,
            Self::Light => Some(tauri::Theme::Light),
            Self::Dark => Some(tauri::Theme::Dark),
        }
    }
}

pub struct SettingsStore {
    path: PathBuf,
    current: Mutex<Settings>,
}

impl SettingsStore {
    /// 读取设置；文件不存在或损坏时使用默认值
    pub fn load(path: PathBuf) -> Self {
        let current = crate::paths::load_json::<Settings>(&path)
            .unwrap_or_default()
            .sanitized();
        Self {
            path,
            current: Mutex::new(current),
        }
    }

    pub fn get(&self) -> Settings {
        self.current.lock().unwrap().clone()
    }

    /// 修改并立即写盘
    pub fn update(&self, f: impl FnOnce(&mut Settings)) -> Result<Settings, String> {
        let mut current = self.current.lock().unwrap();
        // 先在副本上修改，写盘成功后才替换内存中的设置，避免两者不一致
        let mut next = current.clone();
        f(&mut next);
        let next = next.sanitized();
        let text = serde_json::to_string_pretty(&next).map_err(|e| e.to_string())?;
        crate::paths::write_atomic(&self.path, text.as_bytes())
            .map_err(|e| format!("保存设置失败：{e}"))?;
        *current = next.clone();
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 保存后重新读取() {
        let path = std::env::temp_dir().join(format!("meshdock-test-{}.json", std::process::id()));
        let store = SettingsStore::load(path.clone());
        assert_eq!(store.get().at_home, None);
        store.update(|s| s.at_home = Some(true)).unwrap();
        assert_eq!(SettingsStore::load(path.clone()).get().at_home, Some(true));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn 旧版设置文件缺少新字段时取默认值() {
        let s: Settings = serde_json::from_str(r#"{ "atHome": true }"#).unwrap();
        assert_eq!(s.at_home, Some(true));
        assert_eq!(s.theme, ThemeMode::System);
        assert!(!s.launch_on_boot);
        assert_eq!(s.home_gateway, "192.168.66.66");
        assert!(s.notify);
        assert_eq!((s.refresh_secs, s.sample_minutes), (10, 5));
        assert!(s.close_to_tray, "旧设置文件没有这一项时默认最小化到托盘");
        let s: Settings = serde_json::from_str(r#"{ "theme": "dark" }"#).unwrap();
        assert_eq!(s.theme, ThemeMode::Dark);
    }

    #[test]
    fn 间隔不在可选范围时改回默认值() {
        let s: Settings =
            serde_json::from_str(r#"{ "refreshSecs": 1, "sampleMinutes": 999 }"#).unwrap();
        let s = s.sanitized();
        assert_eq!((s.refresh_secs, s.sample_minutes), (10, 5));
        let s: Settings =
            serde_json::from_str(r#"{ "refreshSecs": 60, "sampleMinutes": 1 }"#).unwrap();
        let s = s.sanitized();
        assert_eq!((s.refresh_secs, s.sample_minutes), (60, 1));
        assert_eq!(s.sample_interval(), Duration::from_secs(60));
    }

    #[test]
    fn 窗口隐藏时刷新间隔至少_30_秒() {
        let mut s = Settings {
            refresh_secs: 5,
            ..Default::default()
        };
        assert_eq!(s.refresh_interval(true), Duration::from_secs(5));
        assert_eq!(s.refresh_interval(false), Duration::from_secs(30));
        s.refresh_secs = 60;
        assert_eq!(s.refresh_interval(false), Duration::from_secs(60));
    }

    #[test]
    fn 文件损坏时用默认值() {
        let path = std::env::temp_dir().join(format!("meshdock-bad-{}.json", std::process::id()));
        std::fs::write(&path, "不是 JSON").unwrap();
        assert_eq!(SettingsStore::load(path.clone()).get().at_home, None);
        // 坏文件被改名保留
        let bak = std::path::PathBuf::from(format!("{}.bak", path.display()));
        assert!(!path.exists() && bak.exists());
        std::fs::remove_file(bak).unwrap();
    }
}
