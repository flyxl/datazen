//! Unit tests for Data Transfer IPC helpers.

use crate::data_transfer::mapping::auto_map_columns;
use crate::data_transfer::model::{
    Endpoint, SqlFileTarget, TableExecutionOutcome, TableExecutionResult, TransferExecutionResult,
    TransferOptions, TransferRunOptions, TransferRunSelection,
};
use crate::data_transfer::{
    classify_transfer_pair, TableMapping, TransferJob, TransferMode, TransferProfile,
    TransferRunRequest, WriteMode,
};
use chrono::Utc;
use datazen_driver_api::{ColumnSchema, TableSchema};

fn plan_job(source_db_session_id: String, target_db_session_id: String) -> TransferJob {
    TransferJob {
        source: Endpoint {
            db_session_id: source_db_session_id,
            database: "app".into(),
            schema: None,
        },
        target: Some(Endpoint {
            db_session_id: target_db_session_id,
            database: "app".into(),
            schema: None,
        }),
        sql_file_target: None,
        mode: TransferMode::Data,
        write_mode: WriteMode::Insert,
        tables: vec![TableMapping::auto("users")],
        options: TransferOptions::default(),
    }
}

fn transfer_plan_test_options() -> crate::testing::mock_driver::MockDriverOptions {
    let mut options = crate::testing::app_state::rich_mock_options();
    options.columns =
        crate::testing::mock_driver::MockDriver::default_table_schema("users").columns;
    options.parameterized_writes = true;
    options.execute_rows_affected = 1;
    options
}

fn resumable_transfer_plan_test_options() -> crate::testing::mock_driver::MockDriverOptions {
    use crate::testing::mock_driver::MockDriver;
    use datazen_driver_api::{IndexInfo, TableOptions};

    let mut options = transfer_plan_test_options();
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

#[test]
fn pairing_classifies_ir_and_unsupported() {
    let ok = classify_transfer_pair("postgresql", "mysql");
    assert!(ok.supported);
    assert_eq!(ok.path, "ir");

    let bad = classify_transfer_pair("postgresql", "redis");
    assert!(!bad.supported);
}

#[test]
fn transfer_history_outcome_uses_typed_table_results() {
    let result = |outcome| TransferExecutionResult {
        tables: vec![TableExecutionResult::database(
            "source",
            "target",
            None,
            outcome,
            Some("injected table result".into()),
        )],
        partial: true,
        ..Default::default()
    };

    assert_eq!(
        super::transfer_rollback_history_outcome(&result(TableExecutionOutcome::Unknown)),
        "unknown"
    );
    assert_eq!(
        super::transfer_rollback_history_outcome(&result(TableExecutionOutcome::RolledBack)),
        "rolledBack"
    );
    assert_eq!(
        super::transfer_rollback_history_outcome(&result(TableExecutionOutcome::PartiallyApplied)),
        "partiallyApplied"
    );
    assert_eq!(
        super::transfer_rollback_history_outcome(&result(TableExecutionOutcome::NotStarted)),
        "notStarted"
    );
    assert_eq!(
        super::transfer_rollback_history_outcome(&TransferExecutionResult::default()),
        "notRequired"
    );
    let sql_file_partial = TransferExecutionResult {
        tables: vec![TableExecutionResult {
            source_table: "users".into(),
            target_table: "users".into(),
            rows_inserted: Some(1),
            success: false,
            error: Some("injected SQL-file failure".into()),
            outcome: None,
        }],
        partial: true,
        ..Default::default()
    };
    assert_eq!(
        super::transfer_rollback_history_outcome(&sql_file_partial),
        "unknown",
        "SQL-file history semantics remain outside the typed DB outcomes"
    );
    let mixed = TransferExecutionResult {
        tables: vec![
            TableExecutionResult::database(
                "committed-before-unknown",
                "committed-before-unknown",
                Some(1),
                TableExecutionOutcome::RolledBack,
                Some("confirmed rollback".into()),
            ),
            TableExecutionResult::database(
                "unknown",
                "unknown",
                None,
                TableExecutionOutcome::Unknown,
                Some("lost acknowledgement".into()),
            ),
        ],
        partial: true,
        ..Default::default()
    };
    assert_eq!(super::transfer_rollback_history_outcome(&mixed), "unknown");
    assert_eq!(super::transfer_error_history_outcome(false), "notStarted");
    assert_eq!(super::transfer_error_history_outcome(true), "unknown");
}

#[test]
fn auto_map_columns_matches_names_only() {
    let src = TableSchema {
        table_name: "t".into(),
        columns: vec![
            ColumnSchema {
                name: "a".into(),
                data_type: "int".into(),
                nullable: true,
                default_value: None,
                comment: None,
                is_primary_key: false,
                is_auto_increment: false,
            },
            ColumnSchema {
                name: "b".into(),
                data_type: "text".into(),
                nullable: true,
                default_value: None,
                comment: None,
                is_primary_key: false,
                is_auto_increment: false,
            },
        ],
        primary_keys: vec![],
        indexes: vec![],
        foreign_keys: vec![],
        check_constraints: vec![],
        table_options: Default::default(),
    };
    let tgt = TableSchema {
        table_name: "t".into(),
        columns: vec![ColumnSchema {
            name: "a".into(),
            data_type: "integer".into(),
            nullable: true,
            default_value: None,
            comment: None,
            is_primary_key: false,
            is_auto_increment: false,
        }],
        primary_keys: vec![],
        indexes: vec![],
        foreign_keys: vec![],
        check_constraints: vec![],
        table_options: Default::default(),
    };
    let maps = auto_map_columns(&src, &tgt);
    assert_eq!(maps.len(), 1);
    assert_eq!(maps[0].source_column, "a");
}

#[test]
fn transfer_profile_rejects_runtime_file_token_and_unknown_fields() {
    let payload = r#"{
        "version": 1,
        "id": "profile-1",
        "name": "nightly",
        "sourceConnectionId": "src",
        "destinationMode": "sqlFile",
        "mode": "data",
        "writeMode": "insert",
        "tables": [],
        "options": {"batchSize": 10, "stopOnError": true, "confirmedDestructive": false},
        "createdAt": "2026-09-21T00:00:00Z",
        "updatedAt": "2026-09-21T00:00:00Z",
        "fileToken": "must-not-persist"
    }"#;
    assert!(serde_json::from_str::<TransferProfile>(payload).is_err());
}

