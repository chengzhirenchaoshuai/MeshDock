//! 免安装版的通知身份：在当前用户注册表登记应用标识的显示名和图标，
//! 否则 Windows 通知只显示一串标识（如 com.meshdock.app）且没有图标。
//! 安装版由安装程序创建的开始菜单快捷方式登记身份，不需要这一步
use std::path::{Path, PathBuf};

/// 通知中显示的名称
const DISPLAY_NAME: &str = "MeshDock";
/// 通知图标
const ICON: &[u8] = include_bytes!("../icons/128x128.png");

/// 程序旁有安装程序生成的 uninstall.exe 即为安装版
fn is_installed(exe: &Path) -> bool {
    exe.parent()
        .is_some_and(|d| d.join("uninstall.exe").is_file())
}

/// 免安装版启动时调用：写出图标文件并登记显示名和图标。开发环境（target 目录）不登记，
/// 因为通知插件在那里本来就使用 PowerShell 的身份
pub fn register_if_portable(identifier: &str, icon_dir: &Path) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let in_target = exe
        .parent()
        .is_some_and(|d| d.ends_with("target\\debug") || d.ends_with("target\\release"));
    if in_target || is_installed(&exe) {
        return Ok(());
    }
    std::fs::create_dir_all(icon_dir).map_err(|e| format!("创建目录失败：{e}"))?;
    let icon: PathBuf = icon_dir.join("notify-icon.png");
    std::fs::write(&icon, ICON).map_err(|e| format!("写入通知图标失败：{e}"))?;
    let key = format!("Software\\Classes\\AppUserModelId\\{identifier}");
    set_value(&key, "DisplayName", DISPLAY_NAME)?;
    set_value(&key, "IconUri", &icon.display().to_string())
}

/// 在 HKEY_CURRENT_USER 下写一个字符串值（键不存在时自动创建）
#[cfg(windows)]
fn set_value(key: &str, name: &str, value: &str) -> Result<(), String> {
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, REG_SZ, RegSetKeyValueW};
    let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    let (key_w, name_w, value_w) = (wide(key), wide(name), wide(value));
    // SAFETY: 三个字符串都以 0 结尾，数据长度按字节计（含结尾 0）
    let status = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            key_w.as_ptr(),
            name_w.as_ptr(),
            REG_SZ,
            value_w.as_ptr().cast(),
            (value_w.len() * 2) as u32,
        )
    };
    if status == 0 {
        Ok(())
    } else {
        Err(format!("写入注册表失败（错误码 {status}）"))
    }
}

#[cfg(not(windows))]
fn set_value(_key: &str, _name: &str, _value: &str) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 旁边有卸载程序即为安装版() {
        let dir = std::env::temp_dir().join(format!("meshdock-aumid-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("MeshDock.exe");
        assert!(!is_installed(&exe));
        std::fs::write(dir.join("uninstall.exe"), b"").unwrap();
        assert!(is_installed(&exe));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
