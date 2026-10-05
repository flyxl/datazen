//! 协议 drain barrier：sink 写入在无消费者时**等待**，期限到了就截断（fake-runtime-fixtures.md §6.2）。
//!
//! 依赖方向同父模块：只向下依赖 `clock` 与非夹具的 `connection::{execution, types}`。

use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::Duration;

use crate::connection::execution::{SinkWrite, TruncationReason};
use crate::connection::types::ExecutionId;

use super::super::clock::{FakeClock, MonoTime, TimerId};

// ---------------------------------------------------------------------------
// DrainBarrier —— 协议 drain barrier
// ---------------------------------------------------------------------------

/// drain 上限。设计值来自 §7.2 / §11.2，`lowered_for_tests()` 用 §7.2 允许下调的那几个值。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DrainLimits {
    pub events_per_subscription: u64,
    pub bytes_per_subscription: u64,
    pub bytes_per_execution: u64,
    /// 无消费者等待上限。
    pub no_consumer_wait: Duration,
    /// drain 期限。
    pub drain_deadline: Duration,
}

impl DrainLimits {
    /// §7.2 设计值。
    pub const fn design() -> Self {
        Self {
            events_per_subscription: 256,
            bytes_per_subscription: 1024 * 1024,
            bytes_per_execution: 8 * 1024 * 1024,
            no_consumer_wait: Duration::from_secs(30),
            drain_deadline: Duration::from_secs(10),
        }
    }

    /// §7.2：数据缓冲 8 MiB → 测试下调到 64 KiB；其余数值不动。
    pub const fn lowered_for_tests() -> Self {
        Self {
            bytes_per_execution: 64 * 1024,
            ..Self::design()
        }
    }
}

#[derive(Debug, Default)]
struct SubscriptionCounters {
    events: u64,
    bytes: u64,
}

#[derive(Debug)]
struct DrainState {
    no_consumer: bool,
    /// 已开始写入但尚未被消费的执行 → drain 期限。
    drain_timer: Option<TimerId>,
    /// 停在 `await_drain` 的条件变量上、正等唤醒的线程数。
    drain_waiters: usize,
    /// 每个订阅累计的事件数 / 字节数。
    subscription: SubscriptionCounters,
    /// **每个执行**累计字节数。§6.2 的 8 MiB 是「每执行」上限，不是全局标量：
    /// 两个并发执行必须各自计量，否则先跑的会把后跑的额度吃光。
    per_execution_bytes: Vec<(ExecutionId, u64)>,
    /// 已被截断的执行及其原因。
    truncated: Vec<(ExecutionId, TruncationReason)>,
}

impl DrainState {
    fn truncation_for(&self, execution_id: &ExecutionId) -> Option<TruncationReason> {
        self.truncated
            .iter()
            .find(|(id, _)| id == execution_id)
            .map(|(_, r)| *r)
    }

    fn bytes_of(&self, execution_id: &ExecutionId) -> u64 {
        self.per_execution_bytes
            .iter()
            .find(|(id, _)| id == execution_id)
            .map(|(_, bytes)| *bytes)
            .unwrap_or(0)
    }
}

struct DrainShared {
    clock: FakeClock,
    limits: DrainLimits,
    state: Mutex<DrainState>,
    condvar: Condvar,
}

impl DrainShared {
    fn lock(&self) -> MutexGuard<'_, DrainState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// 协议 drain barrier（CM-64）。
///
/// 无消费者时 `ResultSink` 写入必须**等待**（`SinkWrite::Blocked`）；推进 FakeClock 到 drain
/// 期限后截断，并记下 `truncated` 与 `truncationReason`（§6.2）。
#[derive(Clone)]
pub struct DrainBarrier {
    inner: Arc<DrainShared>,
}

impl DrainBarrier {
    pub fn new(clock: FakeClock, limits: DrainLimits) -> Self {
        let this = Self {
            inner: Arc::new(DrainShared {
                clock: clock.clone(),
                limits,
                state: Mutex::new(DrainState {
                    no_consumer: false,
                    drain_timer: None,
                    drain_waiters: 0,
                    subscription: SubscriptionCounters::default(),
                    per_execution_bytes: Vec::new(),
                    truncated: Vec::new(),
                }),
                condvar: Condvar::new(),
            }),
        };
        // FakeClock 推进后唤醒可能阻塞在 drain 上的线程。
        let inner = Arc::clone(&this.inner);
        clock.add_waker(Arc::new(move || {
            inner.condvar.notify_all();
        }));
        this
    }

    pub fn limits(&self) -> DrainLimits {
        self.inner.limits
    }

    pub fn clock(&self) -> FakeClock {
        self.inner.clock.clone()
    }

    /// 订阅建立后置 `no_consumer=true`：此后写入等待，直到有消费者或 drain 期限到期。
    pub fn set_no_consumer(&self, no_consumer: bool) {
        {
            let mut state = self.inner.lock();
            state.no_consumer = no_consumer;
            if !no_consumer {
                state.drain_timer = None;
            }
        }
        self.inner.condvar.notify_all();
    }

