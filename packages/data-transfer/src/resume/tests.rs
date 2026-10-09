use super::fingerprint::{hash_value, last_cursor};
use super::*;
use datazen_driver_api::{ColumnInfo, ColumnSchema, IndexInfo, QueryResult, TableOptions};
use sha2::{Digest, Sha256};

fn source_schema(keys: &[&str]) -> TableSchema {
    source_schema_with_type(keys, "BIGINT")
}

fn source_schema_with_type(keys: &[&str], data_type: &str) -> TableSchema {
    TableSchema {
        table_name: "items".into(),
        columns: keys
            .iter()
            .map(|name| ColumnSchema {
                name: (*name).into(),
                data_type: data_type.into(),
                nullable: false,
                default_value: None,
                comment: None,
                is_primary_key: true,
                is_auto_increment: false,
            })
            .collect(),
        primary_keys: keys.iter().map(|key| (*key).into()).collect(),
        indexes: vec![IndexInfo {
            name: "PRIMARY".into(),
            columns: keys.iter().map(|key| (*key).into()).collect(),
            is_unique: true,
            is_primary: true,
            index_type: "BTREE".into(),
        }],
        foreign_keys: Vec::new(),
        check_constraints: Vec::new(),
        table_options: TableOptions {
            supports_consistent_snapshot: Some(true),
            ..TableOptions::default()
        },
    }
}

#[test]
fn scalar_keyset_uses_bound_cursor_after_filter_and_hard_limit() {
    let (sql, params) = build_keyset_page(
        "SELECT \"id\", \"payload\" FROM \"items\"",
        Some("WHERE (\"kind\" = $1)"),
        &[Value::String("active".into())],
        &["id".into()],
        Some(&[Value::Integer(9)]),
        500,
        '"',
        |index, _| Ok(format!("${index}")),
        |_| Some("BIGINT".into()),
    )
    .unwrap();
    assert_eq!(params.len(), 2);
    assert!(matches!(&params[0], Value::String(value) if value == "active"));
    assert!(matches!(params[1], Value::Integer(9)));
    assert!(sql.contains("WHERE ((\"kind\" = $1)) AND \"id\" > $2"));
    assert!(sql.ends_with("ORDER BY \"id\" ASC LIMIT 500"));
    assert!(!sql.contains("OFFSET"));
}

#[test]
fn composite_keyset_uses_complete_pk_tuple_and_correct_parameter_positions() {
    let (sql, params) = build_keyset_page(
        "SELECT `tenant`, `id` FROM `items`",
        Some("WHERE (`kind` = ? AND `enabled` = ?)"),
        &[Value::String("active".into()), Value::Bool(true)],
        &["tenant".into(), "id".into()],
        Some(&[Value::String("west".into()), Value::Integer(11)]),
        25,
        '`',
        |_, _| Ok("?".into()),
        |_| Some("BIGINT".into()),
    )
    .unwrap();
    assert_eq!(params.len(), 4);
    assert!(sql.contains("(`tenant`, `id`) > (?, ?)"));
    assert!(sql.ends_with("ORDER BY `tenant` ASC, `id` ASC LIMIT 25"));
}

#[test]
fn keyset_page_refuses_unbounded_or_oversized_limits() {
    for limit in [0, MAX_TRANSFER_BATCH_SIZE + 1] {
        assert!(build_keyset_page(
            "SELECT `id` FROM `items`",
            None,
            &[],
            &["id".into()],
            None,
            limit,
            '`',
            |_, _| Ok("?".into()),
            |_| Some("BIGINT".into()),
        )
        .is_err());
    }
}

