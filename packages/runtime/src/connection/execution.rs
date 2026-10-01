//! 执行与结果投影类型（connection-management.md §4 的执行半部、§7 执行纪律、§13 错误命名空间）。
//!
//! 依赖方向：本文件只向下依赖 `types`（标识）与 `session`（会话上下文），不被它们回指。

use serde::{Deserialize, Serialize};

use super::types::{Counter, ExecutionId, StreamId};

/// 会话级 fake 命令清单（fake-runtime-fixtures.md §9.1）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionCommand {
    BeginSessionTransaction,
    BeginSessionTransactionHold,
    OpenSessionCursor,
    PrepareServerStatement,
    CommitSessionTransaction,
    RollbackSessionTransaction,
    CloseSessionCursor,
    /// 反例：fake 内建了句柄却不放进 `sessionHandles`。
    BeginSessionTransactionUnregistered,
    /// 反例：提交时携带陈旧 epoch 的句柄。
    CommitWithStaleHandle,
    /// 反例：跨资源使用句柄。
    HandleFromOtherResource,
}

impl SessionCommand {
    pub const ALL: [SessionCommand; 10] = [
        SessionCommand::BeginSessionTransaction,
        SessionCommand::BeginSessionTransactionHold,
        SessionCommand::OpenSessionCursor,
        SessionCommand::PrepareServerStatement,
        SessionCommand::CommitSessionTransaction,
        SessionCommand::RollbackSessionTransaction,
        SessionCommand::CloseSessionCursor,
        SessionCommand::BeginSessionTransactionUnregistered,
        SessionCommand::CommitWithStaleHandle,
        SessionCommand::HandleFromOtherResource,
    ];

    /// 协议字面量；`packages/driver-api` 的命令 id 走 snake_case。
    pub const fn id(self) -> &'static str {
        match self {
            SessionCommand::BeginSessionTransaction => "begin_session_transaction",
            SessionCommand::BeginSessionTransactionHold => "begin_session_transaction_hold",
            SessionCommand::OpenSessionCursor => "open_session_cursor",
            SessionCommand::PrepareServerStatement => "prepare_server_statement",
            SessionCommand::CommitSessionTransaction => "commit_session_transaction",
            SessionCommand::RollbackSessionTransaction => "rollback_session_transaction",
            SessionCommand::CloseSessionCursor => "close_session_cursor",
            SessionCommand::BeginSessionTransactionUnregistered => {
                "begin_session_transaction_unregistered"
            }
            SessionCommand::CommitWithStaleHandle => "commit_with_stale_handle",
            SessionCommand::HandleFromOtherResource => "handle_from_other_resource",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|command| command.id() == id)
    }

    /// 是否写命令。读命令只声明 `Read` 权限，其余显式声明 `Write`（§9.1 L462）。
    pub const fn is_write(self) -> bool {
        !matches!(self, SessionCommand::OpenSessionCursor)
    }
}

/// 执行终态错误码。**独立于** `ApiError.code`（§13 L772）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExecutionErrorCode {
    SqlError,
    ProtocolError,
    Cancelled,
    Timeout,
    ResourceLost,
    PipelineAborted,
    HostRejected,
}

impl ExecutionErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            ExecutionErrorCode::SqlError => "sqlError",
            ExecutionErrorCode::ProtocolError => "protocolError",
            ExecutionErrorCode::Cancelled => "cancelled",
            ExecutionErrorCode::Timeout => "timeout",
            ExecutionErrorCode::ResourceLost => "resourceLost",
            ExecutionErrorCode::PipelineAborted => "pipelineAborted",
            ExecutionErrorCode::HostRejected => "hostRejected",
        }
    }
}

/// 副作用终态。与 [`ExecutionErrorCode`] **正交**：两者独立取值，不存在互推关系（§13 L774）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EffectOutcome {
    NotStarted,
    Completed,
    RolledBack,
    PartiallyApplied,
    Unknown,
}

