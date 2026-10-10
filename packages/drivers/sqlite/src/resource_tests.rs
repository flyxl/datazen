//! Lifecycle half of the sqlite provider contract: capabilities, acquire and
//! close, statement execution and cancellation.
//!
//! Each test names the honesty rule it defends. Nothing here touches a real
//! database file — `FakeSqlite` is the whole wire.

use super::test_support::*;
use crate::resource_capabilities::sqlite_capability_set;
use datazen_driver_api::session::{SessionHandleKind, SessionHandleRef};

#[test]
fn factory_capabilities_refuse_everything_the_driver_cannot_do() {
    let capabilities = sqlite_capability_set();
    assert_eq!(capabilities.stateful_session, Availability::Unsupported);
    assert_eq!(capabilities.namespace_switch, NamespaceSwitch::Unsupported);
    assert_eq!(
        capabilities.precise_cancel,
        PreciseCancelSupport::Unsupported
    );
    assert_eq!(capabilities.reset_for_reuse, ResetForReuse::Unsupported);
    assert_eq!(
        capabilities.snapshots,
        datazen_driver_api::capabilities::SnapshotSupport::Unsupported
    );
    assert_eq!(
        capabilities.transaction_observation,
        datazen_driver_api::capabilities::TransactionObservation::Partial,
        "a `BEGIN` really does open a transaction; only the surrounding pool leaves its origin in doubt"
    );
    assert_eq!(
        capabilities.session_scoped_handles,
        datazen_driver_api::capabilities::SessionScopedHandleSupport::Unsupported
    );
}

#[tokio::test]
async fn the_descriptor_refuses_to_call_a_one_connection_pool_a_fixed_session() {
    let (provider, _fake, _budget) = provider();
    let descriptor = provider
        .describe_resource(&describe_request())
        .await
        .expect("describe must succeed for the declared shape");
    // CM-19: `connect` opens a pool of size one. That is a scheduling decision,
    // and reporting it as a pinned session is exactly the lie §6.1 forbids.
    assert_eq!(descriptor.session_continuity, SessionContinuity::Leased);
    assert!(!descriptor.session_continuity.is_fixed());
    assert_eq!(
        descriptor.reuse_policy,
        datazen_driver_api::resource::ReusePolicy::Unknown
    );
    assert!(descriptor.initialization_requirements.is_empty());
    assert!(matches!(
        descriptor.connection_cost_policy,
        datazen_driver_api::resource::ConnectionCostPolicy::DeclaredConservative {
            declared_cost: 1,
            hard_cap: None
        }
    ));
}

#[test]
fn the_namespace_shape_is_one_database_and_no_catalog_or_schema() {
    let shape = namespace_shape();
    let kinds: Vec<NamespaceLevelKind> = shape.levels.iter().map(|level| level.kind).collect();
    assert_eq!(
        kinds,
        vec![
            NamespaceLevelKind::Database,
            NamespaceLevelKind::Catalog,
            NamespaceLevelKind::Schema
        ]
    );
    assert!(shape.levels[0].exists);
    assert!(!shape.levels[1].exists);
    assert!(!shape.levels[2].exists);
}

