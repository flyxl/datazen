//! Safe PostgreSQL sequence ownership phases for the unified operation graph.

use super::{
    object_identity::{
        SchemaObjectDependencySnapshot, SchemaObjectIdentity, SequenceDependencyUsage,
        SequenceDependencyUsageKind, SequenceOwnershipIdentity,
    },
    objects::SchemaObjectSnapshot,
    operations::MigrationOperation,
    types::{normalize_dialect, PlanRequirement},
    unified_objects::{operation_node_key, ObjectDependencyTransition},
    unified_validation::operation_identity,
};
use datazen_driver_api::{
    split_sequence_definition, MigrationRenderer, MigrationRisk, MigrationStatement, ObjectKind,
};
use std::collections::HashMap;

/// Split an owned sequence create only when catalog evidence proves its exact
/// owner column and selected table operation. The returned node-key pairs are
/// explicit prerequisite edges for the unified DAG.
pub(super) fn split_owned_sequence_operations(
    operations: &mut Vec<MigrationOperation>,
    transitions: &mut HashMap<String, ObjectDependencyTransition>,
    source_objects: &[SchemaObjectSnapshot],
    target_objects: &[SchemaObjectSnapshot],
    source_table_dependencies: &[SchemaObjectDependencySnapshot],
    dialect: &str,
    target_schema: Option<&str>,
    target_database: Option<&str>,
) -> (Vec<(String, String)>, Vec<PlanRequirement>) {
    let mut phase_edges = Vec::new();
    let mut requirements = Vec::new();
    let original_operations = std::mem::take(operations);
    let mut rewritten = Vec::with_capacity(original_operations.len());
    for operation in original_operations.iter().cloned() {
        if let Some(requirement) =
            unsafe_sequence_mutation(&operation, target_objects, source_table_dependencies)
        {
            requirements.push(requirement);
            rewritten.push(operation);
            continue;
        }
        let MigrationOperation::CreateSequence { sequence } = &operation else {
            rewritten.push(operation);
            continue;
        };
        let sequence_identity = SchemaObjectIdentity {
            kind: ObjectKind::Sequence,
            schema: sequence.schema.clone(),
            name: sequence.name.clone(),
            signature: None,
            target_schema: None,
            target_name: None,
        };
        let base_key = operation_node_key(&operation);
        let split = match split_sequence_definition(
            &sequence.definition,
            sequence.schema.as_deref(),
            &sequence.name,
        ) {
            Ok(split) => split,
            Err(reason) => {
                requirements.push(blocker(
                    &base_key,
                    format!("The sequence definition cannot be split safely: {reason}. Compare again after the driver returns validated catalog DDL."),
                ));
                rewritten.push(operation);
                continue;
            }
        };
        let Some((owner_schema, owner_name, owner_column)) = split.2 else {
            if source_objects
                .iter()
                .find(|object| object.identity() == sequence_identity)
                .and_then(|object| object.sequence_dependency_usages.as_ref())
                .is_some_and(|usages| {
                    usages
                        .iter()
                        .any(|usage| usage.usage == SequenceDependencyUsageKind::OwnedBy)
                })
            {
                requirements.push(blocker(
                    &base_key,
                    "The sequence definition has no OWNED BY clause but the dependency catalog reports ownership; refresh the driver catalog before deploying.",
                ));
            }
            rewritten.push(operation);
            continue;
        };
        if normalize_dialect(dialect) != "postgresql" {
            requirements.push(blocker(
                &base_key,
                "Sequence ownership phases are supported only by the PostgreSQL renderer.",
            ));
            rewritten.push(operation);
            continue;
        }
        let ownership = SequenceOwnershipIdentity {
            schema: owner_schema,
            table: owner_name,
            column: owner_column,
        };
        let owner_identity = ownership.owner_table();
        let expected_usage = SequenceDependencyUsage {
            sequence: sequence_identity.clone(),
            owner_table: owner_identity.clone(),
            column_name: ownership.column.clone(),
            usage: SequenceDependencyUsageKind::OwnedBy,
        };
        let sequence_snapshot = source_objects
            .iter()
            .find(|object| object.identity() == sequence_identity);
        let sequence_is_proven = sequence_snapshot.is_some_and(|snapshot| {
            snapshot
                .dependencies
                .as_ref()
                .is_some_and(|dependencies| dependencies.contains(&owner_identity))
                && snapshot
                    .sequence_dependency_usages
                    .as_ref()
                    .is_some_and(|usages| usages.as_slice() == [expected_usage.clone()])
        });
        if !sequence_is_proven {
            requirements.push(blocker(
                &base_key,
                format!("The driver must prove the exact OWNED BY relation `{}` → `{}.{}.{}` before sequence ownership can be deferred.", sequence_identity.display_key(), ownership.schema, ownership.table, ownership.column),
            ));
            rewritten.push(operation);
            continue;
        }
        let table_snapshot = source_table_dependencies
            .iter()
            .find(|snapshot| snapshot.identity == owner_identity);
        let default_usage = SequenceDependencyUsage {
            sequence: sequence_identity.clone(),
            owner_table: owner_identity.clone(),
            column_name: ownership.column.clone(),
            usage: SequenceDependencyUsageKind::ColumnDefault,
        };
        let table_is_proven = table_snapshot.is_some_and(|snapshot| {
            snapshot
                .dependencies
                .as_ref()
                .is_some_and(|dependencies| dependencies.contains(&sequence_identity))
                && snapshot
                    .sequence_dependency_usages
                    .as_ref()
                    .is_some_and(|usages| usages.contains(&default_usage))
        });
        if !table_is_proven {
            requirements.push(blocker(
                &base_key,
                format!("The selected source table must provide complete catalog evidence that `{}.{}` uses `{}` as its default sequence.", ownership.table, ownership.column, sequence_identity.display_key()),
            ));
            rewritten.push(operation);
            continue;
        }

        let owner_operations = original_operations
            .iter()
            .filter(|candidate| {
                owner_column_operation(
                    candidate,
                    &owner_identity,
                    &ownership.column,
                    dialect,
                    target_schema,
                    target_database,
                )
            })
            .map(operation_node_key)
            .collect::<Vec<_>>();
        if owner_operations.len() != 1 {
            let reason = if owner_operations.is_empty() {
                format!("Select the owner table/column operation for `{}.{}`. An unselected target table is not sufficient proof that its column and default are already linked to a newly created sequence.", ownership.table, ownership.column)
            } else {
                format!("The owner column `{}.{}` maps to multiple selected table operations; resolve the ambiguous selection before deploying.", ownership.table, ownership.column)
            };
            requirements.push(blocker(&base_key, reason));
            rewritten.push(operation);
            continue;
        }

        let Some(mut transition) = transitions.remove(&base_key) else {
            requirements.push(blocker(
                &base_key,
                "The sequence operation has no dependency transition snapshot; compare again before deploying.",
            ));
            rewritten.push(operation);
            continue;
        };
        let Some(after) = transition.after.as_mut() else {
            requirements.push(blocker(
                &base_key,
                "The sequence dependency snapshot is incomplete; grant catalog visibility and compare again.",
            ));
            transitions.insert(base_key, transition);
            rewritten.push(operation);
            continue;
        };
        if !after.contains(&owner_identity) {
            requirements.push(blocker(
                &base_key,
                "The validated sequence definition and complete dependency set disagree about its owner table.",
            ));
            transitions.insert(base_key, transition);
            rewritten.push(operation);
            continue;
        }
        after.retain(|dependency| dependency != &owner_identity);

        let create_unowned = MigrationOperation::CreateSequenceUnowned {
            sequence: sequence.clone(),
            ownership: ownership.clone(),
        };
        let set_ownership = MigrationOperation::SetSequenceOwnership {
            sequence: sequence.clone(),
            ownership,
        };
        let create_key = operation_node_key(&create_unowned);
        let owner_key = operation_node_key(&set_ownership);
        transitions.insert(create_key.clone(), transition);
        phase_edges.push((create_key, owner_key.clone()));
        phase_edges.push((owner_operations[0].clone(), owner_key));
        rewritten.push(create_unowned);
        rewritten.push(set_ownership);
    }
    *operations = rewritten;
    (phase_edges, requirements)
}

