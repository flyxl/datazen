//! Artifact and reviewed-selection storage for the apply Job.
//!
//! The prepare Job leaves exactly two things behind: a frozen
//! `ChangeSetArtifact` and — only after the user confirmed — the reviewed
//! `SyncRunSelection`. The apply Job may narrow the artifact to that selection
//! and nothing else, and it may do so exactly once.

use crate::data_sync::job::artifact::{ChangeBlock, ChangeSetArtifact};
use crate::data_sync::job::host::ArtifactStoreGuard;
use crate::data_sync::{DataSyncError, SyncOptions};

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
    plans::select_artifact_blocks(&artifact, &stored.selection, &stored.options)
        .map_err(|error| invalid(format!("reviewed selection cannot be applied: {error}")))
}