#[test]
fn empty_bounded_page_without_decoded_column_metadata_is_a_valid_terminator() {
    // Both PG and MySQL derive QueryResult.columns from the first returned
    // row. A SELECT that returns no rows therefore has no column metadata,
    // including the final empty page after an exact chunk-size multiple.
    let empty = QueryResult {
        columns: Vec::new(),
        rows: Vec::new(),
        rows_affected: Some(0),
        execution_time_ms: 0,
    };
    assert!(validate_page(&empty, &["id".into(), "payload".into()], 2).is_ok());

    let wrong_nonempty_projection = QueryResult {
        columns: ["payload", "id"]
            .into_iter()
            .map(|name| ColumnInfo {
                name: name.into(),
                data_type: "INT".into(),
                nullable: false,
            })
            .collect(),
        rows: vec![vec![Some(Value::Integer(1)), Some(Value::Integer(2))]],
        rows_affected: Some(1),
        execution_time_ms: 0,
    };
    let error = validate_page(
        &wrong_nonempty_projection,
        &["id".into(), "payload".into()],
        2,
    )
    .expect_err("non-empty pages must still match the inspected projection");
    assert!(error.to_string().contains("expected [\"id\", \"payload\"]"));
    assert!(error.to_string().contains("got [\"payload\", \"id\"]"));

    let oversized = QueryResult {
        columns: vec![ColumnInfo {
            name: "id".into(),
            data_type: "INT".into(),
            nullable: false,
        }],
        rows: vec![vec![Some(Value::Integer(1))], vec![Some(Value::Integer(2))]],
        rows_affected: Some(2),
        execution_time_ms: 0,
    };
    assert!(validate_page(&oversized, &["id".into()], 1)
        .expect_err("a driver must not exceed the hard page size")
        .to_string()
        .contains("bounded page limit"));
}

#[test]
fn only_exact_nonnullable_declared_primary_key_order_is_resumable() {
    let scalar = source_schema(&["id"]);
    assert_eq!(
        resumable_primary_key(&scalar, None, "postgresql").unwrap(),
        vec!["id"]
    );
    assert_eq!(
        resumable_primary_key(
            &source_schema_with_type(&["tenant", "id"], "INT"),
            None,
            "mysql"
        )
        .unwrap(),
        vec!["tenant", "id"]
    );
    assert!(resumable_primary_key(&scalar, None, "sqlite").is_err());
    let mut nullable = scalar.clone();
    nullable.columns[0].nullable = true;
    assert!(resumable_primary_key(&nullable, None, "postgresql").is_err());
}

#[test]
fn mysql_exact_numeric_keys_require_lossless_integer_cursor_decoding() {
    for data_type in [
        "BIGINT",
        "BIGINT UNSIGNED",
        "DECIMAL(65, 30)",
        "NUMERIC(30, 10)",
    ] {
        let error =
            resumable_primary_key(&source_schema_with_type(&["id"], data_type), None, "mysql")
                .expect_err("MySQL exact numeric text cursors must fail closed");
        assert!(
            error.to_string().contains("string cursor bindings"),
            "{error}"
        );
    }

    assert!(resumable_primary_key(
        &source_schema_with_type(&["id"], "INT UNSIGNED"),
        None,
        "mysql"
    )
    .is_ok());
}

#[test]
fn fingerprint_hashes_large_values_and_float_bits_with_type_tags() {
    let value_a = Value::String("9007199254740993.0000000000001".into());
    let value_b = Value::String("9007199254740993.0000000000002".into());
    let mut a = Sha256::new();
    let mut b = Sha256::new();
    hash_value(&mut a, Some(&value_a));
    hash_value(&mut b, Some(&value_b));
    assert_ne!(a.finalize(), b.finalize());

    let float_a = Value::Float(f64::from_bits(0x3ff0_0000_0000_0000));
    let float_b = Value::Float(f64::from_bits(0x3ff0_0000_0000_0001));
    let mut a = Sha256::new();
    let mut b = Sha256::new();
    hash_value(&mut a, Some(&float_a));
    hash_value(&mut b, Some(&float_b));
    assert_ne!(a.finalize(), b.finalize());
}

#[test]
fn acknowledged_chunk_with_checkpoint_advance_failure_reports_confirmed_target_state() {
    assert_eq!(
        confirmed_chunk_outcome(true),
        TableExecutionOutcome::Committed
    );
    assert_eq!(
        confirmed_chunk_outcome(false),
        TableExecutionOutcome::PartiallyApplied
    );
}

#[test]
fn batch_size_hard_limit_is_user_visible_api_validation() {
    let mut job = TransferJob {
        source: crate::model::Endpoint {
            db_session_id: "src".into(),
            database: "db".into(),
            schema: None,
        },
        target: Some(crate::model::Endpoint {
            db_session_id: "tgt".into(),
            database: "db".into(),
            schema: None,
        }),
        sql_file_target: None,
        mode: crate::model::TransferMode::Data,
        write_mode: crate::model::WriteMode::Insert,
        tables: Vec::new(),
        options: Default::default(),
    };
    job.options.batch_size = MAX_TRANSFER_BATCH_SIZE + 1;
    assert!(job.options.validate().is_err());
    assert_eq!(
        effective_chunk_size(500, 200, MAX_BOUND_QUERY_PARAMETERS).unwrap(),
        300
    );
    assert_eq!(
        effective_chunk_size(500, 1_000, MAX_BOUND_QUERY_PARAMETERS).unwrap(),
        60
    );
    assert_eq!(effective_chunk_size(500, 200, 2_100).unwrap(), 10);
    assert!(effective_chunk_size(500, 2_101, 2_100).is_err());
    assert!(remaining_page_limit(500, 5, Some(6)).is_some_and(|limit| limit == 1));
    assert!(remaining_page_limit(500, 6, Some(6)).is_none());
}