#[test]
fn test_tester_legacy_transfer_profile_without_format_fields_is_compatible() {
    let payload = r#"{
        "version": 1,
        "id": "legacy-profile",
        "name": "legacy",
        "sourceConnectionId": "src",
        "destinationMode": "sqlFile",
        "mode": "data",
        "writeMode": "insert",
        "tables": [],
        "options": {"batchSize": 10, "stopOnError": true, "confirmedDestructive": false},
        "createdAt": "2026-09-21T00:00:00Z",
        "updatedAt": "2026-09-21T00:00:00Z"
    }"#;
    let profile = serde_json::from_str::<TransferProfile>(payload).unwrap();
    assert_eq!(profile.sql_file_encoding, None);
    assert_eq!(profile.sql_file_compression, None);
    profile.validate().unwrap();
}

#[tokio::test]
async fn transfer_profile_store_round_trip_excludes_runtime_sessions() {
    let test = crate::testing::app_state::TestAppState::new().await;
    test.save_connection("profile-src").await;
    let now = Utc::now();
    let profile = TransferProfile {
        version: TransferProfile::CURRENT_VERSION,
        id: "profile-1".into(),
        name: "nightly".into(),
        source_connection_id: "profile-src".into(),
        target_connection_id: None,
        source_database: Some("app".into()),
        target_database: None,
        source_schema: None,
        target_schema: None,
        destination_mode: "sqlFile".into(),
        sql_file_dialect: Some("mysql".into()),
        sql_file_encoding: Some("utf8Bom".into()),
        sql_file_compression: Some("gzip".into()),
        sql_file_database: Some("analytics".into()),
        sql_file_schema: None,
        mode: TransferMode::Data,
        write_mode: WriteMode::Insert,
        tables: vec![TableMapping::auto("users")],
        options: TransferOptions::default(),
        created_at: now,
        updated_at: now,
    };
    test.store
        .save_transfer_profile(profile.clone())
        .await
        .unwrap();
    let stored = test.store.get_transfer_profiles().await;
    assert_eq!(stored, vec![profile]);
    let mut invalid = stored[0].clone();
    invalid.sql_file_encoding = Some("gbk".into());
    assert!(invalid.validate().is_err());
    invalid.sql_file_encoding = Some("utf16Le".into());
    invalid.sql_file_compression = Some("brotli".into());
    assert!(invalid.validate().is_err());
    let json = tokio::fs::read_to_string(test.store.data_dir().join("transfer_profiles.json"))
        .await
        .unwrap();
    assert!(!json.contains("dbSessionId"));
    assert!(!json.contains("fileToken"));
    test.store
        .delete_transfer_profile("profile-1")
        .await
        .unwrap();
    assert!(test.store.get_transfer_profiles().await.is_empty());
}

