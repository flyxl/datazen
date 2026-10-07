//! 来源以请求为准，授权在入口与派发驱动前各判一次。

use std::sync::Arc;

use super::{err, record};
use crate::gateway_fixtures as fx;
use datazen_runtime::connection::Counter;
use datazen_runtime::gateway::{
    EventDisposition, ExecutionEventKind, ExecutionSource, GatewayAction, GatewayError,
    InMemoryIdempotencyStore, SourceKind,
};
// ───────────────── D 授权与来源 ─────────────────

#[tokio::test(start_paused = true)]
async fn authorization_is_rechecked_before_dispatching_the_driver() {
    let authorizer = fx::FlippingAuthorizer::shared();
    let h = fx::harness_with(
        Arc::new(fx::RecordingPort::new(fx::ready_view())),
        authorizer.clone(),
        InMemoryIdempotencyStore::shared(),
    );
    let id = fx::accept(&h, fx::request(fx::REVISION)).await;

    authorizer.revoke_execute();
    let error = err(h.gateway.dispatch(&fx::principal(), &id).await);
    assert_eq!(
        error,
        GatewayError::PermissionDenied {
            action: GatewayAction::Execute,
            reason: "permissionRevoked",
        }
    );
    assert_eq!(h.port.execute_calls(), 0);
}

#[tokio::test(start_paused = true)]
async fn an_unpersistable_source_is_refused_at_the_entry() {
    let h = fx::ready_harness();
    let mut request = fx::request(fx::REVISION);
    // 审计记录里「来自哪」不能是空串：来源不可落盘就等于执行来路不明。
    request.source = ExecutionSource::new(SourceKind::Editor, "", None, None);

    let error = err(h.gateway.accept(&fx::principal(), request).await);
    assert_eq!(
        error,
        GatewayError::InvalidRequest {
            reason: "sourceNotPersistable"
        }
    );
    assert_eq!(h.gateway.execution_count().await, 0);
}

#[tokio::test(start_paused = true)]
async fn a_background_source_is_accepted_and_recorded_as_itself() {
    let h = fx::ready_harness();
    let mut request = fx::request(fx::REVISION);
    request.source = fx::background_source();

    let id = fx::accept(&h, request).await;
    let record = record(&h, &id).await;
    assert_eq!(record.source().kind(), SourceKind::Job);
    assert_eq!(record.source().source_id(), "job-contract");
}

#[tokio::test(start_paused = true)]
async fn a_later_declared_source_event_never_overwrites_the_request_source() {
    let h = fx::ready_harness();
    let id = fx::accept(&h, fx::request(fx::REVISION)).await;

    let mut event = fx::event(
        &id,
        1,
        ExecutionEventKind::ResultChunk {
            chunk_index: Counter::new(0),
            rows: 4,
        },
    );
    event.declared_source = Some(fx::background_source());
    let disposition = h.gateway.apply_event(event).await;
    assert_eq!(disposition, EventDisposition::Applied);

    let record = record(&h, &id).await;
    assert_eq!(record.source().kind(), SourceKind::Editor);
    assert_eq!(record.source().source_id(), "edt-contract");
    assert_eq!(record.declared_source_events_ignored(), 1);
    // 事件里的来源被计数忽略，但仍照常计入行数。
    assert_eq!(record.observed_row_count(), 4);
}
