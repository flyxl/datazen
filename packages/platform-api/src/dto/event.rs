//! 事件契约：`EventEnvelope<T>` 与事件序号。
//!
//! §4.4：**事件归档只写显式白名单投影，不能直接序列化 `EventEnvelope`**。
//! `EventEnvelope.sessionHandle` 永不落盘。`EventSequence` 就是 `Counter`，
//! JSON 中是十进制字符串，前端可无损地做 `afterSequence` 续订。

use serde::{Deserialize, Serialize};

use crate::dto::profile::ConnectionEvent;
use crate::dto::session::SessionHandle;
use crate::id::{Counter, ExecutionId, JobId, RuntimeEpoch};

/// 事件序号。订阅时用作 `after_sequence`，保证 `after` 之后不丢不重。
pub type EventSequence = Counter;

/// 事件信封。
///
/// `stream_id` + `sequence` 是续订的唯一定位；`session_handle` 是**运行时值**，
/// 归档时必须走显式白名单投影把它丢掉。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventEnvelope<T> {
    pub stream_id: crate::id::StreamId,
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
        stream_id: crate::id::StreamId,
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

/// 连接模块的事件流载荷别名。
pub type ConnectionEventEnvelope = EventEnvelope<ConnectionEvent>;

/// 归档白名单投影：显式列出允许落盘的信封字段。
/// `session_handle` 故意缺席——这个类型的存在本身就是该规则的编译期证据。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchivedEnvelope {
    pub stream_id: crate::id::StreamId,
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
    use crate::dto::profile::ConnectionEvent;
    use crate::id::{DbSessionId, StreamId};
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
            envelope.session_handle = Some(SessionHandle {
                db_session_id: DbSessionId::new("sess-1"),
                runtime_epoch: RuntimeEpoch::new("epoch-1"),
            });
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
}
