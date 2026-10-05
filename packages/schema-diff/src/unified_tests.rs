use super::*;
use crate::object_identity::SchemaObjectIdentity;
use crate::operations::MigrationOperation as HostMigrationOperation;
use datazen_driver_api::{
    CheckConstraint, ColumnSchema, ForeignKeyDeferrability, ForeignKeyInfo, MigrationOperation,
    MigrationRisk, MigrationStatement, ObjectKind, TableOptions, TableSchema,
};

#[path = "unified_tests/scope_mapping.rs"]
mod scope_mapping;
#[path = "unified_tests/sequence_ownership.rs"]
mod sequence_ownership;
#[path = "unified_tests/table_catalog.rs"]
mod table_catalog;
#[path = "unified_tests/type_validation.rs"]
mod type_validation;
#[path = "unified_tests/view_self_dependency.rs"]
mod view_self_dependency;

struct TestRenderer {
    omit_rollback: bool,
}

impl MigrationRenderer for TestRenderer {
    fn render(&self, operation: &MigrationOperation) -> Result<MigrationStatement, String> {
        let (summary, risk) = match operation {
            MigrationOperation::CreateType { type_definition } => (
                format!("type+{}", type_definition.name),
                MigrationRisk::Additive,
            ),
            MigrationOperation::DropType { type_definition } => (
                format!("type-{}", type_definition.name),
                MigrationRisk::Destructive,
            ),
            MigrationOperation::CreateTable { table, columns, .. } => (
                format!("table+{table}:{columns:?}"),
                MigrationRisk::Additive,
            ),
            MigrationOperation::DropTable { table } => {
                (format!("table-{table}"), MigrationRisk::Destructive)
            }
            MigrationOperation::AddForeignKey { table, foreign_key } => (
                format!("fk+{table}:{}", foreign_key.name),
                MigrationRisk::Additive,
            ),
            MigrationOperation::CreateView { view } => {
                (format!("view+{}", view.name), MigrationRisk::Additive)
            }
            MigrationOperation::CreateRoutine { routine } => {
                (format!("routine+{}", routine.name), MigrationRisk::Additive)
            }
            MigrationOperation::CreateSequence { sequence } => (
                format!("sequence+{}", sequence.name),
                MigrationRisk::Additive,
            ),
            MigrationOperation::CreateTrigger { trigger } => {
                (format!("trigger+{}", trigger.name), MigrationRisk::Additive)
            }
            other => (format!("{other:?}"), MigrationRisk::Rewrite),
        };
        Ok(MigrationStatement {
            sql: format!("-- {summary}"),
            risk,
            rollback_sql: (!self.omit_rollback).then(|| format!("-- undo {summary}")),
            summary,
        })
    }
}

struct TestCapabilities;

impl MigrationCapabilities for TestCapabilities {
    fn supports(&self, _: &MigrationOperation) -> bool {
        true
    }
}

fn column(name: &str, data_type: &str) -> ColumnSchema {
    ColumnSchema {
        name: name.into(),
        data_type: data_type.into(),
        nullable: true,
        default_value: None,
        comment: None,
        is_primary_key: false,
        is_auto_increment: false,
    }
}

fn table(name: &str, columns: Vec<ColumnSchema>, foreign_keys: Vec<ForeignKeyInfo>) -> TableSchema {
    TableSchema {
        table_name: name.into(),
        columns,
        primary_keys: Vec::new(),
        indexes: Vec::new(),
        foreign_keys,
        check_constraints: Vec::<CheckConstraint>::new(),
        table_options: TableOptions::default(),
    }
}

fn foreign_key(referenced_table: &str) -> ForeignKeyInfo {
    ForeignKeyInfo {
        name: "fk_accounts_parent".into(),
        columns: vec!["parent_id".into()],
        referenced_table: referenced_table.into(),
        referenced_columns: vec!["id".into()],
        on_update: "NO ACTION".into(),
        on_delete: "NO ACTION".into(),
        deferrability: ForeignKeyDeferrability::NotDeferrable,
    }
}

