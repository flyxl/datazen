use super::*;
use crate::data_transfer::model::{
    Endpoint, TableExecutionOutcome, TransferRunOptions, TransferRunSelection,
};
use crate::data_transfer::{
    TableMapping, TransferJob, TransferMode, TransferRunRequest, WriteMode,
};
use crate::testing::app_state::TestAppState;
use crate::testing::mock_driver::{MockDriver, MockDriverOptions};
use datazen_driver_api::{IndexInfo, TableOptions, TableSchema};
use std::collections::HashMap;

fn job(source: String, target: String) -> TransferJob {
    TransferJob {
        source: Endpoint {
            db_session_id: source,
            database: "app".into(),
            schema: None,
        },
        target: Some(Endpoint {
            db_session_id: target,
            database: "app".into(),
            schema: None,
        }),
        sql_file_target: None,
        mode: TransferMode::Data,
        write_mode: WriteMode::Insert,
        tables: vec![TableMapping::auto("users")],
        options: Default::default(),
    }
}

fn options() -> MockDriverOptions {
    let mut options = crate::testing::app_state::rich_mock_options();
    options.parameterized_writes = true;
    options.execute_rows_affected = 1;
    options.columns = MockDriver::default_table_schema("users").columns.clone();
    options
}

fn safe_options() -> MockDriverOptions {
    let mut options = options();
    let mut schema = MockDriver::default_table_schema("users");
    schema.indexes.push(IndexInfo {
        name: "users_pkey".into(),
        columns: vec!["id".into()],
        is_unique: true,
        is_primary: true,
        index_type: "BTREE".into(),
    });
    schema.table_options = TableOptions {
        supports_consistent_snapshot: Some(true),
        ..Default::default()
    };
    options.columns = schema.columns.clone();
    options.primary_keys = schema.primary_keys.clone();
    options.table_schema = Some(schema);
    options.empty_keyset_after_cursor = true;
    options
}

fn transactional_pk_schema(table: &str) -> TableSchema {
    let mut schema = MockDriver::default_table_schema(table);
    schema.indexes.push(IndexInfo {
        name: format!("{table}_pkey"),
        columns: vec!["id".into()],
        is_unique: true,
        is_primary: true,
        index_type: "BTREE".into(),
    });
    schema.table_options = TableOptions {
        supports_consistent_snapshot: Some(true),
        ..Default::default()
    };
    schema
}

fn two_table_safe_options() -> MockDriverOptions {
    let mut options = safe_options();
    options.tables.push(crate::db::TableInfo {
        name: "orders".into(),
        schema: None,
        table_type: crate::db::TableType::Table,
        row_count: Some(2),
    });
    options.table_schemas_by_database.insert(
        "app".into(),
        HashMap::from([
            ("users".into(), transactional_pk_schema("users")),
            ("orders".into(), transactional_pk_schema("orders")),
        ]),
    );
    options
}

fn request(plan_id: String, resume_token: Option<String>) -> TransferRunRequest {
    TransferRunRequest {
        plan_id,
        selection: TransferRunSelection::default(),
        options: TransferRunOptions::default(),
        job_id: None,
        resume_token,
    }
}

#[tokio::test]
async fn unknown_target_transaction_metadata_allows_transfer_without_token() {
    let mut opts = options();
    opts.execute_with_params_error = Some("injected target write failure".into());
    let test = TestAppState::with_options(opts).await;
    let (_, source) = test
        .save_and_connect("transfer-resume-unknown-source")
        .await;
    let (_, target) = test
        .save_and_connect("transfer-resume-unknown-target")
        .await;
    let preview = preview_data_transfer_impl(&test.state, job(source, target))
        .await
        .expect("preview should succeed with unknown target engine metadata");

    let result = execute_data_transfer_impl(&test.state, request(preview.plan_id, None))
        .await
        .expect("the ordinary atomic writer may still attempt the transfer");
    assert!(result.partial);
    assert_eq!(result.resume_token, None);
    assert!(result.tables[0]
        .error
        .as_deref()
        .is_some_and(|error| error.contains("resume token not issued")));
    assert_eq!(test.mock.commit_calls(), 0);
}

