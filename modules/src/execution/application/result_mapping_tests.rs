//! 保守结果映射的行为钉子（B4 归位时补齐——此前零测试）。
//!
//! 覆盖：`SELECT *` 候选识别的接受/拒绝面（引号标识、限定名、LIMIT/OFFSET
//! 尾巴、非白名单形态）与 `verify_result_mapping` 的确认守门（列对齐、
//! 主键覆盖、驱动 resolved 元数据形状）。

use super::{result_relation_candidate, verify_result_mapping};
use crate::execution::target::NamespaceTarget;
use crate::shared::id::ConnectionId;
use datazen_driver_api::{ColumnSchema, ColumnInfo, TableSchema};
use serde_json::json;

fn candidate(sql: &str) -> String {
    result_relation_candidate(sql)
        .expect("candidate expected")
        .relation_sql
}

fn rejects(sql: &str) {
    assert!(
        result_relation_candidate(sql).is_none(),
        "expected rejection: {sql}"
    );
}

// ── result_relation_candidate ──

#[test]
fn accepts_simple_wildcard_select() {
    assert_eq!(candidate("SELECT * FROM users"), "users");
    assert_eq!(candidate("select * from users"), "users");
    assert_eq!(candidate("  select * from users  "), "users");
}

#[test]
fn accepts_limit_and_offset_tails_without_capturing_them() {
    assert_eq!(candidate("SELECT * FROM users LIMIT 10"), "users");
    assert_eq!(
        candidate("SELECT * FROM public.users LIMIT 10 OFFSET 20;"),
        "public.users"
    );
}

