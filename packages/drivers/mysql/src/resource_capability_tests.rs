//! Capability half of the mysql provider contract: what the provider
//! observes, switches, transacts and resets — and what it refuses.
//!
//! Names come from the shared doubles via one glob, so a test that does not
//! use a type simply does not mention it.

use super::test_support::*;
use super::ConnectionCostPolicy;
use crate::resource_capabilities::{mysql_connection_cost, CONTROL_POOL_CONNECTIONS};

#[tokio::test]
async fn observe_session_reports_a_read_back_namespace_as_partial() {
    let (provider, fake, budget) = provider();
    fake.with(|wire| wire.database = Some("app".into()));
    let handle = acquire(&provider, &budget).await;
    let observation = provider.observe_session(&handle).await.expect("observe");
    assert_eq!(
        observation.context.namespace.database.as_deref(),
        Some("app")
    );
    assert_eq!(
        observation.context.confidence,
        ObservationConfidence::Partial
    );
    assert!(
        !observation.context.confidence.is_confirmed(),
        "`context_observation` is Partial, so nothing may be Confirmed"
    );
    assert!(
        observation.handles.is_empty(),
        "session_scoped_handles is Unsupported"
    );
    assert_eq!(
        observation.transaction.state,
        TransactionState::Unknown,
        "no transaction was opened"
    );
}

#[tokio::test]
async fn observe_session_falls_back_to_unknown_when_the_read_back_fails() {
    let (provider, fake, budget) = provider();
    fake.with(|wire| wire.database = None);
    let handle = acquire(&provider, &budget).await;
    let observation = provider.observe_session(&handle).await.expect("observe");
    assert_eq!(
        observation.context.confidence,
        ObservationConfidence::Unknown
    );
    assert!(observation.context.namespace.database.is_none());
}

