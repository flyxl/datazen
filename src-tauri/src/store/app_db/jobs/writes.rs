use datazen_platform_api::context::RequestContext;
use datazen_platform_api::dto::job::JobClaim;
use datazen_platform_api::dto::job::{
    Checkpoint, CommitBoundary, JobRecord, JobState, StageRecord,
};
use datazen_platform_api::error::PortError;
use datazen_platform_api::id::{ExecutionId, JobStateVersion, Timestamp};
use rusqlite::params;

use super::access::{db_read_error, db_write_error};
use super::codec::{decode, encode, read_record, safe_identifier};
use super::runtime_ops::check_claim;
use super::SqliteJobRepository;

impl SqliteJobRepository {
    pub(super) async fn write_stage(
        &self,
        ctx: &RequestContext,
        claim: &JobClaim,
        stage: StageRecord,
    ) -> Result<(), PortError> {
        if stage.job_id != claim.job_id
            || !safe_identifier(stage.stage_id.as_str())
            || !safe_identifier(&stage.kind)
        {
            return Err(PortError::BackendUnavailable("job stage is invalid".into()));
        }
        let execution_ids_json = encode(&stage.execution_ids)?;
        let now = self.clock.now();
        self.with_tx(|tx| {
            check_claim(tx, ctx, claim, &now)?;
            tx.execute(
                "INSERT INTO job_stages(job_id,stage_id,kind,claimed_by,execution_ids_json,started_at,finished_at) \
                 VALUES (?1,?2,?3,?4,?5,?6,?7) \
                 ON CONFLICT(job_id,stage_id) DO UPDATE SET kind=excluded.kind,claimed_by=excluded.claimed_by, \
                 execution_ids_json=excluded.execution_ids_json,started_at=COALESCE(job_stages.started_at,excluded.started_at), \
                 finished_at=excluded.finished_at",
                params![
                    claim.job_id.as_str(),
                    stage.stage_id.as_str(),
                    stage.kind,
                    stage.claimed_by.as_ref().map(|worker| worker.as_str()),
                    execution_ids_json,
                    stage.started_at.as_ref().map(Timestamp::as_str),
                    stage.finished_at.as_ref().map(Timestamp::as_str),
                ],
            )
            .map_err(db_write_error)?;
            let current: String = tx
                .query_row(
                    "SELECT execution_ids_json FROM jobs WHERE job_id=?1",
                    params![claim.job_id.as_str()],
                    |row| row.get(0),
                )
                .map_err(db_read_error)?;
            let mut execution_ids: Vec<ExecutionId> = decode(&current)?;
            for id in stage.execution_ids {
                if !execution_ids.contains(&id) {
                    execution_ids.push(id);
                }
            }
            tx.execute(
                "UPDATE jobs SET stage=?1,execution_ids_json=?2,updated_at=?3 WHERE job_id=?4",
                params![
                    stage.stage_id.as_str(),
                    encode(&execution_ids)?,
                    now.as_str(),
                    claim.job_id.as_str(),
                ],
            )
            .map_err(db_write_error)?;
            Ok(())
        })
    }