fn unsafe_sequence_mutation(
    operation: &MigrationOperation,
    target_objects: &[SchemaObjectSnapshot],
    table_dependencies: &[SchemaObjectDependencySnapshot],
) -> Option<PlanRequirement> {
    let sequence = match operation {
        MigrationOperation::DropSequence { sequence } => sequence,
        MigrationOperation::ReplaceSequence { current, .. } => current,
        _ => return None,
    };
    let key = operation_node_key(operation);
    let identity = SchemaObjectIdentity {
        kind: ObjectKind::Sequence,
        schema: sequence.schema.clone(),
        name: sequence.name.clone(),
        signature: None,
        target_schema: None,
        target_name: None,
    };
    let target_snapshot = target_objects
        .iter()
        .find(|snapshot| snapshot.identity() == identity);
    let has_owned_by_ddl = match split_sequence_definition(
        &sequence.definition,
        sequence.schema.as_deref(),
        &sequence.name,
    ) {
        Ok((_, _, owner, _)) => owner.is_some(),
        Err(reason) => {
            return Some(blocker(
                &key,
                format!("The selected sequence mutation has invalid catalog DDL and cannot be reviewed safely: {reason}. Compare again."),
            ));
        }
    };
    let has_owned_by_catalog = target_snapshot
        .and_then(|snapshot| snapshot.sequence_dependency_usages.as_ref())
        .is_some_and(|usages| {
            usages
                .iter()
                .any(|usage| usage.usage == SequenceDependencyUsageKind::OwnedBy)
        });
    let has_column_default_usage = table_dependencies.iter().any(|snapshot| {
        snapshot
            .sequence_dependency_usages
            .as_ref()
            .is_some_and(|usages| {
                usages.iter().any(|usage| {
                    usage.sequence == identity
                        && usage.usage == SequenceDependencyUsageKind::ColumnDefault
                })
            })
    });
    let mutation = match operation {
        MigrationOperation::DropSequence { .. } => "drop",
        _ => "replace",
    };
    let reason = if has_owned_by_ddl || has_owned_by_catalog {
        format!("This sequence has an OWNED BY table relationship. Unified {mutation} is blocked because PostgreSQL table drops can implicitly remove owned sequences and the existing {mutation} renderer cannot safely stage detachment/reattachment. Keep the sequence unchanged or select a migration path with explicit ownership phases.")
    } else if has_column_default_usage {
        format!("A selected table catalog proves that a column default uses this sequence. Unified {mutation} is blocked because the existing {mutation} renderer cannot safely stage the default dependency.")
    } else {
        format!("Unified sequence {mutation} is blocked because the reviewed target snapshot does not include a complete reverse column-default catalog proving that no table depends on this sequence. Keep the sequence unchanged or use a driver with complete reverse dependency evidence.")
    };
    Some(blocker(&key, reason))
}

