// Dialect integration tests: the driver owns registration and SQL rendering.
extern crate datazen_driver_mysql;

use datazen_driver_api::{ForeignKeyDeferrability, ForeignKeyInfo};
use datazen_modules::schema_diff::plan::*;
use datazen_modules::schema_diff::types::*;

mod schema_diff_support;
use schema_diff_support::{col, schema};

#[test]
fn pg_to_mysql_strips_schema_prefix_in_ddl() {
    let src = schema(vec![col("id", "int"), col("email", "text")]);
    let tgt = schema(vec![col("id", "int")]);
    let plan = build_schema_diff_plan(
        &[("public.users".into(), src, tgt)],
        "postgresql",
        "mysql",
        PlanOptions {
            allow_destructive: false,
            include_indexes: false,
            type_mapper: None,
            cross_dialect: false,
        },
    );
    let add = plan
        .statements
        .iter()
        .find(|s| s.sql.contains("email"))
        .expect("ADD COLUMN for email");
    assert!(add.sql.contains("`users`"));
    assert!(!add.sql.contains("public."));
}

#[test]
fn cross_dialect_check_constraints_are_blocked_without_expression_translation() {
    let mut src = schema(vec![col("age", "integer")]);
    src.check_constraints
        .push(datazen_driver_api::CheckConstraint {
            name: "users_age_check".into(),
            expression: "age >= 0".into(),
        });
    let tgt = schema(vec![col("age", "integer")]);
    let plan = build_schema_diff_plan(
        &[("users".into(), src, tgt)],
        "postgresql",
        "mysql",
        PlanOptions {
            allow_destructive: true,
            include_indexes: true,
            type_mapper: None,
            cross_dialect: false,
        },
    );
    assert!(plan.statements.is_empty());
    assert!(plan.requirements.iter().any(|requirement| matches!(
        requirement,
        PlanRequirement::Unsupported { operation, reason }
            if operation.contains("users_age_check") && reason.contains("dialect-specific")
    )));
}

#[test]
fn mysql_new_table_renders_safe_source_table_options_inline() {
    let mut src = schema(vec![col("id", "int")]);
    src.table_options.engine = Some("InnoDB".into());
    src.table_options.charset = Some("utf8mb4".into());
    src.table_options.collation = Some("utf8mb4_0900_ai_ci".into());
    let tgt = schema(vec![]);

    let plan = build_column_plan("users", &src, &tgt, "mysql").unwrap();

    assert!(plan.requirements.is_empty(), "{:?}", plan.requirements);
    let create = plan
        .statements
        .iter()
        .find(|statement| statement.sql.contains("CREATE TABLE"))
        .expect("CREATE TABLE statement");
    assert!(create.sql.contains("ENGINE = InnoDB"));
    assert!(create.sql.contains("DEFAULT CHARACTER SET = utf8mb4"));
    assert!(create.sql.contains("COLLATE = utf8mb4_0900_ai_ci"));
    assert!(
        !plan
            .statements
            .iter()
            .any(|statement| statement.sql.contains("ALTER TABLE")
                && statement.sql.contains("ENGINE"))
    );
}

#[test]
fn missing_target_table_plans_create_not_add_column() {
    let mut src = schema(vec![col("id", "int"), col("email", "varchar(255)")]);
    src.primary_keys = vec!["id".into()];
    let tgt = schema(vec![]);
    let mysql_plan = build_column_plan("users", &src, &tgt, "mysql").unwrap();
    assert!(mysql_plan
        .statements
        .iter()
        .any(|s| s.sql.contains("CREATE TABLE") && s.sql.contains("PRIMARY KEY (`id`)")));
    assert!(!mysql_plan
        .statements
        .iter()
        .any(|s| s.sql.contains("ADD PRIMARY KEY")));
}

#[test]
fn mysql_drop_requires_destructive_flag_in_metadata() {
    let src = schema(vec![col("id", "int")]);
    let tgt = schema(vec![col("id", "int"), col("legacy", "text")]);
    let plan = build_column_plan("users", &src, &tgt, "mysql").unwrap();
    let drop = plan
        .statements
        .iter()
        .find(|s| s.sql.contains("DROP COLUMN"))
        .unwrap();
    assert_eq!(drop.risk, StatementRisk::Destructive);
}

