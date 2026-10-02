//! The half of the contract this driver must refuse.
//!
//! Stargate is a REST gateway over HBase: it exposes no transaction endpoint,
//! no session state, and no namespace level. Every method below either returns
//! an `Err` or an explicit `Unsupported` value carrying a reason — and the tests
//! here are the ones that would catch the classic migration failure where a
//! refusal turns into an empty `Ok(())` that a caller reads as success.

use std::sync::Arc;

use datazen_driver_api::namespace::{NamespaceLevelKind, NamespaceTarget};
use datazen_driver_api::resource::{Baseline, ResourceError, ResourceHandle, ResourceProvider};
use datazen_driver_api::session::{
    CancelDisposition, ContextChangeDisposition, ResetDisposition, TransactionOptions,
};
use datazen_driver_api::{ConnectionHandle, DatabaseDriver, DriverError};

use crate::hbase::HBaseDriver;
use crate::resource::payload::decode_command_result;
use crate::resource::HBaseResourceProvider;

use super::support::{
    acquire_request, baseline, config_for, execution_id, provider, refusal_reason, scope,
    stargate_answering, unacquired_handle, BudgetLedger,
};

async fn opened() -> (HBaseResourceProvider, ResourceHandle) {
    let server = stargate_answering().await;
    let provider = provider();
    let budget = Arc::new(BudgetLedger::default());
    let handle = provider
        .acquire_resource(&acquire_request(&config_for(&server)), &budget.as_port())
        .await
        .expect("the stub answers, so the resource opens");
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
                reason,
            } => {
                assert_eq!(driver.as_str(), "hbase");
                assert_eq!(reported.as_str(), format!("{operation}_transaction"));
                // The refusal has to name the *surface* that cannot do it, or a
                // caller that knows HBase has transactions will treat this as a
                // driver bug rather than a boundary.
                assert!(
                    reason.contains("no transaction endpoint"),
                    "{operation}: the reason must name the missing surface: {reason}"
                );
                assert!(
                    reason.contains("HBaseDriver::execute"),
                    "{operation}: the reason must point at the write refusal: {reason}"
                );
            }
            other => panic!("{operation} returned {other:?}, not an explicit refusal"),
        }
    }
}

/// A refusal that also refuses the handle is not a transaction refusal — the
/// handle check has to come first, or a caller would be told "this driver has no
/// transactions" when the truth is "this handle is not yours".
#[tokio::test]
async fn a_handle_from_another_epoch_is_refused_before_any_capability_question() {
    let mine = provider();
    let theirs = provider();
    let handle = unacquired_handle(&theirs, "hbase_resource:hbase-cfg");
    let options = TransactionOptions::default();

    for error in [
        mine.begin_transaction(&handle, &options)
            .await
            .expect_err("the handle is not this instance's"),
        mine.commit_transaction(&handle)
            .await
            .expect_err("the handle is not this instance's"),
        mine.rollback_transaction(&handle)
            .await
            .expect_err("the handle is not this instance's"),
    ] {
        assert!(
            matches!(error, ResourceError::StaleRuntimeEpoch { .. }),
            "got {error:?}, which would misreport a foreign handle as a missing \
             capability"
        );
    }
}

#[tokio::test]
async fn a_namespace_switch_is_unsupported_and_naming_a_level_is_a_refusal() {
    let (provider, handle) = opened().await;

    // There is no other namespace to switch *to*, so this is `Unsupported`
    // rather than `RequiresReplacement`.
    let disposition = provider
        .change_context(&handle, &NamespaceTarget::empty())
        .await
        .expect("the empty target names no level, so there is nothing to refuse");
    assert_eq!(disposition, ContextChangeDisposition::Unsupported);

    // Naming a level this driver does not have is refused as invalid, which is
    // more informative than reporting the switch as unsupported.
    for (target, kind) in [
        (
            NamespaceTarget {
                database: Some("default".into()),
                ..NamespaceTarget::empty()
            },
            NamespaceLevelKind::Database,
        ),
        (
            NamespaceTarget {
                schema: Some("public".into()),
                ..NamespaceTarget::empty()
            },
            NamespaceLevelKind::Schema,
        ),
        (
            NamespaceTarget {
                catalog: Some("prod".into()),
                ..NamespaceTarget::empty()
            },
            NamespaceLevelKind::Catalog,
        ),
    ] {
        let error = provider
            .change_context(&handle, &target)
            .await
            .expect_err("Stargate has no such level to switch to");
        match error {
            ResourceError::NonexistentNamespaceLevel { kind: reported } => {
                assert_eq!(reported, kind)
            }
            other => panic!("naming a level returned {other:?}, not a level refusal"),
        }
    }
}

