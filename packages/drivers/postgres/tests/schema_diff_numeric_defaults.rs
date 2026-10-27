// Dialect integration tests: the driver owns registration and SQL rendering.
extern crate datazen_driver_postgres;

use datazen_modules::migration::schema_diff::plan::*;
use datazen_modules::migration::schema_diff::types::*;

mod schema_diff_support;
use schema_diff_support::{col, schema};

#[test]
fn test_tester_mysql_numeric_expression_default_fails_closed_for_postgres() {
    let mut value = col("value", "int");
    value.default_value = Some("IFNULL(1, 2)".into());
    let mapper =
        |_table: &str, _ty: &str, _name: &str| -> Result<String, String> { Ok("integer".into()) };
    let plan = build_schema_diff_plan(
        &[(
            "users".into(),
            schema(vec![col("id", "int"), value]),
            schema(vec![col("id", "integer")]),
        )],
        "mysql",
        "postgresql",
        PlanOptions {
            type_mapper: Some(&mapper),
            ..Default::default()
        },
    );

    assert!(
        plan.requirements.iter().any(|requirement| matches!(
            requirement,
            PlanRequirement::Unsupported { reason, .. }
                if reason.contains("default") || reason.contains("Default")
        )),
        "numeric MySQL expression should block the plan: {plan:?}"
    );
    assert!(!plan
        .statements
        .iter()
        .any(|statement| statement.sql.contains("IFNULL(1, 2)")));
}

#[test]
fn mysql_numeric_literal_defaults_are_preserved_for_postgres() {
    let mapper =
        |_table: &str, _ty: &str, _name: &str| -> Result<String, String> { Ok("numeric".into()) };

    for default in ["0", "+1", "-1.25", ".5", "1.", "1e-3", "1E+3"] {
        let mut value = col("value", "decimal(12,3)");
        value.default_value = Some(default.into());
        let plan = build_schema_diff_plan(
            &[(
                "users".into(),
                schema(vec![col("id", "int"), value]),
                schema(vec![col("id", "integer")]),
            )],
            "mysql",
            "postgresql",
            PlanOptions {
                type_mapper: Some(&mapper),
                ..Default::default()
            },
        );

        assert!(
            plan.requirements.is_empty(),
            "portable numeric literal `{default}` should be preserved: {:?}",
            plan.requirements
        );
        assert!(
            plan.statements
                .iter()
                .any(|statement| statement.sql.contains(&format!("DEFAULT {default}"))),
            "expected numeric literal `{default}` in target DDL: {:?}",
            plan.statements
        );
    }
}

#[test]
fn mysql_integer_literals_are_accepted_but_fractional_forms_fail_for_postgres_integer_targets() {
    let mapper =
        |_table: &str, _ty: &str, _name: &str| -> Result<String, String> { Ok("integer".into()) };

    for default in ["0", "+1", "-42", "-2147483648", "2147483647"] {
        let mut value = col("value", "int");
        value.default_value = Some(default.into());
        let plan = build_schema_diff_plan(
            &[(
                "users".into(),
                schema(vec![col("id", "int"), value]),
                schema(vec![col("id", "integer")]),
            )],
            "mysql",
            "postgresql",
            PlanOptions {
                type_mapper: Some(&mapper),
                ..Default::default()
            },
        );

        assert!(
            plan.requirements.is_empty(),
            "integer literal `{default}` should remain valid: {:?}",
            plan.requirements
        );
        assert!(plan
            .statements
            .iter()
            .any(|statement| statement.sql.contains(&format!("DEFAULT {default}"))));
    }

    for (source_type, default) in [
        ("int", "1.0"),
        ("int", ".5"),
        ("int", "1e3"),
        ("int", "2147483648"),
        ("int", "-2147483649"),
        ("int unsigned", "1.0"),
        ("int unsigned", "4294967295"),
    ] {
        let mut value = col("value", source_type);
        value.default_value = Some(default.into());
        let plan = build_schema_diff_plan(
            &[(
                "users".into(),
                schema(vec![col("id", "int"), value]),
                schema(vec![col("id", "integer")]),
            )],
            "mysql",
            "postgresql",
            PlanOptions {
                type_mapper: Some(&mapper),
                ..Default::default()
            },
        );

        assert!(
            plan.requirements.iter().any(|requirement| matches!(
                requirement,
                PlanRequirement::Unsupported { reason, .. }
                    if reason.contains(default)
            )),
            "fractional numeric form `{default}` must not target PostgreSQL integer: {:?}",
            plan.requirements
        );
        assert!(plan
            .statements
            .iter()
            .all(|statement| !statement.sql.contains(default)));
    }
}

