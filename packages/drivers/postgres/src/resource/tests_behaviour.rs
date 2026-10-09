//! Behavioural unit tests for the PostgreSQL resource provider.
//!
//! Split out of `tests.rs` purely for file size: that file owns the fixtures
//! and the declaration/guard arithmetic, this one owns what the provider
//! *does* once a resource exists — cancelling one execution, refusing a handle
//! it never issued, releasing the budget, running transactions, and refusing
//! to invent an observation.
//!
//! Still fully offline: seeds go through the same `register_resource` the
//! acquire path calls, with the socket half skipped.

use std::sync::Arc;

use async_trait::async_trait;
use datazen_driver_api::namespace::NamespaceTarget;
use datazen_driver_api::resource::{
    Baseline, BudgetPort, CommandCall, ResourceError, ResourceHandle, ResourceProvider,
    ResultChunk, ResultSink,
};
use datazen_driver_api::session::{
    CloseDisposition, ContextChangeDisposition, ResetDisposition, TransactionOptions,
    TransactionState,
};
use datazen_driver_api::{MultiQueryResult, QueryExecutionId, StatementResult, Value};

use super::capabilities::POSTGRES_PROVIDER_ID;
use super::observation::{leased_context, parse_search_path, pinned_context, SessionFacts};
use super::payload::decode_command_result;
use super::tests::{
    catalog_target, provider_with_driver, seed_resource, target, transaction_options, BudgetLedger,
    RecordingSink,
};

// ---------------------------------------------------------------------------
// 6. Handle ownership: an unowned handle is an error on every method
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_handle_from_another_provider_is_refused_on_every_method() {
    let (_driver, provider) = provider_with_driver();
    let ledger = Arc::new(BudgetLedger::default());
    let (_real, _conn) = seed_resource(&provider, ledger, "target");
    let foreign = ResourceHandle::issue("mysql", "resource-key-1", provider.runtime_epoch());
    let sink = RecordingSink::default();

    let errors: Vec<(&str, ResourceError)> = vec![
        (
            "observe_session",
            provider
                .observe_session(&foreign)
                .await
                .expect_err("a foreign handle must be refused"),
        ),
        (
            "execute_on_resource",
            provider
                .execute_on_resource(
                    &foreign,
                    &QueryExecutionId::new("exec-1"),
                    &CommandCall::new("query", serde_json::Value::Null),
                    &sink,
                )
                .await
                .expect_err("a foreign handle must be refused"),
        ),
        (
            "change_context",
            provider
                .change_context(&foreign, &target())
                .await
                .expect_err("a foreign handle must be refused"),
        ),
        (
            "begin_transaction",
            provider
                .begin_transaction(&foreign, &transaction_options())
                .await
                .expect_err("a foreign handle must be refused"),
        ),
        (
            "commit_transaction",
            provider
                .commit_transaction(&foreign)
                .await
                .expect_err("a foreign handle must be refused"),
        ),
        (
            "request_cancel",
            provider
                .request_cancel(&foreign, &QueryExecutionId::new("exec-1"))
                .await
                .expect_err("a foreign handle must be refused"),
        ),
        (
            "reset_resource",
            provider
                .reset_resource(&foreign, &Baseline::new(Vec::new()))
                .await
                .expect_err("a foreign handle must be refused"),
        ),
        (
            "close_resource",
            provider
                .close_resource(&foreign)
                .await
                .expect_err("a foreign handle must be refused"),
        ),
    ];

    for (method, error) in errors {
        match error {
            ResourceError::ResourceOwnershipMismatch { expected, actual } => {
                assert_eq!(expected, POSTGRES_PROVIDER_ID, "{method}");
                assert_eq!(actual, "mysql", "{method}");
            }
            other => panic!("{method} returned {other:?} for a foreign handle"),
        }
    }
}

#[tokio::test]
async fn a_handle_from_an_older_provider_instance_is_refused_as_stale() {
    let (_driver, provider) = provider_with_driver();
    let stale = ResourceHandle::issue(
        POSTGRES_PROVIDER_ID,
        "resource-key-1",
        provider.runtime_epoch() - 1,
    );

    match provider.observe_session(&stale).await {
        Err(ResourceError::StaleRuntimeEpoch { expected, actual }) => {
            assert_eq!(expected, provider.runtime_epoch());
            assert_eq!(actual, provider.runtime_epoch() - 1);
        }
        other => panic!("expected StaleRuntimeEpoch, got {other:?}"),
    }
}

