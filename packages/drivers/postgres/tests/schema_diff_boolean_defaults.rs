// Dialect integration tests: the driver owns registration and SQL rendering.
extern crate datazen_driver_postgres;

use datazen_modules::schema_diff::plan::*;
use datazen_modules::schema_diff::types::*;

mod schema_diff_support;
use schema_diff_support::{col, schema};

#[test]
fn unknown_changed_mysql_boolean_default_blocks_postgres_default_ddl() {
    let mut source_active = col("active", "tinyint(1)");
    source_active.default_value = Some("2".into());
    let source = schema(vec![source_active]);
    let mut target_active = col("active", "boolean");
    target_active.default_value = Some("TRUE".into());
    let target = schema(vec![target_active]);
    let mapper = |_table: &str, ty: &str, _name: &str| -> Result<String, String> {
        if ty.to_ascii_lowercase().starts_with("tinyint(1)") {
            Ok("boolean".into())
        } else {
            Ok(ty.into())
        }
    };

    let plan = build_schema_diff_plan(
        &[("users".into(), source, target)],
        "mysql",
        "postgresql",
        PlanOptions {
            type_mapper: Some(&mapper),
            ..Default::default()
        },
    );

    assert!(plan.requirements.iter().any(|requirement| matches!(
        requirement,
        PlanRequirement::Unsupported { operation, reason }
            if operation.contains("active") && reason.contains("`2`")
    )));
    assert!(!plan
        .statements
        .iter()
        .any(|statement| statement.sql.contains("SET DEFAULT 2")));
}

#[test]
fn explicit_non_boolean_mysql_type_override_keeps_numeric_default() {
    let mut active = col("active", "tinyint(1)");
    active.default_value = Some("1".into());
    let src = schema(vec![col("id", "int"), active]);
    let tgt = schema(vec![col("id", "int")]);
    let mapper = |_table: &str, ty: &str, name: &str| -> Result<String, String> {
        if name == "active" {
            Ok("smallint".into())
        } else {
            Ok(ty.into())
        }
    };

    let plan = build_schema_diff_plan(
        &[("users".into(), src, tgt)],
        "mysql",
        "postgresql",
        PlanOptions {
            type_mapper: Some(&mapper),
            ..Default::default()
        },
    );

    assert!(plan.requirements.is_empty(), "{:?}", plan.requirements);
    assert!(plan
        .statements
        .iter()
        .any(|statement| { statement.sql.contains("\"active\" smallint DEFAULT 1") }));
    assert!(!plan
        .statements
        .iter()
        .any(|statement| statement.sql.contains("DEFAULT TRUE")));
}

#[test]
fn mysql_boolean_type_change_stages_defaults_around_postgres_alter_type() {
    let mapper = |_table: &str, ty: &str, _name: &str| -> Result<String, String> {
        if ty.to_ascii_lowercase().starts_with("tinyint(1)") {
            Ok("boolean".into())
        } else {
            Ok(ty.into())
        }
    };

    // Raw defaults match, but their meaning changes with the mapped type.
    let mut source_active = col("active", "tinyint(1)");
    source_active.default_value = Some("1".into());
    let mut target_active = col("active", "integer");
    target_active.default_value = Some("1".into());
    let plan = build_schema_diff_plan(
        &[(
            "users".into(),
            schema(vec![source_active.clone()]),
            schema(vec![target_active]),
        )],
        "mysql",
        "postgresql",
        PlanOptions {
            type_mapper: Some(&mapper),
            ..Default::default()
        },
    );

    assert!(plan.requirements.is_empty(), "{:?}", plan.requirements);
    let drop_default = plan
        .statements
        .iter()
        .position(|statement| statement.sql.contains("DROP DEFAULT"))
        .expect("old integer default is dropped");
    let alter_type = plan
        .statements
        .iter()
        .position(|statement| statement.sql.contains("TYPE boolean"))
        .expect("column is changed to boolean");
    let set_default = plan
        .statements
        .iter()
        .position(|statement| statement.sql.contains("SET DEFAULT TRUE"))
        .expect("mapped boolean default is installed");
    assert!(drop_default < alter_type && alter_type < set_default);

    // If the target has no default, there is nothing to drop, but the desired
    // source default still follows the type change.
    let plan_without_old_default = build_schema_diff_plan(
        &[(
            "users".into(),
            schema(vec![source_active]),
            schema(vec![col("active", "integer")]),
        )],
        "mysql",
        "postgresql",
        PlanOptions {
            type_mapper: Some(&mapper),
            ..Default::default()
        },
    );
    assert!(
        plan_without_old_default.requirements.is_empty(),
        "{:?}",
        plan_without_old_default.requirements
    );
    let alter_type = plan_without_old_default
        .statements
        .iter()
        .position(|statement| statement.sql.contains("TYPE boolean"))
        .expect("column is changed to boolean");
    let set_default = plan_without_old_default
        .statements
        .iter()
        .position(|statement| statement.sql.contains("SET DEFAULT TRUE"))
        .expect("mapped boolean default is installed");
    assert!(alter_type < set_default);
    assert!(!plan_without_old_default
        .statements
        .iter()
        .any(|statement| statement.sql.contains("DROP DEFAULT")));
}

