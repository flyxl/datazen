//! Command-shell tests for `resolve_foreign_key_targets` (DB-07).
//!
//! The rules live in the shared layer and are tested there; these cases pin
//! the shell's own contract: the request envelope, the wire shape of the
//! answer, and the `grid.fk.*` prefixes the front end classifies on.

use std::collections::HashMap;

use datazen_driver_api::{ColumnSchema, ForeignKeyInfo, IndexInfo, TableSchema, Value};

use super::*;
use crate::testing::app_state::TestAppState;
use crate::testing::mock_driver::MockDriverOptions;

fn column(name: &str, data_type: &str, primary: bool) -> ColumnSchema {
    ColumnSchema {
        name: name.to_string(),
        data_type: data_type.to_string(),
        nullable: !primary,
        default_value: None,
        comment: None,
        is_primary_key: primary,
        is_auto_increment: false,
    }
}

fn schema(
    table_name: &str,
    columns: Vec<ColumnSchema>,
    primary_keys: Vec<String>,
    indexes: Vec<IndexInfo>,
    foreign_keys: Vec<ForeignKeyInfo>,
) -> TableSchema {
    TableSchema {
        table_name: table_name.to_string(),
        columns,
        primary_keys,
        indexes,
        foreign_keys,
        check_constraints: Vec::new(),
        table_options: Default::default(),
    }
}

fn request(
    db_session_id: &str,
    columns: &[&str],
    values: Vec<Value>,
) -> ResolveForeignKeyTargetsRequest {
    ResolveForeignKeyTargetsRequest {
        db_session_id: db_session_id.to_string(),
        database: None,
        schema: None,
        table: "orders".into(),
        qualified_table: None,
        columns: columns.iter().map(|value| value.to_string()).collect(),
        constraint_name: None,
        values,
        limit: None,
    }
}

#[tokio::test]
async fn resolve_foreign_key_targets_requires_a_db_session_id() {
    let test = TestAppState::new().await;
    let error = resolve_foreign_key_targets_impl(
        &test.state,
        request("", &["customer_id"], vec![Value::Integer(7)]),
    )
    .await
    .expect_err("an empty session id is refused");

    assert!(
        matches!(error, CommandError::Validation(ref message) if message.contains("dbSessionId")),
        "unexpected error: {error}"
    );
}

#[tokio::test]
async fn resolve_foreign_key_targets_maps_shape_mismatch_to_the_grid_prefix() {
    let test = TestAppState::new().await;

    let error = resolve_foreign_key_targets_impl(
        &test.state,
        request(
            "session-1",
            &["customer_id"],
            vec![Value::Integer(7), Value::Integer(8)],
        ),
    )
    .await
    .expect_err("mismatched lengths are refused");

    assert!(
        matches!(error, CommandError::Validation(ref message) if message.starts_with("grid.fk.shapeMismatch")),
        "unexpected error: {error}"
    );
}

#[tokio::test]
async fn resolve_foreign_key_targets_maps_value_null_to_the_grid_prefix() {
    let test = TestAppState::new().await;

    let error = resolve_foreign_key_targets_impl(
        &test.state,
        request("session-1", &["customer_id"], vec![Value::Null]),
    )
    .await
    .expect_err("a NULL value is refused");

    assert!(
        matches!(error, CommandError::Validation(ref message) if message.starts_with("grid.fk.valueNull")),
        "unexpected error: {error}"
    );
}

#[tokio::test]
async fn resolve_foreign_key_targets_resolves_a_target_through_the_mock_driver() {
    let source = schema(
        "orders",
        vec![
            column("id", "integer", true),
            column("customer_id", "integer", false),
        ],
        vec!["id".to_string()],
        Vec::new(),
        vec![ForeignKeyInfo {
            name: "orders_customer_fk".into(),
            columns: vec!["customer_id".into()],
            referenced_table: "parents".into(),
            referenced_columns: vec!["id".into()],
            on_update: "NO ACTION".into(),
            on_delete: "NO ACTION".into(),
            deferrability: Default::default(),
        }],
    );
    let target = schema(
        "parents",
        vec![column("id", "integer", true)],
        vec!["id".to_string()],
        Vec::new(),
        Vec::new(),
    );
    let mut tables = HashMap::new();
    tables.insert("orders".to_string(), source);
    tables.insert("parents".to_string(), target);
    let mut by_database = HashMap::new();
    by_database.insert("app".to_string(), tables);

    let test = TestAppState::with_options(MockDriverOptions {
        table_schemas_by_database: by_database,
        query_rows: vec![vec![Some(Value::Integer(7))]],
        ..Default::default()
    })
    .await;
    let (_, db_session_id) = test.save_and_connect("fk-nav").await;

    let resolution = resolve_foreign_key_targets_impl(
        &test.state,
        request(&db_session_id, &["customer_id"], vec![Value::Integer(7)]),
    )
    .await
    .expect("the target resolves");

    assert_eq!(resolution.target.table, "parents");
    assert_eq!(resolution.target.columns, vec!["id".to_string()]);
    assert_eq!(resolution.target.database, "app");
    assert!(resolution.target.unique);
    assert!(resolution.exists);
}