#[test]
fn table_mapping_auto_sets_same_name() {
    let m = TableMapping::auto("users");
    assert_eq!(m.source_table, "users");
    assert_eq!(m.target_table, "users");
    assert!(m.enabled);
    assert!(!m.create_new);
}

#[test]
fn resume_checkpoint_binds_the_effective_subselection() {
    let mut job = plan_job("source".into(), "target".into());
    job.tables.push(TableMapping::auto("orders"));
    let selection = TransferRunSelection {
        source_tables: Some(vec!["orders".into()]),
    };
    assert_eq!(
        super::exec::selected_source_tables(&job, &selection),
        vec!["orders"]
    );
}

#[tokio::test]
async fn test_tester_preview_execute_plan_is_opaque_and_one_shot() {
    use crate::testing::app_state::TestAppState;

    let test = TestAppState::with_options(transfer_plan_test_options()).await;
    let (_src_config, source) = test.save_and_connect("transfer-plan-src").await;
    let (_tgt_config, target) = test.save_and_connect("transfer-plan-tgt").await;

    let preview =
        super::preview_data_transfer_impl(&test.state, plan_job(source.clone(), target.clone()))
            .await
            .expect("preview should issue an executable plan");
    assert!(!preview.plan_id.is_empty());
    assert!(preview.can_execute);

    let result = super::execute_data_transfer_impl(
        &test.state,
        TransferRunRequest {
            plan_id: preview.plan_id.clone(),
            selection: TransferRunSelection::default(),
            options: TransferRunOptions::default(),
            job_id: None,
            resume_token: None,
        },
    )
    .await
    .expect("the server-owned plan should execute");
    assert_eq!(result.rows_inserted, 1);

    let retry = super::execute_data_transfer_impl(
        &test.state,
        TransferRunRequest {
            plan_id: preview.plan_id,
            selection: TransferRunSelection::default(),
            options: TransferRunOptions::default(),
            job_id: None,
            resume_token: None,
        },
    )
    .await
    .expect_err("a consumed plan must never be replayable");
    assert!(retry.to_string().contains("already consumed"), "{retry}");
}

#[tokio::test]
async fn sql_file_source_inspection_returns_create_new_source_mappings() {
    use crate::testing::app_state::TestAppState;

    let test = TestAppState::with_options(transfer_plan_test_options()).await;
    let (_config, source) = test.save_and_connect("transfer-sql-file-inspect-src").await;
    let rows = super::inspect_sql_file_transfer_impl(
        &test.state,
        source,
        Some("app".into()),
        None,
        TransferMode::Data,
        None,
        &[],
    )
    .await
    .expect("source-only SQL-file inspection should succeed");

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].source_table, "users");
    assert_eq!(rows[0].target_table, "users");
    assert!(rows[0].create_new);
    assert!(rows[0].enabled);
    assert_eq!(rows[0].source_columns, vec!["id", "name"]);
    assert!(rows[0].column_mappings.iter().all(|mapping| !mapping.skip));
}

#[tokio::test]
async fn sql_file_target_uses_opaque_path_and_publishes_atomic_output() {
    use crate::data_transfer::sql_file::register_path;
    use crate::testing::app_state::TestAppState;

    let test = TestAppState::with_options(transfer_plan_test_options()).await;
    let (_config, source) = test.save_and_connect("transfer-sql-file-src").await;
    let dir = tempfile::tempdir().expect("temporary SQL output directory");
    let destination = dir.path().join("transfer.sql");
    let token = register_path(destination.clone()).expect("register SQL destination");
    let job = TransferJob {
        source: Endpoint {
            db_session_id: source,
            database: "app".into(),
            schema: None,
        },
        target: None,
        sql_file_target: Some(SqlFileTarget {
            file_token: token,
            database_type: None,
            database: None,
            schema: None,
            encoding: None,
            compression: None,
        }),
        mode: TransferMode::Data,
        write_mode: WriteMode::Insert,
        tables: vec![TableMapping::auto("users")],
        options: TransferOptions::default(),
    };

    let preview = super::preview_data_transfer_impl(&test.state, job)
        .await
        .expect("SQL-file preview should issue a plan");
    assert_eq!(preview.pairing_path, "sqlFile");
    assert!(preview.can_execute);
    let result = super::execute_data_transfer_impl(
        &test.state,
        TransferRunRequest {
            plan_id: preview.plan_id,
            selection: TransferRunSelection::default(),
            options: TransferRunOptions::default(),
            job_id: None,
            resume_token: None,
        },
    )
    .await
    .expect("SQL-file plan should execute");
    assert_eq!(result.rows_inserted, 1);
    let output = std::fs::read_to_string(destination).expect("published SQL output");
    assert!(output.contains("INSERT INTO"));
    assert!(output.contains("COMMIT;"));
}

