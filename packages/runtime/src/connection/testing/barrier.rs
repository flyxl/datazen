//! 命令级 barrier 与协议 drain barrier（fake-runtime-fixtures.md §6）。
//!
//! 依赖方向：本模块只向下依赖 `clock`（FakeClock）与非夹具的 `connection::execution`，
//! 不依赖 `fake_resource`，也不被 `journal` 依赖。
//!
//! §6.4 的纪律：顺序断言**只能**基于 journal `seq` 或本模块的 `Barrier`；
//! 「裸 `sleep(...)` 后直接断言顺序」在本模块不存在，也不应被引入。

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::Duration;

use crate::connection::execution::{ExecutionErrorCode, ResultSink, SinkWrite, TruncationReason};
use crate::connection::types::ExecutionId;

use super::clock::{FakeClock, MonoTime, TimerId};

// ---------------------------------------------------------------------------
// Barrier —— 命令级同步点
// ---------------------------------------------------------------------------

/// 到达凭证。`seq` 来自单一原子计数器，因此并发写入的**相对顺序是确定的**（§5 L269）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BarrierToken {
    pub seq: u64,
    pub tag: String,
}

/// 一次到达。顺序断言只读它。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arrival {
    pub seq: u64,
    pub tag: String,
}

#[derive(Debug, Default)]
struct BarrierState {
    arrivals: Vec<Arrival>,
    released: BTreeSet<String>,
}

struct BarrierShared {
    next_seq: AtomicU64,
    state: Mutex<BarrierState>,
    /// 每个 tag 一个条件变量唤醒集合；用单个 Condvar + 谓词重检即可，避免漏唤醒。
    condvar: Condvar,
}

