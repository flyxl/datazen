//! 执行事件词汇：`EventEnvelope<T>`、事件序号与连接事件载荷。
//!
//! 原属 platform-api `dto/event.rs` 与 `dto/profile.rs` 的事件部分，D3
//! （2026-10-26）起唯一定义在 `datazen-modules::execution::application`；
//! platform-api 只再导出。载荷虽沿用 `ConnectionEvent` 旧名（线格式不变），
//! 但其变体（会话变化/执行变化/结果块）全部是执行侧事实，归 execution。
//!
//! §4.4：**事件归档只写显式白名单投影，不能直接序列化 `EventEnvelope`**。
//! `EventEnvelope.sessionHandle` 永不落盘。`EventSequence` 就是 `Counter`，
//! JSON 中是十进制字符串，前端可无损地做 `afterSequence` 续订。

use serde::{Deserialize, Serialize};

use crate::execution::domain::SessionHandle;
use crate::execution::domain::StatementResultSource;
use crate::shared::id::{ArtifactId, Counter, ExecutionId, JobId, RuntimeEpoch, StreamId};

use super::views::{ExecutionView, SessionView};

/// 事件序号。订阅时用作 `after_sequence`，保证 `after` 之后不丢不重。
pub type EventSequence = Counter;

/// 事件信封。
///
/// `stream_id` + `sequence` 是续订的唯一定位；`session_handle` 是**运行时值**，
/// 归档时必须走显式白名单投影把它丢掉。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventEnvelope<T> {
    pub stream_id: StreamId,
    pub sequence: EventSequence,
    pub runtime_epoch: RuntimeEpoch,
    pub execution_id: Option<ExecutionId>,
    pub job_id: Option<JobId>,
    /// **运行时值**，永不落盘（§4.4）。
    pub session_handle: Option<SessionHandle>,
    pub context_revision: Option<Counter>,
    pub payload: T,
}

impl<T> EventEnvelope<T> {
    pub fn new(
        stream_id: StreamId,
        sequence: EventSequence,
        runtime_epoch: RuntimeEpoch,
        payload: T,
    ) -> Self {
        Self {
            stream_id,
            sequence,
            runtime_epoch,
            execution_id: None,
            job_id: None,
            session_handle: None,
            context_revision: None,
            payload,
        }
    }

    /// 是否属于给定的运行时纪元。旧纪元的事件必须被丢弃，不得当作当前状态。
    pub fn belongs_to(&self, epoch: &RuntimeEpoch) -> bool {
        &self.runtime_epoch == epoch
    }
}

/// 结果块到达事件携带的来源标签。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultChunkEvent {
    pub artifact_id: ArtifactId,
    pub chunk_index: Counter,
    pub source: StatementResultSource,
}

/// 事件载荷（沿用 `ConnectionEvent` 旧名，线格式不变）。
// `ExecutionView` 比 `SessionView` 大（864 vs 544 字节），装箱可以压掉差值，但
// 这个信封不是热路径栈值：每条事件都会跨进程序列化，装箱只会把这一次分配加到
// 每条事件上，而枚举大小的收益在信封生命周期里兑现不了。
// 这里的形状是刻意设计，改成参数结构体或加间接层都会动到已依赖的契约。
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ConnectionEvent {
    /// 会话变化（状态、上下文、attachment）。
    SessionChanged {
        session: SessionView,
    },
    /// 执行状态变化。
    ExecutionChanged {
        execution: ExecutionView,
    },
    /// 结果块到达。
    ResultChunk(ResultChunkEvent),
    /// 流已失效，客户端必须重新订阅；**不得**据此推断执行未发生。
    StreamResetRequired { reason: String },
}

impl ConnectionEvent {
    /// 该事件是否需要客户端重新订阅。
    pub fn requires_resubscribe(&self) -> bool {
        matches!(self, Self::StreamResetRequired { .. })
    }
}

/// 连接模块的事件流载荷别名。
pub type ConnectionEventEnvelope = EventEnvelope<ConnectionEvent>;

/// 归档白名单投影：显式列出允许落盘的信封字段。
/// `session_handle` 故意缺席——这个类型的存在本身就是该规则的编译期证据。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchivedEnvelope {
    pub stream_id: StreamId,
    pub sequence: EventSequence,
    pub runtime_epoch: RuntimeEpoch,
    pub execution_id: Option<ExecutionId>,
    pub job_id: Option<JobId>,
    pub context_revision: Option<Counter>,
    pub payload: ConnectionEvent,
}

