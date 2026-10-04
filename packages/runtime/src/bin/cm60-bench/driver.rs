//! 基准用的假 `SessionPort`：固定 fake command + 确定性事件流。
//!
//! ## 为什么端口里那个 `sleep` 不是「真的在等」
//!
//! §11.2 把这件事写死了：「fake 命令的 10 毫秒由 `tokio::time::pause()` + auto-advance
//! 的虚拟时间消费，因此不进入测量窗口，但调度与分配开销仍然是真实的。」
//!
//! 基准进程在 [`crate::runner::run_bench`] 里建的是 **current_thread** 运行时并立刻
//! `tokio::time::pause()`。于是 [`FakeDriverPort::execute_in_session`] 里那句
//! `tokio::time::sleep(10ms).await` 消耗的是**虚拟时间**：tokio 在所有任务都挂在定时器上时
//! 直接把时钟推进到最近的下一次唤醒，真实世界一纳秒都没等。
//!
//! 这同时满足两件互相拉扯的事：
//! - **测量是真的**：两段窗口用的是 `std::time::Instant`（[`crate::clock::InstantClock`]），
//!   与 tokio 的暂停时钟完全无关，它照样走。量出来的就是真实的锁竞争、分配与调度开销。
//! - **被排除的那一段也是真的**：fake SQL 的 10 毫秒落在两段打点**之间**，
//!   结构性落在测量窗口外（网关 `mod.rs:355` 的注释就说得很清楚）。
//!
//! 真实代价（端口往返的 `std::time::Instant` 纳秒数）被**单独**记进产物
//! [`DriverJournal::round_trip_nanos`]。它不参与门禁，但它是那段被排除窗口的成本账——
//! 只说「不计入」而不说「多少」等于把这笔开销藏起来。
//!
//! ## 并发
//!
//! 锁**绝不跨越 `.await` 持有**：先在临界区里分配 `executionId`、记事件，释放锁，
//! 再去消费那 10 毫秒虚拟时间。于是 8 条并发请求真的能同时停在假命令上，
//! 测的才是「并发 8 下网关的开销」，而不是被一把锁串成 1 的伪并发。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use async_trait::async_trait;

use datazen_runtime::connection::{
    CloseMode, Counter, ExecuteInSessionRequest, ExecutionId, ExecutionReceipt, ExecutionState,
    RuntimeError, SessionHandle, SessionState, SessionView, StreamId,
};
use datazen_runtime::gateway::events::ExecutionEventKind;
use datazen_runtime::latency::nearest_rank_percentile;
use datazen_runtime::registry::SessionPort;

/// 每次执行发出的事件条数。
///
/// §11.3 的事件断言是「单次存活订阅、缓冲未溢出」下的 sequence 连续性。基准要能
/// 证明自己**采过**这条断言，发 0 条等于没采；发 1 条也证明不了「多片连续」。
/// 两条（一片 `ResultChunk` + 一个 `Terminal`）是最小的连续性证据。
pub const EVENTS_PER_EXECUTION: u32 = 2;

/// 端口的账本：调用次数 + 那段被排除的驱动往返耗时。
#[derive(Debug, Default)]
pub struct DriverJournal {
    session_view_calls: AtomicU64,
    execute_calls: AtomicU64,
    cancel_calls: AtomicU64,
    close_calls: AtomicU64,
    events_emitted: AtomicU64,
    /// 端口往返的真实耗时（纳秒），样本在 `execute_in_session` 内部采集。
    round_trips: Mutex<Vec<u64>>,
    dropped_round_trips: AtomicU64,
}

