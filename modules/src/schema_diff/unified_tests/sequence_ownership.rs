use super::*;
use crate::schema_diff::object_identity::{
    SchemaObjectDependencySnapshot, SequenceDependencyUsage, SequenceDependencyUsageKind,
};
use crate::schema_diff::operations::MigrationOperation as HostMigrationOperation;
use datazen_driver_api::{
    MigrationOperation as DriverMigrationOperation, MigrationSequence, MigrationStatement,
};

struct SequencePhaseRenderer;

impl MigrationRenderer for SequencePhaseRenderer {
    fn render(&self, operation: &DriverMigrationOperation) -> Result<MigrationStatement, String> {
        match operation {
            DriverMigrationOperation::CreateSequence { sequence } => Ok(MigrationStatement {
                sql: sequence.definition.clone(),
                rollback_sql: Some("DROP SEQUENCE public.orders_id_seq".into()),
                summary: format!("sequence+{}", sequence.name),
                risk: MigrationRisk::Additive,
            }),
            DriverMigrationOperation::CreateTable { table, .. } => Ok(MigrationStatement {
                sql: format!("CREATE TABLE {table} (id bigint)"),
                rollback_sql: Some(format!("DROP TABLE {table}")),
                summary: format!("table+{table}"),
                risk: MigrationRisk::Additive,
            }),
            other => Err(format!("unexpected sequence test operation: {other:?}")),
        }
    }
}

fn identity(kind: ObjectKind, schema: &str, name: &str) -> SchemaObjectIdentity {
    SchemaObjectIdentity {
        kind,
        schema: Some(schema.into()),
        name: name.into(),
        signature: None,
        target_schema: None,
        target_name: None,
    }
}

fn sequence_fixture() -> (
    SchemaObjectSnapshot,
    SchemaObjectIdentity,
    TableSchema,
    TableSchema,
    SchemaObjectDependencySnapshot,
) {
    let sequence_identity = identity(ObjectKind::Sequence, "public", "orders_id_seq");
    let table_identity = identity(ObjectKind::Table, "public", "orders");
    let sequence = SchemaObjectSnapshot::sequence(
        Some("public"),
        "orders_id_seq",
        "CREATE SEQUENCE public.orders_id_seq AS bigint; ALTER SEQUENCE public.orders_id_seq OWNED BY public.orders.id;",
    )
    .with_dependencies(vec![table_identity.clone()])
    .with_sequence_dependency_usages(vec![SequenceDependencyUsage {
        sequence: sequence_identity.clone(),
        owner_table: table_identity.clone(),
        column_name: "id".into(),
        usage: SequenceDependencyUsageKind::OwnedBy,
    }]);
    let mut source = table("orders", vec![column("id", "bigint")], vec![]);
    source.columns[0].default_value = Some("nextval('public.orders_id_seq'::regclass)".into());
    let target = table("orders", vec![], vec![]);
    let table_dependencies = SchemaObjectDependencySnapshot {
        identity: table_identity.clone(),
        dependencies: Some(vec![sequence_identity]),
        type_dependency_usages: None,
        sequence_dependency_usages: Some(vec![SequenceDependencyUsage {
            sequence: identity(ObjectKind::Sequence, "public", "orders_id_seq"),
            owner_table: table_identity.clone(),
            column_name: "id".into(),
            usage: SequenceDependencyUsageKind::ColumnDefault,
        }]),
    };
    (sequence, table_identity, source, target, table_dependencies)
}

fn plan_owned_sequence(
    sequence: SchemaObjectSnapshot,
    table_identity: SchemaObjectIdentity,
    source: TableSchema,
    target: TableSchema,
    table_dependencies: Vec<SchemaObjectDependencySnapshot>,
    selected_table: bool,
    target_catalog: Vec<SchemaObjectIdentity>,
) -> SchemaDiffPlan {
    let table_pairs = if selected_table {
        vec![("public.orders".into(), source, target)]
    } else {
        vec![]
    };
    let target_dependency_tables = if target_catalog.contains(&table_identity) {
        vec![(
            "public.orders".into(),
            table("orders", vec![column("id", "bigint")], vec![]),
        )]
    } else {
        vec![]
    };
    build_unified_schema_diff_plan_with_components(
        &table_pairs,
        &[],
        &[sequence],
        &[],
        &[],
        &table_dependencies,
        &target_catalog,
        true,
        &target_dependency_tables,
        "postgresql",
        "postgresql",
        Some("public"),
        Some("test_db"),
        false,
        true,
        &[],
        &SequencePhaseRenderer,
        &TestCapabilities,
    )
}

#[test]
fn owned_sequence_apply_and_rollback_split_around_owner_table() {
    let (sequence, owner, source, target, table_dependencies) = sequence_fixture();
    let plan = plan_owned_sequence(
        sequence,
        owner,
        source,
        target,
        vec![table_dependencies],
        true,
        vec![],
    );
    assert!(plan.requirements.is_empty(), "{:?}", plan.requirements);
    assert_eq!(plan.statements.len(), 3);
    assert_eq!(
        plan.statements[0].sql,
        "CREATE SEQUENCE public.orders_id_seq AS bigint"
    );
    assert_eq!(
        plan.statements[1].sql,
        "CREATE TABLE public.orders (id bigint)"
    );
    assert_eq!(
        plan.statements[2].sql,
        "ALTER SEQUENCE public.orders_id_seq OWNED BY public.orders.id"
    );
    assert_eq!(
        plan.statements
            .iter()
            .map(|statement| statement.rollback_sql.as_deref().unwrap_or_default())
            .collect::<Vec<_>>(),
        [
            "DROP SEQUENCE public.orders_id_seq",
            "DROP TABLE public.orders",
            "ALTER SEQUENCE public.orders_id_seq OWNED BY NONE"
        ]
    );
    let reverse = plan
        .statements
        .iter()
        .rev()
        .map(|statement| statement.rollback_sql.as_deref().unwrap_or_default())
        .collect::<Vec<_>>();
    assert_eq!(
        reverse,
        [
            "ALTER SEQUENCE public.orders_id_seq OWNED BY NONE",
            "DROP TABLE public.orders",
            "DROP SEQUENCE public.orders_id_seq"
        ]
    );
}

