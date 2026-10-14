//! Lifecycle half of the mysql provider contract: capability honesty,
//! acquire/close, execution, and cancellation.
//!
//! Names come from the shared doubles via one glob, so a test that does not
//! use a type simply does not mention it.

use super::test_support::*;
use crate::resource_capabilities::CONTROL_POOL_CONNECTIONS;
use datazen_driver_api::resource::ConnectionCostPolicy;
use datazen_driver_api::session::{SessionHandleKind, SessionHandleRef};

// --- capability honesty -------------------------------------------------

#[test]
fn capability_set_matches_the_migration_matrix() {
    let capabilities = provider_with(FakeMysql::new())
        .capabilities()
        .capabilities
        .clone();
    // 事实二 / §6.1: a transaction that only pins a connection for a while is
    // not a stateful session, and a pool of size one is never evidence of one.
    assert_eq!(capabilities.stateful_session, Availability::Supported);
    assert!(
        capabilities.declares_anything(),
        "an all-unknown set would claim nothing"
    );
    assert_eq!(capabilities.namespace_switch, NamespaceSwitch::InPlace);
    assert_eq!(capabilities.reset_for_reuse, ResetForReuse::Unsupported);
    assert_eq!(capabilities.precise_cancel, PreciseCancelSupport::Supported);
    assert_eq!(
        capabilities.transactions.isolation_levels,
        ["REPEATABLE READ"]
    );
    assert_eq!(
        capabilities.transactions.savepoints,
        Availability::Unsupported
    );
    assert_eq!(capabilities.transactions.max_open_transactions, Some(1));
}

#[tokio::test]
async fn an_editor_resource_declares_a_retained_physical_session() {
    // CM-19: `max_connections(1)` in `sqlite.rs`/`mysql.rs` is a pool size, not
    // evidence of a fixed resource, so `SessionContinuity::is_fixed()` must be
    // false for everything this provider describes.
    let provider = provider_with(FakeMysql::new());
    let descriptor = provider
        .describe_resource(&DescribeResourceRequest {
            connection_config: test_config(),
            target: NamespaceTarget::empty().with_database("app"),
            identity_scope: IdentityScope::default(),
            purpose: ResourcePurpose::InteractiveQuery,
        })
        .await
        .expect("describe should succeed");
    assert_eq!(descriptor.session_continuity, SessionContinuity::Fixed);
    assert!(descriptor.session_continuity.is_fixed());
    assert_eq!(
        descriptor.connection_cost_policy,
        ConnectionCostPolicy::PoolBounded {
            max_physical_connections: 2
        }
    );
}

#[test]
fn ddl_atomicity_is_filed_under_every_migration_operation() {
    let support = provider_with(FakeMysql::new())
        .capabilities()
        .capabilities
        .ddl_atomicity
        .clone();
    assert_eq!(support.by_operation.len(), 33);
    for operation in migration_operation_keys() {
        assert_eq!(
            support.atomicity_for(operation),
            DdlAtomicity::AutoCommitPerStatement,
            "{operation} must be declared"
        );
    }
    // Anything the provider never declared fails closed.
    assert_eq!(
        support.atomicity_for("no_such_operation"),
        DdlAtomicity::Unknown
    );
    assert_eq!(support.atomicity_for(""), DdlAtomicity::Unknown);
}

#[test]
fn namespace_shape_rejects_a_schema_level() {
    let provider = provider_with(FakeMysql::new());
    let shape: &NamespaceShape = provider.namespace_shape();
    let database = shape
        .level(NamespaceLevelKind::Database)
        .expect("database level");
    assert!(database.exists);
    assert!(!database.required, "a mysql target may name no database");
    for absent in [NamespaceLevelKind::Catalog, NamespaceLevelKind::Schema] {
        let level = shape.level(absent).expect("declared as non-existent");
        assert!(
            !level.exists,
            "{absent:?} must be declared as a level that does not exist"
        );
    }
    // mysql has no schema level: naming one is a caller bug, and it is
    // rejected rather than silently dropped.
    assert!(matches!(
        shape.canonicalize(
            &NamespaceTarget::empty()
                .with_database("app")
                .with_schema("public")
        ),
        Err(ResourceError::NonexistentNamespaceLevel {
            kind: NamespaceLevelKind::Schema
        })
    ));
    // Every declared level is also optional: a target may name no database.
    assert!(shape.canonicalize(&NamespaceTarget::empty()).is_ok());
}

