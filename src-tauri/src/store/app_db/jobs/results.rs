use datazen_platform_api::context::RequestContext;
use datazen_platform_api::dto::job::{
    JobClaim, JobDomainResult, JobRecoveryVerdict, JobRecoveryVerification,
};
use datazen_platform_api::error::PortError;
use datazen_platform_api::id::{JobId, JobStateVersion};
use datazen_runtime::job::runtime_repository::{safe_result_marker, validate_job_domain_result};
use rusqlite::{params, OptionalExtension};

use super::access::{db_read_error, db_write_error};
use super::codec::encode;
use super::runtime_ops::check_claim;
use super::writes::validate_boundary;
use super::SqliteJobRepository;

const MAX_JOB_DOMAIN_RESULT_BYTES: usize = 512 * 1024;
const MAX_RECOVERY_RESULTS: usize = 64;
const MAX_CONFIRMED_BOUNDARIES: usize = 4096;

impl SqliteJobRepository {
    pub(super) async fn write_domain_result(
        &self,
        ctx: &RequestContext,
        claim: &JobClaim,
        result: JobDomainResult,
    ) -> Result<(), PortError> {
        validate_job_domain_result(&result)?;
        let serialized = encode(&result)?;
        if serialized.len() > MAX_JOB_DOMAIN_RESULT_BYTES {
            return Err(PortError::QuotaExceeded(
                "job result exceeds the local size limit".into(),
            ));
        }
        let now = self.clock.now();
        self.with_tx(|tx| {
            check_claim(tx, ctx, claim, &now)?;
            let active_stage = tx
                .query_row(
                    "SELECT stage FROM jobs WHERE job_id=?1",
                    params![claim.job_id.as_str()],
                    |row| row.get::<_, Option<String>>(0),
                )
                .map_err(db_read_error)?;
            if active_stage.as_deref() != Some(result.stage_id.as_str()) {
                return Err(PortError::StaleClaim);
            }
            tx.execute(
                "INSERT INTO job_domain_results(job_id,stage_id,result_json) VALUES (?1,?2,?3) \
                 ON CONFLICT(job_id,stage_id) DO UPDATE SET result_json=excluded.result_json",
                params![claim.job_id.as_str(), result.stage_id.as_str(), serialized],
            )
            .map_err(db_write_error)?;
            tx.execute(
                "UPDATE jobs SET updated_at=?1 WHERE job_id=?2",
                params![now.as_str(), claim.job_id.as_str()],
            )
            .map_err(db_write_error)?;
            Ok(())
        })
    }

    pub(super) async fn write_recovery_verification(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
        expected: JobStateVersion,
        verification: JobRecoveryVerification,
    ) -> Result<(), PortError> {
        validate_verification(&verification)?;
        let recovery_json = encode(&verification.result)?;
        let result_jsons = verification
            .domain_results
            .iter()
            .map(|result| {
                let json = encode(result)?;
                if json.len() > MAX_JOB_DOMAIN_RESULT_BYTES {
                    return Err(PortError::QuotaExceeded(
                        "job result exceeds the local size limit".into(),
                    ));
                }
                Ok((result, json))
            })
            .collect::<Result<Vec<_>, PortError>>()?;
        let now = self.clock.now();
        self.with_tx(|tx| {
            let current = tx
                .query_row(
                    "SELECT state,pending_verification_reason,state_version \
                     FROM jobs WHERE job_id=?1 AND organization_id=?2 AND owner_principal_id=?3",
                    params![job_id.as_str(), ctx.organization_id.as_str(), ctx.principal_id.as_str()],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, Option<String>>(1)?,
                            row.get::<_, i64>(2)?,
                        ))
                    },
                )
                .optional()
                .map_err(db_read_error)?
                .ok_or_else(|| PortError::NotFound(job_id.as_str().into()))?;
            let persisted_recovery = tx
                .query_row(
                    "SELECT recovery_json FROM job_result_details WHERE job_id=?1",
                    params![job_id.as_str()],
                    |row| row.get::<_, Option<String>>(0),
                )
                .optional()
                .map_err(db_read_error)?
                .flatten();
            let pending_verdict = persisted_recovery
                .as_deref()
                .and_then(|value| serde_json::from_str::<datazen_platform_api::dto::job::JobRecoveryResult>(value).ok())
                .is_some_and(|result| result.verdict == JobRecoveryVerdict::PendingVerification);
            if current.0 != "failed"
                || current.1.is_none()
                || current.2 != i64::try_from(expected.get()).unwrap_or(i64::MAX)
                || !pending_verdict
            {
                return Err(PortError::CasConflict {
                    entity: "job_recovery",
                    id: job_id.as_str().into(),
                });
            }