#[test]
fn target_only_tables_drop_child_before_selected_parent() {
    for dialect in ["mysql"] {
        let (parent, child, parent_ref) = if dialect == "postgresql" {
            ("public.a_parent", "public.z_child", "public.a_parent")
        } else {
            ("a_parent", "z_child", "a_parent")
        };
        let tables = vec![parent.to_string(), child.to_string()];
        let parent_schema = schema(vec![col("id", "int")]);
        let mut child_schema = schema(vec![col("id", "int"), col("parent_id", "int")]);
        child_schema.foreign_keys.push(ForeignKeyInfo {
            name: "fk_child_parent".into(),
            columns: vec!["parent_id".into()],
            referenced_table: parent_ref.into(),
            referenced_columns: vec!["id".into()],
            on_update: "NO ACTION".into(),
            on_delete: "NO ACTION".into(),
            deferrability: ForeignKeyDeferrability::NotDeferrable,
        });
        let target_schemas = vec![
            (parent.to_string(), parent_schema),
            (child.to_string(), child_schema),
        ];
        let plan = build_schema_diff_plan_with_target_only_catalog(
            &[],
            &tables,
            Some(&target_schemas),
            dialect,
            dialect,
            PlanOptions {
                allow_destructive: true,
                ..PlanOptions::default()
            },
        );
        let child_drop = plan
            .statements
            .iter()
            .position(|statement| statement.summary.ends_with("z_child"))
            .expect("child DROP TABLE");
        let parent_drop = plan
            .statements
            .iter()
            .position(|statement| statement.summary.ends_with("a_parent"))
            .expect("parent DROP TABLE");
        assert!(child_drop < parent_drop, "{dialect}: {:?}", plan.statements);
        assert!(
            plan.requirements.is_empty(),
            "{dialect}: {:?}",
            plan.requirements
        );
    }
}

#[test]
fn target_only_parent_drop_is_blocked_when_dependent_child_is_unselected() {
    for dialect in ["mysql"] {
        let (parent, child, parent_ref) = if dialect == "postgresql" {
            ("public.parent", "public.child", "public.parent")
        } else {
            ("parent", "child", "parent")
        };
        let mut child_schema = schema(vec![col("id", "int"), col("parent_id", "int")]);
        child_schema.foreign_keys.push(ForeignKeyInfo {
            name: "fk_child_parent".into(),
            columns: vec!["parent_id".into()],
            referenced_table: parent_ref.into(),
            referenced_columns: vec!["id".into()],
            on_update: "NO ACTION".into(),
            on_delete: "NO ACTION".into(),
            deferrability: ForeignKeyDeferrability::NotDeferrable,
        });
        let selected_child = if dialect == "postgresql" {
            "public.selected_child"
        } else {
            "selected_child"
        };
        let source_selected_child = schema(vec![col("id", "int"), col("parent_id", "int")]);
        let mut target_selected_child = source_selected_child.clone();
        target_selected_child.foreign_keys.push(ForeignKeyInfo {
            name: "fk_selected_child_parent".into(),
            columns: vec!["parent_id".into()],
            referenced_table: parent_ref.into(),
            referenced_columns: vec!["id".into()],
            on_update: "NO ACTION".into(),
            on_delete: "NO ACTION".into(),
            deferrability: ForeignKeyDeferrability::NotDeferrable,
        });
        let catalog = vec![
            (parent.to_string(), schema(vec![col("id", "int")])),
            (child.to_string(), child_schema),
            (selected_child.to_string(), target_selected_child.clone()),
        ];
        let plan = build_schema_diff_plan_with_target_only_catalog(
            &[(
                selected_child.into(),
                source_selected_child,
                target_selected_child,
            )],
            &[parent.to_string()],
            Some(&catalog),
            dialect,
            dialect,
            PlanOptions {
                allow_destructive: true,
                ..PlanOptions::default()
            },
        );

        assert!(
            plan.statements.is_empty(),
            "{dialect}: {:?}",
            plan.statements
        );
        assert!(
            plan.requirements.iter().any(|requirement| matches!(
                requirement,
                PlanRequirement::Unsupported { operation, reason }
                    if operation == "target-only-table-drop-order"
                        && reason.contains("unselected target table")
                        && reason.contains("select the dependent table")
            )),
            "{dialect}: {:?}",
            plan.requirements
        );
    }
}

