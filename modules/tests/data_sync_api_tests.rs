//! B2 sync 域刀测试归位（commands-layer-review §1）：原住桌面
//! `commands/sync/tests.rs` / `sync/exec.rs` 内联测试里的**纯逻辑**用例，
//! 被测本体已在 `datazen::migration::data_sync::api`，随刀迁来。
//!
//! 判据复述：把宿主从 Tauri 换成 axum，这些测试原样通过——它们不碰
//! `AppState`、会话或任何宿主端口，位置就该在 modules。宿主侧保留的是
//! C1（线格式/错误信封映射）与 C4（经真实命令入口的适配器烟测）。

use std::collections::HashMap;

use datazen::migration::data_sync::api::{
    append_bounded_preview_statement, filter_tables_by_schema, finalize_inspect_results,
    is_self_sync, revalidation_report, validate_active_database, SQL_PREVIEW_IPC_MAX_BYTES,
};
use datazen::migration::data_sync::{
    ChangeOperation, SqlStatement, SyncSourceFilter, TableMapping, TableMappingStatus,
    TableResult,
};
use datazen_driver_api::{ColumnSchema, TableInfo, TableSchema, TableType, Value};

// ------------------------------------------------------------------ 迁移用例

/// 迁自桌面 `sync/tests.rs::filter_tables_by_schema_keeps_matching_schema_only`。
#[test]
fn filter_tables_by_schema_keeps_matching_schema_only() {
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
    let filtered = filter_tables_by_schema(tables, Some("public"));
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].name, "a");
}

/// 迁自桌面 `sync/tests.rs::is_self_sync_requires_matching_schema_on_same_connection`。
#[test]
fn is_self_sync_requires_matching_schema_on_same_connection() {
    assert!(is_self_sync(
        "c1",
        "c1",
        "db",
        "db",
        Some("public"),
        Some("public")
    ));
    assert!(!is_self_sync(
        "c1",
        "c1",
        "db",
        "db",
        Some("public"),
        Some("app")
    ));
}

/// 迁自桌面 `sync/tests.rs::execute_data_sync_rejects_when_target_active_database_changes`
/// （原 `pub(super) validate_active_database` 随 B2 sync 域刀下沉 api）。
#[test]
fn validate_active_database_rejects_when_the_side_drifts() {
    let error = validate_active_database(Some("other_database"), "app", "target").unwrap_err();
    // 文案是 IPC 契约（宿主 From<DataSyncError> 原样透传成 Validation）。
    assert_eq!(
        error.to_string(),
        "target session active database changed or cannot be confirmed; return to comparison"
    );
    // 空白/缺失的活动库都无法确认，同样拒绝。
    assert!(validate_active_database(Some("  "), "app", "source").is_err());
    assert!(validate_active_database(None, "app", "source").is_err());
    // 计划侧空白也拒绝（比较时不可能冻结出空库）。
    assert!(validate_active_database(Some("app"), "   ", "source").is_err());
    assert!(validate_active_database(Some(" app "), "app", "source").is_ok());
}

/// 迁自桌面 `sync/exec.rs::sql_preview_rejects_a_statement_that_exceeds_the_ipc_bound`。
#[test]
fn sql_preview_rejects_a_statement_that_exceeds_the_ipc_bound() {
    let statement = sample_statement(vec![Value::String("x".repeat(
        SQL_PREVIEW_IPC_MAX_BYTES + 1,
    ))]);
    let mut statements = Vec::new();
    let mut response_bytes = 2;
    let error = append_bounded_preview_statement(&mut statements, &mut response_bytes, statement)
        .unwrap_err();

    assert!(error.to_string().contains("16 MiB IPC limit"));
    assert!(error.to_string().contains("select fewer rows"));
    assert!(statements.is_empty());
}

/// 迁自桌面 `sync/exec.rs::test_tester_sql_preview_counts_each_statement_and_json_separator`。
#[test]
fn sql_preview_counts_each_statement_and_json_separator() {
    let first = sample_statement(vec![Value::String("first".into())]);
    let second = SqlStatement {
        parameters: vec![Value::String("second".into())],
        row_key: vec![Value::Integer(2)],
        ..first.clone()
    };
    let expected_bytes = 2
        + serde_json::to_vec(&first).unwrap().len()
        + 1
        + serde_json::to_vec(&second).unwrap().len();
    let mut statements = Vec::new();
    let mut response_bytes = 2;

    append_bounded_preview_statement(&mut statements, &mut response_bytes, first).unwrap();
    append_bounded_preview_statement(&mut statements, &mut response_bytes, second).unwrap();

    assert_eq!(statements.len(), 2);
    assert_eq!(response_bytes, expected_bytes);
}

// ------------------------------------------------------------------ 新增覆盖