#[test]
fn mysql_boolean_type_change_removes_old_default_before_alter_when_source_has_none() {
    let mut target_active = col("active", "integer");
    target_active.default_value = Some("1".into());
    let mapper = |_table: &str, ty: &str, _name: &str| -> Result<String, String> {
        if ty.to_ascii_lowercase().starts_with("tinyint(1)") {
            Ok("boolean".into())
        } else {
            Ok(ty.into())
        }
    };
    let plan = build_schema_diff_plan(
        &[(
            "users".into(),
            schema(vec![col("active", "tinyint(1)")]),
            schema(vec![target_active]),
        )],
        "mysql",
        "postgresql",
        PlanOptions {
            type_mapper: Some(&mapper),
            ..Default::default()
        },
    );

    assert!(plan.requirements.is_empty(), "{:?}", plan.requirements);
    let drop_default = plan
        .statements
        .iter()
        .position(|statement| statement.sql.contains("DROP DEFAULT"))
        .expect("old integer default is removed");
    let alter_type = plan
        .statements
        .iter()
        .position(|statement| statement.sql.contains("TYPE boolean"))
        .expect("column is changed to boolean");
    assert!(drop_default < alter_type);
    assert!(!plan
        .statements
        .iter()
        .any(|statement| statement.sql.contains("SET DEFAULT")));
}

