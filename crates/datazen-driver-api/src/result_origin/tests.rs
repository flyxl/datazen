//! Tests for the result-origin contract and its shared reader.

use crate::mock_driver::{MockDriver, MockDriverOptions};
use crate::result_origin::{
    ResultObjectKind, ResultOrigin, ResultShape, SetOperation, UndeterminableReason,
};
use crate::types::{ConnectionHandle, TableInfo, TableType};
use crate::{DatabaseDriver, SqlTarget};

fn handle() -> ConnectionHandle {
    ConnectionHandle {
        id: "session-1".to_string(),
        pool_id: "pool-1".to_string(),
    }
}

fn columns(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| name.to_string()).collect()
}

fn detail(origin: &ResultOrigin) -> &crate::result_origin::ResultOriginDetail {
    match origin {
        ResultOrigin::Determined(detail) => detail,
        ResultOrigin::Undeterminable(reason) => {
            panic!("expected a determined origin, got {reason:?}")
        }
    }
}

#[tokio::test]
async fn default_impl_is_undeterminable() {
    let driver = MockDriver::new("sqlite", MockDriverOptions::default());
    let origin = driver
        .describe_result_origin(
            &handle(),
            SqlTarget::new(None, None),
            &columns(&["id"]),
            "SELECT id FROM orders",
        )
        .await
        .expect("the default implementation answers, it does not fail");

    assert_eq!(
        origin,
        ResultOrigin::Undeterminable(UndeterminableReason::NotImplemented)
    );
}