#[test]
fn test_tester_rejects_ambiguous_primary_key_resume_contracts() {
    let mut no_primary_key = source_schema(&["id"]);
    no_primary_key.primary_keys.clear();
    assert!(resumable_primary_key(&no_primary_key, None, "postgresql")
        .expect_err("a keyless relation cannot be resumed")
        .to_string()
        .contains("declared primary key"));

    let mut duplicate_key = source_schema(&["id"]);
    duplicate_key.primary_keys = vec!["id".into(), "id".into()];
    assert!(resumable_primary_key(&duplicate_key, None, "postgresql")
        .expect_err("duplicated PK metadata must fail closed")
        .to_string()
        .contains("unique primary-key metadata"));

    let mut missing_column = source_schema(&["id"]);
    missing_column.columns.clear();
    assert!(resumable_primary_key(&missing_column, None, "postgresql")
        .expect_err("a PK absent from inspected columns is unsafe")
        .to_string()
        .contains("missing from inspected source metadata"));

    let mut unverified_column = source_schema(&["id"]);
    unverified_column.columns[0].is_primary_key = false;
    assert!(
        resumable_primary_key(&unverified_column, None, "postgresql")
            .expect_err("column metadata must corroborate the declared key")
            .to_string()
            .contains("not proven non-null")
    );

    let mut index_mismatch = source_schema(&["id"]);
    index_mismatch.indexes.clear();
    assert!(resumable_primary_key(&index_mismatch, None, "postgresql")
        .expect_err("the full PK index order must be available")
        .to_string()
        .contains("index order could not be verified"));

    for data_type in ["TEXT", "DOUBLE PRECISION"] {
        assert!(
            resumable_primary_key(
                &source_schema_with_type(&["id"], data_type),
                None,
                "postgresql"
            )
            .is_err(),
            "unsafe ordered key type {data_type} must be rejected"
        );
    }

    let mut recordset_schema = source_schema(&["id"]);
    recordset_schema.columns.push(ColumnSchema {
        name: "label".into(),
        data_type: "TEXT".into(),
        nullable: true,
        default_value: None,
        comment: None,
        is_primary_key: false,
        is_auto_increment: false,
    });
    let recordset = crate::model::TransferRecordset {
        order_by: Some("label".into()),
        start: None,
        end: None,
        tuple_range: None,
        limit: None,
    };
    assert!(
        resumable_primary_key(&recordset_schema, Some(&recordset), "postgresql")
            .expect_err("a user range cannot change the complete PK scan order")
            .to_string()
            .contains("recordset order to match")
    );
}

fn tester_placeholder(_: usize, _: Option<&str>) -> Result<String, TransferError> {
    Ok("?".into())
}

fn tester_reject_placeholder(_: usize, _: Option<&str>) -> Result<String, TransferError> {
    Err(TransferError::unsupported("no typed bind contract"))
}

