//! CM-55：事件投递的容错。
//!
//! 事件通道不是可信的：它会**乱序**、**重复**、**跨 epoch**、甚至带着别的会话的残帧。
//! §12.1 的硬要求是「旧 session 的事件不得更新新会话」，CM-55 把它展开成四条可执行规则：
//!
//! 1. **陈旧 epoch / 陈旧会话 id 的事件一律丢弃**，不覆盖任何状态；
//! 2. **乱序与重复事件被识别并丢弃**，不推进状态机，也不**重复落库**；
//! 3. **大 Counter 必须精确**：不回绕、不截断、不饱和成别的值；
//! 4. **出现序号缺口时不猜**：拒绝投递并要求调用方**读快照或补订阅**。
//!
//! 第 4 条是本模块最容易被写错的地方。缺口最诱人的处理是「把收到的事件先应用了，
//! 反正后面几帧会补上」——但中间丢的那帧可能正是 `Terminal`，于是执行永远停在
//! `Running`。宁可显式进入恢复流程，也不要让状态机在缺帧的情况下静默前移。

use std::collections::BTreeSet;

use crate::connection::{
    ContextConfidence, Counter, DbSessionId, ExecutionId, ExecutionState, SessionView,
};

use crate::gateway::provenance::ExecutionSource;

/// 事件种类。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionEventKind {
    /// §7.2 第 9/10 步：上下文观测。
    ContextObserved { confidence: ContextConfidence },
    /// 流式结果分片。`rows` 是本片行数；重复片不得让行数再涨。
    ResultChunk { chunk_index: Counter, rows: u64 },
    /// 非终态状态迁移。
    StateChanged { state: ExecutionState },
    /// 终态。到达它之前执行都还没结束。
    Terminal { state: ExecutionState },
}

impl ExecutionEventKind {
    pub fn is_terminal(&self) -> bool {
        matches!(self, ExecutionEventKind::Terminal { .. })
    }
}

/// 一条执行事件。
///
/// `declared_source` 是**对抗性字段**：真实事件流并不携带来源，
/// 它被放进来是为了让「后来的事件不得改写请求来源」这条规则有一个真实的对手。
/// [`EventStore::apply`] 会无条件丢弃它并计数（见
/// [`EventStore::declared_source_events_ignored`]）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionEvent {
    /// 事件归属的执行。**必须显式携带**：同一个会话里可以并发多条执行，
    /// 靠 `(dbSessionId, runtimeEpoch)` 反查归属是不确定的。
    /// 注意它并**不**取代下面的 epoch 校验——驱动重启后可能复用旧的
    /// `executionId`，那种帧要靠 `runtime_epoch` 认出来。
    pub execution_id: ExecutionId,
    pub db_session_id: DbSessionId,
    pub runtime_epoch: Counter,
    pub sequence: Counter,
    pub context_revision: Counter,
    pub kind: ExecutionEventKind,
    pub declared_source: Option<ExecutionSource>,
}

/// 投递结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventDisposition {
    /// 已应用并推进状态机。
    Applied,
    /// 事件属于别的会话（旧会话残留）。
    IgnoredStaleSession { bound: DbSessionId },
    /// 事件的 epoch 比绑定旧——资源已被替换过。
    IgnoredStaleEpoch { bound: Counter },
    /// 事件的 epoch 比绑定新——绑定已过期，本执行不属于那个资源。
    IgnoredForeignEpoch { bound: Counter },
    /// 序号已经见过（纯重投）。
    IgnoredDuplicate { sequence: Counter },
    /// 序号比水位旧（乱序迟到的帧）。
    IgnoredOutOfOrder {
        sequence: Counter,
        watermark: Counter,
    },
    /// 分片序号已存在：序号被消费，但**行数不再累加**。
    IgnoredDuplicateChunk { chunk_index: Counter },
    /// 出现缺口：状态机**没有**前移，需要读快照或补订阅。
    Gap {
        expected: Counter,
        received: Counter,
    },
    /// 尚未绑定会话：事件早于任何一次受理。
    Unbound,
}

impl EventDisposition {
    /// 是否真的推进了状态机。恢复流程只关心这个。
    pub fn mutated_state(self) -> bool {
        matches!(self, EventDisposition::Applied)
    }

    /// 是否需要调用方介入（缺口 / 未绑定）。
    pub fn needs_recovery(self) -> bool {
        matches!(
            self,
            EventDisposition::Gap { .. } | EventDisposition::Unbound
        )
    }
}