impl ArchivedEnvelope {
    /// 显式白名单投影。`session_handle` 不参与构造，因此无法被写进归档。
    pub fn project(envelope: &ConnectionEventEnvelope) -> Self {
        Self {
            stream_id: envelope.stream_id.clone(),
            sequence: envelope.sequence,
            runtime_epoch: envelope.runtime_epoch.clone(),
            execution_id: envelope.execution_id.clone(),
            job_id: envelope.job_id.clone(),
            context_revision: envelope.context_revision,
            payload: envelope.payload.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::id::DbSessionId;
    use serde_json::json;

    fn envelope(with_handle: bool) -> ConnectionEventEnvelope {
        let payload = ConnectionEvent::StreamResetRequired {
            reason: "runtime restarted".into(),
        };
        let mut envelope = EventEnvelope::new(
            StreamId::new("stream-1"),
            EventSequence::new(7),
            RuntimeEpoch::new("epoch-1"),
            payload,
        );
        envelope.execution_id = Some(ExecutionId::new("exec-1"));
        envelope.context_revision = Some(Counter::new(2));
        if with_handle {
            envelope.session_handle = Some(SessionHandle::new(
                DbSessionId::new("sess-1"),
                RuntimeEpoch::new("epoch-1"),
            ));
        }
        envelope
    }

    #[test]
    fn event_envelope_round_trips_with_decimal_string_sequence() {
        let original = envelope(true);
        let value = serde_json::to_value(&original).expect("serialize");
        assert_eq!(value["sequence"], json!("7"));
        assert_eq!(value["runtimeEpoch"], json!("epoch-1"));
        assert_eq!(value["contextRevision"], json!("2"));
        assert_eq!(value["sessionHandle"]["dbSessionId"], json!("sess-1"));
        assert_eq!(value["payload"]["kind"], json!("streamResetRequired"));
        assert_eq!(
            serde_json::from_value::<ConnectionEventEnvelope>(value).expect("deserialize"),
            original
        );
    }

    #[test]
    fn event_envelope_without_optional_fields() {
        let original = envelope(false);
        let value = serde_json::to_value(&original).expect("serialize");
        assert_eq!(value["sessionHandle"], json!(null));
        assert_eq!(value["jobId"], json!(null));
        assert_eq!(
            serde_json::from_value::<ConnectionEventEnvelope>(value).expect("deserialize"),
            original
        );
    }

    #[test]
    fn archived_projection_drops_the_session_handle() {
        let live = envelope(true);
        let archived = ArchivedEnvelope::project(&live);
        let value = serde_json::to_value(&archived).expect("serialize");
        assert!(
            value.get("sessionHandle").is_none(),
            "归档投影不得包含 sessionHandle：{value}"
        );
        assert_eq!(value["sequence"], json!("7"));
        assert_eq!(value["payload"]["kind"], json!("streamResetRequired"));
        assert_eq!(
            serde_json::from_value::<ArchivedEnvelope>(value).expect("deserialize"),
            archived
        );
    }

    #[test]
    fn stale_epoch_events_are_recognizable() {
        let live = envelope(true);
        assert!(live.belongs_to(&RuntimeEpoch::new("epoch-1")));
        assert!(!live.belongs_to(&RuntimeEpoch::new("epoch-2")));
    }

    #[test]
    fn connection_event_uses_the_spec_tag_names() {
        use crate::execution::domain::{EffectOutcome, ExecutionState, ResultCompleteness};

        let events = [
            ConnectionEvent::StreamResetRequired {
                reason: "runtime restarted".into(),
            },
            ConnectionEvent::ExecutionChanged {
                execution: ExecutionView {
                    execution_id: ExecutionId::new("exec-1"),
                    state: ExecutionState::Running,
                    effect_outcome: EffectOutcome::NotStarted,
                    provenance: None,
                    artifact_ids: vec![],
                    result_completeness: ResultCompleteness::Pending,
                    truncation_reason: None,
                    error_code: None,
                    runtime_binding: None,
                },
            },
        ];
        let kinds: Vec<String> = events
            .iter()
            .map(|e| {
                serde_json::to_value(e).expect("serialize")["kind"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned()
            })
            .collect();
        assert_eq!(kinds, ["streamResetRequired", "executionChanged"]);

        for event in &events {
            let value = serde_json::to_value(event).expect("serialize");
            assert_eq!(
                serde_json::from_value::<ConnectionEvent>(value).expect("deserialize"),
                *event
            );
        }

        assert!(events[0].requires_resubscribe());
        assert!(!events[1].requires_resubscribe());
    }
}
