//! §3.2 / D-01：取消三态透传、绑定校验、取消失败不得降级为「已取消」。

use std::sync::Arc;

use super::{err, record, runtime_err};
use crate::gateway_fixtures as fx;
use datazen_runtime::connection::capability::PreciseCancel;
use datazen_runtime::connection::{ExecutionId, ExecutionState, RuntimeError};
use datazen_runtime::gateway::cancel::binding;
use datazen_runtime::gateway::{
    CancelDisposition, GatewayAction, GatewayError, InMemoryIdempotencyStore,
};
// ───────────────── E §3.2 取消 ─────────────────

#[tokio::test(start_paused = true)]
async fn a_supported_cancel_is_requested_through_the_port() {
    let h = fx::ready_harness();
    let id = fx::accept(&h, fx::request(fx::REVISION)).await;

    let outcome = h
        .gateway
        .cancel(
            &fx::principal(),
            fx::cancel_request(&id, PreciseCancel::Supported),
        )
        .await
        .expect("取消应当被受理");
    assert_eq!(outcome.disposition, CancelDisposition::Requested);
    assert_eq!(outcome.disposition.as_str(), "requested");
    assert_eq!(outcome.state, ExecutionState::CancelRequested);
    // 受理了取消 ≠ 已经取消：终态只能由端口如实回报，网关不许自己下结论。
    assert!(!outcome.is_terminal());
    assert_eq!(h.port.cancel_calls(), 1);
}

#[tokio::test(start_paused = true)]
async fn an_unsupported_cancel_never_reaches_the_driver() {
    let h = fx::ready_harness();
    let id = fx::accept(&h, fx::request(fx::REVISION)).await;

    let outcome = h
        .gateway
        .cancel(
            &fx::principal(),
            fx::cancel_request(&id, PreciseCancel::Unsupported),
        )
        .await
        .expect("取消应当被受理");
    assert_eq!(outcome.disposition, CancelDisposition::Unsupported);
    assert_eq!(outcome.disposition.as_str(), "unsupported");
    assert_eq!(h.port.cancel_calls(), 0);
}

#[tokio::test(start_paused = true)]
async fn a_finished_execution_reports_already_finished() {
    let h = fx::ready_harness();
    h.port.set_cancel(Ok(ExecutionState::Succeeded));
    let id = fx::accept(&h, fx::request(fx::REVISION)).await;

    let outcome = h
        .gateway
        .cancel(
            &fx::principal(),
            fx::cancel_request(&id, PreciseCancel::Supported),
        )
        .await
        .expect("取消应当被受理");
    assert_eq!(outcome.disposition, CancelDisposition::AlreadyFinished);
    assert_eq!(outcome.disposition.as_str(), "alreadyFinished");
    assert_eq!(h.port.cancel_calls(), 1);
}

#[tokio::test(start_paused = true)]
async fn a_binding_mismatch_is_cancel_failed_not_cancelled() {
    let h = fx::ready_harness();
    let id = fx::accept(&h, fx::request(fx::REVISION)).await;
    let mut request = fx::cancel_request(&id, PreciseCancel::Supported);
    // 同一个 executionId，但声称属于另一个 epoch 的句柄。
    request.binding.handle = fx::handle(fx::SESSION, 9);

    let error = err(h.gateway.cancel(&fx::principal(), request).await);
    match runtime_err(&error) {
        RuntimeError::CancelFailed(reason) => {
            assert_eq!(*reason, binding::EPOCH_MISMATCH)
        }
        other => panic!("期望 CancelFailed，实际 {other:?}"),
    }
    assert_eq!(h.port.cancel_calls(), 0);
    assert_eq!(record(&h, &id).await.last_state(), ExecutionState::Queued);
}

#[tokio::test(start_paused = true)]
async fn a_cancel_failure_is_not_downgraded_to_cancelled() {
    let h = fx::ready_harness();
    h.port
        .set_cancel(Err(RuntimeError::CancelFailed("driverPathUnreachable")));
    let id = fx::accept(&h, fx::request(fx::REVISION)).await;

    let error = err(h
        .gateway
        .cancel(
            &fx::principal(),
            fx::cancel_request(&id, PreciseCancel::Supported),
        )
        .await);
    assert_eq!(
        runtime_err(&error),
        &RuntimeError::CancelFailed("driverPathUnreachable")
    );
    assert_eq!(
        record(&h, &id).await.last_state(),
        ExecutionState::Queued,
        "取消失败不得把执行记成取消中"
    );
}

#[tokio::test(start_paused = true)]
async fn cancelling_an_unknown_execution_is_cancel_failed() {
    let h = fx::ready_harness();
    let request = fx::cancel_request(
        &ExecutionId::new("exe_contract_none"),
        PreciseCancel::Supported,
    );

    let error = err(h.gateway.cancel(&fx::principal(), request).await);
    match runtime_err(&error) {
        RuntimeError::CancelFailed(reason) => assert_eq!(*reason, binding::UNKNOWN_EXECUTION),
        other => panic!("期望 CancelFailed，实际 {other:?}"),
    }
    assert_eq!(h.port.cancel_calls(), 0);
}

#[tokio::test(start_paused = true)]
async fn cancel_requires_its_own_authorization() {
    let h = fx::harness_with(
        Arc::new(fx::RecordingPort::new(fx::ready_view())),
        Arc::new(fx::CancelDenyAuthorizer),
        InMemoryIdempotencyStore::shared(),
    );
    let id = fx::accept(&h, fx::request(fx::REVISION)).await;

    let error = err(h
        .gateway
        .cancel(
            &fx::principal(),
            fx::cancel_request(&id, PreciseCancel::Supported),
        )
        .await);
    assert_eq!(
        error,
        GatewayError::PermissionDenied {
            action: GatewayAction::Cancel,
            reason: "cancelNotPermitted",
        }
    );
    assert_eq!(h.port.cancel_calls(), 0);
}