#[tokio::test]
async fn sql_file_empty_selection_keeps_server_discovered_tables() {
    use crate::data_transfer::sql_file::register_path;
    use crate::db::{TableInfo, TableType};
    use crate::testing::app_state::TestAppState;

    let mut options = transfer_plan_test_options();
    options.tables.push(TableInfo {
        name: "orders".into(),
        schema: None,
        table_type: TableType::Table,
        row_count: Some(1),
    });
    let test = TestAppState::with_options(options).await;
    let (_config, source) = test.save_and_connect("transfer-sql-file-multi-src").await;
    let dir = tempfile::tempdir().expect("temporary SQL output directory");
    let destination = dir.path().join("multi.sql");
    let token = register_path(destination.clone()).expect("register SQL destination");
    let job = TransferJob {
        source: Endpoint {
            db_session_id: source,
            database: "app".into(),
            schema: None,
        },
        target: None,
        sql_file_target: Some(SqlFileTarget {
            file_token: token,
            database_type: None,
            database: None,
            schema: None,
            encoding: None,
            compression: None,
        }),
        mode: TransferMode::Data,
        write_mode: WriteMode::Insert,
        // Empty means the server preview discovers all source tables.
        tables: vec![],
        options: TransferOptions::default(),
    };

    let preview = super::preview_data_transfer_impl(&test.state, job)
        .await
        .expect("SQL-file preview should discover both tables");
    assert_eq!(preview.write_plans.len(), 2);
    let result = super::execute_data_transfer_impl(
        &test.state,
        TransferRunRequest {
            plan_id: preview.plan_id,
            // A stale UI may still submit an empty selection. It must not
            // erase the server-owned default table selection.
            selection: TransferRunSelection {
                source_tables: Some(vec![]),
            },
            options: TransferRunOptions::default(),
            job_id: None,
            resume_token: None,
        },
    )
    .await
    .expect("SQL-file execution should retain both tables");
    assert_eq!(result.tables.len(), 2);
    assert!(result.tables.iter().all(|table| table.success));
    let output = std::fs::read_to_string(destination).expect("published SQL output");
    assert!(output.contains("INSERT INTO"));
    assert!(output.contains("\"users\""));
    assert!(output.contains("\"orders\""));
}

#[tokio::test]
async fn sql_file_preview_honors_table_selection_renames_and_skipped_columns() {
    use crate::data_transfer::sql_file::register_path;
    use crate::db::{TableInfo, TableType};
    use crate::testing::app_state::TestAppState;

    let mut options = transfer_plan_test_options();
    options.tables.push(TableInfo {
        name: "orders".into(),
        schema: None,
        table_type: TableType::Table,
        row_count: Some(1),
    });
    let test = TestAppState::with_options(options).await;
    let (_config, source) = test.save_and_connect("transfer-sql-file-mapping-src").await;
    let dir = tempfile::tempdir().expect("temporary SQL output directory");
    let token = register_path(dir.path().join("mapping.sql")).expect("register SQL destination");
    let job = TransferJob {
        source: Endpoint {
            db_session_id: source,
            database: "app".into(),
            schema: None,
        },
        target: None,
        sql_file_target: Some(SqlFileTarget {
            file_token: token,
            database_type: None,
            database: None,
            schema: None,
            encoding: None,
            compression: None,
        }),
        mode: TransferMode::Data,
        write_mode: WriteMode::Insert,
        tables: vec![
            TableMapping {
                source_table: "users".into(),
                target_table: "users_copy".into(),
                create_new: true,
                enabled: true,
                column_mappings: vec![
                    crate::data_transfer::model::ColumnMapping {
                        source_column: "id".into(),
                        target_column: "user_id".into(),
                        skip: false,
                        target_native_type: None,
                    },
                    crate::data_transfer::model::ColumnMapping {
                        source_column: "name".into(),
                        target_column: "name".into(),
                        skip: true,
                        target_native_type: None,
                    },
                ],
                ddl_override: None,
                source_filter: None,
                recordset: None,
            },
            TableMapping {
                source_table: "orders".into(),
                target_table: "orders_copy".into(),
                create_new: true,
                enabled: false,
                column_mappings: Vec::new(),
                ddl_override: None,
                source_filter: None,
                recordset: None,
            },
        ],
        options: TransferOptions::default(),
    };

    let preview = super::preview_data_transfer_impl(&test.state, job)
        .await
        .expect("SQL-file preview should preserve explicit mappings");
    assert_eq!(preview.write_plans.len(), 1);
    assert_eq!(preview.write_plans[0].source_table, "users");
    assert_eq!(preview.write_plans[0].target_table, "users_copy");
    assert_eq!(preview.write_plans[0].mapped_columns.len(), 2);
    assert!(preview.write_plans[0]
        .mapped_columns
        .iter()
        .any(|mapping| mapping.source_column == "name" && mapping.skip));
}