#[test]
fn explicit_non_boolean_mysql_override_to_postgres_boolean_maps_only_zero_and_one() {
    let mut enabled = col("enabled", "int");
    enabled.default_value = Some("1".into());
    let mut disabled = col("disabled", "int");
    disabled.default_value = Some("0".into());
    let src = schema(vec![col("id", "int"), enabled, disabled]);
    let tgt = schema(vec![col("id", "int")]);
    let mapper = |_table: &str, ty: &str, name: &str| -> Result<String, String> {
        if matches!(name, "active" | "enabled" | "disabled") {
            Ok("boolean".into())
        } else {
            Ok(ty.into())
        }
    };
    let plan = build_schema_diff_plan(
        &[("users".into(), src, tgt)],
        "mysql",
        "postgresql",
        PlanOptions {
            type_mapper: Some(&mapper),
            ..Default::default()
        },
    );

    assert!(plan.requirements.is_empty(), "{:?}", plan.requirements);
    assert!(plan
        .statements
        .iter()
        .any(|statement| statement.sql.contains("\"enabled\" boolean DEFAULT TRUE")));
    assert!(plan
        .statements
        .iter()
        .any(|statement| statement.sql.contains("\"disabled\" boolean DEFAULT FALSE")));

    let mut source_existing = col("active", "int");
    source_existing.default_value = Some("1".into());
    let mut target_existing = col("active", "boolean");
    target_existing.default_value = Some("FALSE".into());
    let existing_column_plan = build_schema_diff_plan(
        &[(
            "users".into(),
            schema(vec![source_existing]),
            schema(vec![target_existing]),
        )],
        "mysql",
        "postgresql",
        PlanOptions {
            type_mapper: Some(&mapper),
            ..Default::default()
        },
    );
    assert!(
        existing_column_plan.requirements.is_empty(),
        "{:?}",
        existing_column_plan.requirements
    );
    let default_statements = existing_column_plan
        .statements
        .iter()
        .filter(|statement| statement.sql.contains("DEFAULT"))
        .collect::<Vec<_>>();
    assert_eq!(
        default_statements.len(),
        2,
        "{:?}",
        existing_column_plan.statements
    );
    assert!(default_statements[0].sql.contains("DROP DEFAULT"));
    assert!(default_statements[1].sql.contains("SET DEFAULT TRUE"));

    let mut invalid = col("active", "int");
    invalid.default_value = Some("2".into());
    let invalid_mapper =
        |_table: &str, _ty: &str, _name: &str| -> Result<String, String> { Ok("boolean".into()) };
    let invalid_plan = build_schema_diff_plan(
        &[("users".into(), schema(vec![invalid]), schema(vec![]))],
        "mysql",
        "postgresql",
        PlanOptions {
            type_mapper: Some(&invalid_mapper),
            ..Default::default()
        },
    );
    assert!(invalid_plan.statements.is_empty());
    assert!(invalid_plan.requirements.iter().any(|requirement| matches!(
        requirement,
        PlanRequirement::Unsupported { reason, .. } if reason.contains("`2`")
    )));

    let mut invalid_source = col("active", "int");
    invalid_source.default_value = Some("2".into());
    let mut current_target = col("active", "boolean");
    current_target.default_value = Some("FALSE".into());
    let invalid_default_change = build_schema_diff_plan(
        &[(
            "users".into(),
            schema(vec![invalid_source]),
            schema(vec![current_target]),
        )],
        "mysql",
        "postgresql",
        PlanOptions {
            type_mapper: Some(&mapper),
            ..Default::default()
        },
    );
    assert!(invalid_default_change.statements.is_empty());
    assert!(invalid_default_change
        .requirements
        .iter()
        .any(|requirement| matches!(
            requirement,
            PlanRequirement::Unsupported { reason, .. } if reason.contains("`2`")
        )));
}

#[test]
fn mysql_bit_boolean_defaults_map_to_numeric_override_or_fail_closed() {
    let mut enabled = col("enabled", "bit(1)");
    enabled.default_value = Some("b'1'".into());
    let mut disabled = col("disabled", "BIT");
    disabled.default_value = Some("0b0".into());
    let src = schema(vec![col("id", "int"), enabled, disabled]);
    let tgt = schema(vec![col("id", "int")]);
    let mapper =
        |_table: &str, _ty: &str, _name: &str| -> Result<String, String> { Ok("smallint".into()) };
    let plan = build_schema_diff_plan(
        &[("users".into(), src.clone(), tgt)],
        "mysql",
        "postgresql",
        PlanOptions {
            type_mapper: Some(&mapper),
            ..Default::default()
        },
    );

    assert!(plan.requirements.is_empty(), "{:?}", plan.requirements);
    assert!(plan
        .statements
        .iter()
        .any(|statement| statement.sql.contains("\"enabled\" smallint DEFAULT 1")));
    assert!(plan
        .statements
        .iter()
        .any(|statement| statement.sql.contains("\"disabled\" smallint DEFAULT 0")));

    let create_plan = build_schema_diff_plan(
        &[("users".into(), src.clone(), schema(vec![]))],
        "mysql",
        "postgresql",
        PlanOptions {
            type_mapper: Some(&mapper),
            ..Default::default()
        },
    );
    assert!(
        create_plan.requirements.is_empty(),
        "{:?}",
        create_plan.requirements
    );
    let create = create_plan
        .statements
        .iter()
        .find(|statement| statement.sql.starts_with("CREATE TABLE"))
        .expect("CREATE TABLE statement");
    assert!(create.sql.contains("\"enabled\" smallint DEFAULT 1"));
    assert!(create.sql.contains("\"disabled\" smallint DEFAULT 0"));

    let mut source_active = col("active", "bit(1)");
    source_active.default_value = Some("b'1'".into());
    let mut target_active = col("active", "smallint");
    target_active.default_value = Some("0".into());
    let default_change_plan = build_schema_diff_plan(
        &[(
            "users".into(),
            schema(vec![source_active]),
            schema(vec![target_active]),
        )],
        "mysql",
        "postgresql",
        PlanOptions {
            type_mapper: Some(&mapper),
            ..Default::default()
        },
    );
    assert!(
        default_change_plan.requirements.is_empty(),
        "{:?}",
        default_change_plan.requirements
    );
    assert!(default_change_plan
        .statements
        .iter()
        .any(|statement| statement.sql.contains("SET DEFAULT 1")));

    let mut invalid = col("active", "bit(1)");
    invalid.default_value = Some("b'10'".into());
    let invalid_plan = build_schema_diff_plan(
        &[("users".into(), schema(vec![invalid]), schema(vec![]))],
        "mysql",
        "postgresql",
        PlanOptions {
            type_mapper: Some(&mapper),
            ..Default::default()
        },
    );
    assert!(invalid_plan.statements.is_empty());
    assert!(invalid_plan.requirements.iter().any(|requirement| matches!(
        requirement,
        PlanRequirement::Unsupported { reason, .. } if reason.contains("b'10'")
    )));
}

