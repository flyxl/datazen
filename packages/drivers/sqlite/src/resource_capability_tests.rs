//! Capability half of the sqlite provider contract: observation, context
//! switching, transactions, reset and the unobservable shapes.
//!
//! Each test names the honesty rule it defends. Nothing here touches a real
//! database file — `FakeSqlite` is the whole wire.

use super::test_support::*;
use crate::resource_capabilities::{migration_operation_keys, sqlite_capability_set};

#[tokio::test]
async fn observation_comes_off_the_wire_and_stays_partial() {
    let (provider, fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;

    let observation = provider
        .observe_session(&handle)
        .await
        .expect("observation must reach the driver");

    assert_eq!(
        fake.read(|wire| wire.statements.last().cloned()),
        Some("PRAGMA database_list".to_string()),
        "the namespace must be read back, not remembered"
    );
    assert_eq!(
        observation.context.namespace.database.as_deref(),
        Some("main")
    );
    // `context_observation` is `Partial`: the attached databases came off the
    // wire but no identity and no autocommit flag were read, so the context as
    // a whole is never `Confirmed`.
    assert_eq!(
        observation.context.confidence,
        ObservationConfidence::Partial
    );
    assert!(!observation.context.confidence.is_confirmed());
    assert!(observation.context.autocommit.is_none());
    assert!(observation.context.effective_identity.is_none());
    assert!(observation.context.search_path.is_empty());
    assert!(observation.handles.is_empty());
}

#[tokio::test]
async fn an_empty_read_back_is_reported_as_unknown_not_as_a_guess() {
    let (provider, fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;
    fake.with(|wire| wire.attached = Some(Vec::new()));

    let observation = provider
        .observe_session(&handle)
        .await
        .expect("still an observation");
    assert_eq!(
        observation.context.confidence,
        ObservationConfidence::Unknown
    );
    assert!(
        observation.context.namespace.database.is_none(),
        "an unobservable namespace must be absent, not borrowed from the connect config"
    );
}

#[tokio::test]
async fn an_observed_transaction_is_active_and_a_quiet_one_is_unknown() {
    let (provider, _fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;

    // No record: the physical connection comes from a pool, so the provider
    // cannot rule out a `BEGIN` somebody else left open. `Idle` would be an
    // invented observation.
    let quiet = provider
        .observe_session(&handle)
        .await
        .expect("observation");
    assert_eq!(quiet.transaction.state, TransactionState::Unknown);
    assert!(quiet.transaction.transaction_id.is_none());

    provider
        .begin_transaction(&handle, &TransactionOptions::default())
        .await
        .expect("a bare BEGIN must open");
    let busy = provider
        .observe_session(&handle)
        .await
        .expect("observation");
    assert_eq!(busy.transaction.state, TransactionState::Active);
    assert_eq!(
        busy.transaction.transaction_id.as_deref(),
        Some("sqlite_tx_1")
    );
}

#[tokio::test]
async fn a_context_switch_is_refused_because_a_file_is_not_a_session() {
    let (provider, fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;

    let disposition = provider
        .change_context(&handle, &NamespaceTarget::empty().with_database("other"))
        .await
        .expect("a refusal is a disposition, not an error");

    assert_eq!(disposition, ContextChangeDisposition::Unsupported);
    assert!(
        fake.read(|wire| wire.statements.is_empty()),
        "refusing must not issue anything on the wire"
    );
}

#[tokio::test]
async fn a_named_isolation_level_is_refused_because_none_is_issued() {
    let (provider, fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;

    let outcome = provider
        .begin_transaction(
            &handle,
            &TransactionOptions {
                isolation_level: Some("SERIALIZABLE".to_string()),
                read_only: None,
                defers_commit: false,
            },
        )
        .await;

    match outcome {
        Err(ResourceError::OperationNotSupported {
            operation, reason, ..
        }) => {
            assert_eq!(operation, "begin_transaction");
            assert!(
                reason.contains("SERIALIZABLE"),
                "the reason must name what was refused"
            );
        }
        other => panic!("expected an explicit refusal, got {other:?}"),
    }
    assert_eq!(
        fake.read(|wire| wire.begin_calls),
        0,
        "a refused BEGIN must not touch the wire"
    );
}

#[tokio::test]
async fn a_second_begin_on_an_open_transaction_is_refused() {
    let (provider, fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;
    provider
        .begin_transaction(&handle, &TransactionOptions::default())
        .await
        .expect("first BEGIN");
    assert_eq!(fake.read(|wire| wire.begin_calls), 1);

    let outcome = provider
        .begin_transaction(&handle, &TransactionOptions::default())
        .await;
    assert!(matches!(
        outcome,
        Err(ResourceError::InvalidResourceState { .. })
    ));
    assert_eq!(
        fake.read(|wire| wire.begin_calls),
        1,
        "the refused BEGIN never went out"
    );
}

#[tokio::test]
async fn commit_and_rollback_reach_the_driver_and_report_what_they_did() {
    let (provider, fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;
    provider
        .begin_transaction(&handle, &TransactionOptions::default())
        .await
        .expect("BEGIN");

    let commit = provider
        .commit_transaction(&handle)
        .await
        .expect("COMMIT round-trips");
    assert_eq!(commit.state, TransactionState::Idle);
    assert_eq!(commit.effect, Some(EffectOutcome::Completed));
    assert_eq!(fake.read(|wire| wire.commit_calls), 1);

    provider
        .begin_transaction(&handle, &TransactionOptions::default())
        .await
        .expect("BEGIN again");
    let rollback = provider
        .rollback_transaction(&handle)
        .await
        .expect("ROLLBACK round-trips");
    assert_eq!(rollback.state, TransactionState::Idle);
    assert_eq!(rollback.effect, Some(EffectOutcome::RolledBack));
    assert_eq!(fake.read(|wire| wire.rollback_calls), 1);
}

#[tokio::test]
async fn a_failed_commit_reports_unknown_instead_of_a_guess() {
    let (provider, fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;
    provider
        .begin_transaction(&handle, &TransactionOptions::default())
        .await
        .expect("BEGIN");
    fake.with(|wire| wire.commit_fails = true);

    let commit = provider
        .commit_transaction(&handle)
        .await
        .expect("a failed COMMIT is reported, not raised");
    assert_eq!(commit.state, TransactionState::Unknown);
    assert_eq!(commit.effect, Some(EffectOutcome::Unknown));
}

#[tokio::test]
async fn committing_with_nothing_open_is_an_error_not_a_successful_no_op() {
    let (provider, _fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;
    let outcome = provider.commit_transaction(&handle).await;
    assert!(matches!(
        outcome,
        Err(ResourceError::InvalidResourceState { .. })
    ));
}

#[tokio::test]
async fn a_statement_inside_a_transaction_is_only_partially_applied() {
    let (provider, _fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;
    let sink = RecordingSink::default();
    let completion = provider
        .execute_on_resource(
            &handle,
            &QueryExecutionId::new("exec-1"),
            &query_call(),
            &sink,
        )
        .await
        .expect("round-trip");
    assert_eq!(
        completion.effect_outcome,
        EffectOutcome::Completed,
        "with nothing open the effects are durable"
    );

    provider
        .begin_transaction(&handle, &TransactionOptions::default())
        .await
        .expect("BEGIN");
    let inside = provider
        .execute_on_resource(
            &handle,
            &QueryExecutionId::new("exec-1"),
            &query_call(),
            &sink,
        )
        .await
        .expect("round-trip");
    assert_eq!(
        inside.effect_outcome,
        EffectOutcome::PartiallyApplied,
        "a rollback can still discard them"
    );
    assert_eq!(
        inside.transaction_observation.state,
        TransactionState::Active
    );
}

#[tokio::test]
async fn a_completion_reports_no_session_scoped_handles() {
    let (provider, _fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;
    let completion = provider
        .execute_on_resource(
            &handle,
            &QueryExecutionId::new("exec-1"),
            &query_call(),
            &RecordingSink::default(),
        )
        .await
        .expect("round-trip");
    assert!(
        completion.session_handles.is_empty(),
        "`session_scoped_handles` is Unsupported, so none may be reported"
    );
    // The hot path does not pay for a read-back.
    assert_eq!(completion.context_before, SessionContext::unobserved());
    assert_eq!(completion.context_after, SessionContext::unobserved());
}

#[tokio::test]
async fn reset_always_discards_because_a_pool_of_one_is_not_a_baseline() {
    let (provider, fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;

    let disposition = provider
        .reset_resource(&handle, &baseline())
        .await
        .expect("a refusal is still a disposition");

    assert_eq!(disposition, ResetDisposition::Discard);
    assert!(
        fake.read(|wire| wire.statements.is_empty()),
        "an unverified reset must not run statements"
    );
    // The resource stays usable: discard means "re-acquire", not "broken".
    assert!(provider.observe_session(&handle).await.is_ok());
}

#[tokio::test]
async fn closing_a_resource_with_an_open_transaction_is_not_a_clean_close() {
    let (provider, _fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;
    provider
        .begin_transaction(&handle, &TransactionOptions::default())
        .await
        .expect("BEGIN");

    let disposition = provider.close_resource(&handle).await.expect("close");
    assert_eq!(disposition, CloseDisposition::CloseUnconfirmed);
    assert_eq!(
        budget.released(),
        1,
        "the physical connection is genuinely gone, so the charge is settled"
    );
}

#[test]
fn every_migration_operation_declares_the_ddl_atomicity_the_driver_reports() {
    let capabilities = sqlite_capability_set();
    let keys = migration_operation_keys();
    assert_eq!(keys.len(), 33);
    for key in keys {
        assert_eq!(
            capabilities.ddl_atomicity.atomicity_for(key),
            datazen_driver_api::DdlAtomicity::Transactional,
            "`{key}` must match `SqliteDriver::ddl_atomicity`"
        );
    }
    // An unknown key must fail closed, never inherit the driver's value.
    assert_eq!(
        capabilities
            .ddl_atomicity
            .atomicity_for("not_a_migration_operation"),
        datazen_driver_api::DdlAtomicity::Unknown
    );
}

#[test]
fn the_transaction_declaration_refuses_savepoints_and_names_no_level() {
    let capabilities = sqlite_capability_set();
    assert!(
        capabilities.transactions.isolation_levels.is_empty(),
        "a bare BEGIN selects no level, so naming one would over-claim"
    );
    assert_eq!(
        capabilities.transactions.savepoints,
        Availability::Unsupported
    );
    assert_eq!(capabilities.transactions.max_open_transactions, Some(1));
}

fn query_call() -> CommandCall {
    CommandCall::new("query", serde_json::json!({ "sql": "SELECT 1" }))
}