#[tokio::test]
async fn sql_file_target_renders_registered_mysql_dialect() {
    use crate::data_transfer::sql_file::register_path;
    use crate::testing::app_state::TestAppState;

    let mut options = transfer_plan_test_options();
    // This test checks target-dialect quoting and file publication. Keep its
    // source fixture to a portable scalar so an unmodeled PostgreSQL text
    // collation does not imply a safe MySQL table definition.
    options.columns.retain(|column| column.name == "id");
    options.query_rows = vec![vec![Some(crate::db::Value::Integer(1))]];
    let mut schema = crate::testing::mock_driver::MockDriver::default_table_schema("users");
    schema.columns.retain(|column| column.name == "id");
    options.table_schema = Some(schema);
    let test = TestAppState::with_options(options).await;
    let (_config, source) = test.save_and_connect("transfer-sql-file-mysql-src").await;
    let dir = tempfile::tempdir().expect("temporary SQL output directory");
    let destination = dir.path().join("mysql.sql");
    let token = register_path(destination.clone()).expect("register SQL destination");
    let job = TransferJob {
        source: Endpoint {
            db_session_id: source,
            database: "app".into(),
            schema: None,
        },
        target: None,
        sql_file_target: Some(SqlFileTarget {
            file_token: token,
            database_type: Some("mysql".into()),
            database: None,
            schema: None,
            encoding: None,
            compression: None,
        }),
        mode: TransferMode::StructureAndData,
        write_mode: WriteMode::Insert,
        tables: vec![TableMapping::auto("users")],
        options: TransferOptions::default(),
    };

    let preview = super::preview_data_transfer_impl(&test.state, job)
        .await
        .expect("cross-dialect SQL-file preview should succeed");
    assert!(preview.can_execute);
    assert!(preview
        .warnings
        .iter()
        .any(|warning| warning.contains("mysql")));
    let _result = super::execute_data_transfer_impl(
        &test.state,
        TransferRunRequest {
            plan_id: preview.plan_id,
            selection: TransferRunSelection::default(),
            options: TransferRunOptions::default(),
            job_id: None,
            resume_token: None,
        },
    )
    .await
    .expect("cross-dialect SQL-file execution should succeed");
    let output = std::fs::read_to_string(destination).expect("published SQL output");
    assert!(output.contains("`users`"), "{output}");
    assert!(output.contains("INSERT INTO `users`"), "{output}");
    assert!(
        !output.contains("`app`.`users`"),
        "source database must not become an implicit SQL-file target catalog: {output}"
    );
}

