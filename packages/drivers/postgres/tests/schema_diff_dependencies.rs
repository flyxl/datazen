// Dialect integration tests: the driver owns registration and SQL rendering.
extern crate datazen_driver_postgres;

use datazen_driver_api::{ForeignKeyDeferrability, ForeignKeyInfo, IndexInfo};
use datazen_modules::migration::schema_diff::plan::*;
use datazen_modules::migration::schema_diff::types::*;

mod schema_diff_support;
use schema_diff_support::{col, schema};

#[test]
fn missing_tables_keep_source_foreign_keys_in_the_reviewed_plan() {
    let mut child = schema(vec![col("id", "int"), col("parent_id", "int")]);
    child.foreign_keys.push(ForeignKeyInfo {
        name: "fk_child_parent".into(),
        columns: vec!["parent_id".into()],
        referenced_table: "parent".into(),
        referenced_columns: vec!["id".into()],
        on_update: "NO ACTION".into(),
        on_delete: "NO ACTION".into(),
        deferrability: ForeignKeyDeferrability::NotDeferrable,
    });
    let plan = build_schema_diff_plan(
        &[
            (
                "a_parent".into(),
                schema(vec![col("id", "int")]),
                schema(vec![]),
            ),
            ("z_child".into(), child, schema(vec![])),
        ],
        "postgresql",
        "postgresql",
        PlanOptions::default(),
    );

    let fk = plan
        .statements
        .iter()
        .position(|statement| statement.summary.starts_with("ADD FOREIGN KEY"))
        .expect("source FK must remain in the reviewed plan");
    let parent = plan
        .statements
        .iter()
        .position(|statement| {
            statement.sql.contains("CREATE TABLE") && statement.sql.contains("a_parent")
        })
        .expect("parent table create");
    let child = plan
        .statements
        .iter()
        .position(|statement| {
            statement.sql.contains("CREATE TABLE") && statement.sql.contains("z_child")
        })
        .expect("child table create");
    assert!(parent < fk && child < fk);
}

#[test]
fn existing_table_foreign_key_difference_creates_an_add_operation() {
    let mut source = schema(vec![col("id", "int"), col("parent_id", "int")]);
    source.foreign_keys.push(ForeignKeyInfo {
        name: "fk_child_parent".into(),
        columns: vec!["parent_id".into()],
        referenced_table: "parent".into(),
        referenced_columns: vec!["id".into()],
        on_update: "NO ACTION".into(),
        on_delete: "NO ACTION".into(),
        deferrability: ForeignKeyDeferrability::NotDeferrable,
    });
    let target = schema(vec![col("id", "int"), col("parent_id", "int")]);
    let plan = build_schema_diff_plan(
        &[("child".into(), source, target)],
        "postgresql",
        "postgresql",
        PlanOptions::default(),
    );

    let fk = plan
        .statements
        .iter()
        .find(|statement| statement.summary.starts_with("ADD FOREIGN KEY"))
        .expect("missing source FK must produce an ADD operation");
    assert!(fk.sql.contains("fk_child_parent"));
    assert!(plan.requirements.is_empty(), "{:?}", plan.requirements);
}