fn target_table(name: &str) -> SchemaObjectIdentity {
    SchemaObjectIdentity::table(Some("public"), name)
}

#[test]
fn physical_layout_blocker_prevents_unified_pk_change_and_partial_sql() {
    let mut source = table(
        "blocked",
        vec![column("id", "integer"), column("new_id", "integer")],
        vec![],
    );
    source.primary_keys = vec!["new_id".into()];
    source
        .table_options
        .migration_blockers
        .push("nonclustered primary key cannot be represented".into());
    let mut target = table(
        "blocked",
        vec![column("id", "integer"), column("new_id", "integer")],
        vec![],
    );
    target.primary_keys = vec!["id".into()];

    let safe_source = table(
        "safe",
        vec![column("id", "integer"), column("email", "text")],
        vec![],
    );
    let safe_target = table("safe", vec![column("id", "integer")], vec![]);
    let renderer = TestRenderer {
        omit_rollback: false,
    };
    let plan = build(
        &[
            ("blocked".into(), source, target),
            ("safe".into(), safe_source, safe_target),
        ],
        &[],
        &[],
        &[],
        &[],
        &[],
        true,
        &renderer,
    );

    assert!(plan.statements.is_empty(), "{:?}", plan.statements);
    assert!(plan.requirements.iter().any(|requirement| matches!(
        requirement,
        PlanRequirement::Unsupported { operation, reason }
            if operation == "table:blocked" && reason.contains("nonclustered primary key")
    )));
}

fn build(
    table_pairs: &[(String, TableSchema, TableSchema)],
    target_only_tables: &[String],
    source_objects: &[SchemaObjectSnapshot],
    target_objects: &[SchemaObjectSnapshot],
    target_catalog: &[SchemaObjectIdentity],
    target_dependency_tables: &[(String, TableSchema)],
    allow_destructive: bool,
    renderer: &TestRenderer,
) -> SchemaDiffPlan {
    build_with_catalog(
        table_pairs,
        target_only_tables,
        source_objects,
        target_objects,
        target_objects,
        target_catalog,
        target_dependency_tables,
        allow_destructive,
        renderer,
    )
}

#[allow(clippy::too_many_arguments)]
fn build_with_catalog(
    table_pairs: &[(String, TableSchema, TableSchema)],
    target_only_tables: &[String],
    source_objects: &[SchemaObjectSnapshot],
    target_objects: &[SchemaObjectSnapshot],
    target_catalog_objects: &[SchemaObjectSnapshot],
    target_catalog: &[SchemaObjectIdentity],
    target_dependency_tables: &[(String, TableSchema)],
    allow_destructive: bool,
    renderer: &TestRenderer,
) -> SchemaDiffPlan {
    build_unified_schema_diff_plan_with_components(
        table_pairs,
        target_only_tables,
        source_objects,
        target_objects,
        target_catalog_objects,
        &[],
        target_catalog,
        true,
        target_dependency_tables,
        "postgresql",
        "postgresql",
        Some("public"),
        Some("test_db"),
        allow_destructive,
        true,
        &[],
        renderer,
        &TestCapabilities,
    )
}

