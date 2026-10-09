//! Job 时钟。时间源由调用方注入（与 BudgetClock 同一原则）：
//! 仓储与 runtime 只消费 [`JobClock`]，测试用固定/可推进时钟，生产路径由组装层提供。

use std::sync::{Arc, Mutex};

use datazen_platform_api::id::Timestamp;

use crate::job::error::JobError;

/// 时间源。
pub trait JobClock: Send + Sync + 'static {
    fn now(&self) -> Timestamp;
}

/// Production UTC clock for persisted desktop job timestamps.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemJobClock;

impl JobClock for SystemJobClock {
    fn now(&self) -> Timestamp {
        Timestamp::new(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
    }
}

/// 单线程测试用可推进时钟。
#[derive(Debug, Clone)]
pub struct SharedClock {
    now: Arc<Mutex<Timestamp>>,
}

impl SharedClock {
    pub fn at(ts: impl Into<String>) -> Self {
        Self {
            now: Arc::new(Mutex::new(Timestamp::new(ts))),
        }
    }

    pub fn set(&self, ts: impl Into<String>) {
        if let Ok(mut guard) = self.now.lock() {
            *guard = Timestamp::new(ts);
        }
    }
}

impl JobClock for SharedClock {
    fn now(&self) -> Timestamp {
        match self.now.lock() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }
}

/// 在定宽 ISO-8601 时间戳上加秒数。解析失败返回 `JobError::BackendUnavailable` 等价错误。
///
/// `Timestamp` 是只读字符串契约（§4.1）：比较是字典序，所以这里用 chrono 做一次
/// 解析→加算术→重写，**不改变它仍是字符串的事实**。生产路径不 panic。
pub fn after_seconds(ts: &Timestamp, secs: i64) -> Result<Timestamp, JobError> {
    let parsed = chrono::DateTime::parse_from_rfc3339(ts.as_str())
        .map_err(|_| JobError::PlanProjectionInvalid("unparseable timestamp".into()))?;
    let shifted = parsed + chrono::Duration::seconds(secs);
    Ok(Timestamp::new(
        shifted.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn after_seconds_shifts_a_wellformed_timestamp() {
        let ts = Timestamp::new("2026-01-01T00:00:00Z");
        let later = after_seconds(&ts, 90).expect("shift");
        assert_eq!(later.as_str(), "2026-01-01T00:01:30Z");
    }

    #[test]
    fn after_seconds_rejects_malformed_input_without_panicking() {
        let ts = Timestamp::new("not-a-time");
        assert!(after_seconds(&ts, 1).is_err());
    }
}