#[tokio::test]
async fn change_context_is_confirmed_only_on_an_exact_read_back() {
    let (provider, fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;
    fake.with(|wire| wire.database = Some("app".into()));
    assert_eq!(
        provider
            .change_context(&handle, &NamespaceTarget::empty().with_database("other"))
            .await
            .unwrap(),
        ContextChangeDisposition::RequiresReplacement,
        "the USE was issued but the read-back did not match"
    );
    assert_eq!(fake.read(|wire| wire.use_targets.clone()), vec!["other"]);

    fake.with(|wire| wire.database = Some("other".into()));
    assert_eq!(
        provider
            .change_context(&handle, &NamespaceTarget::empty().with_database("other"))
            .await
            .unwrap(),
        ContextChangeDisposition::Confirmed
    );
}

#[tokio::test]
async fn change_context_refuses_while_a_transaction_is_pinned() {
    let (provider, fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;
    provider
        .begin_transaction(&handle, &TransactionOptions::default())
        .await
        .expect("begin");
    assert_eq!(
        provider
            .change_context(&handle, &NamespaceTarget::empty().with_database("other"))
            .await
            .unwrap(),
        ContextChangeDisposition::RequiresReplacement
    );
    assert!(
        fake.read(|wire| wire.use_targets.is_empty()),
        "no USE may be issued on a pinned transaction connection"
    );
}

#[tokio::test]
async fn change_context_without_a_database_is_unsupported() {
    let (provider, _fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;
    assert_eq!(
        provider
            .change_context(&handle, &NamespaceTarget::empty())
            .await
            .unwrap(),
        ContextChangeDisposition::Unsupported
    );
}

#[tokio::test]
async fn change_context_refuses_a_schema_level() {
    let (provider, _fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;
    let error = provider
        .change_context(
            &handle,
            &NamespaceTarget::empty()
                .with_database("app")
                .with_schema("public"),
        )
        .await
        .expect_err("mysql has no schema level");
    assert!(matches!(
        error,
        ResourceError::NonexistentNamespaceLevel { .. }
            | ResourceError::MissingRequiredNamespaceLevel { .. }
            | ResourceError::ForbiddenNamespaceLevel { .. }
            | ResourceError::NamespaceTargetRejected { .. }
    ));
}

// --- transactions -------------------------------------------------------

#[tokio::test]
async fn begin_refuses_an_undeclared_isolation_level() {
    let (provider, fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;
    let error = provider
        .begin_transaction(
            &handle,
            &TransactionOptions {
                isolation_level: Some("SERIALIZABLE".into()),
                read_only: None,
                defers_commit: false,
            },
        )
        .await
        .expect_err("only REPEATABLE READ is declared, and only that is issued");
    assert!(matches!(error, ResourceError::OperationNotSupported { .. }));
    assert_eq!(
        fake.read(|wire| wire.begin_calls + wire.snapshot_calls),
        0,
        "a refused level must not reach the wire"
    );
}

#[tokio::test]
async fn read_only_begin_uses_the_snapshot_statement() {
    let (provider, fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;
    let observation = provider
        .begin_transaction(
            &handle,
            &TransactionOptions {
                isolation_level: Some("repeatable read".into()),
                read_only: Some(true),
                defers_commit: false,
            },
        )
        .await
        .expect("begin");
    assert_eq!(observation.state, TransactionState::Active);
    assert_eq!(
        fake.read(|wire| wire.snapshot_calls),
        1,
        "`START TRANSACTION WITH CONSISTENT SNAPSHOT, READ ONLY` is the read-only path"
    );
    assert_eq!(fake.read(|wire| wire.begin_calls), 0);
}

#[tokio::test]
async fn a_second_transaction_on_one_resource_is_refused() {
    let (provider, _fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;
    provider
        .begin_transaction(&handle, &TransactionOptions::default())
        .await
        .expect("first begin");
    let error = provider
        .begin_transaction(&handle, &TransactionOptions::default())
        .await
        .expect_err("max_open_transactions is 1");
    assert!(matches!(error, ResourceError::InvalidResourceState { .. }));
}

#[tokio::test]
async fn commit_reports_completed_and_a_failed_commit_reports_unknown() {
    let (provider, fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;

    provider
        .begin_transaction(&handle, &TransactionOptions::default())
        .await
        .expect("begin");
    fake.with(|wire| wire.commit_fails = true);
    let failed = provider.commit_transaction(&handle).await.expect("commit");
    assert_eq!(failed.state, TransactionState::Unknown);
    assert_eq!(failed.effect, Some(EffectOutcome::Unknown));
    assert!(!failed.effect.unwrap().is_certain());

    provider
        .begin_transaction(&handle, &TransactionOptions::default())
        .await
        .expect("begin again");
    fake.with(|wire| wire.commit_fails = false);
    let committed = provider.commit_transaction(&handle).await.expect("commit");
    assert_eq!(committed.effect, Some(EffectOutcome::Completed));
}

#[tokio::test]
async fn committing_without_an_open_transaction_is_an_explicit_error() {
    let (provider, _fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;
    let error = provider
        .commit_transaction(&handle)
        .await
        .expect_err("there is nothing to commit");
    assert!(matches!(
        error,
        ResourceError::InvalidResourceState { .. }
            | ResourceError::TransactionResolutionRequired { .. }
    ));
}

#[tokio::test]
async fn an_open_transaction_downgrades_a_statement_effect_to_partial() {
    let (provider, _fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;
    provider
        .begin_transaction(&handle, &TransactionOptions::default())
        .await
        .expect("begin");
    let sink = RecordingSink::default();
    let completion = provider
        .execute_on_resource(
            &handle,
            &QueryExecutionId::new("exec-8"),
            &CommandCall::new("query", serde_json::json!({ "sql": "SELECT 1" })),
            &sink,
        )
        .await
        .expect("query");
    assert_eq!(
        completion.effect_outcome,
        EffectOutcome::PartiallyApplied,
        "inside a transaction the effects can still be rolled back"
    );
}

#[tokio::test]
async fn the_provider_tracks_transaction_ids_itself() {
    // The driver keeps no transaction registry, so the provider's own map is
    // the only place the id can come from; a `None` there would be a hole in
    // `observe_session`.
    let (provider, _fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;
    assert!(super::lock(&provider.transactions)
        .get(handle.resource_key())
        .is_none());
    provider
        .begin_transaction(&handle, &TransactionOptions::default())
        .await
        .expect("begin");
    let open: Option<(String, bool)> = super::lock(&provider.transactions)
        .get(handle.resource_key())
        .map(|transaction| (transaction.handle.id.clone(), transaction.snapshot));
    assert_eq!(open, Some(("mysql_tx_1".to_string(), false)));
    provider
        .rollback_transaction(&handle)
        .await
        .expect("rollback");
    assert!(super::lock(&provider.transactions)
        .get(handle.resource_key())
        .is_none());
}

// --- reset --------------------------------------------------------------

#[tokio::test]
async fn reset_always_discards_because_no_baseline_path_is_verified() {
    let (provider, _fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;
    assert_eq!(
        provider.reset_resource(&handle, &baseline()).await.unwrap(),
        ResetDisposition::Discard
    );
}

// --- declared cost -------------------------------------------------------

/// CM-30: the cost a provider declares must cover the connections it really
/// opens. `MysqlDriver::connect` builds a data pool of
/// `effective_max_pool_size()` plus a separate control pool of one, so a
/// declared cost of 1 understated the real figure by the entire pool and would
/// have let a caller size a budget the provider then exceeds.
#[test]
fn the_declared_cost_covers_the_data_pool_and_the_control_pool() {
    let config = test_config();
    let ConnectionCostPolicy::PoolBounded {
        max_physical_connections,
    } = mysql_connection_cost(&config)
    else {
        panic!("mysql owns its pool and knows its size; it must not declare a guess");
    };
    assert_eq!(
        max_physical_connections,
        config.effective_max_pool_size() + CONTROL_POOL_CONNECTIONS,
        "the declared cost must be the data pool plus the control pool"
    );
    assert!(
        max_physical_connections > 1,
        "a pool of more than one connection can never honestly cost 1"
    );
}

#[tokio::test]
async fn the_descriptor_reports_the_same_cost_the_driver_will_charge() {
    let (provider, _fake, _budget) = provider();
    let descriptor = provider
        .describe_resource(&DescribeResourceRequest {
            connection_config: test_config(),
            target: NamespaceTarget::empty().with_database("app"),
            identity_scope: IdentityScope::default(),
            purpose: ResourcePurpose::InteractiveQuery,
        })
        .await
        .expect("describe");
    assert_eq!(
        descriptor.connection_cost_policy,
        mysql_connection_cost(&test_config()),
        "describe and acquire must not be able to disagree about the cost"
    );
}

// --- unobservable shapes ------------------------------------------------

#[tokio::test]
async fn the_provider_never_reports_a_session_scoped_handle() {
    let (provider, _fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;
    let observation: SessionObservation = provider.observe_session(&handle).await.unwrap();
    assert!(observation.handles.is_empty());
    assert!(observation.protocol_drained);
}
