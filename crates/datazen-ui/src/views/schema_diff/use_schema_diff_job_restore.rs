//! Port of `packages/frontend/src/windows/schema-diff/
//! useSchemaDiffJobRestore.ts` (T402).
//!
//! Historical jobs stay in the status panel. They never overwrite a
//! new wizard's endpoint scope, reviewed plan or execution result.
//!
//! The React hook is one effect: when the tracked job was submitted
//! **by this wizard** and its details carry a deploy result, adopt
//! that result into the wizard's execution slot. The port is the
//! pure decision — [`restore_deploy_result`] answers the result the
//! host adopts (or `None`, keeping history separate); the
//! `setDeployResult` state write is the caller's.
//!
//! Known deltas: none behavioural — the `submittedLocally &&
//! details?.deployResult` gate is the source's verbatim.

use crate::commands::schema_diff::{SchemaDiffDeployResultVm, SchemaDiffJobDetailsVm};
use crate::views::schema_diff::use_schema_diff_job_lifecycle::SchemaDiffTrackedJobVm;

/// The effect: the deploy result a locally-submitted job's details
/// carry, or `None` when the job is historical (not this wizard's)
/// or has no result yet.
pub(crate) fn restore_deploy_result(
    current_job: Option<&SchemaDiffTrackedJobVm>,
) -> Option<SchemaDiffDeployResultVm> {
    let job = current_job?;
    if !job.submitted_locally {
        return None;
    }
    job.details.as_ref()?.deploy_result.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::schema_diff::wire::SchemaDiffJobViewVm;
    use crate::commands::schema_diff::DeployStatusVm;
    use crate::views::data_transfer::data_transfer_attached_jobs::{JobProgress, JobState};

    /// The React suite's fixture: a succeeded apply job whose details
    /// carry one committed deploy result.
    fn job(submitted_locally: bool, with_result: bool) -> SchemaDiffTrackedJobVm {
        let deploy_result = with_result.then(|| SchemaDiffDeployResultVm {
            status: DeployStatusVm::Committed,
            executed_count: 1,
            statement_count: 1,
            errors: Vec::new(),
            statement_results: Vec::new(),
        });
        SchemaDiffTrackedJobVm {
            job_id: "historical-apply".to_owned(),
            kind: "schemaDiffApply".to_owned(),
            state: JobState::Succeeded,
            progress: JobProgress::default(),
            details: Some(SchemaDiffJobDetailsVm {
                details: SchemaDiffJobViewVm {
                    job_id: "historical-apply".to_owned(),
                    kind: "schemaDiffApply".to_owned(),
                    state: JobState::Succeeded,
                    stage: None,
                    cancel_requested: false,
                    error: None,
                    progress: JobProgress::default(),
                    updated_at: 1,
                    pending_verification_reason: None,
                },
                state_version: 1,
                prepared: None,
                plan_unavailable_after_restart: false,
                deploy_result,
                recovery: None,
                recovery_targets: Vec::new(),
            }),
            submitted_locally,
        }
    }

    #[test]
    fn keeps_history_separate_then_adopts_only_this_wizards_result() {
        // A historical job (submitted elsewhere): never adopted.
        assert!(restore_deploy_result(Some(&job(false, true))).is_none());
        // The same id, submitted locally: the result is adopted.
        let adopted = restore_deploy_result(Some(&job(true, true))).expect("adopted");
        assert_eq!(adopted.executed_count, 1);
        assert_eq!(adopted.statement_count, 1);
        // Back to a historical job: separate again.
        assert!(restore_deploy_result(Some(&job(false, true))).is_none());
    }

    #[test]
    fn no_job_or_no_result_adopts_nothing() {
        assert!(restore_deploy_result(None).is_none());
        // A locally-submitted job whose details lack a result (e.g. a
        // prepare job, or an apply still running).
        assert!(restore_deploy_result(Some(&job(true, false))).is_none());
    }
}
