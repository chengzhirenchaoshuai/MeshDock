//! 24 小时延迟历史：后台定时测速（间隔可设置）和手动测试的结果都记在这里，按设备保存到本机文件
use crate::classify::{ConnKind, PingResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

/// 只保留最近 24 小时
const KEEP_MS: i64 = 24 * 3600 * 1000;

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

    pub fn get(&self) -> History {
        self.data.lock().unwrap().clone()
    }

    /// 记录一批测量，去掉 24 小时前的，并写盘。
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
    fn 只保留_24_小时并能重新读取() {
        let path =
            std::env::temp_dir().join(format!("meshdock-history-{}.json", std::process::id()));
        let now = 100 * 3600 * 1000;
        let store = HistoryStore::load(path.clone(), now);
        store.add(
            vec![
                ("a".into(), sample(now - KEEP_MS - 1)), // 超过 24 小时，丢弃
                ("a".into(), sample(now - 1000)),
                ("b".into(), sample(now - KEEP_MS - 5)), // b 全部过期，整台设备去掉
            ],
            now,
        );
        let reloaded = HistoryStore::load(path.clone(), now).get();
        assert_eq!(reloaded.len(), 1);
        assert_eq!(reloaded["a"], vec![sample(now - 1000)]);
        std::fs::remove_file(path).unwrap();
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