#[tokio::test]
async fn cancelling_reports_that_nothing_happened_rather_than_pretending() {
    let (provider, handle) = opened().await;
    let execution_id = execution_id();

    let receipt = provider
        .request_cancel(&handle, &execution_id)
        .await
        .expect("a cancel is refused, not failed");

    assert_eq!(receipt.execution_id, execution_id);
    assert_eq!(receipt.disposition, CancelDisposition::Unsupported);
    assert!(
        receipt.state.is_none(),
        "no state may be reported for an execution this driver never tracked"
    );

    // The driver's own `cancel_query` is `Ok(())` — it cancels nothing. A
    // provider that delegated to it would hand back a receipt implying a cancel
    // had been requested, which is the exact receipt the contract forbids. So
    // the mismatch is asserted here: the driver succeeds, and this provider must
    // still decline to speak for it.
    let driver = HBaseDriver::new();
    let untracked = ConnectionHandle {
        id: "hbase-untracked".to_string(),
        pool_id: "hbase".to_string(),
    };
    assert!(
        driver.cancel_query(&untracked).await.is_ok(),
        "the driver's cancel is a no-op; this provider must therefore not use it, \
         which is what CancelDisposition::Unsupported above reports"
    );
}

#[tokio::test]
async fn resetting_discards_because_nothing_was_verified() {
    let (provider, handle) = opened().await;

    for baseline in [Baseline::default(), baseline()] {
        let disposition = provider
            .reset_resource(&handle, &baseline)
            .await
            .expect("a reset with nothing to replay is still an answer");
        assert_eq!(
            disposition,
            ResetDisposition::Discard,
            "Clean would claim the resource was verified back to a known state, \
             and no SQL was replayed to prove it"
        );
    }
}

#[tokio::test]
async fn a_scope_asking_for_a_transaction_or_a_pinned_handle_is_refused() {
    let server = stargate_answering().await;
    let provider = provider();
    let budget = Arc::new(BudgetLedger::default());

    for (field, reason_fragment) in [
        ("holds_open_transaction", "no transaction to hold"),
        ("pin_for_streaming", "nothing survives the call"),
    ] {
        let scope = match field {
            "holds_open_transaction" => datazen_driver_api::resource::ResourceScope {
                holds_open_transaction: true,
                ..scope()
            },
            _ => datazen_driver_api::resource::ResourceScope {
                pin_for_streaming: true,
                ..scope()
            },
        };
        let error = provider
            .acquire_resource(
                &datazen_driver_api::resource::AcquireResourceRequest {
                    scope,
                    ..acquire_request(&config_for(&server))
                },
                &budget.as_port(),
            )
            .await
            .expect_err("this resource cannot honour that scope");
        assert!(matches!(error, ResourceError::OperationNotSupported { .. }));
        let reason = refusal_reason(&error);
        assert!(
            reason.contains(reason_fragment),
            "{field}: the refusal must explain itself: {reason}"
        );
    }
    assert_eq!(
        budget.granted(),
        Vec::<u32>::new(),
        "a refused scope must not be charged"
    );
}

/// The driver's own write path. `HBaseDriver::execute` refuses every write
/// before it reaches the wire, which is why the transaction refusals above can
/// be stated as facts rather than assumptions.
#[tokio::test]
async fn the_driver_refuses_writes_before_they_reach_the_wire() {
    let driver = HBaseDriver::new();
    let error = driver
        .execute(
            &ConnectionHandle {
                id: "probe".into(),
                pool_id: "probe".into(),
            },
            "put books row-1 value-1",
        )
        .await
        .expect_err("writes are refused before they reach the wire");

    assert!(
        matches!(&error, DriverError::QueryFailed(reason) if reason.contains("writes are not supported")),
        "the write refusal must say writes are unsupported, got {error:?}"
    );
}

// ---------------------------------------------------------------------------
// payload decoding
// ---------------------------------------------------------------------------