#[test]
fn test_tester_keyset_builder_fails_closed_on_incomplete_or_unsafe_cursors() {
    let no_keys = build_keyset_page(
        "SELECT `id` FROM `items`",
        None,
        &[],
        &[],
        None,
        2,
        '`',
        tester_placeholder,
        |_| None,
    )
    .expect_err("keyset scans need a declared key");
    assert!(no_keys.to_string().contains("complete source primary key"));

    let wrong_arity = build_keyset_page(
        "SELECT `tenant`, `id` FROM `items`",
        None,
        &[],
        &["tenant".into(), "id".into()],
        Some(&[Value::String("north".into())]),
        2,
        '`',
        tester_placeholder,
        |_| Some("TEXT".into()),
    )
    .expect_err("a composite cursor must bind every key component");
    assert!(wrong_arity
        .to_string()
        .contains("complete source primary key"));

    let null_cursor = build_keyset_page(
        "SELECT `id` FROM `items`",
        None,
        &[],
        &["id".into()],
        Some(&[Value::Null]),
        2,
        '`',
        tester_placeholder,
        |_| Some("BIGINT".into()),
    )
    .expect_err("NULL cannot define a keyset continuation");
    assert!(null_cursor.to_string().contains("NULL primary-key value"));

    let too_many_parameters = vec![Value::Integer(1); MAX_BOUND_QUERY_PARAMETERS + 1];
    assert!(build_keyset_page(
        "SELECT `id` FROM `items`",
        None,
        &too_many_parameters,
        &["id".into()],
        None,
        2,
        '`',
        tester_placeholder,
        |_| Some("BIGINT".into()),
    )
    .expect_err("the source page must remain below the driver bind limit")
    .to_string()
    .contains("safe limit"));

    let placeholder_error = build_keyset_page(
        "SELECT `id` FROM `items`",
        None,
        &[],
        &["id".into()],
        Some(&[Value::Integer(1)]),
        2,
        '`',
        tester_reject_placeholder,
        |_| Some("BIGINT".into()),
    )
    .expect_err("a driver placeholder failure must propagate");
    assert!(placeholder_error
        .to_string()
        .contains("typed bind contract"));
}

#[test]
fn test_tester_hash_and_cursor_helpers_cover_supported_value_tags() {
    let values = [
        None,
        Some(Value::Null),
        Some(Value::Bool(true)),
        Some(Value::Integer(-42)),
        Some(Value::Float(f64::from_bits(0x3ff0_0000_0000_0001))),
        Some(Value::String("utf8 雪".into())),
        Some(Value::Bytes(vec![0, 127, 255])),
        Some(Value::Timestamp("2026-09-25T12:00:00Z".into())),
        Some(Value::Json(serde_json::json!({"a": [1, true]}))),
    ];
    let mut digest = Sha256::new();
    for value in &values {
        hash_value(&mut digest, value.as_ref());
    }
    assert_ne!(digest.finalize(), Sha256::digest(b""));

    assert!(cursor_value_supported(&Value::Integer(1)));
    assert!(cursor_value_supported(&Value::String("key".into())));
    assert!(cursor_value_supported(&Value::Bool(false)));
    assert!(!cursor_value_supported(&Value::Float(1.0)));
    assert!(last_cursor(&[], &[0]).is_err());
    assert!(last_cursor(&[vec![Some(Value::Null)]], &[0]).is_err());
    assert!(last_cursor(&[vec![Some(Value::Float(1.0))]], &[0]).is_err());
}

#[derive(Default)]
struct TesterCheckpoint {
    progress: Option<ResumeTableProgress>,
    renew_calls: usize,
    fail_renew_on_call: Option<usize>,
    fail_prepare: bool,
    fail_advance: bool,
    invalidated: bool,
}

impl TransferResumeCheckpoint for TesterCheckpoint {
    fn renew(&mut self) -> Result<(), TransferError> {
        self.renew_calls += 1;
        if self.fail_renew_on_call == Some(self.renew_calls) {
            return Err(TransferError::validation(
                "injected checkpoint renewal failure",
            ));
        }
        Ok(())
    }

    fn prepare_table(
        &mut self,
        source_table: &str,
        target_table: &str,
        key_columns: &[String],
        chunk_size: u32,
        source_fingerprint: &str,
    ) -> Result<ResumeTableProgress, TransferError> {
        if self.fail_prepare {
            return Err(TransferError::validation(
                "injected checkpoint prepare failure",
            ));
        }
        Ok(self
            .progress
            .get_or_insert_with(|| ResumeTableProgress {
                source_table: source_table.into(),
                target_table: target_table.into(),
                key_columns: key_columns.to_vec(),
                chunk_size,
                source_fingerprint: source_fingerprint.into(),
                cursor: None,
                rows_seen: 0,
            })
            .clone())
    }

    fn advance_table(
        &mut self,
        source_table: &str,
        cursor: Vec<Value>,
        rows_seen: u64,
    ) -> Result<(), TransferError> {
        if self.fail_advance {
            return Err(TransferError::validation(
                "injected checkpoint advancement failure",
            ));
        }
        let progress = self
            .progress
            .as_mut()
            .filter(|progress| progress.source_table == source_table)
            .ok_or_else(|| TransferError::validation("missing checkpoint table"))?;
        progress.cursor = Some(cursor);
        progress.rows_seen = rows_seen;
        Ok(())
    }

