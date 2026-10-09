//! FakeClock：夹具唯一的时间源。
//!
//! **单调时间（`MonoTime`）与 UTC 时间（`FixedUtc`）必须分离**：
//!
//! - 单调时间只用于**持续时间**判定：acquire 超时、队列等待上限、掉线 grace、
//!   无事务 idle、空闲事务、cancel/cleanup 期限、pool idle TTL、幂等令牌 24 小时。
//! - UTC 只用于生成 `expiresAt` 一类对外投影。
//!
//! 基准固定为常量 `T0`，**不读系统时钟**。`advance(Duration)` 是同步的：推进之后
//! 所有已注册该期限的 timer 立即到期，同一线程内即可断言。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use chrono::{DateTime, TimeDelta, Utc};

/// 虚拟基准时刻（单调纳秒）。任何夹具的起点都必须是这一个常量，保证跨运行一致。
pub const T0_NANOS: u64 = 0;

/// 虚拟基准时刻（UTC）。`2026-01-01T00:00:00Z`，基准固定为常量 T0。
pub const T0_UTC: &str = "2026-01-01T00:00:00Z";

/// 单调时间点（自 `T0` 起的纳秒数）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct MonoTime(u64);

impl MonoTime {
    pub const fn from_nanos(nanos: u64) -> Self {
        Self(nanos)
    }

    pub const fn as_nanos(self) -> u64 {
        self.0
    }

    pub fn as_duration(self) -> Duration {
        Duration::from_nanos(self.0)
    }

    /// 两个单调时刻之间的经过时间；只接受非负差值。
    pub fn saturating_duration_since(self, earlier: MonoTime) -> Duration {
        Duration::from_nanos(self.0.saturating_sub(earlier.0))
    }
}

impl std::fmt::Display for MonoTime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "T0+{}ns", self.0)
    }
}

/// 固定 UTC 投影。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct FixedUtc(DateTime<Utc>);

impl FixedUtc {
    pub fn as_rfc3339(&self) -> String {
        self.0.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    }
}

impl std::fmt::Display for FixedUtc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.as_rfc3339())
    }
}

fn base_utc() -> DateTime<Utc> {
    // 常量 `T0_UTC` 写成 `2026-01-01T00:00:00Z`，解析失败属于代码缺陷，
    // 但它是夹具初始化路径、不是生产路径，因此此处允许显式 panic 并注释说明。
    T0_UTC
        .parse::<DateTime<Utc>>()
        .expect("T0_UTC 必须是合法的 RFC3339 常量（夹具初始化不变量）")
}

/// 已注册期限的标识。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TimerId(pub u64);

/// 一个已注册期限。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Timer {
    pub id: TimerId,
    /// 人类可读标签，仅用于断言消息与 journal 摘要；**不得**写入凭据类信息。
    pub label: String,
    pub armed_at_nanos: u64,
    pub deadline_nanos: u64,
}

impl Timer {
    pub fn remaining(&self, now: MonoTime) -> Duration {
        Duration::from_nanos(self.deadline_nanos.saturating_sub(now.as_nanos()))
    }
}

/// 到期的期限。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FiredTimer {
    pub id: TimerId,
    pub label: String,
    pub deadline_nanos: u64,
    pub fired_at_nanos: u64,
}

#[derive(Debug, Default)]
struct ClockState {
    nanos: u64,
    timers: Vec<Timer>,
    fired: Vec<FiredTimer>,
    next_timer: u64,
}

/// 时间推进回调。`advance` 结束后被调用，用于唤醒阻塞在条件变量上的等待方。
pub type Waker = Arc<dyn Fn() + Send + Sync + 'static>;