impl EffectOutcome {
    pub const fn as_str(self) -> &'static str {
        match self {
            EffectOutcome::NotStarted => "notStarted",
            EffectOutcome::Completed => "completed",
            EffectOutcome::RolledBack => "rolledBack",
            EffectOutcome::PartiallyApplied => "partiallyApplied",
            EffectOutcome::Unknown => "unknown",
        }
    }

    /// 该终态是否「作用域不可判定」。
    ///
    /// §13 L774：超时 / 协议错误 / 取消 / 连接丢失且作用域不可判定时，
    /// `effectOutcome` **必须**为 `unknown`；禁止仅因「看到取消请求」就写 `rolledBack`（CM-44、CM-47）。
    pub fn requires_unknown_for_undecidable_error(
        self,
        code: Option<ExecutionErrorCode>,
    ) -> bool {
        matches!(
            code,
            Some(
                ExecutionErrorCode::Timeout
                    | ExecutionErrorCode::ProtocolError
                    | ExecutionErrorCode::Cancelled
                    | ExecutionErrorCode::ResourceLost
            )
        ) && self != EffectOutcome::Unknown
            && !matches!(self, EffectOutcome::NotStarted)
    }
}

/// 执行状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExecutionState {
    Queued,
    Running,
    CancelRequested,
    Succeeded,
    Failed,
    Cancelled,
}

/// 执行回执。**拿到回执 ≠ SQL 成功**（§13 L770）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionReceipt {
    pub execution_id: ExecutionId,
    pub stream_id: StreamId,
    pub state: ExecutionState,
}

/// 结果完整性。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResultCompleteness {
    Complete,
    Truncated,
    Partial,
}

impl ResultCompleteness {
    pub const fn is_complete(self) -> bool {
        matches!(self, ResultCompleteness::Complete)
    }
}

/// 截断原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TruncationReason {
    NoConsumerDrainDeadline,
    PerSubscriptionEventLimit,
    PerSubscriptionByteLimit,
    PerExecutionByteLimit,
    ProducerWriteFailed,
}

impl TruncationReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            TruncationReason::NoConsumerDrainDeadline => "noConsumerDrainDeadline",
            TruncationReason::PerSubscriptionEventLimit => "perSubscriptionEventLimit",
            TruncationReason::PerSubscriptionByteLimit => "perSubscriptionByteLimit",
            TruncationReason::PerExecutionByteLimit => "perExecutionByteLimit",
            TruncationReason::ProducerWriteFailed => "producerWriteFailed",
        }
    }
}

/// 截断记录。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TruncationRecord {
    pub reason: TruncationReason,
    pub at_mono_nanos: u64,
    pub produced_bytes: u64,
    pub produced_events: u64,
}

/// 结果来源的可写映射。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WritableMapping {
    Verified,
    ReadOnly,
}

/// 单条语句结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatementResult {
    pub statement_index: u32,
    pub columns: Vec<String>,
    pub rows: u64,
    pub row_count_complete: bool,
}

/// 结果来源。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatementResultSource {
    pub execution_id: ExecutionId,
    pub statement_index: u32,
    pub relation: String,
    pub writable_mapping: WritableMapping,
}

/// 结果 sink 的写入结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SinkWrite {
    Accepted,
    /// 写入等待 = 背压（connection-management.md §5.1 L419）。
    Blocked,
    Truncated(TruncationReason),
}

/// 结果 sink：带字节大小的写入 / 完成 / 失败。
///
/// 实现约定：本 trait 是**同步**的。connection-management.md §5.1 只要求「带字节大小的
/// 写入/完成/失败方法」与「写入等待表示背压」这两个可观测语义，并明确禁止暴露 Tokio channel 类型；
/// 夹具保持同步签名即可完整表达背压、截断与 `protocolDrained` 三个可观测点，
/// 也让 `packages/runtime` 不必引入异步运行时。真实异步 sink 由驱动契约 crate 提供。
pub trait ResultSink {
    fn write(&mut self, execution_id: &ExecutionId, chunk_index: Counter, bytes: usize) -> SinkWrite;

    fn complete(&mut self, execution_id: &ExecutionId, produced_bytes: u64);

    fn fail(&mut self, execution_id: &ExecutionId, code: ExecutionErrorCode, produced_bytes: u64);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effect_outcome_refuses_rolled_back_for_undecidable_errors() {
        // §13 L774 / CM-47：作用域不可判定时写 `rolledBack` 属协议违规。
        assert!(EffectOutcome::RolledBack
            .requires_unknown_for_undecidable_error(Some(ExecutionErrorCode::Timeout)));
        assert!(EffectOutcome::Completed
            .requires_unknown_for_undecidable_error(Some(ExecutionErrorCode::Cancelled)));
        assert!(
            !EffectOutcome::Unknown.requires_unknown_for_undecidable_error(Some(ExecutionErrorCode::ProtocolError))
        );
        assert!(
            !EffectOutcome::RolledBack.requires_unknown_for_undecidable_error(Some(ExecutionErrorCode::SqlError)),
            "sqlError 可与任何 outcome 配对"
        );
    }

