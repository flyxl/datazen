//! 受理入口：校验顺序、受理回执≠结果、拒绝不得静默降级为「已受理」。

use std::sync::Arc;

use super::{err, record, runtime_err};
use crate::gateway_fixtures as fx;
use datazen_runtime::connection::{
    Counter, ExecutionId, ExecutionState, RuntimeError, SessionState,
};
use datazen_runtime::gateway::idempotency::RequestFingerprint;
use datazen_runtime::gateway::{
    GatewayAction, GatewayError, IdempotencyRecord, IdempotencyScope, InMemoryIdempotencyStore,
};
// ───────────────── A 受理入口 ─────────────────

#[tokio::test(start_paused = true)]
async fn an_acceptance_is_a_receipt_not_a_result() {
    let h = fx::ready_harness();
    let acceptance = h
        .gateway
        .accept(&fx::principal(), fx::request(fx::REVISION))
        .await
        .expect("应当受理");

    assert_eq!(acceptance.receipt.state, ExecutionState::Queued);
    assert!(acceptance.is_queued_not_finished());
    assert!(!acceptance.is_replay());
    // 回执在手，驱动还没被叫醒。
    assert_eq!(h.port.execute_calls(), 0);
    assert_eq!(h.gateway.execution_count().await, 1);
}

#[tokio::test(start_paused = true)]
async fn an_invalid_request_is_refused_before_the_port_is_touched() {
    let h = fx::ready_harness();
    let mut request = fx::request(fx::REVISION);
    request.call.command = "   ".to_string();

    let error = err(h.gateway.accept(&fx::principal(), request).await);
    assert_eq!(
        error,
        GatewayError::InvalidRequest {
            reason: "commandRequired"
        }
    );
    assert_eq!(h.port.session_view_calls(), 0);
    assert_eq!(h.gateway.execution_count().await, 0);
}

#[tokio::test(start_paused = true)]
async fn a_closed_session_is_not_downgraded_to_accepted() {
    let h = fx::ready_harness();
    h.port.set_view(Ok(fx::view(
        fx::SESSION,
        1,
        fx::REVISION,
        SessionState::Closed,
    )));

    let error = err(h
        .gateway
        .accept(&fx::principal(), fx::request(fx::REVISION))
        .await);
    assert_eq!(
        runtime_err(&error),
        &RuntimeError::SessionClosed(fx::SESSION.to_string())
    );
    assert_eq!(h.gateway.execution_count().await, 0);
}

#[tokio::test(start_paused = true)]
async fn a_lost_session_is_not_downgraded_to_accepted() {
    let h = fx::ready_harness();
    h.port.set_view(Ok(fx::view(
        fx::SESSION,
        1,
        fx::REVISION,
        SessionState::Lost,
    )));

    let error = err(h
        .gateway
        .accept(&fx::principal(), fx::request(fx::REVISION))
        .await);
    assert_eq!(
        runtime_err(&error),
        &RuntimeError::SessionLost(fx::SESSION.to_string())
    );
    assert_eq!(h.gateway.execution_count().await, 0);
}

#[tokio::test(start_paused = true)]
async fn an_unknown_session_is_not_downgraded_to_accepted() {
    let h = fx::ready_harness();
    h.port
        .set_view(Err(RuntimeError::UnknownSession(fx::SESSION.to_string())));

    let error = err(h
        .gateway
        .accept(&fx::principal(), fx::request(fx::REVISION))
        .await);
    assert_eq!(
        runtime_err(&error),
        &RuntimeError::UnknownSession(fx::SESSION.to_string())
    );
    assert_eq!(h.gateway.execution_count().await, 0);
}

#[tokio::test(start_paused = true)]
async fn budget_exhaustion_is_not_downgraded_to_accepted() {
    let h = fx::ready_harness();
    h.port
        .set_view(Err(RuntimeError::BudgetExhausted("sessionRowBudget")));
    let error = err(h
        .gateway
        .accept(&fx::principal(), fx::request(fx::REVISION))
        .await);
    assert_eq!(
        runtime_err(&error),
        &RuntimeError::BudgetExhausted("sessionRowBudget")
    );
    assert_eq!(h.gateway.execution_count().await, 0);
}

#[tokio::test(start_paused = true)]
async fn session_quarantine_is_not_downgraded_to_accepted() {
    let h = fx::ready_harness();
    h.port
        .set_view(Err(RuntimeError::SessionQuarantined("statementTimeout")));
    let error = err(h
        .gateway
        .accept(&fx::principal(), fx::request(fx::REVISION))
        .await);
    assert_eq!(
        runtime_err(&error),
        &RuntimeError::SessionQuarantined("statementTimeout")
    );
    assert_eq!(h.gateway.execution_count().await, 0);
}

#[tokio::test(start_paused = true)]
async fn a_session_lost_after_acceptance_never_reaches_the_driver() {
    let h = fx::ready_harness();
    let id = fx::accept(&h, fx::request(fx::REVISION)).await;
    h.port.set_view(Ok(fx::view(
        fx::SESSION,
        1,
        fx::REVISION,
        SessionState::Lost,
    )));

    let error = err(h.gateway.dispatch(&fx::principal(), &id).await);
    assert_eq!(
        runtime_err(&error),
        &RuntimeError::SessionLost(fx::SESSION.to_string())
    );
    assert_eq!(h.port.execute_calls(), 0);
    assert!(!record(&h, &id).await.is_dispatched());
}

#[tokio::test(start_paused = true)]
async fn validation_runs_before_authorization() {
    let authorizer = fx::FlippingAuthorizer::shared();
    authorizer.revoke_execute();
    let h = fx::harness_with(
        Arc::new(fx::RecordingPort::new(fx::ready_view())),
        authorizer,
        InMemoryIdempotencyStore::shared(),
    );

    let mut request = fx::request(fx::REVISION);
    request.call.command = String::new();
    let error = err(h.gateway.accept(&fx::principal(), request).await);
    assert_eq!(
        error,
        GatewayError::InvalidRequest {
            reason: "commandRequired"
        }
    );
}

#[tokio::test(start_paused = true)]
async fn authorization_runs_before_the_idempotency_lookup() {
    // 存储里已经有这个 key 的记录：若查重先于授权，这里会回放成功。
    let store = InMemoryIdempotencyStore::shared();
    store.force_insert(
        IdempotencyScope::new(fx::db_session(), Counter::new(1), fx::IDEMPOTENCY_KEY),
        IdempotencyRecord {
            execution_id: ExecutionId::new("exe_contract_preset"),
            fingerprint: RequestFingerprint::of(
                &fx::call(),
                Counter::new(fx::REVISION),
                &fx::source(),
            ),
            first_accepted_at_nanos: 11,
        },
    );
    let authorizer = fx::FlippingAuthorizer::shared();
    authorizer.revoke_execute();
    let h = fx::harness_with(
        Arc::new(fx::RecordingPort::new(fx::ready_view())),
        authorizer,
        store.clone(),
    );

    let error = err(h
        .gateway
        .accept(&fx::principal(), fx::request(fx::REVISION))
        .await);
    assert_eq!(
        error,
        GatewayError::PermissionDenied {
            action: GatewayAction::Execute,
            reason: "permissionRevoked",
        }
    );
    assert_eq!(store.len(), 1);
}
