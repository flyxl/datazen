//! The narrow write path's refusals.
//!
//! The narrow path is the one that can write a query result cell back. It
//! re-verifies the mapping against the live catalog, and it must refuse a
//! **read-only connection** before a single statement of that write leaves the
//! process — the narrow mapping verdict says nothing about the connection's
//! read-only flag, so the host has to ask for it itself.

use super::result_write::{commit_query_result_cell_impl, ResultCellWrite};
use crate::testing::app_state::TestAppState;
use crate::testing::mock_driver::MockDriverOptions;
use datazen::application::sessions::ConnectionUseCases;
use datazen::execution::application::requests::{
    CommandCall, ExecuteInSessionRequest, OpenSessionRequest,
};
use datazen::{
    execution::application::idempotency::IdempotentOperation,
    execution::target::{ExecutionTarget, NamespaceTarget},
    execution::{ExecutionState, RuntimeResultBinding, SessionHandle},
    shared::context::OwnerRef,
    shared::id::ConnectionId,
};
use datazen_driver_api::{ColumnSchema, Value};

/// The table the fixture query reads.
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

fn options() -> MockDriverOptions {
    MockDriverOptions {
        columns: columns(),
        primary_keys: vec!["id".into()],
        query_rows: vec![vec![
            Some(Value::Integer(1)),
            Some(Value::String("alice".into())),
        ]],
        ..Default::default()
    }
}

/// Open a runtime session and keep its handle: the write path binds to it.
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

/// Run the fixture query and hand back the binding the write path needs.
async fn run_fixture_query(test: &TestAppState, handle: &SessionHandle) -> RuntimeResultBinding {
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
    // The runtime's optimistic concurrency gate: the caller states the context
    // revision it means to write in.
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
                    input: serde_json::json!({ "sql": format!("SELECT id FROM {TABLE}") }),
                },
                idempotency_key: token.idempotency_key,
            },
        )
        .await
        .expect("the fixture query runs");
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
    if view.state != ExecutionState::Succeeded {
        let mut outputs = Vec::new();
        for artifact in &view.artifact_ids {
            let first = runtime
                .read_artifact_chunk(
                    &context,
                    artifact.clone(),
                    datazen::shared::id::Counter::ZERO,
                )
                .await
                .expect("the artifact chunk is readable");
            for index in 0..first.published_chunk_count.get() {
                let chunk = runtime
                    .read_artifact_chunk(
                        &context,
                        artifact.clone(),
                        datazen::shared::id::Counter::new(index),
                    )
                    .await
                    .expect("the artifact chunk is readable");
                outputs.push(chunk.output);
            }
        }
        panic!(
            "the fixture query failed: {:?} / {:?} / {:?}",
            view.error_code, view.effect_outcome, outputs
        );
    }
    view.runtime_binding
        .expect("the runtime binds the result to its session")
}

/// The write the grid would send for one cell.
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
async fn a_read_only_connection_refuses_the_write_and_sends_no_statement() {
    let test = TestAppState::with_options(options()).await;
    let mut config = crate::testing::app_state::sample_postgres_config("narrow-read-only");
    config.read_only = true;
    test.store.save_connection(config).await.unwrap();
    let handle = open_session(&test, "narrow-read-only").await;
    let binding = run_fixture_query(&test, &handle).await;
    let before = test.mock.query_calls();

    let error = commit_query_result_cell_impl(&test.state, cell_write(binding))
        .await
        .unwrap_err()
        .to_string();

    // The grid's own reason code, so the front end classifies the refusal the
    // way it classifies every other read-only verdict.
    assert!(
        error.starts_with("grid.result.writeDenied: "),
        "got: {error}"
    );
    assert!(error.contains("read-only"), "got: {error}");
    // Nothing of the write left the process: not even the metadata read the
    // write path does before it builds its statement.
    assert_eq!(
        test.mock.query_calls(),
        before,
        "no statement of the write may leave the process"
    );
}

#[tokio::test]
async fn a_writable_connection_is_not_refused_as_read_only() {
    let test = TestAppState::with_options(options()).await;
    test.save_connection("narrow-writable").await;
    let handle = open_session(&test, "narrow-writable").await;
    let binding = run_fixture_query(&test, &handle).await;

    let error = commit_query_result_cell_impl(&test.state, cell_write(binding))
        .await
        .unwrap_err()
        .to_string();

    assert!(
        !error.starts_with("grid.result.writeDenied: "),
        "a writable connection must not be refused as read-only; got: {error}"
    );
}