#[test]
fn paired_child_does_not_hide_a_foreign_key_retained_by_the_source() {
    for dialect in ["mysql"] {
        let (parent, child, parent_ref) = if dialect == "postgresql" {
            ("public.parent", "public.child", "public.parent")
        } else {
            ("parent", "child", "parent")
        };
        let mut child_schema = schema(vec![col("id", "int"), col("parent_id", "int")]);
        child_schema.foreign_keys.push(ForeignKeyInfo {
            name: "fk_child_parent".into(),
            columns: vec!["parent_id".into()],
            referenced_table: parent_ref.into(),
            referenced_columns: vec!["id".into()],
            on_update: "NO ACTION".into(),
            on_delete: "NO ACTION".into(),
            deferrability: ForeignKeyDeferrability::NotDeferrable,
        });
        let catalog = vec![
            (parent.to_string(), schema(vec![col("id", "int")])),
            (child.to_string(), child_schema.clone()),
        ];
        let plan = build_schema_diff_plan_with_target_only_catalog(
            &[(child.into(), child_schema.clone(), child_schema)],
            &[parent.to_string()],
            Some(&catalog),
            dialect,
            dialect,
            PlanOptions {
                allow_destructive: true,
                ..PlanOptions::default()
            },
        );

        assert!(
            plan.statements.is_empty(),
            "{dialect}: {:?}",
            plan.statements
        );
        assert!(
            plan.requirements.iter().any(|requirement| matches!(
                requirement,
                PlanRequirement::Unsupported { operation, reason }
                    if operation == "target-only-table-drop-order"
                        && reason.contains("select the dependent table")
            )),
            "{dialect}: {:?}",
            plan.requirements
        );
    }
}

#[test]
fn paired_child_can_remove_its_foreign_key_before_parent_drop() {
    for dialect in ["mysql"] {
        let (parent, child, parent_ref) = if dialect == "postgresql" {
            ("public.parent", "public.child", "public.parent")
        } else {
            ("parent", "child", "parent")
        };
        let source_child = schema(vec![col("id", "int"), col("parent_id", "int")]);
        let mut target_child = source_child.clone();
        target_child.foreign_keys.push(ForeignKeyInfo {
            name: "fk_child_parent".into(),
            columns: vec!["parent_id".into()],
            referenced_table: parent_ref.into(),
            referenced_columns: vec!["id".into()],
            on_update: "NO ACTION".into(),
            on_delete: "NO ACTION".into(),
            deferrability: ForeignKeyDeferrability::NotDeferrable,
        });
        let catalog = vec![
            (parent.to_string(), schema(vec![col("id", "int")])),
            (child.to_string(), target_child.clone()),
        ];
        let plan = build_schema_diff_plan_with_target_only_catalog(
            &[(child.into(), source_child, target_child)],
            &[parent.to_string()],
            Some(&catalog),
            dialect,
            dialect,
            PlanOptions {
                allow_destructive: true,
                ..PlanOptions::default()
            },
        );

        let drop_fk = plan
            .statements
            .iter()
            .position(|statement| statement.summary.starts_with("DROP FOREIGN KEY"))
            .unwrap_or_else(|| panic!("{dialect}: {:?}", plan.statements));
        let drop_parent = plan
            .statements
            .iter()
            .position(|statement| {
                statement.summary.starts_with("DROP TABLE ")
                    && statement
                        .summary
                        .ends_with(parent.rsplit('.').next().unwrap())
            })
            .expect("parent DROP TABLE");
        assert!(drop_fk < drop_parent, "{dialect}: {:?}", plan.statements);
        assert!(
            plan.requirements.is_empty(),
            "{dialect}: {:?}",
            plan.requirements
        );
    }
}

