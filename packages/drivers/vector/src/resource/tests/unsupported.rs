//! The half of the contract this driver must refuse.
//!
//! Qdrant is a search server: there is no transaction endpoint, no session
//! state, and no server-side handle a stream could hold open. Every method
//! below either returns an `Err` or an explicit `Unsupported` value carrying a
//! reason — and the tests here are the ones that would catch the classic
//! migration failure where a refusal turns into an empty `Ok(())` that a
//! caller reads as success.

use std::sync::Arc;

use datazen_driver_api::namespace::{NamespaceLevelKind, NamespaceTarget};
use datazen_driver_api::resource::{Baseline, ResourceError, ResourceHandle, ResourceProvider};
use datazen_driver_api::session::{
    CancelDisposition, ContextChangeDisposition, ResetDisposition, TransactionOptions,
};
use datazen_driver_api::{ConnectionHandle, DatabaseDriver, DriverError};

use super::support::{
    acquire_request, config, execution_id, provider, refusal_reason, scope, unacquired_handle,
    BudgetLedger,
};
use crate::resource::payload::decode_command_result;
use crate::vector::VectorDriver;

async fn opened() -> (crate::resource::VectorResourceProvider, ResourceHandle) {
    let provider = provider();
    let budget = Arc::new(BudgetLedger::default());
    let handle = provider
        .acquire_resource(&acquire_request(&config(), scope()), &budget.as_port())
        .await
        .expect("the client registers without a round trip");
    (provider, handle)
}

#[tokio::test]
async fn no_transaction_operation_is_ever_reported_as_succeeded() {
    let (provider, handle) = opened().await;
    let options = TransactionOptions::default();

    for (operation, error) in [
        (
            "begin",
            provider
                .begin_transaction(&handle, &options)
                .await
                .expect_err("there is no transaction to begin"),
        ),
        (
            "commit",
            provider
                .commit_transaction(&handle)
                .await
                .expect_err("there is no transaction to commit"),
        ),
        (
            "rollback",
            provider
                .rollback_transaction(&handle)
                .await
                .expect_err("there is no transaction to roll back"),
        ),
    ] {
        match &error {
            ResourceError::OperationNotSupported {
                driver,
                operation: reported,
                ..
            } => {
                assert_eq!(driver, "vector");
                assert!(
                    reported.starts_with(operation),
                    "{reported} names the wrong call"
                );
            }
            other => panic!("{operation} must refuse, not succeed: {other:?}"),
        }
        let reason = refusal_reason(&error);
        assert!(
            reason.to_lowercase().contains("no transaction"),
            "the {operation} refusal must say why: {reason}"
        );
        assert!(
            reason.contains("VectorDriver::execute"),
            "the {operation} refusal must point at the code that proves it: {reason}"
        );
    }
}

#[tokio::test]
async fn a_transaction_call_on_a_foreign_handle_is_refused_as_stale_first() {
    let provider = provider();
    let handle = unacquired_handle(&provider, "vector_resource:never-opened");

    // The ownership check runs before the refusal, so a caller cannot probe
    // another instance's resource by asking for a transaction on it.
    let error = provider
        .begin_transaction(&handle, &TransactionOptions::default())
        .await
        .expect_err("no key here was ever opened");
    assert!(matches!(error, ResourceError::InvalidResourceState { .. }));
}

#[tokio::test]
async fn changing_context_is_unsupported_because_there_is_no_second_namespace() {
    let (provider, handle) = opened().await;

    let disposition = provider
        .change_context(&handle, &NamespaceTarget::empty())
        .await
        .expect("an empty target is valid, just not switchable");
    assert!(matches!(disposition, ContextChangeDisposition::Unsupported));

    // A level this driver does not have is refused as invalid, which is more
    // informative than calling the switch unsupported.
    let error = provider
        .change_context(
            &handle,
            &NamespaceTarget {
                schema: Some("public".into()),
                ..NamespaceTarget::empty()
            },
        )
        .await
        .expect_err("Qdrant has no schema level to switch to");
    assert!(matches!(
        error,
        ResourceError::NonexistentNamespaceLevel {
            kind: NamespaceLevelKind::Schema
        }
    ));
}

#[tokio::test]
async fn cancelling_reports_unsupported_and_claims_no_execution_state() {
    let (provider, handle) = opened().await;
    let execution = execution_id();

    let receipt = provider
        .request_cancel(&handle, &execution)
        .await
        .expect("an unsupported cancel is an answer, not an error");
    assert_eq!(receipt.execution_id, execution);
    assert_eq!(receipt.disposition, CancelDisposition::Unsupported);
    assert!(
        receipt.state.is_none(),
        "a cancel that did nothing must not report a state"
    );
}