#[tokio::test]
async fn unsupported_source_snapshot_keeps_transactional_table_boundary_resume() {
    let mut opts = safe_options();
    opts.execute_with_params_error = Some("injected target write failure".into());
    opts.table_schema
        .as_mut()
        .expect("configured schema")
        .table_options
        .supports_consistent_snapshot = Some(false);
    let mut target_schema = MockDriver::default_table_schema("users");
    target_schema.indexes.push(IndexInfo {
        name: "users_pkey".into(),
        columns: vec!["id".into()],
        is_unique: true,
        is_primary: true,
        index_type: "BTREE".into(),
    });
    target_schema.table_options = TableOptions {
        supports_consistent_snapshot: Some(true),
        ..Default::default()
    };
    opts.table_schemas_by_database.insert(
        "target_app".into(),
        HashMap::from([("users".into(), target_schema)]),
    );

    let test = TestAppState::with_options(opts).await;
    let (_, source) = test
        .save_and_connect("transfer-resume-source-snapshot-unsupported-source")
        .await;
    let (_, target) = test
        .save_and_connect("transfer-resume-source-snapshot-unsupported-target")
        .await;
    let mut transfer = job(source, target);
    transfer.target.as_mut().expect("database target").database = "target_app".into();
    let preview = preview_data_transfer_impl(&test.state, transfer)
        .await
        .expect("preview should include source and target schema metadata");

    let result = execute_data_transfer_impl(&test.state, request(preview.plan_id, None))
        .await
        .expect("the transactional whole-table fallback should execute");
    assert!(result.partial);
    assert!(result.resume_token.is_some());
    assert!(result.tables[0]
        .error
        .as_deref()
        .is_some_and(|error| error.contains("row-level resume is unavailable")));
}

#[tokio::test]
async fn mysql_bigint_key_keeps_atomic_table_boundary_resume() {
    let test = TestAppState::with_options(MockDriverOptions::default()).await;
    let mut source_options = safe_options();
    let source_schema = source_options
        .table_schema
        .as_mut()
        .expect("configured MySQL source schema");
    source_schema
        .columns
        .iter_mut()
        .find(|column| column.name == "id")
        .expect("primary key column")
        .data_type = "bigint unsigned".into();
    source_options.columns = source_schema.columns.clone();
    test.registry
        .register_test_driver("mysql", MockDriver::new("mysql", source_options))
        .await;

    let mut target_options = safe_options();
    target_options.execute_with_params_error = Some("injected target write failure".into());
    test.registry
        .register_test_driver("postgres", MockDriver::new("postgres", target_options))
        .await;

    let mut source_config =
        crate::testing::app_state::sample_postgres_config("transfer-resume-mysql-bigint-source");
    source_config.database_type = "mysql".into();
    source_config.port = Some(3306);
    test.store
        .save_connection(source_config)
        .await
        .expect("save MySQL source connection");
    let source = crate::services::migration_endpoint::open_dedicated_session(
        &test.state,
        "transfer-resume-mysql-bigint-source",
        "app",
        "data-transfer-test",
    )
    .await
    .expect("connect MySQL source");
    let (_, target) = test
        .save_and_connect("transfer-resume-mysql-bigint-target")
        .await;
    let preview = preview_data_transfer_impl(&test.state, job(source, target))
        .await
        .expect("preview MySQL to PostgreSQL transfer");

    let partial = execute_data_transfer_impl(&test.state, request(preview.plan_id.clone(), None))
        .await
        .expect("atomic table failure should return a table-boundary checkpoint");
    let token = partial
        .resume_token
        .expect("transactional target should keep the table-boundary checkpoint");
    assert!(partial.partial);

    test.registry
        .register_test_driver("postgres", MockDriver::new("postgres", safe_options()))
        .await;
    let resumed = execute_data_transfer_impl(&test.state, request(preview.plan_id, Some(token)))
        .await
        .expect("unsupported MySQL BIGINT key should fall back to whole-table transaction resume");
    assert!(!resumed.partial);
    assert_eq!(resumed.resume_token, None);
    assert!(resumed.rows_inserted > 0);
}