#[test]
fn mysql_drop_dependency_uses_exact_database_qualified_identity() {
    let parent_schema = schema(vec![col("id", "int")]);
    let archive_parent_schema = schema(vec![col("id", "int")]);
    let mut archive_child_schema = schema(vec![col("id", "int"), col("parent_id", "int")]);
    archive_child_schema.foreign_keys.push(ForeignKeyInfo {
        name: "fk_archive_child_parent".into(),
        columns: vec!["parent_id".into()],
        referenced_table: "archive.parent".into(),
        referenced_columns: vec!["id".into()],
        on_update: "NO ACTION".into(),
        on_delete: "NO ACTION".into(),
        deferrability: ForeignKeyDeferrability::NotDeferrable,
    });
    let catalog = vec![
        ("parent".into(), parent_schema),
        ("archive.parent".into(), archive_parent_schema),
        ("archive.child".into(), archive_child_schema.clone()),
    ];
    let plan = build_schema_diff_plan_with_target_only_catalog_in_scope(
        &[],
        &["parent".into()],
        Some(&catalog),
        "mysql",
        "mysql",
        Some("app"),
        PlanOptions {
            allow_destructive: true,
            ..PlanOptions::default()
        },
    );
    assert!(plan.requirements.is_empty(), "{:?}", plan.requirements);
    assert_eq!(plan.statements.len(), 1, "{:?}", plan.statements);
    assert!(plan.statements[0].sql.contains("DROP TABLE `parent`"));

    archive_child_schema.foreign_keys[0].referenced_table = "app.parent".into();
    let catalog = vec![
        ("parent".into(), schema(vec![col("id", "int")])),
        ("archive.parent".into(), schema(vec![col("id", "int")])),
        ("archive.child".into(), archive_child_schema),
    ];
    let blocked = build_schema_diff_plan_with_target_only_catalog_in_scope(
        &[],
        &["parent".into()],
        Some(&catalog),
        "mysql",
        "mysql",
        Some("app"),
        PlanOptions {
            allow_destructive: true,
            ..PlanOptions::default()
        },
    );
    assert!(blocked.statements.is_empty(), "{:?}", blocked.statements);
    assert!(
        blocked.requirements.iter().any(|requirement| matches!(
            requirement,
            PlanRequirement::Unsupported { operation, reason }
                if operation == "target-only-table-drop-order"
                    && reason.contains("unselected target table")
        )),
        "{:?}",
        blocked.requirements
    );
}

#[test]
fn target_only_drop_is_blocked_when_dependency_catalog_cannot_be_read() {
    let plan = build_schema_diff_plan_with_target_only_catalog(
        &[],
        &["parent".into()],
        None,
        "mysql",
        "mysql",
        PlanOptions {
            allow_destructive: true,
            ..PlanOptions::default()
        },
    );

    assert!(plan.statements.is_empty());
    assert!(plan.requirements.iter().any(|requirement| matches!(
        requirement,
        PlanRequirement::Unsupported { operation, reason }
            if operation == "target-only-table-drop-order" && reason.contains("dependency boundary")
    )));
}

#[test]
fn target_only_drop_cycle_fails_closed() {
    let mut table_a = schema(vec![col("id", "int"), col("b_id", "int")]);
    table_a.foreign_keys.push(ForeignKeyInfo {
        name: "fk_a_b".into(),
        columns: vec!["b_id".into()],
        referenced_table: "b".into(),
        referenced_columns: vec!["id".into()],
        on_update: "NO ACTION".into(),
        on_delete: "NO ACTION".into(),
        deferrability: ForeignKeyDeferrability::NotDeferrable,
    });
    let mut table_b = schema(vec![col("id", "int"), col("a_id", "int")]);
    table_b.foreign_keys.push(ForeignKeyInfo {
        name: "fk_b_a".into(),
        columns: vec!["a_id".into()],
        referenced_table: "a".into(),
        referenced_columns: vec!["id".into()],
        on_update: "NO ACTION".into(),
        on_delete: "NO ACTION".into(),
        deferrability: ForeignKeyDeferrability::NotDeferrable,
    });
    let catalog = vec![("a".into(), table_a), ("b".into(), table_b)];
    let plan = build_schema_diff_plan_with_target_only_catalog(
        &[],
        &["a".into(), "b".into()],
        Some(&catalog),
        "mysql",
        "mysql",
        PlanOptions {
            allow_destructive: true,
            ..PlanOptions::default()
        },
    );

    assert!(plan.statements.is_empty(), "{:?}", plan.statements);
    assert!(plan.requirements.iter().any(|requirement| matches!(
        requirement,
        PlanRequirement::Unsupported { operation, reason }
            if operation == "target-only-table-drop-order" && reason.contains("cycle")
    )));
}
