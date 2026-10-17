//! 本机日志：正式版没有控制台，错误和关键操作写到数据目录的 meshdock.log，供排查和诊断报告使用
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// 日志超过这个大小就换新文件，旧的改名为 meshdock.old.log（只保留一份）
const MAX_BYTES: u64 = 1024 * 1024;

static LOG: OnceLock<Mutex<PathBuf>> = OnceLock::new();

/// 指定日志文件并接管 panic：程序以 abort 方式结束前，先把出错位置写进日志
pub fn init(path: PathBuf) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = LOG.set(Mutex::new(path));
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_default();
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_default();
        error(format!("程序崩溃（{location}）：{message}"));
        default_hook(info);
    }));
}

pub fn info(msg: impl AsRef<str>) {
    write("信息", msg.as_ref());
}

pub fn error(msg: impl AsRef<str>) {
    write("错误", msg.as_ref());
}

fn write(level: &str, msg: &str) {
    let line = format!(
        "{} [{level}] {msg}\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    );
    // 开发版同时输出到控制台
    #[cfg(debug_assertions)]
    eprint!("{line}");
    let Some(lock) = LOG.get() else {
        return;
    };
    // panic 发生在持锁期间时锁会中毒，日志仍然照写
    let path = lock.lock().unwrap_or_else(|e| e.into_inner());
    rotate(&path);
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&*path) {
        let _ = f.write_all(line.as_bytes());
    }
}

/// 超过大小上限时把当前日志改名为 .old.log
fn rotate(path: &Path) {
    if std::fs::metadata(path).is_ok_and(|m| m.len() > MAX_BYTES) {
        let _ = std::fs::rename(path, path.with_extension("old.log"));
    }
}

/// 最近的若干行日志（诊断报告用）；还没有日志时返回空
pub fn tail(lines: usize) -> String {
    let Some(lock) = LOG.get() else {
        return String::new();
    };
    let path = lock.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(lines)..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 超过上限时换新文件() {
        let dir = std::env::temp_dir().join(format!("meshdock-log-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("meshdock.log");
        std::fs::write(&path, vec![b'x'; (MAX_BYTES + 1) as usize]).unwrap();
        rotate(&path);
        assert!(!path.exists());
        assert!(dir.join("meshdock.old.log").exists());
        // 没超过时不动
        std::fs::write(&path, "短").unwrap();
        rotate(&path);
        assert!(path.exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