#[test]
fn creates_type_table_fk_view_routine_and_trigger_in_dependency_order() {
    let parent = target_table("parents");
    let accounts = target_table("accounts");
    let routine_id = SchemaObjectIdentity {
        kind: ObjectKind::Function,
        schema: Some("public".into()),
        name: "audit_row".into(),
        signature: Some("integer".into()),
        target_schema: None,
        target_name: None,
    };
    let status_type = SchemaObjectSnapshot::type_definition(
        Some("public"),
        "account_status",
        "CREATE TYPE public.account_status AS ENUM ('open', 'closed')",
    )
    .with_dependencies(vec![]);
    let routine = SchemaObjectSnapshot::routine(
        ObjectKind::Function,
        Some("public"),
        "audit_row",
        Some("integer"),
        "CREATE FUNCTION audit_row(integer) RETURNS integer AS $$ SELECT 1 $$",
    )
    .with_dependencies(vec![]);
    let sequence = SchemaObjectSnapshot::sequence(
        Some("public"),
        "account_id_seq",
        "CREATE SEQUENCE \"public\".\"account_id_seq\" AS bigint INCREMENT BY 1 MINVALUE 1 MAXVALUE 9223372036854775807 START WITH 1 CACHE 1 NO CYCLE",
    )
    .with_dependencies(vec![]);
    let view =
        SchemaObjectSnapshot::view(Some("public"), "account_view", "SELECT id FROM accounts")
            .with_dependencies(vec![accounts.clone(), routine_id.clone()]);
    let trigger = SchemaObjectSnapshot::trigger(
        Some("public"),
        "accounts_audit",
        Some("public"),
        "accounts",
        "CREATE TRIGGER accounts_audit AFTER INSERT ON accounts EXECUTE FUNCTION audit_row(1)",
    )
    .with_dependencies(vec![accounts.clone(), routine_id]);

    let mut accounts_source = table(
        "accounts",
        vec![
            column("id", "integer"),
            column("status", "public.account_status"),
        ],
        vec![foreign_key("public.parents")],
    );
    accounts_source.primary_keys = vec!["id".into()];
    let accounts_target = table("accounts", vec![], vec![]);
    let mut child_source = table(
        "account_events",
        vec![column("id", "integer"), column("account_id", "integer")],
        vec![foreign_key("public.accounts")],
    );
    child_source.foreign_keys[0].name = "fk_events_accounts".into();
    let child_target = table(
        "account_events",
        vec![column("id", "integer"), column("account_id", "integer")],
        vec![],
    );
    let table_pairs = vec![
        ("public.accounts".into(), accounts_source, accounts_target),
        ("public.account_events".into(), child_source, child_target),
    ];
    let target_dependency_tables = vec![
        (
            "public.parents".into(),
            table("parents", vec![column("id", "integer")], vec![]),
        ),
        (
            "public.account_events".into(),
            table(
                "account_events",
                vec![column("id", "integer"), column("account_id", "integer")],
                vec![],
            ),
        ),
    ];
    let plan = build(
        &table_pairs,
        &[],
        &[status_type, routine, sequence, view, trigger],
        &[],
        &[parent.clone(), target_table("account_events")],
        &target_dependency_tables,
        false,
        &TestRenderer {
            omit_rollback: false,
        },
    );

    assert!(plan.requirements.is_empty(), "{:?}", plan.requirements);
    let summaries = plan
        .statements
        .iter()
        .map(|statement| statement.summary.as_str())
        .collect::<Vec<_>>();
    let position = |prefix: &str| {
        summaries
            .iter()
            .position(|summary| summary.starts_with(prefix))
            .unwrap()
    };
    assert!(position("type+") < position("table+public.accounts"));
    assert!(position("table+public.accounts") < position("fk+public.accounts"));
    assert!(position("routine+") < position("view+"));
    assert!(position("table+public.accounts") < position("trigger+"));
}

