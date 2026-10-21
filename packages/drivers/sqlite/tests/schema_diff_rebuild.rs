// Dialect integration tests: the driver owns registration and SQL rendering.
extern crate datazen_driver_sqlite;

use datazen_schema_diff::plan::*;
use datazen_schema_diff::types::*;

mod schema_diff_support;
use schema_diff_support::{col, schema};

#[test]
fn sqlite_alter_type_becomes_transaction_required_table_rebuild() {
    let src = schema(vec![col("id", "int")]);
    let mut target = src.clone();
    target.columns[0].data_type = "text".into();
    let expected_target = target.clone();
    let plan = build_schema_diff_plan(
        &[("users".into(), src, target)],
        "sqlite",
        "sqlite",
        PlanOptions::default(),
    );
    assert!(plan.requirements.is_empty(), "{:?}", plan.requirements);
    assert_eq!(plan.expected_target_schemas, vec![expected_target]);
    assert!(plan.statements.len() >= 5);
    assert!(plan
        .statements
        .iter()
        .all(|statement| statement.requires_transaction));
    assert!(plan.rollback_completeness.complete);
    assert!(plan.statements[0]
        .sql
        .contains("PRAGMA defer_foreign_keys = ON"));
}

#[test]
fn sqlite_rebuild_is_blocked_before_sql_when_target_catalog_is_not_round_trippable() {
    let src = schema(vec![col("id", "integer"), col("email", "text")]);
    let mut target = schema(vec![col("id", "integer"), col("email", "varchar(50)")]);
    target
        .table_options
        .migration_blockers
        .push("Trigger `users_audit` is outside the table schema snapshot".into());
    let plan = build_schema_diff_plan(
        &[("users".into(), src, target)],
        "sqlite",
        "sqlite",
        PlanOptions::default(),
    );
    assert!(plan.statements.is_empty());
    assert!(plan.requirements.iter().any(|requirement| matches!(
        requirement,
        PlanRequirement::Unsupported { reason, .. } if reason.contains("users_audit")
    )));
}
