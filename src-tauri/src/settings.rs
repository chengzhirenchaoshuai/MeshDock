//! 本机设置：保存在应用配置目录的 settings.json，只存界面偏好，不存任何密钥
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// 用户手动标记“我现在在家”：true 在家，false 在外，None 没标记过（P1 改为自动识别网关）
    pub at_home: Option<bool>,
    /// 外观：跟随系统 / 浅色 / 深色
    pub theme: ThemeMode,
    /// 开机自启，默认关闭
    pub launch_on_boot: bool,
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
        let current = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
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
        f(&mut current);
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("创建配置目录失败：{e}"))?;
        }
        let text = serde_json::to_string_pretty(&*current).map_err(|e| e.to_string())?;
        std::fs::write(&self.path, text).map_err(|e| format!("保存设置失败：{e}"))?;
        Ok(current.clone())
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
        let s: Settings = serde_json::from_str(r#"{ "theme": "dark" }"#).unwrap();
        assert_eq!(s.theme, ThemeMode::Dark);
    }

    #[test]
    fn 文件损坏时用默认值() {
        let path = std::env::temp_dir().join(format!("meshdock-bad-{}.json", std::process::id()));
        std::fs::write(&path, "不是 JSON").unwrap();
        assert_eq!(SettingsStore::load(path.clone()).get().at_home, None);
        std::fs::remove_file(path).unwrap();
    }
}