    pub(super) async fn write_boundary(
        &self,
        ctx: &RequestContext,
        claim: &JobClaim,
        boundary: CommitBoundary,
    ) -> Result<(), PortError> {
        validate_boundary(&boundary)?;
        let serialized = encode(&boundary)?;
        let now = self.clock.now();
        self.with_tx(|tx| {
            check_claim(tx, ctx, claim, &now)?;
            let sequence: i64 = tx
                .query_row(
                    "SELECT COALESCE(MAX(sequence),0)+1 FROM job_commit_boundaries WHERE job_id=?1",
                    params![claim.job_id.as_str()],
                    |row| row.get(0),
                )
                .map_err(db_read_error)?;
            tx.execute(
                "INSERT INTO job_commit_boundaries(job_id,sequence,boundary_json) VALUES (?1,?2,?3)",
                params![claim.job_id.as_str(), sequence, serialized],
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

    pub(super) async fn cas_state(
        &self,
        ctx: &RequestContext,
        claim: &JobClaim,
        expected: JobStateVersion,
        next: JobState,
    ) -> Result<JobRecord, PortError> {
        let now = self.clock.now();
        let next_text = serde_json::to_value(next)
            .map_err(|_| PortError::BackendUnavailable("job state encoding failed".into()))?
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| PortError::BackendUnavailable("job state is invalid".into()))?;
        self.with_tx(|tx| {
            check_claim(tx, ctx, claim, &now)?;
            let next_version = expected
                .get()
                .checked_add(1)
                .and_then(|value| i64::try_from(value).ok())
                .ok_or_else(|| PortError::BackendUnavailable("job state version overflow".into()))?;
            let changed = tx
                .execute(
                    "UPDATE jobs SET state=?1,state_version=?2,updated_at=?3, \
                     worker_id=CASE WHEN ?4 THEN NULL ELSE worker_id END, \
                     claim_stage_id=CASE WHEN ?4 THEN NULL ELSE claim_stage_id END, \
                     claim_expires_at=CASE WHEN ?4 THEN NULL ELSE claim_expires_at END \
                     WHERE job_id=?5 AND organization_id=?6 AND owner_principal_id=?7 AND state_version=?8",
                    params![
                        next_text,
                        next_version,
                        now.as_str(),
                        next.is_terminal(),
                        claim.job_id.as_str(),
                        ctx.organization_id.as_str(),
                        ctx.principal_id.as_str(),
                        i64::try_from(expected.get()).unwrap_or(i64::MAX),
                    ],
                )
                .map_err(db_write_error)?;
            if changed != 1 {
                return Err(PortError::CasConflict {
                    entity: "job",
                    id: claim.job_id.as_str().into(),
                });
            }
            read_record(tx, ctx, &claim.job_id, &now)
        })
    }

    pub(super) async fn write_checkpoint(
        &self,
        ctx: &RequestContext,
        claim: &JobClaim,
        checkpoint: Checkpoint,
    ) -> Result<(), PortError> {
        if checkpoint.job_id != claim.job_id {
            return Err(PortError::BackendUnavailable(
                "job checkpoint identity mismatch".into(),
            ));
        }
        validate_checkpoint(&checkpoint)?;
        let json = encode(&checkpoint)?;
        let version = i64::try_from(checkpoint.state_version.get())
            .map_err(|_| PortError::BackendUnavailable("checkpoint version overflow".into()))?;
        let now = self.clock.now();
        self.with_tx(|tx| {
            check_claim(tx, ctx, claim, &now)?;
            tx.execute(
                "INSERT INTO job_checkpoints(job_id,state_version,checkpoint_json) VALUES (?1,?2,?3)",
                params![claim.job_id.as_str(), version, json],
            )
            .map_err(|error| match error {
                rusqlite::Error::SqliteFailure(info, _)
                    if info.code == rusqlite::ErrorCode::ConstraintViolation =>
                {
                    PortError::CasConflict {
                        entity: "job_checkpoint",
                        id: claim.job_id.as_str().into(),
                    }
                }
                _ => db_write_error(error),
            })?;
            Ok(())
        })
    }
}

fn validate_boundary(boundary: &CommitBoundary) -> Result<(), PortError> {
    if !safe_marker(boundary.stage_id.as_str())
        || !safe_marker(&boundary.stable_target_fingerprint)
        || boundary
            .operation_id
            .as_deref()
            .is_some_and(|id| !safe_marker(id))
        || boundary
            .batch_id
            .as_deref()
            .is_some_and(|id| !safe_marker(id))
        || boundary
            .payload_digest
            .as_deref()
            .is_some_and(|id| !safe_marker(id))
        || boundary.evidence.len() > 128
        || boundary
            .evidence
            .iter()
            .any(|marker| !safe_evidence(marker))
    {
        return Err(PortError::BackendUnavailable(
            "commit boundary contains unsupported durable detail".into(),
        ));
    }
    Ok(())
}

fn validate_checkpoint(checkpoint: &Checkpoint) -> Result<(), PortError> {
    if !safe_marker(&checkpoint.stable_target_fingerprint)
        || !safe_identifier(&checkpoint.recovery_policy)
        || checkpoint.committed.len() > 100_000
        || checkpoint.verification_evidence.len() > 256
        || checkpoint
            .verification_evidence
            .iter()
            .any(|marker| !safe_evidence(marker))
    {
        return Err(PortError::BackendUnavailable(
            "checkpoint contains unsupported durable detail".into(),
        ));
    }
    for boundary in &checkpoint.committed {
        validate_boundary(boundary)?;
    }
    Ok(())
}

fn safe_marker(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 2048
        && !value.chars().any(char::is_control)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:-/#=".contains(&byte))
}

fn safe_evidence(value: &str) -> bool {
    if !safe_marker(value) {
        return false;
    }
    let lower = value.to_ascii_lowercase();
    let contains_sensitive_label = [
        "password",
        "passwd",
        "credential",
        "secret",
        "token",
        "dbsession",
        "sessionhandle",
        "select",
        "delete from",
        "insert into",
        "update set",
        "drop table",
    ]
    .iter()
    .any(|part| lower.contains(part));
    !contains_sensitive_label
}