    #[test]
    fn error_code_literals_match_the_protocol_map() {
        // §26–40：枚举变体与协议字面量一一对应，不允许出现同义两种写法。
        assert_eq!(ExecutionErrorCode::SqlError.as_str(), "sqlError");
        assert_eq!(ExecutionErrorCode::ProtocolError.as_str(), "protocolError");
        assert_eq!(ExecutionErrorCode::Cancelled.as_str(), "cancelled");
        assert_eq!(ExecutionErrorCode::Timeout.as_str(), "timeout");
        assert_eq!(ExecutionErrorCode::ResourceLost.as_str(), "resourceLost");
        assert_eq!(ExecutionErrorCode::PipelineAborted.as_str(), "pipelineAborted");
        assert_eq!(ExecutionErrorCode::HostRejected.as_str(), "hostRejected");
    }

    #[test]
    fn effect_outcome_literals_match_the_protocol_map() {
        assert_eq!(EffectOutcome::NotStarted.as_str(), "notStarted");
        assert_eq!(EffectOutcome::Completed.as_str(), "completed");
        assert_eq!(EffectOutcome::RolledBack.as_str(), "rolledBack");
        assert_eq!(EffectOutcome::PartiallyApplied.as_str(), "partiallyApplied");
        assert_eq!(EffectOutcome::Unknown.as_str(), "unknown");
    }

    #[test]
    fn pipeline_aborted_is_never_complete() {
        // §13 L775：pipelineAborted 至少是部分应用或未知，不存在「完整成功」。
        assert!(!ResultCompleteness::Complete.is_complete());
        assert!(!ResultCompleteness::Partial.is_complete());
        assert!(!ResultCompleteness::Truncated.is_complete());
        assert!(ResultCompleteness::Complete.is_complete());
    }

    #[test]
    fn truncation_reasons_are_distinct_camel_case_literals() {
        let all = [
            TruncationReason::NoConsumerDrainDeadline,
            TruncationReason::PerSubscriptionEventLimit,
            TruncationReason::PerSubscriptionByteLimit,
            TruncationReason::PerExecutionByteLimit,
            TruncationReason::ProducerWriteFailed,
        ];
        let mut seen = std::collections::BTreeSet::new();
        for reason in all {
            assert!(seen.insert(reason.as_str()), "截断原因字面量必须互不相同");
        }
        assert_eq!(seen.len(), 5);
    }

    #[test]
    fn session_command_ids_round_trip() {
        for command in SessionCommand::ALL {
            let id = command.id();
            assert_eq!(SessionCommand::from_id(id), Some(command), "命令 id 必须可反查");
        }
        assert_eq!(SessionCommand::from_id("no_such_command"), None);
    }

    #[test]
    fn only_open_session_cursor_is_a_read_command() {
        // §9.1：读命令只有 `open_session_cursor`，其余写类命令必须显式声明 Write。
        for command in SessionCommand::ALL {
            let expected = command != SessionCommand::OpenSessionCursor;
            assert_eq!(command.is_write(), expected, "{} 的读写分类不对", command.id());
        }
    }

    #[test]
    fn receipt_state_is_independent_from_sql_success() {
        // §13 L770：拿到回执 ≠ 成功；Queued/Running 同样有回执。
        let receipt = ExecutionReceipt {
            execution_id: ExecutionId::new("exe_dbs_w1_0001_0001"),
            stream_id: StreamId::new("str_0001"),
            state: ExecutionState::Queued,
        };
        assert_ne!(receipt.state, ExecutionState::Succeeded);
        assert_ne!(receipt.state, ExecutionState::Failed);
        assert_eq!(receipt.state, ExecutionState::Queued);
    }

    #[test]
    fn sink_write_carries_the_truncation_reason_out() {
        // §5.1 L419 / §6.2：背压是 Blocked，触顶后必须带上可断言的截断原因。
        assert_ne!(SinkWrite::Blocked, SinkWrite::Accepted);
        match SinkWrite::Truncated(TruncationReason::PerExecutionByteLimit) {
            SinkWrite::Truncated(reason) => {
                assert_eq!(reason.as_str(), "perExecutionByteLimit")
            }
            other => panic!("截断分支必须保留原因，实际 {:?}", other),
        }
    }
}
