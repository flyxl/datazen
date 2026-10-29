//! Object scope gate and schema object selector resolution tests.

use super::super::object_scope::sqlserver_object_scope_requirement;
use super::*;

#[test]
fn sqlserver_object_only_plan_blocks_cross_schema_statements() {
    let source = vec![SchemaObjectSnapshot::view(
        Some("dbo"),
        "v",
        "SELECT 1 AS value",
    )];
    let target = vec![SchemaObjectSnapshot::view(
        Some("sales"),
        "v",
        "SELECT 2 AS value",
    )];
    let mut plan = SchemaDiffPlan {
        plan_id: None,
        table: "v".into(),
        tables: Vec::new(),
        source_dialect: "sqlserver".into(),
        target_dialect: "sqlserver".into(),
        same_dialect: true,
        statements: vec![PlanStatement {
            sql: "CREATE VIEW [dbo].[v] AS SELECT 1 AS value".into(),
            risk: StatementRisk::Additive,
            rollback_sql: None,
            summary: "create view".into(),
            requires_transaction: false,
        }],
        warnings: Vec::new(),
        requirements: Vec::new(),
        rollback_completeness: RollbackCompleteness {
            complete: false,
            missing: vec!["view rollback is not complete".into()],
        },
        type_suggestions: Vec::new(),
        expected_target_schemas: Vec::new(),
    };

    apply_sqlserver_object_scope_gate(
        &mut plan,
        "sqlserver",
        "sqlserver",
        Some("dbo"),
        Some("sales"),
        &source,
        &target,
    );

    assert!(plan.statements.is_empty());
    assert!(matches!(
        plan.requirements.as_slice(),
        [PlanRequirement::Unsupported { operation, reason }]
            if operation == "sqlserver-object-schema-scope"
                && reason.contains("cannot rewrite object definitions across schemas")
    ));

    assert!(sqlserver_object_scope_requirement(
        "sqlserver",
        "sqlserver",
        None,
        None,
        &[SchemaObjectSnapshot::view(
            Some("dbo"),
            "v",
            "SELECT 1 AS value",
        )],
        &[SchemaObjectSnapshot::view(
            Some("dbo"),
            "v",
            "SELECT 2 AS value",
        )],
    )
    .is_none());
}

#[test]
fn test_tester_schema_object_selectors_require_unambiguous_identity_and_allow_target_only_drop() {
    let source = vec![datazen_driver_api::DatabaseObject {
        kind: "view".into(),
        schema: Some("public".into()),
        name: "active_users".into(),
        signature: None,
        target_schema: None,
        target_name: None,
    }];
    let target = vec![datazen_driver_api::DatabaseObject {
        kind: "view".into(),
        schema: Some("public".into()),
        name: "legacy_users".into(),
        signature: None,
        target_schema: None,
        target_name: None,
    }];
    let (source_selected, target_selected) =
        select_schema_object_pair(&source, &target, &["public.active_users".into()]).unwrap();
    assert_eq!(source_selected.len(), 1);
    assert!(target_selected.is_empty());
    let (source_selected, target_selected) =
        select_schema_object_pair(&source, &target, &["public.legacy_users".into()]).unwrap();
    assert!(source_selected.is_empty());
    assert_eq!(target_selected.len(), 1);

    let ambiguous = vec![
        datazen_driver_api::DatabaseObject {
            kind: "view".into(),
            schema: Some("one".into()),
            name: "same".into(),
            signature: None,
            target_schema: None,
            target_name: None,
        },
        datazen_driver_api::DatabaseObject {
            kind: "view".into(),
            schema: Some("two".into()),
            name: "same".into(),
            signature: None,
            target_schema: None,
            target_name: None,
        },
    ];
    assert!(select_schema_object_pair(&ambiguous, &[], &["same".into()]).is_err());
}

#[test]
fn test_tester_schema_object_selectors_preserve_overloads_and_trigger_relations() {
    let functions = vec![
        datazen_driver_api::DatabaseObject {
            kind: "function".into(),
            schema: Some("public".into()),
            name: "lookup".into(),
            signature: Some("integer".into()),
            target_schema: None,
            target_name: None,
        },
        datazen_driver_api::DatabaseObject {
            kind: "function".into(),
            schema: Some("public".into()),
            name: "lookup".into(),
            signature: Some("text".into()),
            target_schema: None,
            target_name: None,
        },
    ];
    let (source, target) =
        select_schema_object_pair(&functions, &functions, &["public.lookup(integer)".into()])
            .unwrap();
    assert_eq!(source.len(), 1);
    assert_eq!(target.len(), 1);
    assert_eq!(source[0].signature.as_deref(), Some("integer"));
    assert!(select_schema_object_pair(&functions, &functions, &["lookup".into()]).is_err());

    let triggers = vec![
        datazen_driver_api::DatabaseObject {
            kind: "trigger".into(),
            schema: Some("public".into()),
            name: "audit".into(),
            signature: None,
            target_schema: Some("public".into()),
            target_name: Some("orders".into()),
        },
        datazen_driver_api::DatabaseObject {
            kind: "trigger".into(),
            schema: Some("public".into()),
            name: "audit".into(),
            signature: None,
            target_schema: Some("public".into()),
            target_name: Some("invoices".into()),
        },
    ];
    let (source, target) = select_schema_object_pair(
        &triggers,
        &triggers,
        &["public.audit ON public.orders".into()],
    )
    .unwrap();
    assert_eq!(source.len(), 1);
    assert_eq!(target.len(), 1);
    assert_eq!(source[0].target_name.as_deref(), Some("orders"));
    assert!(select_schema_object_pair(&triggers, &triggers, &["audit".into()]).is_err());
}