struct Shared {
    state: Mutex<ClockState>,
    wakers: Mutex<Vec<Waker>>,
    wakes: AtomicU64,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, ClockState> {
        // FakeClock 是纯内存状态，锁中毒意味着某个测试 panic 已经污染了它。
        // 恢复而不是传播，保证「同线程内即可断言」的路径不会因为一次误用而永久不可用。
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// FakeClock。克隆共享同一状态。
#[derive(Clone)]
pub struct FakeClock {
    inner: Arc<Shared>,
}

impl FakeClock {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Shared {
                state: Mutex::new(ClockState {
                    nanos: T0_NANOS,
                    timers: Vec::new(),
                    fired: Vec::new(),
                    next_timer: 1,
                }),
                wakers: Mutex::new(Vec::new()),
                wakes: AtomicU64::new(0),
            }),
        }
    }

    /// 当前单调时刻。所有持续时间判定都用它。
    pub fn monotonic(&self) -> MonoTime {
        MonoTime(self.inner.lock().nanos)
    }

    /// 当前 UTC 投影。只用于 `expiresAt` 之类的对外字段。
    pub fn utc(&self) -> FixedUtc {
        let elapsed = self.inner.lock().nanos;
        // 虚拟时间以纳秒为粒度累加，夹具实际推进量远小于 i64 上限。
        let delta = TimeDelta::nanoseconds(i64::try_from(elapsed).unwrap_or(i64::MAX));
        FixedUtc(base_utc() + delta)
    }

    /// 注册一个期限：`ttl` 之后到期。
    pub fn arm(&self, label: impl Into<String>, ttl: Duration) -> TimerId {
        let mut state = self.inner.lock();
        let id = TimerId(state.next_timer);
        state.next_timer += 1;
        let armed_at_nanos = state.nanos;
        state.timers.push(Timer {
            id,
            label: label.into(),
            armed_at_nanos,
            deadline_nanos: armed_at_nanos
                .saturating_add(ttl.as_nanos().min(u128::from(u64::MAX)) as u64),
        });
        id
    }

    /// 注销一个期限（例如宿主取消等待）。
    pub fn disarm(&self, id: TimerId) -> bool {
        let mut state = self.inner.lock();
        let before = state.timers.len();
        state.timers.retain(|timer| timer.id != id);
        state.timers.len() != before
    }

    /// 推进虚拟时间，并返回本次推进内到期的全部期限。
    ///
    /// 同步语义：函数返回后所有 `deadline <= now` 的 timer 都已进入 `fired`，
    /// 调用方在同一线程内直接断言即可。
    pub fn advance(&self, delta: Duration) -> Vec<FiredTimer> {
        let nanos = u64::try_from(delta.as_nanos()).unwrap_or(u64::MAX);
        let fired = {
            let mut state = self.inner.lock();
            state.nanos = state.nanos.saturating_add(nanos);
            let now = state.nanos;
            let mut fired = Vec::new();
            let mut retained = Vec::with_capacity(state.timers.len());
            for timer in state.timers.drain(..) {
                if timer.deadline_nanos <= now {
                    fired.push(FiredTimer {
                        id: timer.id,
                        label: timer.label.clone(),
                        deadline_nanos: timer.deadline_nanos,
                        fired_at_nanos: now,
                    });
                } else {
                    retained.push(timer);
                }
            }
            state.timers = retained;
            state.fired.extend(fired.iter().cloned());
            fired
        };
        self.notify_wakers();
        fired
    }

    /// 已注册且已到期的期限（不消费）。
    pub fn expired(&self) -> Vec<Timer> {
        let state = self.inner.lock();
        let now = state.nanos;
        state
            .timers
            .iter()
            .filter(|timer| timer.deadline_nanos <= now)
            .cloned()
            .collect()
    }

    /// 至今为止到期过的全部期限（累计，含已被 `advance` 消费的部分）。
    pub fn fired_history(&self) -> Vec<FiredTimer> {
        self.inner.lock().fired.clone()
    }

    /// 仍在等待的期限。
    pub fn pending(&self) -> Vec<Timer> {
        self.inner.lock().timers.clone()
    }

    /// 跨期限选择：返回**最早**到期的那个。
    pub fn earliest_deadline(deadlines: &[Deadline]) -> Option<Deadline> {
        deadlines
            .iter()
            .copied()
            .min_by_key(|deadline| (deadline.due_at_nanos(), deadline.label))
    }

    /// 判断某个期限集合里，此刻到期的到底是哪些。
    pub fn due_deadlines(&self, deadlines: &[Deadline]) -> Vec<Deadline> {
        let now = self.monotonic();
        deadlines
            .iter()
            .copied()
            .filter(|deadline| deadline.due(now))
            .collect()
    }

    /// 推进到最早期限并返回被选中的那个；测试据此断言「实际选用的是最早者」。
    pub fn advance_to_earliest(&self, deadlines: &[Deadline]) -> Option<Deadline> {
        let earliest = Self::earliest_deadline(deadlines)?;
        let now = self.monotonic();
        let delta = earliest.due_at_nanos().saturating_sub(now.as_nanos());
        self.advance(Duration::from_nanos(delta));
        Some(earliest)
    }

    /// 注册一个时间推进回调。`advance` 结束后回调被唤醒，用于让阻塞在条件变量上的
    /// 等待方（`DrainBarrier`）重新检查自己的期限。
    pub fn add_waker(&self, waker: Waker) {
        let mut wakers = match self.inner.wakers.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let _ = &mut self.inner.wakes.fetch_add(1, Ordering::Relaxed);
        wakers.push(waker);
    }

    fn notify_wakers(&self) {
        let snapshot = match self.inner.wakers.lock() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        };
        for waker in snapshot {
            waker();
        }
    }
}

impl Default for FakeClock {
    fn default() -> Self {
        Self::new()
    }
}

/// 一个候选期限。宿主需要同时盯住多个期限时用它建模，选中的是最早者。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Deadline {
    pub label: &'static str,
    pub from: MonoTime,
    pub ttl: Duration,
}

impl Deadline {
    pub fn new(label: &'static str, from: MonoTime, ttl: Duration) -> Self {
        Self { label, from, ttl }
    }