#[tokio::test]
async fn acquire_charges_the_budget_once_and_close_releases_it_once() {
    let (provider, _fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;
    assert_eq!(budget.granted(), vec![1]);
    assert_eq!(budget.released(), 0);

    let disposition = provider
        .close_resource(&handle)
        .await
        .expect("close must succeed");
    assert_eq!(disposition, CloseDisposition::Closed);
    assert_eq!(budget.released(), 1);

    // Idempotent: a repeat close is not an error and does not charge again.
    let repeat = provider
        .close_resource(&handle)
        .await
        .expect("a repeated close is not an error");
    assert_eq!(repeat, CloseDisposition::Closed);
    assert_eq!(budget.released(), 1);
}

#[tokio::test]
async fn a_failed_connect_gives_the_budget_permit_back() {
    let (provider, fake, budget) = provider();
    fake.with(|wire| wire.connect_fails = true);
    let outcome = provider
        .acquire_resource(&acquire_request(), &budget_as_port(&budget))
        .await;
    assert!(matches!(outcome, Err(ResourceError::Driver(_))));
    assert_eq!(budget.granted(), vec![1]);
    assert_eq!(
        budget.released(),
        1,
        "a failed connect must not leak the permit"
    );
}

#[tokio::test]
async fn a_denied_budget_stops_the_provider_before_it_connects() {
    let (provider, fake, budget) = provider();
    budget.deny();
    let outcome = provider
        .acquire_resource(&acquire_request(), &budget_as_port(&budget))
        .await;
    assert!(matches!(outcome, Err(ResourceError::BudgetDenied { .. })));
    assert_eq!(fake.read(|wire| wire.connect_calls), 0);
}

#[tokio::test]
async fn a_failed_disconnect_keeps_the_permit_charged() {
    let (provider, fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;
    fake.with(|wire| wire.disconnect_fails = true);
    let outcome = provider.close_resource(&handle).await;
    assert!(matches!(outcome, Err(ResourceError::Driver(_))));
    assert_eq!(
        budget.released(),
        0,
        "the physical connection may still be alive; the charge stays until the leak is settled"
    );
}

#[tokio::test]
async fn a_handle_from_another_provider_is_rejected_everywhere() {
    let (provider, _fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;
    let forged = forged_handle(&provider);

    assert!(matches!(
        provider
            .execute_on_resource(
                &forged,
                &QueryExecutionId::new("exec-1"),
                &call("query"),
                &RecordingSink::default()
            )
            .await,
        Err(ResourceError::StaleRuntimeEpoch { .. })
    ));
    assert!(provider.observe_session(&forged).await.is_err());
    assert!(provider
        .request_cancel(&forged, &QueryExecutionId::new("exec-1"))
        .await
        .is_err());
    assert!(provider.reset_resource(&forged, &baseline()).await.is_err());
    assert!(provider.close_resource(&forged).await.is_err());

    // The real handle still works afterwards: a forged call changed nothing.
    assert!(provider.observe_session(&handle).await.is_ok());
}

#[tokio::test]
async fn a_query_reaches_the_driver_and_streams_every_statement() {
    let (provider, fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;
    let sink = RecordingSink::default();
    let execution = QueryExecutionId::new("exec-1");

    let completion = provider
        .execute_on_resource(&handle, &execution, &call("query"), &sink)
        .await
        .expect("the query must round-trip");

    assert_eq!(completion.completion_status, completion_status_succeeded());
    assert_eq!(sink.chunks.lock().unwrap().len(), 1);
    assert_eq!(sink.completed.load(Ordering::SeqCst), 1);
    assert_eq!(
        fake.read(|wire| wire.statements.clone()),
        vec!["SELECT 1".to_string()]
    );
    // Registered and then deregistered, so a cancel cannot address a finished run.
    assert_eq!(
        fake.read(|wire| wire.prepared.clone()),
        vec![execution.as_str().to_string()]
    );
    assert_eq!(
        fake.read(|wire| wire.cleaned.clone()),
        vec![execution.as_str().to_string()]
    );
}

#[tokio::test]
async fn an_execute_reports_the_rows_affected_it_was_given() {
    let (provider, _fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;
    let sink = RecordingSink::default();

    provider
        .execute_on_resource(
            &handle,
            &QueryExecutionId::new("exec-1"),
            &call("execute"),
            &sink,
        )
        .await
        .expect("the statement must round-trip");

    let chunks = sink.chunks.lock().unwrap();
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].rows_affected, Some(1));
    assert!(chunks[0].rows.is_empty());
}

#[tokio::test]
async fn a_command_the_provider_does_not_know_is_dispatched_to_the_driver() {
    let (provider, fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;
    let sink = RecordingSink::default();

    let completion = provider
        .execute_on_resource(
            &handle,
            &QueryExecutionId::new("exec-1"),
            &CommandCall::new("list_tables", serde_json::json!({})),
            &sink,
        )
        .await
        .expect("the driver owns this command");

    assert_eq!(completion.statement_results.len(), 0);
    assert_eq!(
        fake.read(|wire| wire.commands.first().cloned()),
        Some(("list_tables".to_string(), "{}".to_string()))
    );
    assert_eq!(sink.chunks.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn a_command_without_sql_is_refused_with_an_explicit_reason() {
    let (provider, _fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;
    let outcome = provider
        .execute_on_resource(
            &handle,
            &QueryExecutionId::new("exec-1"),
            &CommandCall::new("query", serde_json::json!({ "limit": 5 })),
            &RecordingSink::default(),
        )
        .await;
    match outcome {
        Err(ResourceError::OperationNotSupported {
            operation, reason, ..
        }) => {
            assert_eq!(operation, "execute_on_resource");
            assert!(
                reason.contains("sql"),
                "the reason must name what is missing"
            );
        }
        other => panic!("expected an explicit refusal, got {other:?}"),
    }
}

#[tokio::test]
async fn cancellation_is_refused_and_never_falls_back_to_a_session_wide_kill() {
    let (provider, fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;

    let receipt = provider
        .request_cancel(&handle, &QueryExecutionId::new("exec-1"))
        .await
        .expect("a refusal is a receipt, not an error");

    assert_eq!(receipt.disposition, CancelDisposition::Unsupported);
    assert!(
        receipt.state.is_none(),
        "an unsupported cancel must not invent an execution state"
    );
    // The whole point: neither cancel path on the driver was reached.
    assert_eq!(fake.read(|wire| wire.session_wide_cancels), 0);
    assert_eq!(fake.read(|wire| wire.targeted_cancels), 0);
}

#[tokio::test]
async fn the_driver_itself_refuses_a_session_wide_cancel() {
    // The provider refuses; the driver must not answer `Ok(())` behind it.
    // `driver-capability-migration.md` §7.1 names this exact call.
    use datazen_driver_api::DatabaseDriver;
    let driver = crate::SqliteDriver::new();
    let handle = ConnectionHandle {
        id: "sqlite_pool_1".into(),
        pool_id: "sqlite_pool_1".into(),
    };
    assert!(matches!(
        driver.cancel_query(&handle).await,
        Err(DriverError::Unsupported(_))
    ));
}

fn call(command: &str) -> CommandCall {
    CommandCall::new(command, serde_json::json!({ "sql": "SELECT 1" }))
}

fn completion_status_succeeded() -> datazen_driver_api::session::CompletionStatus {
    datazen_driver_api::session::CompletionStatus::Succeeded
}

// --- per-handle close (§6.5) ---------------------------------------------

/// A `SessionHandleRef` as the host would hand it over: same resource, same
/// epoch, a name. sqlite has no such handle to name, which is the point.
fn session_handle_for(handle: &ResourceHandle) -> SessionHandleRef {
    SessionHandleRef::new(
        "tx-1",
        SessionHandleKind::Transaction,
        handle.resource_key().to_string(),
        handle.runtime_epoch(),
    )
}

#[tokio::test]
async fn close_handle_refuses_rather_than_claiming_a_close_that_never_happened() {
    // §6.5: a returned `Ok(())` here would let the host deregister the ref and
    // judge the session clean while nothing was ever closed. sqlite issues no
    // session-scoped handle at all, so the honest answer is a refusal — and a
    // refusal that names the handle, so the mismatch is diagnosable.
    let (provider, fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;

    let error = provider
        .close_handle(&handle, &session_handle_for(&handle))
        .await
        .expect_err("a provider that issues no session-scoped handle must not report one closed");

    assert!(
        matches!(
            error,
            ResourceError::OperationNotSupported { ref operation, ref reason, .. }
                if operation == "close_handle"
                    && reason.contains("tx-1")
                    && reason.contains("Transaction")
        ),
        "the refusal must name the handle and its kind, got {error:?}"
    );
    // Nothing was closed, so nothing may claim to be: the resource handle is
    // still open, the permit is still charged, and the wire was never touched.
    assert!(!handle.is_closed());
    assert_eq!(budget.released(), 0);
    fake.with(|wire| assert!(wire.statements.is_empty()));
}

#[tokio::test]
async fn close_handle_refuses_for_every_handle_kind() {
    // The refusal is a property of the provider, not of the kind it was handed.
    // sqlite issues none of the three, so none of the three may come back `Ok`.
    let (provider, _fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;

    for kind in [
        SessionHandleKind::Transaction,
        SessionHandleKind::Cursor,
        SessionHandleKind::ServerPrepared,
    ] {
        let session_handle = SessionHandleRef::new(
            format!("{kind:?}-1"),
            kind,
            handle.resource_key().to_string(),
            handle.runtime_epoch(),
        );
        assert!(
            provider
                .close_handle(&handle, &session_handle)
                .await
                .is_err(),
            "a {kind:?} handle must be refused too"
        );
    }
    assert!(!handle.is_closed());
}

#[tokio::test]
async fn close_handle_checks_ownership_before_it_refuses() {
    // A refusal must not also swallow a wrong-handle bug: the ownership check
    // runs first, so a handle from another provider is reported as exactly
    // that rather than as "this driver has no such handles".
    let (provider, _fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;
    let foreign =
        ResourceHandle::issue("some-other-provider", "not-issued", handle.runtime_epoch());
    let session_handle = SessionHandleRef::new(
        "tx-1",
        SessionHandleKind::Transaction,
        "not-issued".to_string(),
        handle.runtime_epoch(),
    );

    let error = provider
        .close_handle(&foreign, &session_handle)
        .await
        .expect_err("a handle this provider never issued cannot be closed through it");

    assert!(
        matches!(
            error,
            ResourceError::ResourceOwnershipMismatch { ref expected, ref actual }
                if expected == "sqlite" && actual == "some-other-provider"
        ),
        "expected an ownership mismatch, got {error:?}"
    );
}

#[tokio::test]
async fn close_handle_on_an_already_closed_resource_is_idempotent_success() {
    // §6.5 names exactly one legitimate repeat: the resource handle is already
    // marked closed, so the close has happened and re-doing it is not
    // information. A host that released the lease before this ref went away is
    // not a caller that missed its own handle, so it must not be told an error
    // — `LeaseFacade::release` is fail-closed and would keep the lease parked
    // forever. The refusal below stays the honest answer for a *live* resource.
    let (provider, fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;
    handle.mark_closed();

    provider
        .close_handle(&handle, &session_handle_for(&handle))
        .await
        .expect("closing a handle on an already-closed resource is idempotent success");

    // Nothing may be touched on the way to that `Ok`: the wire is silent, no
    // permit is released (the resource's own close already did that), and the
    // flag stays flipped rather than being the evidence this call manufactured.
    assert!(handle.is_closed());
    assert_eq!(budget.released(), 0);
    fake.with(|wire| assert!(wire.statements.is_empty()));
}

#[tokio::test]
async fn close_handle_refuses_a_foreign_ref_even_on_a_closed_resource() {
    // Ordering, not just presence: `is_closed` must come *after* the ownership
    // check, so a closed resource never excuses a ref that belongs to another
    // one. Matching postgres exactly is what keeps one behaviour across the
    // nine providers instead of two subtly different contracts.
    let (provider, _fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;
    handle.mark_closed();
    let foreign =
        ResourceHandle::issue("some-other-provider", "not-issued", handle.runtime_epoch());
    let session_handle = SessionHandleRef::new(
        "tx-1",
        SessionHandleKind::Transaction,
        "not-issued".to_string(),
        handle.runtime_epoch(),
    );

    let error = provider
        .close_handle(&foreign, &session_handle)
        .await
        .expect_err("a closed resource must not excuse a ref from another one");

    assert!(
        matches!(
            error,
            ResourceError::ResourceOwnershipMismatch { ref expected, ref actual }
                if expected == "sqlite" && actual == "some-other-provider"
        ),
        "expected an ownership mismatch, got {error:?}"
    );
}