    pub fn is_no_consumer(&self) -> bool {
        self.inner.lock().no_consumer
    }

    /// 该执行是否已被截断，以及原因。
    pub fn truncation_of(&self, execution_id: &ExecutionId) -> Option<TruncationReason> {
        self.inner.lock().truncation_for(execution_id)
    }

    /// 已产出字节计数。§6.2：桌面 256 MiB 产物上限以它断言，不以内存占用断言。
    pub fn produced_bytes(&self) -> u64 {
        let state = self.inner.lock();
        state.subscription.bytes
    }

    pub fn produced_events(&self) -> u64 {
        self.inner.lock().subscription.events
    }

    /// 无消费者分支共用的判定：装载 drain 期限（只装一次），再用 FakeClock 判定是否已到期。
    ///
    /// 期限**只由 FakeClock 决定**，不读挂钟也不 `sleep`（§6.4）。返回 `Some(reason)`
    /// 表示期限已过，调用方应记截断并返回 `Truncated`。
    fn arm_drain_and_check(&self, state: &mut DrainState) -> Option<TruncationReason> {
        let timer = match state.drain_timer {
            Some(timer) => timer,
            None => {
                let timer = self
                    .inner
                    .clock
                    .arm("drain-deadline", self.inner.limits.drain_deadline);
                state.drain_timer = Some(timer);
                timer
            }
        };
        self.inner
            .clock
            .fired_history()
            .iter()
            .any(|fired| fired.id == timer)
            .then_some(TruncationReason::NoConsumerDrainDeadline)
    }

    fn remember_truncation(
        state: &mut DrainState,
        execution_id: &ExecutionId,
        reason: TruncationReason,
    ) {
        if state.truncation_for(execution_id).is_none() {
            state.truncated.push((execution_id.clone(), reason));
        }
    }

    /// 一次结果写入。返回 `Accepted` / `Blocked`（背压）/ `Truncated(原因)`。
    pub fn write(&self, execution_id: &ExecutionId, chunk_index: u32, bytes: usize) -> SinkWrite {
        let bytes = bytes as u64;
        let mut state = self.inner.lock();

        if let Some(reason) = state.truncation_for(execution_id) {
            return SinkWrite::Truncated(reason);
        }

        if state.no_consumer {
            if let Some(reason) = self.arm_drain_and_check(&mut state) {
                Self::remember_truncation(&mut state, execution_id, reason);
                return SinkWrite::Truncated(reason);
            }
            return SinkWrite::Blocked;
        }

        if state.drain_timer.is_some() {
            state.drain_timer = None;
        }

        if state.bytes_of(execution_id).saturating_add(bytes)
            > self.inner.limits.bytes_per_execution
        {
            let reason = TruncationReason::PerExecutionByteLimit;
            Self::remember_truncation(&mut state, execution_id, reason);
            return SinkWrite::Truncated(reason);
        }

        if state.subscription.events + 1 > self.inner.limits.events_per_subscription {
            let reason = TruncationReason::PerSubscriptionEventLimit;
            Self::remember_truncation(&mut state, execution_id, reason);
            return SinkWrite::Truncated(reason);
        }

        if state.subscription.bytes.saturating_add(bytes) > self.inner.limits.bytes_per_subscription
        {
            let reason = TruncationReason::PerSubscriptionByteLimit;
            Self::remember_truncation(&mut state, execution_id, reason);
            return SinkWrite::Truncated(reason);
        }

        let _ = chunk_index;
        state.subscription.events += 1;
        state.subscription.bytes += bytes;
        match state
            .per_execution_bytes
            .iter_mut()
            .find(|(id, _)| id == execution_id)
        {
            Some((_, counted)) => *counted += bytes,
            None => state
                .per_execution_bytes
                .push((execution_id.clone(), bytes)),
        }
        SinkWrite::Accepted
    }

    /// 执行结束：丢弃该执行的字节计数**和**它的截断记录（订阅级计数继续累计）。
    ///
    /// 必须带 `execution_id`：截断是**按执行**的粘性结论，不清掉的话同一个 id 重跑
    /// 会永远读到上一次的 `Truncated`，§6.2 的「每执行 8 MiB」也就无法复测。
    pub fn finish_execution(&self, execution_id: &ExecutionId) {
        let mut state = self.inner.lock();
        state
            .per_execution_bytes
            .retain(|(id, _)| id != execution_id);
        state.truncated.retain(|(id, _)| id != execution_id);
        state.drain_timer = None;
    }

    /// 是否已越过 drain 期限（无消费者等待被 FakeClock 判定为到期）。
    pub fn drain_deadline_elapsed(&self) -> bool {
        let state = self.inner.lock();
        self.drain_deadline_elapsed_in(&state)
    }

