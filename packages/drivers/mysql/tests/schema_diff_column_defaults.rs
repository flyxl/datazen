// Dialect integration tests: the driver owns registration and SQL rendering.
extern crate datazen_driver_mysql;

use datazen::migration::schema_diff::plan::*;
use datazen::migration::schema_diff::types::*;
use datazen_driver_api::{ForeignKeyDeferrability, ForeignKeyInfo, IndexInfo};

mod schema_diff_support;
use schema_diff_support::{col, schema};

#[test]
fn cross_dialect_foreign_key_reference_uses_target_relation_name() {
    let mut source = schema(vec![col("user_id", "int")]);
    source.foreign_keys.push(ForeignKeyInfo {
        name: "orders_user_id_fk".into(),
        columns: vec!["user_id".into()],
        referenced_table: "public.users".into(),
        referenced_columns: vec!["id".into()],
        on_update: "NO ACTION".into(),
        on_delete: "NO ACTION".into(),
        deferrability: ForeignKeyDeferrability::NotDeferrable,
    });
    let plan = build_schema_diff_plan(
        &[("public.orders".into(), source, schema(vec![]))],
        "postgresql",
        "mysql",
        PlanOptions::default(),
    );
    let fk = plan
        .statements
        .iter()
        .find(|statement| statement.summary.starts_with("ADD FOREIGN KEY"))
        .expect("foreign key statement");
    assert!(fk.sql.contains("REFERENCES `users`"), "{}", fk.sql);
    assert!(!fk.sql.contains("public"), "{}", fk.sql);
}

#[test]
fn cross_dialect_type_mapper() {
    let src = schema(vec![col("id", "int4"), col("email", "character varying")]);
    let tgt = schema(vec![col("id", "int")]);
    let mapper = |_table: &str, ty: &str, _name: &str| -> Result<String, String> {
        if ty.contains("varying") || ty == "text" {
            Ok("VARCHAR(255)".into())
        } else if ty.starts_with("int") {
            Ok("INT".into())
        } else {
            Err(format!("unsupported type {ty}"))
        }
    };
    let plan = build_schema_diff_plan(
        &[("users".into(), src, tgt)],
        "postgresql",
        "mysql",
        PlanOptions {
            allow_destructive: false,
            include_indexes: false,
            type_mapper: Some(&mapper),
            cross_dialect: false,
        },
    );
    assert!(!plan.same_dialect);
    let add = plan
        .statements
        .iter()
        .find(|s| s.sql.contains("email"))
        .unwrap();
    assert!(add.sql.contains("VARCHAR(255)"));
}

#[test]
fn changed_default_generates_target_dialect_ddl() {
    let mut src_c = col("status", "int");
    src_c.default_value = Some("0".into());
    let mut tgt_c = col("status", "int");
    tgt_c.default_value = Some("1".into());
    let src = schema(vec![src_c]);
    let tgt = schema(vec![tgt_c]);
    let plan = build_column_plan("users", &src, &tgt, "mysql").unwrap();
    let stmt = plan
        .statements
        .iter()
        .find(|s| s.summary.starts_with("ALTER DEFAULT"))
        .unwrap();
    assert!(stmt.sql.contains("SET DEFAULT 0"));
    assert!(stmt
        .rollback_sql
        .as_deref()
        .unwrap()
        .contains("SET DEFAULT 1"));
}

#[test]
fn mysql_primary_key_change_generates_drop_and_add() {
    let mut src = schema(vec![col("id", "int"), col("user_id", "int")]);
    src.primary_keys = vec!["user_id".into()];
    let mut tgt = schema(vec![col("id", "int"), col("user_id", "int")]);
    tgt.primary_keys = vec!["id".into()];
    let plan = build_column_plan("users", &src, &tgt, "mysql").unwrap();
    assert!(plan
        .statements
        .iter()
        .any(|s| s.sql.contains("DROP PRIMARY KEY")));
    assert!(plan
        .statements
        .iter()
        .any(|s| s.sql.contains("ADD PRIMARY KEY")));
}

