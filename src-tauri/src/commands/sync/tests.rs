use super::compare::{diff_table_schemas_ir, format_ir_type};
use super::tasks::{
    check_sync_conflicts_impl, delete_sync_task_impl, get_sync_tasks_impl,
    save_sync_task_direct_impl,
};
use crate::commands::schema_diff::compare_table_schemas_impl;
use crate::schema_diff::diff_table_schemas;
use crate::store::SyncTask;
use crate::transfer::ir::{IRColumn, IRTable, IRType};

use crate::db::{ColumnSchema, TableSchema, Value};

fn ir_col(name: &str, ir_type: IRType, nullable: bool, is_primary_key: bool) -> IRColumn {
    IRColumn {
        name: name.into(),
        ir_type,
        nullable,
        default_expr: None,
        is_primary_key,
        is_auto_increment: false,
        comment: None,
    }
}

fn col(name: &str, data_type: &str, nullable: bool, pk: bool) -> ColumnSchema {
    ColumnSchema {
        name: name.into(),
        data_type: data_type.into(),
        nullable,
        default_value: None,
        comment: None,
        is_primary_key: pk,
        is_auto_increment: false,
    }
}

fn table(name: &str, columns: Vec<ColumnSchema>, primary_keys: Vec<String>) -> TableSchema {
    TableSchema {
        table_name: name.into(),
        columns,
        primary_keys,
        indexes: vec![],
        foreign_keys: vec![],
        check_constraints: vec![],
        table_options: Default::default(),
    }
}

