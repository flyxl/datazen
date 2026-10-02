//! Refusal half of the mongodb provider contract: session observation, context
//! change, transactions, reset and close.
//!
//! A MongoDB client carries no session state, so every method here must answer
//! "I cannot know" or "I will not do that" rather than invent an observation.
//! Each test names the honesty rule it defends.

use super::test_support::*;

#[tokio::test]
async fn observing_a_client_reports_unknown_rather_than_a_guessed_context() {
    let (provider, _fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;

    let observation = provider
        .observe_session(&handle)
        .await
        .expect("the handle is ours, so the method may answer");

    assert_eq!(observation.state, SessionState::Unknown);
    assert_eq!(
        observation.context.confidence,
        ObservationConfidence::Unknown,
        "§2.1 declares `contextObservation` Unsupported; anything stronger is a fabrication"
    );
    assert_eq!(
        observation.transaction.state,
        datazen_driver_api::session::TransactionState::Unsupported
    );
    assert!(
        observation.handles.is_empty(),
        "the provider issues none, so it must not report any"
    );
}

#[tokio::test]
async fn every_observation_advances_its_own_revision() {
    let (provider, _fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;

    let first = provider.observe_session(&handle).await.unwrap();
    let second = provider.observe_session(&handle).await.unwrap();

    assert!(
        second.context_revision > first.context_revision,
        "a repeated read is a new observation, not a cached one"
    );
}

#[tokio::test]
async fn changing_context_is_refused_because_the_database_lives_on_the_statement() {
    let (provider, _fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;

    let disposition = provider
        .change_context(&handle, &NamespaceTarget::empty())
        .await
        .expect("a refusal is a disposition, not an error");

    assert_eq!(
        disposition,
        ContextChangeDisposition::Unsupported,
        "`mongodb.rs::resolve_database` takes the database from the command and never mutates pool state; there is nothing to switch"
    );
}

#[tokio::test]
async fn no_transaction_entry_point_reports_success_without_a_transaction() {
    let (provider, fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;

    for (operation, reason) in [
        ("begin_transaction", "begin_transaction"),
        ("commit_transaction", "commit_transaction"),
        ("rollback_transaction", "rollback_transaction"),
    ] {
        let outcome = match operation {
            "begin_transaction" => provider
                .begin_transaction(
                    &handle,
                    &TransactionOptions {
                        isolation_level: None,
                        read_only: None,
                        defers_commit: false,
                    },
                )
                .await
                .map(|observation| format!("{observation:?}")),
            "commit_transaction" => provider
                .commit_transaction(&handle)
                .await
                .map(|observation| format!("{observation:?}")),
            _ => provider
                .rollback_transaction(&handle)
                .await
                .map(|observation| format!("{observation:?}")),
        };
        match outcome {
            Err(ResourceError::OperationNotSupported {
                operation: named, ..
            }) => assert_eq!(named, reason),
            other => panic!("`{operation}` must refuse, got {other:?}"),
        }
    }

    assert_eq!(
        fake.read(|wire| wire.begin_calls),
        0,
        "the refusal must happen in the provider, before the driver is asked to fake one"
    );
}

#[tokio::test]
async fn a_named_isolation_level_is_refused_before_the_driver_is_reached() {
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
                reason.contains("session"),
                "the reason must say what is actually missing: {reason}"
            );
        }
        other => panic!("expected an explicit refusal, got {other:?}"),
    }
    assert_eq!(fake.read(|wire| wire.begin_calls), 0);
}

#[tokio::test]
async fn the_completion_of_a_write_claims_no_transaction_outcome() {
    let (provider, _fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;

    let completion = provider
        .execute_on_resource(
            &handle,
            &QueryExecutionId::new("exec-1"),
            &CommandCall::new(
                "execute",
                serde_json::json!({
                    "sql": r#"{"collection": "orders"}"#,
                    "database": "billing",
                }),
            ),
            &RecordingSink::default(),
        )
        .await
        .expect("the write must round-trip");

    assert_eq!(
        completion.effect_outcome,
        datazen_driver_api::session::EffectOutcome::Unknown,
        "the provider opened no transaction that could roll the write back; `Completed` would claim an outcome nobody observed"
    );
    assert_eq!(
        completion.transaction_observation.state,
        datazen_driver_api::session::TransactionState::Unknown
    );
    assert!(completion.session_handles.is_empty());
}

#[tokio::test]
async fn reset_always_discards_and_never_claims_the_baseline_was_restored() {
    let (provider, _fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;

    let disposition = provider
        .reset_resource(&handle, &baseline())
        .await
        .expect("`Discard` is a disposition, not an error");

    assert_eq!(
        disposition,
        ResetDisposition::Discard,
        "§6.1: a driver with no verified path back to its baseline must not claim `restored`"
    );
}

#[tokio::test]
async fn a_close_after_statements_is_clean_because_no_transaction_was_opened() {
    let (provider, fake, budget) = provider();
    let handle = acquire(&provider, &budget).await;

    provider
        .execute_on_resource(
            &handle,
            &QueryExecutionId::new("exec-1"),
            &CommandCall::new(
                "query",
                serde_json::json!({ "sql": r#"{"collection": "orders"}"# }),
            ),
            &RecordingSink::default(),
        )
        .await
        .expect("the query must round-trip");

    let disposition = provider
        .close_resource(&handle)
        .await
        .expect("close must succeed");

    assert_eq!(disposition, CloseDisposition::Closed);
    assert_eq!(fake.read(|wire| wire.disconnect_calls), 1);
    assert_eq!(budget.released(), 1);
}