impl DriverJournal {
    fn record_round_trip(&self, nanos: u64) {
        match self.round_trips.lock() {
            Ok(mut samples) => samples.push(nanos),
            // 取不到锁时**丢弃这一次采样**而不是 panic：账本丢一个诊断数字，
            // 不能让整个基准进程挂掉。丢弃数由 `dropped_round_trips` 记账。
            Err(_) => {
                self.dropped_round_trips.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// 驱动往返耗时的 p95（纳秒）——**非门禁**列，只用于披露被排除窗口的成本。
    pub fn round_trip_nanos(&self) -> DriverRoundTrip {
        let Ok(samples) = self.round_trips.lock() else {
            return DriverRoundTrip::empty();
        };
        DriverRoundTrip {
            samples: samples.len(),
            p95_nanos: nearest_rank_percentile(&samples, 0.95),
            max_nanos: samples.iter().copied().max(),
        }
    }

    /// 驱动往返的中位数（纳秒），用于说明「虚拟 10 毫秒 ≠ 0 成本」。
    pub fn round_trip_median_nanos(&self) -> Option<u64> {
        let samples = self.round_trips.lock().ok()?.clone();
        nearest_rank_percentile(&samples, 0.50)
    }

    pub fn session_view_calls(&self) -> u64 {
        self.session_view_calls.load(Ordering::Relaxed)
    }
    pub fn execute_calls(&self) -> u64 {
        self.execute_calls.load(Ordering::Relaxed)
    }
    pub fn cancel_calls(&self) -> u64 {
        self.cancel_calls.load(Ordering::Relaxed)
    }
    pub fn close_calls(&self) -> u64 {
        self.close_calls.load(Ordering::Relaxed)
    }
    pub fn events_emitted(&self) -> u64 {
        self.events_emitted.load(Ordering::Relaxed)
    }
    pub fn round_trip_samples(&self) -> u64 {
        self.round_trips.lock().map(|s| s.len() as u64).unwrap_or(0)
    }
    pub fn dropped_round_trips(&self) -> u64 {
        self.dropped_round_trips.load(Ordering::Relaxed)
    }
}

/// 驱动往返耗时的诊断分位数（非门禁）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct DriverRoundTrip {
    pub samples: usize,
    pub p95_nanos: Option<u64>,
    pub max_nanos: Option<u64>,
}

impl DriverRoundTrip {
    /// 没有采到往返样本时的取值：两个分位数都是 `None`（不是 0）。
    pub fn empty() -> Self {
        Self {
            samples: 0,
            p95_nanos: None,
            max_nanos: None,
        }
    }
}

/// 事件投影的结论：重复数与丢失数，两者都必须为 0。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct ProjectionReport {
    /// 已应用事件数。
    pub applied: u64,
    /// 网关判定为「序号已见过」的重复事件数。
    pub duplicates: u64,
    /// 网关判定为「序号比水位旧」的迟到事件数。
    pub out_of_order: u64,
    /// 网关判定为「序号重复但行数不再累加」的重复分片数。
    pub duplicate_chunks: u64,
    /// 网关判定为「出现缺口」的事件数（这就是丢失）。
    pub lost: u64,
    /// 未绑定事件数。
    pub unbound: u64,
    /// 会话/epoch 不匹配被判丢弃的事件数：基准里恒为 0，非 0 说明事件串到了别的会话。
    pub foreign: u64,
    /// 已终结、未完成计数的执行数；末尾必须为 0。
    pub outstanding_at_end: u64,
}

impl ProjectionReport {
    /// §11.1 的门禁位：**事件重复/丢失数为 0**。
    pub fn is_clean(&self) -> bool {
        self.duplicates == 0
            && self.out_of_order == 0
            && self.duplicate_chunks == 0
            && self.lost == 0
            && self.unbound == 0
            && self.foreign == 0
            && self.outstanding_at_end == 0
    }