#[test]
fn cross_dialect_pg_to_mysql_does_not_translate_nextval() {
    let mut id_col = col("id", "integer");
    id_col.nullable = false;
    id_col.default_value = Some("nextval('orders_id_seq'::regclass)".into());
    let src = schema(vec![id_col, col("name", "text")]);
    let tgt = schema(vec![col("name", "text")]);
    let plan = build_schema_diff_plan(
        &[("orders".into(), src, tgt)],
        "postgresql",
        "mysql",
        PlanOptions::default(),
    );
    assert!(!plan.statements.iter().any(|s| s.sql.contains("nextval")));
    assert!(plan
        .warnings
        .iter()
        .any(|w| w.contains("Cross-dialect plan without IR type mapper")));
    assert!(plan.requirements.iter().any(|requirement| matches!(
        requirement,
        PlanRequirement::Backfill { column, .. } if column == "id"
    )));
}

#[test]
fn cross_dialect_pg_character_literal_casts_keep_portable_quoted_defaults() {
    let mut code = col("code", "character varying(16)");
    code.nullable = false;
    code.default_value = Some("'CODE0001'::bpchar".into());
    let mut label = col("label", "varchar(32)");
    label.nullable = false;
    label.default_value = Some("'unnamed'::character varying".into());
    let mut owner = col("owner", "char(32)");
    owner.nullable = false;
    owner.default_value = Some("'owner''s'::varchar(32)".into());
    let src = schema(vec![col("id", "integer"), code, label, owner]);
    let tgt = schema(vec![col("id", "integer")]);
    let mapper = |_table: &str, ty: &str, _name: &str| -> Result<String, String> {
        if ty.to_ascii_lowercase().contains("char") {
            Ok("VARCHAR(64)".into())
        } else {
            Ok("BIGINT".into())
        }
    };
    let plan = build_schema_diff_plan(
        &[("users".into(), src, tgt)],
        "postgresql",
        "mysql",
        PlanOptions {
            type_mapper: Some(&mapper),
            ..Default::default()
        },
    );

    assert!(plan.requirements.is_empty(), "{:?}", plan.requirements);
    assert!(plan.statements.iter().any(|statement| statement
        .sql
        .contains("`code` VARCHAR(64) NOT NULL DEFAULT 'CODE0001'")));
    assert!(plan.statements.iter().any(|statement| statement
        .sql
        .contains("`label` VARCHAR(64) NOT NULL DEFAULT 'unnamed'")));
    assert!(plan.statements.iter().any(|statement| statement
        .sql
        .contains("`owner` VARCHAR(64) NOT NULL DEFAULT 'owner''s'")));
    assert!(!plan
        .statements
        .iter()
        .any(|statement| statement.sql.contains("::")));
}

#[test]
fn cross_dialect_pg_unknown_cast_expressions_are_stripped_and_require_backfill() {
    let mut function_default = col("function_default", "varchar(32)");
    function_default.nullable = false;
    function_default.default_value = Some("lower('NAME')::character varying".into());
    let mut unknown_cast = col("unknown_cast", "varchar(32)");
    unknown_cast.nullable = false;
    unknown_cast.default_value = Some("'CODE0001'::custom_string_type".into());
    let mut malformed_literal = col("malformed_literal", "varchar(32)");
    malformed_literal.nullable = false;
    malformed_literal.default_value = Some("'CODE0001::character varying".into());
    let mut sequence_default = col("sequence_default", "bigint");
    sequence_default.nullable = false;
    sequence_default.default_value = Some("nextval('users_seq'::regclass)".into());
    let src = schema(vec![
        col("id", "integer"),
        function_default,
        unknown_cast,
        malformed_literal,
        sequence_default,
    ]);
    let tgt = schema(vec![col("id", "integer")]);
    let mapper = |_table: &str, ty: &str, _name: &str| -> Result<String, String> {
        if ty.to_ascii_lowercase().contains("char") {
            Ok("VARCHAR(64)".into())
        } else {
            Ok("BIGINT".into())
        }
    };
    let plan = build_schema_diff_plan(
        &[("users".into(), src, tgt)],
        "postgresql",
        "mysql",
        PlanOptions {
            type_mapper: Some(&mapper),
            ..Default::default()
        },
    );

    for column in [
        "function_default",
        "unknown_cast",
        "malformed_literal",
        "sequence_default",
    ] {
        assert!(
            plan.requirements.iter().any(|requirement| matches!(
                requirement,
                PlanRequirement::Backfill { column: required, .. } if required == column
            )),
            "missing Backfill requirement for {column}: {:?}",
            plan.requirements
        );
    }
    assert!(!plan.statements.iter().any(|statement| {
        statement.sql.contains("lower('NAME')")
            || statement.sql.contains("custom_string_type")
            || statement.sql.contains("nextval")
            || statement.sql.contains("::regclass")
    }));
}

