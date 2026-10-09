//! 乐观并发闸：`expectedContextRevision` 在真实执行前必须重比，
//! 不匹配时回传服务端实际值。

use super::{err, record, runtime_err};
use crate::gateway_fixtures as fx;
use datazen_runtime::connection::{ExecutionState, RuntimeError, SessionState};
use datazen_runtime::gateway::GatewayError;
// ───────────────── C 乐观并发闸 ─────────────────

#[tokio::test(start_paused = true)]
async fn the_context_revision_gate_is_rechecked_before_the_driver_runs() {
    let h = fx::ready_harness();
    let id = fx::accept(&h, fx::request(fx::REVISION)).await;
    h.port.set_view(Ok(fx::view(
        fx::SESSION,
        1,
        fx::REVISION + 3,
        SessionState::Ready,
    )));

    let error = err(h.gateway.dispatch(&fx::principal(), &id).await);
    assert_eq!(
        runtime_err(&error),
        &RuntimeError::ContextRevisionMismatch {
            expected: fx::REVISION,
            actual: fx::REVISION + 3,
        }
    );
    assert_eq!(h.port.execute_calls(), 0);
}

#[tokio::test(start_paused = true)]
async fn the_entry_gate_reports_the_servers_actual_revision() {
    let (h, store) = fx::counted_harness();
    let error = err(h
        .gateway
        .accept(&fx::principal(), fx::request(fx::REVISION - 1))
        .await);
    assert_eq!(
        runtime_err(&error),
        &RuntimeError::ContextRevisionMismatch {
            expected: fx::REVISION - 1,
            actual: fx::REVISION,
        }
    );
    assert_eq!(store.write_count(), 0);
    assert_eq!(h.gateway.execution_count().await, 0);
}

#[tokio::test(start_paused = true)]
async fn a_dispatch_hands_the_ports_receipt_back_unchanged() {
    let h = fx::ready_harness();
    let id = fx::accept(&h, fx::request(fx::REVISION)).await;

    let receipt = h
        .gateway
        .dispatch(&fx::principal(), &id)
        .await
        .expect("应当派发");
    assert_eq!(h.port.execute_calls(), 1);
    assert_eq!(receipt.state, ExecutionState::Queued);
    assert_eq!(h.port.executed_requests().len(), 1);
    assert!(record(&h, &id).await.is_dispatched());

    let error = err(h.gateway.dispatch(&fx::principal(), &id).await);
    assert_eq!(
        error,
        GatewayError::InvalidRequest {
            reason: "alreadyDispatched"
        }
    );
}
