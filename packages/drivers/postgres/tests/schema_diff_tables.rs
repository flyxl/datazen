// Dialect integration tests: the driver owns registration and SQL rendering.
extern crate datazen_driver_postgres;

use datazen_driver_api::{ForeignKeyDeferrability, ForeignKeyInfo};
use datazen_schema_diff::plan::*;
use datazen_schema_diff::types::*;

mod schema_diff_support;
use schema_diff_support::{col, schema};

#[test]
fn cross_dialect_table_options_are_skipped_with_a_review_warning() {
    let mut src = schema(vec![col("id", "int")]);
    src.table_options.engine = Some("InnoDB".into());
    src.table_options.charset = Some("utf8mb4".into());
    src.table_options.comment = Some("source catalog".into());
    let tgt = schema(vec![col("id", "int")]);

    let plan = build_schema_diff_plan(
        &[("users".into(), src, tgt)],
        "mysql",
        "postgresql",
        PlanOptions {
            allow_destructive: false,
            include_indexes: false,
            type_mapper: None,
            cross_dialect: true,
        },
    );

    assert!(plan.requirements.is_empty(), "{:?}", plan.requirements);
    assert!(plan.statements.is_empty());
    assert!(plan.warnings.iter().any(|warning| {
        warning.contains("table options") && warning.contains("remain unchanged")
    }));
}

#[test]
fn missing_target_table_plans_create_not_add_column() {
    let mut src = schema(vec![col("id", "int"), col("email", "varchar(255)")]);
    src.primary_keys = vec!["id".into()];
    let tgt = schema(vec![]);
    let plan = build_column_plan("public.users", &src, &tgt, "postgresql").unwrap();
    assert!(plan
        .statements
        .iter()
        .any(|s| s.sql.contains("CREATE TABLE") && s.sql.contains("email")));
    assert!(!plan.statements.iter().any(|s| s.sql.contains("ADD COLUMN")));
    assert!(!plan
        .statements
        .iter()
        .any(|s| s.sql.contains("ADD PRIMARY KEY")));
}

#[test]
fn target_only_table_is_blocked_without_destructive_approval() {
    let src = schema(vec![]);
    let tgt = schema(vec![col("id", "integer")]);
    let plan = build_schema_diff_plan(
        &[("archive".into(), src, tgt)],
        "postgresql",
        "postgresql",
        PlanOptions {
            allow_destructive: false,
            include_indexes: true,
            type_mapper: None,
            cross_dialect: false,
        },
    );

    assert!(plan.statements.is_empty());
    assert!(plan
        .warnings
        .iter()
        .any(|warning| warning.contains("table:archive")));
}

#[test]
fn approved_target_only_table_has_no_rollback_and_requires_review() {
    let src = schema(vec![]);
    let tgt = schema(vec![col("id", "integer")]);
    let plan = build_schema_diff_plan(
        &[("audit.events".into(), src, tgt)],
        "postgresql",
        "postgresql",
        PlanOptions {
            allow_destructive: true,
            include_indexes: true,
            type_mapper: None,
            cross_dialect: false,
        },
    );

    assert_eq!(plan.statements.len(), 1);
    let statement = &plan.statements[0];
    assert_eq!(statement.sql, "DROP TABLE \"audit\".\"events\"");
    assert_eq!(statement.risk, StatementRisk::Destructive);
    assert!(statement.rollback_sql.is_none());
    assert!(!plan.rollback_completeness.complete);
    assert!(plan.rollback_completeness.missing[0].contains("DROP TABLE"));
}

#[test]
fn test_tester_target_only_empty_identifier_is_not_executable() {
    let src = schema(vec![]);
    let tgt = schema(vec![col("id", "integer")]);
    let plan = build_schema_diff_plan(
        &[(String::new(), src, tgt)],
        "postgresql",
        "postgresql",
        PlanOptions {
            allow_destructive: true,
            include_indexes: true,
            type_mapper: None,
            cross_dialect: false,
        },
    );

    assert!(plan.statements.is_empty());
    assert!(plan.requirements.iter().any(|requirement| {
        matches!(
            requirement,
            datazen_schema_diff::types::PlanRequirement::Unsupported { .. }
        )
    }));
}

#[test]
fn target_only_blank_control_or_invalid_identifier_is_not_executable() {
    let src = schema(vec![]);
    let tgt = schema(vec![col("id", "integer")]);
    for table in [" ", "audit\nevents", "audit..events", "audit. events"] {
        let plan = build_schema_diff_plan(
            &[(table.into(), src.clone(), tgt.clone())],
            "postgresql",
            "postgresql",
            PlanOptions {
                allow_destructive: true,
                include_indexes: true,
                type_mapper: None,
                cross_dialect: false,
            },
        );

        assert!(plan.statements.is_empty(), "{table:?}");
        assert!(
            plan.requirements.iter().any(|requirement| {
                matches!(
                    requirement,
                    datazen_schema_diff::types::PlanRequirement::Unsupported { .. }
                )
            }),
            "{table:?}"
        );
    }
}

