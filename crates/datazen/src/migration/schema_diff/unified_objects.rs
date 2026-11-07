//! Diff selected views, routines, triggers, sequences, and types into typed
//! operations for the unified migration planner.

use super::{
    object_identity::SchemaObjectIdentity,
    objects::SchemaObjectSnapshot,
    operations::MigrationOperation,
    types::{normalize_dialect, PlanRequirement},
};
use datazen_driver_api::{
    validate_object_definition_with_identity, validate_sequence_definition_with_identity,
    validate_type_definition_with_identity, validate_view_definition, ObjectKind,
};
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Debug, Clone, Default)]
pub(super) struct ObjectOperationBatch {
    pub operations: Vec<MigrationOperation>,
    pub transitions: HashMap<String, ObjectDependencyTransition>,
    pub labels: Vec<String>,
    pub warnings: Vec<String>,
    pub requirements: Vec<PlanRequirement>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct ObjectDependencyTransition {
    /// `None` means the pre-migration object's dependencies are opaque.
    pub before: Option<Vec<SchemaObjectIdentity>>,
    /// `None` means the desired object's dependencies are opaque.
    pub after: Option<Vec<SchemaObjectIdentity>>,
}

/// Diff all selected schema-object kinds into one typed operation collection.
/// This intentionally does not render SQL before the cross-kind graph passes.
pub(super) fn diff_schema_objects_to_operations(
    source: &[SchemaObjectSnapshot],
    target: &[SchemaObjectSnapshot],
    source_dialect: &str,
    target_dialect: &str,
    allow_destructive: bool,
) -> ObjectOperationBatch {
    let source_dialect = normalize_dialect(source_dialect);
    let target_dialect = normalize_dialect(target_dialect);
    let mut batch = ObjectOperationBatch::default();
    if source_dialect != target_dialect && (!source.is_empty() || !target.is_empty()) {
        batch.requirements.push(PlanRequirement::Unsupported {
            operation: "schema-objects".into(),
            reason: "Cross-dialect view, routine, trigger, sequence, and type DDL is opaque; select matching source and target dialects or use a driver-proven translation.".into(),
        });
        return batch;
    }

    let mut source_by_identity = BTreeMap::new();
    let mut target_by_identity = BTreeMap::new();
    for (objects, index, label) in [
        (source, &mut source_by_identity, "source"),
        (target, &mut target_by_identity, "target"),
    ] {
        for object in objects {
            if let Err(reason) = validate_object(object) {
                batch.requirements.push(PlanRequirement::Unsupported {
                    operation: object.identity().display_key(),
                    reason,
                });
                continue;
            }
            let identity = object.identity();
            if index.insert(identity.clone(), object).is_some() {
                batch.requirements.push(PlanRequirement::Unsupported {
                    operation: identity.display_key(),
                    reason: format!("{label} contains duplicate object identities"),
                });
            }
        }
    }

    let mut identities = BTreeSet::new();
    identities.extend(source_by_identity.keys().cloned());
    identities.extend(target_by_identity.keys().cloned());
    for identity in identities {
        let desired = source_by_identity.get(&identity).copied();
        let current = target_by_identity.get(&identity).copied();
        let Some(label_object) = desired.or(current) else {
            continue;
        };
        batch.labels.push(label_object.identity().display_key());
        let before = current
            .map(|object| object.dependencies.clone())
            .unwrap_or(Some(Vec::new()));
        let after = desired
            .map(|object| object.dependencies.clone())
            .unwrap_or(Some(Vec::new()));
        let operation = match (desired, current) {
            (Some(desired), None) => create_operation(desired),
            (Some(desired), Some(current))
                if desired.definition.trim() != current.definition.trim() =>
            {
                replace_operation(current, desired)
            }
            (None, Some(current)) => match drop_operation(current) {
                Ok(operation) if allow_destructive => Ok(operation),
                Ok(operation) => {
                    batch
                        .warnings
                        .push(format!("Skipped destructive operation {}", operation.key()));
                    continue;
                }
                Err(reason) => Err(reason),
            },
            _ => continue,
        };
        match operation {
            Ok(operation) => {
                let node_key = operation_node_key(&operation);
                batch
                    .transitions
                    .insert(node_key, ObjectDependencyTransition { before, after });
                batch.operations.push(operation);
            }
            Err(reason) => batch.requirements.push(PlanRequirement::Unsupported {
                operation: identity.display_key(),
                reason,
            }),
        }
    }
    batch.labels.sort();
    batch.labels.dedup();
    batch
}

fn validate_object(object: &SchemaObjectSnapshot) -> Result<(), String> {
    if object.name.trim().is_empty() {
        return Err("Schema object name must not be empty".into());
    }
    match object.kind {
        ObjectKind::View => validate_view_definition(&object.definition),
        ObjectKind::Function | ObjectKind::Procedure => validate_object_definition_with_identity(
            &object.definition,
            object.kind,
            &object.name,
            object.signature.as_deref(),
        ),
        ObjectKind::Trigger => {
            if object
                .target_name
                .as_deref()
                .unwrap_or("")
                .trim()
                .is_empty()
            {
                return Err("Trigger target relation is required".into());
            }
            validate_object_definition_with_identity(
                &object.definition,
                object.kind,
                &object.name,
                None,
            )
        }
        ObjectKind::Sequence => {
            let Some(schema) = object.schema.as_deref().filter(|schema| !schema.is_empty()) else {
                return Err("Sequence schema-qualified identity is required".into());
            };
            validate_sequence_definition_with_identity(
                &object.definition,
                Some(schema),
                &object.name,
            )
        }
        ObjectKind::Type => validate_type_definition_with_identity(
            &object.definition,
            object.schema.as_deref(),
            &object.name,
        ),
        ObjectKind::Table => Err("Tables must be supplied as table schema snapshots".into()),
    }
}

pub(super) fn operation_node_key(operation: &MigrationOperation) -> String {
    use MigrationOperation::*;
    let action = match operation {
        CreateTable { .. } => "create-table",
        DropTable { .. } => "drop-table",
        AddColumn { .. } => "add-column",
        DropColumn { .. } => "drop-column",
        AlterColumnType { .. } => "alter-column-type",
        SetNullable { .. } => "set-nullable",
        SetDefault { .. } => "set-default",
        SetComment { .. } => "set-comment",
        SetTableOptions { .. } => "set-table-options",
        SetAutoIncrement { .. } => "set-auto-increment",
        AddPrimaryKey { .. } => "add-primary-key",
        DropPrimaryKey { .. } => "drop-primary-key",
        CreateIndex { .. } => "create-index",
        DropIndex { .. } => "drop-index",
        AddForeignKey { .. } => "add-foreign-key",
        DropForeignKey { .. } => "drop-foreign-key",
        AddCheckConstraint { .. } => "add-check-constraint",
        DropCheckConstraint { .. } => "drop-check-constraint",
        CreateView { .. } => "create-view",
        ReplaceView { .. } => "replace-view",
        DropView { .. } => "drop-view",
        CreateRoutine { .. } => "create-routine",
        ReplaceRoutine { .. } => "replace-routine",
        DropRoutine { .. } => "drop-routine",
        CreateTrigger { .. } => "create-trigger",
        ReplaceTrigger { .. } => "replace-trigger",
        DropTrigger { .. } => "drop-trigger",
        CreateSequence { .. } => "create-sequence",
        CreateSequenceUnowned { .. } => "create-sequence-unowned",
        SetSequenceOwnership { .. } => "set-sequence-ownership",
        ReplaceSequence { .. } => "replace-sequence",
        DropSequence { .. } => "drop-sequence",
        CreateType { .. } => "create-type",
        ReplaceType { .. } => "replace-type",
        DropType { .. } => "drop-type",
    };
    format!("{}|{action}", operation.key())
}

fn create_operation(object: &SchemaObjectSnapshot) -> Result<MigrationOperation, String> {
    match object.kind {
        ObjectKind::View => Ok(MigrationOperation::CreateView {
            view: object.as_migration_view(),
        }),
        ObjectKind::Function | ObjectKind::Procedure => Ok(MigrationOperation::CreateRoutine {
            routine: object.as_migration_routine()?,
        }),
        ObjectKind::Trigger => Ok(MigrationOperation::CreateTrigger {
            trigger: object.as_migration_trigger()?,
        }),
        ObjectKind::Sequence => Ok(MigrationOperation::CreateSequence {
            sequence: object.as_migration_sequence()?,
        }),
        ObjectKind::Type => Ok(MigrationOperation::CreateType {
            type_definition: object.as_migration_type()?,
        }),
        ObjectKind::Table => Err("Tables must be supplied as table schema snapshots".into()),
    }
}

fn replace_operation(
    current: &SchemaObjectSnapshot,
    desired: &SchemaObjectSnapshot,
) -> Result<MigrationOperation, String> {
    Ok(match desired.kind {
        ObjectKind::View => MigrationOperation::ReplaceView {
            current: current.as_migration_view(),
            desired: desired.as_migration_view(),
        },
        ObjectKind::Function | ObjectKind::Procedure => MigrationOperation::ReplaceRoutine {
            current: current.as_migration_routine()?,
            desired: desired.as_migration_routine()?,
        },
        ObjectKind::Trigger => MigrationOperation::ReplaceTrigger {
            current: current.as_migration_trigger()?,
            desired: desired.as_migration_trigger()?,
        },
        ObjectKind::Sequence => MigrationOperation::ReplaceSequence {
            current: current.as_migration_sequence()?,
            desired: desired.as_migration_sequence()?,
        },
        ObjectKind::Type => MigrationOperation::ReplaceType {
            current: current.as_migration_type()?,
            desired: desired.as_migration_type()?,
        },
        ObjectKind::Table => return Err("Tables must be supplied as table schema snapshots".into()),
    })
}

fn drop_operation(object: &SchemaObjectSnapshot) -> Result<MigrationOperation, String> {
    match object.kind {
        ObjectKind::View => Ok(MigrationOperation::DropView {
            view: object.as_migration_view(),
        }),
        ObjectKind::Function | ObjectKind::Procedure => Ok(MigrationOperation::DropRoutine {
            routine: object.as_migration_routine()?,
        }),
        ObjectKind::Trigger => Ok(MigrationOperation::DropTrigger {
            trigger: object.as_migration_trigger()?,
        }),
        ObjectKind::Sequence => Ok(MigrationOperation::DropSequence {
            sequence: object.as_migration_sequence()?,
        }),
        ObjectKind::Type => Ok(MigrationOperation::DropType {
            type_definition: object.as_migration_type()?,
        }),
        ObjectKind::Table => Err("Tables must be supplied as table schema snapshots".into()),
    }
}
