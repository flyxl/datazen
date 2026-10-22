//! The result source the narrow write path is built on.
//!
//! A single-table `SELECT` in a retained session asks the driver whether the
//! result maps to a real relation; when the driver confirms it, the Host
//! attaches a `StatementResultSource` to the statement metadata and marks the
//! mapping `Verified`. That source carries the **session context the driver
//! observed**, projected into the platform-api shape.
//!
//! Two things are pinned here:
//!
//! * the projection is a real projection — every field survives, including a
//!   `path` that is a sequence of segments rather than one joined string;
//! * a `Verified` mapping is what opens the narrow write path: it writes, and a
//!   result without it is refused.

use super::result_write::{commit_query_result_cell_impl, ResultCellWrite};
use crate::testing::app_state::TestAppState;
use crate::testing::mock_driver::MockDriverOptions;
use datazen_application::{
    dto::requests::{CommandCall, ExecuteInSessionRequest, OpenSessionRequest},
    sessions::ConnectionUseCases,
};
use datazen_driver_api::{
    namespace::NamespaceTarget as DriverNamespace,
    session::{ObservationConfidence, SessionContext as DriverContext, TransactionState},
    ColumnSchema, TableSchema, Value,
};
use datazen_platform_api::{
    context::OwnerRef,
    dto::{
        execution::{ExecutionState, RuntimeResultBinding, StatementResultSource},
        idempotency::IdempotentOperation,
        session::{ContextConfidence, SessionHandle, TransactionState as PlatformTransactionState},
    },
    id::ConnectionId,
    target::{ExecutionTarget, NamespaceTarget},
};
use datazen_runtime::application::RuntimeArtifactChunk;

/// The table the fixture query reads. `SELECT *` is what the Host can map.
const TABLE: &str = "users";

fn columns() -> Vec<ColumnSchema> {
    vec![
        ColumnSchema {
            name: "id".into(),
            data_type: "integer".into(),
            nullable: false,
            default_value: None,
            comment: None,
            is_primary_key: true,
            is_auto_increment: false,
        },
        ColumnSchema {
            name: "name".into(),
            data_type: "text".into(),
            nullable: true,
            default_value: None,
            comment: None,
            is_primary_key: false,
            is_auto_increment: false,
        },
    ]
}

fn table_schema() -> TableSchema {
    TableSchema {
        table_name: TABLE.into(),
        columns: columns(),
        primary_keys: vec!["id".into()],
        indexes: Vec::new(),
        foreign_keys: Vec::new(),
        check_constraints: Vec::new(),
        table_options: Default::default(),
    }
}

/// What the driver answers for `get_table_schema` on this relation. The
/// `relationSql` probe and the write path's re-verification both read it.
fn resolved() -> serde_json::Value {
    serde_json::json!({
        "database": "app",
        "catalog": null,
        "schema": "public",
        "table": TABLE,
        "path": [],
        "sessionScoped": false,
    })
}

/// The observation the driver reports for the session.
///
/// `path` carries **two** segments on purpose: it is the field the runtime
/// kernel's scalar projection used to join into one string, so a fixture with a
/// single segment could not tell a correct projection from a lossy one.
fn observed_context() -> DriverContext {
    DriverContext {
        namespace: DriverNamespace {
            database: Some("app".into()),
            catalog: Some("app_catalog".into()),
            schema: Some("public".into()),
            path: vec!["tenant_a".into(), "shard_1".into()],
        },
        search_path: vec!["public".into(), "extra".into()],
        effective_identity: Some("app_user".into()),
        transaction_state: TransactionState::Idle,
        autocommit: Some(false),
        confidence: ObservationConfidence::Confirmed,
    }
}

/// The same observation, restricted to what a retained session can carry
/// end-to-end: the runtime kernel's target model holds **one** path segment, so
/// a two-segment observation cannot survive the session view for comparison.
fn writable_context() -> DriverContext {
    DriverContext {
        namespace: DriverNamespace {
            database: Some("app".into()),
            catalog: None,
            schema: Some("public".into()),
            path: vec!["primary".into()],
        },
        search_path: vec!["public".into()],
        effective_identity: Some("app_user".into()),
        transaction_state: TransactionState::Idle,
        autocommit: Some(true),
        confidence: ObservationConfidence::Confirmed,
    }
}