#[test]
fn result_origin_serde_roundtrip() {
    let undeterminable = ResultOrigin::Undeterminable(UndeterminableReason::UnsupportedDialect);
    let json = serde_json::to_string(&undeterminable).expect("serializable");
    assert_eq!(
        json,
        r#"{"kind":"undeterminable","detail":"unsupportedDialect"}"#
    );
    assert_eq!(
        serde_json::from_str::<ResultOrigin>(&json).expect("deserializable"),
        undeterminable
    );

    let determined = ResultOrigin::Determined(crate::result_origin::ResultOriginDetail {
        shape: ResultShape::SingleBaseTable,
        table: Some(crate::result_origin::ResultTableRef {
            catalog: None,
            schema: Some("public".to_string()),
            name: "orders".to_string(),
            aliased: false,
        }),
        referenced_tables: 1,
        deduplicated: false,
        aggregated: false,
        set_operation: None,
        has_derived_source: false,
        object_kind: ResultObjectKind::BaseTable,
        columns: vec![crate::result_origin::ResultColumnOrigin {
            column: "id".to_string(),
            passthrough: true,
            table: None,
            source_column: Some("id".to_string()),
        }],
    });
    let json = serde_json::to_string(&determined).expect("serializable");
    assert!(json.starts_with(r#"{"kind":"determined","detail":{"#));
    assert_eq!(
        serde_json::from_str::<ResultOrigin>(&json).expect("deserializable"),
        determined
    );
}

#[test]
fn a_single_base_table_reports_passthrough_columns() {
    let origin = crate::result_origin::analyze_select_origin(
        "SELECT id, total FROM orders",
        &columns(&["id", "total"]),
    );
    let detail = detail(&origin);

    assert_eq!(detail.shape, ResultShape::SingleBaseTable);
    assert_eq!(detail.referenced_tables, 1);
    assert!(!detail.deduplicated);
    assert!(!detail.aggregated);
    assert_eq!(detail.set_operation, None);
    assert!(!detail.has_derived_source);
    assert_eq!(
        detail.table.as_ref().map(|table| table.name.as_str()),
        Some("orders")
    );
    assert_eq!(detail.columns.len(), 2);
    assert!(detail.columns.iter().all(|column| column.passthrough));
    assert_eq!(
        detail.columns[1].source_column.as_deref(),
        Some("total"),
        "the real column name is what a write-back statement may use"
    );
}

#[test]
fn a_join_is_more_than_one_relation() {
    let origin = crate::result_origin::analyze_select_origin(
        "SELECT o.id FROM orders o JOIN customers c ON c.id = o.customer_id",
        &columns(&["id"]),
    );
    let detail = detail(&origin);

    assert_eq!(detail.shape, ResultShape::MultiTable);
    assert_eq!(detail.referenced_tables, 2);
    assert!(detail.table.is_none());
}

#[test]
fn an_implicit_comma_join_is_more_than_one_relation() {
    let origin =
        crate::result_origin::analyze_select_origin("SELECT id FROM a, b", &columns(&["id"]));
    assert_eq!(detail(&origin).shape, ResultShape::MultiTable);
}

#[test]
fn an_aggregate_is_reported_as_aggregated() {
    let origin = crate::result_origin::analyze_select_origin(
        "SELECT customer_id, COUNT(*) FROM orders GROUP BY customer_id",
        &columns(&["customer_id", "count"]),
    );
    let detail = detail(&origin);

    assert!(detail.aggregated);
    assert_eq!(detail.shape, ResultShape::SingleBaseTable);
}

#[test]
fn a_window_function_is_reported_as_aggregated() {
    let origin = crate::result_origin::analyze_select_origin(
        "SELECT id, ROW_NUMBER() OVER (ORDER BY id) FROM orders",
        &columns(&["id", "row_number"]),
    );
    assert!(detail(&origin).aggregated);
}

#[test]
fn distinct_is_reported_as_deduplicated() {
    let origin = crate::result_origin::analyze_select_origin(
        "SELECT DISTINCT id FROM orders",
        &columns(&["id"]),
    );
    assert!(detail(&origin).deduplicated);
}

#[test]
fn a_union_is_reported_as_a_set_operation() {
    let origin = crate::result_origin::analyze_select_origin(
        "SELECT id FROM orders UNION SELECT id FROM archive",
        &columns(&["id"]),
    );
    let detail = detail(&origin);

    assert_eq!(detail.set_operation, Some(SetOperation::Union));
    assert_ne!(detail.shape, ResultShape::SingleBaseTable);
}

#[test]
fn a_derived_table_is_reported_as_a_derived_source() {
    let origin = crate::result_origin::analyze_select_origin(
        "SELECT id FROM (SELECT id FROM orders) t",
        &columns(&["id"]),
    );
    let detail = detail(&origin);

    assert!(detail.has_derived_source);
    assert_ne!(detail.shape, ResultShape::SingleBaseTable);
}

#[test]
fn a_subquery_in_the_projection_is_a_derived_source() {
    let origin = crate::result_origin::analyze_select_origin(
        "SELECT id, (SELECT MAX(total) FROM archive) AS peak FROM orders",
        &columns(&["id", "peak"]),
    );
    let detail = detail(&origin);

    assert!(detail.has_derived_source);
    assert!(
        !detail.columns[1].passthrough,
        "a computed column is never a straight copy of a base column"
    );
}

#[test]
fn a_subquery_in_the_where_clause_does_not_change_the_column_sources() {
    let origin = crate::result_origin::analyze_select_origin(
        "SELECT id FROM orders WHERE customer_id IN (SELECT id FROM customers)",
        &columns(&["id"]),
    );
    let detail = detail(&origin);

    assert!(
        !detail.has_derived_source,
        "a WHERE subquery leaves the column sources untouched"
    );
    assert_eq!(detail.shape, ResultShape::SingleBaseTable);
    assert!(detail.columns[0].passthrough);
}

#[test]
fn an_alias_keeps_the_real_table_name_and_the_real_column_name() {
    let origin = crate::result_origin::analyze_select_origin(
        "SELECT o.id AS order_id, o.total AS amount FROM orders o",
        &columns(&["order_id", "amount"]),
    );
    let detail = detail(&origin);

    let table = detail.table.as_ref().expect("a single base table");
    assert_eq!(
        table.name, "orders",
        "the alias never becomes the table name"
    );
    assert_eq!(
        detail.columns[0].source_column.as_deref(),
        Some("id"),
        "the alias never becomes the column name either"
    );
    assert_eq!(detail.columns[1].source_column.as_deref(), Some("total"));
}

#[test]
fn a_star_expands_to_the_columns_the_result_reports() {
    let origin = crate::result_origin::analyze_select_origin(
        "SELECT * FROM orders",
        &columns(&["id", "total"]),
    );
    let detail = detail(&origin);

    assert_eq!(detail.columns.len(), 2);
    assert!(detail.columns.iter().all(|column| column.passthrough));
    assert_eq!(detail.columns[0].source_column.as_deref(), Some("id"));
}

#[test]
fn an_expression_column_is_never_a_passthrough() {
    let origin = crate::result_origin::analyze_select_origin(
        "SELECT id, total * 2 AS doubled FROM orders",
        &columns(&["id", "doubled"]),
    );
    let detail = detail(&origin);

    assert!(detail.columns[0].passthrough);
    assert!(!detail.columns[1].passthrough);
    assert_eq!(detail.columns[1].source_column, None);
}

#[test]
fn a_column_of_another_relation_is_not_a_passthrough() {
    let origin = crate::result_origin::analyze_select_origin(
        "SELECT other.id FROM orders",
        &columns(&["id"]),
    );
    let detail = detail(&origin);

    assert!(
        !detail.columns[0].passthrough,
        "a qualified column of an unknown relation cannot be attributed"
    );
}

#[test]
fn an_unreadable_statement_is_undeterminable() {
    let origin = crate::result_origin::analyze_select_origin("SELECT * FROM", &columns(&["id"]));
    assert_eq!(
        origin,
        ResultOrigin::Undeterminable(UndeterminableReason::UnsupportedDialect)
    );
}

#[test]
fn a_statement_that_is_not_a_select_is_undeterminable() {
    let origin = crate::result_origin::analyze_select_origin("DELETE FROM orders", &columns(&[]));
    assert_eq!(
        origin,
        ResultOrigin::Undeterminable(UndeterminableReason::UnsupportedDialect)
    );
}

#[tokio::test]
async fn the_catalog_confirms_that_the_relation_is_a_base_table() {
    let options = MockDriverOptions {
        tables: vec![TableInfo {
            name: "orders".to_string(),
            schema: None,
            table_type: TableType::Table,
            row_count: None,
        }],
        ..MockDriverOptions::default()
    };
    let driver = MockDriver::new("sqlite", options);

    let origin = crate::result_origin::describe_result_origin_with_catalog(
        driver.as_ref(),
        &handle(),
        SqlTarget::new(None, None),
        &columns(&["id"]),
        "SELECT id FROM orders",
    )
    .await
    .expect("the catalog read answers");

    assert_eq!(detail(&origin).object_kind, ResultObjectKind::BaseTable);
}

#[tokio::test]
async fn the_catalog_reports_a_view_as_a_view() {
    let options = MockDriverOptions {
        tables: vec![TableInfo {
            name: "orders".to_string(),
            schema: None,
            table_type: TableType::View,
            row_count: None,
        }],
        ..MockDriverOptions::default()
    };
    let driver = MockDriver::new("sqlite", options);

    let origin = crate::result_origin::describe_result_origin_with_catalog(
        driver.as_ref(),
        &handle(),
        SqlTarget::new(None, None),
        &columns(&["id"]),
        "SELECT id FROM orders",
    )
    .await
    .expect("the catalog read answers");

    assert_eq!(detail(&origin).object_kind, ResultObjectKind::View);
}

#[tokio::test]
async fn an_unreadable_catalog_leaves_the_kind_unknown() {
    // The mock reports no tables at all, so the relation cannot be confirmed.
    let driver = MockDriver::new("sqlite", MockDriverOptions::default());

    let origin = crate::result_origin::describe_result_origin_with_catalog(
        driver.as_ref(),
        &handle(),
        SqlTarget::new(None, None),
        &columns(&["id"]),
        "SELECT id FROM orders",
    )
    .await
    .expect("the catalog read answers");

    assert_eq!(
        detail(&origin).object_kind,
        ResultObjectKind::Unknown,
        "an unconfirmed relation is never guessed to be a base table"
    );
}