/// A Stargate catalog listing is an object, not a row set. Decoding it as "no
/// rows, succeeded" is the shell behaviour this whole decoder exists to prevent.
#[test]
fn a_catalog_listing_is_refused_by_the_shape_that_arrived() {
    for (command, payload) in [
        (
            "list_databases",
            serde_json::json!({ "databases": ["default"] }),
        ),
        (
            "list_tables",
            serde_json::json!({ "tables": ["books", "authors"] }),
        ),
        (
            "get_table_schema",
            serde_json::json!({ "schema": { "name": "books" } }),
        ),
    ] {
        let error =
            decode_command_result(&payload).expect_err("a catalog object is not a result set");
        let reason = refusal_reason(&error);
        assert!(
            reason.contains("ResultChunk"),
            "{command}: the refusal must name the channel it cannot use: {reason}"
        );
        // The shape is named, never the contents: a catalog listing carries
        // table names and row data.
        assert!(
            !reason.contains("books") && !reason.contains("authors"),
            "{command}: the refusal echoed payload contents: {reason}"
        );
        assert!(
            reason.contains("key(s)"),
            "{command}: the refusal must describe the shape: {reason}"
        );
    }
}

/// The two shapes that *are* accepted, and what each becomes.
#[test]
fn the_two_shipped_shapes_decode_and_nothing_else_does() {
    // A result set: statements and chunks both carry through, with the chunk's
    // statement index pointing at the statement it came from.
    let (statements, chunks) = decode_command_result(&serde_json::json!({
        "results": [
            {
                "sql": "scan books",
                "columns": [{ "name": "key", "dataType": "Utf8", "nullable": false }],
                "rows": [["row-1"], ["row-2"]],
                "rowsAffected": null,
                "executionTimeMs": 4,
                "truncated": false
            }
        ],
        "totalTimeMs": 5
    }))
    .expect("a MultiQueryResult is what query_multi returns");
    assert_eq!(statements.len(), 1);
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].statement_index, 0);
    assert_eq!(chunks[0].sql, "scan books");
    assert_eq!(chunks[0].rows.len(), 2);
    assert_eq!(chunks[0].rows_affected, None);

    // The `execute` acknowledgement: no statement text, but a row count, so it
    // is one chunk rather than zero.
    let (statements, chunks) = decode_command_result(&serde_json::json!({ "rowsAffected": 7 }))
        .expect("the execute acknowledgement is decodable");
    assert!(statements.is_empty());
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].rows_affected, Some(7));
    assert!(chunks[0].rows.is_empty());
    assert!(chunks[0].sql.is_empty());

    // Everything else is somebody else's JSON.
    for payload in [
        serde_json::json!({ "rowsAffected": 7, "warnings": [] }),
        serde_json::json!([{ "sql": "scan books" }, { "sql": "scan authors" }]),
        serde_json::json!(null),
        serde_json::json!("scan books"),
        serde_json::json!(42),
    ] {
        let error = decode_command_result(&payload).expect_err("this shape is not decoded");
        assert!(matches!(error, ResourceError::OperationNotSupported { .. }));
        assert_eq!(refusal_reason(&error).contains("ResultChunk"), true);
    }
}

/// The decoder is strict about the *serde* shape too, because a loose one would
/// turn a schema drift into silently empty rows.
#[test]
fn a_result_set_missing_a_required_field_is_refused_rather_than_half_read() {
    // `StatementResult` requires `sql`, `columns`, `rows`, `rowsAffected`,
    // `executionTimeMs` and `truncated`; and serde renames `data_type` to
    // `dataType`. Any of them missing means the driver and the decoder disagree,
    // which is worth an error rather than an empty row set.
    let payload = serde_json::json!({
        "results": [{ "columns": [], "rows": [], "rowsAffected": null, "executionTimeMs": 1, "truncated": false }],
        "totalTimeMs": 1
    });
    let error =
        decode_command_result(&payload).expect_err("a result set without `sql` is not read");
    assert!(matches!(error, ResourceError::OperationNotSupported { .. }));

    let payload = serde_json::json!({
        "results": [{
            "sql": "scan books",
            "columns": [{ "name": "key", "data_type": "Utf8", "nullable": false }],
            "rows": [],
            "rowsAffected": null,
            "executionTimeMs": 1,
            "truncated": false
        }],
        "totalTimeMs": 1
    });
    decode_command_result(&payload)
        .expect_err("camelCase is the wire name; a snake_case column is a drift, not a row");
}