    /// 合并另一段投影（预热轮 + 各轮的投影累加到这里）。
    pub fn merge(&mut self, other: &ProjectionReport) {
        self.applied += other.applied;
        self.duplicates += other.duplicates;
        self.out_of_order += other.out_of_order;
        self.duplicate_chunks += other.duplicate_chunks;
        self.lost += other.lost;
        self.unbound += other.unbound;
        self.foreign += other.foreign;
        self.outstanding_at_end = other.outstanding_at_end.max(self.outstanding_at_end);
    }
}

/// 假驱动端口。
pub struct FakeDriverPort {
    inner: Mutex<DriverState>,
    journal: Arc<DriverJournal>,
    /// 假命令的虚拟耗时（§11.1 固定 10 毫秒）。
    fake_command: Duration,
    next_execution: AtomicU64,
    prefix: String,
}

struct DriverState {
    view: SessionView,
    /// 已派发但尚未收到终态事件的执行数：基准末尾必须为 0（泄漏检测）。
    outstanding: u64,
}

impl FakeDriverPort {
    /// 造一个端口，`fake_command` 是**虚拟**耗时。
    pub fn new(
        view: SessionView,
        fake_command: Duration,
        prefix: &str,
    ) -> (Arc<Self>, Arc<DriverJournal>) {
        let journal = Arc::new(DriverJournal::default());
        (
            Arc::new(Self {
                inner: Mutex::new(DriverState {
                    view,
                    outstanding: 0,
                }),
                journal: Arc::clone(&journal),
                fake_command,
                next_execution: AtomicU64::new(1),
                prefix: prefix.to_owned(),
            }),
            journal,
        )
    }

    /// 尚未收到终态事件的执行数。
    pub fn outstanding(&self) -> u64 {
        self.inner.lock().map(|s| s.outstanding).unwrap_or(0)
    }

    pub fn journal(&self) -> Arc<DriverJournal> {
        Arc::clone(&self.journal)
    }

    /// 收到某条执行的终态事件后递减未完成计数（§11.5 的泄漏对账）。
    pub fn mark_terminal(&self) {
        let Ok(mut state) = self.inner.lock() else {
            return;
        };
        state.outstanding = state.outstanding.saturating_sub(1);
    }

    /// 锁只保护计数与视图；取不到锁时返回错误而不是 panic。
    fn lock(&self) -> Result<MutexGuard<'_, DriverState>, RuntimeError> {
        self.inner
            .lock()
            .map_err(|_| RuntimeError::InvariantBroken("cm60BenchDriverPoisoned"))
    }
}

#[async_trait]
impl SessionPort for FakeDriverPort {
    async fn session_view(&self, handle: &SessionHandle) -> Result<SessionView, RuntimeError> {
        self.journal
            .session_view_calls
            .fetch_add(1, Ordering::Relaxed);
        let state = self.lock()?;
        if state.view.handle != *handle {
            return RuntimeError::UnknownSession(handle.db_session_id.as_str().to_owned())
                .rejected("cm60BenchSessionView");
        }
        Ok(state.view.clone())
    }

    /// 固定 fake 命令：登记事件 → **释放锁** → 消费 10 毫秒虚拟时间 → 交回回执。
    async fn execute_in_session(
        &self,
        request: ExecuteInSessionRequest,
    ) -> Result<ExecutionReceipt, RuntimeError> {
        let started = Instant::now();
        self.journal.execute_calls.fetch_add(1, Ordering::Relaxed);

        let execution_id = {
            let mut state = self.lock()?;
            if state.view.state == SessionState::Closed {
                return RuntimeError::SessionClosed(
                    state.view.handle.db_session_id.as_str().to_owned(),
                )
                .rejected("cm60BenchExecute");
            }
            if request.expected_context_revision != state.view.context_revision {
                return RuntimeError::ContextRevisionMismatch {
                    expected: request.expected_context_revision.get(),
                    actual: state.view.context_revision.get(),
                }
                .rejected("cm60BenchExecute");
            }
            let ordinal = self.next_execution.fetch_add(1, Ordering::Relaxed);
            let execution_id = ExecutionId::new(format!("{}-{:08}", self.prefix, ordinal));
            state.outstanding += 1;
            execution_id
        };
        // 事件是端口产出的，runner 稍后按序订阅并投给网关做连续性判定。
        self.journal
            .events_emitted
            .fetch_add(u64::from(EVENTS_PER_EXECUTION), Ordering::Relaxed);

        // 假 SQL 的 10 毫秒。tokio 的时钟被 `pause()` 了，这里消耗的是虚拟时间，
        // 真实等待为零；两段窗口用的是 `std::time::Instant`，不受影响。
        tokio::time::sleep(self.fake_command).await;

        let nanos = u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX);
        self.journal.record_round_trip(nanos);

