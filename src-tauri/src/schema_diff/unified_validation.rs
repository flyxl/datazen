//! Cross-kind dependency proof and operation identity helpers for the unified planner.

use super::{
    object_identity::{
        SchemaObjectDependencySnapshot, SchemaObjectIdentity, TypeDependencyUsageKind,
    },
    objects::SchemaObjectSnapshot,
    operations::MigrationOperation,
    types::{normalize_dialect, PlanRequirement, RollbackCompleteness, SchemaDiffPlan},
    unified_objects::operation_node_key,
    unified_type_validation::{is_builtin_type, type_name_matches, type_parts},
};
use crate::db::TableSchema;
use datazen_driver_api::ObjectKind;
use std::collections::{BTreeSet, HashMap};

mod table_catalog;
pub(super) use table_catalog::{
    validate_custom_type_drops, validate_table_catalog_dependencies, validate_table_drop_catalog,
};

pub(super) fn validate_table_dependencies(
    operations: &[MigrationOperation],
    target_catalog: &BTreeSet<SchemaObjectIdentity>,
    identities_created: &HashMap<SchemaObjectIdentity, usize>,
    identities_dropped: &HashMap<SchemaObjectIdentity, usize>,
    node_keys: &[String],
    extra_edges: &mut Vec<(usize, usize)>,
    requirements: &mut Vec<PlanRequirement>,
    dialect: &str,
    target_schema: Option<&str>,
    target_database: Option<&str>,
) {
    for (index, operation) in operations.iter().enumerate() {
        match operation {
            MigrationOperation::AddForeignKey { table, foreign_key } => {
                let child = table_identity(table, dialect, target_schema, target_database);
                let parent = table_identity(
                    &foreign_key.referenced_table,
                    dialect,
                    target_schema,
                    target_database,
                );
                require_dependency(
                    &child,
                    index,
                    operations,
                    target_catalog,
                    identities_created,
                    identities_dropped,
                    extra_edges,
                    requirements,
                    node_keys,
                    "foreign key local table",
                );
                require_dependency(
                    &parent,
                    index,
                    operations,
                    target_catalog,
                    identities_created,
                    identities_dropped,
                    extra_edges,
                    requirements,
                    node_keys,
                    "foreign key referenced table",
                );
            }
            MigrationOperation::CreateTrigger { trigger }
            | MigrationOperation::ReplaceTrigger {
                desired: trigger, ..
            } => {
                let target = table_identity(
                    &trigger.target_name,
                    dialect,
                    trigger.target_schema.as_deref().or(target_schema),
                    target_database,
                );
                require_dependency(
                    &target,
                    index,
                    operations,
                    target_catalog,
                    identities_created,
                    identities_dropped,
                    extra_edges,
                    requirements,
                    node_keys,
                    "trigger target table",
                );
            }
            MigrationOperation::CreateTable { columns, .. } => {
                for column in columns {
                    require_custom_type(
                        &column.data_type,
                        index,
                        target_catalog,
                        identities_created,
                        identities_dropped,
                        extra_edges,
                        requirements,
                        node_keys,
                        dialect,
                    );
                }
            }
            MigrationOperation::AddColumn { column, .. } => require_custom_type(
                &column.data_type,
                index,
                target_catalog,
                identities_created,
                identities_dropped,
                extra_edges,
                requirements,
                node_keys,
                dialect,
            ),
            MigrationOperation::DropType { type_definition } => {
                let dropping = SchemaObjectIdentity {
                    kind: ObjectKind::Type,
                    schema: type_definition.schema.clone(),
                    name: type_definition.name.clone(),
                    signature: None,
                    target_schema: None,
                    target_name: None,
                };
                for (candidate_index, candidate) in operations.iter().enumerate() {
                    if let MigrationOperation::DropColumn { column, .. } = candidate {
                        if type_name_matches(&column.data_type, &dropping, dialect) {
                            extra_edges.push((candidate_index, index));
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn require_custom_type(
    raw_type: &str,
    dependent_index: usize,
    target_catalog: &BTreeSet<SchemaObjectIdentity>,
    identities_created: &HashMap<SchemaObjectIdentity, usize>,
    identities_dropped: &HashMap<SchemaObjectIdentity, usize>,
    extra_edges: &mut Vec<(usize, usize)>,
    requirements: &mut Vec<PlanRequirement>,
    node_keys: &[String],
    dialect: &str,
) {
    let parts = type_parts(raw_type, dialect);
    let Some(last) = parts.last() else {
        return;
    };
    let is_builtin = !last.1 && is_builtin_type(&last.0, dialect)
        || (parts.len() > 1
            && parts[parts.len() - 2].0 == "pg_catalog"
            && is_builtin_type(&last.0, dialect));
    if is_builtin {
        return;
    }
    let actual = if parts.len() == 2 {
        SchemaObjectIdentity {
            kind: ObjectKind::Type,
            schema: Some(parts[0].0.clone()),
            name: last.0.clone(),
            signature: None,
            target_schema: None,
            target_name: None,
        }
    } else {
        SchemaObjectIdentity {
            kind: ObjectKind::Type,
            schema: None,
            name: last.0.clone(),
            signature: None,
            target_schema: None,
            target_name: None,
        }
    };
    let candidates = identities_created
        .keys()
        .chain(target_catalog.iter())
        .filter(|identity| identity.kind == ObjectKind::Type && identity.name == actual.name)
        .collect::<BTreeSet<_>>();
    let exact = candidates
        .iter()
        .copied()
        .filter(|candidate| {
            candidate.name == actual.name
                && (actual.schema.is_none() || candidate.schema == actual.schema)
        })
        .collect::<Vec<_>>();
    if exact.len() > 1 {
        requirements.push(unsupported(
            node_keys.get(dependent_index).map(String::as_str).unwrap_or("column"),
            format!("Column type `{raw_type}` matches multiple custom type identities; qualify it with its exact schema."),
        ));
        return;
    }
    if exact.is_empty() && !candidates.is_empty() {
        requirements.push(unsupported(
            node_keys.get(dependent_index).map(String::as_str).unwrap_or("column"),
            format!("Column type `{raw_type}` only matches a custom type by basename; use an exact schema-qualified identity."),
        ));
        return;
    }
    if let Some(identity) = exact.first() {
        require_dependency(
            identity,
            dependent_index,
            &[],
            target_catalog,
            identities_created,
            identities_dropped,
            extra_edges,
            requirements,
            node_keys,
            "column custom type",
        );
    } else {
        let reason = if actual.schema.is_some() {
            format!(
                "Custom type `{raw_type}` is absent from the complete target snapshot and selected source objects."
            )
        } else {
            format!(
                "Unqualified type `{raw_type}` is not a recognized built-in type or present in the complete target snapshot; qualify it or select its exact source type."
            )
        };
        requirements.push(unsupported(
            node_keys
                .get(dependent_index)
                .map(String::as_str)
                .unwrap_or("column"),
            reason,
        ));
    }
}

#[allow(clippy::too_many_arguments)]
fn require_dependency(
    identity: &SchemaObjectIdentity,
    dependent_index: usize,
    _operations: &[MigrationOperation],
    target_catalog: &BTreeSet<SchemaObjectIdentity>,
    identities_created: &HashMap<SchemaObjectIdentity, usize>,
    identities_dropped: &HashMap<SchemaObjectIdentity, usize>,
    extra_edges: &mut Vec<(usize, usize)>,
    requirements: &mut Vec<PlanRequirement>,
    node_keys: &[String],
    label: &str,
) {
    if let Some(prerequisite_index) = identities_created.get(identity) {
        if *prerequisite_index != dependent_index {
            extra_edges.push((*prerequisite_index, dependent_index));
        }
        return;
    }
    if identities_dropped.contains_key(identity) {
        requirements.push(unsupported(
            node_keys.get(dependent_index).map(String::as_str).unwrap_or(label),
            format!("{} depends on `{}`, which is also selected for removal. Keep the dependency or remove the dependent object first.", label, identity.display_key()),
        ));
    } else if !target_catalog.contains(identity) {
        let basename_matches = target_catalog
            .iter()
            .filter(|candidate| candidate.kind == identity.kind && candidate.name == identity.name)
            .map(SchemaObjectIdentity::display_key)
            .collect::<Vec<_>>();
        let reason = if basename_matches.is_empty() {
            format!(
                "{} `{}` is missing from the complete target snapshot and selected operations.",
                label,
                identity.display_key()
            )
        } else {
            format!(
                "{} `{}` is ambiguous; target snapshot contains only basename candidates: {}.",
                label,
                identity.display_key(),
                basename_matches.join(", ")
            )
        };
        requirements.push(unsupported(
            node_keys
                .get(dependent_index)
                .map(String::as_str)
                .unwrap_or(label),
            reason,
        ));
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn validate_object_dependencies(
    operations: &[MigrationOperation],
    transitions: &HashMap<String, super::unified_objects::ObjectDependencyTransition>,
    target_objects: &[SchemaObjectSnapshot],
    target_catalog: &BTreeSet<SchemaObjectIdentity>,
    identities_created: &HashMap<SchemaObjectIdentity, usize>,
    identities_dropped: &HashMap<SchemaObjectIdentity, usize>,
    extra_edges: &mut Vec<(usize, usize)>,
    requirements: &mut Vec<PlanRequirement>,
) {
    let node_keys = operations
        .iter()
        .map(operation_node_key)
        .collect::<Vec<_>>();
    for (index, operation) in operations.iter().enumerate() {
        let Some(identity) = operation_object_identity(operation) else {
            continue;
        };
        let Some(transition) = transitions.get(&node_keys[index]) else {
            continue;
        };
        let is_drop = operation_drops_identity(operation);
        let is_create = operation_creates_or_replaces_identity(operation) && !is_drop;
        let before = transition.before.as_deref();
        let after = transition.after.as_deref();
        if is_drop && before.is_none() {
            requirements.push(unsupported(
                &node_keys[index],
                format!("Dependencies of `{}` are opaque; the driver must provide exact dependency identities before this object can be dropped in a unified plan.", identity.display_key()),
            ));
            continue;
        }
        if is_create && after.is_none() {
            requirements.push(unsupported(
                &node_keys[index],
                format!("Dependencies of `{}` are opaque; the driver must provide exact dependency identities before this object can be created or replaced in a unified plan.", identity.display_key()),
            ));
            continue;
        }
        for dependency in after.unwrap_or_default() {
            match identities_created.get(dependency) {
                Some(prerequisite) if *prerequisite != index => {
                    extra_edges.push((*prerequisite, index));
                }
                Some(_) => requirements.push(unsupported(
                    &node_keys[index],
                    format!(
                        "`{}` depends on itself; correct the source object metadata.",
                        identity.display_key()
                    ),
                )),
                None if identities_dropped.contains_key(dependency) => {
                    requirements.push(unsupported(
                        &node_keys[index],
                        format!(
                            "`{}` depends on `{}`, which is also selected for removal.",
                            identity.display_key(),
                            dependency.display_key()
                        ),
                    ))
                }
                None if target_catalog.contains(dependency) => {}
                None => {
                    let basename_candidates = target_catalog
                        .iter()
                        .filter(|candidate| {
                            candidate.kind == dependency.kind && candidate.name == dependency.name
                        })
                        .map(SchemaObjectIdentity::display_key)
                        .collect::<Vec<_>>();
                    let reason = if basename_candidates.is_empty() {
                        format!(
                            "`{}` requires `{}`, which is not present in the target snapshot or selected plan.",
                            identity.display_key(),
                            dependency.display_key()
                        )
                    } else {
                        format!(
                            "`{}` requires exact identity `{}`, but the target snapshot has only ambiguous basename candidates: {}. Select the exact dependency or correct the source object reference.",
                            identity.display_key(),
                            dependency.display_key(),
                            basename_candidates.join(", ")
                        )
                    };
                    requirements.push(unsupported(&node_keys[index], reason));
                }
            }
        }
        for dependency in before.unwrap_or_default() {
            if let Some(prerequisite) = identities_dropped.get(dependency) {
                if *prerequisite != index {
                    // Remove/replace the dependent definition before removing
                    // or replacing an object it referenced in the old snapshot.
                    extra_edges.push((index, *prerequisite));
                }
            }
        }
    }

    let has_drops = !identities_dropped.is_empty();
    for target in target_objects {
        let identity = target.identity();
        // Any unselected opaque object can be a dependent of a selected drop.
        // Since we cannot inspect its body, there is no safe way to prove the
        // drop is independent. A selected drop's own old dependencies are also
        // required to order reverse operations safely.
        if has_drops && target.dependencies.is_none() {
            requirements.push(unsupported(
                identity.display_key(),
                format!("The target catalog contains `{}` with opaque dependencies, so the planner cannot prove selected drops are safe. Keep the drop out of this plan or use a driver with complete structured dependency metadata.", identity.display_key()),
            ));
            continue;
        }
        let Some(dependencies) = target.dependencies.as_ref() else {
            continue;
        };
        for dependency in dependencies {
            if let Some(prerequisite_drop) = identities_dropped.get(dependency) {
                if let Some(dependent_op) = identities_dropped
                    .get(&identity)
                    .or_else(|| identities_created.get(&identity))
                {
                    extra_edges.push((*dependent_op, *prerequisite_drop));
                } else {
                    requirements.push(unsupported(
                        dependency.display_key(),
                        format!("Unselected target object `{}` depends on selected drop `{}`. Select the dependent object for removal/replacement or keep the prerequisite.", identity.display_key(), dependency.display_key()),
                    ));
                }
            }
        }
    }
}

pub(super) fn operation_identity(
    operation: &MigrationOperation,
    dialect: &str,
    target_schema: Option<&str>,
    target_database: Option<&str>,
) -> Option<SchemaObjectIdentity> {
    match operation {
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
        | MigrationOperation::DropCheckConstraint { table, .. } => Some(table_identity(
            table,
            dialect,
            target_schema,
            target_database,
        )),
        _ => operation_object_identity(operation),
    }
}

fn operation_object_identity(operation: &MigrationOperation) -> Option<SchemaObjectIdentity> {
    use MigrationOperation::*;
    match operation {
        CreateView { view } | ReplaceView { desired: view, .. } | DropView { view } => {
            Some(SchemaObjectIdentity {
                kind: ObjectKind::View,
                schema: view.schema.clone(),
                name: view.name.clone(),
                signature: None,
                target_schema: None,
                target_name: None,
            })
        }
        CreateRoutine { routine }
        | ReplaceRoutine {
            desired: routine, ..
        }
        | DropRoutine { routine } => Some(SchemaObjectIdentity {
            kind: routine.kind,
            schema: routine.schema.clone(),
            name: routine.name.clone(),
            signature: routine.signature.clone(),
            target_schema: None,
            target_name: None,
        }),
        CreateTrigger { trigger }
        | ReplaceTrigger {
            desired: trigger, ..
        }
        | DropTrigger { trigger } => Some(SchemaObjectIdentity {
            kind: ObjectKind::Trigger,
            schema: trigger.schema.clone(),
            name: trigger.name.clone(),
            signature: None,
            target_schema: trigger.target_schema.clone(),
            target_name: Some(trigger.target_name.clone()),
        }),
        CreateSequence { sequence }
        | CreateSequenceUnowned { sequence, .. }
        | SetSequenceOwnership { sequence, .. }
        | ReplaceSequence {
            desired: sequence, ..
        }
        | DropSequence { sequence } => Some(SchemaObjectIdentity {
            kind: ObjectKind::Sequence,
            schema: sequence.schema.clone(),
            name: sequence.name.clone(),
            signature: None,
            target_schema: None,
            target_name: None,
        }),
        CreateType { type_definition }
        | ReplaceType {
            desired: type_definition,
            ..
        }
        | DropType { type_definition } => Some(SchemaObjectIdentity {
            kind: ObjectKind::Type,
            schema: type_definition.schema.clone(),
            name: type_definition.name.clone(),
            signature: None,
            target_schema: None,
            target_name: None,
        }),
        _ => None,
    }
}

pub(super) fn operation_creates_or_replaces_identity(operation: &MigrationOperation) -> bool {
    matches!(
        operation,
        MigrationOperation::CreateTable { .. }
            | MigrationOperation::CreateView { .. }
            | MigrationOperation::ReplaceView { .. }
            | MigrationOperation::CreateRoutine { .. }
            | MigrationOperation::ReplaceRoutine { .. }
            | MigrationOperation::CreateTrigger { .. }
            | MigrationOperation::ReplaceTrigger { .. }
            | MigrationOperation::CreateSequence { .. }
            | MigrationOperation::CreateSequenceUnowned { .. }
            | MigrationOperation::ReplaceSequence { .. }
            | MigrationOperation::CreateType { .. }
            | MigrationOperation::ReplaceType { .. }
    )
}

pub(super) fn operation_drops_identity(operation: &MigrationOperation) -> bool {
    matches!(
        operation,
        MigrationOperation::DropTable { .. }
            | MigrationOperation::DropView { .. }
            | MigrationOperation::DropRoutine { .. }
            | MigrationOperation::DropTrigger { .. }
            | MigrationOperation::DropSequence { .. }
            | MigrationOperation::DropType { .. }
    )
}

fn table_identity(
    raw: &str,
    dialect: &str,
    target_schema: Option<&str>,
    target_database: Option<&str>,
) -> SchemaObjectIdentity {
    let (scope, name) = raw
        .rsplit_once('.')
        .map(|(scope, name)| (Some(scope), name))
        .unwrap_or((None, raw));
    let scope = match normalize_dialect(dialect).as_str() {
        "postgresql" | "sqlserver" => scope.or(target_schema),
        "mysql" => scope.or(target_database),
        _ => scope,
    };
    SchemaObjectIdentity::table(scope, name)
}

pub(super) fn unsupported(
    operation: impl Into<String>,
    reason: impl Into<String>,
) -> PlanRequirement {
    PlanRequirement::Unsupported {
        operation: operation.into(),
        reason: reason.into(),
    }
}

pub(super) fn empty_plan(
    source_dialect: &str,
    target_dialect: &str,
    labels: BTreeSet<String>,
    warnings: Vec<String>,
    requirements: Vec<PlanRequirement>,
) -> SchemaDiffPlan {
    SchemaDiffPlan {
        plan_id: None,
        table: labels.first().cloned().unwrap_or_else(|| "unified".into()),
        tables: labels.into_iter().collect(),
        source_dialect: source_dialect.into(),
        target_dialect: target_dialect.into(),
        same_dialect: source_dialect == target_dialect,
        statements: Vec::new(),
        warnings,
        requirements,
        rollback_completeness: RollbackCompleteness {
            complete: true,
            missing: Vec::new(),
        },
        type_suggestions: Vec::new(),
        expected_target_schemas: Vec::new(),
    }
}