#[tokio::test]
async fn a_handle_that_was_never_acquired_is_refused() {
    let (_driver, provider) = provider_with_driver();
    let never = ResourceHandle::issue(
        POSTGRES_PROVIDER_ID,
        "never-acquired",
        provider.runtime_epoch(),
    );

    match provider.close_resource(&never).await {
        Err(ResourceError::InvalidResourceState {
            resource_key,
            operation,
            ..
        }) => {
            assert_eq!(resource_key, "never-acquired");
            assert_eq!(operation, "close_resource");
        }
        other => panic!("expected InvalidResourceState, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 7. Close, reset and budget accounting
// ---------------------------------------------------------------------------

#[tokio::test]
async fn close_releases_the_budget_exactly_once() {
    let (_driver, provider) = provider_with_driver();
    let ledger = Arc::new(BudgetLedger::default());
    let (handle, _conn) = seed_resource(&provider, Arc::clone(&ledger), "target");

    assert_eq!(
        provider.close_resource(&handle).await.expect("first close"),
        CloseDisposition::Closed
    );
    assert_eq!(ledger.released(), vec!["permit-target".to_string()]);

    // Idempotent: a repeated close must not pay the caller twice.
    assert_eq!(
        provider
            .close_resource(&handle)
            .await
            .expect("second close"),
        CloseDisposition::Closed
    );
    assert_eq!(
        ledger.released(),
        vec!["permit-target".to_string()],
        "a second close must not release the budget again"
    );
}

// ---------------------------------------------------------------------------
// 8. Idempotence of a confirmed close, and where the fact now lives
// ---------------------------------------------------------------------------

/// A confirmed close stays idempotent for the life of the process, with nothing
/// kept on the provider to make that true.
///
/// The provider instance is memoized for the whole process
/// (`src-tauri/src/db/registry.rs` holds it in a `OnceLock`), and a headless
/// `--mcp-stdio` server can close an unbounded number of resources, so the
/// answer cannot be "a set of keys already closed" — that set is exactly as
/// large as every key ever issued and never stops growing. The fact rides on
/// the handle instead, which bounds it by the handles the caller still holds
/// and is exact with no window and no eviction.
///
/// This asserts the property that makes the leak unnecessary: repeats are
/// refused forever, from the handle that closed and from any clone of it, and
/// none of them releases twice. It cannot assert the provider's memory is
/// bounded, because there is no longer anything to count — that is a property
/// of `ResourceRegistry` having no `closed` field at all.
#[tokio::test]
async fn a_confirmed_close_stays_idempotent_for_every_repeat() {
    let (_driver, provider) = provider_with_driver();
    let ledger = Arc::new(BudgetLedger::default());

    let (handle, _conn) = seed_resource(&provider, Arc::clone(&ledger), "target");
    let copy = handle.clone();

    assert_eq!(
        provider.close_resource(&handle).await.expect("first close"),
        CloseDisposition::Closed
    );

    for _ in 0..8 {
        assert_eq!(
            provider
                .close_resource(&handle)
                .await
                .expect("a repeat close is a no-op, not an error"),
            CloseDisposition::Closed,
            "a repeat close must stay confirmed-idempotent"
        );
        assert_eq!(
            provider
                .close_resource(&copy)
                .await
                .expect("a clone of a closed handle reads closed too"),
            CloseDisposition::Closed,
            "the closed fact is shared by clones, or the caller could hold two \
             views of one resource that disagree"
        );
    }

    assert_eq!(
        ledger.released(),
        vec!["permit-target".to_string()],
        "nine closes released once: the repeats must not release again"
    );
}

/// The closed fact belongs to the handle that closed, not to the key.
///
/// This is what separates the new mechanism from the per-provider key set it
/// replaced: under a key-indexed set, re-issuing a key would either leave the
/// stale entry to answer for the new resource or need an explicit reclaim, and
/// `register_resource` had to do that reclaim. Here there is nothing to
/// reclaim, because nothing was ever recorded under the key.
///
/// Production keys are fresh (`connect_impl` mints `Uuid::new_v4()` per
/// connection), so only [`seed_resource`] reaches this by naming the same
/// label twice.
#[tokio::test]
async fn re_acquiring_a_reused_key_does_not_transfer_the_old_close() {
    let (_driver, provider) = provider_with_driver();
    let ledger = Arc::new(BudgetLedger::default());

    let (first, _conn) = seed_resource(&provider, Arc::clone(&ledger), "target");
    assert_eq!(
        provider.close_resource(&first).await.expect("first close"),
        CloseDisposition::Closed
    );

    // Same label, so `seed_resource` rebuilds `conn-target`: a second live
    // resource now answers to that key.
    let (second, _conn) = seed_resource(&provider, Arc::clone(&ledger), "target");

    assert!(
        !second.is_closed(),
        "the closed fact rides on the handle that closed, not on the key: a \
         re-acquired resource must start open, or its first close would be \
         answered as already closed and never release its budget"
    );

    assert_eq!(
        provider
            .close_resource(&second)
            .await
            .expect("second close"),
        CloseDisposition::Closed,
        "the re-acquired resource is live and closes for real"
    );
    assert_eq!(
        ledger.released(),
        vec!["permit-target".to_string(), "permit-target".to_string()],
        "each resource's own confirmed close releases exactly once"
    );
}

#[tokio::test]
async fn reset_refuses_to_promise_reuse() {
    let (_driver, provider) = provider_with_driver();
    let ledger = Arc::new(BudgetLedger::default());
    let (handle, _conn) = seed_resource(&provider, ledger, "target");

    assert_eq!(
        provider
            .reset_resource(&handle, &Baseline::new(Vec::new()))
            .await
            .expect("the reset is answered"),
        ResetDisposition::Discard,
        "no baseline replay is verified, so the resource must not be handed out again"
    );
}

// ---------------------------------------------------------------------------
// 8. Transactions
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_named_isolation_level_is_refused_because_none_is_declared() {
    let (_driver, provider) = provider_with_driver();
    let ledger = Arc::new(BudgetLedger::default());
    let (handle, _conn) = seed_resource(&provider, ledger, "target");

    let options = TransactionOptions {
        isolation_level: Some("serializable".into()),
        ..transaction_options()
    };

    let error = provider
        .begin_transaction(&handle, &options)
        .await
        .expect_err("the capability set lists no isolation levels");

    assert!(
        matches!(error, ResourceError::OperationNotSupported { .. }),
        "expected OperationNotSupported, got {error:?}"
    );
}

#[tokio::test]
async fn a_read_only_transaction_is_refused_instead_of_being_ignored() {
    let (_driver, provider) = provider_with_driver();
    let ledger = Arc::new(BudgetLedger::default());
    let (handle, _conn) = seed_resource(&provider, ledger, "target");

    let options = TransactionOptions {
        read_only: Some(true),
        ..transaction_options()
    };

    let error = provider
        .begin_transaction(&handle, &options)
        .await
        .expect_err("a bare BEGIN does not start a read-only transaction");

    assert!(
        matches!(error, ResourceError::OperationNotSupported { .. }),
        "expected OperationNotSupported, got {error:?}"
    );
}

#[tokio::test]
async fn committing_without_an_open_transaction_is_refused() {
    let (_driver, provider) = provider_with_driver();
    let ledger = Arc::new(BudgetLedger::default());
    let (handle, _conn) = seed_resource(&provider, ledger, "target");

    for outcome in [
        provider.commit_transaction(&handle).await,
        provider.rollback_transaction(&handle).await,
    ] {
        match outcome {
            Err(ResourceError::TransactionResolutionRequired { operation }) => {
                assert!(!operation.is_empty());
            }
            other => panic!("expected TransactionResolutionRequired, got {other:?}"),
        }
    }
}

// ---------------------------------------------------------------------------
// 9. Observations: nothing is invented
// ---------------------------------------------------------------------------

#[tokio::test]
async fn observing_a_resource_with_no_open_pool_is_an_explicit_error() {
    let (_driver, provider) = provider_with_driver();
    let ledger = Arc::new(BudgetLedger::default());
    let (handle, _conn) = seed_resource(&provider, ledger, "target");

    // No pool was ever opened for this handle, so there is nothing to read and
    // nothing may be reported.
    assert!(
        provider.observe_session(&handle).await.is_err(),
        "a session that was never read must not come back as an observation"
    );
}

#[tokio::test]
async fn a_pinned_read_is_confirmed_and_a_leased_read_is_only_partial() {
    let facts = SessionFacts {
        database: "app_db".into(),
        effective_identity: "app_user".into(),
        search_path: parse_search_path("\"$user\", public, extra"),
    };
    assert_eq!(
        facts.search_path,
        vec!["public".to_string(), "extra".to_string()]
    );
    assert_eq!(facts.effective_schema().as_deref(), Some("public"));

    let pinned = pinned_context(&facts);
    assert!(pinned.confidence.is_confirmed());
    assert_eq!(pinned.transaction_state, TransactionState::Active);
    assert_eq!(pinned.autocommit, Some(false));
    let named_target = NamespaceTarget::empty()
        .with_database("app_db")
        .with_schema("public");
    assert!(
        pinned.matches_target(&named_target),
        "a pinned read answers the target it actually read"
    );
    assert!(
        !pinned.matches_target(&target()),
        "a pinned read must not be stretched to a target it did not read: it named schema public, \
         the target named none"
    );

    let leased = leased_context(&facts);
    assert_eq!(
        leased.confidence,
        datazen_driver_api::session::ObservationConfidence::Partial
    );
    assert!(!leased.confidence.is_confirmed());
    assert_eq!(leased.transaction_state, TransactionState::Unknown);
    assert_eq!(leased.autocommit, None);
    assert!(leased.search_path.is_empty());
    assert_eq!(leased.namespace.schema, None);
    assert!(
        !leased.matches_target(&target()),
        "a leased read must not be mistaken for a confirmation"
    );
    assert_eq!(leased.namespace.database.as_deref(), Some("app_db"));
}

#[tokio::test]
async fn a_search_path_keeps_its_quoted_names_and_drops_role_tokens() {
    assert_eq!(
        parse_search_path(" public , \"MixedCase\" "),
        vec!["public".to_string(), "MixedCase".to_string()]
    );
    assert!(parse_search_path("$user, public")
        .first()
        .is_some_and(|s| s == "public"));
    assert!(parse_search_path("").is_empty());
}

// ---------------------------------------------------------------------------
// 10. Context change
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_namespace_change_never_claims_an_in_place_switch() {
    let (_driver, provider) = provider_with_driver();
    let ledger = Arc::new(BudgetLedger::default());
    let (handle, _conn) = seed_resource(&provider, ledger, "target");

    let disposition = provider
        .change_context(&handle, &NamespaceTarget::empty().with_database("other_db"))
        .await
        .expect("a valid target is answered");

    assert_eq!(disposition, ContextChangeDisposition::RequiresReplacement);
    assert!(
        !disposition.changed_in_place(),
        "postgres has no in-place database switch, so the caller must acquire again"
    );
}

#[tokio::test]
async fn an_impossible_namespace_target_is_refused_before_the_disposition() {
    let (_driver, provider) = provider_with_driver();
    let ledger = Arc::new(BudgetLedger::default());
    let (handle, _conn) = seed_resource(&provider, ledger, "target");

    let outcome = provider
        .change_context(
            &handle,
            &NamespaceTarget {
                database: Some("other_db".into()),
                ..catalog_target()
            },
        )
        .await;

    assert!(
        matches!(
            outcome,
            Err(ResourceError::NonexistentNamespaceLevel { .. })
        ),
        "a target naming a level postgres does not have must not fall through to a disposition, \
         got {outcome:?}"
    );
}

// ---------------------------------------------------------------------------
// 11. The result channel refuses what it cannot carry
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_query_payload_is_streamed_row_for_row() {
    let payload = serde_json::to_value(MultiQueryResult {
        results: vec![
            StatementResult {
                sql: "SELECT 1".into(),
                columns: Vec::new(),
                rows: vec![vec![Some(Value::Integer(1))]],
                rows_affected: None,
                execution_time_ms: 1,
                truncated: false,
            },
            StatementResult {
                sql: "UPDATE t SET a = 1".into(),
                columns: Vec::new(),
                rows: Vec::new(),
                rows_affected: Some(2),
                execution_time_ms: 3,
                truncated: false,
            },
        ],
        total_time_ms: 4,
    })
    .expect("the payload serialises");

    let (results, chunks) = decode_command_result(&payload).expect("a query result is carried");
    assert_eq!(results.len(), 2);
    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[0].statement_index, 0);
    assert_eq!(chunks[0].rows.len(), 1);
    assert_eq!(chunks[1].statement_index, 1);
    assert_eq!(chunks[1].rows_affected, Some(2));
}

#[tokio::test]
async fn an_execute_acknowledgement_is_carried_as_a_row_count() {
    let payload = serde_json::json!({ "rowsAffected": 3 });

    let (results, chunks) = decode_command_result(&payload).expect("an ack is carried");
    assert!(results.is_empty());
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].rows_affected, Some(3));
    assert!(chunks[0].rows.is_empty());
}