#[test]
fn existing_unselected_target_dependency_is_accepted_but_missing_or_ambiguous_is_blocked() {
    let accepted = build(
        &[],
        &[],
        &[
            SchemaObjectSnapshot::view(Some("public"), "new_view", "SELECT id FROM parents")
                .with_dependencies(vec![target_table("parents")]),
        ],
        &[],
        &[target_table("parents")],
        &[],
        false,
        &TestRenderer {
            omit_rollback: false,
        },
    );
    assert_eq!(accepted.statements.len(), 1);
    assert!(accepted.requirements.is_empty());

    let missing = SchemaObjectSnapshot::view(Some("public"), "new_view", "SELECT id FROM absent")
        .with_dependencies(vec![target_table("absent")]);
    let blocked = build(
        &[],
        &[],
        &[missing],
        &[],
        &[],
        &[],
        false,
        &TestRenderer {
            omit_rollback: false,
        },
    );
    assert!(blocked.statements.is_empty());
    assert!(
        format!("{:?}", blocked.requirements).contains("not present"),
        "requirements: {:?}; statements: {:?}",
        blocked.requirements,
        blocked.statements
    );

    let wrong_schema =
        SchemaObjectSnapshot::view(Some("public"), "new_view", "SELECT id FROM parents")
            .with_dependencies(vec![SchemaObjectIdentity::table(Some("wrong"), "parents")]);
    let ambiguous = build(
        &[],
        &[],
        &[wrong_schema],
        &[],
        &[
            target_table("parents"),
            SchemaObjectIdentity::table(Some("archive"), "parents"),
        ],
        &[],
        false,
        &TestRenderer {
            omit_rollback: false,
        },
    );
    assert!(ambiguous.statements.is_empty());
    assert!(format!("{:?}", ambiguous.requirements).contains("ambiguous"));
}

#[test]
fn opaque_and_cyclic_object_dependencies_produce_no_executable_prefix() {
    let opaque = SchemaObjectSnapshot::view(Some("public"), "opaque_view", "SELECT 1");
    let opaque_plan = build(
        &[],
        &[],
        &[opaque],
        &[],
        &[],
        &[],
        false,
        &TestRenderer {
            omit_rollback: false,
        },
    );
    assert!(opaque_plan.statements.is_empty());
    assert!(format!("{:?}", opaque_plan.requirements).contains("opaque"));

    let left = SchemaObjectIdentity {
        kind: ObjectKind::View,
        schema: Some("public".into()),
        name: "left_view".into(),
        signature: None,
        target_schema: None,
        target_name: None,
    };
    let right = SchemaObjectIdentity {
        kind: ObjectKind::View,
        name: "right_view".into(),
        ..left.clone()
    };
    let cycle = build(
        &[],
        &[],
        &[
            SchemaObjectSnapshot::view(Some("public"), "left_view", "SELECT 1")
                .with_dependencies(vec![right.clone()]),
            SchemaObjectSnapshot::view(Some("public"), "right_view", "SELECT 2")
                .with_dependencies(vec![left]),
        ],
        &[],
        &[],
        &[],
        false,
        &TestRenderer {
            omit_rollback: false,
        },
    );
    assert!(cycle.statements.is_empty());
    assert!(format!("{:?}", cycle.requirements).contains("cycle"));
}

#[test]
fn selected_table_drop_is_blocked_by_unselected_view_and_type_users_order_in_reverse() {
    let users = target_table("users");
    let unselected_view =
        SchemaObjectSnapshot::view(Some("public"), "users_view", "SELECT id FROM users")
            .with_dependencies(vec![users.clone()]);
    let blocked = build_with_catalog(
        &[],
        &["public.users".into()],
        &[],
        &[],
        &[unselected_view],
        &[users.clone()],
        &[(
            "public.users".into(),
            table("users", vec![column("id", "integer")], vec![]),
        )],
        true,
        &TestRenderer {
            omit_rollback: false,
        },
    );
    assert!(
        blocked.statements.is_empty(),
        "requirements: {:?}; statements: {:?}",
        blocked.requirements,
        blocked.statements
    );
    assert!(format!("{:?}", blocked.requirements).contains("Unselected target object"));

    let color = SchemaObjectSnapshot::type_definition(
        Some("public"),
        "color_code",
        "CREATE TYPE public.color_code AS ENUM ('r', 'b')",
    )
    .with_dependencies(vec![]);
    let target_table_snapshot = table("colors", vec![column("code", "public.color_code")], vec![]);
    let drop_plan = build(
        &[],
        &["public.colors".into()],
        &[],
        &[color],
        &[
            target_table("colors"),
            SchemaObjectIdentity {
                kind: ObjectKind::Type,
                schema: Some("public".into()),
                name: "color_code".into(),
                signature: None,
                target_schema: None,
                target_name: None,
            },
        ],
        &[("public.colors".into(), target_table_snapshot)],
        true,
        &TestRenderer {
            omit_rollback: false,
        },
    );
    assert!(
        drop_plan.requirements.is_empty(),
        "{:?}",
        drop_plan.requirements
    );
    let drop_summaries = drop_plan
        .statements
        .iter()
        .map(|statement| statement.summary.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        drop_summaries,
        vec!["table-public.colors", "type-color_code"]
    );
}