/// 会话事件水位。
#[derive(Debug, Clone, PartialEq, Eq)]
struct SessionWatermark {
    db_session_id: DbSessionId,
    runtime_epoch: Counter,
    /// `None` = 还没收到过任何帧（刚建立订阅），此时任意序号都是合法的首帧。
    last_sequence: Option<Counter>,
    context_revision: Counter,
    observed_chunks: BTreeSet<Counter>,
    row_count: u64,
    state: ExecutionState,
    declared_source_events_ignored: u64,
}

impl SessionWatermark {
    fn new(db_session_id: DbSessionId, runtime_epoch: Counter) -> Self {
        Self {
            db_session_id,
            runtime_epoch,
            last_sequence: None,
            context_revision: Counter::ZERO,
            observed_chunks: BTreeSet::new(),
            row_count: 0,
            state: ExecutionState::Queued,
            declared_source_events_ignored: 0,
        }
    }

    /// 下一帧期望的序号。`None` 表示尚无期望值。
    fn expected_sequence(&self) -> Option<Counter> {
        self.last_sequence.map(|value| value.saturating_increment())
    }
}

/// 事件投递状态。
///
/// 绑定在**受理**时一次性确定，之后不再改变：一次执行只属于一个
/// `(dbSessionId, runtimeEpoch)`，任何不匹配的帧都是别的执行的。
#[derive(Clone)]
pub struct EventStore {
    watermark: Option<SessionWatermark>,
    /// 最近一次缺口，调用方据此决定补订阅起点。
    resubscribe_from: Option<Counter>,
}

impl Default for EventStore {
    fn default() -> Self {
        Self::new()
    }
}

impl EventStore {
    pub fn new() -> Self {
        Self {
            watermark: None,
            resubscribe_from: None,
        }
    }

    /// 绑定会话。已经绑定时**不**改绑——改绑等于让旧执行的帧污染新执行。
    pub fn bind(&mut self, db_session_id: DbSessionId, runtime_epoch: Counter) {
        match &self.watermark {
            None => {
                self.watermark = Some(SessionWatermark::new(db_session_id, runtime_epoch));
                self.resubscribe_from = None;
            }
            Some(existing)
                if existing.db_session_id == db_session_id
                    && existing.runtime_epoch == runtime_epoch => {}
            Some(_) => {
                // 已有绑定：新执行另起一个网关实例，网关负责这件事。
                // 这里保持旧绑定不动，宁可拒绝新帧也不让旧帧落到新水位上。
            }
        }
    }

    pub fn is_bound(&self) -> bool {
        self.watermark.is_some()
    }

    pub fn bound_session(&self) -> Option<&DbSessionId> {
        self.watermark.as_ref().map(|w| &w.db_session_id)
    }

    pub fn bound_epoch(&self) -> Option<Counter> {
        self.watermark.as_ref().map(|w| w.runtime_epoch)
    }

    /// 序号水位。
    pub fn last_sequence(&self) -> Option<Counter> {
        self.watermark.as_ref().and_then(|w| w.last_sequence)
    }

    /// 当前观测到的 `contextRevision`。大 Counter 精确保存。
    pub fn context_revision(&self) -> Counter {
        self.watermark
            .as_ref()
            .map(|w| w.context_revision)
            .unwrap_or(Counter::ZERO)
    }

    /// 已累计行数。重复分片不会让它再涨。
    pub fn row_count(&self) -> u64 {
        self.watermark.as_ref().map(|w| w.row_count).unwrap_or(0)
    }

    /// 当前执行状态。
    pub fn execution_state(&self) -> ExecutionState {
        self.watermark
            .as_ref()
            .map(|w| w.state)
            .unwrap_or(ExecutionState::Queued)
    }

    /// 已经被丢弃的「事件自带来源」次数。CM-61 的直接观测点。
    pub fn declared_source_events_ignored(&self) -> u64 {
        self.watermark
            .as_ref()
            .map(|w| w.declared_source_events_ignored)
            .unwrap_or(0)
    }

    /// 是否处于「必须恢复」状态（出现过缺口）。
    pub fn needs_recovery(&self) -> bool {
        self.resubscribe_from.is_some()
    }

    /// 补订阅起点：缺口前最后一个已应用序号。
    pub fn resubscribe_from(&self) -> Option<Counter> {
        self.resubscribe_from
    }