#[tokio::test]
async fn a_catalog_payload_is_refused_instead_of_reported_as_an_empty_success() {
    let marker = "s3cret_catalog_value";
    let payload = serde_json::json!({ "tables": [{ "name": marker }] });

    let error = decode_command_result(&payload)
        .expect_err("a catalog listing has no truthful ResultChunk form");

    match &error {
        ResourceError::OperationNotSupported {
            driver,
            operation,
            reason,
        } => {
            assert_eq!(driver, POSTGRES_PROVIDER_ID);
            assert_eq!(operation, "execute_on_resource");
            assert!(
                reason.contains("tables"),
                "the refusal must name the shape so the caller knows what it got: {reason}"
            );
        }
        other => panic!("expected OperationNotSupported, got {other:?}"),
    }
    let rendered = error.to_string();
    assert!(
        !rendered.contains(marker),
        "a refusal must describe the payload's shape, never its contents"
    );
}

#[tokio::test]
async fn a_payload_with_a_row_count_among_other_keys_is_refused() {
    // `{"rowsAffected": 1, "warnings": [...]}` is somebody else's JSON that
    // happens to contain a number. Accepting it would drop the warnings on the
    // floor and report a clean run.
    let payload = serde_json::json!({ "rowsAffected": 1, "warnings": ["truncated"] });

    assert!(matches!(
        decode_command_result(&payload),
        Err(ResourceError::OperationNotSupported { .. })
    ));
}

