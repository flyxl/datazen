//! Lifecycle half of the mysql provider contract: capability honesty,
//! acquire/close, execution, and cancellation.
//!
//! Names come from the shared doubles via one glob, so a test that does not
//! use a type simply does not mention it.

use super::test_support::*;

// --- capability honesty -------------------------------------------------

#[test]
fn capability_set_matches_the_migration_matrix() {
    let capabilities = provider_with(FakeMysql::new())
        .capabilities()
        .capabilities
        .clone();
    // 事实二 / §6.1: a transaction that only pins a connection for a while is
    // not a stateful session, and a pool of size one is never evidence of one.
    assert_eq!(capabilities.stateful_session, Availability::Unsupported);
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
async fn a_pool_of_one_is_never_reported_as_a_fixed_session() {
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
    assert_eq!(descriptor.session_continuity, SessionContinuity::Leased);
    assert!(!descriptor.session_continuity.is_fixed());
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
    // 1 granted, 1 released: the repeated close must not release again.
    assert_eq!(budget.granted(), vec![1]);
    assert_eq!(budget.released(), 1);
    assert_eq!(fake.read(|wire| wire.disconnect_calls), 1);
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
    assert_eq!(budget.granted(), vec![1]);
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
async fn closing_with_an_open_transaction_is_not_reported_as_clean() {
    let (provider, _fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;
    provider
        .begin_transaction(&handle, &TransactionOptions::default())
        .await
        .expect("begin should succeed");
    // The connection is gone, but the caller's transaction was resolved
    // without its knowledge: that is `CloseUnconfirmed`, not `Closed`.
    assert_eq!(
        provider.close_resource(&handle).await.unwrap(),
        CloseDisposition::CloseUnconfirmed
    );
    assert_eq!(
        budget.released(),
        1,
        "the permit is still released exactly once"
    );
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
    let chunks = sink
        .chunks
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone();
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].sql, "SELECT 1");
    assert_eq!(chunks[0].rows_affected, Some(1));
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
    let chunks = sink
        .chunks
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone();
    assert_eq!(chunks[0].rows_affected, Some(1));
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
    let chunks = sink
        .chunks
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone();
    match &chunks[0].rows[0][0] {
        Some(Value::Json(data)) => assert_eq!(data["echo"], "show_status"),
        other => panic!("expected a JSON result, got {other:?}"),
    }
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

// --- observation / context ----------------------------------------------