    /// 期限是否**此刻**已到期。挂在单调状态上而不是挂在一次 `notify` 上：
    /// 条件变量醒来后重判谓词、走回循环再判一次，两处必须问同一个问题。
    ///
    /// 只能问累计的 `fired_history`：`advance` 会把到期项从待触发队列里取走，
    /// `expired()` 在推进之后反而是空的。
    fn drain_deadline_elapsed_in(&self, state: &DrainState) -> bool {
        let Some(timer) = state.drain_timer else {
            return false;
        };
        self.inner
            .clock
            .fired_history()
            .iter()
            .any(|fired| fired.id == timer)
    }

    /// 此刻真的停在 `await_drain` 条件变量上的线程数。
    pub fn drain_waiter_count(&self) -> usize {
        self.inner.lock().drain_waiters
    }

    /// 阻塞到至少 `count` 个线程停在 [`Self::await_drain`] 里。
    ///
    /// 编排侧的 `FakeClock::advance` 只有在这之后才有意义：`advance` 先于期限装载发生，
    /// 期限的到期时刻就挂在推进之后的 `now` 上，此后没有任何一次推进会再让它到期。
    #[track_caller]
    pub fn wait_for_drain_waiters(&self, count: usize) {
        let locked = self.inner.lock();
        let waited = super::wait_until(&self.inner.condvar, locked, |s| s.drain_waiters >= count);
        // 同 `Barrier::wait_for_count`：先读、先放锁、再 panic，否则 panic 里会自死锁。
        if let Err(state) = waited {
            let actual = state.drain_waiters;
            let armed = state.drain_timer.is_some();
            drop(state);
            let here = std::panic::Location::caller();
            panic!(
                "`DrainBarrier::wait_for_drain_waiters({count})` 在真实时间 {:?} 内等不到：\
                 实际停在 `await_drain` 里的是 {actual} 个线程、drain 期限已装载={armed} —— \
                 被等的那个没有真的阻塞，编排顺序就没有被钉住。调用方 {}:{}",
                super::BLOCK_REAL_TIME_BUDGET,
                here.file(),
                here.line()
            );
        }
    }

    /// 阻塞到「有消费者」或「drain 期限到期」。返回是否因期限到期而截断。
    ///
    /// 这是唯一会真正阻塞的路径，仅用于证明 drain barrier 与 FakeClock 之间确实是唤醒关系；
    /// 顺序断言仍不依赖它（§6.4）。
    ///
    /// 等待带**真实时间**预算（见 [`super::wait_until`]）：`FakeClock::advance`
    /// 被掏空时不会有任何唤醒，原先这里是永久挂住；现在预算耗尽就 panic，
    /// 并在 panic 文本里带上调用点的 `file:line`。
    #[track_caller]
    pub fn await_drain(&self, execution_id: &ExecutionId) -> Option<TruncationReason> {
        let mut state = self.inner.lock();
        loop {
            if let Some(reason) = state.truncation_for(execution_id) {
                return Some(reason);
            }
            if !state.no_consumer {
                return None;
            }
            // 期限必须在 `wait` **之前**判一次。若时钟在本线程走到这里之前就已推进，
            // `condvar` 再也等不到一次唤醒，线程会永久挂住——这正是 CM-64 之前会死锁的原因。
            if let Some(reason) = self.arm_drain_and_check(&mut state) {
                Self::remember_truncation(&mut state, execution_id, reason);
                return Some(reason);
            }
            // 谓词挂在**单调状态**上，不挂在「有人 notify 过我」上：唤醒若在挂上之前
            // 送达就会被丢掉，而期限是否已到期是随时可重问的事实。
            let exhausted = {
                state.drain_waiters = state.drain_waiters.saturating_add(1);
                self.inner.condvar.notify_all();
                let (mut next, exhausted) =
                    match super::wait_until(&self.inner.condvar, state, |s| {
                        !s.no_consumer
                            || s.truncation_for(execution_id).is_some()
                            || self.drain_deadline_elapsed_in(s)
                    }) {
                        Ok(next) => (next, false),
                        Err(next) => (next, true),
                    };
                next.drain_waiters = next.drain_waiters.saturating_sub(1);
                state = next;
                self.inner.condvar.notify_all();
                exhausted
            };
            if exhausted {
                let no_consumer = state.no_consumer;
                drop(state);
                let here = std::panic::Location::caller();
                panic!(
                    "`DrainBarrier::await_drain({execution_id})` 在真实时间内等不到「有消费者」，\
                     也没等到 drain 期限到期 —— FakeClock 的单调时刻没有推进（`advance` 可能是坏的），\
                     于是 drain waker 从未触发。当前 no_consumer={no_consumer}、期限已到期={}。调用方 {}:{}",
                    self.drain_deadline_elapsed(),
                    here.file(),
                    here.line()
                );
            }
        }
    }

    /// 供 provider 组装 `ExecutionCompletion` 时取当前单调时刻。
    pub fn now(&self) -> MonoTime {
        self.inner.clock.monotonic()
    }
}

impl Default for DrainBarrier {
    fn default() -> Self {
        Self::new(FakeClock::new(), DrainLimits::lowered_for_tests())
    }
}
