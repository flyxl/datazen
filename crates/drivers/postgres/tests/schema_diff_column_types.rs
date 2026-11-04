// Dialect integration tests: the driver owns registration and SQL rendering.
extern crate datazen_driver_postgres;

use datazen::migration::schema_diff::plan::*;
use datazen::migration::schema_diff::types::*;
use datazen_driver_api::IndexInfo;

mod schema_diff_support;
use schema_diff_support::{col, schema};

#[test]
fn add_not_null_column_without_default_requires_backfill() {
    let mut c = col("status", "int");
    c.nullable = false;
    let src = schema(vec![col("id", "int"), c]);
    let tgt = schema(vec![col("id", "int")]);
    let plan = build_column_plan("users", &src, &tgt, "postgresql").unwrap();
    let add = plan
        .statements
        .iter()
        .find(|s| s.summary.starts_with("ADD COLUMN"))
        .unwrap();
    assert!(add.sql.contains("status"));
    assert!(!add.sql.contains("NOT NULL"));
    assert!(plan.requirements.iter().any(|r| matches!(r, datazen::migration::schema_diff::types::PlanRequirement::Backfill { column, .. } if column == "status")));
}

#[test]
fn add_not_null_column_with_default_preserves_default() {
    let mut c = col("status", "int");
    c.nullable = false;
    c.default_value = Some("0".into());
    let src = schema(vec![col("id", "int"), c]);
    let tgt = schema(vec![col("id", "int")]);
    let plan = build_column_plan("users", &src, &tgt, "postgresql").unwrap();
    let add = plan
        .statements
        .iter()
        .find(|s| s.summary.starts_with("ADD COLUMN"))
        .unwrap();
    assert!(add.sql.contains("NOT NULL"));
    assert!(!plan
        .statements
        .iter()
        .any(|s| s.summary.starts_with("DROP DEFAULT")));
}

#[test]
fn varchar_narrowing_marks_destructive_risk() {
    let src = schema(vec![col("id", "int"), col("code", "varchar(100)")]);
    let tgt = schema(vec![col("id", "int"), col("code", "varchar(255)")]);
    let plan = build_column_plan("users", &src, &tgt, "postgresql").unwrap();
    let alter = plan
        .statements
        .iter()
        .find(|s| s.summary.contains("ALTER TYPE") || s.sql.contains("TYPE"))
        .expect("ALTER TYPE statement");
    assert_eq!(alter.risk, StatementRisk::Destructive);
    assert!(plan.warnings.iter().any(|w| w.contains("truncate")));
}

#[test]
fn int_to_smallint_narrowing_marks_destructive_risk() {
    let src = schema(vec![col("id", "int"), col("qty", "smallint")]);
    let tgt = schema(vec![col("id", "int"), col("qty", "int")]);
    let plan = build_column_plan("users", &src, &tgt, "postgresql").unwrap();
    let alter = plan
        .statements
        .iter()
        .find(|s| s.summary.contains("ALTER TYPE") || s.sql.contains("TYPE"))
        .expect("ALTER TYPE statement");
    assert_eq!(alter.risk, StatementRisk::Destructive);
}

#[test]
fn unknown_type_change_without_length_is_not_narrowing() {
    let src = schema(vec![col("id", "int"), col("payload", "json")]);
    let tgt = schema(vec![col("id", "int"), col("payload", "text")]);
    let plan = build_column_plan("users", &src, &tgt, "postgresql").unwrap();
    let alter = plan
        .statements
        .iter()
        .find(|s| s.summary.contains("ALTER TYPE"))
        .expect("ALTER TYPE statement");
    assert_eq!(alter.risk, StatementRisk::Rewrite);
    assert!(!plan.warnings.iter().any(|w| w.contains("truncate")));
}

#[test]
fn set_not_null_narrowing_requires_destructive_flag() {
    let mut src_c = col("status", "int");
    src_c.nullable = false;
    let mut tgt_c = col("status", "int");
    tgt_c.nullable = true;
    let src = schema(vec![col("id", "int"), src_c]);
    let tgt = schema(vec![col("id", "int"), tgt_c]);
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
    assert!(plan.warnings.iter().any(|w| w.contains("NOT NULL")));
}