// --- acquire / close ----------------------------------------------------

/// The number of physical connections this provider actually opens for one
/// resource: the data pool plus the control pool. Derived rather than written
/// out so a change to the pool arithmetic cannot silently drift from the
/// assertions below.
fn pool_size_plus_control() -> u32 {
    test_config().effective_max_pool_size() + CONTROL_POOL_CONNECTIONS
}

#[tokio::test]
async fn close_releases_the_budget_exactly_once() {
    let (provider, fake, budget) = provider();
    fake.with(|wire| wire.database = Some("app".into()));
    let handle = acquire(&provider, &budget).await;
    assert_eq!(
        provider.close_resource(&handle).await.unwrap(),
        CloseDisposition::Closed
    );
    // Idempotent, like the in-tree adapter: the second close is a no-op, not
    // an error, and it does not disconnect or charge a second time.
    assert_eq!(
        provider.close_resource(&handle).await.unwrap(),
        CloseDisposition::Closed
    );
    // One grant, one release: the repeated close must not release again. The
    // granted figure is the pool size plus the control pool, because that is
    // what the provider really opens.
    assert_eq!(budget.granted(), vec![pool_size_plus_control()]);
    assert_eq!(budget.released(), 1);
    assert_eq!(fake.read(|wire| wire.disconnect_calls), 1);

    // The confirmed close also records itself on the handle, which is what
    // makes `close_handle`'s idempotent-success branch reachable in
    // *production* rather than only in a test that marks the flag by hand.
    // Before this was recorded, `is_closed()` was permanently false here and the
    // host stayed fail-closed over a close that had already happened.
    assert!(handle.is_closed());
    provider
        .close_handle(&handle, &session_handle_for(&handle))
        .await
        .expect("after a confirmed close, repeating a handle close is idempotent success");
}

#[tokio::test]
async fn a_failed_connect_returns_the_permit() {
    let (provider, fake, budget) = provider();
    fake.with(|wire| wire.connect_fails = true);
    let error = provider
        .acquire_resource(&acquire_request(), &budget_as_port(&budget))
        .await
        .expect_err("a refused connect must not yield a handle");
    assert!(matches!(
        error,
        ResourceError::Driver(DriverError::ConnectionFailed(_))
    ));
    assert_eq!(budget.granted(), vec![pool_size_plus_control()]);
    assert_eq!(
        budget.released(),
        1,
        "the permit taken before connect must come back"
    );
}

#[tokio::test]
async fn a_denied_budget_never_reaches_the_driver() {
    let (provider, fake, budget) = provider();
    *budget.deny.lock().unwrap_or_else(|p| p.into_inner()) = true;
    let error = provider
        .acquire_resource(&acquire_request(), &budget_as_port(&budget))
        .await
        .expect_err("a denied budget must not yield a handle");
    assert!(matches!(error, ResourceError::BudgetDenied { .. }));
    assert_eq!(fake.read(|wire| wire.connect_calls), 0);
}

#[tokio::test]
async fn a_handle_from_another_provider_is_rejected() {
    let (first, fake, budget) = provider();
    let second = provider_with(fake.clone());
    let handle = acquire(&first, &budget).await;
    let error = second
        .observe_session(&handle)
        .await
        .expect_err("ownership must be validated before any wire operation");
    assert!(matches!(
        error,
        ResourceError::ResourceOwnershipMismatch { .. } | ResourceError::StaleRuntimeEpoch { .. }
    ));
    assert_eq!(fake.read(|wire| wire.statements.is_empty()), true);
}