#[test]
fn explicit_target_only_picker_does_not_invent_source_snapshot() {
    let source = schema(vec![col("id", "integer")]);
    let target = schema(vec![col("id", "integer")]);
    let target_catalog = vec![
        ("users".into(), target.clone()),
        ("archive".into(), schema(vec![col("id", "integer")])),
    ];
    let plan = build_schema_diff_plan_with_target_only_catalog(
        &[("users".into(), source, target)],
        &["archive".into()],
        Some(&target_catalog),
        "postgresql",
        "postgresql",
        PlanOptions {
            allow_destructive: true,
            include_indexes: true,
            type_mapper: None,
            cross_dialect: false,
        },
    );

    assert_eq!(plan.tables, vec!["users", "archive"]);
    assert!(plan
        .statements
        .iter()
        .any(|statement| statement.sql == "DROP TABLE \"archive\""));
    assert!(!plan
        .statements
        .iter()
        .any(|statement| statement.sql.contains("DROP TABLE \"users\"")));
}

#[test]
fn explicit_target_only_picker_keeps_drop_rejected_by_default() {
    let plan = build_schema_diff_plan_with_target_only(
        &[],
        &["archive".into()],
        "postgresql",
        "postgresql",
        PlanOptions {
            allow_destructive: false,
            include_indexes: true,
            type_mapper: None,
            cross_dialect: false,
        },
    );

    assert!(plan.statements.is_empty());
    assert!(plan
        .warnings
        .iter()
        .any(|warning| warning.contains("table:archive")));
}

#[test]
fn postgres_add_varchar_column() {
    let src = schema(vec![col("id", "int"), col("email", "varchar(255)")]);
    let tgt = schema(vec![col("id", "int")]);
    let plan = build_column_plan("users", &src, &tgt, "postgresql").unwrap();
    assert!(plan
        .statements
        .iter()
        .any(|s| { s.sql.contains("ADD COLUMN") && s.sql.contains("email") }));
}

#[test]
fn additive_default_skips_drop() {
    let src = schema(vec![col("id", "int")]);
    let tgt = schema(vec![col("id", "int"), col("legacy", "text")]);
    let plan = build_schema_diff_plan(
        &[("users".into(), src, tgt)],
        "postgresql",
        "postgresql",
        PlanOptions {
            allow_destructive: false,
            include_indexes: false,
            type_mapper: None,
            cross_dialect: false,
        },
    );
    assert!(plan.statements.is_empty());
    assert!(plan
        .warnings
        .iter()
        .any(|w| w.contains("Skipped destructive")));
}

#[test]
fn multi_table_concatenates() {
    let src_a = schema(vec![col("id", "int"), col("a", "text")]);
    let tgt_a = schema(vec![col("id", "int")]);
    let src_b = schema(vec![col("id", "int"), col("b", "text")]);
    let tgt_b = schema(vec![col("id", "int")]);
    let plan = build_schema_diff_plan(
        &[("t_a".into(), src_a, tgt_a), ("t_b".into(), src_b, tgt_b)],
        "postgresql",
        "postgresql",
        PlanOptions::default(),
    );
    assert_eq!(plan.tables.len(), 2);
    assert!(plan.statements.len() >= 2);
}

#[test]
fn foreign_keys_are_emitted_after_all_table_definitions() {
    let mut orders = schema(vec![col("id", "int"), col("user_id", "int")]);
    orders.foreign_keys.push(ForeignKeyInfo {
        name: "orders_user_id_fk".into(),
        columns: vec!["user_id".into()],
        referenced_table: "users".into(),
        referenced_columns: vec!["id".into()],
        on_update: "CASCADE".into(),
        on_delete: "RESTRICT".into(),
        deferrability: ForeignKeyDeferrability::NotDeferrable,
    });
    let users = schema(vec![col("id", "int")]);
    let plan = build_schema_diff_plan(
        &[
            ("orders".into(), orders, schema(vec![])),
            ("users".into(), users, schema(vec![])),
        ],
        "postgresql",
        "postgresql",
        PlanOptions::default(),
    );
    let fk = plan
        .statements
        .iter()
        .position(|statement| statement.summary.starts_with("ADD FOREIGN KEY"))
        .expect("foreign key statement");
    assert!(
        plan.statements[..fk]
            .iter()
            .filter(|statement| statement.sql.starts_with("CREATE TABLE"))
            .count()
            >= 2
    );
}
