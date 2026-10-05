//! Dependency ordering for migration operations.

use super::dependency_graph::DependencyGraph;
use super::operation_dependency_references::*;
use super::operations::MigrationOperation;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct OperationNodeIdentity {
    object: MigrationObjectIdentity,
    action: OperationAction,
    transition: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum MigrationObjectIdentity {
    Table(String),
    Column {
        table: String,
        column: String,
    },
    Index {
        table: String,
        name: String,
    },
    ForeignKey {
        table: String,
        name: String,
    },
    CheckConstraint {
        table: String,
        name: String,
    },
    View {
        schema: Option<String>,
        name: String,
    },
    Routine {
        kind: datazen_driver_api::ObjectKind,
        schema: Option<String>,
        name: String,
        signature: Option<String>,
    },
    Trigger {
        schema: Option<String>,
        name: String,
        target_schema: Option<String>,
        target_name: String,
    },
    Sequence {
        schema: Option<String>,
        name: String,
    },
    Type {
        schema: Option<String>,
        name: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum OperationAction {
    CreateTable,
    CreateView,
    CreateRoutine,
    CreateTrigger,
    CreateSequence,
    CreateType,
    AddColumn,
    AddPrimaryKey,
    AddCheckConstraint,
    AddForeignKey,
    CreateIndex,
    ReplaceView,
    ReplaceRoutine,
    ReplaceTrigger,
    ReplaceSequence,
    ReplaceType,
    AlterColumnType,
    SetNullable,
    SetDefault,
    SetComment,
    SetTableOptions,
    SetAutoIncrement,
    DropForeignKey,
    DropIndex,
    DropCheckConstraint,
    DropPrimaryKey,
    DropColumn,
    DropView,
    DropRoutine,
    DropTrigger,
    DropSequence,
    DropType,
    DropTable,
}

fn operation_identity(op: &MigrationOperation) -> OperationNodeIdentity {
    use MigrationOperation::*;
    let action = match op {
        CreateTable { .. } => OperationAction::CreateTable,
        DropTable { .. } => OperationAction::DropTable,
        AddColumn { .. } => OperationAction::AddColumn,
        DropColumn { .. } => OperationAction::DropColumn,
        AlterColumnType { .. } => OperationAction::AlterColumnType,
        SetNullable { .. } => OperationAction::SetNullable,
        SetDefault { .. } => OperationAction::SetDefault,
        SetComment { .. } => OperationAction::SetComment,
        SetTableOptions { .. } => OperationAction::SetTableOptions,
        SetAutoIncrement { .. } => OperationAction::SetAutoIncrement,
        AddPrimaryKey { .. } => OperationAction::AddPrimaryKey,
        DropPrimaryKey { .. } => OperationAction::DropPrimaryKey,
        CreateIndex { .. } => OperationAction::CreateIndex,
        DropIndex { .. } => OperationAction::DropIndex,
        AddForeignKey { .. } => OperationAction::AddForeignKey,
        DropForeignKey { .. } => OperationAction::DropForeignKey,
        AddCheckConstraint { .. } => OperationAction::AddCheckConstraint,
        DropCheckConstraint { .. } => OperationAction::DropCheckConstraint,
        CreateView { .. } => OperationAction::CreateView,
        ReplaceView { .. } => OperationAction::ReplaceView,
        DropView { .. } => OperationAction::DropView,
        CreateRoutine { .. } => OperationAction::CreateRoutine,
        ReplaceRoutine { .. } => OperationAction::ReplaceRoutine,
        DropRoutine { .. } => OperationAction::DropRoutine,
        CreateTrigger { .. } => OperationAction::CreateTrigger,
        ReplaceTrigger { .. } => OperationAction::ReplaceTrigger,
        DropTrigger { .. } => OperationAction::DropTrigger,
        CreateSequence { .. } | CreateSequenceUnowned { .. } | SetSequenceOwnership { .. } => {
            OperationAction::CreateSequence
        }
        ReplaceSequence { .. } => OperationAction::ReplaceSequence,
        DropSequence { .. } => OperationAction::DropSequence,
        CreateType { .. } => OperationAction::CreateType,
        ReplaceType { .. } => OperationAction::ReplaceType,
        DropType { .. } => OperationAction::DropType,
    };
    OperationNodeIdentity {
        object: migration_object_identity(op),
        action,
        transition: operation_transition(op),
    }
}

fn migration_object_identity(op: &MigrationOperation) -> MigrationObjectIdentity {
    use MigrationOperation::*;
    match op {
        CreateTable { table, .. }
        | DropTable { table }
        | AddPrimaryKey { table, .. }
        | DropPrimaryKey { table, .. }
        | SetTableOptions { table, .. } => MigrationObjectIdentity::Table(table.clone()),
        AddColumn { table, column } | DropColumn { table, column } => {
            MigrationObjectIdentity::Column {
                table: table.clone(),
                column: column.name.clone(),
            }
        }
        AlterColumnType { table, column, .. }
        | SetNullable { table, column, .. }
        | SetDefault { table, column, .. }
        | SetComment { table, column, .. }
        | SetAutoIncrement { table, column, .. } => MigrationObjectIdentity::Column {
            table: table.clone(),
            column: column.clone(),
        },
        CreateIndex { table, index } | DropIndex { table, index } => {
            MigrationObjectIdentity::Index {
                table: table.clone(),
                name: index.name.clone(),
            }
        }
        AddForeignKey { table, foreign_key } | DropForeignKey { table, foreign_key } => {
            MigrationObjectIdentity::ForeignKey {
                table: table.clone(),
                name: foreign_key.name.clone(),
            }
        }
        AddCheckConstraint { table, constraint } | DropCheckConstraint { table, constraint } => {
            MigrationObjectIdentity::CheckConstraint {
                table: table.clone(),
                name: constraint.name.clone(),
            }
        }
        CreateView { view } | ReplaceView { desired: view, .. } | DropView { view } => {
            MigrationObjectIdentity::View {
                schema: view.schema.clone(),
                name: view.name.clone(),
            }
        }
        CreateRoutine { routine }
        | ReplaceRoutine {
            desired: routine, ..
        }
        | DropRoutine { routine } => MigrationObjectIdentity::Routine {
            kind: routine.kind,
            schema: routine.schema.clone(),
            name: routine.name.clone(),
            signature: routine.signature.clone(),
        },
        CreateTrigger { trigger }
        | ReplaceTrigger {
            desired: trigger, ..
        }
        | DropTrigger { trigger } => MigrationObjectIdentity::Trigger {
            schema: trigger.schema.clone(),
            name: trigger.name.clone(),
            target_schema: trigger.target_schema.clone(),
            target_name: trigger.target_name.clone(),
        },
        CreateSequence { sequence }
        | CreateSequenceUnowned { sequence, .. }
        | SetSequenceOwnership { sequence, .. }
        | ReplaceSequence {
            desired: sequence, ..
        }
        | DropSequence { sequence } => MigrationObjectIdentity::Sequence {
            schema: sequence.schema.clone(),
            name: sequence.name.clone(),
        },
        CreateType { type_definition }
        | ReplaceType {
            desired: type_definition,
            ..
        }
        | DropType { type_definition } => MigrationObjectIdentity::Type {
            schema: type_definition.schema.clone(),
            name: type_definition.name.clone(),
        },
    }
}

fn operation_transition(op: &MigrationOperation) -> String {
    match op {
        MigrationOperation::SetDefault { to, .. } => match to {
            Some(value) => format!("set:{value}"),
            None => "remove".into(),
        },
        MigrationOperation::SetNullable { nullable, .. } => format!("nullable:{nullable}"),
        MigrationOperation::SetComment { to, .. } => format!("comment:{to:?}"),
        MigrationOperation::SetAutoIncrement { to, .. } => format!("auto_increment:{to}"),
        MigrationOperation::SetTableOptions { to, .. } => format!("table_options:{to:?}"),
        MigrationOperation::CreateSequenceUnowned { .. } => "phase:create-unowned".into(),
        MigrationOperation::SetSequenceOwnership { .. } => "phase:set-ownership".into(),
        _ => String::new(),
    }
}

fn op_table(op: &MigrationOperation) -> &str {
    match op {
        MigrationOperation::CreateTable { table, .. }
        | MigrationOperation::DropTable { table }
        | MigrationOperation::AddColumn { table, .. }
        | MigrationOperation::DropColumn { table, .. }
        | MigrationOperation::AlterColumnType { table, .. }
        | MigrationOperation::SetNullable { table, .. }
        | MigrationOperation::SetDefault { table, .. }
        | MigrationOperation::SetComment { table, .. }
        | MigrationOperation::SetTableOptions { table, .. }
        | MigrationOperation::SetAutoIncrement { table, .. }
        | MigrationOperation::AddPrimaryKey { table, .. }
        | MigrationOperation::DropPrimaryKey { table, .. }
        | MigrationOperation::CreateIndex { table, .. }
        | MigrationOperation::DropIndex { table, .. }
        | MigrationOperation::AddForeignKey { table, .. }
        | MigrationOperation::DropForeignKey { table, .. }
        | MigrationOperation::AddCheckConstraint { table, .. }
        | MigrationOperation::DropCheckConstraint { table, .. } => table,
        MigrationOperation::CreateView { view }
        | MigrationOperation::ReplaceView { desired: view, .. }
        | MigrationOperation::DropView { view } => &view.name,
        MigrationOperation::CreateRoutine { routine }
        | MigrationOperation::ReplaceRoutine {
            desired: routine, ..
        }
        | MigrationOperation::DropRoutine { routine } => &routine.name,
        MigrationOperation::CreateTrigger { trigger }
        | MigrationOperation::ReplaceTrigger {
            desired: trigger, ..
        }
        | MigrationOperation::DropTrigger { trigger } => &trigger.name,
        MigrationOperation::CreateSequence { sequence }
        | MigrationOperation::CreateSequenceUnowned { sequence, .. }
        | MigrationOperation::SetSequenceOwnership { sequence, .. }
        | MigrationOperation::ReplaceSequence {
            desired: sequence, ..
        }
        | MigrationOperation::DropSequence { sequence } => &sequence.name,
        MigrationOperation::CreateType { type_definition }
        | MigrationOperation::ReplaceType {
            desired: type_definition,
            ..
        }
        | MigrationOperation::DropType { type_definition } => &type_definition.name,
    }
}

/// A directed edge means `before` must complete before `after`.
fn precedes(before: &MigrationOperation, after: &MigrationOperation) -> bool {
    use MigrationOperation::*;
    match (before, after) {
        (CreateTable { table, .. }, AddForeignKey { foreign_key, .. })
            if *table == op_table(after) || foreign_key_references_table(foreign_key, table) =>
        {
            true
        }
        (CreateTable { table, .. }, CreateTrigger { trigger })
        | (
            CreateTable { table, .. },
            ReplaceTrigger {
                desired: trigger, ..
            },
        ) if trigger_targets_table(trigger, table) => true,
        (DropTrigger { trigger }, DropTable { table }) if trigger_targets_table(trigger, table) => {
            true
        }
        (CreateType { type_definition }, CreateTable { columns, .. })
            if columns
                .iter()
                .any(|column| column_uses_type(column, type_definition)) =>
        {
            true
        }
        (CreateType { type_definition }, AddColumn { column, .. })
            if column_uses_type(column, type_definition) =>
        {
            true
        }
        (DropColumn { column, .. }, DropType { type_definition })
            if column_uses_type(column, type_definition) =>
        {
            true
        }
        (CreateTable { .. }, _) if op_table(before) == op_table(after) => true,
        (AddColumn { table, column }, AddForeignKey { foreign_key, .. })
            if *table == op_table(after)
                && foreign_key.columns.iter().any(|name| name == &column.name) =>
        {
            true
        }
        (AddPrimaryKey { table, columns }, AddForeignKey { foreign_key, .. })
            if foreign_key_references_table(foreign_key, table)
                && columns
                    .iter()
                    .any(|column| foreign_key.referenced_columns.contains(column)) =>
        {
            true
        }
        (CreateIndex { table, index }, AddForeignKey { foreign_key, .. })
            if *table == op_table(after)
                && index
                    .columns
                    .iter()
                    .any(|column| foreign_key.columns.contains(column)) =>
        {
            true
        }
        (
            DropForeignKey {
                table: foreign_key_table,
                foreign_key,
            },
            DropTable { table },
        ) if op_table_name_matches(foreign_key_table, table)
            || foreign_key_references_table(foreign_key, table) =>
        {
            true
        }
        (_, DropTable { table }) if op_table(before) == table => true,
        (
            DropForeignKey {
                table: foreign_key_table,
                foreign_key,
            },
            DropColumn { table, column },
        ) if foreign_key_uses_column(foreign_key_table, foreign_key, table, &column.name) => true,
        (
            DropForeignKey {
                table: foreign_key_table,
                foreign_key,
            },
            DropPrimaryKey { table, columns },
        ) if (op_table_name_matches(foreign_key_table, table)
            && columns
                .iter()
                .any(|column| foreign_key.columns.contains(column)))
            || (foreign_key_references_table(foreign_key, table)
                && columns
                    .iter()
                    .any(|column| foreign_key.referenced_columns.contains(column))) =>
        {
            true
        }
        (
            DropForeignKey {
                table: foreign_key_table,
                foreign_key,
            },
            AlterColumnType { table, column, .. },
        ) if foreign_key_uses_column(foreign_key_table, foreign_key, table, column) => true,
        (
            DropForeignKey {
                table: foreign_key_table,
                foreign_key,
            },
            SetNullable { table, column, .. },
        ) if foreign_key_uses_column(foreign_key_table, foreign_key, table, column) => true,
        (
            DropForeignKey {
                table: foreign_key_table,
                foreign_key,
            },
            SetDefault { table, column, .. },
        ) if foreign_key_uses_column(foreign_key_table, foreign_key, table, column) => true,
        (
            DropForeignKey {
                table: foreign_key_table,
                foreign_key,
            },
            SetComment { table, column, .. },
        ) if foreign_key_uses_column(foreign_key_table, foreign_key, table, column) => true,
        (
            DropForeignKey {
                table: foreign_key_table,
                foreign_key,
            },
            SetAutoIncrement { table, column, .. },
        ) if foreign_key_uses_column(foreign_key_table, foreign_key, table, column) => true,
        (
            DropForeignKey {
                table: foreign_key_table,
                foreign_key,
            },
            DropIndex { table, index },
        ) if op_table_name_matches(foreign_key_table, table)
            && index
                .columns
                .iter()
                .any(|column| foreign_key.columns.contains(column)) =>
        {
            true
        }
        (DropPrimaryKey { .. }, AddPrimaryKey { .. }) => true,
        (
            DropPrimaryKey { columns, .. },
            SetNullable {
                column,
                nullable: true,
                ..
            },
        ) => columns.contains(column),
        (DropIndex { index: old, .. }, CreateIndex { index: new, .. }) => old.name == new.name,
        (
            DropForeignKey {
                foreign_key: old, ..
            },
            AddForeignKey {
                foreign_key: new, ..
            },
        ) => old.name == new.name,
        (DropPrimaryKey { columns, .. }, DropColumn { .. } | AlterColumnType { .. }) => match after
        {
            DropColumn { column, .. } => columns.contains(&column.name),
            AlterColumnType { column, .. } => columns.contains(column),
            _ => false,
        },
        (
            SetDefault {
                table: default_table,
                column: default_column,
                to: None,
                ..
            },
            AlterColumnType {
                table: alter_table,
                column: alter_column,
                ..
            },
        ) if default_table == alter_table && default_column == alter_column => true,
        (
            AlterColumnType {
                table: alter_table,
                column: alter_column,
                ..
            },
            SetDefault {
                table: default_table,
                column: default_column,
                to: Some(_),
                ..
            },
        ) if default_table == alter_table && default_column == alter_column => true,
        (DropIndex { index, .. }, DropColumn { column, .. }) => {
            index.columns.contains(&column.name)
        }
        (DropIndex { index, .. }, AlterColumnType { column, .. }) => index.columns.contains(column),
        (AddColumn { column, .. }, AddPrimaryKey { columns, .. }) => columns.contains(&column.name),
        (AddColumn { column, .. }, CreateIndex { index, .. }) => {
            index.columns.contains(&column.name)
        }
        (
            AlterColumnType { column, .. } | SetNullable { column, .. },
            AddPrimaryKey { columns, .. },
        ) => columns.contains(column),
        (AlterColumnType { column, .. }, CreateIndex { index, .. }) => {
            index.columns.contains(column)
        }
        _ if op_table(before) != op_table(after) => false,
        _ => false,
    }
}

/// Removing a prerequisite also removes its dependents. Replacements are indivisible
/// for selection, while their execution edges remain directional.
pub fn retain_dependency_closed(
    all: &[MigrationOperation],
    selected: &mut Vec<MigrationOperation>,
) {
    loop {
        let previous = selected.clone();
        selected.retain(|op| all.iter().all(|dependency| {
            let replacement = matches!((op, dependency),
                (MigrationOperation::DropPrimaryKey { .. }, MigrationOperation::AddPrimaryKey { .. }))
                && op_table(op) == op_table(dependency)
                || matches!((op, dependency),
                    (MigrationOperation::DropIndex { index: a, .. }, MigrationOperation::CreateIndex { index: b, .. }) if a.name == b.name && op_table(op) == op_table(dependency))
                || matches!((op, dependency),
                    (MigrationOperation::DropForeignKey { foreign_key: a, .. }, MigrationOperation::AddForeignKey { foreign_key: b, .. }) if a.name == b.name && op_table(op) == op_table(dependency));
            let replacement = replacement || matches!((op, dependency),
                (MigrationOperation::DropCheckConstraint { constraint: a, .. }, MigrationOperation::AddCheckConstraint { constraint: b, .. }) if a.name == b.name && op_table(op) == op_table(dependency));
            // The old default must be dropped before changing a column type,
            // but it must not be deployed alone if the type change is filtered.
            let replacement = replacement || matches!((op, dependency),
                (MigrationOperation::SetDefault { table: set_table, column: set_column, to: None, .. }, MigrationOperation::AlterColumnType { table: alter_table, column: alter_column, .. })
                    if set_table == alter_table && set_column == alter_column)
                || matches!((op, dependency),
                    (MigrationOperation::AlterColumnType { table: alter_table, column: alter_column, .. }, MigrationOperation::SetDefault { table: set_table, column: set_column, to: None, .. })
                        if set_table == alter_table && set_column == alter_column);
            !(precedes(dependency, op) || replacement) || previous.iter().any(|present| std::mem::discriminant(present) == std::mem::discriminant(dependency) && present.key() == dependency.key())
        }));
        if selected.len() == previous.len() {
            break;
        }
    }
}

pub(super) fn try_resolve_dependencies(
    ops: &[MigrationOperation],
) -> Result<Vec<MigrationOperation>, String> {
    try_resolve_dependencies_with_table_drop_edges(ops, &[])
}

pub(super) fn try_resolve_dependencies_with_table_drop_edges(
    ops: &[MigrationOperation],
    dependent_before_referenced: &[(String, String)],
) -> Result<Vec<MigrationOperation>, String> {
    let extra_edges = dependent_before_referenced
        .iter()
        .filter_map(|(dependent_table, referenced_table)| {
            let dependent = ops.iter().position(|operation| {
                matches!(operation, MigrationOperation::DropTable { table } if table == dependent_table)
            });
            let referenced = ops.iter().position(|operation| {
                matches!(operation, MigrationOperation::DropTable { table } if table == referenced_table)
            });
            dependent.zip(referenced)
        })
        .collect::<Vec<_>>();
    try_resolve_dependencies_with_operation_edges(ops, &extra_edges)
}

/// Resolve the normal operation graph plus explicit cross-category edges.
/// Each pair is `(prerequisite operation index, dependent operation index)`.
pub(super) fn try_resolve_dependencies_with_operation_edges(
    ops: &[MigrationOperation],
    extra_edges: &[(usize, usize)],
) -> Result<Vec<MigrationOperation>, String> {
    reject_ambiguous_references(ops).map_err(|error| error.to_string())?;

    let identities = ops.iter().map(operation_identity).collect::<Vec<_>>();
    let mut graph = DependencyGraph::new();
    for identity in &identities {
        graph
            .add_node(identity.clone())
            .map_err(|error| error.to_string())?;
    }
    for (before_index, before) in ops.iter().enumerate() {
        for (after_index, after) in ops.iter().enumerate() {
            if before_index != after_index && precedes(before, after) {
                graph
                    .add_dependency(&identities[before_index], &identities[after_index])
                    .map_err(|error| error.to_string())?;
            }
        }
    }

    for (prerequisite, dependent) in extra_edges {
        let prerequisite = identities.get(*prerequisite).ok_or_else(|| {
            format!("unified dependency references missing operation {prerequisite}")
        })?;
        let dependent = identities.get(*dependent).ok_or_else(|| {
            format!("unified dependency references missing operation {dependent}")
        })?;
        graph
            .add_dependency(prerequisite, dependent)
            .map_err(|error| error.to_string())?;
    }

    let ordered = graph
        .topological_order()
        .map_err(|error| error.to_string())?;
    ordered
        .iter()
        .map(|identity| {
            identities
                .iter()
                .position(|candidate| candidate == identity)
                .map(|index| ops[index].clone())
                .ok_or_else(|| format!("migration operation identity was lost: {identity:?}"))
        })
        .collect()
}

pub fn resolve_dependencies(ops: Vec<MigrationOperation>) -> Vec<MigrationOperation> {
    try_resolve_dependencies(&ops).unwrap_or_default()
}

#[cfg(test)]
#[path = "operation_dependency_replacement_tests.rs"]
mod replacement_tests;
#[cfg(test)]
#[path = "operation_dependencies_tests.rs"]
mod tests;
