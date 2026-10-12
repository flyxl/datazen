//! Shared DDL execution keeps table/index/FK outcomes and evidence aligned.
use super::*;
use crate::job::checkpoint::commit_boundary;
use crate::model::DdlPreviewKind;
use crate::structure::DatabaseStructurePhase;
use sha2::{Digest, Sha256};

impl DataTransferHandler {
    pub(super) async fn run_database_structure(
        &self,
        spec: &StageSpec,
        cancel: &CancelToken,
        phase: DatabaseStructurePhase,
    ) -> Result<StageOutcome, JobError> {
        let mut boundaries = Vec::new();
        let mut effect = EffectOutcome::Completed;
        let mut terminal = StageTerminal::Succeeded;
        if let TransferEndpoints::Database {
            target_driver,
            target_handle,
            ..
        } = &self.endpoints
        {
            let selected: HashSet<String> = self
                .freeze
                .job
                .tables
                .iter()
                .filter(|table| table.enabled)
                .map(|table| table.source_table.clone())
                .collect();
            let plan = self.database_structure.as_deref().unwrap_or(&[]);
            let planned = plan
                .iter()
                .filter(|item| {
                    selected.contains(&item.source_table)
                        && match phase {
                            DatabaseStructurePhase::Prepare => {
                                item.kind != DdlPreviewKind::ForeignKey
                            }
                            DatabaseStructurePhase::ForeignKeys => {
                                item.kind == DdlPreviewKind::ForeignKey
                            }
                            DatabaseStructurePhase::All => true,
                        }
                })
                .collect::<Vec<_>>();
            let results = crate::structure::execute_database_structure_plan(
                target_driver.as_ref(),
                target_handle,
                plan,
                &selected,
                phase,
                Some(cancel.flag()),
                None,
            )
            .await;
            // Results preserve the exact filtered plan order, including entries
            // not attempted after a failure. Matching only by table would mark a
            // failed index as committed when CREATE TABLE had succeeded earlier.
            for (item, result) in planned.iter().zip(&results) {
                if result.outcome == Some(TableExecutionOutcome::Committed) {
                    let digest = format!("{:x}", Sha256::digest(item.ddl.as_bytes()));
                    boundaries.push(commit_boundary(
                        spec.stage_id.as_str(),
                        &format!("{}#{:?}#{digest}", item.source_table, item.kind),
                        &digest,
                        0,
                        "target-ddl-ack",
                        "sha256:structure",
                        None,
                    ));
                }
            }
            let succeeded = results.len() == planned.len()
                && results
                    .iter()
                    .all(|result| result.outcome == Some(TableExecutionOutcome::Committed));
            if !succeeded {
                terminal = if cancel.is_cancelled() {
                    StageTerminal::Cancelled
                } else {
                    StageTerminal::Failed
                };
                effect = if results
                    .iter()
                    .any(|result| result.outcome == Some(TableExecutionOutcome::Unknown))
                {
                    EffectOutcome::Unknown
                } else if !boundaries.is_empty() {
                    EffectOutcome::PartiallyApplied
                } else {
                    EffectOutcome::NotStarted
                };
            }
        }
        Ok(StageOutcome {
            stage_id: spec.stage_id.clone(),
            terminal,
            progress: JobProgress::default(),
            commit_boundaries: boundaries,
            execution_ids: Vec::new(),
            artifact_ids: Vec::new(),
            effect_outcome: effect,
            error_code: None,
        })
    }
}