#[tokio::test]
async fn sql_file_target_executes_after_source_type_enrichment() {
    use crate::data_transfer::sql_file::register_path;
    use crate::db::Value;
    use crate::testing::app_state::TestAppState;

    let mut options = transfer_plan_test_options();
    options.table_schema = Some(TableSchema {
        table_name: "users".into(),
        columns: vec![ColumnSchema {
            name: "id".into(),
            // The normal schema endpoint exposes a placeholder native type;
            // the adapter's full-type query resolves it to a portable type.
            data_type: "USER-DEFINED".into(),
            nullable: false,
            default_value: None,
            comment: None,
            is_primary_key: true,
            is_auto_increment: false,
        }],
        primary_keys: vec!["id".into()],
        indexes: vec![],
        foreign_keys: vec![],
        check_constraints: vec![],
        table_options: Default::default(),
    });
    options.columns = options
        .table_schema
        .as_ref()
        .expect("schema")
        .columns
        .clone();
    options.query_rows = vec![vec![
        Some(Value::String("id".into())),
        Some(Value::String("integer".into())),
    ]];
    let test = TestAppState::with_options(options).await;
    let (_config, source) = test.save_and_connect("transfer-sql-file-enrich-src").await;
    let dir = tempfile::tempdir().expect("temporary SQL output directory");
    let destination = dir.path().join("enriched.sql");
    let token = register_path(destination.clone()).expect("register SQL destination");
    let job = TransferJob {
        source: Endpoint {
            db_session_id: source,
            database: "app".into(),
            schema: None,
        },
        target: None,
        sql_file_target: Some(SqlFileTarget {
            file_token: token,
            database_type: Some("mysql".into()),
            database: None,
            schema: None,
            encoding: None,
            compression: None,
        }),
        mode: TransferMode::Structure,
        write_mode: WriteMode::Insert,
        tables: vec![TableMapping::auto("users")],
        options: TransferOptions::default(),
    };

    let preview = super::preview_data_transfer_impl(&test.state, job)
        .await
        .expect("preview should enrich source types before target IR validation");
    let error = super::execute_data_transfer_impl(
        &test.state,
        TransferRunRequest {
            plan_id: preview.plan_id,
            selection: TransferRunSelection::default(),
            options: TransferRunOptions::default(),
            job_id: None,
            resume_token: None,
        },
    )
    .await
    .expect("execution should use the same enriched source snapshot as preview");
    assert!(error.tables.iter().all(|table| {
        table.success && table.rows_inserted == Some(0) && table.outcome.is_none()
    }));
}

#[tokio::test]
async fn test_tester_disabled_existing_table_does_not_invalidate_plan() {
    use crate::db::{TableInfo, TableType};
    use crate::testing::app_state::TestAppState;

    let mut options = transfer_plan_test_options();
    options.tables.push(TableInfo {
        name: "archived".into(),
        schema: None,
        table_type: TableType::Table,
        row_count: Some(1),
    });
    let test = TestAppState::with_options(options).await;
    let (_src_config, source) = test.save_and_connect("transfer-plan-disabled-src").await;
    let (_tgt_config, target) = test.save_and_connect("transfer-plan-disabled-tgt").await;

    let mut job = plan_job(source, target);
    let mut disabled = TableMapping::auto("archived");
    disabled.enabled = false;
    job.tables.push(disabled);
    let preview = super::preview_data_transfer_impl(&test.state, job)
        .await
        .expect("preview should accept a disabled mapping beside an active one");
    assert!(preview.can_execute);

    let result = super::execute_data_transfer_impl(
        &test.state,
        TransferRunRequest {
            plan_id: preview.plan_id,
            selection: TransferRunSelection::default(),
            options: TransferRunOptions::default(),
            job_id: None,
            resume_token: None,
        },
    )
    .await
    .expect("disabled mappings must not change the execution snapshot");
    assert_eq!(result.rows_inserted, 1);
}

#[tokio::test]
async fn test_tester_changed_driver_contract_fails_before_execution() {
    use crate::testing::app_state::TestAppState;
    use crate::testing::mock_driver::{MockDriver, MockDriverOptions};

    let test = TestAppState::with_options(transfer_plan_test_options()).await;
    let (_src_config, source) = test.save_and_connect("transfer-plan-driver-src").await;
    let (_tgt_config, target) = test.save_and_connect("transfer-plan-driver-tgt").await;
    let preview = super::preview_data_transfer_impl(&test.state, plan_job(source, target))
        .await
        .unwrap();

    let replacement = MockDriver::new(
        "postgres",
        MockDriverOptions {
            table_schema: Some(MockDriver::default_table_schema("users-v2")),
            parameterized_writes: true,
            ..Default::default()
        },
    );
    test.registry
        .register_test_driver("postgres", replacement.clone())
        .await;

    let err = super::execute_data_transfer_impl(
        &test.state,
        TransferRunRequest {
            plan_id: preview.plan_id,
            selection: TransferRunSelection::default(),
            options: TransferRunOptions::default(),
            job_id: None,
            resume_token: None,
        },
    )
    .await
    .expect_err("a changed schema/driver contract must fail closed");
    assert!(err.to_string().contains("schema changed"), "{err}");
    assert_eq!(
        replacement.query_calls(),
        0,
        "no source scan or target write may start"
    );
}

