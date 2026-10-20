//! MeshDock 自身的新版本检查：仓库是私有的，匿名访问 GitHub 读不到发布信息，程序也不保存任何令牌，
//! 所以借用本机已登录的 GitHub 命令行 `gh release view` 查询最新正式版。没装或没登录 gh 时视为查不到，不提示
use serde::{Deserialize, Serialize};
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;

/// 发布 MeshDock 的仓库
const REPO: &str = "chengzhirenchaoshuai/MeshDock";
/// gh 查询的超时（含联网）
const TIMEOUT: Duration = Duration::from_secs(20);

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 有新版本时的信息
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdate {
    /// 新版本号，不带 v，如 0.5.0
    pub version: String,
    /// 发布页地址
    pub url: String,
}

/// 检查结果，显示在设置页“关于”里
#[derive(Serialize, Clone, Debug, PartialEq, Default)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum UpdateState {
    /// 还没查过
    #[default]
    Unknown,
    /// 已是最新版
    Latest,
    /// 有新版本
    Available(AppUpdate),
    /// 查不到（gh 没装、没登录、断网等）
    Unavailable { reason: String },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Release {
    tag_name: String,
    url: String,
}

/// 版本号 → 数字序列：v0.4.10 → [0, 4, 10]；预发布后缀（-beta 等）忽略
fn numbers(v: &str) -> Vec<u64> {
    v.trim()
        .trim_start_matches('v')
        .split(['-', '+'])
        .next()
        .unwrap_or_default()
        .split('.')
        .map(|p| p.parse().unwrap_or(0))
        .collect()
}

/// latest 比 current 新
fn is_newer(current: &str, latest: &str) -> bool {
    numbers(latest) > numbers(current)
}

/// 解析 `gh release view --json tagName,url` 的输出；比当前版本新时返回新版本信息
fn parse(json: &str, current: &str) -> Option<AppUpdate> {
    let r: Release = serde_json::from_str(json).ok()?;
    is_newer(current, &r.tag_name).then(|| AppUpdate {
        version: r.tag_name.trim_start_matches('v').to_string(),
        url: r.url,
    })
}

/// 查询最新正式版。Ok(None) 表示已是最新；gh 没装、没登录、断网等查不到时返回 Err（不提示，稍后重试）
pub async fn check(current: &str) -> Result<Option<AppUpdate>, String> {
    let mut cmd = Command::new("gh");
    cmd.args(["release", "view", "--repo", REPO, "--json", "tagName,url"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let out = match tokio::time::timeout(TIMEOUT, cmd.output()).await {
        Err(_) => return Err("gh 查询超时".into()),
        Ok(Err(e)) => return Err(format!("执行 gh 失败：{e}")),
        Ok(Ok(out)) => out,
    };
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    let text = String::from_utf8_lossy(&out.stdout);
    if serde_json::from_str::<Release>(&text).is_err() {
        return Err(format!("gh 的输出无法识别：{}", text.trim()));
    }
    Ok(parse(&text, current))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 版本比较() {
        assert!(is_newer("0.4.0", "v0.5.0"));
        assert!(is_newer("0.4.0", "v0.4.1"));
        assert!(is_newer("0.4.9", "v0.4.10"), "按数字比较，不按字符串");
        assert!(!is_newer("0.4.0", "v0.4.0"));
        assert!(!is_newer("0.5.0", "v0.4.9"));
        assert!(!is_newer("0.5.0", "v0.5.0-beta"));
    }

    #[test]
    fn 解析_gh_输出() {
        // 2026-10-20 实际执行 gh release view --repo … --json tagName,url 的输出
        let json = r#"{"tagName":"v0.4.0","url":"https://github.com/chengzhirenchaoshuai/MeshDock/releases/tag/v0.4.0"}"#;
        assert_eq!(parse(json, "0.4.0"), None);
        assert_eq!(
            parse(json, "0.3.0"),
            Some(AppUpdate {
                version: "0.4.0".into(),
                url: "https://github.com/chengzhirenchaoshuai/MeshDock/releases/tag/v0.4.0".into(),
            })
        );
        assert_eq!(parse("release not found", "0.3.0"), None);
    }

    /// 实际调用本机的 gh，需手动运行：cargo test -- --ignored
    #[test]
    #[ignore]
    fn 实机_查询最新版() {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let r = rt.block_on(check("0.0.1")).unwrap();
        assert!(r.is_some_and(|u| u.url.starts_with("https://github.com/")));
    }
}
