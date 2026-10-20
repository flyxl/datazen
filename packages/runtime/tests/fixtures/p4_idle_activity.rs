use super::*;
use datazen_runtime::{
    application::IdlePolicy, connection::testing::clock::FakeClock, gateway::MonotonicClock,
};
use std::time::Duration;
struct Clock(Arc<FakeClock>);
impl MonotonicClock for Clock {
    fn now_nanos(&self) -> u64 {
        self.0.monotonic().as_nanos()
    }
}
fn setup_clock() -> (
    RuntimeConnectionUseCases,
    Arc<Backend>,
    Arc<FakeClock>,
    Arc<Policy>,
) {
    let (_, backend, profiles, policy) = setup();
    let clock = Arc::new(FakeClock::new());
    let service = RuntimeConnectionUseCases::new_with_clock(
        profiles,
        policy.clone(),
        backend.clone(),
        Arc::new(Clock(clock.clone())),
    );
    service.with_idle_policy(IdlePolicy::with_deadline(
        Duration::from_millis(100),
        Duration::from_millis(10),
    ));
    *lock(&backend.sink) = Some(service.result_sink());
    (service, backend, clock, policy)
}
fn advance(clock: &FakeClock, ms: u64) {
    clock.advance(Duration::from_millis(ms));
}
#[tokio::test]
async fn real_execution_refreshes_past_old_deadline_then_expires_after_new_idle_period() {
    let (service, backend, clock, _) = setup_clock();
    let context = ctx("user", "client");
    let opened = open(&service, &context, "idle").await;
    advance(&clock, 80);
    let receipt = execute(&service, &context, &opened.session.handle, "query").await;
    terminal(&service, &context, &receipt.execution_id).await;
    advance(&clock, 30);
    assert!(service.reclaim_idle().await.reclaimed.is_empty());
    assert!(service
        .get_session(&context, opened.session.handle.clone())
        .await
        .is_ok());
    advance(&clock, 71);
    assert_eq!(
        service.reclaim_idle().await.reclaimed,
        vec![opened.session.handle.db_session_id]
    );
    assert_eq!(backend.closed.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn in_flight_execution_and_pending_receipt_cannot_be_idle_evicted() {
    let (service, backend, clock, _) = setup_clock();
    let context = ctx("user", "client");
    let opened = open(&service, &context, "busy").await;
    let receipt = execute(&service, &context, &opened.session.handle, "hold").await;
    backend.started.notified().await;
    advance(&clock, 500);
    assert!(service.reclaim_idle().await.reclaimed.is_empty());
    assert_eq!(backend.closed.load(Ordering::SeqCst), 0);
    backend.release.notify_one();
    terminal(&service, &context, &receipt.execution_id).await;
    advance(&clock, 99);
    assert!(service.reclaim_idle().await.reclaimed.is_empty());
    advance(&clock, 2);
    assert_eq!(service.reclaim_idle().await.reclaimed.len(), 1);
}
#[tokio::test]
async fn reads_attachments_and_receipt_replays_do_not_refresh_business_idle() {
    let (service, _, clock, _) = setup_clock();
    let context = ctx("user", "client");
    let opened = open(&service, &context, "read-only-activity").await;
    let token = service
        .issue_submission_token(
            &context,
            IdempotentOperation::ExecuteInSession,
            Some(&opened.session.handle),
        )
        .await
        .unwrap();
    let request = ExecuteInSessionRequest::new(
        opened.session.handle.clone(),
        CommandCall::new("query", serde_json::json!({})),
        token.idempotency_key,
    );
    let first = service
        .execute_in_session(&context, request.clone())
        .await
        .unwrap();
    terminal(&service, &context, &first.execution_id).await;
    advance(&clock, 80);
    for _ in 0..3 {
        service
            .get_session(&context, opened.session.handle.clone())
            .await
            .unwrap();
        service
            .get_execution(&context, first.execution_id.clone())
            .await
            .unwrap();
        service
            .attach_session(
                &context,
                AttachmentRequest::new(
                    opened.session.handle.clone(),
                    opened.attachment_token.clone(),
                ),
            )
            .await
            .unwrap();
    }
    let _subscription = service
        .subscribe_events(&context, first.stream_id.clone(), None)
        .await
        .unwrap();
    let replay = service.execute_in_session(&context, request).await.unwrap();
    assert_eq!(replay.execution_id, first.execution_id);
    advance(&clock, 21);
    assert_eq!(service.reclaim_idle().await.reclaimed.len(), 1);
}
#[tokio::test]
async fn rejected_authorization_does_not_refresh_idle_deadline() {
    let (service, backend, clock, policy) = setup_clock();
    let context = ctx("user", "client");
    let opened = open(&service, &context, "denied").await;
    let token = service
        .issue_submission_token(
            &context,
            IdempotentOperation::ExecuteInSession,
            Some(&opened.session.handle),
        )
        .await
        .unwrap();
    advance(&clock, 80);
    policy.allow.store(false, Ordering::SeqCst);
    assert!(service
        .execute_in_session(
            &context,
            ExecuteInSessionRequest::new(
                opened.session.handle.clone(),
                CommandCall::new("query", serde_json::json!({})),
                token.idempotency_key
            )
        )
        .await
        .is_err());
    advance(&clock, 21);
    assert_eq!(service.reclaim_idle().await.reclaimed.len(), 1);
    assert!(lock(&backend.ids).is_empty());
}

#[tokio::test]
async fn failed_and_unknown_executions_refresh_without_changing_effect_evidence() {
    for command in ["fail", "unknown"] {
        let (service, _, clock, _) = setup_clock();
        let context = ctx("user", "client");
        let opened = open(&service, &context, "effects").await;
        advance(&clock, 80);
        let receipt = execute(&service, &context, &opened.session.handle, command).await;
        let result = terminal(&service, &context, &receipt.execution_id).await;
        if command == "fail" {
            assert_eq!(result.state, ExecutionState::Failed);
            assert_eq!(result.effect_outcome, EffectOutcome::RolledBack);
        } else {
            assert_eq!(result.effect_outcome, EffectOutcome::Unknown);
        }
        advance(&clock, 30);
        assert!(service.reclaim_idle().await.reclaimed.is_empty());
        advance(&clock, 71);
        assert_eq!(service.reclaim_idle().await.reclaimed.len(), 1);
    }
}
#[tokio::test]
async fn overlapping_background_dispatches_hold_idle_until_last_completion() {
    let (service, backend, clock, _) = setup_clock();
    let context = ctx("user", "client");
    let opened = open(&service, &context, "queue").await;
    let first = execute(&service, &context, &opened.session.handle, "hold").await;
    backend.started.notified().await;
    let token = service
        .issue_submission_token(
            &context,
            IdempotentOperation::ExecuteInSession,
            Some(&opened.session.handle),
        )
        .await
        .unwrap();
    let second = service
        .execute_in_session(
            &context,
            ExecuteInSessionRequest::new(
                opened.session.handle.clone(),
                CommandCall::new("hold", serde_json::json!({"queue":2})),
                token.idempotency_key,
            ),
        )
        .await
        .unwrap();
    advance(&clock, 200);
    assert!(service.reclaim_idle().await.reclaimed.is_empty());
    backend.release.notify_one();
    backend.started.notified().await;
    terminal(&service, &context, &first.execution_id).await;
    advance(&clock, 200);
    assert!(service.reclaim_idle().await.reclaimed.is_empty());
    backend.release.notify_one();
    terminal(&service, &context, &second.execution_id).await;
    advance(&clock, 99);
    assert!(service.reclaim_idle().await.reclaimed.is_empty());
    advance(&clock, 2);
    assert_eq!(service.reclaim_idle().await.reclaimed.len(), 1);
}
#[tokio::test]
async fn detach_during_execution_applies_grace_only_after_completion() {
    let (service, backend, clock, _) = setup_clock();
    let context = ctx("user", "client");
    service.with_idle_policy(
        IdlePolicy::with_deadline(Duration::from_millis(100), Duration::from_millis(10))
            .with_detached_grace(Some(Duration::from_millis(30))),
    );
    let opened = open(&service, &context, "detached").await;
    let receipt = execute(&service, &context, &opened.session.handle, "hold").await;
    backend.started.notified().await;
    service
        .detach_session(
            &context,
            AttachmentRequest::new(opened.session.handle.clone(), opened.attachment_token),
        )
        .await
        .unwrap();
    advance(&clock, 500);
    assert!(service.reclaim_idle().await.reclaimed.is_empty());
    backend.release.notify_one();
    terminal(&service, &context, &receipt.execution_id).await;
    advance(&clock, 29);
    assert!(service.reclaim_idle().await.reclaimed.is_empty());
    advance(&clock, 2);
    assert_eq!(service.reclaim_idle().await.reclaimed.len(), 1);
}

#[tokio::test]
async fn cancelled_execution_renews_idle_only_after_actual_terminal_cleanup() {
    let (service, backend, clock, _) = setup_clock();
    let context = ctx("user", "client");
    let opened = open(&service, &context, "cancelled").await;
    let receipt = execute(&service, &context, &opened.session.handle, "hold").await;
    backend.started.notified().await;
    advance(&clock, 500);
    service
        .cancel_execution(&context, receipt.execution_id.clone())
        .await
        .unwrap();
    let result = terminal(&service, &context, &receipt.execution_id).await;
    assert_eq!(result.state, ExecutionState::Cancelled);
    assert_eq!(result.effect_outcome, EffectOutcome::Unknown);
    advance(&clock, 99);
    assert!(service.reclaim_idle().await.reclaimed.is_empty());
    advance(&clock, 2);
    assert_eq!(service.reclaim_idle().await.reclaimed.len(), 1);
}

#[tokio::test]
async fn accepted_authorization_wait_holds_resource_then_dispatches() {
    let (service, backend, clock, policy) = setup_clock();
    let context = ctx("user", "client");
    let opened = open(&service, &context, "auth-wait").await;
    advance(&clock, 80);
    policy.wait_check.store(2, Ordering::SeqCst);
    let receipt = execute(&service, &context, &opened.session.handle, "query").await;
    policy.wait_entered.notified().await;
    advance(&clock, 100);
    assert!(service.reclaim_idle().await.reclaimed.is_empty());
    assert_eq!(backend.closed.load(Ordering::SeqCst), 0);
    assert!(lock(&backend.ids).is_empty());
    policy.wait_release.notify_one();
    terminal(&service, &context, &receipt.execution_id).await;
    assert_eq!(
        service
            .get_execution(&context, receipt.execution_id)
            .await
            .unwrap()
            .state,
        ExecutionState::Succeeded
    );
    advance(&clock, 99);
    assert!(service.reclaim_idle().await.reclaimed.is_empty());
    advance(&clock, 2);
    assert_eq!(service.reclaim_idle().await.reclaimed.len(), 1);
}
#[tokio::test]
async fn rejected_authorization_restores_original_idle_deadline() {
    let (service, backend, clock, policy) = setup_clock();
    let context = ctx("user", "client");
    let opened = open(&service, &context, "auth-reject").await;
    advance(&clock, 80);
    policy.wait_check.store(2, Ordering::SeqCst);
    policy.wait_deny.store(true, Ordering::SeqCst);
    let receipt = execute(&service, &context, &opened.session.handle, "query").await;
    policy.wait_entered.notified().await;
    advance(&clock, 40);
    assert!(service.reclaim_idle().await.reclaimed.is_empty());
    policy.wait_release.notify_one();
    terminal(&service, &context, &receipt.execution_id).await;
    assert!(lock(&backend.ids).is_empty());
    assert_eq!(service.reclaim_idle().await.reclaimed.len(), 1);
}
#[tokio::test]
async fn rejected_waiter_does_not_remove_concurrent_business_refresh() {
    let (service, backend, clock, policy) = setup_clock();
    let context = ctx("user", "client");
    let opened = open(&service, &context, "auth-concurrent").await;
    advance(&clock, 80);
    policy.wait_check.store(2, Ordering::SeqCst);
    policy.wait_deny.store(true, Ordering::SeqCst);
    let rejected = execute(&service, &context, &opened.session.handle, "query").await;
    policy.wait_entered.notified().await;
    let running = execute(&service, &context, &opened.session.handle, "hold").await;
    backend.started.notified().await;
    advance(&clock, 100);
    policy.wait_release.notify_one();
    terminal(&service, &context, &rejected.execution_id).await;
    assert!(service.reclaim_idle().await.reclaimed.is_empty());
    backend.release.notify_one();
    terminal(&service, &context, &running.execution_id).await;
    advance(&clock, 99);
    assert!(service.reclaim_idle().await.reclaimed.is_empty());
    advance(&clock, 2);
    assert_eq!(service.reclaim_idle().await.reclaimed.len(), 1);
}

#[tokio::test(start_paused = true)]
async fn authorization_timeout_releases_hold_without_business_renewal() {
    let (service, backend, clock, policy) = setup_clock();
    let context = ctx("user", "client");
    let opened = open(&service, &context, "auth-timeout").await;
    policy.wait_check.store(2, Ordering::SeqCst);
    let receipt = execute(&service, &context, &opened.session.handle, "query").await;
    policy.wait_entered.notified().await;
    advance(&clock, 101);
    assert!(service.reclaim_idle().await.reclaimed.is_empty());
    tokio::time::advance(Duration::from_secs(31)).await;
    terminal(&service, &context, &receipt.execution_id).await;
    assert!(lock(&backend.ids).is_empty());
    assert_eq!(service.reclaim_idle().await.reclaimed.len(), 1);
}

#[tokio::test]
async fn last_rejected_waiter_does_not_extend_completed_business_deadline() {
    let (service, _, clock, policy) = setup_clock();
    let context = ctx("user", "client");
    let opened = open(&service, &context, "auth-last-reject").await;
    advance(&clock, 80);
    policy.wait_check.store(2, Ordering::SeqCst);
    policy.wait_deny.store(true, Ordering::SeqCst);
    let rejected = execute(&service, &context, &opened.session.handle, "query").await;
    policy.wait_entered.notified().await;
    let business = execute(&service, &context, &opened.session.handle, "query").await;
    terminal(&service, &context, &business.execution_id).await;
    advance(&clock, 101);
    assert!(service.reclaim_idle().await.reclaimed.is_empty());
    policy.wait_release.notify_one();
    terminal(&service, &context, &rejected.execution_id).await;
    assert_eq!(service.reclaim_idle().await.reclaimed.len(), 1);
}
