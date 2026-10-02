//! Lifecycle half of the mongodb provider contract: capabilities, acquire and
//! close, statement execution and cancellation.
//!
//! Each test names the honesty rule it defends. Nothing here touches a real
//! MongoDB deployment — `FakeMongodb` is the whole wire.

use super::test_support::*;
use crate::resource_capabilities::mongodb_capability_set;

#[test]
fn factory_capabilities_refuse_everything_the_driver_cannot_do() {
    let capabilities = mongodb_capability_set();
    assert_eq!(capabilities.stateful_session, Availability::Unsupported);
    assert_eq!(
        capabilities.namespace_switch,
        NamespaceSwitch::Unknown,
        "§2.1 defers this cell to P0 verification, and an unverified cell is Unknown — not Unsupported, and never InPlace"
    );
    assert_eq!(
        capabilities.context_observation,
        datazen_driver_api::capabilities::ContextObservation::Unsupported
    );
    assert_eq!(
        capabilities.transaction_observation,
        datazen_driver_api::capabilities::TransactionObservation::Unsupported
    );
    assert_eq!(
        capabilities.session_scoped_handles,
        SessionScopedHandleSupport::Unsupported
    );
    assert_eq!(capabilities.reset_for_reuse, ResetForReuse::Unsupported);
    assert_eq!(
        capabilities.precise_cancel,
        PreciseCancelSupport::Unsupported
    );
    assert_eq!(
        capabilities.snapshots,
        datazen_driver_api::capabilities::SnapshotSupport::Unsupported
    );
    assert!(capabilities.transactions.isolation_levels.is_empty());
    assert_eq!(
        capabilities.transactions.savepoints,
        Availability::Unsupported
    );
    assert_eq!(
        capabilities.transactions.max_open_transactions, None,
        "declaring a maximum would imply a transaction can be opened at all"
    );
}

#[test]
fn every_migration_operation_declares_an_unknown_ddl_atomicity() {
    let capabilities = mongodb_capability_set();
    for operation in crate::resource_capabilities::migration_operation_keys() {
        assert_eq!(
            capabilities.ddl_atomicity.atomicity_for(operation),
            DdlAtomicity::Unknown,
            "§2.1 records no verified DDL atomicity for mongodb; `{}` must not be filled in from a guess",
            operation
        );
    }
}

#[tokio::test]
async fn the_descriptor_refuses_to_call_a_client_pool_a_fixed_session() {
    let (provider, _fake, _budget) = provider();
    let descriptor = provider
        .describe_resource(&describe_request())
        .await
        .expect("describe must succeed for the declared shape");
    // A `Client` owns MongoDB's own connection pool and every statement names
    // its database explicitly. Reporting it as a pinned session is the lie §6.1
    // forbids.
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
        "the client may still be alive; the charge stays until the leak is settled"
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
        .change_context(&forged, &NamespaceTarget::empty())
        .await
        .is_err());
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

    assert_eq!(
        completion.completion_status,
        datazen_driver_api::session::CompletionStatus::Succeeded
    );
    assert_eq!(sink.chunks.lock().unwrap().len(), 1);
    assert_eq!(sink.completed.load(Ordering::SeqCst), 1);
    assert_eq!(fake.read(|wire| wire.statements.len()), 1);
    // Registered and then deregistered, so a cancel cannot address a finished
    // run.
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
async fn the_namespace_travels_with_the_statement_not_with_the_client() {
    let (provider, fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;

    // Two statements, two databases, one client — the same shape a caller gets
    // from the Data Explorer. Nothing on the client may move between them.
    for database in ["billing", "archive"] {
        provider
            .execute_on_resource(
                &handle,
                &QueryExecutionId::new(format!("exec-{database}")),
                &CommandCall::new(
                    "query",
                    serde_json::json!({ "sql": format!(r#"{{"collection": "orders", "database": "{database}"}}"#) }),
                ),
                &RecordingSink::default(),
            )
            .await
            .expect("the statement must round-trip");
    }

    // MongoDB namespaces live on the command, so the provider cannot have
    // switched anything on the client — and this proves it passed each
    // database through instead of collapsing them onto one.
    assert_eq!(
        fake.read(|wire| wire.statement_databases.clone()),
        vec!["billing".to_string(), "archive".to_string()]
    );
    assert_eq!(fake.read(|wire| wire.session_wide_cancels), 0);
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
    assert_eq!(chunks[0].rows_affected, Some(2));
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
            &CommandCall::new("list_collections", serde_json::json!({})),
            &sink,
        )
        .await
        .expect("the driver owns this command");

    assert_eq!(completion.statement_results.len(), 0);
    assert_eq!(
        fake.read(|wire| wire.commands.first().cloned()),
        Some(("list_collections".to_string(), "{}".to_string()))
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
    let driver = crate::MongodbDriver::new();
    let handle = ConnectionHandle {
        id: "mongodb_pool_1".into(),
        pool_id: "mongodb_pool_1".into(),
    };
    assert!(matches!(
        driver.cancel_query(&handle).await,
        Err(DriverError::Unsupported(_))
    ));
}

/// A syntactically valid MongoDB command, the way a caller really sends one:
/// the namespace travels inside `sql`, and `database` is the envelope field the
/// provider reads for the `SqlTarget`.
fn call(command: &str) -> CommandCall {
    CommandCall::new(
        command,
        serde_json::json!({ "sql": r#"{"collection": "orders"}"# }),
    )
}