#[test]
fn unsupported_driver_operation_becomes_requirement() {
    let mut src = schema(vec![col("id", "int")]);
    src.columns[0].is_auto_increment = true;
    let tgt = schema(vec![col("id", "int")]);
    let plan = build_schema_diff_plan(
        &[("users".into(), src, tgt)],
        "postgresql",
        "postgresql",
        PlanOptions::default(),
    );
    assert!(plan.requirements.iter().any(|r| matches!(
        r,
        datazen::migration::schema_diff::types::PlanRequirement::Unsupported { .. }
    )));
}

#[test]
fn unsupported_physical_layout_blocks_primary_key_change_and_clears_other_table_sql() {
    let mut source = schema(vec![col("id", "integer"), col("new_id", "integer")]);
    source.primary_keys = vec!["new_id".into()];
    source
        .table_options
        .migration_blockers
        .push("nonclustered primary key cannot be represented".into());
    let mut target = source.clone();
    target.primary_keys = vec!["id".into()];
    target.table_options.migration_blockers.clear();

    let safe_source = schema(vec![col("id", "integer"), col("email", "text")]);
    let safe_target = schema(vec![col("id", "integer")]);
    let plan = build_schema_diff_plan(
        &[
            ("blocked".into(), source, target),
            ("safe".into(), safe_source, safe_target),
        ],
        "postgresql",
        "postgresql",
        PlanOptions {
            allow_destructive: true,
            include_indexes: true,
            ..PlanOptions::default()
        },
    );

    assert!(plan.statements.is_empty(), "{:?}", plan.statements);
    assert!(plan.requirements.iter().any(|requirement| matches!(
        requirement,
        PlanRequirement::Unsupported { operation, reason }
            if operation == "table:blocked" && reason.contains("nonclustered primary key")
    )));
}

#[test]
fn postgres_indexes_do_not_receive_mysql_prefix_length() {
    let mut src = schema(vec![
        col("id", "int"),
        col("name", "text"),
        col("region", "text"),
    ]);
    src.indexes.push(IndexInfo {
        name: "idx_demo_customers_region".into(),
        columns: vec!["region".into()],
        is_unique: false,
        is_primary: false,
        index_type: "BTREE".into(),
    });
    src.indexes.push(IndexInfo {
        name: "uq_demo_customers_name".into(),
        columns: vec!["name".into()],
        is_unique: true,
        is_primary: false,
        index_type: "BTREE".into(),
    });
    let tgt = schema(vec![]);

    // PostgreSQL target: no prefix length should be added
    let pg_plan = build_schema_diff_plan(
        &[("demo_customers".into(), src, tgt)],
        "mysql",
        "postgresql",
        PlanOptions::default(),
    );
    let pg_region_stmt = pg_plan
        .statements
        .iter()
        .find(|s| s.sql.contains("idx_demo_customers_region"))
        .expect("should have pg region index statement");
    assert!(
        pg_region_stmt.sql.contains("\"region\""),
        "PostgreSQL index should keep bare column: {}",
        pg_region_stmt.sql
    );
    assert!(!pg_region_stmt.sql.contains("255"));
}