        Ok(ExecutionReceipt {
            execution_id,
            stream_id: StreamId::new(format!("strm-{}", self.prefix)),
            state: ExecutionState::Queued,
        })
    }

    async fn cancel_execution(
        &self,
        handle: &SessionHandle,
        execution_id: &ExecutionId,
    ) -> Result<ExecutionState, RuntimeError> {
        self.journal.cancel_calls.fetch_add(1, Ordering::Relaxed);
        let _ = (handle, execution_id);
        Ok(ExecutionState::CancelRequested)
    }

    async fn close_session(
        &self,
        handle: &SessionHandle,
        _mode: CloseMode,
    ) -> Result<(), RuntimeError> {
        self.journal.close_calls.fetch_add(1, Ordering::Relaxed);
        let mut state = self.lock()?;
        if state.view.handle != *handle {
            return RuntimeError::UnknownSession(handle.db_session_id.as_str().to_owned())
                .rejected("cm60BenchClose");
        }
        state.view.state = SessionState::Closed;
        Ok(())
    }
}

/// 单条执行的事件序号：1..=EVENTS_PER_EXECUTION，末位是终态。
pub fn event_sequence() -> Vec<u64> {
    (1..=u64::from(EVENTS_PER_EXECUTION)).collect()
}

/// 序号对应的事件 kind：末位是终态，之前是结果分片。
pub fn event_kind(sequence: u64) -> ExecutionEventKind {
    if sequence == u64::from(EVENTS_PER_EXECUTION) {
        ExecutionEventKind::Terminal {
            state: ExecutionState::Succeeded,
        }
    } else {
        ExecutionEventKind::ResultChunk {
            chunk_index: Counter::new(sequence),
            rows: 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 事件序号必须从 1 连续到 `EVENTS_PER_EXECUTION`，末位是终态。
    /// 缺了连续性，事件段就等于没采。
    #[test]
    fn event_sequence_is_contiguous_and_ends_terminal() {
        assert_eq!(event_sequence(), vec![1, 2]);
        assert!(event_kind(1).eq(&ExecutionEventKind::ResultChunk {
            chunk_index: Counter::new(1),
            rows: 1,
        }));
        assert!(event_kind(2).is_terminal());
    }

    /// 投影报告的「干净」判定包含重复、迟到、缺口、未绑定、残留未完成五项。
    #[test]
    fn projection_report_gates_on_every_kind_of_defect() {
        let clean = ProjectionReport {
            applied: 4,
            outstanding_at_end: 0,
            ..ProjectionReport::default()
        };
        assert!(clean.is_clean());
        for broken in [
            ProjectionReport {
                duplicates: 1,
                ..clean
            },
            ProjectionReport {
                out_of_order: 1,
                ..clean
            },
            ProjectionReport {
                duplicate_chunks: 1,
                ..clean
            },
            ProjectionReport { lost: 1, ..clean },
            ProjectionReport {
                unbound: 1,
                ..clean
            },
            ProjectionReport {
                outstanding_at_end: 1,
                ..clean
            },
        ] {
            assert!(!broken.is_clean(), "{broken:?} 不该被判干净");
        }
    }

    /// 空账本的分位数是 `None`，不是 0：没测到 ≠ 0 纳秒。
    #[test]
    fn empty_journal_reports_none_not_zero() {
        let journal = DriverJournal::default();
        assert_eq!(journal.round_trip_nanos().p95_nanos, None);
        assert_eq!(journal.round_trip_median_nanos(), None);
    }
}
