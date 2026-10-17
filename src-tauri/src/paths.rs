//! 数据文件位置：安装版放在 %APPDATA%，便携版放在程序旁的 data 目录
use serde::de::DeserializeOwned;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

pub struct AppPaths {
    pub settings_file: PathBuf,
    /// 24 小时延迟历史
    pub history_file: PathBuf,
    /// 日志文件 meshdock.log
    pub log_file: PathBuf,
    /// WebView2 数据目录；None 表示使用默认位置（%LOCALAPPDATA%\com.meshdock.app）
    pub webview_dir: Option<PathBuf>,
}

impl AppPaths {
    /// 程序所在目录存在 data 子目录时进入便携模式，所有数据都写在其中
    pub fn resolve(app: &AppHandle) -> tauri::Result<Self> {
        let portable_dir = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|dir| dir.join("data")))
            .filter(|dir| dir.is_dir());
        if let Some(dir) = portable_dir {
            return Ok(Self {
                settings_file: dir.join("settings.json"),
                history_file: dir.join("history.json"),
                log_file: dir.join("meshdock.log"),
                webview_dir: Some(dir.join("webview")),
            });
        }
        Ok(Self {
            settings_file: app.path().app_config_dir()?.join("settings.json"),
            history_file: app.path().app_data_dir()?.join("history.json"),
            // %LOCALAPPDATA%\com.meshdock.app\logs
            log_file: app.path().app_log_dir()?.join("meshdock.log"),
            webview_dir: None,
        })
    }
}

/// 原子写入：先写到同目录的临时文件并刷到磁盘，再改名替换原文件。
/// 写到一半断电或被结束时，原文件保持完整
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = with_suffix(path, ".tmp");
    let mut f = std::fs::File::create(&tmp)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    drop(f);
    // Windows 上 rename 会替换已存在的目标文件
    std::fs::rename(&tmp, path)
}

/// 读取 JSON 文件。文件不存在返回 None；内容损坏时把它改名为 .bak 保留下来、记入日志，再返回 None
pub fn load_json<T: DeserializeOwned>(path: &Path) -> Option<T> {
    let text = std::fs::read_to_string(path).ok()?;
    match serde_json::from_str(&text) {
        Ok(v) => Some(v),
        Err(e) => {
            let bak = with_suffix(path, ".bak");
            let _ = std::fs::rename(path, &bak);
            crate::applog::error(format!(
                "{} 内容损坏，已改名为 {} 并使用默认值：{e}",
                path.display(),
                bak.display()
            ));
            None
        }
    }
}

/// settings.json → settings.json.tmp
fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("meshdock-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn 原子写入覆盖旧文件且不留临时文件() {
        let dir = temp_dir("atomic");
        let path = dir.join("a.json");
        write_atomic(&path, b"1").unwrap();
        write_atomic(&path, b"2").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "2");
        assert!(!dir.join("a.json.tmp").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn 损坏的文件改名为_bak() {
        let dir = temp_dir("corrupt");
        let path = dir.join("a.json");
        assert_eq!(load_json::<u32>(&path), None); // 不存在
        std::fs::write(&path, "不是 JSON").unwrap();
        assert_eq!(load_json::<u32>(&path), None);
        assert!(!path.exists());
        assert_eq!(
            std::fs::read_to_string(dir.join("a.json.bak")).unwrap(),
            "不是 JSON"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
