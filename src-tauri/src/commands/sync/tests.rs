use super::compare::{
    compare_databases_impl, compare_table_data_impl, compare_table_schemas_impl,
    diff_table_schemas_ir, format_ir_type, resolve_pk_columns, row_key, row_to_json_map,
    rows_equal, rows_to_key_map, value_key_part, values_equal,
};
use super::table_sync::{sync_table_impl, sync_tables_impl};
use super::tasks::{
    check_sync_conflicts_impl, delete_sync_task_impl, get_sync_tasks_impl,
    save_sync_task_direct_impl,
};
use crate::schema_diff::diff_table_schemas;
use crate::store::SyncTask;
use crate::sync::ir::{IRColumn, IRTable, IRType};

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

#[test]
fn resolve_pk_columns_prefers_primary_keys_list() {
    let schema = table(
        "t",
        vec![col("a", "int", false, true), col("b", "int", false, false)],
        vec!["b".into()],
    );
    assert_eq!(resolve_pk_columns(&schema), vec!["b".to_string()]);
}

#[test]
fn resolve_pk_columns_falls_back_to_column_flags() {
    let schema = table(
        "t",
        vec![col("a", "int", false, true), col("b", "int", false, false)],
        vec![],
    );
    assert_eq!(resolve_pk_columns(&schema), vec!["a".to_string()]);
}

#[test]
fn row_key_uses_pk_values() {
    let cols = vec!["id".into(), "name".into()];
    let row = vec![
        Some(Value::Integer(42)),
        Some(Value::String("alice".into())),
    ];
    let key = row_key(&cols, &["id".into()], &row);
    assert!(key.contains("42"));
    assert!(!key.starts_with("h:"));
}

#[test]
fn row_key_hashes_when_no_pk() {
    let cols = vec!["name".into()];
    let row = vec![Some(Value::String("bob".into()))];
    let key = row_key(&cols, &[], &row);
    assert!(key.starts_with("h:"));
    assert_eq!(key, row_key(&cols, &[], &row));
}

#[test]
fn value_key_part_serializes_null_and_values() {
    assert_eq!(value_key_part(&None), "\\N");
    assert_eq!(
        value_key_part(&Some(Value::Integer(1))),
        serde_json::to_string(&Value::Integer(1)).unwrap()
    );
}

#[test]
fn rows_to_key_map_last_row_wins_duplicate_keys() {
    let cols = vec!["id".into()];
    let rows = vec![vec![Some(Value::Integer(1))], vec![Some(Value::Integer(1))]];
    let map = rows_to_key_map(&cols, &["id".into()], &rows);
    assert_eq!(map.len(), 1);
}

#[test]
fn row_to_json_map_aligns_columns() {
    let cols = vec!["id".into(), "name".into()];
    let row = vec![Some(Value::Integer(7)), Some(Value::String("x".into()))];
    let json = row_to_json_map(&cols, &row);
    assert_eq!(json["id"], 7);
    assert_eq!(json["name"], "x");
}

#[test]
fn rows_equal_requires_matching_values_for_all_source_columns() {
    let src_cols = vec!["id".into(), "name".into()];
    let tgt_cols = vec!["id".into(), "extra".into()];
    let src_row = vec![Some(Value::Integer(1)), Some(Value::String("a".into()))];
    let tgt_row = vec![
        Some(Value::Integer(1)),
        Some(Value::String("ignored".into())),
    ];
    assert!(!rows_equal(&src_cols, &src_row, &tgt_cols, &tgt_row));
}

#[test]
fn rows_equal_true_when_columns_align() {
    let cols = vec!["id".into(), "name".into()];
    let a = vec![Some(Value::Integer(1)), Some(Value::String("a".into()))];
    let b = vec![Some(Value::Integer(1)), Some(Value::String("a".into()))];
    assert!(rows_equal(&cols, &a, &cols, &b));
}