    /// 按快照恢复。
    ///
    /// 快照**只用来补齐缺口，不是用来重新开始**。已经积累下来的执行事实
    /// ——序号水位、已见分片、累计行数、状态机位置、被丢弃的伪来源计数——一律保留：
    ///
    /// - 重建水位会把已经到终态的执行倒回 `Queued`；
    /// - 会把累计行数清零，于是同一批结果在快照后又被当作新结果累加一遍；
    /// - 会让缺口之前已应用的序号重新变成「未见过」，重投的旧帧被二次计入
    ///   （等于恢复路径自己把 CM-55 再打开一次）。
    ///
    /// 因此这里只更新两样东西：`contextRevision`（与 `apply` 同一口径，只增不减，
    /// 免得一份陈旧快照把已观测到的版本号拉低）和缺口标记。
    /// 绑定 `(dbSessionId, runtimeEpoch)` 在受理时确定，恢复不改绑——改绑等于让
    /// 别的执行的帧凭一张快照混进来。
    pub fn recover_from_snapshot(&mut self, view: &SessionView) {
        match &mut self.watermark {
            Some(watermark) => {
                if view.context_revision > watermark.context_revision {
                    watermark.context_revision = view.context_revision;
                }
            }
            None => {
                let mut watermark = SessionWatermark::new(
                    view.handle.db_session_id.clone(),
                    view.handle.runtime_epoch,
                );
                watermark.context_revision = view.context_revision;
                self.watermark = Some(watermark);
            }
        }
        self.resubscribe_from = None;
    }

    /// 投递一帧。
    pub fn apply(&mut self, event: &ExecutionEvent) -> EventDisposition {
        let watermark = match &mut self.watermark {
            Some(w) => w,
            None => return EventDisposition::Unbound,
        };

        // 1) 会话 id 必须逐字段相等：旧会话残留一律丢弃。
        if event.db_session_id != watermark.db_session_id {
            return EventDisposition::IgnoredStaleSession {
                bound: watermark.db_session_id.clone(),
            };
        }

        // 2) epoch 必须**精确**相等。旧的是资源被替换，新的是本执行不属于该资源。
        if event.runtime_epoch < watermark.runtime_epoch {
            return EventDisposition::IgnoredStaleEpoch {
                bound: watermark.runtime_epoch,
            };
        }
        if event.runtime_epoch > watermark.runtime_epoch {
            return EventDisposition::IgnoredForeignEpoch {
                bound: watermark.runtime_epoch,
            };
        }

        // 3) 序号闸门。
        match watermark.last_sequence {
            Some(last) if event.sequence > last => {}
            Some(last) if event.sequence == last => {
                return EventDisposition::IgnoredDuplicate {
                    sequence: event.sequence,
                };
            }
            Some(last) => {
                return EventDisposition::IgnoredOutOfOrder {
                    sequence: event.sequence,
                    watermark: last,
                };
            }
            None => {}
        }
        if let Some(expected) = watermark.expected_sequence() {
            if event.sequence != expected {
                // 缺口：**不前移状态机**，只记下补订阅起点。
                self.resubscribe_from = watermark.last_sequence;
                return EventDisposition::Gap {
                    expected,
                    received: event.sequence,
                };
            }
        }

        // 序号合法。事件自带的来源一律丢弃（CM-61）。
        if event.declared_source.is_some() {
            watermark.declared_source_events_ignored =
                watermark.declared_source_events_ignored.saturating_add(1);
        }

        // contextRevision 只增不减：乱序帧不得把已观测到的修订号拉回去。
        if event.context_revision > watermark.context_revision {
            watermark.context_revision = event.context_revision;
        }

        // 分片去重：序号已消费，行数不再累加。
        if let ExecutionEventKind::ResultChunk { chunk_index, rows } = &event.kind {
            if !watermark.observed_chunks.insert(*chunk_index) {
                watermark.last_sequence = Some(event.sequence);
                return EventDisposition::IgnoredDuplicateChunk {
                    chunk_index: *chunk_index,
                };
            }
            watermark.row_count = watermark.row_count.saturating_add(*rows);
        }

        match &event.kind {
            ExecutionEventKind::ContextObserved { .. } => {}
            ExecutionEventKind::ResultChunk { .. } => {}
            ExecutionEventKind::StateChanged { state } => {
                watermark.state = *state;
            }
            ExecutionEventKind::Terminal { state } => {
                watermark.state = *state;
            }
        }

        watermark.last_sequence = Some(event.sequence);
        EventDisposition::Applied
    }
}

impl std::fmt::Debug for EventStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventStore")
            .field("watermark", &self.watermark)
            .field("resubscribe_from", &self.resubscribe_from)
            .finish()
    }
}