#[tokio::test]
async fn test_tester_read_only_change_fails_before_target_write() {
    use crate::testing::app_state::TestAppState;

    let test = TestAppState::with_options(transfer_plan_test_options()).await;
    let (_src_config, source) = test.save_and_connect("transfer-plan-ro-src").await;
    let (_tgt_config, target) = test.save_and_connect("transfer-plan-ro-tgt").await;
    let preview = super::preview_data_transfer_impl(&test.state, plan_job(source, target.clone()))
        .await
        .unwrap();
    let query_calls_before = test.mock.query_calls();

    let mut read_only_config = test
        .store
        .get_connection("transfer-plan-ro-tgt")
        .await
        .expect("target config");
    read_only_config.read_only = true;
    crate::services::migration_endpoint::close_dedicated_session(&test.state, &target)
        .await
        .expect("close target runtime session");
    test.store
        .save_connection(read_only_config)
        .await
        .expect("save read-only target config");
    let target = crate::services::migration_endpoint::open_dedicated_session(
        &test.state,
        "transfer-plan-ro-tgt",
        "app",
        "data-transfer-test",
    )
    .await
    .expect("reopen read-only target runtime session");

    let err = super::execute_data_transfer_impl(
        &test.state,
        TransferRunRequest {
            plan_id: preview.plan_id,
            selection: TransferRunSelection::default(),
            options: TransferRunOptions::default(),
            job_id: None,
            resume_token: None,
        },
    )
    .await
    .expect_err("a target becoming read-only must fail closed");
    assert!(err.to_string().contains("read-only"), "{err}");
    assert_eq!(
        test.mock.query_calls(),
        query_calls_before,
        "no transfer scan or target write may start"
    );
}

#[tokio::test]
async fn transfer_partial_run_returns_resume_token_and_completes_once_recovered() {
    use crate::testing::app_state::TestAppState;
    use crate::testing::mock_driver::MockDriver;

    let test = TestAppState::with_options(resumable_transfer_plan_test_options()).await;
    let (_src_config, source) = test.save_and_connect("transfer-plan-fail-src").await;
    let (_tgt_config, target) = test.save_and_connect("transfer-plan-fail-tgt").await;
    let preview = super::preview_data_transfer_impl(&test.state, plan_job(source, target))
        .await
        .unwrap();

    let mut failing_options = resumable_transfer_plan_test_options();
    failing_options.execute_with_params_error = Some("injected target chunk failure".into());
    let failing = MockDriver::new("postgres", failing_options);
    test.registry
        .register_test_driver("postgres", failing)
        .await;

    let request = TransferRunRequest {
        plan_id: preview.plan_id.clone(),
        selection: TransferRunSelection::default(),
        options: TransferRunOptions::default(),
        job_id: None,
        resume_token: None,
    };
    let first = super::execute_data_transfer_impl(&test.state, request.clone())
        .await
        .expect("the failed table must be reported in the execution result");
    assert!(first.partial);
    assert_eq!(first.tables.len(), 1);
    assert!(first.tables[0]
        .error
        .as_deref()
        .is_some_and(|error| error.contains("target chunk write failed")));
    let resume_token = first
        .resume_token
        .clone()
        .expect("a failed transactional chunk should be resumable");

    test.registry
        .register_test_driver(
            "postgres",
            MockDriver::new("postgres", resumable_transfer_plan_test_options()),
        )
        .await;
    let resumed = super::execute_data_transfer_impl(
        &test.state,
        TransferRunRequest {
            resume_token: Some(resume_token.clone()),
            ..request.clone()
        },
    )
    .await
    .expect("a safe partial run should resume from its row cursor");
    assert!(!resumed.partial);
    assert_eq!(resumed.rows_inserted, 1);

    let retry = super::execute_data_transfer_impl(
        &test.state,
        TransferRunRequest {
            resume_token: Some(resume_token),
            ..request
        },
    )
    .await
    .expect_err("a completed resume token must not be replayable");
    assert!(retry.to_string().contains("consumed"), "{retry}");
}

