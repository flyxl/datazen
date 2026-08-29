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
    }
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
    };

    let diff = diff_table_schemas("t", &src, &tgt);
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
    let diff = diff_table_schemas("users", &src, &tgt);
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

    let schema_diff =
        compare_table_schemas_impl(&test.state, src.clone(), tgt.clone(), "users".into())
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
    };
    let err = super::execute_data_sync_impl(&test.state, id, vec![stmt], Some(job), None)
        .await
        .unwrap_err();
    assert!(err.to_string().to_lowercase().contains("cancel"), "{err}");
}

#[tokio::test]
async fn compare_data_sync_fills_row_diff_for_matched_tables() {
    use crate::data_sync::TableMappingStatus;
    use crate::testing::app_state::TestAppState;

    let test = TestAppState::with_tables().await;
    test.save_and_connect("src-cmp").await;
    test.save_and_connect("tgt-cmp").await;
    let src = test.connect_config("src-cmp").await;
    let tgt = test.connect_config("tgt-cmp").await;
    let results = super::compare_data_sync_impl(
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
    )
    .await
    .unwrap();
    let users = results
        .iter()
        .find(|r| r.source_table == "users")
        .expect("users");
    assert_eq!(users.status, TableMappingStatus::Matched);
    assert!(!users.rows.is_empty());
}

#[tokio::test]
async fn apply_data_sync_rejects_empty_change_set() {
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
    assert!(
        err.to_string().to_lowercase().contains("empty")
            || err.to_string().to_lowercase().contains("nothing"),
        "{err}"
    );
}

#[test]
fn value_as_u64_accepts_int_float_and_numeric_string() {
    use super::compare::value_as_u64;
    assert_eq!(value_as_u64(&Value::Integer(12)), Some(12));
    assert_eq!(value_as_u64(&Value::Float(7.0)), Some(7));
    assert_eq!(value_as_u64(&Value::String("3".into())), Some(3));
    assert_eq!(value_as_u64(&Value::Integer(-1)), None);
}
