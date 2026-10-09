//! 替换提交的操作记录。
//!
//! 替换是最容易出「旧的和新的同时跑起来」的地方，因此这里的模型只有一条规矩：
//! **在知道结果之前，两边都不可路由。**
//!
//! ```text
//!   Prepared ──Committed──► Committed   （新条目放行，旧条目改关闭路由）
//!      │
//!      ├──RolledBack──────► RolledBack  （新条目销毁，旧条目原样恢复）
//!      │
//!      └─ 应答丢失 ───────► Undecided   （两边都不可路由）
//!                                │
//!                          commit_status(key)
//!                                ▼
//!                          Committed / RolledBack
//! ```
//!
//! `Undecided` 就是屏障。目录**不会**去猜「大概率成功了」，
//! 调用方拿不到确认就按 operation key 来查，查清楚之前旧会话不会继续执行。

use std::fmt;

use datazen_platform_api::dto::session::SessionHandle;
use datazen_platform_api::ports::session_directory::{ReplacementOperation, SessionOwner};

/// 替换操作键。提交结果未知时，调用方靠它查状态。
///
/// 键由**旧句柄**确定性地推导，因此调用方不必先做一次额外往返就能算出自己的键——
/// 避免了「查状态」这一步本身依赖一次可能失败的通信。
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReplacementOperationKey(String);

impl ReplacementOperationKey {
    pub fn new(raw: impl Into<String>) -> Self {
        Self(raw.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 由旧句柄推导。一次替换只针对一个旧句柄，因此这是无冲突的。
    pub fn for_handle(handle: &SessionHandle) -> Self {
        Self(format!(
            "rep_{}@{}",
            handle.db_session_id, handle.runtime_epoch
        ))
    }
}

impl fmt::Debug for ReplacementOperationKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ReplacementOperationKey({})", self.0)
    }
}

impl fmt::Display for ReplacementOperationKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// 一次替换在目录内部的状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RecordState {
    /// 新会话已登记，等原子切换。此刻新条目**不可路由**。
    Prepared,
    /// 提交动作已发出但结果未知。**屏障**：新旧都不可路由。
    Undecided,
    /// 已切换。新条目可路由，旧条目改关闭路由。
    Committed,
    /// 已回滚。新条目销毁，旧条目原样恢复。
    RolledBack,
}

impl RecordState {
    pub const fn as_str(self) -> &'static str {
        match self {
            RecordState::Prepared => "prepared",
            RecordState::Undecided => "undecided",
            RecordState::Committed => "committed",
            RecordState::RolledBack => "rolledBack",
        }
    }

    /// 屏障是否还举着。
    pub const fn is_barrier(self) -> bool {
        matches!(self, RecordState::Prepared | RecordState::Undecided)
    }
}

/// `commit_status` 的回答。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitStatus {
    /// 操作没登记，或者还举着屏障且尚未查清。
    Pending,
    /// 已提交：新条目现在可路由。
    Committed { handle: SessionHandle },
    /// 已回滚：旧条目已恢复可路由。
    RolledBack { restored: SessionHandle },
}

impl CommitStatus {
    pub fn is_pending(&self) -> bool {
        matches!(self, CommitStatus::Pending)
    }

    pub const fn is_committed(&self) -> bool {
        matches!(self, CommitStatus::Committed { .. })
    }

    pub const fn is_rolled_back(&self) -> bool {
        matches!(self, CommitStatus::RolledBack { .. })
    }
}

/// 一次替换的记录。留在内存里，没有墓碑、没有落盘。
#[derive(Debug, Clone)]
pub struct ReplacementRecord {
    key: ReplacementOperationKey,
    old: SessionHandle,
    new_owner: SessionOwner,
    state: RecordState,
    /// 举着屏障时，服务端**实际**已经落定的结果。
    ///
    /// 这不是「猜测的结果」：屏障挡的是调用方，切换动作早已原子生效，
    /// 缺的只是一次确认。所以按 key 查询状态时能给出确定答案，
    /// 而不是让调用方在 Committed / RolledBack 之间二选一。
    undecided_truth: Option<RecordState>,
}

impl ReplacementRecord {
    pub fn prepared(
        key: ReplacementOperationKey,
        old: SessionHandle,
        new_owner: SessionOwner,
    ) -> Self {
        Self {
            key,
            old,
            new_owner,
            state: RecordState::Prepared,
            undecided_truth: None,
        }
    }

    pub const fn key(&self) -> &ReplacementOperationKey {
        &self.key
    }

    pub const fn old(&self) -> &SessionHandle {
        &self.old
    }

    pub const fn new_owner(&self) -> &SessionOwner {
        &self.new_owner
    }

    /// 新条目的句柄（`dbSessionId` + 新 `runtimeEpoch`）。
    pub fn new_handle(&self) -> SessionHandle {
        self.new_owner.to_handle()
    }

    pub const fn state(&self) -> RecordState {
        self.state
    }

    pub fn set_state(&mut self, state: RecordState) {
        self.state = state;
        if state != RecordState::Undecided {
            self.undecided_truth = None;
        }
    }

    /// 举屏障，并把服务端已落定的结果一并记下。
    pub fn mark_undecided(&mut self, truth: RecordState) {
        self.state = RecordState::Undecided;
        self.undecided_truth = Some(truth);
    }

    pub const fn undecided_truth(&self) -> Option<RecordState> {
        self.undecided_truth
    }

    /// 当前状态下能对外回答什么。屏障未解时一律 `Pending`。
    pub fn status(&self) -> CommitStatus {
        match self.state {
            RecordState::Prepared | RecordState::Undecided => CommitStatus::Pending,
            RecordState::Committed => CommitStatus::Committed {
                handle: self.new_handle(),
            },
            RecordState::RolledBack => CommitStatus::RolledBack {
                restored: self.old.clone(),
            },
        }
    }
}

/// 故障注入：告诉目录「服务端其实已经落定成这个结果，但应答丢了」。
///
/// 生产路径恒为 `None`。它存在的唯一理由是让测试能确定性地复现
/// 「提交结果未知」这条分支——真实世界里它由应答丢失/超时触发，
/// 不该用真实超时去测。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnknownCommitOutcome {
    /// 服务端已提交，但应答丢了。
    Committed,
    /// 服务端已回滚，但应答丢了。
    RolledBack,
}

impl UnknownCommitOutcome {
    pub const fn as_str(self) -> &'static str {
        match self {
            UnknownCommitOutcome::Committed => "committed",
            UnknownCommitOutcome::RolledBack => "rolledBack",
        }
    }

    pub const fn operation(self) -> ReplacementOperation {
        match self {
            UnknownCommitOutcome::Committed => ReplacementOperation::Committed,
            UnknownCommitOutcome::RolledBack => ReplacementOperation::RolledBack,
        }
    }

    /// 服务端已落定的记录状态。
    pub const fn state(self) -> RecordState {
        match self {
            UnknownCommitOutcome::Committed => RecordState::Committed,
            UnknownCommitOutcome::RolledBack => RecordState::RolledBack,
        }
    }
}

/// 操作 → 记录状态的映射。三个动作互不相同，绝不互相顶替。
pub const fn record_state_for(operation: ReplacementOperation) -> RecordState {
    match operation {
        ReplacementOperation::Prepared => RecordState::Prepared,
        ReplacementOperation::Committed => RecordState::Committed,
        ReplacementOperation::RolledBack => RecordState::RolledBack,
    }
}