#[test]
fn test_tester_mysql_integer_defaults_follow_postgres_target_widths() {
    let defaults = [
        ("small_min", "smallint", "-32768", true),
        ("small_max", "smallint", "32767", true),
        ("small_overflow", "smallint", "32768", false),
        ("big_min", "bigint", "-9223372036854775808", true),
        ("big_max", "bigint", "9223372036854775807", true),
        ("big_overflow", "bigint", "9223372036854775808", false),
        ("array_target", "integer[]", "1", false),
        ("empty_numeric", "numeric", "", false),
    ];
    let mut source_columns = vec![col("id", "int")];
    let mut target_types = std::collections::HashMap::new();
    for (name, target_type, default, _) in defaults {
        let mut value = col(name, "int");
        value.default_value = Some(default.into());
        source_columns.push(value);
        target_types.insert(name, target_type);
    }

    let mapper = |_table: &str, _ty: &str, name: &str| -> Result<String, String> {
        Ok(target_types.get(name).copied().unwrap_or("integer").into())
    };
    let plan = build_schema_diff_plan(
        &[(
            "users".into(),
            schema(source_columns),
            schema(vec![col("id", "integer")]),
        )],
        "mysql",
        "postgresql",
        PlanOptions {
            type_mapper: Some(&mapper),
            ..Default::default()
        },
    );

    for (name, target_type, default, should_pass) in defaults {
        let sql = plan
            .statements
            .iter()
            .find(|statement| statement.sql.contains(&format!("\"{name}\"")));
        let has_unsupported = plan.requirements.iter().any(|requirement| {
            matches!(
                requirement,
                PlanRequirement::Unsupported { operation, reason }
                    if operation.contains(name) || reason.contains(name)
            )
        });

        if should_pass {
            assert!(
                !has_unsupported,
                "{name} ({target_type} DEFAULT {default}) unexpectedly failed: {:?}",
                plan.requirements
            );
            assert!(
                sql.is_some_and(|statement| statement.sql.contains(&format!("DEFAULT {default}"))),
                "{name} ({target_type} DEFAULT {default}) was not retained: {:?}",
                plan.statements
            );
        } else {
            assert!(
                has_unsupported,
                "{name} ({target_type} DEFAULT {default}) must fail closed: {:?}",
                plan.requirements
            );
            assert!(
                sql.is_none_or(|statement| !statement.sql.contains(&format!("DEFAULT {default}"))),
                "unsupported {name} default leaked into target SQL: {:?}",
                plan.statements
            );
        }
    }
}