#[tokio::test]
async fn resetting_reports_discard_and_keeps_the_charge_until_close() {
    let provider = provider();
    let budget = Arc::new(BudgetLedger::default());
    let handle = provider
        .acquire_resource(&acquire_request(&config(), scope()), &budget.as_port())
        .await
        .expect("the client registers without a round trip");

    assert_eq!(
        provider
            .reset_resource(&handle, &Baseline::default())
            .await
            .expect("a reset is answered"),
        ResetDisposition::Discard,
        "nothing was replayed, so nothing can be called clean"
    );
    // The resource keeps its record: it still owns a permit, and forgetting it
    // here would strand that charge with no way to release it.
    assert!(
        budget.released().is_empty(),
        "a reset does not refund an open resource"
    );
    provider
        .close_resource(&handle)
        .await
        .expect("the resource is still closeable");
    assert_eq!(budget.released(), vec!["1".to_string()]);
}

#[tokio::test]
async fn a_writes_command_is_refused_by_the_driver_itself() {
    // The provider's whole reason for declaring no transaction capability: this
    // driver will not write. If that ever changes, the capability table and the
    // transaction refusals above must change with it.
    let driver = VectorDriver::new();
    let error = driver
        .execute(
            &ConnectionHandle {
                id: "probe".into(),
                pool_id: "probe".into(),
            },
            "delete all",
        )
        .await
        .expect_err("writes are refused before they reach the wire");
    assert!(matches!(error, DriverError::QueryFailed(_)));
}

// ---------------------------------------------------------------------------
// Payload decoding
// ---------------------------------------------------------------------------

#[test]
fn a_multi_statement_result_is_split_into_statements_and_rows() {
    let payload = serde_json::json!({
        "results": [
            { "sql": "{\"collection\":\"books\"}",
              "columns": [{ "name": "id", "dataType": "string", "nullable": false }],
              "rows": [["1"], ["2"]],
              "rowsAffected": null,
              "executionTimeMs": 3 },
            { "sql": "{\"collection\":\"authors\"}",
              "columns": [],
              "rows": [],
              "rowsAffected": 0,
              "executionTimeMs": 0 }
        ],
        "totalTimeMs": 5
    });
    let (statements, chunks) =
        decode_command_result(&payload).expect("a multi-result payload is a valid answer");
    assert_eq!(statements.len(), 2);
    // One chunk per statement, indexed by position, so a caller reading the
    // sink can line rows up with the statements it is told about.
    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[0].statement_index, 0);
    assert_eq!(chunks[0].rows.len(), 2);
    assert_eq!(chunks[1].statement_index, 1);
    assert!(chunks[1].rows.is_empty());
    assert_eq!(chunks[0].sql, statements[0].sql);
}

#[test]
fn a_writes_result_is_reported_without_inventing_rows() {
    let payload = serde_json::json!({ "rowsAffected": 7 });
    let (statements, chunks) =
        decode_command_result(&payload).expect("a row count is a valid answer");
    assert!(
        statements.is_empty(),
        "a row count is not a statement result and must not be reported as one"
    );
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].rows_affected, Some(7));
    assert!(
        chunks[0].rows.is_empty(),
        "a write produced no rows, and none may be invented for it"
    );
}

#[test]
fn a_payload_that_only_mentions_a_row_count_is_not_that_payload() {
    // The `execute` shape is exactly one key. Anything wider is somebody
    // else's payload, and reading a row count out of it would be a guess.
    let payload = serde_json::json!({ "rowsAffected": 7, "warnings": [] });
    let error = decode_command_result(&payload)
        .expect_err("a two-key object is not the single row-count shape");
    let reason = refusal_reason(&error);
    assert!(
        reason.contains("2 key(s)"),
        "the refusal must describe what arrived: {reason}"
    );
}

#[test]
fn a_payload_that_is_neither_shape_is_refused_with_its_keys_named() {
    let payload = serde_json::json!({ "totalTimeMs": 12 });
    let error = decode_command_result(&payload)
        .expect_err("an unrecognised payload must not decode to an empty success");
    let reason = refusal_reason(&error);
    assert!(
        reason.contains("totalTimeMs"),
        "the refusal must name what it received: {reason}"
    );
    assert!(
        !reason.contains("api-key"),
        "a refusal must not echo the command"
    );
}

#[test]
fn an_empty_result_set_still_terminates_with_a_statement() {
    let payload = serde_json::json!({
        "results": [{ "sql": "{\"collection\":\"books\"}",
                      "columns": [{ "name": "id", "dataType": "string", "nullable": false }],
                      "rows": [],
                      "rowsAffected": null,
                      "executionTimeMs": 0 }],
        "totalTimeMs": 0
    });
    let (statements, chunks) = decode_command_result(&payload).expect("no rows is still a result");
    assert_eq!(statements.len(), 1);
    assert_eq!(
        chunks.len(),
        1,
        "an empty read still produces a chunk to complete"
    );
    assert!(chunks.iter().all(|chunk| chunk.rows.is_empty()));
}

#[test]
fn an_array_payload_is_refused_by_its_length() {
    let error = decode_command_result(&serde_json::json!([{ "id": 1 }, { "id": 2 }]))
        .expect_err("an array is not a shape any command of this driver returns");
    let reason = refusal_reason(&error);
    assert!(
        reason.contains("array of 2 item(s)"),
        "the refusal must describe what arrived: {reason}"
    );
}