#[test]
fn accepts_quoted_and_multi_part_names() {
    assert_eq!(candidate(r#"SELECT * FROM "app"."users""#), r#""app"."users""#);
    assert_eq!(
        candidate("SELECT * FROM db.app.users LIMIT 1"),
        "db.app.users"
    );
    // 引号内转义（"" 表面引号）原样保留。
    assert_eq!(candidate(r#"SELECT * FROM "weird""name""#), r#""weird""name""#);
    // 反引号（MySQL 风格）与方括号（SQL Server 风格）。
    assert_eq!(candidate("SELECT * FROM `app`.`users`"), "`app`.`users`");
    assert_eq!(candidate("SELECT * FROM [app].[users]"), "[app].[users]");
}

#[test]
fn rejects_non_wildcard_or_extra_select_forms() {
    rejects("UPDATE users SET id = 1");
    rejects("SELECT id FROM users");
    rejects("SELECT a, b FROM users");
    rejects("SELECT * FROM users u JOIN orders o ON u.id = o.user_id");
    rejects("SELECT * FROM users WHERE id = 1");
    rejects("SELECT * FROM a.b.c.d");
    rejects("SELECT * FROM users LIMIT 'ten'");
    rejects("SELECT * FROM users OFFSET 10");
    rejects("INSERT INTO users VALUES (1)");
    rejects("");
    rejects("* FROM users");
}

#[test]
fn rejects_malformed_relation_paths() {
    // 尾点悬挂：candidate 解析不出闭合标识。
    rejects("SELECT * FROM users.");
    // 空引号名。
    rejects(r#"SELECT * FROM """#);
    // 非法起始字符。
    rejects("SELECT * FROM 9users");
}

// ── verify_result_mapping ──

fn pk_schema() -> TableSchema {
    TableSchema {
        table_name: "users".into(),
        columns: vec![
            ColumnSchema {
                name: "id".into(),
                data_type: "int".into(),
                nullable: false,
                default_value: None,
                is_primary_key: true,
                is_auto_increment: false,
                comment: None,
            },
            ColumnSchema {
                name: "name".into(),
                data_type: "text".into(),
                nullable: true,
                default_value: None,
                is_primary_key: false,
                is_auto_increment: false,
                comment: None,
            },
        ],
        primary_keys: vec!["id".into()],
        indexes: vec![],
        foreign_keys: vec![],
        check_constraints: vec![],
        table_options: Default::default(),
    }
}

fn columns(names: &[&str]) -> Vec<ColumnInfo> {
    names
        .iter()
        .map(|name| ColumnInfo {
            name: name.to_string(),
            data_type: "int".into(),
            nullable: false,
        })
        .collect()
}

fn resolved() -> serde_json::Value {
    json!({"database": "app", "schema": "public", "table": "users"})
}

#[test]
fn verify_accepts_matching_columns_with_primary_key_coverage() {
    let target = verify_result_mapping(
        ConnectionId::new("conn-1"),
        &columns(&["id", "name"]),
        &pk_schema(),
        &resolved(),
    )
    .expect("mapping verified");
    assert_eq!(target.namespace.database.as_deref(), Some("app"));
    assert_eq!(target.namespace.schema.as_deref(), Some("public"));
    assert_eq!(
        target.object.as_ref().map(|o| (o.kind.as_str(), o.name.as_str())),
        Some(("relation", "users"))
    );
}

#[test]
fn verify_accepts_null_schema_as_unnamed_namespace() {
    let mut resolved = resolved();
    resolved["schema"] = serde_json::Value::Null;
    let target = verify_result_mapping(
        ConnectionId::new("conn-1"),
        &columns(&["id", "name"]),
        &pk_schema(),
        &resolved,
    )
    .expect("mapping verified");
    assert_eq!(target.namespace.schema, None);
}

#[test]
fn verify_rejects_column_shape_mismatch() {
    // 列名不一致。
    assert!(verify_result_mapping(
        ConnectionId::new("c"),
        &columns(&["id", "other"]),
        &pk_schema(),
        &resolved()
    )
    .is_none());
    // 列数不一致。
    assert!(verify_result_mapping(
        ConnectionId::new("c"),
        &columns(&["id"]),
        &pk_schema(),
        &resolved()
    )
    .is_none());
    // 空列。
    assert!(verify_result_mapping(
        ConnectionId::new("c"),
        &columns(&[]),
        &pk_schema(),
        &resolved()
    )
    .is_none());
    // 重复列名。
    assert!(verify_result_mapping(
        ConnectionId::new("c"),
        &columns(&["id", "id"]),
        &pk_schema(),
        &resolved()
    )
    .is_none());
}

#[test]
fn verify_rejects_missing_primary_key_coverage() {
    // 主键列不在结果里 → 拒绝（结果不可定位行）。
    let schema = pk_schema();
    let mut no_pk = schema.clone();
    no_pk.primary_keys = vec!["ghost".into()];
    assert!(verify_result_mapping(
        ConnectionId::new("c"),
        &columns(&["id", "name"]),
        &no_pk,
        &resolved()
    )
    .is_none());
}

#[test]
fn verify_rejects_bad_resolved_metadata() {
    // 缺 database / table。
    assert!(verify_result_mapping(
        ConnectionId::new("c"),
        &columns(&["id", "name"]),
        &pk_schema(),
        &json!({"table": "users"})
    )
    .is_none());
    // 空串。
    assert!(verify_result_mapping(
        ConnectionId::new("c"),
        &columns(&["id", "name"]),
        &pk_schema(),
        &json!({"database": "", "table": "users"})
    )
    .is_none());
    // schema 既非 Null 也非非空 String。
    assert!(verify_result_mapping(
        ConnectionId::new("c"),
        &columns(&["id", "name"]),
        &pk_schema(),
        &json!({"database": "app", "schema": "", "table": "users"})
    )
    .is_none());
    assert!(verify_result_mapping(
        ConnectionId::new("c"),
        &columns(&["id", "name"]),
        &pk_schema(),
        &json!({"database": "app", "schema": 42, "table": "users"})
    )
    .is_none());
}

#[test]
fn verify_target_namespace_carries_only_database_and_schema() {
    // 元数据里多余的键（如 search_path 细节）不得进入 target。
    let mut resolved = resolved();
    resolved["extra"] = json!({"search_path": "pg_temp"});
    let target = verify_result_mapping(
        ConnectionId::new("c"),
        &columns(&["id", "name"]),
        &pk_schema(),
        &resolved,
    )
    .expect("mapping verified");
    assert_eq!(
        target.namespace,
        NamespaceTarget::new(
            Some("app".into()),
            None,
            Some("public".into()),
            Vec::new()
        )
    );
}