#[test]
fn mysql_numeric_literal_defaults_are_preserved_for_postgres_create_and_set_default() {
    let mapper =
        |_table: &str, _ty: &str, _name: &str| -> Result<String, String> { Ok("numeric".into()) };
    let mut created_value = col("value", "decimal(12,3)");
    created_value.default_value = Some("-1.25".into());
    let create_plan = build_schema_diff_plan(
        &[(
            "users".into(),
            schema(vec![col("id", "int"), created_value]),
            schema(vec![]),
        )],
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
    assert!(create_plan
        .statements
        .iter()
        .any(|statement| statement.sql.contains("DEFAULT -1.25")));

    let mut changed_value = col("value", "decimal(12,3)");
    changed_value.default_value = Some("1e-3".into());
    let mut target_value = col("value", "numeric");
    target_value.default_value = Some("0".into());
    let set_default_plan = build_schema_diff_plan(
        &[(
            "users".into(),
            schema(vec![changed_value]),
            schema(vec![target_value]),
        )],
        "mysql",
        "postgresql",
        PlanOptions {
            type_mapper: Some(&mapper),
            ..Default::default()
        },
    );
    assert!(
        set_default_plan.requirements.is_empty(),
        "{:?}",
        set_default_plan.requirements
    );
    assert!(set_default_plan
        .statements
        .iter()
        .any(|statement| statement.sql.contains("SET DEFAULT 1e-3")));
}

#[test]
fn mysql_numeric_expressions_and_source_specific_literals_fail_closed_for_postgres() {
    let mapper =
        |_table: &str, _ty: &str, _name: &str| -> Result<String, String> { Ok("numeric".into()) };

    for default in [
        "IFNULL(1, 2)",
        "COALESCE(1, 2)",
        "1 + 2",
        "(1)",
        "0x2a",
        "b'101'",
        "1e",
    ] {
        let mut value = col("value", "int");
        value.default_value = Some(default.into());
        let plan = build_schema_diff_plan(
            &[(
                "users".into(),
                schema(vec![col("id", "int"), value]),
                schema(vec![col("id", "integer")]),
            )],
            "mysql",
            "postgresql",
            PlanOptions {
                type_mapper: Some(&mapper),
                ..Default::default()
            },
        );

        assert!(
            plan.requirements.iter().any(|requirement| matches!(
                requirement,
                PlanRequirement::Unsupported { reason, .. }
                    if reason.contains("numeric default expression")
            )),
            "expected Unsupported for MySQL default `{default}`: {:?}",
            plan.requirements
        );
        assert!(
            plan.statements
                .iter()
                .all(|statement| !statement.sql.contains(default)),
            "unsafe default `{default}` leaked into target DDL: {:?}",
            plan.statements
        );
    }
}

#[test]
fn mysql_numeric_default_expressions_fail_closed_for_postgres_create_and_set_default() {
    let mapper =
        |_table: &str, _ty: &str, _name: &str| -> Result<String, String> { Ok("numeric".into()) };
    let mut value = col("value", "int");
    value.default_value = Some("IFNULL(1, 2)".into());

    let create_plan = build_schema_diff_plan(
        &[(
            "users".into(),
            schema(vec![col("id", "int"), value.clone()]),
            schema(vec![]),
        )],
        "mysql",
        "postgresql",
        PlanOptions {
            type_mapper: Some(&mapper),
            ..Default::default()
        },
    );
    assert!(create_plan.requirements.iter().any(|requirement| matches!(
        requirement,
        PlanRequirement::Unsupported { reason, .. }
            if reason.contains("numeric default expression")
    )));
    assert!(create_plan
        .statements
        .iter()
        .all(|statement| !statement.sql.contains("IFNULL(1, 2)")));

    let mut target_value = col("value", "numeric");
    target_value.default_value = Some("0".into());
    let set_default_plan = build_schema_diff_plan(
        &[(
            "users".into(),
            schema(vec![value]),
            schema(vec![target_value]),
        )],
        "mysql",
        "postgresql",
        PlanOptions {
            type_mapper: Some(&mapper),
            ..Default::default()
        },
    );
    assert!(set_default_plan.requirements.iter().any(
        |requirement| matches!(requirement, PlanRequirement::Unsupported { reason, .. }
            if reason.contains("numeric default expression"))
    ));
    assert!(set_default_plan
        .statements
        .iter()
        .all(|statement| !statement.sql.contains("IFNULL(1, 2)")));
}

#[test]
fn unsafe_mysql_string_defaults_fail_closed_for_postgres() {
    for (source_type, default, target_type) in [
        ("varchar(32)", "uuid()", "character varying(32)"),
        ("geometry", "PENDING", "text"),
        ("varchar(32)", "PENDING", "jsonb"),
    ] {
        let mut value = col("value", source_type);
        value.default_value = Some(default.into());
        let mapper = |_table: &str, _ty: &str, _name: &str| -> Result<String, String> {
            Ok(target_type.into())
        };
        let plan = build_schema_diff_plan(
            &[(
                "users".into(),
                schema(vec![col("id", "int"), value]),
                schema(vec![col("id", "int")]),
            )],
            "mysql",
            "postgresql",
            PlanOptions {
                type_mapper: Some(&mapper),
                ..Default::default()
            },
        );
        assert!(
            plan.requirements.iter().any(|requirement| matches!(
                requirement,
                PlanRequirement::Unsupported { reason, .. }
                    if reason.contains("default") || reason.contains("Default")
            )),
            "expected Unsupported for {source_type} default {default}: {:?}",
            plan.requirements
        );
        assert!(!plan
            .statements
            .iter()
            .any(|statement| statement.sql.contains(default)));
    }
}

#[test]
fn mysql_bit_boolean_literals_are_translated_for_postgres() {
    let mut enabled = col("enabled", "bit(1)");
    enabled.default_value = Some("b'1'".into());
    let mut disabled = col("disabled", "BIT");
    disabled.default_value = Some("0b0".into());
    let src = schema(vec![col("id", "int"), enabled, disabled]);
    let mapper = |_table: &str, ty: &str, _name: &str| -> Result<String, String> {
        if ty.trim().to_ascii_lowercase().starts_with("bit") {
            Ok("boolean".into())
        } else {
            Ok(ty.into())
        }
    };

    let add_plan = build_schema_diff_plan(
        &[("users".into(), src.clone(), schema(vec![col("id", "int")]))],
        "mysql",
        "postgresql",
        PlanOptions {
            type_mapper: Some(&mapper),
            ..Default::default()
        },
    );
    assert!(
        add_plan.requirements.is_empty(),
        "{:?}",
        add_plan.requirements
    );
    assert!(add_plan
        .statements
        .iter()
        .any(|statement| statement.sql.contains("\"enabled\" boolean DEFAULT TRUE")));
    assert!(add_plan
        .statements
        .iter()
        .any(|statement| statement.sql.contains("\"disabled\" boolean DEFAULT FALSE")));

    let create_plan = build_schema_diff_plan(
        &[("users".into(), src, schema(vec![]))],
        "mysql",
        "postgresql",
        PlanOptions {
            type_mapper: Some(&mapper),
            ..Default::default()
        },
    );
    let create = create_plan
        .statements
        .iter()
        .find(|statement| statement.sql.starts_with("CREATE TABLE"))
        .expect("CREATE TABLE statement");
    assert!(create.sql.contains("\"enabled\" boolean DEFAULT TRUE"));
    assert!(create.sql.contains("\"disabled\" boolean DEFAULT FALSE"));
}

#[test]
fn unknown_mysql_bit_boolean_default_blocks_postgres_ddl() {
    let mut active = col("active", "bit(1)");
    active.default_value = Some("1".into());
    let src = schema(vec![col("id", "int"), active]);
    let tgt = schema(vec![col("id", "int")]);
    let mapper = |_table: &str, ty: &str, _name: &str| -> Result<String, String> {
        if ty.trim().to_ascii_lowercase().starts_with("bit") {
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

    assert!(plan.statements.is_empty());
    assert!(plan.requirements.iter().any(|requirement| matches!(
        requirement,
        PlanRequirement::Unsupported { operation, reason }
            if operation.contains("active") && reason.contains("`1`")
    )));
}

#[test]
fn unknown_mysql_boolean_default_blocks_postgres_ddl() {
    let mut active = col("active", "tinyint(1)");
    active.default_value = Some("2".into());
    let src = schema(vec![col("id", "int"), active]);
    let tgt = schema(vec![col("id", "int")]);
    let mapper = |_table: &str, ty: &str, _name: &str| -> Result<String, String> {
        if ty.to_ascii_lowercase().starts_with("tinyint(1)") {
            Ok("boolean".into())
        } else {
            Ok(ty.into())
        }
    };

    let plan = build_schema_diff_plan(
        &[("users".into(), src.clone(), tgt)],
        "mysql",
        "postgresql",
        PlanOptions {
            type_mapper: Some(&mapper),
            ..Default::default()
        },
    );

    assert!(plan.statements.is_empty());
    assert!(plan.requirements.iter().any(|requirement| matches!(
        requirement,
        PlanRequirement::Unsupported { operation, reason }
            if operation.contains("active") && reason.contains("`2`")
    )));

    let create_plan = build_schema_diff_plan(
        &[("users".into(), src, schema(vec![]))],
        "mysql",
        "postgresql",
        PlanOptions {
            type_mapper: Some(&mapper),
            ..Default::default()
        },
    );
    assert!(create_plan.statements.is_empty());
    assert!(create_plan.requirements.iter().any(|requirement| matches!(
        requirement,
        PlanRequirement::Unsupported { reason, .. } if reason.contains("`2`")
    )));
}

#[test]
fn changed_mysql_boolean_default_is_translated_for_postgres() {
    let mut source_active = col("active", "tinyint(1)");
    source_active.default_value = Some("0".into());
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

    let set_default = plan
        .statements
        .iter()
        .find(|statement| statement.sql.contains("SET DEFAULT FALSE"))
        .expect("mapped ALTER DEFAULT statement");
    assert!(set_default.sql.contains("SET DEFAULT FALSE"));
    assert!(!set_default.sql.contains("SET DEFAULT 0"));
}
