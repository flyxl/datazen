use super::*;

#[test]
fn type_overrides_are_applied_before_custom_type_dependency_validation() {
    let source = table("items", vec![column("id", "source_type")], vec![]);
    let target = table("items", vec![], vec![]);
    let operations = diff_to_operations("public.items", &source, &target, None);
    let mut overridden = operations;
    apply_type_overrides(
        "public.items",
        &mut overridden,
        &[ColumnTypeOverride {
            table: "public.items".into(),
            column: "id".into(),
            target_type: "integer".into(),
        }],
        "postgresql",
    );
    assert!(matches!(
        &overridden[0],
        crate::schema_diff::operations::MigrationOperation::CreateTable { columns, .. }
            if columns[0].data_type == "integer"
    ));
}

#[test]
fn type_override_filters_only_catalog_edges_used_exclusively_by_overridden_columns() {
    let custom_type = SchemaObjectIdentity {
        kind: ObjectKind::Type,
        schema: Some("public".into()),
        name: "source_state".into(),
        signature: None,
        target_schema: None,
        target_name: None,
    };
    let type_object = SchemaObjectSnapshot::type_definition(
        Some("public"),
        "source_state",
        "CREATE TYPE public.source_state AS ENUM ('ready')",
    )
    .with_dependencies(vec![]);
    let table_identity = target_table("items");
    let source = table(
        "items",
        vec![column("state", "public.source_state")],
        vec![],
    );
    let target = table("items", vec![], vec![]);
    let override_ = ColumnTypeOverride {
        table: "public.items".into(),
        column: "state".into(),
        target_type: "varchar(32)".into(),
    };
    let make_snapshot = |usage| SchemaObjectDependencySnapshot {
        identity: table_identity.clone(),
        dependencies: Some(vec![custom_type.clone()]),
        type_dependency_usages: Some(vec![
            crate::schema_diff::object_identity::TypeDependencyUsage {
                dependency: custom_type.clone(),
                usage,
                column_name: matches!(
                    usage,
                    crate::schema_diff::object_identity::TypeDependencyUsageKind::ColumnType
                )
                .then(|| "state".into()),
            },
        ]),
        sequence_dependency_usages: None,
    };
    let build_plan = |snapshot: SchemaObjectDependencySnapshot, source: TableSchema| {
        build_unified_schema_diff_plan_with_components(
            &[("public.items".into(), source, target.clone())],
            &[],
            &[type_object.clone()],
            &[],
            &[],
            &[snapshot],
            &[],
            true,
            &[],
            "postgresql",
            "postgresql",
            Some("public"),
            Some("test_db"),
            false,
            true,
            &[override_.clone()],
            &TestRenderer {
                omit_rollback: false,
            },
            &TestCapabilities,
        )
    };

    let column_only = build_plan(
        make_snapshot(crate::schema_diff::object_identity::TypeDependencyUsageKind::ColumnType),
        source.clone(),
    );
    assert!(
        column_only.requirements.is_empty(),
        "{:?}",
        column_only.requirements
    );
    assert!(column_only.statements[0].summary.starts_with("table+"));
    assert!(column_only.statements[1].summary.starts_with("type+"));

    let mut expression_source = source;
    expression_source.columns[0].default_value = Some("'ready'::public.source_state".into());
    let mixed_usage = build_plan(
        SchemaObjectDependencySnapshot {
            identity: table_identity,
            dependencies: Some(vec![custom_type.clone()]),
            type_dependency_usages: Some(vec![
                crate::schema_diff::object_identity::TypeDependencyUsage {
                    dependency: custom_type.clone(),
                    usage: crate::schema_diff::object_identity::TypeDependencyUsageKind::ColumnType,
                    column_name: Some("state".into()),
                },
                crate::schema_diff::object_identity::TypeDependencyUsage {
                    dependency: custom_type,
                    usage: crate::schema_diff::object_identity::TypeDependencyUsageKind::Expression,
                    column_name: None,
                },
            ]),
            sequence_dependency_usages: None,
        },
        expression_source,
    );
    assert!(
        mixed_usage.requirements.is_empty(),
        "{:?}",
        mixed_usage.requirements
    );
    assert!(mixed_usage.statements[0].summary.starts_with("type+"));
    assert!(mixed_usage.statements[1].summary.starts_with("table+"));
}

#[test]
fn unqualified_missing_custom_type_blocks_but_native_builtin_type_does_not() {
    let source = table("items", vec![column("state", "account_state")], vec![]);
    let empty_target = table("items", vec![], vec![]);
    let missing = build(
        &[("public.items".into(), source, empty_target)],
        &[],
        &[],
        &[],
        &[],
        &[],
        false,
        &TestRenderer {
            omit_rollback: false,
        },
    );
    assert!(missing.statements.is_empty());
    assert!(format!("{:?}", missing.requirements).contains("Unqualified type"));

    let source = table("items", vec![column("id", "integer")], vec![]);
    let empty_target = table("items", vec![], vec![]);
    let builtin = build(
        &[("public.items".into(), source, empty_target)],
        &[],
        &[],
        &[],
        &[],
        &[],
        false,
        &TestRenderer {
            omit_rollback: false,
        },
    );
    assert!(
        builtin.requirements.is_empty(),
        "{:?}",
        builtin.requirements
    );
    assert_eq!(builtin.statements.len(), 1);
}
