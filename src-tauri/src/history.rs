//! 延迟历史：后台定时测速（间隔可设置）和手动测试的结果都记在这里，按设备保存到本机文件。
//! 保留 7 天：设备卡片的小图只用最近 24 小时，详情弹窗可看 7 天
use crate::classify::{ConnKind, PingResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

/// 保留最近 7 天
const KEEP_MS: i64 = 7 * DAY_MS;
/// 设备卡片小图的时间跨度
pub const DAY_MS: i64 = 24 * 3600 * 1000;

/// 一次测量
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Sample {
    pub at_ms: i64,
    /// 延迟毫秒；没有回应为 None
    pub ms: Option<f64>,
    /// 实测路径；没有回应为 None
    pub kind: Option<ConnKind>,
}

impl Sample {
    pub fn from_ping(at_ms: i64, r: &PingResult) -> Self {
        Self {
            at_ms,
            ms: r.latency_ms,
            kind: r.conn.as_ref().map(|c| c.kind),
        }
    }
}

/// 设备 id → 按时间排列的测量
pub type History = HashMap<String, Vec<Sample>>;

pub struct HistoryStore {
    path: PathBuf,
    data: Mutex<History>,
}

impl HistoryStore {
    /// 读取历史；文件不存在或损坏时从空开始
    pub fn load(path: PathBuf, now_ms: i64) -> Self {
        let mut data: History = crate::paths::load_json(&path).unwrap_or_default();
        prune(&mut data, now_ms);
        Self {
            path,
            data: Mutex::new(data),
        }
    }

    /// 全部设备 since_ms 之后的测量（设备卡片用最近 24 小时，避免每次推送整整 7 天的数据）
    pub fn recent(&self, since_ms: i64) -> History {
        self.data
            .lock()
            .unwrap()
            .iter()
            .filter_map(|(id, list)| {
                let start = list.partition_point(|s| s.at_ms < since_ms);
                (start < list.len()).then(|| (id.clone(), list[start..].to_vec()))
            })
            .collect()
    }

    /// 一台设备保留的全部测量（最近 7 天）
    pub fn device(&self, id: &str) -> Vec<Sample> {
        self.data.lock().unwrap().get(id).cloned().unwrap_or_default()
    }

    /// 记录一批测量，去掉 7 天前的，并写盘。
    /// 持锁写盘：两批测量同时到达时，后写入的一定是较新的内容
    pub fn add(&self, samples: Vec<(String, Sample)>, now_ms: i64) {
        let mut data = self.data.lock().unwrap();
        for (id, s) in samples {
            let list = data.entry(id).or_default();
            list.push(s);
            list.sort_by_key(|s| s.at_ms);
        }
        prune(&mut data, now_ms);
        if let Ok(text) = serde_json::to_string(&*data)
            && let Err(e) = crate::paths::write_atomic(&self.path, text.as_bytes())
        {
            crate::applog::error(format!("保存延迟历史失败：{e}"));
        }
    }
}

/// 导出成 CSV：时间、延迟（毫秒，没有回应为空）、路径。time 把毫秒时间戳格式化成本地时间。
/// 开头加 UTF-8 BOM，Excel 打开时中文不乱码
pub fn to_csv(samples: &[Sample], time: impl Fn(i64) -> String) -> String {
    let mut out = String::from("\u{feff}时间,延迟（毫秒）,路径\r\n");
    for s in samples {
        let ms = s.ms.map(|v| format!("{v:.1}")).unwrap_or_default();
        let path = match (s.ms, s.kind) {
            (None, _) => "没有回应",
            (_, Some(ConnKind::Lan)) => "局域网直连",
            (_, Some(ConnKind::Ipv6)) => "IPv6 直连",
            (_, Some(ConnKind::Public)) => "公网直连",
            (_, Some(ConnKind::Relay)) => "中继",
            (_, Some(ConnKind::PeerRelay)) => "节点中转",
            _ => "未知",
        };
        out.push_str(&format!("{},{ms},{path}\r\n", time(s.at_ms)));
    }
    out
}

fn prune(data: &mut History, now_ms: i64) {
    for list in data.values_mut() {
        list.retain(|s| now_ms - s.at_ms <= KEEP_MS);
    }
    data.retain(|_, list| !list.is_empty());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(at_ms: i64) -> Sample {
        Sample {
            at_ms,
            ms: Some(1.0),
            kind: Some(ConnKind::Lan),
        }
    }

    #[test]
    fn 只保留_7_天并能重新读取() {
        let path =
            std::env::temp_dir().join(format!("meshdock-history-{}.json", std::process::id()));
        let now = 100 * 3600 * 1000;
        let store = HistoryStore::load(path.clone(), now);
        store.add(
            vec![
                ("a".into(), sample(now - KEEP_MS - 1)), // 超过 7 天，丢弃
                ("a".into(), sample(now - 1000)),
                ("b".into(), sample(now - KEEP_MS - 5)), // b 全部过期，整台设备去掉
            ],
            now,
        );
        let reloaded = HistoryStore::load(path.clone(), now).recent(0);
        assert_eq!(reloaded.len(), 1);
        assert_eq!(reloaded["a"], vec![sample(now - 1000)]);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn 卡片只取最近一段_详情取全部() {
        let path = std::env::temp_dir()
            .join(format!("meshdock-history-recent-{}.json", std::process::id()));
        let now = 100 * 3600 * 1000;
        let store = HistoryStore::load(path.clone(), now);
        store.add(
            vec![
                ("a".into(), sample(now - 3 * DAY_MS)),
                ("a".into(), sample(now - 1000)),
                ("b".into(), sample(now - 2 * DAY_MS)), // b 最近 24 小时没有测量
            ],
            now,
        );
        let recent = store.recent(now - DAY_MS);
        assert_eq!(recent.len(), 1);
        assert_eq!(recent["a"], vec![sample(now - 1000)]);
        assert_eq!(store.device("a").len(), 2, "7 天内的都保留");
        assert!(store.device("x").is_empty());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn 导出_csv() {
        let samples = [
            sample(1),
            Sample { at_ms: 2, ms: None, kind: None },
            Sample { at_ms: 3, ms: Some(46.25), kind: Some(ConnKind::Relay) },
        ];
        let csv = to_csv(&samples, |ms| format!("t{ms}"));
        assert_eq!(
            csv,
            "\u{feff}时间,延迟（毫秒）,路径\r\nt1,1.0,局域网直连\r\nt2,,没有回应\r\nt3,46.2,中继\r\n"
        );
    }

    #[test]
    fn 没有回应记为空延迟() {
        let r = PingResult {
            conn: None,
            latency_ms: None,
            error: Some("没有回应".into()),
            raw: String::new(),
        };
        assert_eq!(
            Sample::from_ping(5, &r),
            Sample {
                at_ms: 5,
                ms: None,
                kind: None
            }
        );
    }
}