fn owner_column_operation(
    operation: &MigrationOperation,
    owner: &SchemaObjectIdentity,
    column: &str,
    dialect: &str,
    target_schema: Option<&str>,
    target_database: Option<&str>,
) -> bool {
    if operation_identity(operation, dialect, target_schema, target_database).as_ref()
        != Some(owner)
    {
        return false;
    }
    match operation {
        MigrationOperation::CreateTable { columns, .. } => columns
            .iter()
            .any(|candidate| candidate.name == column && candidate.default_value.is_some()),
        MigrationOperation::AddColumn {
            column: candidate, ..
        } => candidate.name == column && candidate.default_value.is_some(),
        MigrationOperation::SetDefault {
            column: candidate,
            to: Some(_),
            ..
        } => candidate == column,
        _ => false,
    }
}

fn blocker(operation: &str, reason: impl Into<String>) -> PlanRequirement {
    PlanRequirement::Unsupported {
        operation: operation.to_owned(),
        reason: reason.into(),
    }
}

/// Render one synthetic phase through the driver's existing CreateSequence
/// renderer. The Host only selects exact statements returned by the validated
/// split helper; it never constructs dialect SQL itself.
pub(super) fn render_sequence_phase(
    renderer: &dyn MigrationRenderer,
    operation: &MigrationOperation,
) -> Result<Option<MigrationStatement>, String> {
    let (sequence, ownership, is_ownership) = match operation {
        MigrationOperation::CreateSequenceUnowned {
            sequence,
            ownership,
        } => (sequence, ownership, false),
        MigrationOperation::SetSequenceOwnership {
            sequence,
            ownership,
        } => (sequence, ownership, true),
        _ => return Ok(None),
    };
    let driver_operation = datazen_driver_api::MigrationOperation::CreateSequence {
        sequence: sequence.clone(),
    };
    let rendered = renderer.render(&driver_operation)?;
    let (create, owner_sql, exact_owner, reset) =
        split_sequence_definition(&rendered.sql, sequence.schema.as_deref(), &sequence.name)?;
    let expected_owner = (
        ownership.schema.clone(),
        ownership.table.clone(),
        ownership.column.clone(),
    );
    if exact_owner.as_ref() != Some(&expected_owner) {
        return Err("The target renderer's validated OWNED BY identity does not match the reviewed source catalog".into());
    }
    let (sql, rollback_sql, summary) = if is_ownership {
        (
            owner_sql.ok_or("validated sequence renderer output is missing OWNED BY")?,
            reset,
            format!(
                "Attach sequence {} to {}.{}.{}",
                sequence.schema.as_deref().unwrap_or_default(),
                ownership.schema,
                ownership.table,
                ownership.column
            ),
        )
    } else {
        (
            create,
            rendered.rollback_sql,
            format!(
                "Create sequence {}.{} before owner table",
                sequence.schema.as_deref().unwrap_or_default(),
                sequence.name
            ),
        )
    };
    Ok(Some(MigrationStatement {
        sql,
        rollback_sql,
        risk: MigrationRisk::Additive,
        summary,
    }))
}
