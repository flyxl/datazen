//! The target-name, probe-value and truncation contract of the resolver.
//!
//! These cases pin the three facts that are easy to lose: the target is
//! addressed by the **driver's** name, a key value never travels back
//! through an error message, and the candidate list is truncated only when
//! it really was cut off.

use super::*;

/// A value that must never reach an error message.
const SENTINEL: &str = "SENTINEL-VALUE-42";

/// The catalog of a source table whose foreign key points at
/// `referenced_table`, with the target registered under `catalog_name`.
fn catalog_with(
    referenced_table: &str,
    catalog_name: &str,
    target_table_name: &str,
) -> HashMap<String, TableSchema> {
    let source = schema(
        "orders",
        vec![
            column("id", "integer", true),
            column("customer_id", "integer", false),
        ],
        vec!["id".to_string()],
        Vec::new(),
        vec![foreign_key(
            "orders_customer_fk",
            &["customer_id"],
            referenced_table,
            &["id"],
        )],
    );
    let target = schema(
        target_table_name,
        vec![column("id", "integer", true)],
        vec!["id".to_string()],
        Vec::new(),
        Vec::new(),
    );
    let mut tables = HashMap::new();
    tables.insert("orders".to_string(), source);
    tables.insert(catalog_name.to_string(), target);
    tables
}

#[tokio::test]
async fn resolve_fk_targets_reports_the_driver_normalized_target_name() {
    // A driver that folds the namespace into the table name (PostgreSQL
    // reports `public.parents`) answers with the **bare** name, and the
    // probe has to address the table by that name: `FROM "public.parents"`
    // would be qualified into `"public"."public.parents"`, a relation that
    // cannot exist.
    let tables = catalog_with("public.parents", "public.parents", "parents");
    let driver = MockDriver::new("postgres", options_with(tables, Vec::new()));
    let source = TestSource::new(driver);

    let resolution =
        resolve_foreign_key_targets(&source, &request(&["customer_id"], vec![Value::Integer(7)]))
            .await
            .expect("a schema-qualified reference resolves");

    assert_eq!(resolution.target.table, "parents");
    assert_eq!(resolution.target.schema.as_deref(), Some("public"));
}

#[tokio::test]
async fn resolve_fk_targets_prefers_the_referenced_namespace_over_the_source_one() {
    // The source table lives in `public`, the target in `sales`: the target's
    // namespace is the one the reference names, not the source table's.
    let tables = catalog_with("sales.parents", "sales.parents", "parents");
    let mut options = options_with(tables, Vec::new());
    options.has_schema_level = true;
    options.default_schema = Some("public");
    let driver = MockDriver::new("postgres", options);
    let source = TestSource::new(driver);

    let resolution =
        resolve_foreign_key_targets(&source, &request(&["customer_id"], vec![Value::Integer(7)]))
            .await
            .expect("a cross-schema reference resolves");

    assert_eq!(resolution.target.table, "parents");
    assert_eq!(resolution.target.schema.as_deref(), Some("sales"));
}

#[tokio::test]
async fn resolve_fk_targets_probe_failure_does_not_echo_the_key_value() {
    // A server-side type error quotes the value it choked on. That value is
    // the user's own data, so it must not travel back through IPC.
    let tables = catalog_with("parents", "parents", "parents");
    let mut options = options_with(tables, Vec::new());
    options.query_error = Some(format!(
        "Incorrect integer value: '{SENTINEL}' for column 'id'"
    ));
    let driver = MockDriver::new("postgres", options);
    let source = TestSource::new(driver);

    let error = resolve_foreign_key_targets(
        &source,
        &request(&["customer_id"], vec![Value::String(SENTINEL.into())]),
    )
    .await
    .expect_err("a failing probe is reported");

    let message = error.to_string();
    assert!(
        message.starts_with("grid.fk.probeFailed"),
        "unexpected message: {message}"
    );
    assert!(
        message.contains("parents"),
        "the message must name the probed table: {message}"
    );
    assert!(
        !message.contains(SENTINEL),
        "the message must not echo the key value: {message}"
    );
}

#[tokio::test]
async fn resolve_fk_targets_binds_the_probe_values_instead_of_inlining_them() {
    // A driver with a bound-parameter vocabulary gets the values as
    // parameters, so a key value never reaches the SQL text — and never
    // reaches a driver error message either. The mock proves it: it answers a
    // keyed read with the matching row only, which it can only do when the
    // value arrived as a parameter.
    let tables = catalog_with("parents", "parents", "parents");
    let mut options = options_with(
        tables,
        vec![
            vec![Some(Value::Integer(7))],
            vec![Some(Value::Integer(99))],
        ],
    );
    options.parameterized_writes = true;
    options.filter_rows_by_key_equality = true;
    let driver = MockDriver::new("postgres", options);
    let source = TestSource::new(driver);

    let resolution =
        resolve_foreign_key_targets(&source, &request(&["customer_id"], vec![Value::Integer(7)]))
            .await
            .expect("the probe resolves");

    assert_eq!(
        resolution.candidates.len(),
        1,
        "only the row whose key equals the bound value comes back"
    );
    assert_eq!(cells(&resolution.candidates[0]), vec![serde_json::json!(7)]);
}

#[tokio::test]
async fn resolve_fk_targets_exactly_the_limit_rows_is_not_truncated() {
    let rows: Vec<Vec<Option<Value>>> = (0..3)
        .map(|index| vec![Some(Value::Integer(index))])
        .collect();
    let driver = driver_with(
        foreign_key("orders_customer_fk", &["customer_id"], "parents", &["id"]),
        single_column_target(),
        rows,
    );
    let source = TestSource::new(driver);
    let mut request = request(&["customer_id"], vec![Value::Integer(7)]);
    request.limit = Some(3);

    let resolution = resolve_foreign_key_targets(&source, &request)
        .await
        .expect("a full page still resolves");

    assert_eq!(resolution.candidates.len(), 3);
    assert!(
        !resolution.truncated,
        "exactly the limit rows were not cut off by the limit"
    );
}

#[tokio::test]
async fn resolve_fk_targets_one_row_over_the_limit_is_truncated() {
    let rows: Vec<Vec<Option<Value>>> = (0..4)
        .map(|index| vec![Some(Value::Integer(index))])
        .collect();
    let driver = driver_with(
        foreign_key("orders_customer_fk", &["customer_id"], "parents", &["id"]),
        single_column_target(),
        rows,
    );
    let source = TestSource::new(driver);
    let mut request = request(&["customer_id"], vec![Value::Integer(7)]);
    request.limit = Some(3);

    let resolution = resolve_foreign_key_targets(&source, &request)
        .await
        .expect("an over-sized page still resolves");

    assert_eq!(resolution.candidates.len(), 3);
    assert!(
        resolution.truncated,
        "one row over the limit means the list was cut off"
    );
}