#[test]
fn unapproved_primary_key_replacement_produces_no_half_plan() {
    let mut source = schema(vec![col("id", "int"), col("new_id", "int")]);
    source.primary_keys = vec!["new_id".into()];
    let mut target = source.clone();
    target.primary_keys = vec!["id".into()];
    let plan = build_schema_diff_plan(
        &[("t".into(), source, target)],
        "postgresql",
        "postgresql",
        PlanOptions::default(),
    );
    assert!(plan.statements.is_empty());
    assert!(plan.warnings.iter().any(|w| w.contains("dependent")));
}

#[test]
fn test_tester_primary_key_removal_precedes_nullable_relaxation() {
    let source = schema(vec![col("id", "integer")]);
    let mut target = source.clone();
    target.columns[0].nullable = false;
    target.columns[0].is_primary_key = true;
    target.primary_keys = vec!["id".into()];
    let plan = build_schema_diff_plan(
        &[("users".into(), source, target)],
        "postgresql",
        "postgresql",
        PlanOptions {
            allow_destructive: true,
            include_indexes: true,
            ..PlanOptions::default()
        },
    );
    let drop_pk = plan
        .statements
        .iter()
        .position(|s| s.sql.contains("DROP CONSTRAINT"))
        .expect("PK drop");
    let nullable = plan
        .statements
        .iter()
        .position(|s| s.sql.contains("DROP NOT NULL"))
        .expect("nullable change");
    assert!(
        drop_pk < nullable,
        "PK must be removed before its column can become nullable: {:?}",
        plan.statements
    );
}

#[test]
fn test_tester_excluding_pk_drop_excludes_dependent_nullable_change() {
    let source = schema(vec![col("id", "integer"), col("extra", "text")]);
    let mut target = schema(vec![col("id", "integer")]);
    target.columns[0].nullable = false;
    target.columns[0].is_primary_key = true;
    target.primary_keys = vec!["id".into()];
    let plan = build_schema_diff_plan(
        &[("users".into(), source, target)],
        "postgresql",
        "postgresql",
        PlanOptions::default(),
    );
    assert!(
        !plan
            .statements
            .iter()
            .any(|s| s.sql.contains("DROP NOT NULL")),
        "Cannot relax nullability while retaining primary key"
    );
    assert!(
        plan.statements
            .iter()
            .any(|s| s.sql.contains("ADD COLUMN") && s.sql.contains("extra")),
        "unrelated additive operation must remain"
    );
}