#[test]
fn target_only_tables_drop_child_before_selected_parent() {
    for dialect in ["postgresql"] {
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
fn pg_target_schema_context_resolves_bare_selected_names_to_qualified_catalog_identity() {
    let parent_schema = schema(vec![col("id", "int")]);
    let mut child_schema = schema(vec![col("id", "int"), col("parent_id", "int")]);
    child_schema.foreign_keys.push(ForeignKeyInfo {
        name: "fk_child_parent".into(),
        columns: vec!["parent_id".into()],
        referenced_table: "public.parent".into(),
        referenced_columns: vec!["id".into()],
        on_update: "NO ACTION".into(),
        on_delete: "NO ACTION".into(),
        deferrability: ForeignKeyDeferrability::NotDeferrable,
    });
    let catalog = vec![
        ("public.parent".into(), parent_schema),
        ("public.child".into(), child_schema),
    ];
    let plan = build_schema_diff_plan_with_target_only_catalog_in_schema_scope(
        &[],
        &["parent".into(), "child".into()],
        Some(&catalog),
        "postgresql",
        "postgresql",
        None,
        Some("public"),
        PlanOptions {
            allow_destructive: true,
            ..PlanOptions::default()
        },
    );

    assert!(plan.requirements.is_empty(), "{:?}", plan.requirements);
    assert_eq!(plan.statements.len(), 2, "{:?}", plan.statements);
    let child_drop = plan
        .statements
        .iter()
        .position(|statement| statement.summary.ends_with("child"))
        .expect("child DROP TABLE");
    let parent_drop = plan
        .statements
        .iter()
        .position(|statement| statement.summary.ends_with("parent"))
        .expect("parent DROP TABLE");
    assert!(child_drop < parent_drop, "{:?}", plan.statements);

    let blocked = build_schema_diff_plan_with_target_only_catalog_in_schema_scope(
        &[],
        &["parent".into()],
        Some(&catalog),
        "postgresql",
        "postgresql",
        None,
        Some("public"),
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
fn target_only_parent_drop_is_blocked_when_dependent_child_is_unselected() {
    for dialect in ["postgresql"] {
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
    for dialect in ["postgresql"] {
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
    for dialect in ["postgresql"] {
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
fn target_only_drop_refuses_basename_only_foreign_key_identity() {
    let tables = vec![
        "public.a_parent".into(),
        "other.a_parent".into(),
        "z_child".into(),
    ];
    let mut child_schema = schema(vec![col("id", "int"), col("parent_id", "int")]);
    child_schema.foreign_keys.push(ForeignKeyInfo {
        name: "fk_child_parent".into(),
        columns: vec!["parent_id".into()],
        referenced_table: "a_parent".into(),
        referenced_columns: vec!["id".into()],
        on_update: "NO ACTION".into(),
        on_delete: "NO ACTION".into(),
        deferrability: ForeignKeyDeferrability::NotDeferrable,
    });
    let target_schemas = vec![
        ("public.a_parent".into(), schema(vec![col("id", "int")])),
        ("other.a_parent".into(), schema(vec![col("id", "int")])),
        ("z_child".into(), child_schema),
    ];
    let plan = build_schema_diff_plan_with_target_only_catalog(
        &[],
        &tables,
        Some(&target_schemas),
        "postgresql",
        "postgresql",
        PlanOptions {
            allow_destructive: true,
            ..PlanOptions::default()
        },
    );

    assert!(plan.statements.is_empty());
    assert!(plan.requirements.iter().any(|requirement| matches!(
        requirement,
        PlanRequirement::Unsupported { operation, reason }
            if operation == "target-only-table-drop-order" && reason.contains("basename")
    )));
}

#[test]
fn postgres_target_only_drop_uses_exact_schema_qualified_fk_identity() {
    let mut archive_child = schema(vec![col("id", "int"), col("parent_id", "int")]);
    archive_child.foreign_keys.push(ForeignKeyInfo {
        name: "fk_archive_child_parent".into(),
        columns: vec!["parent_id".into()],
        referenced_table: "archive.parent".into(),
        referenced_columns: vec!["id".into()],
        on_update: "NO ACTION".into(),
        on_delete: "NO ACTION".into(),
        deferrability: ForeignKeyDeferrability::NotDeferrable,
    });
    let catalog = vec![
        ("public.parent".into(), schema(vec![col("id", "int")])),
        ("archive.parent".into(), schema(vec![col("id", "int")])),
        ("archive.child".into(), archive_child),
    ];
    let plan = build_schema_diff_plan_with_target_only_catalog(
        &[],
        &["public.parent".into()],
        Some(&catalog),
        "postgresql",
        "postgresql",
        PlanOptions {
            allow_destructive: true,
            ..PlanOptions::default()
        },
    );

    assert!(plan.requirements.is_empty(), "{:?}", plan.requirements);
    assert_eq!(plan.statements.len(), 1, "{:?}", plan.statements);
    assert!(plan.statements[0].sql.contains("\"public\""));
    assert!(plan.statements[0].sql.contains("\"parent\""));
    assert!(!plan.statements[0].sql.contains("archive"));
}

#[test]
fn target_only_drop_fails_closed_when_selected_table_is_missing_from_full_snapshot() {
    let catalog = vec![("public.child".into(), schema(vec![col("id", "int")]))];
    let plan = build_schema_diff_plan_with_target_only_catalog(
        &[],
        &["public.parent".into()],
        Some(&catalog),
        "postgresql",
        "postgresql",
        PlanOptions {
            allow_destructive: true,
            ..PlanOptions::default()
        },
    );

    assert!(plan.statements.is_empty(), "{:?}", plan.statements);
    assert!(plan.requirements.iter().any(|requirement| matches!(
        requirement,
        PlanRequirement::Unsupported { operation, reason }
            if operation == "target-only-table-drop-order"
                && reason.contains("complete target dependency snapshot")
    )));
}

#[test]
fn index_create_planned() {
    let mut src = schema(vec![col("id", "int"), col("email", "text")]);
    src.indexes.push(IndexInfo {
        name: "idx_email".into(),
        columns: vec!["email".into()],
        is_unique: true,
        is_primary: false,
        index_type: "btree".into(),
    });
    let tgt = schema(vec![col("id", "int"), col("email", "text")]);
    let plan = build_schema_diff_plan(
        &[("users".into(), src, tgt)],
        "postgresql",
        "postgresql",
        PlanOptions {
            allow_destructive: false,
            include_indexes: true,
            type_mapper: None,
            cross_dialect: false,
        },
    );
    assert!(plan
        .statements
        .iter()
        .any(|s| s.sql.contains("CREATE") && s.sql.contains("idx_email")));
}

#[test]
fn rollback_completeness_false_when_drop_index() {
    let src = schema(vec![col("id", "int")]);
    let mut tgt = schema(vec![col("id", "int")]);
    tgt.indexes.push(IndexInfo {
        name: "idx_old".into(),
        columns: vec!["id".into()],
        is_unique: false,
        is_primary: false,
        index_type: "btree".into(),
    });
    let plan = build_schema_diff_plan(
        &[("users".into(), src, tgt)],
        "postgresql",
        "postgresql",
        PlanOptions {
            allow_destructive: true,
            include_indexes: true,
            type_mapper: None,
            cross_dialect: false,
        },
    );
    assert!(!plan.rollback_completeness.complete);
    assert!(!plan.rollback_completeness.missing.is_empty());
}