#[tokio::test]
async fn unknown_commit_invalidates_resume_checkpoint_and_consumes_old_token() {
    use crate::data_transfer::model::TableExecutionOutcome;
    use crate::testing::app_state::TestAppState;
    use crate::testing::mock_driver::MockDriver;

    let test = TestAppState::with_options(resumable_transfer_plan_test_options()).await;
    let (_src_config, source) = test.save_and_connect("transfer-unknown-resume-src").await;
    let (_tgt_config, target) = test.save_and_connect("transfer-unknown-resume-tgt").await;
    let preview = super::preview_data_transfer_impl(&test.state, plan_job(source, target))
        .await
        .unwrap();

    let mut failing_options = resumable_transfer_plan_test_options();
    failing_options.execute_with_params_error = Some("injected target chunk failure".into());
    test.registry
        .register_test_driver("postgres", MockDriver::new("postgres", failing_options))
        .await;
    let request = TransferRunRequest {
        plan_id: preview.plan_id.clone(),
        selection: TransferRunSelection::default(),
        options: TransferRunOptions::default(),
        job_id: None,
        resume_token: None,
    };
    let partial = super::execute_data_transfer_impl(&test.state, request.clone())
        .await
        .expect("a confirmed chunk rollback should create a safe checkpoint");
    let old_token = partial.resume_token.expect("partial run checkpoint");

    let mut commit_loss_options = resumable_transfer_plan_test_options();
    commit_loss_options.commit_error_on_call = Some(1);
    commit_loss_options.commit_error_after_effect = true;
    let commit_loss_driver = MockDriver::new("postgres", commit_loss_options);
    test.registry
        .register_test_driver("postgres", commit_loss_driver.clone())
        .await;
    let unknown = super::execute_data_transfer_impl(
        &test.state,
        TransferRunRequest {
            resume_token: Some(old_token.clone()),
            ..request.clone()
        },
    )
    .await
    .expect("unknown commit outcomes should be reported as table results");

    assert!(unknown.partial);
    assert_eq!(unknown.resume_token, None);
    assert_eq!(unknown.tables.len(), 1);
    assert_eq!(
        unknown.tables[0].outcome,
        Some(TableExecutionOutcome::Unknown)
    );
    assert_eq!(unknown.tables[0].rows_inserted, None);
    assert_eq!(commit_loss_driver.commit_calls(), 1);

    let plan_replay = super::execute_data_transfer_impl(&test.state, request.clone())
        .await
        .expect_err("the claimed immutable plan must not be replayable");
    assert!(plan_replay.to_string().contains("already consumed"));

    let replay = super::execute_data_transfer_impl(
        &test.state,
        TransferRunRequest {
            resume_token: Some(old_token),
            ..request
        },
    )
    .await
    .expect_err("the old checkpoint must be invalidated after unknown commit");
    assert!(replay.to_string().contains("consumed"), "{replay}");
}

#[tokio::test]
async fn test_tester_invalid_selection_and_unknown_plan_are_rejected() {
    use crate::testing::app_state::TestAppState;

    let test = TestAppState::with_options(transfer_plan_test_options()).await;
    let (_src_config, source) = test.save_and_connect("transfer-plan-selection-src").await;
    let (_tgt_config, target) = test.save_and_connect("transfer-plan-selection-tgt").await;
    let preview = super::preview_data_transfer_impl(&test.state, plan_job(source, target))
        .await
        .unwrap();

    let invalid = super::execute_data_transfer_impl(
        &test.state,
        TransferRunRequest {
            plan_id: preview.plan_id,
            selection: TransferRunSelection {
                source_tables: Some(vec!["not-in-preview".into()]),
            },
            options: TransferRunOptions::default(),
            job_id: None,
            resume_token: None,
        },
    )
    .await
    .expect_err("the client cannot add an unplanned table");
    assert!(
        invalid.to_string().contains("not in the preview plan"),
        "{invalid}"
    );

    let unknown = super::execute_data_transfer_impl(
        &test.state,
        TransferRunRequest {
            plan_id: "unknown-plan-id".into(),
            selection: TransferRunSelection::default(),
            options: TransferRunOptions::default(),
            job_id: None,
            resume_token: None,
        },
    )
    .await
    .expect_err("unknown plans must fail closed");
    assert!(
        unknown.to_string().contains("unknown or has expired"),
        "{unknown}"
    );
}