fn sync_profile(id: &str, source: &str, target: &str) -> crate::data_sync::SyncProfile {
    crate::data_sync::SyncProfile {
        version: crate::data_sync::SyncProfile::CURRENT_VERSION,
        id: id.into(),
        name: "Nightly sync".into(),
        source_connection_id: source.into(),
        target_connection_id: target.into(),
        source_database: Some("app".into()),
        target_database: Some("app".into()),
        source_schema: Some("public".into()),
        target_schema: Some("public".into()),
        tables: vec![crate::data_sync::TableMapping::auto("users")],
        options: crate::data_sync::SyncOptions::default(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

#[tokio::test]
async fn sync_profile_ipc_validates_connections_and_round_trips() {
    use crate::testing::app_state::TestAppState;

    let test = TestAppState::new().await;
    test.save_connection("sync-profile-source").await;
    test.save_connection("sync-profile-target").await;
    let profile = sync_profile(
        "sync-profile-1",
        "sync-profile-source",
        "sync-profile-target",
    );

    super::save_sync_profile_impl(&test.state, profile.clone())
        .await
        .expect("profile should save");
    let loaded = super::get_sync_profiles_impl(&test.state)
        .await
        .expect("profiles should load");
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].id, profile.id);
    assert_eq!(loaded[0].tables, profile.tables);
    assert_eq!(loaded[0].options, profile.options);

    super::delete_sync_profile_impl(&test.state, "sync-profile-1")
        .await
        .expect("profile should delete");
    assert!(super::get_sync_profiles_impl(&test.state)
        .await
        .expect("profiles should load")
        .is_empty());

    let missing_target = sync_profile("missing", "sync-profile-source", "gone");
    let error = super::save_sync_profile_impl(&test.state, missing_target)
        .await
        .expect_err("missing target must be rejected");
    assert!(error
        .to_string()
        .contains("target connection no longer exists"));

    let missing_source = sync_profile("missing-source", "gone", "sync-profile-target");
    let error = super::save_sync_profile_impl(&test.state, missing_source)
        .await
        .expect_err("missing source must be rejected");
    assert!(error
        .to_string()
        .contains("source connection no longer exists"));
}

#[test]
fn ir_diff_treats_equivalent_varchar_as_same() {
    // Postgres `character varying(100)` and MySQL `varchar(100)` both map to
    // IRType::Varchar { length: Some(100) } — must not report dataType change.
    let src = IRTable {
        name: "t".into(),
        columns: vec![ir_col(
            "name",
            IRType::Varchar { length: Some(100) },
            true,
            false,
        )],
        primary_keys: vec![],
        table_options: None,
    };
    let tgt = IRTable {
        name: "t".into(),
        columns: vec![ir_col(
            "name",
            IRType::Varchar { length: Some(100) },
            true,
            false,
        )],
        primary_keys: vec![],
        table_options: None,
    };

    let diff = diff_table_schemas_ir("t", &src, &tgt);
    assert!(diff.added.is_empty());
    assert!(diff.removed.is_empty());
    assert!(diff.changed.is_empty());
}

#[test]
fn ir_diff_detects_columns_missing_on_target() {
    let src = IRTable {
        name: "t".into(),
        columns: vec![
            ir_col("id", IRType::Int64, false, true),
            ir_col("email", IRType::Text, true, false),
        ],
        primary_keys: vec!["id".into()],
        table_options: None,
    };
    let tgt = IRTable {
        name: "t".into(),
        columns: vec![ir_col("id", IRType::Int64, false, true)],
        primary_keys: vec!["id".into()],
        table_options: None,
    };

    let diff = diff_table_schemas_ir("t", &src, &tgt);
    assert_eq!(diff.missing_on_target.len(), 1);
    assert_eq!(diff.missing_on_target[0].name, "email");
    assert_eq!(diff.added.len(), 1);
    assert!(diff.extra_on_target.is_empty());
    assert!(diff.changed.is_empty());
}

#[test]
fn ir_diff_detects_type_nullable_and_pk_changes() {
    let src = IRTable {
        name: "t".into(),
        columns: vec![
            ir_col("id", IRType::Int32, false, true),
            ir_col("name", IRType::Varchar { length: Some(50) }, true, false),
        ],
        primary_keys: vec!["id".into()],
        table_options: None,
    };
    let tgt = IRTable {
        name: "t".into(),
        columns: vec![
            ir_col("id", IRType::Int64, false, false),
            ir_col("name", IRType::Varchar { length: Some(50) }, false, false),
            ir_col("extra", IRType::Text, true, false),
        ],
        primary_keys: vec![],
        table_options: None,
    };

    let diff = diff_table_schemas_ir("t", &src, &tgt);
    assert!(
        diff.added.is_empty(),
        "source columns are all present on target"
    );
    assert_eq!(diff.removed.len(), 1);
    assert_eq!(diff.removed[0].name, "extra");
    assert_eq!(diff.removed[0].data_type, "Text");
    assert_eq!(diff.changed.len(), 2);

    let id = diff.changed.iter().find(|c| c.name == "id").unwrap();
    assert!(id.changes.contains(&"dataType".into()));
    assert!(id.changes.contains(&"isPrimaryKey".into()));
    assert_eq!(id.source.data_type, "Int32");
    assert_eq!(id.target.data_type, "Int64");

    let name = diff.changed.iter().find(|c| c.name == "name").unwrap();
    assert_eq!(name.changes, vec!["nullable".to_string()]);
}

#[test]
fn format_ir_type_is_stable() {
    assert_eq!(
        format_ir_type(&IRType::Varchar { length: Some(255) }),
        "Varchar(Some(255))"
    );
    assert_eq!(
        format_ir_type(&IRType::Varchar { length: None }),
        "Varchar(None)"
    );
    assert_eq!(
        format_ir_type(&IRType::Decimal {
            precision: 10,
            scale: 2
        }),
        "Decimal(10,2)"
    );
}

#[test]
fn raw_diff_still_flags_native_string_mismatch() {
    let src = TableSchema {
        table_name: "t".into(),
        columns: vec![ColumnSchema {
            name: "name".into(),
            data_type: "character varying(100)".into(),
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
    let tgt = TableSchema {
        table_name: "t".into(),
        columns: vec![ColumnSchema {
            name: "name".into(),
            data_type: "varchar(100)".into(),
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

    let diff = diff_table_schemas("t", &src, &tgt, None);
    assert_eq!(diff.changed.len(), 1);
    assert!(diff.changed[0].changes.contains(&"dataType".into()));
}

#[test]
fn diff_table_schemas_detects_added_removed_changed() {
    let src = table(
        "users",
        vec![
            col("id", "integer", false, true),
            col("name", "text", true, false),
            col("legacy", "text", true, false),
        ],
        vec!["id".into()],
    );
    let tgt = table(
        "users",
        vec![
            col("id", "integer", false, true),
            col("name", "varchar", true, false),
            col("email", "text", false, false),
        ],
        vec!["id".into()],
    );

    // Source = desired state: columns on src missing from tgt are "added" (to target).
    let diff = diff_table_schemas("users", &src, &tgt, None);
    assert_eq!(diff.added.len(), 1);
    assert_eq!(diff.added[0].name, "legacy");
    assert_eq!(diff.removed.len(), 1);
    assert_eq!(diff.removed[0].name, "email");
    assert_eq!(diff.changed.len(), 1);
    assert_eq!(diff.changed[0].name, "name");
    assert!(diff.changed[0].changes.contains(&"dataType".into()));
}

#[tokio::test]
async fn sync_task_crud_and_inspect() {
    use crate::data_sync::TableMappingStatus;
    use crate::testing::app_state::{sample_postgres_config, TestAppState};
    use chrono::Utc;

    let test = TestAppState::with_tables().await;
    test.store
        .save_connection(sample_postgres_config("src-cfg"))
        .await
        .unwrap();
    test.store
        .save_connection({
            let mut c = sample_postgres_config("tgt-cfg");
            c.name = "Target".into();
            c
        })
        .await
        .unwrap();

    let src_conn = test.connect_config("src-cfg").await;
    let tgt_conn = test.connect_config("tgt-cfg").await;

    let results = super::inspect_data_sync_impl(
        &test.state,
        src_conn.clone(),
        tgt_conn.clone(),
        None,
        None,
        None,
        None,
        &[],
    )
    .await
    .unwrap();
    assert!(results
        .iter()
        .any(|r| r.status == TableMappingStatus::Matched && r.source_table == "users"));

    let task = SyncTask {
        id: "task-1".into(),
        source_db_session_id: src_conn.clone(),
        target_db_session_id: tgt_conn,
        source_connection_id: "src-cfg".into(),
        target_connection_id: "tgt-cfg".into(),
        source_database: Some("app".into()),
        target_database: Some("app".into()),
        source_schema: None,
        target_schema: None,
        tables: vec!["users".into()],
        completed_tables: vec![],
        current_table: None,
        current_table_offset: 0,
        source_row_counts: [("users".to_string(), 2u64)].into_iter().collect(),
        strategy: "full".into(),
        status: "running".into(),
        error_message: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        resume_state: "unknown".into(),
    };
    save_sync_task_direct_impl(&test.state, task).await.unwrap();
    assert_eq!(get_sync_tasks_impl(&test.state).await.unwrap().len(), 1);

    let conflicts = check_sync_conflicts_impl(&test.state, "task-1".into())
        .await
        .unwrap();
    assert_eq!(conflicts["hasConflicts"], false);

    delete_sync_task_impl(&test.state, "task-1".into())
        .await
        .unwrap();
    assert!(get_sync_tasks_impl(&test.state).await.unwrap().is_empty());
}

#[tokio::test]
async fn compare_table_schemas_impl_returns_diff_for_table() {
    use crate::testing::app_state::{sample_postgres_config, TestAppState};

    let test = TestAppState::new().await;
    test.store
        .save_connection(sample_postgres_config("src"))
        .await
        .unwrap();
    test.store
        .save_connection(sample_postgres_config("tgt"))
        .await
        .unwrap();
    let src = test.connect_config("src").await;
    let tgt = test.connect_config("tgt").await;

    let schema_diff = compare_table_schemas_impl(
        &test.state,
        src.clone(),
        tgt.clone(),
        "users".into(),
        "users".into(),
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(schema_diff["table"], "users");
}

#[test]
fn legacy_transfer_ir_compare_ipc_removed() {
    let compare_src = include_str!("compare.rs");
    assert!(
        !compare_src.contains("compare_databases_impl"),
        "Transfer IR compare_databases must stay removed"
    );
    let sync_mod = include_str!("mod.rs");
    assert!(
        !sync_mod.contains("compare_databases"),
        "legacy compare_databases IPC must not be registered in sync mod"
    );
    assert!(
        !sync_mod.contains("sync_table"),
        "legacy sync_table IPC must not be registered in sync mod"
    );
    assert!(
        !sync_mod.contains("sync_tables"),
        "legacy sync_tables IPC must not be registered in sync mod"
    );
    assert!(
        !sync_mod.contains("classify_sync_pair"),
        "legacy classify_sync_pair IPC must stay removed"
    );
    assert!(
        sync_mod.contains("classify_data_sync_pair"),
        "classify_data_sync_pair IPC must be registered for single-source pairing"
    );
}

#[test]
fn filter_tables_by_schema_keeps_matching_schema_only() {
    use crate::db::{TableInfo, TableType};

    let tables = vec![
        TableInfo {
            schema: Some("public".into()),
            name: "a".into(),
            table_type: TableType::Table,
            row_count: None,
        },
        TableInfo {
            schema: Some("app".into()),
            name: "b".into(),
            table_type: TableType::Table,
            row_count: None,
        },
    ];
    let filtered = super::types::filter_tables_by_schema(tables, Some("public"));
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].name, "a");
}

#[test]
fn is_self_sync_requires_matching_schema_on_same_connection() {
    assert!(super::types::is_self_sync(
        "c1",
        "c1",
        "db",
        "db",
        Some("public"),
        Some("public")
    ));
    assert!(!super::types::is_self_sync(
        "c1",
        "c1",
        "db",
        "db",
        Some("public"),
        Some("app")
    ));
}

#[tokio::test]
async fn inspect_data_sync_returns_matched_tables() {
    use crate::data_sync::TableMappingStatus;
    use crate::testing::app_state::TestAppState;

    let test = TestAppState::with_tables().await;
    test.save_and_connect("src-ins").await;
    test.save_and_connect("tgt-ins").await;
    let src = test.connect_config("src-ins").await;
    let tgt = test.connect_config("tgt-ins").await;
    let results = super::inspect_data_sync_impl(&test.state, src, tgt, None, None, None, None, &[])
        .await
        .unwrap();
    assert!(results
        .iter()
        .any(|r| r.status == TableMappingStatus::Matched && r.source_table == "users"));
}

#[tokio::test]
async fn compare_data_sync_returns_an_opaque_server_plan() {
    use crate::testing::app_state::TestAppState;

    let test = TestAppState::new().await;
    test.save_and_connect("src-plan").await;
    test.save_and_connect("tgt-plan").await;
    let source = test.connect_config("src-plan").await;
    let target = test.connect_config("tgt-plan").await;
    let preview = super::compare_data_sync_impl(
        &test.state,
        source,
        target,
        Vec::new(),
        None,
        None,
        None,
        None,
        None,
        crate::data_sync::SyncOptions::default(),
        &[],
        &std::collections::HashMap::new(),
    )
    .await
    .unwrap();
    assert!(!preview.plan_id.is_empty());
    assert_eq!(preview.selection_revision, 1);
    assert_eq!(test.mock.open_transaction_count(), 0);
}

#[tokio::test]
async fn execute_data_sync_rejects_when_target_active_database_changes() {
    use crate::testing::app_state::TestAppState;

    let test = TestAppState::new().await;
    test.save_and_connect("src-plan-db").await;
    test.save_and_connect("tgt-plan-db").await;
    let source = test.connect_config("src-plan-db").await;
    let target = test.connect_config("tgt-plan-db").await;
    let preview = super::compare_data_sync_impl(
        &test.state,
        source,
        target.clone(),
        Vec::new(),
        None,
        None,
        None,
        None,
        None,
        crate::data_sync::SyncOptions::default(),
        &[],
        &std::collections::HashMap::new(),
    )
    .await
    .unwrap();

    test.state
        .connection_manager
        .set_active_database(&target, "other_database")
        .await
        .unwrap();
    let err = super::execute_data_sync_plan_impl(
        &test.state,
        super::plans::SyncRunRequest {
            plan_id: preview.plan_id,
            selection: super::plans::SyncRunSelection {
                revision: preview.selection_revision,
                rows: Vec::new(),
                scopes: Vec::new(),
            },
            options: crate::data_sync::SyncOptions::default(),
            job_id: None,
        },
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("active database"), "{err}");
}

#[tokio::test]
async fn execute_data_sync_rejects_read_only_target() {
    use crate::data_sync::{ChangeOperation, SqlStatement};
    use crate::testing::app_state::{sample_postgres_config, TestAppState};

    let test = TestAppState::new().await;
    test.registry
        .register_test_driver("postgresql", test.mock.clone())
        .await;
    let mut cfg = sample_postgres_config("ro-tgt");
    cfg.database_type = "postgresql".into();
    cfg.read_only = true;
    test.store.save_connection(cfg).await.unwrap();
    let id = test.connect_config("ro-tgt").await;
    let stmt = SqlStatement {
        table: "t".into(),
        operation: ChangeOperation::Insert,
        sql: "INSERT INTO t VALUES (1)".into(),
        preview_sql: "INSERT INTO t VALUES (1)".into(),
        parameters: vec![],
        row_key: vec![],
        identity_insert: None,
    };
    let err = super::execute_data_sync_impl(&test.state, id, vec![stmt], None, None)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("read-only"));
}

#[tokio::test]
async fn check_sync_conflicts_missing_task_errors() {
    use crate::testing::app_state::TestAppState;

    let test = TestAppState::new().await;
    assert!(check_sync_conflicts_impl(&test.state, "missing".into())
        .await
        .is_err());
}

#[tokio::test]
async fn check_sync_conflicts_ignores_stale_session_ids_and_reconnects_by_connection() {
    use crate::testing::app_state::{sample_postgres_config, TestAppState};
    use chrono::Utc;

    let test = TestAppState::with_tables().await;
    test.store
        .save_connection(sample_postgres_config("persisted-source"))
        .await
        .unwrap();
    test.store
        .save_connection(sample_postgres_config("persisted-target"))
        .await
        .unwrap();

    // Simulate a pre-fix file from a previous process. No runtime session is
    // opened before the command, so the only safe route is connectionId.
    let now = Utc::now();
    let legacy = serde_json::json!([{
        "id": "legacy-conflict-check",
        "sourceDbSessionId": "stale-source-session",
        "targetDbSessionId": "stale-target-session",
        "sourceConnectionId": "persisted-source",
        "targetConnectionId": "persisted-target",
        "sourceDatabase": "app",
        "targetDatabase": "app",
        "sourceSchema": null,
        "targetSchema": null,
        "tables": ["users"],
        "completedTables": [],
        "currentTable": null,
        "currentTableOffset": 0,
        "sourceRowCounts": {"users": 2},
        "strategy": "full",
        "status": "completed",
        "errorMessage": null,
        "createdAt": now,
        "updatedAt": now
    }]);
    tokio::fs::write(
        test.store.data_dir().join("sync_tasks.json"),
        serde_json::to_vec_pretty(&legacy).unwrap(),
    )
    .await
    .unwrap();

    let result = check_sync_conflicts_impl(&test.state, "legacy-conflict-check".into())
        .await
        .unwrap();
    assert_eq!(result["hasConflicts"], false);
}

#[tokio::test]
async fn check_sync_conflicts_reconnects_selected_database_with_override() {
    use crate::testing::app_state::{sample_postgres_config, TestAppState};
    use chrono::Utc;

    let test = TestAppState::with_tables().await;
    test.store
        .save_connection(sample_postgres_config("selected-source"))
        .await
        .unwrap();
    test.store
        .save_connection(sample_postgres_config("selected-target"))
        .await
        .unwrap();

    let now = Utc::now();
    let legacy = serde_json::json!([{
        "id": "selected-database-check",
        "sourceDbSessionId": "stale-source-session",
        "targetDbSessionId": "stale-target-session",
        "sourceConnectionId": "selected-source",
        "targetConnectionId": "selected-target",
        "sourceDatabase": "analytics",
        "targetDatabase": "app",
        "sourceSchema": null,
        "targetSchema": null,
        "tables": ["users"],
        "completedTables": [],
        "currentTable": null,
        "currentTableOffset": 0,
        "sourceRowCounts": {"users": 2},
        "strategy": "full",
        "status": "completed",
        "errorMessage": null,
        "createdAt": now,
        "updatedAt": now
    }]);
    tokio::fs::write(
        test.store.data_dir().join("sync_tasks.json"),
        serde_json::to_vec_pretty(&legacy).unwrap(),
    )
    .await
    .unwrap();

    for _ in 0..2 {
        let result = check_sync_conflicts_impl(&test.state, "selected-database-check".into())
            .await
            .unwrap();
        assert_eq!(result["hasConflicts"], false);
        assert_eq!(
            test.state.connection_manager.session_owner_map_len().await,
            0,
            "dedicated task sessions must be released after each check"
        );
    }
}

#[tokio::test]
async fn check_sync_conflicts_releases_selected_database_sessions_on_count_error() {
    use crate::testing::app_state::{rich_mock_options, sample_postgres_config, TestAppState};
    use chrono::Utc;

    let mut options = rich_mock_options();
    options.query_error = Some("count failed".into());
    let test = TestAppState::with_options(options).await;
    test.store
        .save_connection(sample_postgres_config("error-source"))
        .await
        .unwrap();
    test.store
        .save_connection(sample_postgres_config("error-target"))
        .await
        .unwrap();

    let now = Utc::now();
    let legacy = serde_json::json!([{
        "id": "count-error-check",
        "sourceDbSessionId": "stale-source-session",
        "targetDbSessionId": "stale-target-session",
        "sourceConnectionId": "error-source",
        "targetConnectionId": "error-target",
        "sourceDatabase": "analytics",
        "targetDatabase": "app",
        "tables": ["users"],
        "completedTables": [],
        "currentTable": null,
        "currentTableOffset": 0,
        "sourceRowCounts": {"users": 2},
        "strategy": "full",
        "status": "completed",
        "errorMessage": null,
        "createdAt": now,
        "updatedAt": now
    }]);
    tokio::fs::write(
        test.store.data_dir().join("sync_tasks.json"),
        serde_json::to_vec_pretty(&legacy).unwrap(),
    )
    .await
    .unwrap();

    let error = check_sync_conflicts_impl(&test.state, "count-error-check".into())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("count failed"));
    assert_eq!(
        test.state.connection_manager.session_owner_map_len().await,
        0,
        "count_rows errors must release both dedicated task sessions"
    );
}

#[tokio::test]
async fn cancel_data_sync_stops_execute_before_start() {
    use crate::data_sync::{ChangeOperation, SqlStatement};
    use crate::testing::app_state::{sample_postgres_config, TestAppState};

    let test = TestAppState::new().await;
    test.registry
        .register_test_driver("postgresql", test.mock.clone())
        .await;
    let mut cfg = sample_postgres_config("cancel-tgt");
    cfg.database_type = "postgresql".into();
    test.store.save_connection(cfg).await.unwrap();
    let id = test.connect_config("cancel-tgt").await;
    let job = format!("job-{}", uuid::Uuid::new_v4());
    assert!(super::cancel_job(&job).await);
    let stmt = SqlStatement {
        table: "t".into(),
        operation: ChangeOperation::Insert,
        sql: "INSERT INTO t VALUES (1)".into(),
        preview_sql: "INSERT INTO t VALUES (1)".into(),
        parameters: vec![],
        row_key: vec![],
        identity_insert: None,
    };
    let error = super::execute_data_sync_impl(&test.state, id, vec![stmt], Some(job), None)
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        crate::commands::error::CommandError::DataSyncNotStarted(message)
            if message.to_lowercase().contains("cancel")
    ));
}

#[tokio::test]
async fn compare_rejects_mock_driver_that_repeats_keyset_pages() {
    use crate::testing::app_state::{rich_mock_options, TestAppState};
    use crate::testing::mock_driver::MockDriverOptions;

    let test = TestAppState::with_options(MockDriverOptions {
        parameterized_writes: true,
        ..rich_mock_options()
    })
    .await;
    test.save_and_connect("src-cmp").await;
    test.save_and_connect("tgt-cmp").await;
    let src = test.connect_config("src-cmp").await;
    let tgt = test.connect_config("tgt-cmp").await;
    let err = super::compare_data_sync_impl(
        &test.state,
        src,
        tgt,
        vec!["users".into()],
        None,
        None,
        None,
        None,
        None,
        crate::data_sync::SyncOptions::default(),
        &[],
        &std::collections::HashMap::new(),
    )
    .await
    .unwrap_err();
    // The shared mock always returns the same page and cannot honor keyset WHERE.
    // This must fail rather than treating a repeated page as end-of-stream.
    assert!(err.to_string().contains("not strictly increasing"));
    assert_eq!(test.mock.open_transaction_count(), 0);
}

#[tokio::test]
async fn generated_binary_preview_uses_the_target_driver_literal_renderer() {
    use crate::data_sync::{RowChange, SyncOptions, TableResult};
    use crate::testing::app_state::{sample_postgres_config, TestAppState};
    use crate::testing::mock_driver::MockDriverOptions;

    let schema = table(
        "binary_rows",
        vec![
            col("id", "INT", false, true),
            col("payload", "BINARY", true, false),
        ],
        vec!["id".into()],
    );
    let test = TestAppState::with_options(MockDriverOptions {
        table_schema: Some(schema),
        parameterized_writes: true,
        ..MockDriverOptions::default()
    })
    .await;
    let bytes = vec![0, 255, 254];

    for (database_type, expected_literal) in [("postgresql", "'\\x00fffe'"), ("mysql", "X'00fffe'")]
    {
        test.registry
            .register_test_driver(database_type, test.mock.clone())
            .await;
        let connection_id = format!("binary-{database_type}");
        let mut config = sample_postgres_config(&connection_id);
        config.database_type = database_type.into();
        test.store.save_connection(config).await.unwrap();
        let target_db_session_id = test.connect_config(&connection_id).await;

        let options = SyncOptions::default();
        let mut result = TableResult::matched(
            "binary_rows",
            "binary_rows",
            vec![RowChange::insert(
                vec![Value::Integer(1)],
                vec![Some(Value::Integer(1)), Some(Value::Bytes(bytes.clone()))],
                &options,
            )],
        );
        result.columns = vec!["id".into(), "payload".into()];
        result.column_types = vec!["INT".into(), "BINARY".into()];
        result.primary_keys = vec!["id".into()];

        let statements = super::generate_data_sync_sql_impl(
            &test.state,
            target_db_session_id,
            vec![result],
            options.clone(),
            Some("app".into()),
            None,
        )
        .await
        .unwrap();
        assert_eq!(statements.len(), 1);
        assert!(statements[0].preview_sql.contains(expected_literal));
        assert!(!statements[0].preview_sql.contains('\u{fffd}'));
        assert!(!statements[0].preview_sql.contains('\0'));
        assert!(matches!(
            &statements[0].parameters[1],
            Value::Bytes(actual) if actual == &bytes
        ));
    }
}

#[tokio::test]
async fn legacy_apply_rejects_unreviewed_recomparison() {
    use crate::testing::app_state::TestAppState;

    let test = TestAppState::with_tables().await;
    test.save_and_connect("src-ap").await;
    test.save_and_connect("tgt-ap").await;
    let src = test.connect_config("src-ap").await;
    let tgt = test.connect_config("tgt-ap").await;
    let err = super::apply_data_sync_impl(
        &test.state,
        src,
        tgt,
        vec!["users".into()],
        None,
        None,
        None,
        None,
        None,
        crate::data_sync::SyncOptions::default(),
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("reviewed row selection"), "{err}");
}

#[test]
fn value_as_u64_accepts_int_float_and_numeric_string() {
    use super::compare::value_as_u64;
    assert_eq!(value_as_u64(&Value::Integer(12)), Some(12));
    assert_eq!(value_as_u64(&Value::Float(7.0)), Some(7));
    assert_eq!(value_as_u64(&Value::String("3".into())), Some(3));
    assert_eq!(value_as_u64(&Value::Integer(-1)), None);
}

#[test]
fn execution_ipc_response_preserves_all_four_evidence_outcomes_flattened() {
    use crate::commands::error::CommandError;
    use crate::data_sync::{ExecutionOutcome, ExecutionResult};

    let execution = || ExecutionResult {
        applied: 2,
        rolled_back: false,
        rollback_reason: None,
        affected_rows: 2,
        skipped: 0,
        conflicts: Vec::new(),
    };
    let cases = [
        (
            super::execution_response_from_result(Err(CommandError::DataSyncNotStarted(
                "preflight rejected".into(),
            ))),
            ExecutionOutcome::NotStarted,
        ),
        (
            super::execution_response_from_result(Ok(ExecutionResult {
                rolled_back: true,
                ..execution()
            })),
            ExecutionOutcome::RolledBack,
        ),
        (
            super::execution_response_from_result(Ok(execution())),
            ExecutionOutcome::Committed,
        ),
        (
            super::execution_response_from_result(Err(CommandError::DataSyncOutcomeUnknown(
                "commit acknowledgement lost".into(),
            ))),
            ExecutionOutcome::Unknown,
        ),
    ];

    for (response, expected) in cases {
        assert_eq!(response.outcome, expected);
        let json = serde_json::to_value(response).unwrap();
        assert_eq!(
            json["outcome"],
            match expected {
                ExecutionOutcome::NotStarted => "not_started",
                ExecutionOutcome::Committed => "committed",
                ExecutionOutcome::RolledBack => "rolled_back",
                ExecutionOutcome::Unknown => "unknown",
            }
        );
        assert!(
            json.get("result").is_none(),
            "legacy result fields stay flat"
        );
        assert!(json.get("applied").is_some());
    }
}

#[test]
fn test_tester_data_sync_error_conversion_keeps_not_started_and_unknown_distinct() {
    use crate::commands::error::CommandError;
    use crate::data_sync::DataSyncError;

    assert!(matches!(
        CommandError::from(DataSyncError::not_started("preflight rejected")),
        CommandError::DataSyncNotStarted(message) if message == "preflight rejected"
    ));
    assert!(matches!(
        CommandError::from(DataSyncError::outcome_unknown("commit acknowledgement lost")),
        CommandError::DataSyncOutcomeUnknown(message) if message == "commit acknowledgement lost"
    ));
    assert!(matches!(
        CommandError::from(DataSyncError::validation("invalid plan")),
        CommandError::Validation(message) if message == "invalid plan"
    ));
}

#[test]
fn confirmed_sync_cancellation_is_distinguished_before_safe_error_redaction() {
    use crate::commands::error::CommandError;
    use crate::data_sync::ExecutionResult;

    let (not_started, cancelled_before_start) =
        super::execution_response_and_cancelled(Err(CommandError::DataSyncNotStarted(
            "execute cancelled before any changes were applied".into(),
        )));
    assert_eq!(
        not_started.outcome,
        crate::data_sync::ExecutionOutcome::NotStarted
    );
    assert!(cancelled_before_start);
    assert_eq!(
        not_started.error.as_deref(),
        Some("Execution did not start. Check the plan and endpoint context, then compare again.")
    );

    let (rolled_back, cancelled_after_start) =
        super::execution_response_and_cancelled(Ok(ExecutionResult {
            applied: 1,
            rolled_back: true,
            rollback_reason: Some("execute cancelled; all changes were rolled back".into()),
            affected_rows: 1,
            skipped: 0,
            conflicts: Vec::new(),
        }));
    assert_eq!(
        rolled_back.outcome,
        crate::data_sync::ExecutionOutcome::RolledBack
    );
    assert!(cancelled_after_start);

    let (_, unknown_cancellation) =
        super::execution_response_and_cancelled(Err(CommandError::DataSyncOutcomeUnknown(
            "execute cancelled; rollback failed, outcome UNKNOWN".into(),
        )));
    assert!(!unknown_cancellation);
}