impl BarrierShared {
    fn lock(&self) -> MutexGuard<'_, BarrierState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// 命令级 barrier。
///
/// 语义：某条执行路径 `arrive("eviction-close-precheck")` 后**停住**，
/// 测试在拿到 `wait_for` 确认后再 `release` 它。这替代「睡一会儿猜对方跑到哪了」。
#[derive(Clone)]
pub struct Barrier {
    inner: Arc<BarrierShared>,
}

impl Barrier {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(BarrierShared {
                next_seq: AtomicU64::new(1),
                state: Mutex::new(BarrierState::default()),
                condvar: Condvar::new(),
            }),
        }
    }

    /// 到达某个同步点。
    pub fn arrive(&self, tag: &str) -> BarrierToken {
        let seq = self.inner.next_seq.fetch_add(1, Ordering::SeqCst);
        {
            let mut state = self.inner.lock();
            state.arrivals.push(Arrival {
                seq,
                tag: tag.to_string(),
            });
        }
        self.inner.condvar.notify_all();
        BarrierToken {
            seq,
            tag: tag.to_string(),
        }
    }

    /// 阻塞到 `tag` 至少被到达一次。
    pub fn wait_for(&self, tag: &str) {
        let mut state = self.inner.lock();
        while !state.arrivals.iter().any(|a| a.tag == tag) {
            state = self
                .inner
                .condvar
                .wait(state)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
    }

    /// 阻塞到 `tag` 被到达 `count` 次。
    pub fn wait_for_count(&self, tag: &str, count: usize) {
        let mut state = self.inner.lock();
        while state.arrivals.iter().filter(|a| a.tag == tag).count() < count {
            state = self
                .inner
                .condvar
                .wait(state)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
    }

    /// 释放某个同步点上阻塞的路径。
    pub fn release(&self, tag: &str) {
        {
            let mut state = self.inner.lock();
            state.released.insert(tag.to_string());
        }
        self.inner.condvar.notify_all();
    }

    /// 阻塞到 `tag` 被释放。
    pub fn wait_released(&self, tag: &str) {
        let mut state = self.inner.lock();
        while !state.released.contains(tag) {
            state = self
                .inner
                .condvar
                .wait(state)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
    }

    pub fn is_released(&self, tag: &str) -> bool {
        self.inner.lock().released.contains(tag)
    }

    pub fn arrival_count(&self, tag: &str) -> usize {
        self.inner
            .lock()
            .arrivals
            .iter()
            .filter(|a| a.tag == tag)
            .count()
    }

    /// 全部到达，按 `seq` 升序。并发写入的确定性顺序就靠它断言。
    pub fn order(&self) -> Vec<Arrival> {
        let state = self.inner.lock();
        let mut arrivals = state.arrivals.clone();
        arrivals.sort_by_key(|a| a.seq);
        arrivals
    }

    /// 断言 `a` 先于 `b` 到达。顺序只来自 `seq`，不来自挂钟。
    pub fn assert_arrived_before(&self, a: &str, b: &str) {
        let state = self.inner.lock();
        let first = state.arrivals.iter().find(|x| x.tag == a).map(|x| x.seq);
        let second = state.arrivals.iter().find(|x| x.tag == b).map(|x| x.seq);
        match (first, second) {
            (Some(x), Some(y)) => assert!(
                x < y,
                "barrier 顺序断言失败：{a}(seq={x}) 必须先于 {b}(seq={y})"
            ),
            _ => {
                panic!("barrier 顺序断言失败：{a}/{b} 未全部到达（{a}={first:?}, {b}={second:?}）")
            }
        }
    }
}

impl Default for Barrier {
    fn default() -> Self {
        Self::new()
    }
}

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
        let Some(timer) = state.drain_timer else {
            return false;
        };
        self.inner
            .clock
            .fired_history()
            .iter()
            .any(|fired| fired.id == timer)
    }

    /// 阻塞到「有消费者」或「drain 期限到期」。返回是否因期限到期而截断。
    ///
    /// 这是唯一会真正阻塞的路径，仅用于证明 drain barrier 与 FakeClock 之间确实是唤醒关系；
    /// 顺序断言仍不依赖它（§6.4）。
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
            state = self
                .inner
                .condvar
                .wait(state)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
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

// ---------------------------------------------------------------------------
// 供 provider / 测试使用的 sink
// ---------------------------------------------------------------------------

/// 收集型 sink：把写入落到内存，供断言「已产出字节」。
#[derive(Debug, Default)]
pub struct CollectingSink {
    produced_bytes: u64,
    events: u64,
    completed: Option<u64>,
    failure: Option<ExecutionErrorCode>,
}

impl CollectingSink {
    pub fn produced_bytes(&self) -> u64 {
        self.produced_bytes
    }

    pub fn events(&self) -> u64 {
        self.events
    }

    pub fn completed_at(&self) -> Option<u64> {
        self.completed
    }

    pub fn failure(&self) -> Option<ExecutionErrorCode> {
        self.failure
    }
}

impl ResultSink for CollectingSink {
    fn write(
        &mut self,
        _execution_id: &ExecutionId,
        _chunk_index: crate::connection::types::Counter,
        bytes: usize,
    ) -> SinkWrite {
        self.produced_bytes += bytes as u64;
        self.events += 1;
        SinkWrite::Accepted
    }

    fn complete(&mut self, _execution_id: &ExecutionId, produced_bytes: u64) {
        self.completed = Some(produced_bytes);
    }

    fn fail(&mut self, _execution_id: &ExecutionId, code: ExecutionErrorCode, produced_bytes: u64) {
        self.failure = Some(code);
        self.completed = Some(produced_bytes);
    }
}

/// 写失败型 sink：F5「`ResultSink` 写入失败」的载体。
#[derive(Debug, Default)]
pub struct FailingSink {
    pub code: Option<ExecutionErrorCode>,
}

impl ResultSink for FailingSink {
    fn write(
        &mut self,
        _execution_id: &ExecutionId,
        _chunk_index: crate::connection::types::Counter,
        _bytes: usize,
    ) -> SinkWrite {
        SinkWrite::Truncated(TruncationReason::ProducerWriteFailed)
    }

    fn complete(&mut self, _execution_id: &ExecutionId, produced_bytes: u64) {
        let _ = produced_bytes;
    }

    fn fail(&mut self, _execution_id: &ExecutionId, code: ExecutionErrorCode, produced_bytes: u64) {
        self.code = Some(code);
        let _ = produced_bytes;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::types::Counter;

    #[test]
    fn arrival_order_comes_from_seq_not_from_timing() {
        let barrier = Barrier::new();
        let first = barrier.arrive("session-created");
        let second = barrier.arrive("eviction-close-precheck");
        assert!(first.seq < second.seq, "先到达的 seq 必须更小");
        barrier.assert_arrived_before("session-created", "eviction-close-precheck");
        let arrivals = barrier.order();
        let order: Vec<&str> = arrivals.iter().map(|a| a.tag.as_str()).collect();
        assert_eq!(order, vec!["session-created", "eviction-close-precheck"]);
    }

    #[test]
    fn concurrent_arrivals_still_produce_a_deterministic_sequence() {
        let barrier = Barrier::new();
        let mut handles = Vec::new();
        for _ in 0..4 {
            let barrier = barrier.clone();
            handles.push(std::thread::spawn(move || barrier.arrive("racing")));
        }
        for handle in handles {
            handle.join().expect("线程必须正常结束");
        }
        barrier.wait_for_count("racing", 4);
        assert_eq!(barrier.arrival_count("racing"), 4);
        let seqs: Vec<u64> = barrier.order().into_iter().map(|a| a.seq).collect();
        let mut sorted = seqs.clone();
        sorted.sort_unstable();
        assert_eq!(seqs, sorted, "seq 必须唯一且单调，顺序可断言");
        sorted.dedup();
        assert_eq!(sorted.len(), 4, "并发到达不得产生重复 seq");
    }

    #[test]
    fn wait_for_blocks_until_the_tag_is_reached() {
        let barrier = Barrier::new();
        let worker = {
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                // `wait_for` 等的是「tag 已被到达」；必须先 arrive 才可能有人观察到。
                barrier.arrive("hold-point");
                barrier.wait_for("hold-point");
                barrier.release("hold-point");
            })
        };
        barrier.wait_for("hold-point");
        barrier.wait_released("hold-point");
        assert!(barrier.is_released("hold-point"));
        worker.join().expect("线程必须正常结束");
    }

    #[test]
    fn a_blocked_path_only_continues_after_release() {
        // §9.3：淘汰在关闭前置检查处挂起；此时发起 begin，落点必须在同一资源上。
        let barrier = Barrier::new();
        barrier.arrive("eviction-close-precheck");
        assert!(!barrier.is_released("eviction-close-precheck"));
        barrier.release("eviction-close-precheck");
        assert!(barrier.is_released("eviction-close-precheck"));
        // 放行之后才到达的续点：顺序断言必须建立在**两个**真实到达之上。
        barrier.arrive("eviction-released");
        barrier.assert_arrived_before("eviction-close-precheck", "eviction-released");
    }

    #[test]
    fn write_without_consumer_blocks_then_truncates_after_the_drain_deadline() {
        // §6.2 竞态表 CM-64：无消费者 → 写入等待 → FakeClock 推进 10s → 截断。
        let clock = FakeClock::new();
        let drain = DrainBarrier::new(clock.clone(), DrainLimits::lowered_for_tests());
        drain.set_no_consumer(true);
        let execution = ExecutionId::new("exe_dbs_w1_0001_0001");

        assert_eq!(drain.write(&execution, 0, 1024), SinkWrite::Blocked);
        assert_eq!(drain.write(&execution, 1, 1024), SinkWrite::Blocked);
        assert!(
            !drain.drain_deadline_elapsed(),
            "未推进时钟前期限不得视为到期"
        );
        assert_eq!(drain.truncation_of(&execution), None);

        clock.advance(Duration::from_secs(10));
        assert!(drain.drain_deadline_elapsed());
        assert_eq!(
            drain.write(&execution, 2, 1024),
            SinkWrite::Truncated(TruncationReason::NoConsumerDrainDeadline)
        );
        assert_eq!(
            drain.truncation_of(&execution),
            Some(TruncationReason::NoConsumerDrainDeadline)
        );
        assert_eq!(drain.produced_bytes(), 0, "无消费者时不得计为已产出");
    }

    #[test]
    fn a_consumer_restores_acceptance_without_truncation() {
        let clock = FakeClock::new();
        let drain = DrainBarrier::new(clock.clone(), DrainLimits::lowered_for_tests());
        let execution = ExecutionId::new("exe_dbs_w1_0001_0001");
        drain.set_no_consumer(true);
        assert_eq!(drain.write(&execution, 0, 512), SinkWrite::Blocked);
        drain.set_no_consumer(false);
        assert_eq!(drain.write(&execution, 1, 512), SinkWrite::Accepted);
        clock.advance(Duration::from_secs(600));
        assert_eq!(
            drain.truncation_of(&execution),
            None,
            "消费者在期限内接管不得产生截断"
        );
        assert_eq!(drain.produced_bytes(), 512);
        assert_eq!(drain.produced_events(), 1);
    }

    #[test]
    fn per_execution_byte_limit_truncates_with_its_own_reason() {
        // §6.2：每执行 8 MiB → 测试下调到 64 KiB；触顶后必须截断或按字节背压。
        let clock = FakeClock::new();
        let drain = DrainBarrier::new(clock, DrainLimits::lowered_for_tests());
        let execution = ExecutionId::new("exe_dbs_w1_0001_0002");
        assert_eq!(drain.write(&execution, 0, 32 * 1024), SinkWrite::Accepted);
        assert_eq!(drain.write(&execution, 1, 32 * 1024), SinkWrite::Accepted);
        assert_eq!(
            drain.write(&execution, 2, 1),
            SinkWrite::Truncated(TruncationReason::PerExecutionByteLimit)
        );
        assert_eq!(
            drain.truncation_of(&execution),
            Some(TruncationReason::PerExecutionByteLimit)
        );
        drain.finish_execution(&execution);
        assert_eq!(
            drain.write(&execution, 3, 1),
            SinkWrite::Accepted,
            "新执行应重新计量"
        );
    }

    #[test]
    fn subscription_limits_are_counted_independently_of_the_execution_limit() {
        let clock = FakeClock::new();
        let limits = DrainLimits {
            events_per_subscription: 3,
            bytes_per_subscription: 1024 * 1024,
            ..DrainLimits::lowered_for_tests()
        };
        let drain = DrainBarrier::new(clock, limits);
        let a = ExecutionId::new("exe_dbs_w1_0001_0001");
        let b = ExecutionId::new("exe_dbs_w1_0001_0002");
        for index in 0..3 {
            assert_eq!(drain.write(&a, index, 8), SinkWrite::Accepted);
        }
        assert_eq!(
            drain.write(&b, 0, 8),
            SinkWrite::Truncated(TruncationReason::PerSubscriptionEventLimit)
        );
        assert_eq!(
            drain.truncation_of(&b),
            Some(TruncationReason::PerSubscriptionEventLimit)
        );
        assert_eq!(
            drain.truncation_of(&a),
            None,
            "订阅上限不得追溯到先前已接受的执行"
        );
    }

    #[test]
    fn await_drain_wakes_when_the_clock_crosses_the_deadline() {
        let clock = FakeClock::new();
        let drain = DrainBarrier::new(clock.clone(), DrainLimits::design());
        drain.set_no_consumer(true);
        let execution = ExecutionId::new("exe_dbs_w1_0001_0001");
        assert_eq!(drain.write(&execution, 0, 64), SinkWrite::Blocked);

        let waiter = {
            let drain = drain.clone();
            let execution = execution.clone();
            std::thread::spawn(move || drain.await_drain(&execution))
        };
        clock.advance(Duration::from_secs(10));
        let reason = waiter.join().expect("线程必须正常结束");
        assert_eq!(reason, Some(TruncationReason::NoConsumerDrainDeadline));
    }

    #[test]
    fn collecting_sink_counts_produced_bytes_and_terminal_state() {
        let mut sink = CollectingSink::default();
        let execution = ExecutionId::new("exe_dbs_w1_0001_0001");
        assert_eq!(
            sink.write(&execution, Counter::new(0), 100),
            SinkWrite::Accepted
        );
        assert_eq!(
            sink.write(&execution, Counter::new(1), 150),
            SinkWrite::Accepted
        );
        sink.complete(&execution, 250);
        assert_eq!(sink.produced_bytes(), 250);
        assert_eq!(sink.events(), 2);
        assert_eq!(sink.completed_at(), Some(250));
        assert_eq!(sink.failure(), None);
    }

    #[test]
    fn failing_sink_reports_producer_write_failed() {
        // F5：`ResultSink` 写入失败必须带 `producerWriteFailed`，不得静默成功。
        let mut sink = FailingSink::default();
        let execution = ExecutionId::new("exe_dbs_w1_0001_0001");
        assert_eq!(
            sink.write(&execution, Counter::new(0), 10),
            SinkWrite::Truncated(TruncationReason::ProducerWriteFailed)
        );
        sink.fail(&execution, ExecutionErrorCode::ProtocolError, 0);
        assert_eq!(sink.code, Some(ExecutionErrorCode::ProtocolError));
    }
}