    pub fn due_at_nanos(&self) -> u64 {
        self.from
            .as_nanos()
            .saturating_add(u64::try_from(self.ttl.as_nanos()).unwrap_or(u64::MAX))
    }

    pub fn due_at(&self) -> MonoTime {
        MonoTime(self.due_at_nanos())
    }

    pub fn due(&self, now: MonoTime) -> bool {
        self.due_at_nanos() <= now.as_nanos()
    }

    /// 距到期还剩多久；已到期返回零。
    pub fn remaining_at(&self, now: MonoTime) -> Duration {
        Duration::from_nanos(self.due_at_nanos().saturating_sub(now.as_nanos()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    #[test]
    fn advance_is_the_only_time_source_and_starts_at_t0() {
        let clock = FakeClock::new();
        assert_eq!(clock.monotonic(), MonoTime::from_nanos(T0_NANOS));
        assert_eq!(clock.utc().as_rfc3339(), "2026-01-01T00:00:00.000Z");

        clock.advance(secs(90));
        assert_eq!(clock.monotonic(), MonoTime::from_nanos(90_000_000_000));
        assert_eq!(clock.utc().as_rfc3339(), "2026-01-01T00:01:30.000Z");
    }

    #[test]
    fn armed_timers_fire_inside_the_same_thread_after_advance() {
        // `advance` 同步生效，推进后立即断言，不需要 sleep、不需要别的线程。
        let clock = FakeClock::new();
        let idle_no_txn = clock.arm("idle-no-transaction", secs(30 * 60));
        let idle_txn = clock.arm("idle-transaction", secs(5 * 60));
        let cleanup = clock.arm("cleanup-deadline", secs(10));

        assert!(clock.advance(secs(10)).iter().any(|f| f.id == cleanup));
        assert!(
            clock.expired().is_empty(),
            "已到期的 timer 已被 advance 消费"
        );

        let fired = clock.advance(secs(5 * 60 - 10));
        assert_eq!(fired.len(), 1, "只应触发 5 分钟空闲事务期限");
        assert_eq!(fired[0].id, idle_txn);
        assert!(clock.expired().is_empty(), "30 分钟期限尚未到期");

        let fired = clock.advance(secs(30 * 60 - 5 * 60));
        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0].id, idle_no_txn);
    }

    #[test]
    fn earliest_deadline_wins_and_only_one_fires() {
        // 掉线保留 60s 与 idle 取最早者 —— 推进后必须只有最早者触发。
        let clock = FakeClock::new();
        let base = clock.monotonic();
        let deadlines = [
            Deadline::new("idle-no-transaction", base, secs(30 * 60)),
            Deadline::new("client-disconnect-grace", base, secs(60)),
        ];
        assert_eq!(
            FakeClock::earliest_deadline(&deadlines).map(|d| d.label),
            Some("client-disconnect-grace")
        );

        let disconnect = clock.arm("client-disconnect-grace", secs(60));
        let idle = clock.arm("idle-no-transaction", secs(30 * 60));
        let fired = clock.advance(secs(60));
        assert_eq!(fired.len(), 1, "跨期限不得同时触发");
        assert_eq!(fired[0].id, disconnect);
        assert!(clock.expired().iter().all(|timer| timer.id == idle));
    }

    #[test]
    fn deadlines_report_which_one_is_due_at_a_given_instant() {
        let clock = FakeClock::new();
        let base = clock.monotonic();
        let deadlines = [
            Deadline::new("a", base, secs(5)),
            Deadline::new("b", base, secs(10)),
        ];
        clock.advance(secs(6));
        let due: Vec<&str> = clock
            .due_deadlines(&deadlines)
            .iter()
            .map(|deadline| deadline.label)
            .collect();
        assert_eq!(due, vec!["a"]);
    }

    #[test]
    fn idempotency_token_expiry_is_24h_of_virtual_time() {
        // 幂等提交令牌 24 小时。
        let clock = FakeClock::new();
        let token = clock.arm("idempotency-token", secs(24 * 3600));
        assert!(clock.advance(secs(24 * 3600 - 1)).is_empty());
        let fired = clock.advance(secs(1));
        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0].id, token);
    }

    #[test]
    fn wakers_run_after_advance() {
        let clock = FakeClock::new();
        let hits = Arc::new(Mutex::new(0usize));
        let recorder = Arc::clone(&hits);
        clock.add_waker(Arc::new(move || {
            *recorder.lock().unwrap_or_else(|p| p.into_inner()) += 1;
        }));
        clock.advance(secs(1));
        clock.advance(secs(1));
        assert_eq!(*hits.lock().unwrap_or_else(|p| p.into_inner()), 2);
    }

    #[test]
    fn disarmed_timer_never_fires() {
        let clock = FakeClock::new();
        let id = clock.arm("cancelled-wait", secs(10));
        assert!(clock.disarm(id));
        assert!(clock.advance(secs(60)).is_empty());
    }
}
