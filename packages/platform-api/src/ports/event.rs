//! `EventSink`：事件发布/订阅端口。
//!
//! 词汇表（§4.6）：`EventSubscription`（与 dto 层的 `EventSequence`）。
//!
//! 契约要点：
//!
//! * 服务端一半在这里；前端一半（`AsyncIterable<EventEnvelope<ConnectionEvent>>`）
//!   是 §7 的抽象，本端口的订阅形态**可以**是 iterator，也可以是别的实现。
//! * **归档写入显式白名单投影**：落盘的是 `ArchivedEnvelope`（dto 层），
//!   它**不含 `sessionHandle`**（[连接 §4.4](connection-management.md) 的持久化红线）。
//! * `subscribe` 的 `after_sequence` 是**重连补偿位点**，不是 lease，更不落盘。
//! * `runtime_epoch` 不匹配的事件由订阅方丢弃（`EventEnvelope::belongs_to`），
//!   端口不负责过滤：过滤发生在前端可见的一侧，避免消息在传输层被判死。

use async_trait::async_trait;

use crate::context::RequestContext;
use crate::dto::event::{ConnectionEventEnvelope, EventSequence};
use crate::error::PortError;
use crate::id::StreamId;

/// 订阅。断线后由宿主重新 `subscribe(after_sequence)` 续读。
pub type EventSubscription = Box<dyn Iterator<Item = ConnectionEventEnvelope> + Send + 'static>;

#[async_trait]
pub trait EventSink: Send + Sync + 'static {
    async fn publish(
        &self,
        ctx: &RequestContext,
        envelope: ConnectionEventEnvelope,
    ) -> Result<EventSequence, PortError>;

    /// 服务端一半；前端一半见 §7 的 AsyncIterable 抽象。
    async fn subscribe(
        &self,
        ctx: &RequestContext,
        stream_id: StreamId,
        after_sequence: Option<EventSequence>,
    ) -> Result<EventSubscription, PortError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::event::EventEnvelope;
    use crate::dto::profile::ConnectionEvent;
    use crate::dto::session::SessionHandle;
    use crate::id::{
        ClientInstanceId, Counter, DbSessionId, OrganizationId, PrincipalId, RequestId,
        RuntimeEpoch,
    };

    fn context() -> RequestContext {
        RequestContext::new(
            OrganizationId::new("org-1"),
            PrincipalId::new("user-1"),
            None,
            ClientInstanceId::new("client-1"),
            RequestId::new("req-1"),
            None,
        )
    }

    fn envelope(sequence: u64, epoch: &str) -> ConnectionEventEnvelope {
        let mut envelope = EventEnvelope::new(
            StreamId::new("stream-1"),
            Counter::new(sequence),
            RuntimeEpoch::new(epoch),
            ConnectionEvent::StreamResetRequired {
                reason: "compaction".to_string(),
            },
        );
        envelope.session_handle = Some(SessionHandle {
            db_session_id: DbSessionId::new("sess-1"),
            runtime_epoch: RuntimeEpoch::new(epoch),
        });
        envelope
    }

    #[tokio::test]
    async fn a_subscription_can_be_consumed_as_a_plain_iterator() {
        // `EventSubscription` 是 `Box<dyn Iterator<..>>`：宿主实现可自由，调用方只当迭代器用。
        let collected: Vec<ConnectionEventEnvelope> =
            vec![envelope(1, "epoch-1"), envelope(2, "epoch-1")]
                .into_iter()
                .collect();
        let subscription: EventSubscription = Box::new(collected.into_iter());
        let seen: Vec<u64> = subscription.map(|event| event.sequence.get()).collect();
        assert_eq!(seen, vec![1, 2]);
    }

    #[tokio::test]
    async fn publish_returns_a_counter_sequence_not_a_raw_usize() {
        let published = EventSequence::new(42);
        let json = serde_json::to_string(&published).expect("serialize");
        assert_eq!(json, "\"42\"");
        assert_eq!(published.get(), 42);
    }

    #[test]
    fn subscriptions_are_constructed_from_an_explicit_resume_point() {
        // `after_sequence` 只是补偿位点：没有它就从 0 开始，二者都不构成 lease。
        let from_start: Option<EventSequence> = None;
        let resume: Option<EventSequence> = Some(EventSequence::new(17));
        assert_eq!(resume.map(|s| s.get()), Some(17));
        assert!(from_start.is_none());
        assert_eq!(context().organization_id, OrganizationId::new("org-1"));
    }

    #[test]
    fn stale_epoch_events_stay_delivered_so_the_visible_side_can_filter_them() {
        // 端口不静默丢事件：epoch 过期的判断交给 `belongs_to`。
        let stale = envelope(1, "epoch-old");
        let current = envelope(2, "epoch-new");
        assert!(!stale.belongs_to(&RuntimeEpoch::new("epoch-new")));
        assert!(current.belongs_to(&RuntimeEpoch::new("epoch-new")));
    }

    #[test]
    fn archived_projection_never_carries_the_session_handle() {
        let envelope = envelope(1, "epoch-1");
        let archived = crate::dto::event::ArchivedEnvelope::project(&envelope);
        // 归档类型里根本没有这个字段（不是「字段为 None」）：编译期即不可能写出句柄。
        let json = serde_json::to_value(&archived).expect("serialize");
        assert!(json.get("sessionHandle").is_none());
        assert!(envelope.session_handle.is_some(), "实时信封本身仍携带句柄");
    }
}