fn options(verified: bool) -> MockDriverOptions {
    let mut options = MockDriverOptions {
        columns: columns(),
        primary_keys: vec!["id".into()],
        query_rows: vec![vec![
            Some(Value::Integer(1)),
            Some(Value::String("alice".into())),
        ]],
        // The write path is a parameterized `UPDATE` the driver must be able to
        // qualify; both are driver capabilities, not assertions about the Host.
        parameterized_writes: true,
        rewrite_sql_target: true,
        query_rows_affected: Some(1),
        ..Default::default()
    };
    if verified {
        options.command_results.insert(
            "get_table_schema".into(),
            serde_json::json!({"schema": table_schema(), "resolvedTarget": resolved()}),
        );
    }
    options
}

/// Open a retained runtime session and hand back its handle.
async fn open_session(test: &TestAppState, connection_id: &str) -> SessionHandle {
    let config = test
        .store
        .get_connection(connection_id)
        .await
        .expect("the profile exists");
    let adapter = test.state.platform_require().expect("platform is ready");
    let call = adapter.begin_call();
    let runtime = adapter.runtime().expect("runtime is ready");
    let token = runtime
        .issue_submission_token(call.context(), IdempotentOperation::OpenSession, None)
        .await
        .expect("runtime session token issues");
    let opened = runtime
        .open_session(
            call.context(),
            OpenSessionRequest::new(
                ExecutionTarget::new(
                    ConnectionId::new(connection_id),
                    NamespaceTarget::database(config.database.as_deref().unwrap_or("app")),
                ),
                OwnerRef::ClientSession {
                    client_instance_id: call.context().client_instance_id.clone(),
                    purpose: "commandTest".into(),
                },
                token.idempotency_key,
            ),
        )
        .await
        .expect("runtime session opens");
    opened.session.handle
}