            let existing_boundaries: i64 = tx
                .query_row(
                    "SELECT COUNT(*) FROM job_commit_boundaries WHERE job_id=?1",
                    params![job_id.as_str()],
                    |row| row.get(0),
                )
                .map_err(db_read_error)?;
            let boundary_total = existing_boundaries
                .saturating_add(i64::try_from(verification.confirmed_boundaries.len()).unwrap_or(i64::MAX));
            if verification
                .result
                .resume_through
                .is_some_and(|index| i64::try_from(index).unwrap_or(i64::MAX) > boundary_total)
            {
                return Err(PortError::BackendUnavailable(
                    "recovery boundary index is invalid".into(),
                ));
            }

            let mut sequence: i64 = tx
                .query_row(
                    "SELECT COALESCE(MAX(sequence),0) FROM job_commit_boundaries WHERE job_id=?1",
                    params![job_id.as_str()],
                    |row| row.get(0),
                )
                .map_err(db_read_error)?;
            for boundary in &verification.confirmed_boundaries {
                sequence = sequence.checked_add(1).ok_or_else(|| {
                    PortError::BackendUnavailable("commit boundary sequence overflow".into())
                })?;
                tx.execute(
                    "INSERT INTO job_commit_boundaries(job_id,sequence,boundary_json) VALUES (?1,?2,?3)",
                    params![job_id.as_str(), sequence, encode(boundary)?],
                )
                .map_err(db_write_error)?;
            }
            for (result, serialized) in &result_jsons {
                tx.execute(
                    "INSERT INTO job_domain_results(job_id,stage_id,result_json) VALUES (?1,?2,?3) \
                     ON CONFLICT(job_id,stage_id) DO UPDATE SET result_json=excluded.result_json",
                    params![job_id.as_str(), result.stage_id.as_str(), serialized],
                )
                .map_err(db_write_error)?;
            }
            tx.execute(
                "INSERT INTO job_result_details(job_id,recovery_json) VALUES (?1,?2) \
                 ON CONFLICT(job_id) DO UPDATE SET recovery_json=excluded.recovery_json",
                params![job_id.as_str(), recovery_json],
            )
            .map_err(db_write_error)?;
            let is_resumable = verification.result.verdict == JobRecoveryVerdict::ResumeAfterVerify;
            let pending_reason = if is_resumable {
                None
            } else {
                Some(
                    verification
                        .result
                        .reason_code
                        .as_deref()
                        .unwrap_or("recoveryNeedsReview"),
                )
            };
            let changed = tx
                .execute(
                    "UPDATE jobs SET pending_verification_reason=?1,result_error_code=?2, \
                     state_version=state_version+1,updated_at=?3 \
                     WHERE job_id=?4 AND organization_id=?5 AND owner_principal_id=?6 AND state_version=?7",
                    params![
                        pending_reason,
                        pending_reason,
                        now.as_str(),
                        job_id.as_str(),
                        ctx.organization_id.as_str(),
                        ctx.principal_id.as_str(),
                        current.2,
                    ],
                )
                .map_err(db_write_error)?;
            if changed != 1 {
                return Err(PortError::CasConflict {
                    entity: "job_recovery",
                    id: job_id.as_str().into(),
                });
            }
            Ok(())
        })
    }
}

fn validate_verification(verification: &JobRecoveryVerification) -> Result<(), PortError> {
    if verification.domain_results.len() > MAX_RECOVERY_RESULTS
        || verification.confirmed_boundaries.len() > MAX_CONFIRMED_BOUNDARIES
        || verification
            .result
            .reason_code
            .as_deref()
            .is_some_and(|code| !safe_result_marker(code))
        || (!verification.confirmed_boundaries.is_empty()
            && verification.result.verdict != JobRecoveryVerdict::ResumeAfterVerify)
    {
        return Err(PortError::BackendUnavailable(
            "recovery verification is outside the safe format".into(),
        ));
    }
    for boundary in &verification.confirmed_boundaries {
        validate_boundary(boundary)?;
    }
    for result in &verification.domain_results {
        validate_job_domain_result(result)?;
    }
    Ok(())
}