#[test]
fn owned_sequence_blocks_without_exact_owner_column_usage_or_selected_table_operation() {
    let (sequence, owner, source, target, table_dependencies) = sequence_fixture();
    let missing_sequence_usage = SchemaObjectSnapshot::sequence(
        Some("public"),
        "orders_id_seq",
        "CREATE SEQUENCE public.orders_id_seq AS bigint; ALTER SEQUENCE public.orders_id_seq OWNED BY public.orders.id;",
    )
    .with_dependencies(vec![owner.clone()]);
    let blocked = plan_owned_sequence(
        missing_sequence_usage,
        owner.clone(),
        source.clone(),
        target.clone(),
        vec![table_dependencies.clone()],
        true,
        vec![],
    );
    assert!(blocked.statements.is_empty());
    assert!(format!("{:?}", blocked.requirements).contains("exact OWNED BY relation"));

    let wrong_owner =
        sequence
            .clone()
            .with_sequence_dependency_usages(vec![SequenceDependencyUsage {
                sequence: identity(ObjectKind::Sequence, "public", "orders_id_seq"),
                owner_table: owner.clone(),
                column_name: "other_id".into(),
                usage: SequenceDependencyUsageKind::OwnedBy,
            }]);
    let blocked = plan_owned_sequence(
        wrong_owner,
        owner.clone(),
        source.clone(),
        target.clone(),
        vec![table_dependencies.clone()],
        true,
        vec![],
    );
    assert!(blocked.statements.is_empty());
    assert!(format!("{:?}", blocked.requirements).contains("exact OWNED BY relation"));

    let blocked = plan_owned_sequence(
        sequence,
        owner.clone(),
        source,
        target,
        vec![table_dependencies],
        false,
        vec![owner.clone()],
    );
    assert!(blocked.statements.is_empty());
    assert!(
        format!("{:?}", blocked.requirements).contains("Select the owner table/column operation")
    );
}

#[test]
fn sequence_phase_renderer_uses_validated_definition_and_fails_closed_on_owner_mismatch() {
    let sequence = MigrationSequence {
        schema: Some("public".into()),
        name: "orders_id_seq".into(),
        definition: "CREATE SEQUENCE public.orders_id_seq AS bigint; ALTER SEQUENCE public.orders_id_seq OWNED BY public.orders.id;".into(),
    };
    let operation = HostMigrationOperation::SetSequenceOwnership {
        sequence,
        ownership: super::super::super::object_identity::SequenceOwnershipIdentity {
            schema: "public".into(),
            table: "wrong_table".into(),
            column: "id".into(),
        },
    };
    assert!(render_sequence_phase(&SequencePhaseRenderer, &operation).is_err());
}

#[test]
fn owned_sequence_drop_and_replace_block_before_rendering() {
    let (sequence, owner, _, _, _) = sequence_fixture();
    let sequence_id = sequence.identity();
    let owned_target = sequence.clone();
    let drop_plan = build_unified_schema_diff_plan_with_components(
        &[],
        &["public.orders".into()],
        &[],
        std::slice::from_ref(&owned_target),
        std::slice::from_ref(&owned_target),
        &[],
        &[owner.clone(), sequence_id.clone()],
        true,
        &[(
            "public.orders".into(),
            table("orders", vec![column("id", "bigint")], vec![]),
        )],
        "postgresql",
        "postgresql",
        Some("public"),
        Some("test_db"),
        true,
        true,
        &[],
        &SequencePhaseRenderer,
        &TestCapabilities,
    );
    assert!(drop_plan.statements.is_empty());
    assert!(format!("{:?}", drop_plan.requirements).contains("OWNED BY table relationship"));

    let desired = SchemaObjectSnapshot::sequence(
        Some("public"),
        "orders_id_seq",
        "CREATE SEQUENCE public.orders_id_seq AS bigint INCREMENT BY 2; ALTER SEQUENCE public.orders_id_seq OWNED BY public.orders.id;",
    )
    .with_dependencies(vec![owner.clone()])
    .with_sequence_dependency_usages(
        owned_target.sequence_dependency_usages.clone().unwrap_or_default(),
    );
    let replace_plan = build_unified_schema_diff_plan_with_components(
        &[],
        &[],
        &[desired],
        std::slice::from_ref(&owned_target),
        std::slice::from_ref(&owned_target),
        &[],
        &[owner, sequence_id],
        true,
        &[],
        "postgresql",
        "postgresql",
        Some("public"),
        Some("test_db"),
        true,
        true,
        &[],
        &SequencePhaseRenderer,
        &TestCapabilities,
    );
    assert!(replace_plan.statements.is_empty());
    assert!(format!("{:?}", replace_plan.requirements).contains("OWNED BY table relationship"));
}