/// Run the map-able single-table query and return its binding plus every
/// published chunk. A failed execution panics with the error the Host reported.
async fn run_select(
    test: &TestAppState,
    handle: &SessionHandle,
    label: &str,
) -> (RuntimeResultBinding, Vec<RuntimeArtifactChunk>) {
    let adapter = test.state.platform_require().expect("platform is ready");
    let context = adapter.begin_call().context().clone();
    let runtime = adapter.runtime().expect("runtime is ready");
    let token = runtime
        .issue_submission_token(
            &context,
            IdempotentOperation::ExecuteInSession,
            Some(handle),
        )
        .await
        .expect("runtime execution token issues");
    let session = runtime
        .get_session(&context, handle.clone())
        .await
        .expect("the session is readable");
    let receipt = runtime
        .execute_in_session(
            &context,
            ExecuteInSessionRequest {
                handle: handle.clone(),
                expected_context_revision: Some(session.context_revision),
                call: CommandCall {
                    command: "query".into(),
                    input: serde_json::json!({ "sql": format!("SELECT * FROM {TABLE}") }),
                },
                idempotency_key: token.idempotency_key,
            },
        )
        .await
        .expect("the fixture query is admitted");
    let view = loop {
        let view = runtime
            .get_execution(&context, receipt.execution_id.clone())
            .await
            .expect("the execution is readable");
        if view.state.is_terminal() {
            break view;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    };
    let mut chunks = Vec::new();
    for artifact in &view.artifact_ids {
        let first = runtime
            .read_artifact_chunk(
                &context,
                artifact.clone(),
                datazen_platform_api::id::Counter::ZERO,
            )
            .await
            .expect("the artifact chunk is readable");
        for index in 0..first.published_chunk_count.get() {
            chunks.push(
                runtime
                    .read_artifact_chunk(
                        &context,
                        artifact.clone(),
                        datazen_platform_api::id::Counter::new(index),
                    )
                    .await
                    .expect("the artifact chunk is readable"),
            );
            // One read per published index; the first chunk is re-read rather
            // than cloned so a storage failure cannot hide behind a clone.
        }
    }
    if view.state != ExecutionState::Succeeded {
        let outputs: Vec<&serde_json::Value> = chunks
            .iter()
            .filter_map(|chunk| chunk.output.as_ref())
            .collect();
        panic!(
            "{label}: a single-table SELECT in a retained session must not fail: {:?} / {:?} / {outputs:?}",
            view.error_code, view.effect_outcome
        );
    }
    (
        view.runtime_binding
            .expect("the runtime binds the result to its session"),
        chunks,
    )
}

/// The statement source the Host attached to the result, if any.
fn statement_source(chunks: &[RuntimeArtifactChunk]) -> &StatementResultSource {
    chunks
        .iter()
        .map(|chunk| &chunk.source)
        .find(|source| {
            source.writable_mapping
                == datazen_platform_api::dto::execution::WritableMapping::Verified
        })
        .expect("the driver confirmed the relation, so the source must be verified")
}

/// Rows actually delivered to the grid.
fn delivered_rows(chunks: &[RuntimeArtifactChunk]) -> usize {
    chunks
        .iter()
        .filter_map(|chunk| chunk.output.as_ref())
        .filter(|output| output.get("kind").and_then(|kind| kind.as_str()) == Some("rows"))
        .filter_map(|output| output.get("rows").and_then(|rows| rows.as_array()))
        .map(Vec::len)
        .sum()
}

fn cell_write(binding: RuntimeResultBinding) -> ResultCellWrite {
    ResultCellWrite {
        binding,
        statement_index: 0,
        row_index: 0,
        column: "name".into(),
        value: Value::String("bob".into()),
    }
}

#[tokio::test]
async fn a_confirmed_result_carries_the_observed_context_without_compressing_it() {
    let observed = observed_context();
    let test = TestAppState::with_session_context(options(true), observed.clone()).await;
    test.save_connection("narrow-source").await;
    let handle = open_session(&test, "narrow-source").await;

    let (_, chunks) = run_select(&test, &handle, "confirmed result").await;
    let source = statement_source(&chunks);

    assert_eq!(
        source.writable_mapping,
        datazen_platform_api::dto::execution::WritableMapping::Verified
    );
    // Field by field against the observation, not against a second copy of the
    // mapping function: the assertion has to fail when a field is dropped.
    assert_eq!(
        source.context.namespace.path, observed.namespace.path,
        "path is a segment sequence and must not be joined into one string"
    );
    assert_eq!(
        source.context.namespace.path,
        vec!["tenant_a".to_string(), "shard_1".to_string()],
        "a two-segment path must arrive as two segments"
    );
    assert_eq!(
        source.context.namespace.database, observed.namespace.database,
        "database must survive the projection"
    );
    assert_eq!(
        source.context.namespace.catalog, observed.namespace.catalog,
        "catalog must survive the projection"
    );
    assert_eq!(
        source.context.namespace.schema, observed.namespace.schema,
        "schema must survive the projection"
    );
    assert_eq!(
        source.context.search_path,
        Some(observed.search_path.clone()),
        "search_path is one observed list, not `Some([])`"
    );
    assert_eq!(
        source.context.effective_identity, observed.effective_identity,
        "the observed identity must survive the projection"
    );
    assert_eq!(
        source.context.transaction_state,
        PlatformTransactionState::None,
        "an idle driver transaction is `none` in the platform-api shape"
    );
    assert_eq!(
        source.context.autocommit,
        Some(false),
        "a reported autocommit value must not decay to `false`/`None`"
    );
    assert_eq!(
        source.context.confidence,
        ContextConfidence::Confirmed,
        "confidence must not be flattened"
    );
    // The relation itself is driver-confirmed, not guessed from the SQL text.
    let relation = source
        .relation
        .as_ref()
        .expect("a verified source names its relation");
    assert_eq!(
        relation.object.as_ref().map(|o| o.name.as_str()),
        Some(TABLE)
    );
    assert_eq!(relation.namespace.database.as_deref(), Some("app"));
    assert_eq!(relation.namespace.schema.as_deref(), Some("public"));
}

#[tokio::test]
async fn a_verified_mapping_writes_the_cell_through_its_live_session() {
    let test = TestAppState::with_session_context(options(true), writable_context()).await;
    test.save_connection("narrow-write-ok").await;
    let handle = open_session(&test, "narrow-write-ok").await;
    let (binding, chunks) = run_select(&test, &handle, "writable result").await;
    assert_eq!(
        statement_source(&chunks).writable_mapping,
        datazen_platform_api::dto::execution::WritableMapping::Verified
    );
    let before = test.mock.query_calls();

    commit_query_result_cell_impl(&test.state, cell_write(binding))
        .await
        .expect("a verified mapping writes the cell back");

    assert!(
        test.mock.query_calls() > before,
        "the write must reach the driver as a statement"
    );
}

#[tokio::test]
async fn a_result_without_a_verified_mapping_is_refused() {
    // No confirmed `get_table_schema`: the driver did not answer for the
    // relation, so the Host must leave the result read-only.
    let test = TestAppState::with_session_context(options(false), writable_context()).await;
    test.save_connection("narrow-write-unverified").await;
    let handle = open_session(&test, "narrow-write-unverified").await;
    let (binding, chunks) = run_select(&test, &handle, "writable result").await;
    assert!(
        chunks.iter().all(|chunk| chunk.source.writable_mapping
            != datazen_platform_api::dto::execution::WritableMapping::Verified),
        "without a driver confirmation nothing may be marked verified"
    );
    let before = test.mock.query_calls();

    let error = commit_query_result_cell_impl(&test.state, cell_write(binding))
        .await
        .unwrap_err()
        .to_string();

    assert!(
        error.contains("no verified writable source"),
        "an unverified result must be refused by the narrow gate; got: {error}"
    );
    assert_eq!(
        test.mock.query_calls(),
        before,
        "a refused write must not send a statement"
    );
}

/// A probe that cannot answer must degrade, never veto the query.
///
/// Attaching `writable_mapping=verified` is a *capability probe*: it answers
/// "could these rows be written back?". Its failure mode is "unknown", which
/// means the grid stays read-only — **not** that the user loses the result.
/// This test therefore fails if any step of the probe starts aborting the
/// execution again (`?` on the probe, or an early return that drops the rows).
#[tokio::test]
async fn a_probe_that_cannot_answer_degrades_instead_of_failing_the_query() {
    let cases = [
        (
            "unreadable relation metadata",
            serde_json::json!({"schema": "not-a-table-schema", "resolvedTarget": resolved()}),
        ),
        (
            "incomplete relation metadata",
            serde_json::json!({"schema": table_schema()}),
        ),
        (
            "unverified relation target",
            serde_json::json!({
                "schema": table_schema(),
                "resolvedTarget": {"database": "", "table": TABLE, "schema": "public"},
            }),
        ),
    ];
    for (case, answer) in cases {
        let mut options = options(false);
        options
            .command_results
            .insert("get_table_schema".into(), answer);
        let test = TestAppState::with_session_context(options, writable_context()).await;
        test.save_connection("narrow-probe-degraded").await;
        let handle = open_session(&test, "narrow-probe-degraded").await;

        // Panics if the execution fails: the probe has no authority over that.
        let (_, chunks) = run_select(&test, &handle, case).await;

        assert_eq!(
            delivered_rows(&chunks),
            1,
            "{case}: the rows must still reach the grid"
        );
        assert!(
            chunks.iter().all(|chunk| chunk.source.writable_mapping
                != datazen_platform_api::dto::execution::WritableMapping::Verified),
            "{case}: a probe that cannot answer must leave the result read-only"
        );
        assert!(
            chunks.iter().all(|chunk| chunk.source.relation.is_none()),
            "{case}: no relation may be invented when the probe failed"
        );
    }
}