#[test]
fn incomplete_rollback_is_reported_without_fabricating_inverse_sql() {
    let target_type = SchemaObjectSnapshot::type_definition(
        Some("public"),
        "old_status",
        "CREATE TYPE public.old_status AS ENUM ('old')",
    )
    .with_dependencies(vec![]);
    let plan = build(
        &[],
        &[],
        &[],
        &[target_type],
        &[SchemaObjectIdentity {
            kind: ObjectKind::Type,
            schema: Some("public".into()),
            name: "old_status".into(),
            signature: None,
            target_schema: None,
            target_name: None,
        }],
        &[(
            "public.some_table".into(),
            table("some_table", vec![], vec![]),
        )],
        true,
        &TestRenderer {
            omit_rollback: true,
        },
    );
    assert_eq!(plan.statements.len(), 1);
    assert!(!plan.rollback_completeness.complete);
    assert_eq!(plan.rollback_completeness.missing, vec!["type-old_status"]);
    assert!(plan.statements[0].rollback_sql.is_none());
}

#[test]
fn object_creation_blocks_when_source_schema_cannot_map_to_target_scope() {
    let source = SchemaObjectSnapshot::view(Some("source_schema"), "report", "SELECT 1")
        .with_dependencies(vec![]);
    let plan = build_unified_schema_diff_plan_with_components(
        &[],
        &[],
        &[source],
        &[],
        &[],
        &[],
        &[],
        true,
        &[],
        "postgresql",
        "postgresql",
        Some("target_schema"),
        Some("test_db"),
        false,
        true,
        &[],
        &TestRenderer {
            omit_rollback: false,
        },
        &TestCapabilities,
    );
    assert!(plan.statements.is_empty());
    assert!(format!("{:?}", plan.requirements)
        .contains("Cross-scope migration is only proven for MySQL views"));
}

#[test]
fn table_only_cross_dialect_plan_preserves_type_overrides_and_dialect_flag() {
    let source = table("items", vec![column("id", "integer")], vec![]);
    let target = table("items", vec![], vec![]);
    let plan = build_unified_schema_diff_plan_with_components(
        &[("items".into(), source, target)],
        &[],
        &[],
        &[],
        &[],
        &[],
        &[],
        true,
        &[],
        "postgresql",
        "mysql",
        None,
        Some("target_db"),
        false,
        true,
        &[ColumnTypeOverride {
            table: "items".into(),
            column: "id".into(),
            target_type: "varchar(12)".into(),
        }],
        &TestRenderer {
            omit_rollback: false,
        },
        &TestCapabilities,
    );
    assert!(!plan.same_dialect);
    assert!(plan.requirements.is_empty(), "{:?}", plan.requirements);
    assert!(plan.statements[0].sql.contains("varchar(12)"));

    let source_object = SchemaObjectSnapshot::view(None, "view", "SELECT 1");
    let blocked = build_unified_schema_diff_plan_with_components(
        &[],
        &[],
        &[source_object],
        &[],
        &[],
        &[],
        &[],
        true,
        &[],
        "postgresql",
        "mysql",
        None,
        Some("target_db"),
        false,
        true,
        &[],
        &TestRenderer {
            omit_rollback: false,
        },
        &TestCapabilities,
    );
    assert!(!blocked.same_dialect);
    assert!(blocked.statements.is_empty());
    assert!(!blocked.requirements.is_empty());
}