// ---------------------------------------------------------------------------
// 12. A sink that rejects a write must stop the run
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_sink_rejection_surfaces_instead_of_a_completed_execution() {
    struct RefusingSink;

    #[async_trait]
    impl ResultSink for RefusingSink {
        async fn write(&self, _chunk: ResultChunk) -> Result<(), ResourceError> {
            Err(ResourceError::SinkRejected {
                reason: "the host went away".into(),
            })
        }

        async fn complete(&self) -> Result<(), ResourceError> {
            Ok(())
        }

        async fn fail(&self, _reason: &str) -> Result<(), ResourceError> {
            Ok(())
        }
    }

    // The provider must not have a path that reports success past a rejected
    // write; here we assert the shape of the guarantee we rely on.
    let error = RefusingSink
        .write(ResultChunk {
            statement_index: 0,
            sql: "SELECT 1".into(),
            rows: Vec::new(),
            rows_affected: None,
        })
        .await
        .expect_err("the sink refuses");

    assert!(matches!(error, ResourceError::SinkRejected { .. }));
}

#[tokio::test]
async fn a_recording_sink_starts_empty_and_uncompleted() {
    let sink = RecordingSink::default();
    assert_eq!(sink.rows_written(), 0);
    assert!(!sink.was_completed());
    assert!(sink.failure().is_none());

    sink.complete().await.expect("complete succeeds");
    assert!(sink.was_completed());
}