#[test]
fn cross_dialect_pg_set_default_maps_known_string_cast_and_blocks_unknown_expression() {
    let mut source = col("label", "character varying(32)");
    source.default_value = Some("'fresh'' label'::character varying".into());
    let mut target = col("label", "VARCHAR(64)");
    target.default_value = Some("'old label'".into());
    let mapper = |_table: &str, ty: &str, _name: &str| -> Result<String, String> {
        if ty.to_ascii_lowercase().contains("char") {
            Ok("VARCHAR(64)".into())
        } else {
            Ok(ty.into())
        }
    };
    let plan = build_schema_diff_plan(
        &[(
            "users".into(),
            schema(vec![source.clone()]),
            schema(vec![target.clone()]),
        )],
        "postgresql",
        "mysql",
        PlanOptions {
            type_mapper: Some(&mapper),
            ..Default::default()
        },
    );
    assert!(plan.requirements.is_empty(), "{:?}", plan.requirements);
    assert!(plan
        .statements
        .iter()
        .any(|statement| statement.sql.contains("SET DEFAULT 'fresh'' label'")));
    assert!(!plan
        .statements
        .iter()
        .any(|statement| statement.sql.contains("::")));

    source.default_value = Some("lower('fresh')::character varying".into());
    target.default_value = Some("'old label'".into());
    let unknown_plan = build_schema_diff_plan(
        &[("users".into(), schema(vec![source]), schema(vec![target]))],
        "postgresql",
        "mysql",
        PlanOptions {
            type_mapper: Some(&mapper),
            ..Default::default()
        },
    );
    assert!(unknown_plan.requirements.iter().any(|requirement| matches!(
        requirement,
        PlanRequirement::Unsupported { reason, .. }
            if reason.contains("PostgreSQL default expression")
    )));
    assert!(!unknown_plan
        .statements
        .iter()
        .any(|statement| statement.sql.contains("lower('fresh')")));
}

#[test]
fn type_mapper_failure_becomes_unsupported_not_executable() {
    let src = schema(vec![col("id", "int"), col("payload", "jsonb")]);
    let tgt = schema(vec![col("id", "int")]);
    let mapper = |_table: &str, _ty: &str, name: &str| -> Result<String, String> {
        if name == "payload" {
            Err("no mysql equivalent for jsonb".into())
        } else {
            Ok("INT".into())
        }
    };
    let plan = build_schema_diff_plan(
        &[("users".into(), src, tgt)],
        "postgresql",
        "mysql",
        PlanOptions {
            allow_destructive: true,
            include_indexes: false,
            type_mapper: Some(&mapper),
            cross_dialect: false,
        },
    );
    assert!(plan.statements.is_empty());
    assert!(plan.requirements.iter().any(|r| {
        matches!(
            r,
            datazen::migration::schema_diff::types::PlanRequirement::Unsupported { operation, reason }
            if operation.contains("payload") && reason.contains("jsonb")
        )
    }));
}

