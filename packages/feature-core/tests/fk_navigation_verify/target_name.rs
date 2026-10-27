//! The target name and namespace the resolver reports.
//!
//! Part of the DB-07 verification harness; the shared fixtures live in
//! the parent module.

use super::*;

// ─────────────────────────────────────────────────────────────────────────────
// FK-D · the target name must be the driver's normalized name, and the
// namespace must be the one the reference names — a schema-qualified name
// cannot survive the driver's own qualification step.
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn the_target_table_name_is_the_driver_normalized_name_not_the_raw_reference() {
    // PostgreSQL reports `referenced_table` as "{schema}.{table}" and its
    // `TableSchema.table_name` is the **bare** name. The resolver has to
    // report the driver's normalized name: the probe addresses the table by
    // it, and `FROM "public.parents"` would be qualified into
    // `"public"."public.parents"`, a relation that cannot exist.
    let mut tables = HashMap::new();
    tables.insert(
        "orders".to_string(),
        schema(
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
                "public.parents",
                &["id"],
            )],
        ),
    );
    // The driver resolves the qualified reference and reports the bare name.
    tables.insert(
        "public.parents".to_string(),
        schema(
            "parents",
            vec![column("id", "integer", true)],
            vec!["id".to_string()],
            Vec::new(),
            Vec::new(),
        ),
    );
    let driver = MockDriver::new("postgres", options_with(tables, Vec::new()));

    let resolution = resolve_foreign_key_targets(
        &TestSource::new(driver),
        &request(&["customer_id"], vec![Value::Integer(7)]),
    )
    .await
    .expect("the target resolves");

    // The catalogued (driver-normalized) name is "parents".
    assert_eq!(resolution.target.table, "parents");
    // And the namespace is the one the reference names.
    assert_eq!(resolution.target.schema.as_deref(), Some("public"));
}

#[tokio::test]
async fn the_target_namespace_is_the_referenced_one_not_the_source_tables() {
    // The source table lives in `public`, the target in `sales`: the target's
    // namespace is the one the reference names, not the source table's.
    let mut tables = HashMap::new();
    tables.insert(
        "orders".to_string(),
        schema(
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
                "sales.parents",
                &["id"],
            )],
        ),
    );
    tables.insert(
        "sales.parents".to_string(),
        schema(
            "parents",
            vec![column("id", "integer", true)],
            vec!["id".to_string()],
            Vec::new(),
            Vec::new(),
        ),
    );
    let mut options = options_with(tables, Vec::new());
    options.has_schema_level = true;
    options.default_schema = Some("public");
    let driver = MockDriver::new("postgres", options);

    let resolution = resolve_foreign_key_targets(
        &TestSource::new(driver),
        &request(&["customer_id"], vec![Value::Integer(7)]),
    )
    .await
    .expect("the target resolves");

    assert_eq!(resolution.target.table, "parents");
    assert_eq!(resolution.target.schema.as_deref(), Some("sales"));
}

#[test]
fn a_schema_qualified_target_name_cannot_survive_the_drivers_qualification() {
    use datazen_driver_api::sql_execution::target::{qualify_sql_with, QualifierQuote};
    use sqlparser::dialect::PostgreSqlDialect;

    // The probe SQL the resolver builds for a PostgreSQL target whose
    // `referenced_table` is schema-qualified: the table name is quoted as one
    // identifier, so the dot stays inside the quotes.
    let sql = r#"SELECT "id" FROM "public.customers" WHERE "id" = 7 LIMIT 51"#;
    let out = qualify_sql_with(
        &PostgreSqlDialect {},
        QualifierQuote::DoubleQuote,
        &["public"],
        sql,
    );

    // The driver's own qualification step turns the quoted dotted name into a
    // two-part relation that cannot exist in the catalog.
    assert!(
        out.sql.contains(r#""public"."public.customers""#),
        "the qualified name is re-qualified: {}",
        out.sql
    );

    // The bare name is what the driver can actually resolve.
    let bare = r#"SELECT "id" FROM "customers" WHERE "id" = 7 LIMIT 51"#;
    let out = qualify_sql_with(
        &PostgreSqlDialect {},
        QualifierQuote::DoubleQuote,
        &["public"],
        bare,
    );
    assert!(
        out.sql.contains(r#"FROM "public"."customers""#),
        "{}",
        out.sql
    );
}

#[test]
fn the_probe_sql_of_a_qualified_target_quotes_the_dotted_name_as_one_identifier() {
    use datazen_driver_api::dialect::LegacySqlDialect;
    use datazen_feature_core::services::query_executor::QueryExecutor;

    let driver = MockDriver::new("postgres", MockDriverOptions::default());
    let dialect = LegacySqlDialect::new(driver.as_ref());
    let sql = QueryExecutor::build_select_sql(
        "public.customers",
        &[column("id", "integer", true)],
        Some(vec![datazen_driver_api::filters::FilterCondition {
            column: "id".into(),
            operator: datazen_driver_api::filters::FilterOperator::Eq,
            value: Value::Integer(7),
            connector: None,
        }]),
        None,
        &[],
        &dialect,
        51,
        0,
        Some("and"),
    )
    .expect("the probe SQL is expressible");

    assert!(
        sql.contains(r#"FROM "public.customers""#),
        "the dotted name is one quoted identifier: {sql}"
    );
}