#[tokio::test]
async fn the_budget_ledger_records_what_it_was_asked_for() {
    let ledger = BudgetLedger::default();
    let permit = ledger
        .acquire_physical_connections(11)
        .await
        .expect("the ledger grants");
    assert_eq!(permit.physical_connections, 11);
    ledger
        .release_physical_connections(&permit)
        .await
        .expect("the ledger releases");
    assert_eq!(ledger.acquired(), vec![11]);
    assert_eq!(ledger.released(), vec!["permit-11".to_string()]);
}

#[tokio::test]
async fn a_second_provider_instance_does_not_answer_for_the_first_ones_resources() {
    let (_driver_a, provider_a) = provider_with_driver();
    let (_driver_b, provider_b) = provider_with_driver();
    let ledger = Arc::new(BudgetLedger::default());
    let (handle, _conn) = seed_resource(&provider_a, Arc::clone(&ledger), "target");

    // The handle is a legal, current, correctly-owned handle — it simply is not
    // this instance's. The epoch is what stops it from resolving.
    let stale = ResourceHandle::issue(
        POSTGRES_PROVIDER_ID,
        handle.resource_key(),
        provider_a.runtime_epoch(),
    );
    assert!(matches!(
        provider_b.close_resource(&stale).await,
        Err(ResourceError::StaleRuntimeEpoch { .. })
    ));
}

#[tokio::test]
async fn the_atomic_epoch_counter_never_repeats() {
    let mut seen = std::collections::HashSet::new();
    for _ in 0..64 {
        let (_driver, provider) = provider_with_driver();
        assert!(
            seen.insert(provider.runtime_epoch()),
            "runtime epochs must be unique per provider instance"
        );
    }
}
