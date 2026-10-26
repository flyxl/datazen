// Dialect integration tests: the driver owns registration and SQL rendering.
extern crate datazen_driver_mysql;

use datazen_modules::schema_diff::plan::*;

mod schema_diff_support;
use schema_diff_support::{col, schema};

#[test]
fn type_mapping_uses_current_table_even_when_columns_and_source_types_match() {
    let mapper = |table: &str, _ty: &str, _column: &str| -> Result<String, String> {
        Ok(if table == "a" {
            "VARCHAR(128)"
        } else {
            "VARCHAR(512)"
        }
        .into())
    };
    for names in [["a", "b"], ["b", "a"]] {
        let pairs: Vec<_> = names
            .iter()
            .map(|name| {
                (
                    name.to_string(),
                    schema(vec![col("payload", "text")]),
                    schema(vec![]),
                )
            })
            .collect();
        let plan = build_schema_diff_plan(
            &pairs,
            "postgresql",
            "mysql",
            PlanOptions {
                type_mapper: Some(&mapper),
                ..Default::default()
            },
        );
        assert!(plan
            .statements
            .iter()
            .any(|s| s.sql.contains("`a`") && s.sql.contains("VARCHAR(128)")));
        assert!(plan
            .statements
            .iter()
            .any(|s| s.sql.contains("`b`") && s.sql.contains("VARCHAR(512)")));
    }
}