    fn token(&self) -> Option<String> {
        Some("tester-opaque-token".into())
    }

    fn has_table_progress(&self, source_table: &str) -> bool {
        self.progress
            .as_ref()
            .is_some_and(|progress| progress.source_table == source_table)
    }

    fn table_has_committed_chunks(&self, source_table: &str) -> bool {
        self.progress
            .as_ref()
            .is_some_and(|progress| progress.source_table == source_table && progress.rows_seen > 0)
    }

    fn is_invalidated(&self) -> bool {
        self.invalidated
    }

    fn invalidate(&mut self) {
        self.invalidated = true;
    }
}

fn chunk_test_drivers(
    query_error: Option<String>,
    source_rollback_error: bool,
) -> (
    std::sync::Arc<datazen_driver_api::mock_driver::MockDriver>,
    std::sync::Arc<datazen_driver_api::mock_driver::MockDriver>,
    TableSchema,
    TableSchema,
) {
    use datazen_driver_api::mock_driver::{MockDriver, MockDriverOptions};

    let source_schema = source_schema(&["id"]);
    let mut target_schema = source_schema.clone();
    target_schema.table_name = "items_copy".into();
    let source = MockDriver::new(
        "postgresql",
        MockDriverOptions {
            columns: source_schema.columns.clone(),
            table_schema: Some(source_schema.clone()),
            query_rows: vec![vec![Some(Value::Integer(1))]],
            empty_keyset_after_cursor: true,
            parameterized_writes: true,
            query_error,
            rollback_error_on_call: source_rollback_error.then_some(1),
            rollback_error: Some("injected source snapshot rollback failure".into()),
            ..Default::default()
        },
    );
    let target = MockDriver::new(
        "mysql",
        MockDriverOptions {
            columns: target_schema.columns.clone(),
            table_schema: Some(target_schema.clone()),
            parameterized_writes: true,
            execute_rows_affected: 1,
            ..Default::default()
        },
    );
    (source, target, source_schema, target_schema)
}

async fn run_test_chunk(
    checkpoint: &mut TesterCheckpoint,
    query_error: Option<String>,
    source_rollback_error: bool,
) -> (
    Result<ChunkedTableResult, TransferError>,
    std::sync::Arc<datazen_driver_api::mock_driver::MockDriver>,
    std::sync::Arc<datazen_driver_api::mock_driver::MockDriver>,
) {
    let _guard = crate::TEST_COMMIT_ACK_LOSS_TEST_LOCK.lock().await;
    run_test_chunk_without_ack_fault_lock(checkpoint, query_error, source_rollback_error).await
}

async fn run_test_chunk_without_ack_fault_lock(
    checkpoint: &mut TesterCheckpoint,
    query_error: Option<String>,
    source_rollback_error: bool,
) -> (
    Result<ChunkedTableResult, TransferError>,
    std::sync::Arc<datazen_driver_api::mock_driver::MockDriver>,
    std::sync::Arc<datazen_driver_api::mock_driver::MockDriver>,
) {
    let (source, target, source_schema, target_schema) =
        chunk_test_drivers(query_error, source_rollback_error);
    let source_handle = datazen_driver_api::ConnectionHandle {
        id: "source-session".into(),
        pool_id: "source-pool".into(),
    };
    let target_handle = datazen_driver_api::ConnectionHandle {
        id: "target-session".into(),
        pool_id: "target-pool".into(),
    };
    let mut mapping = crate::model::TableMapping::auto("items");
    mapping.target_table = "items_copy".into();
    mapping.column_mappings = vec![ColumnMapping {
        source_column: "id".into(),
        target_column: "id".into(),
        skip: false,
        target_native_type: None,
    }];
    let job = TransferJob {
        source: crate::model::Endpoint {
            db_session_id: source_handle.id.clone(),
            database: "app".into(),
            schema: None,
        },
        target: Some(crate::model::Endpoint {
            db_session_id: target_handle.id.clone(),
            database: "app".into(),
            schema: None,
        }),
        sql_file_target: None,
        mode: crate::model::TransferMode::Data,
        write_mode: crate::model::WriteMode::Insert,
        tables: vec![mapping],
        options: crate::model::TransferOptions {
            batch_size: 2,
            stop_on_error: true,
            confirmed_destructive: false,
            use_target_default_collation: false,
        },
    };
    let table = TableInspectResult {
        source_table: "items".into(),
        target_table: "items_copy".into(),
        status: crate::model::TableMappingStatus::Matched,
        create_new: false,
        enabled: true,
        column_mappings: job.tables[0].column_mappings.clone(),
        source_columns: vec!["id".into()],
        source_primary_keys: vec!["id".into()],
        target_columns: vec!["id".into()],
        source_column_types: Default::default(),
        target_column_types: Default::default(),
        incompatible_reason: None,
        source_row_count: Some(1),
        recordset: None,
    };
    let source_scope = SourceScope {
        where_sql: None,
        recordset_sql: None,
        params: Vec::new(),
        count_params: Vec::new(),
    };
    let columns: Vec<_> = job.tables[0].column_mappings.iter().collect();
    let formatter = ValueFormatter::SameFamily;
    let result = execute_chunked_table(ChunkedTransferContext {
        source_driver: source.as_ref(),
        source_handle: &source_handle,
        target_driver: target.as_ref(),
        target_handle: &target_handle,
        job: &job,
        table: &table,
        source_schema: &source_schema,
        target_schema: &target_schema,
        source_scope: &source_scope,
        source_table_ref: "items",
        target_table_ref: "items_copy",
        source_quote: '"',
        target_type: "mysql",
        columns: &columns,
        formatter: &formatter,
        cancelled: None,
        write_started: None,
        checkpoint,
    })
    .await;
    (result, source, target)
}

