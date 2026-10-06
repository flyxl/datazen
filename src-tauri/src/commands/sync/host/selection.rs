//! Artifact and reviewed-selection storage for the apply Job.
//!
//! The prepare Job leaves exactly two things behind: a frozen
//! `ChangeSetArtifact` and — only after the user confirmed — the reviewed
//! `SyncRunSelection`. The apply Job may narrow the artifact to that selection
//! and nothing else, and it may do so exactly once.

use std::collections::HashMap;

use crate::data_sync::job::artifact::{ChangeBlock, ChangeSetArtifact, RelationIdentity};
use crate::data_sync::job::host::ArtifactStoreGuard;
use crate::data_sync::{
    ChangeOperation, ComparisonResult, DataSyncError, SyncOptions, TableMappingStatus,
};

use super::super::artifact_view::comparison_from_artifact;
use super::super::plans;
use super::{invalid, state};

/// Freeze the artifact under the id the handler reports in its receipt.
pub(super) fn store_artifact(
    artifact: ChangeSetArtifact,
) -> Result<ArtifactStoreGuard, DataSyncError> {
    let artifact_id = state::artifact_id(&artifact.plan_id);
    state::store_artifact(artifact);
    Ok(ArtifactStoreGuard { artifact_id })
}

/// The confirmed block subset for one revision of the reviewed plan.
///
/// Three invariants live here because this is the only place that sees both the
/// frozen artifact and the client-confirmed selection: the revision must be the
/// one the user reviewed, the selection is consumed on read (§2.1 / CM-41 — a
/// second claim of the same planId finds nothing to execute), and every selected
/// row must still be executable under the reviewed options.
pub(super) fn selection(
    plan_id: &str,
    selection_revision: u64,
) -> Result<Vec<ChangeBlock>, DataSyncError> {
    let artifact = state::load_artifact(plan_id).ok_or_else(|| {
        invalid(format!(
            "plan {plan_id} is not available; compare again before applying"
        ))
    })?;
    if artifact.plan_id != plan_id {
        return Err(invalid(format!(
            "plan {plan_id} does not match the stored ChangeSet"
        )));
    }
    let stored = state::take_selection(plan_id).ok_or_else(|| {
        invalid(format!(
            "plan {plan_id} was already submitted for apply; compare again"
        ))
    })?;
    if stored.selection.revision != selection_revision {
        return Err(invalid(format!(
            "selection revision {selection_revision} does not match the reviewed revision {}; review the selection again",
            stored.selection.revision
        )));
    }
    let comparison = comparison_from_artifact(&artifact);
    let selected = plans::apply_selection(&comparison, &stored.selection, &stored.options)
        .map_err(|error| invalid(format!("reviewed selection cannot be applied: {error}")))?;
    blocks_for(&artifact, &selected, &stored.options)
}

/// Project the confirmed rows back into the artifact's block shape.
///
/// Row values come from the artifact only: the client contributes keys and
/// scope metadata, never payloads, so `before`/`after` stay exactly the values
/// the user reviewed.
fn blocks_for(
    artifact: &ChangeSetArtifact,
    selected: &ComparisonResult,
    options: &SyncOptions,
) -> Result<Vec<ChangeBlock>, DataSyncError> {
    let table_meta = artifact
        .table_meta
        .iter()
        .map(|meta| (meta.relation.clone(), meta))
        .collect::<HashMap<RelationIdentity, _>>();
    let mut blocks = Vec::new();
    for table in &selected.tables {
        if table.status != TableMappingStatus::Matched {
            continue;
        }
        let relation = RelationIdentity {
            database: artifact.target.database.clone(),
            schema: artifact.target.schema.clone(),
            table: table.target_table.clone(),
        };
        let meta = table_meta.get(&relation).ok_or_else(|| {
            invalid(format!(
                "table {} is not part of the reviewed ChangeSet",
                table.target_table
            ))
        })?;
        for change in table.rows.iter().filter(|change| change.selected) {
            if change.operation == ChangeOperation::Unchanged || !options.allows(change.operation) {
                return Err(invalid(format!(
                    "table {} selected a {} row that the reviewed options do not execute; review the selection again",
                    table.target_table,
                    match change.operation {
                        ChangeOperation::Insert => "insert",
                        ChangeOperation::Update => "update",
                        ChangeOperation::Delete => "delete",
                        ChangeOperation::Unchanged => "unchanged",
                    }
                )));
            }
            blocks.push(ChangeBlock {
                relation: relation.clone(),
                operation: change.operation,
                key: change.key.clone(),
                before: change.target_row.clone(),
                after: change.source_row.clone(),
                column_names: meta
                    .columns
                    .iter()
                    .map(|column| column.name.clone())
                    .collect(),
                pk_columns: meta.pk_columns.clone(),
                changed_columns: change.changed_columns.clone(),
                source_table: table.source_table.clone(),
            });
        }
    }
    Ok(blocks)
}