/// B2 下沉物的载荷键契约：`ok` / `staleTables`（`sourceTable` / `targetTable`
/// / `status` / `reason`），空选集视作全表复核。
#[test]
fn revalidation_report_lists_only_stale_rows_and_treats_empty_selection_as_all() {
    let mut matched = TableResult::matched("users", "users", Vec::new());
    matched.status = TableMappingStatus::Matched;
    let mut incompatible = TableResult::matched("orders", "orders", Vec::new());
    incompatible.status = TableMappingStatus::Incompatible;
    incompatible.incompatible_reason = Some("missing primary key".into());
    let mut unmapped = TableResult::matched("logs", "logs", Vec::new());
    unmapped.status = TableMappingStatus::UnmappedSource;

    // 空选集：全部行都复核。
    let all = revalidation_report(Vec::new(), vec![matched.clone(), incompatible.clone()]);
    assert_eq!(all["ok"], false);
    let stale = all["staleTables"].as_array().unwrap();
    assert_eq!(stale.len(), 2, "matched rows never count as stale");

    // 指定选集：只复核提到的表（source 或 target 名均可命中）。
    let scoped = revalidation_report(
        vec!["orders".into()],
        vec![matched, incompatible, unmapped],
    );
    assert_eq!(scoped["ok"], false);
    let stale = scoped["staleTables"].as_array().unwrap();
    assert_eq!(stale.len(), 1);
    assert_eq!(stale[0]["sourceTable"], "orders");
    assert_eq!(stale[0]["targetTable"], "orders");
    assert_eq!(stale[0]["status"], "INCOMPATIBLE");
    assert_eq!(stale[0]["reason"], "missing primary key");

    // 全部 Matched ⇒ ok=true 且 staleTables 为空数组（键必须始终在场）。
    let clean = TableResult::matched("users", "users", Vec::new());
    let report = revalidation_report(vec!["users".into()], vec![clean]);
    assert_eq!(report["ok"], true);
    assert_eq!(report["staleTables"].as_array().unwrap().len(), 0);
}

/// B2 下沉物：`classify_tables` 之后的收尾投影——Matched 行补列投影/主键/
/// 结构化过滤，Incompatible 行补 SQL Server 预检失败原因（固定文案）。
#[test]
fn finalize_inspect_results_enriches_matched_and_attaches_preflight_reasons() {
    let schema = TableSchema {
        table_name: "users".into(),
        columns: vec![
            column("id", "int", true),
            column("name", "text", false),
        ],
        primary_keys: vec!["id".into()],
        indexes: vec![],
        foreign_keys: vec![],
        check_constraints: vec![],
        table_options: Default::default(),
    };
    let mut source_schemas = HashMap::new();
    source_schemas.insert("users".to_string(), schema);

    let mut mapping = TableMapping::auto("users");
    mapping.source_filter = Some(SyncSourceFilter(serde_json::json!({})));
    let mappings = vec![mapping];

    let matched = TableResult::matched("users", "users", Vec::new());
    assert!(matched.columns.is_empty(), "fixture: no projection yet");

    let mut broken = TableResult::matched("legacy", "legacy", Vec::new());
    broken.status = TableMappingStatus::Incompatible;

    let mut source_errors = HashMap::new();
    source_errors.insert("legacy".to_string(), "catalog read timed out".to_string());
    let mut target_errors = HashMap::new();
    target_errors.insert("legacy".to_string(), "login failed".to_string());

    let finalized = finalize_inspect_results(
        vec![matched, broken],
        &mappings,
        &source_schemas,
        &source_errors,
        &target_errors,
    );

    assert_eq!(finalized.len(), 2);
    assert_eq!(finalized[0].status, TableMappingStatus::Matched);
    assert_eq!(finalized[0].columns, vec!["id".to_string(), "name".to_string()]);
    assert_eq!(finalized[0].column_types, vec!["int".to_string(), "text".to_string()]);
    assert_eq!(finalized[0].primary_keys, vec!["id".to_string()]);
    assert!(finalized[0].source_filter.is_some(), "structured filter rides the mapping");

    assert_eq!(finalized[1].status, TableMappingStatus::Incompatible);
    assert_eq!(
        finalized[1].incompatible_reason.as_deref(),
        Some("SQL Server schema preflight failed; source: catalog read timed out; target: login failed"),
        "the preflight reason is fixed wording, never raw driver diagnostics echoed per side"
    );
}

/// 只有源侧预检失败时，原因单独成文（固定文案的另一支）。
#[test]
fn finalize_inspect_results_attaches_source_only_prefailures() {
    let mut row = TableResult::matched("legacy", "legacy", Vec::new());
    row.status = TableMappingStatus::Incompatible;
    let mut source_errors = HashMap::new();
    source_errors.insert("legacy".to_string(), "catalog read timed out".to_string());

    let finalized = finalize_inspect_results(
        vec![row],
        &[],
        &HashMap::new(),
        &source_errors,
        &HashMap::new(),
    );
    assert_eq!(
        finalized[0].incompatible_reason.as_deref(),
        Some("SQL Server source schema preflight failed: catalog read timed out")
    );

    let mut target_errors = HashMap::new();
    target_errors.insert("legacy".to_string(), "login failed".to_string());
    let finalized = finalize_inspect_results(
        vec![TableResult::matched("legacy", "legacy", Vec::new())],
        &[],
        &HashMap::new(),
        &HashMap::new(),
        &target_errors,
    );
    // 预检原因只挂在 Incompatible 行上：Matched 行即使有错误记录也不改写。
    assert_eq!(finalized[0].status, TableMappingStatus::Matched);
    assert!(finalized[0].incompatible_reason.is_none());
}

// -------------------------------------------------------------------- 夹具

fn column(name: &str, data_type: &str, pk: bool) -> ColumnSchema {
    ColumnSchema {
        name: name.into(),
        data_type: data_type.into(),
        nullable: true,
        default_value: None,
        comment: None,
        is_primary_key: pk,
        is_auto_increment: false,
    }
}

fn sample_statement(parameters: Vec<Value>) -> SqlStatement {
    SqlStatement {
        table: "users".into(),
        operation: ChangeOperation::Insert,
        sql: "INSERT INTO users VALUES (?)".into(),
        preview_sql: "INSERT INTO users VALUES (?)".into(),
        parameters,
        row_key: vec![Value::Integer(1)],
        identity_insert: None,
    }
}