#[test]
fn rows_equal_detects_mismatch() {
    let cols = vec!["id".into()];
    let a = vec![Some(Value::Integer(1))];
    let b = vec![Some(Value::Integer(2))];
    assert!(!rows_equal(&cols, &a, &cols, &b));
}

#[test]
fn values_equal_treats_missing_and_null_as_distinct() {
    assert!(values_equal(Some(&None), Some(&None)));
    assert!(!values_equal(None, Some(&Some(Value::Null))));
    assert!(values_equal(
        Some(&Some(Value::String("a".into()))),
        Some(&Some(Value::String("a".into())))
    ));
}

#[tokio::test]
async fn sync_task_crud_and_compare() {
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

    let results = compare_databases_impl(&test.state, src_conn.clone(), tgt_conn.clone())
        .await
        .unwrap();
    assert!(!results.is_empty());
    assert_eq!(results[0]["status"], "identical");

    let task = SyncTask {
        id: "task-1".into(),
        source_connection_id: src_conn.clone(),
        target_connection_id: tgt_conn,
        source_config_id: "src-cfg".into(),
        target_config_id: "tgt-cfg".into(),
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
async fn compare_table_schemas_and_data_impl() {
    use crate::db::Value;
    use crate::testing::app_state::{sample_postgres_config, TestAppState};
    use crate::testing::mock_driver::MockDriverOptions;

    let opts = MockDriverOptions {
        count_total: 1,
        query_rows: vec![vec![
            Some(Value::Integer(1)),
            Some(Value::String("alice".into())),
        ]],
        ..Default::default()
    };
    let test = TestAppState::with_options(opts).await;
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

    let data_diff = compare_table_data_impl(&test.state, src, tgt, "users".into())
        .await
        .unwrap();
    assert_eq!(data_diff["table"], "users");
}

#[tokio::test]
async fn sync_table_impl_refuses_overwrite_copy() {
    use crate::testing::app_state::TestAppState;

    let test = TestAppState::new().await;
    let err = sync_table_impl(&test.state, "src".into(), "tgt".into(), "users".into())
        .await
        .unwrap_err();
    assert!(crate::data_sync::is_overwrite_copy_retired_message(
        &err.to_string()
    ));
}

#[tokio::test]
async fn sync_tables_impl_refuses_overwrite_copy() {
    use crate::testing::app_state::TestAppState;

    let test = TestAppState::new().await;
    let err = sync_tables_impl(
        &test.state,
        "task-1".into(),
        "src".into(),
        "tgt".into(),
        "src-cfg".into(),
        "tgt-cfg".into(),
        vec!["users".into()],
        vec![],
        "overwrite".into(),
        None,
        0,
    )
    .await
    .unwrap_err();
    assert!(crate::data_sync::is_overwrite_copy_retired_message(
        &err.to_string()
    ));
}

#[test]
fn table_sync_module_has_no_drop_insert_body() {
    let src = include_str!("table_sync.rs");
    assert!(
        !src.contains("DROP TABLE"),
        "overwrite-copy DROP TABLE body must be deleted"
    );
    assert!(
        !src.contains("sync_one_table"),
        "legacy sync_one_table must be deleted"
    );
    assert!(
        !src.contains("sync_table_impl_legacy"),
        "legacy sync_table_impl_legacy must be deleted"
    );
    assert!(
        src.contains("refuse_overwrite_copy"),
        "compat IPC must still refuse overwrite copy"
    );
}

#[test]
fn classify_sync_pair_rejects_ir_and_allows_mysql_family() {
    let mysql = super::classify_sync_pair("mysql".into(), "mariadb".into()).unwrap();
    assert_eq!(mysql["path"], "direct");
    assert_eq!(mysql["supported"], true);
    let ir = super::classify_sync_pair("postgresql".into(), "mysql".into()).unwrap();
    assert_eq!(ir["path"], "ir");
    assert_eq!(ir["supported"], false);
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
    let results = super::inspect_data_sync_impl(&test.state, src, tgt)
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
    let err = super::execute_data_sync_impl(&test.state, id, vec![stmt])
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
