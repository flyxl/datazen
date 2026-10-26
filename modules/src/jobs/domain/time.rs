//! Job 时钟。时间源由调用方注入（与 BudgetClock 同一原则）：
//! 仓储与 runtime 只消费 [`JobClock`]，测试用固定/可推进时钟，生产路径由组装层提供。
//!
//! 生产实现在这里**不再自己读系统时间**：[`SystemJobClock`] 只是平台契约
//! [`crate::shared::clock::Clock`] 的 `SystemClock` 在 job 侧的别名，
//! 因此「job 时间戳的形状」只有一处定义（契约层的定宽 RFC3339 毫秒 UTC）。
//! 需要把任意 [`Clock`] 接进来的装配层用 [`PortJobClock`]。

use std::sync::{Arc, Mutex};

use crate::shared::clock::{Clock, SystemClock};
use crate::shared::id::Timestamp;

use crate::jobs::domain::error::JobError;

/// 时间源。
pub trait JobClock: Send + Sync + 'static {
    fn now(&self) -> Timestamp;
}

/// Production UTC clock for persisted desktop job timestamps.
///
/// 委托给平台端口的 [`SystemClock`]：job 时间戳不再有第二份格式化代码。
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemJobClock;

impl JobClock for SystemJobClock {
    fn now(&self) -> Timestamp {
        SystemClock.now()
    }
}

/// 把平台端口 [`Clock`] 适配成 [`JobClock`]：装配层给一个时间源，
/// 仓储与 runtime 阶段就按它写时刻（测试可注入固定时钟）。
#[derive(Debug, Clone, Copy, Default)]
pub struct PortJobClock<C: Clock = SystemClock> {
    clock: C,
}

impl<C: Clock> PortJobClock<C> {
    pub fn new(clock: C) -> Self {
        Self { clock }
    }
}

impl<C: Clock> JobClock for PortJobClock<C> {
    fn now(&self) -> Timestamp {
        self.clock.now()
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

    #[test]
    fn an_injected_port_clock_drives_the_job_clock() {
        use crate::shared::clock::FixedClock;

        let fixed = FixedClock::at("2026-05-05T00:00:00.000Z");
        let clock = PortJobClock::new(fixed.clone());
        assert_eq!(clock.now().as_str(), "2026-05-05T00:00:00.000Z");

        fixed.set("2026-05-05T00:02:30.000Z");
        assert_eq!(clock.now().as_str(), "2026-05-05T00:02:30.000Z");
    }

    #[test]
    fn the_system_job_clock_writes_the_port_shape() {
        // 与契约层同一个形状：定宽、毫秒、UTC（字典序比较的前提）。
        let now = SystemJobClock.now();
        assert_eq!(now.as_str().len(), "2026-01-01T00:00:00.000Z".len());
        assert!(now.as_str().ends_with('Z'), "{}", now.as_str());
        assert_eq!(&now.as_str()[19..20], ".");
    }
}