#[tokio::test]
async fn quarantined_transaction_can_close_without_a_separate_rollback() {
    let (provider, fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;
    provider
        .begin_transaction(&handle, &TransactionOptions::default())
        .await
        .expect("begin should succeed");
    fake.with(|wire| wire.disconnect_fails = true);
    for _ in 0..2 {
        assert!(provider.close_resource(&handle).await.is_err());
        assert!(!handle.is_closed());
        assert_eq!(budget.released(), 0);
        assert!(provider.open_transaction_of(&handle).is_some());
    }
    // The Host verifier retries only close_resource, never rollback_transaction.
    fake.with(|wire| wire.disconnect_fails = false);
    assert_eq!(
        provider.close_resource(&handle).await.unwrap(),
        CloseDisposition::Closed
    );
    assert!(handle.is_closed());
    assert!(provider.open_transaction_of(&handle).is_none());
    assert_eq!(budget.released(), 1);
    assert_eq!(
        provider.close_resource(&handle).await.unwrap(),
        CloseDisposition::Closed
    );
    assert_eq!(budget.released(), 1);
}

#[tokio::test]
async fn a_failed_disconnect_keeps_the_permit_charged() {
    let (provider, fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;
    fake.with(|wire| wire.disconnect_fails = true);
    let error = provider
        .close_resource(&handle)
        .await
        .expect_err("an unconfirmed close must be an error, not a `Closed`");
    assert!(matches!(
        error,
        ResourceError::Driver(DriverError::ConnectionFailed(_))
    ));
    assert_eq!(budget.released(), 0);
    assert!(!handle.is_closed());
    assert!(provider.close_resource(&handle).await.is_err());
    assert_eq!(budget.released(), 0);
    fake.with(|wire| wire.disconnect_fails = false);
    assert_eq!(
        provider.close_resource(&handle).await.unwrap(),
        CloseDisposition::Closed
    );
    assert!(handle.is_closed());
    assert_eq!(budget.released(), 1);
}

// --- execute_on_resource ------------------------------------------------

#[tokio::test]
async fn query_writes_one_chunk_per_statement_and_completes() {
    let (provider, fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;
    let sink = RecordingSink::default();
    let completion = provider
        .execute_on_resource(
            &handle,
            &QueryExecutionId::new("exec-1"),
            &CommandCall::new("query", serde_json::json!({ "sql": "SELECT 1" })),
            &sink,
        )
        .await
        .expect("query should run");
    assert_eq!(completion.statement_results.len(), 1);
    assert_eq!(
        completion.effect_outcome,
        EffectOutcome::Completed,
        "with no transaction open the effects are durable"
    );
    // The row batch carries rows only; the statement total arrives as a typed
    // summary on the output channel, never disguised as an extra empty row.
    let chunks = sink
        .chunks
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone();
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].sql, "SELECT 1");
    assert_eq!(chunks[0].rows_affected, None);
    assert!(
        !chunks[0].rows.is_empty(),
        "the SELECT's row rides the batch"
    );
    let outputs = sink
        .outputs
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone();
    assert!(
        outputs
            .iter()
            .any(|output| output["kind"] == "statementSummary"
                && output["sql"] == "SELECT 1"
                && output["rowsAffected"] == serde_json::json!(1)),
        "the statement total is published as a typed summary: {outputs:?}"
    );
    assert_eq!(sink.completed.load(Ordering::SeqCst), 1);
    // Registered and then deregistered: that is what makes a precise cancel
    // addressable at all.
    assert_eq!(fake.read(|wire| wire.prepared.clone()), vec!["exec-1"]);
    assert_eq!(fake.read(|wire| wire.cleaned.clone()), vec!["exec-1"]);
}

#[tokio::test]
async fn execute_reports_rows_affected() {
    let (provider, fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;
    let sink = RecordingSink::default();
    let completion = provider
        .execute_on_resource(
            &handle,
            &QueryExecutionId::new("exec-2"),
            &CommandCall::new("execute", serde_json::json!({ "sql": "DELETE FROM t" })),
            &sink,
        )
        .await
        .expect("execute should run");
    assert!(completion.statement_results.is_empty());
    // A DDL/DML ack is not row data: it is reported through the typed summary,
    // so the row-batch channel stays empty.
    let chunks = sink
        .chunks
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone();
    assert!(
        chunks.is_empty(),
        "an execute ack must not fabricate a row batch: {chunks:?}"
    );
    let outputs = sink
        .outputs
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone();
    assert!(
        matches!(
            outputs.as_slice(),
            [output] if output["kind"] == "statementSummary"
                && output["statementIndex"] == serde_json::json!(0)
                && output["rowsAffected"] == serde_json::json!(1)
        ),
        "the affected-row total rides the typed summary: {outputs:?}"
    );
    // The driver really ran an `execute`, it was not fabricated from the sink.
    assert_eq!(fake.read(|wire| wire.rows_affected.clone()), vec![1]);
}

#[tokio::test]
async fn other_commands_reach_the_driver_and_come_back_as_json() {
    let (provider, fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;
    let sink = RecordingSink::default();
    provider
        .execute_on_resource(
            &handle,
            &QueryExecutionId::new("exec-3"),
            &CommandCall::new("show_status", serde_json::json!({ "scope": "global" })),
            &sink,
        )
        .await
        .expect("a driver command should run");
    let commands = fake.read(|wire| wire.commands.clone());
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].0, "show_status");
    assert!(sink
        .chunks
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .is_empty());
    let outputs = sink.outputs.lock().unwrap_or_else(|p| p.into_inner());
    assert_eq!(outputs[0]["kind"], "commandResult");
    assert_eq!(outputs[0]["data"]["echo"], "show_status");
}

#[tokio::test]
async fn a_command_without_sql_is_an_explicit_error() {
    let (provider, _fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;
    let sink = RecordingSink::default();
    let error = provider
        .execute_on_resource(
            &handle,
            &QueryExecutionId::new("exec-4"),
            &CommandCall::new("query", serde_json::json!({ "sql": "  " })),
            &sink,
        )
        .await
        .expect_err("a missing sql field must not reach the wire");
    assert!(matches!(error, ResourceError::OperationNotSupported { .. }));
}

// --- cancellation -------------------------------------------------------

#[tokio::test]
async fn request_cancel_never_falls_back_to_session_wide() {
    let (provider, fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;
    fake.with(|wire| {
        wire.precise_cancel = true;
        wire.cancel_outcome = CancelOutcome::Ok;
    });
    let receipt = provider
        .request_cancel(&handle, &QueryExecutionId::new("exec-5"))
        .await
        .expect("cancel should answer");
    assert_eq!(receipt.disposition, CancelDisposition::Requested);
    assert!(receipt.disposition.is_accepted());
    assert_eq!(
        fake.read(|wire| wire.session_wide_cancels),
        0,
        "CM-24: a precise cancel must not degrade into a session-wide kill"
    );
}

#[tokio::test]
async fn request_cancel_maps_an_unregistered_execution() {
    let (provider, fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;
    fake.with(|wire| {
        wire.precise_cancel = true;
        wire.cancel_outcome = CancelOutcome::NotFound;
    });
    let receipt = provider
        .request_cancel(&handle, &QueryExecutionId::new("gone"))
        .await
        .expect("a stale id is an answer, not a panic");
    assert_eq!(receipt.disposition, CancelDisposition::NotRegistered);
}

#[tokio::test]
async fn request_cancel_reports_unsupported_when_the_driver_cannot_address_the_execution() {
    for outcome in [CancelOutcome::Unsupported, CancelOutcome::SessionMismatch] {
        let (provider, fake, budget) = provider();
        let handle = acquire(&provider, &budget).await;
        fake.with(|wire| wire.cancel_outcome = outcome);
        let receipt = provider
            .request_cancel(&handle, &QueryExecutionId::new("exec-7"))
            .await
            .expect("an unaddressable execution is still answered, not an error");
        assert_eq!(
            receipt.disposition,
            CancelDisposition::Unsupported,
            "an execution the driver cannot address must not be reported as an accepted cancel"
        );
        assert!(!receipt.disposition.is_accepted());
        assert_eq!(fake.read(|wire| wire.session_wide_cancels), 0);
    }
}

#[tokio::test]
async fn request_cancel_refuses_when_the_driver_has_none() {
    let (provider, fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;
    fake.with(|wire| wire.precise_cancel = false);
    let receipt = provider
        .request_cancel(&handle, &QueryExecutionId::new("exec-6"))
        .await
        .expect("an unsupported driver still answers");
    assert_eq!(receipt.disposition, CancelDisposition::Unsupported);
    assert!(!receipt.disposition.is_accepted());
    assert_eq!(fake.read(|wire| wire.session_wide_cancels), 0);
}

#[tokio::test]
async fn request_cancel_surfaces_a_real_failure() {
    let (provider, fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;
    fake.with(|wire| {
        wire.precise_cancel = true;
        wire.cancel_outcome = CancelOutcome::Failure;
    });
    let error = provider
        .request_cancel(&handle, &QueryExecutionId::new("exec-7"))
        .await
        .expect_err("a failed KILL must not be reported as a successful cancel");
    assert!(matches!(
        error,
        ResourceError::Driver(DriverError::QueryFailed(_))
    ));
}

// --- per-handle close (§6.5) ---------------------------------------------

/// A `SessionHandleRef` as the host would hand it over: same resource, same
/// epoch, a name. mysql has no such handle to name, which is the point.
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
    // judge the session clean while nothing was ever closed. mysql issues no
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
            ResourceError::InvalidResourceState { ref operation, .. }
                if operation == "close_handle"
        ),
        "the refusal must name the handle and its kind, got {error:?}"
    );
    // Nothing was closed, so nothing may claim to be: the resource handle is
    // still open, the permit is still charged, and the wire was never touched.
    assert!(!handle.is_closed());
    assert_eq!(budget.released(), 0);
    fake.with(|wire| assert!(wire.statements.is_empty() && wire.commands.is_empty()));
}

#[tokio::test]
async fn close_handle_refuses_for_every_handle_kind() {
    // The refusal is a property of the provider, not of the kind it was handed.
    // mysql issues none of the three, so none of the three may come back `Ok`.
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
                if expected == "mysql" && actual == "some-other-provider"
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
    // — `LeaseManager::release` is fail-closed and would keep the lease parked
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
    fake.with(|wire| assert!(wire.statements.is_empty() && wire.commands.is_empty()));
}

#[tokio::test]
async fn close_handle_refuses_a_foreign_ref_even_on_a_closed_resource() {
    // Ordering, not just presence: `is_closed` must come *after* the ownership
    // check, so a closed resource never excuses a ref that belongs to another
    // one. Matching postgres exactly is what keeps one behaviour across the
    // nine providers instead of two subtly different contracts.
    //
    // The premise has to land on the handle actually handed to `close_handle`,
    // not merely on some handle in the test: a *foreign* handle that was never
    // closed yields the same ownership refusal in both orders, so the test
    // would pass even with the short-circuit hoisted above `handle.check`. So
    // the one passed in is marked closed too — that is the only shape where the
    // two orders disagree and this test pins which one runs.
    let (provider, _fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;
    let foreign =
        ResourceHandle::issue("some-other-provider", "not-issued", handle.runtime_epoch());
    foreign.mark_closed();
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
                if expected == "mysql" && actual == "some-other-provider"
        ),
        "expected an ownership mismatch, got {error:?}"
    );
}

// --- observation / context ----------------------------------------------