#[tokio::test]
async fn changed_target_transaction_contract_invalidates_before_legacy_write() {
    let test = TestAppState::with_options(safe_options()).await;
    let (_, source) = test
        .save_and_connect("transfer-resume-target-change-source")
        .await;
    let (_, target) = test
        .save_and_connect("transfer-resume-target-change-target")
        .await;
    let preview = preview_data_transfer_impl(&test.state, job(source, target))
        .await
        .expect("preview should capture the transactional target contract");
    let mut failing = safe_options();
    failing.execute_with_params_error = Some("injected target chunk failure".into());
    test.registry
        .register_test_driver("postgres", MockDriver::new("postgres", failing))
        .await;
    let partial = execute_data_transfer_impl(&test.state, request(preview.plan_id.clone(), None))
        .await
        .expect("a rolled-back chunk should return a resumable result");
    let token = partial.resume_token.expect("safe chunk checkpoint");

    let mut unsafe_target = safe_options();
    unsafe_target
        .table_schema
        .as_mut()
        .expect("configured schema")
        .table_options
        .supports_consistent_snapshot = Some(false);
    let replacement = MockDriver::new("postgres", unsafe_target);
    test.registry
        .register_test_driver("postgres", replacement.clone())
        .await;
    let resumed = execute_data_transfer_impl(
        &test.state,
        request(preview.plan_id.clone(), Some(token.clone())),
    )
    .await
    .expect_err("a token cannot fall back to the legacy full-table writer");
    let message = resumed.to_string();
    assert!(
        message.contains("resume token was invalidated before writing")
            || message.contains("schema changed since preview"),
        "unexpected pre-write rejection: {message}"
    );
    assert_eq!(replacement.commit_calls(), 0);

    let replay = execute_data_transfer_impl(&test.state, request(preview.plan_id, Some(token)))
        .await
        .expect_err("the changed target contract must consume the old token");
    let replay_message = replay.to_string();
    assert!(
        replay_message.contains("resume token")
            && (replay_message.contains("unknown")
                || replay_message.contains("consumed")
                || replay_message.contains("expired")),
        "unexpected consumed-token rejection: {replay_message}"
    );
    assert_eq!(replacement.commit_calls(), 0);
}

#[tokio::test]
async fn unknown_source_snapshot_close_preserves_committed_chunk_and_stops_later_tables() {
    let test = TestAppState::with_options(two_table_safe_options()).await;
    let (_, source) = test
        .save_and_connect("transfer-resume-snapshot-close-source")
        .await;
    let (_, target) = test
        .save_and_connect("transfer-resume-snapshot-close-target")
        .await;
    let mut transfer = job(source, target);
    transfer.tables.push(TableMapping::auto("orders"));
    transfer.options.stop_on_error = true;
    let preview = preview_data_transfer_impl(&test.state, transfer)
        .await
        .expect("preview should include both transactional tables");

    let mut failing_options = two_table_safe_options();
    failing_options.execute_with_params_error = Some("injected target page failure".into());
    test.registry
        .register_test_driver("postgres", MockDriver::new("postgres", failing_options))
        .await;
    let partial = execute_data_transfer_impl(&test.state, request(preview.plan_id.clone(), None))
        .await
        .expect("a confirmed rollback should return a checkpoint");
    let token = partial.resume_token.expect("table resume checkpoint");

    let mut close_failure_options = two_table_safe_options();
    close_failure_options.commit_error_on_call = Some(2);
    close_failure_options.commit_error_after_effect = true;
    let close_failure_driver = MockDriver::new("postgres", close_failure_options);
    test.registry
        .register_test_driver("postgres", close_failure_driver.clone())
        .await;
    let result = execute_data_transfer_impl(
        &test.state,
        request(preview.plan_id.clone(), Some(token.clone())),
    )
    .await
    .expect("source snapshot close uncertainty should return known target results");

    assert!(result.partial);
    assert_eq!(result.resume_token, None);
    assert_eq!(result.tables.len(), 2);
    assert_eq!(result.tables[0].source_table, "users");
    assert_eq!(
        result.tables[0].outcome,
        Some(TableExecutionOutcome::Committed)
    );
    assert_eq!(result.tables[0].rows_inserted, Some(1));
    assert!(result.tables[0]
        .error
        .as_deref()
        .is_some_and(|error| error.contains("source snapshot close outcome is UNKNOWN")));
    assert_eq!(result.tables[1].source_table, "orders");
    assert_eq!(
        result.tables[1].outcome,
        Some(TableExecutionOutcome::NotStarted)
    );
    assert_eq!(close_failure_driver.commit_calls(), 2);
    let replay = execute_data_transfer_impl(&test.state, request(preview.plan_id, Some(token)))
        .await
        .expect_err("source snapshot close uncertainty must invalidate the token");
    assert!(replay.to_string().contains("resume token"), "{replay}");
    assert_eq!(close_failure_driver.commit_calls(), 2);
}