#[test]
fn table_catalog_sequence_edges_order_unowned_sequence_and_block_owned_default_cycle() {
    let accounts = target_table("orders");
    let sequence_id = SchemaObjectIdentity {
        kind: ObjectKind::Sequence,
        schema: Some("public".into()),
        name: "orders_id_seq".into(),
        signature: None,
        target_schema: None,
        target_name: None,
    };
    let sequence = SchemaObjectSnapshot::sequence(
        Some("public"),
        "orders_id_seq",
        "CREATE SEQUENCE \"public\".\"orders_id_seq\" AS bigint INCREMENT BY 1 MINVALUE 1 MAXVALUE 9223372036854775807 START WITH 1 CACHE 1 NO CYCLE;",
    )
    .with_dependencies(vec![]);
    let mut source = table("orders", vec![column("id", "bigint")], vec![]);
    source.columns[0].default_value = Some("nextval('public.orders_id_seq'::regclass)".into());
    let target = table("orders", vec![], vec![]);
    let source_table_dependencies = vec![SchemaObjectDependencySnapshot {
        identity: accounts.clone(),
        dependencies: Some(vec![sequence_id.clone()]),
        type_dependency_usages: None,
        sequence_dependency_usages: None,
    }];
    let ordered = build_unified_schema_diff_plan_with_components(
        &[("public.orders".into(), source.clone(), target.clone())],
        &[],
        &[sequence.clone()],
        &[],
        &[],
        &source_table_dependencies,
        &[],
        true,
        &[],
        "postgresql",
        "postgresql",
        Some("public"),
        Some("test_db"),
        false,
        true,
        &[],
        &TestRenderer {
            omit_rollback: false,
        },
        &TestCapabilities,
    );
    assert!(
        ordered.requirements.is_empty(),
        "{:?}",
        ordered.requirements
    );
    assert!(ordered.statements[0].summary.starts_with("sequence+"));

    let owned_sequence = sequence.with_dependencies(vec![accounts.clone()]);
    let cycle = build_unified_schema_diff_plan_with_components(
        &[("public.orders".into(), source, target)],
        &[],
        &[owned_sequence],
        &[],
        &[],
        &source_table_dependencies,
        &[],
        true,
        &[],
        "postgresql",
        "postgresql",
        Some("public"),
        Some("test_db"),
        false,
        true,
        &[],
        &TestRenderer {
            omit_rollback: false,
        },
        &TestCapabilities,
    );
    assert!(cycle.statements.is_empty());
    assert!(format!("{:?}", cycle.requirements).contains("cycle"));
}