#[tokio::test]
async fn test_tester_confirmed_chunk_commit_fences_failed_checkpoint_advance() {
    let mut checkpoint = TesterCheckpoint {
        fail_advance: true,
        ..Default::default()
    };
    let (result, source, target) = run_test_chunk(&mut checkpoint, None, false).await;
    let result = result.expect("a confirmed target commit should return typed partial state");

    assert_eq!(
        result.result.outcome,
        Some(TableExecutionOutcome::Committed)
    );
    assert_eq!(result.result.rows_inserted, Some(1));
    assert_eq!(result.confirmed_rows, 1);
    assert!(result.stop_later_tables_reason.is_some());
    assert!(checkpoint.invalidated);
    assert_eq!(target.commit_calls(), 1);
    assert_eq!(source.open_transaction_count(), 0);
    assert_eq!(target.open_transaction_count(), 0);
}

#[tokio::test]
async fn test_tester_chunk_ack_loss_fences_checkpoint_after_target_commit() {
    use super::super::execute::{arm_test_commit_ack_loss, clear_test_commit_ack_loss};

    let _guard = crate::TEST_COMMIT_ACK_LOSS_TEST_LOCK.lock().await;
    let _reset = clear_test_commit_ack_loss();
    arm_test_commit_ack_loss("items_copy").expect("the one-shot test seam should arm");
    let mut checkpoint = TesterCheckpoint::default();
    let (result, source, target) =
        run_test_chunk_without_ack_fault_lock(&mut checkpoint, None, false).await;
    clear_test_commit_ack_loss();
    let result = result.expect("unknown commit acknowledgement is a typed table result");

    assert_eq!(result.result.outcome, Some(TableExecutionOutcome::Unknown));
    assert_eq!(result.result.rows_inserted, None);
    assert!(checkpoint.invalidated);
    assert_eq!(target.commit_calls(), 1);
    assert_eq!(target.open_transaction_count(), 0);
    assert_eq!(source.open_transaction_count(), 0);
}

#[tokio::test]
async fn test_tester_source_fingerprint_failure_with_unknown_snapshot_rollback_fences_resume() {
    let mut checkpoint = TesterCheckpoint::default();
    let (result, source, target) = run_test_chunk(
        &mut checkpoint,
        Some("injected source fingerprint query failure".into()),
        true,
    )
    .await;
    let result = result.expect("unknown snapshot cleanup is returned as a fenced outcome");

    assert_eq!(
        result.result.outcome,
        Some(TableExecutionOutcome::NotStarted)
    );
    assert_eq!(result.result.rows_inserted, None);
    assert!(result.stop_later_tables_reason.is_some());
    assert!(result
        .result
        .error
        .as_deref()
        .is_some_and(|error| error.contains("source snapshot rollback outcome is UNKNOWN")));
    assert!(checkpoint.invalidated);
    assert_eq!(target.commit_calls(), 0);
    assert_eq!(source.open_transaction_count(), 1);
}