#[tokio::test]
async fn chunk_write_and_rollback_failure_fences_later_table_and_consumes_checkpoint() {
    let test = TestAppState::with_options(two_table_safe_options()).await;
    let (_, source) = test
        .save_and_connect("transfer-resume-chunk-rollback-source")
        .await;
    let (_, target) = test
        .save_and_connect("transfer-resume-chunk-rollback-target")
        .await;
    let mut transfer = job(source, target);
    transfer.tables.push(TableMapping::auto("orders"));
    transfer.options.stop_on_error = true;
    let preview = preview_data_transfer_impl(&test.state, transfer)
        .await
        .expect("preview should include both snapshot-safe tables");
    let plan_id = preview.plan_id.clone();

    let mut write_failure_options = two_table_safe_options();
    write_failure_options.execute_with_params_error =
        Some("injected target chunk write failure".into());
    test.registry
        .register_test_driver(
            "postgres",
            MockDriver::new("postgres", write_failure_options),
        )
        .await;
    let first = execute_data_transfer_impl(&test.state, request(plan_id.clone(), None))
        .await
        .expect("a confirmed target rollback should return a checkpoint");
    let old_token = first
        .resume_token
        .expect("a failed chunk with confirmed rollback should be resumable");
    assert!(first.tables[0]
        .error
        .as_deref()
        .is_some_and(|error| error.contains("target chunk write failed")));

    let mut rollback_failure_options = two_table_safe_options();
    rollback_failure_options.execute_with_params_error =
        Some("injected target chunk write failure".into());
    // The command first rolls back its target capability probe; fail the next
    // rollback, which belongs to the actual target row chunk.
    rollback_failure_options.rollback_error_on_call = Some(2);
    rollback_failure_options.rollback_error = Some("injected target chunk rollback failure".into());
    let rollback_failure_driver = MockDriver::new("postgres", rollback_failure_options);
    test.registry
        .register_test_driver("postgres", rollback_failure_driver.clone())
        .await;
    let unknown = execute_data_transfer_impl(
        &test.state,
        request(plan_id.clone(), Some(old_token.clone())),
    )
    .await
    .expect("a chunk rollback error must be reported as an unknown table result");

    assert!(unknown.partial);
    assert_eq!(unknown.resume_token, None);
    assert_eq!(unknown.tables.len(), 2);
    assert_eq!(unknown.tables[0].source_table, "users");
    assert_eq!(
        unknown.tables[0].outcome,
        Some(TableExecutionOutcome::Unknown)
    );
    assert_eq!(unknown.tables[0].rows_inserted, None);
    let error = unknown.tables[0].error.as_deref().unwrap_or_default();
    assert!(error.contains("target chunk write failed"), "{error}");
    assert!(
        error.contains("target rollback outcome is UNKNOWN"),
        "{error}"
    );
    assert!(
        error.contains("injected target chunk rollback failure"),
        "{error}"
    );
    assert_eq!(unknown.tables[1].source_table, "orders");
    assert_eq!(
        unknown.tables[1].outcome,
        Some(TableExecutionOutcome::NotStarted)
    );
    assert_eq!(unknown.tables[1].rows_inserted, Some(0));
    assert_eq!(rollback_failure_driver.commit_calls(), 0);
    assert_eq!(
        rollback_failure_driver.open_transaction_count(),
        1,
        "the failed target rollback leaves only its transaction outcome unknown"
    );

    let plan_replay = execute_data_transfer_impl(&test.state, request(plan_id.clone(), None))
        .await
        .expect_err("the immutable plan must stay consumed after unknown rollback");
    assert!(plan_replay.to_string().contains("already consumed"));
    let checkpoint_replay =
        execute_data_transfer_impl(&test.state, request(plan_id, Some(old_token)))
            .await
            .expect_err("the checkpoint token must not be replayable after an unknown rollback");
    assert!(checkpoint_replay.to_string().contains("consumed"));
}