#[test]
fn test_tester_unified_object_diff_emits_exact_replace_and_drop_operations() {
    let source = [
        SchemaObjectSnapshot::view(Some("public"), "active_view", "SELECT 2"),
        SchemaObjectSnapshot::routine(
            ObjectKind::Function,
            Some("public"),
            "audit_row",
            None,
            "CREATE FUNCTION audit_row() RETURNS integer AS $$ SELECT 2 $$",
        ),
        SchemaObjectSnapshot::trigger(
            Some("public"),
            "orders_audit",
            Some("public"),
            "orders",
            "CREATE TRIGGER orders_audit AFTER INSERT ON orders EXECUTE FUNCTION audit_row()",
        ),
        SchemaObjectSnapshot::sequence(
            Some("public"),
            "orders_id_seq",
            "CREATE SEQUENCE public.orders_id_seq AS bigint INCREMENT BY 2",
        ),
        SchemaObjectSnapshot::type_definition(
            Some("public"),
            "order_state",
            "CREATE TYPE public.order_state AS ENUM ('new', 'ready')",
        ),
    ];
    let target = [
        SchemaObjectSnapshot::view(Some("public"), "active_view", "SELECT 1"),
        SchemaObjectSnapshot::routine(
            ObjectKind::Function,
            Some("public"),
            "audit_row",
            None,
            "CREATE FUNCTION audit_row() RETURNS integer AS $$ SELECT 1 $$",
        ),
        SchemaObjectSnapshot::trigger(
            Some("public"),
            "orders_audit",
            Some("public"),
            "orders",
            "CREATE TRIGGER orders_audit AFTER UPDATE ON orders EXECUTE FUNCTION audit_row()",
        ),
        SchemaObjectSnapshot::sequence(
            Some("public"),
            "orders_id_seq",
            "CREATE SEQUENCE public.orders_id_seq AS bigint INCREMENT BY 1",
        ),
        SchemaObjectSnapshot::type_definition(
            Some("public"),
            "order_state",
            "CREATE TYPE public.order_state AS ENUM ('new')",
        ),
        SchemaObjectSnapshot::view(Some("public"), "retired_view", "SELECT 0"),
    ];

    let batch = crate::unified_objects::diff_schema_objects_to_operations(
        &source,
        &target,
        "postgresql",
        "postgresql",
        true,
    );
    assert!(batch.requirements.is_empty(), "{:?}", batch.requirements);
    assert_eq!(batch.operations.len(), 6);
    assert_eq!(batch.transitions.len(), 6);
    assert!(batch
        .operations
        .iter()
        .any(|operation| matches!(operation, HostMigrationOperation::ReplaceView { .. })));
    assert!(batch
        .operations
        .iter()
        .any(|operation| matches!(operation, HostMigrationOperation::ReplaceRoutine { .. })));
    assert!(batch
        .operations
        .iter()
        .any(|operation| matches!(operation, HostMigrationOperation::ReplaceTrigger { .. })));
    assert!(batch
        .operations
        .iter()
        .any(|operation| matches!(operation, HostMigrationOperation::ReplaceSequence { .. })));
    assert!(batch
        .operations
        .iter()
        .any(|operation| matches!(operation, HostMigrationOperation::ReplaceType { .. })));
    assert!(batch
        .operations
        .iter()
        .any(|operation| matches!(operation, HostMigrationOperation::DropView { view } if view.name == "retired_view")));
}

#[test]
fn test_tester_unified_object_diff_blocks_invalid_scopes_and_unapproved_drops() {
    let source_view = SchemaObjectSnapshot::view(Some("public"), "active_view", "SELECT 1");
    let cross_dialect = crate::unified_objects::diff_schema_objects_to_operations(
        &[source_view.clone()],
        &[],
        "postgresql",
        "mysql",
        false,
    );
    assert!(cross_dialect.operations.is_empty());
    assert!(format!("{:?}", cross_dialect.requirements).contains("Cross-dialect"));

    let mut table_as_object = source_view.clone();
    table_as_object.kind = ObjectKind::Table;
    let unnamed_view = SchemaObjectSnapshot::view(Some("public"), "  ", "SELECT 1");
    let missing_sequence_schema = SchemaObjectSnapshot::sequence(
        None,
        "orders_id_seq",
        "CREATE SEQUENCE orders_id_seq AS bigint",
    );
    let invalid = crate::unified_objects::diff_schema_objects_to_operations(
        &[table_as_object, unnamed_view, missing_sequence_schema],
        &[],
        "postgresql",
        "postgresql",
        false,
    );
    assert!(invalid.operations.is_empty());
    assert_eq!(invalid.requirements.len(), 3, "{:?}", invalid.requirements);

    let duplicate = crate::unified_objects::diff_schema_objects_to_operations(
        &[source_view.clone(), source_view.clone()],
        &[],
        "postgresql",
        "postgresql",
        false,
    );
    assert!(format!("{:?}", duplicate.requirements).contains("duplicate object identities"));

    let skipped_drop = crate::unified_objects::diff_schema_objects_to_operations(
        &[],
        &[source_view.clone()],
        "postgresql",
        "postgresql",
        false,
    );
    assert!(skipped_drop.operations.is_empty());
    assert!(skipped_drop
        .warnings
        .iter()
        .any(|warning| warning.contains("Skipped destructive operation")));
}
