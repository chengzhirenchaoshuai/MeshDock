//! 带有效期的缓存值
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// 带有效期的缓存值
pub(super) struct Ttl<T> {
    ttl: Duration,
    value: Mutex<Option<(Instant, T)>>,
}

impl<T: Clone> Ttl<T> {
    pub(super) fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            value: Mutex::new(None),
        }
    }

    /// 未过期时返回缓存值
    pub(super) fn get(&self) -> Option<T> {
        let value = self.value.lock().unwrap();
        let (at, v) = value.as_ref()?;
        (at.elapsed() < self.ttl).then(|| v.clone())
    }

    pub(super) fn set(&self, v: T) {
        *self.value.lock().unwrap() = Some((Instant::now(), v));
    }

    pub(super) fn clear(&self) {
        *self.value.lock().unwrap() = None;
    }

    /// 未过期时用缓存，否则重新计算并缓存
    pub(super) fn get_or(&self, f: impl FnOnce() -> T) -> T {
        self.get().unwrap_or_else(|| {
            let v = f();
            self.set(v.clone());
            v
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 缓存未过期时不重新计算_作废后重新计算() {
        let cache = Ttl::new(Duration::from_secs(60));
        let mut calls = 0;
        assert!(cache.get_or(|| {
            calls += 1;
            true
        }));
        assert!(cache.get_or(|| {
            calls += 1;
            false
        }));
        assert_eq!(calls, 1);
        cache.clear();
        assert_eq!(cache.get(), None);
        // 有效期为 0 时总是过期
        let expired = Ttl::new(Duration::ZERO);
        expired.set(Some(true));
        assert_eq!(expired.get(), None);
    }
}