#[test]
fn create_table_type_mapper_failure_becomes_unsupported() {
    let src = schema(vec![col("id", "int"), col("meta", "hstore")]);
    let tgt = schema(vec![]);
    let mapper = |_table: &str, _ty: &str, name: &str| -> Result<String, String> {
        if name == "meta" {
            Err("unsupported hstore".into())
        } else {
            Ok("INT".into())
        }
    };
    let plan = build_schema_diff_plan(
        &[("items".into(), src, tgt)],
        "postgresql",
        "mysql",
        PlanOptions {
            allow_destructive: true,
            include_indexes: false,
            type_mapper: Some(&mapper),
            cross_dialect: false,
        },
    );
    assert!(!plan
        .statements
        .iter()
        .any(|s| s.sql.contains("CREATE TABLE")));
    assert!(plan.requirements.iter().any(|r| {
        matches!(
            r,
            datazen::migration::schema_diff::types::PlanRequirement::Unsupported { reason, .. }
            if reason.contains("hstore")
        )
    }));
}

#[test]
fn mysql_indexes_on_blob_text_columns_receive_prefix_length() {
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

    // MySQL target
    let mysql_plan = build_schema_diff_plan(
        &[("demo_customers".into(), src.clone(), tgt.clone())],
        "postgresql",
        "mysql",
        PlanOptions::default(),
    );
    let region_stmt = mysql_plan
        .statements
        .iter()
        .find(|s| s.sql.contains("idx_demo_customers_region"))
        .expect("should have region index statement");
    assert!(
        region_stmt.sql.contains("`region`(255)"),
        "MySQL index should have prefix length: {}",
        region_stmt.sql
    );
    let name_stmt = mysql_plan
        .statements
        .iter()
        .find(|s| s.sql.contains("uq_demo_customers_name"))
        .expect("should have name unique index statement");
    assert!(
        name_stmt.sql.contains("`name`(255)"),
        "MySQL unique index should have prefix length: {}",
        name_stmt.sql
    );
    assert!(mysql_plan
        .warnings
        .iter()
        .any(|w| w.contains("prefix length (255)")));
}

#[test]
fn cross_dialect_mysql_detects_unbounded_text_type_suggestions() {
    let mut src = schema(vec![
        col("id", "int"),
        col("name", "text"),
        col("region", "text"),
        col("notes", "text"),
    ]);
    src.indexes.push(IndexInfo {
        name: "uq_demo_customers_name".into(),
        columns: vec!["name".into()],
        is_unique: true,
        is_primary: false,
        index_type: "BTREE".into(),
    });
    src.indexes.push(IndexInfo {
        name: "idx_demo_customers_region".into(),
        columns: vec!["region".into()],
        is_unique: false,
        is_primary: false,
        index_type: "BTREE".into(),
    });
    let tgt = schema(vec![]);

    let plan = build_schema_diff_plan(
        &[("demo_customers".into(), src, tgt)],
        "postgresql",
        "mysql",
        PlanOptions::default(),
    );

    assert_eq!(plan.type_suggestions.len(), 3);
    let name_sug = plan
        .type_suggestions
        .iter()
        .find(|s| s.column == "name")
        .expect("should have name suggestion");
    assert_eq!(name_sug.suggested_type, "VARCHAR(255)");
    assert!(name_sug.is_key_or_indexed);
    assert!(name_sug.reason.contains("Unique index"));

    let region_sug = plan
        .type_suggestions
        .iter()
        .find(|s| s.column == "region")
        .expect("should have region suggestion");
    assert_eq!(region_sug.suggested_type, "VARCHAR(255)");
    assert!(region_sug.is_key_or_indexed);
    assert!(region_sug.reason.contains("Indexed column"));

    let notes_sug = plan
        .type_suggestions
        .iter()
        .find(|s| s.column == "notes")
        .expect("should have notes suggestion");
    assert_eq!(notes_sug.suggested_type, "VARCHAR(255)");
    assert!(!notes_sug.is_key_or_indexed);
}