#[test]
fn mysql_boolean_defaults_are_translated_for_postgres_add_columns() {
    let mut enabled = col("enabled", "tinyint(1)");
    enabled.default_value = Some("1".into());
    let mut disabled = col("disabled", "BOOLEAN");
    disabled.default_value = Some("0".into());
    let mut explicit_true = col("explicit_true", "bool");
    explicit_true.default_value = Some("TRUE".into());
    let src = schema(vec![col("id", "int"), enabled, disabled, explicit_true]);
    let tgt = schema(vec![col("id", "int")]);
    let mapper = |_table: &str, ty: &str, _name: &str| -> Result<String, String> {
        if ty.to_ascii_lowercase().starts_with("tinyint(1)")
            || matches!(ty.to_ascii_lowercase().as_str(), "bool" | "boolean")
        {
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
        .any(|statement| { statement.sql.contains("\"enabled\" boolean DEFAULT TRUE") }));
    assert!(plan
        .statements
        .iter()
        .any(|statement| { statement.sql.contains("\"disabled\" boolean DEFAULT FALSE") }));
    assert!(plan.statements.iter().any(|statement| {
        statement
            .sql
            .contains("\"explicit_true\" boolean DEFAULT TRUE")
    }));
}

#[test]
fn mysql_boolean_defaults_are_translated_for_postgres_create_table() {
    let mut enabled = col("enabled", "tinyint(1) unsigned");
    enabled.default_value = Some("1".into());
    let mut disabled = col("disabled", "tinyint(1)");
    disabled.default_value = Some("false".into());
    let src = schema(vec![col("id", "int"), enabled, disabled]);
    let tgt = schema(vec![]);
    let mapper = |_table: &str, ty: &str, _name: &str| -> Result<String, String> {
        if ty.to_ascii_lowercase().starts_with("tinyint(1)") {
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
    let create = plan
        .statements
        .iter()
        .find(|statement| statement.sql.starts_with("CREATE TABLE"))
        .expect("CREATE TABLE statement");
    assert!(create.sql.contains("\"enabled\" boolean DEFAULT TRUE"));
    assert!(create.sql.contains("\"disabled\" boolean DEFAULT FALSE"));
}

#[test]
fn mysql_string_defaults_are_quoted_for_postgres_add_create_and_set_default() {
    let mut code = col("code", "char(8)");
    code.default_value = Some("CODE0001".into());
    let mut label = col("label", "varchar(32)");
    label.default_value = Some("  fresh' label  ".into());
    let mut amount = col("amount", "decimal(10,2)");
    amount.default_value = Some("0.00".into());
    let mut created_at = col("created_at", "datetime");
    created_at.default_value = Some("CURRENT_TIMESTAMP(6)".into());
    let mut event_date = col("event_date", "date");
    event_date.default_value = Some("2026-09-23".into());
    let source = schema(vec![
        col("id", "int"),
        code.clone(),
        label.clone(),
        amount.clone(),
        created_at.clone(),
        event_date.clone(),
    ]);
    let mapper = |_table: &str, ty: &str, _name: &str| -> Result<String, String> {
        let lower = ty.to_ascii_lowercase();
        if lower.starts_with("char") || lower.starts_with("varchar") {
            Ok("character varying(128)".into())
        } else if lower.starts_with("decimal") {
            Ok("numeric(10,2)".into())
        } else if lower == "datetime" {
            Ok("timestamp without time zone".into())
        } else {
            Ok(ty.into())
        }
    };
    let add_plan = build_schema_diff_plan(
        &[(
            "users".into(),
            source.clone(),
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
        add_plan.requirements.is_empty(),
        "{:?}",
        add_plan.requirements
    );
    let add_sql = add_plan
        .statements
        .iter()
        .map(|statement| statement.sql.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(add_sql.contains("\"code\" character varying(128) DEFAULT 'CODE0001'"));
    assert!(add_sql.contains("\"label\" character varying(128) DEFAULT '  fresh'' label  '"));
    assert!(add_sql.contains("\"amount\" numeric(10,2) DEFAULT 0.00"));
    assert!(
        add_sql.contains("\"created_at\" timestamp without time zone DEFAULT CURRENT_TIMESTAMP(6)")
    );
    assert!(add_sql.contains("\"event_date\" date DEFAULT '2026-09-23'"));

    let create_plan = build_schema_diff_plan(
        &[("users".into(), source.clone(), schema(vec![]))],
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
    let create_sql = create_plan
        .statements
        .iter()
        .find(|statement| statement.sql.starts_with("CREATE TABLE"))
        .expect("CREATE TABLE statement")
        .sql
        .as_str();
    assert!(create_sql.contains("\"code\" character varying(128) DEFAULT 'CODE0001'"));
    assert!(create_sql.contains("\"label\" character varying(128) DEFAULT '  fresh'' label  '"));
    assert!(create_sql
        .contains("\"created_at\" timestamp without time zone DEFAULT CURRENT_TIMESTAMP(6)"));

    let mut changed_label = label;
    changed_label.default_value = Some("PENDING".into());
    let mut target_label = col("label", "character varying(64)");
    target_label.default_value = Some("'OLD'::character varying".into());
    let set_plan = build_schema_diff_plan(
        &[(
            "users".into(),
            schema(vec![changed_label]),
            schema(vec![target_label]),
        )],
        "mysql",
        "postgresql",
        PlanOptions {
            type_mapper: Some(&mapper),
            ..Default::default()
        },
    );
    assert!(
        set_plan.requirements.is_empty(),
        "{:?}",
        set_plan.requirements
    );
    assert!(set_plan
        .statements
        .iter()
        .any(|statement| statement.sql.contains("SET DEFAULT 'PENDING'")));
}
