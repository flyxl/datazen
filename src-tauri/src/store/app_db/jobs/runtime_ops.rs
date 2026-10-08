use datazen_platform_api::context::RequestContext;
use datazen_platform_api::dto::execution::EffectOutcome;
use datazen_platform_api::dto::job::{
    Checkpoint, CommitBoundary, JobClaim, JobDetails, JobDomainResult, JobProgress, JobRecord,
    JobRecoveryResult, JobRecoveryVerdict, JobRecoveryVerification, JobState,
};
use datazen_platform_api::error::PortError;
use datazen_platform_api::id::{
    ArtifactId, IdempotencyKey, JobId, JobStateVersion, StageId, Timestamp, WorkerId,
};
use datazen_runtime::job::repository::CancelPollSnapshot;
use datazen_runtime::job::runtime_repository::JobRuntimeRepository;
use datazen_runtime::job::time::after_seconds;
use rusqlite::{params, OptionalExtension, Transaction};

use super::access::{db_read_error, db_write_error, prune_expired_artifacts};
use super::codec::{decode, encode, idempotency_hash, read_record, safe_identifier};
use super::SqliteJobRepository;

#[derive(Debug)]
struct ClaimRow {
    organization: String,
    principal: String,
    state: String,
    worker: Option<String>,
    generation: i64,
    expires_at: Option<String>,
}

pub(super) fn check_claim(
    conn: &Transaction<'_>,
    ctx: &RequestContext,
    claim: &JobClaim,
    now: &Timestamp,
) -> Result<(), PortError> {
    let current = conn
        .query_row(
            "SELECT organization_id,owner_principal_id,state,worker_id,claim_generation,claim_expires_at \
             FROM jobs WHERE job_id=?1",
            params![claim.job_id.as_str()],
            |row| {
                Ok(ClaimRow {
                    organization: row.get(0)?,
                    principal: row.get(1)?,
                    state: row.get(2)?,
                    worker: row.get(3)?,
                    generation: row.get(4)?,
                    expires_at: row.get(5)?,
                })
            },
        )
        .optional()
        .map_err(db_read_error)?
        .ok_or_else(|| PortError::NotFound(claim.job_id.as_str().into()))?;
    let generation = i64::try_from(claim.claim_generation.get()).unwrap_or(i64::MAX);
    let valid = current.organization == ctx.organization_id.as_str()
        && current.principal == ctx.principal_id.as_str()
        && current.state == "running"
        && current.worker.as_deref() == Some(claim.worker_id.as_str())
        && current.generation == generation
        && current
            .expires_at
            .as_deref()
            .is_some_and(|expiry| expiry > now.as_str());
    if valid {
        Ok(())
    } else {
        Err(PortError::StaleClaim)
    }
}

impl SqliteJobRepository {
    async fn cancel_poll_for(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
    ) -> Result<CancelPollSnapshot, PortError> {
        self.with_read(|conn| {
            let row = conn
                .query_row(
                    "SELECT state,cancel_requested FROM jobs WHERE job_id=?1 AND organization_id=?2 AND owner_principal_id=?3",
                    params![job_id.as_str(),ctx.organization_id.as_str(),ctx.principal_id.as_str()],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?)),
                )
                .optional()
                .map_err(db_read_error)?
                .ok_or_else(|| PortError::NotFound(job_id.as_str().into()))?;
            Ok(CancelPollSnapshot {
                cancel_requested: row.1,
                terminal: matches!(row.0.as_str(), "succeeded" | "failed" | "cancelled"),
            })
        })
    }

    async fn request_cancel_for(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
    ) -> Result<JobRecord, PortError> {
        let now = self.clock.now();
        self.with_tx(|tx| {
            let changed = tx
                .execute(
                    "UPDATE jobs SET cancel_requested=1,cancel_requested_at=?1,updated_at=?1 \
                     WHERE job_id=?2 AND organization_id=?3 AND owner_principal_id=?4 \
                     AND state IN ('queued','running')",
                    params![now.as_str(),job_id.as_str(),ctx.organization_id.as_str(),ctx.principal_id.as_str()],
                )
                .map_err(db_write_error)?;
            if changed == 0 {
                let exists = tx
                    .query_row(
                        "SELECT 1 FROM jobs WHERE job_id=?1 AND organization_id=?2 AND owner_principal_id=?3",
                        params![job_id.as_str(),ctx.organization_id.as_str(),ctx.principal_id.as_str()],
                        |_| Ok(()),
                    )
                    .optional()
                    .map_err(db_read_error)?;
                if exists.is_none() {
                    return Err(PortError::NotFound(job_id.as_str().into()));
                }
                return Err(PortError::CasConflict {
                    entity: "job",
                    id: job_id.as_str().into(),
                });
            }
            read_record(tx, ctx, job_id, &now)
        })
    }

    async fn confirm_cancelled_for(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
        outcome: EffectOutcome,
    ) -> Result<JobRecord, PortError> {
        let now = self.clock.now();
        let outcome_text = enum_text(&outcome)?;
        self.with_tx(|tx| {
            let changed = tx
                .execute(
                    "UPDATE jobs SET state='cancelled',effect_outcome=?1,state_version=state_version+1, \
                     updated_at=?2,worker_id=NULL,claim_stage_id=NULL,claim_expires_at=NULL \
                     WHERE job_id=?3 AND organization_id=?4 AND owner_principal_id=?5 \
                     AND state='queued' AND cancel_requested=1 AND worker_id IS NULL",
                    params![outcome_text,now.as_str(),job_id.as_str(),ctx.organization_id.as_str(),ctx.principal_id.as_str()],
                )
                .map_err(db_write_error)?;
            if changed != 1 {
                return Err(not_found_or_conflict(tx, ctx, job_id)?);
            }
            read_record(tx, ctx, job_id, &now)
        })
    }

    async fn mark_failed_unstarted_for(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
        reason_code: &str,
    ) -> Result<JobRecord, PortError> {
        validate_code(reason_code)?;
        let now = self.clock.now();
        self.with_tx(|tx| {
            let changed = tx
                .execute(
                    "UPDATE jobs SET state='failed',effect_outcome='notStarted',result_error_code=?1, \
                     pending_verification_reason=NULL,state_version=state_version+1,updated_at=?2 \
                     WHERE job_id=?3 AND organization_id=?4 AND owner_principal_id=?5 \
                     AND state='queued' AND worker_id IS NULL",
                    params![reason_code,now.as_str(),job_id.as_str(),ctx.organization_id.as_str(),ctx.principal_id.as_str()],
                )
                .map_err(db_write_error)?;
            if changed != 1 {
                return Err(not_found_or_conflict(tx, ctx, job_id)?);
            }
            let recovery = JobRecoveryResult {
                verdict: JobRecoveryVerdict::NotExecuted,
                resume_through: None,
                reason_code: Some(reason_code.to_owned()),
            };
            tx.execute(
                "INSERT INTO job_result_details(job_id,recovery_json) VALUES (?1,?2) ON CONFLICT(job_id) DO UPDATE SET recovery_json=excluded.recovery_json",
                params![job_id.as_str(), encode(&recovery)?],
            )
            .map_err(db_write_error)?;
            read_record(tx, ctx, job_id, &now)
        })
    }

    async fn mark_pending_for(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
        reason_code: &str,
    ) -> Result<JobRecord, PortError> {
        validate_code(reason_code)?;
        let now = self.clock.now();
        self.with_tx(|tx| {
            let old_state = tx
                .query_row(
                    "SELECT state,worker_id,effect_outcome FROM jobs WHERE job_id=?1 AND organization_id=?2 AND owner_principal_id=?3",
                    params![job_id.as_str(),ctx.organization_id.as_str(),ctx.principal_id.as_str()],
                    |row| Ok((row.get::<_, String>(0)?,row.get::<_, Option<String>>(1)?,row.get::<_, Option<String>>(2)?)),
                )
                .optional()
                .map_err(db_read_error)?
                .ok_or_else(|| PortError::NotFound(job_id.as_str().into()))?;
            let was_terminal = matches!(old_state.0.as_str(), "succeeded" | "failed" | "cancelled");
            let outcome = if was_terminal {
                old_state.2.unwrap_or_else(|| "unknown".into())
            } else if old_state.1.is_some() || old_state.0 == "running" {
                "unknown".into()
            } else {
                "notStarted".into()
            };
            let state = if was_terminal { old_state.0 } else { "failed".into() };
            let version_bump = !was_terminal;
            tx.execute(
                "UPDATE jobs SET state=?1,effect_outcome=?2,result_error_code=?3, \
                 pending_verification_reason=?3,state_version=state_version+?4,updated_at=?5, \
                 worker_id=NULL,claim_stage_id=NULL,claim_expires_at=NULL \
                 WHERE job_id=?6 AND organization_id=?7 AND owner_principal_id=?8",
                params![state,outcome,reason_code,if version_bump {1} else {0},now.as_str(),job_id.as_str(),ctx.organization_id.as_str(),ctx.principal_id.as_str()],
            )
            .map_err(db_write_error)?;
            let recovery = JobRecoveryResult {
                verdict: JobRecoveryVerdict::PendingVerification,
                resume_through: None,
                reason_code: Some(reason_code.to_string()),
            };
            tx.execute(
                "INSERT INTO job_result_details(job_id,recovery_json) VALUES (?1,?2) \
                 ON CONFLICT(job_id) DO UPDATE SET recovery_json=excluded.recovery_json",
                params![job_id.as_str(), encode(&recovery)?],
            )
            .map_err(db_write_error)?;
            read_record(tx, ctx, job_id, &now)
        })
    }

    async fn record_progress_for(
        &self,
        ctx: &RequestContext,
        claim: &JobClaim,
        progress: JobProgress,
    ) -> Result<(), PortError> {
        let now = self.clock.now();
        let progress_json = encode(&progress)?;
        self.with_tx(|tx| {
            check_claim(tx, ctx, claim, &now)?;
            tx.execute(
                "UPDATE jobs SET progress_json=?1,updated_at=?2 WHERE job_id=?3",
                params![progress_json, now.as_str(), claim.job_id.as_str()],
            )
            .map_err(db_write_error)?;
            Ok(())
        })
    }

    async fn record_artifacts_for(
        &self,
        ctx: &RequestContext,
        claim: &JobClaim,
        artifacts: &[ArtifactId],
    ) -> Result<(), PortError> {
        let now = self.clock.now();
        let expires =
            after_seconds(&now, super::JOB_ARTIFACT_REFERENCE_TTL_SECS).map_err(PortError::from)?;
        validate_artifacts(artifacts)?;
        self.with_tx(|tx| {
            prune_expired_artifacts(tx, &now)?;
            check_claim(tx, ctx, claim, &now)?;
            for artifact in artifacts {
                tx.execute(
                    "INSERT INTO job_artifact_refs(job_id,artifact_id,created_at,expires_at) \
                     VALUES (?1,?2,?3,?4) ON CONFLICT(job_id,artifact_id) DO NOTHING",
                    params![
                        claim.job_id.as_str(),
                        artifact.as_str(),
                        now.as_str(),
                        expires.as_str()
                    ],
                )
                .map_err(db_write_error)?;
            }
            tx.execute(
                "UPDATE jobs SET updated_at=?1 WHERE job_id=?2",
                params![now.as_str(), claim.job_id.as_str()],
            )
            .map_err(db_write_error)?;
            Ok(())
        })
    }

    async fn append_external_artifacts_for(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
        artifacts: &[ArtifactId],
    ) -> Result<(), PortError> {
        validate_artifacts(artifacts)?;
        let now = self.clock.now();
        let expires =
            after_seconds(&now, super::JOB_ARTIFACT_REFERENCE_TTL_SECS).map_err(PortError::from)?;
        self.with_tx(|tx| {
            prune_expired_artifacts(tx, &now)?;
            let state = tx
                .query_row(
                    "SELECT state FROM jobs WHERE job_id=?1 AND organization_id=?2 AND owner_principal_id=?3",
                    params![job_id.as_str(),ctx.organization_id.as_str(),ctx.principal_id.as_str()],
                    |row| row.get::<_,String>(0),
                )
                .optional()
                .map_err(db_read_error)?
                .ok_or_else(|| PortError::NotFound(job_id.as_str().into()))?;
            if !matches!(state.as_str(), "succeeded" | "failed" | "cancelled") {
                return Err(PortError::CasConflict {
                    entity: "job_artifact",
                    id: job_id.as_str().into(),
                });
            }
            for artifact in artifacts {
                tx.execute(
                    "INSERT INTO job_artifact_refs(job_id,artifact_id,created_at,expires_at) \
                     VALUES (?1,?2,?3,?4) ON CONFLICT(job_id,artifact_id) DO NOTHING",
                    params![job_id.as_str(),artifact.as_str(),now.as_str(),expires.as_str()],
                )
                .map_err(db_write_error)?;
            }
            tx.execute("UPDATE jobs SET updated_at=?1 WHERE job_id=?2",params![now.as_str(),job_id.as_str()]).map_err(db_write_error)?;
            Ok(())
        })
    }

    async fn finish_job(
        &self,
        ctx: &RequestContext,
        claim: &JobClaim,
        expected: JobStateVersion,
        state: JobState,
        outcome: EffectOutcome,
        progress: JobProgress,
        error_code: Option<String>,
        pending_reason: Option<String>,
    ) -> Result<JobRecord, PortError> {
        if !state.is_terminal() {
            return Err(PortError::BackendUnavailable(
                "job finish requires terminal state".into(),
            ));
        }
        if let Some(code) = error_code.as_deref() {
            validate_code(code)?;
        }
        if let Some(code) = pending_reason.as_deref() {
            validate_code(code)?;
        }
        let now = self.clock.now();
        let state_text = enum_text(&state)?;
        let outcome_text = enum_text(&outcome)?;
        let progress_json = encode(&progress)?;
        self.with_tx(|tx| {
            check_claim(tx, ctx, claim, &now)?;
            let expected_version = i64::try_from(expected.get())
                .map_err(|_| PortError::BackendUnavailable("job version overflow".into()))?;
            let changed = tx
                .execute(
                    "UPDATE jobs SET state=?1,effect_outcome=?2,progress_json=?3,result_error_code=?4, \
                     pending_verification_reason=?5,state_version=state_version+1,updated_at=?6, \
                     worker_id=NULL,claim_stage_id=NULL,claim_expires_at=NULL \
                     WHERE job_id=?7 AND organization_id=?8 AND owner_principal_id=?9 AND state_version=?10",
                    params![state_text,outcome_text,progress_json,error_code,pending_reason,now.as_str(),claim.job_id.as_str(),ctx.organization_id.as_str(),ctx.principal_id.as_str(),expected_version],
                )
                .map_err(db_write_error)?;
            if changed != 1 {
                return Err(PortError::CasConflict {
                    entity: "job",
                    id: claim.job_id.as_str().into(),
                });
            }
            if let Some(reason_code) = pending_reason.as_deref() {
                let recovery = JobRecoveryResult {
                    verdict: JobRecoveryVerdict::PendingVerification,
                    resume_through: None,
                    reason_code: Some(reason_code.to_owned()),
                };
                tx.execute(
                    "INSERT INTO job_result_details(job_id,recovery_json) VALUES (?1,?2) ON CONFLICT(job_id) DO UPDATE SET recovery_json=excluded.recovery_json",
                    params![claim.job_id.as_str(), encode(&recovery)?],
                )
                .map_err(db_write_error)?;
            }
            read_record(tx, ctx, &claim.job_id, &now)
        })
    }

    async fn persist_recovery_for(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
        recovery: JobRecoveryResult,
    ) -> Result<(), PortError> {
        if let Some(code) = recovery.reason_code.as_deref() {
            validate_code(code)?;
        }
        let now = self.clock.now();
        let recovery_json = encode(&recovery)?;
        self.with_tx(|tx| {
            let boundaries: i64 = tx
                .query_row(
                    "SELECT COUNT(*) FROM job_commit_boundaries WHERE job_id=?1",
                    params![job_id.as_str()],
                    |row| row.get(0),
                )
                .map_err(db_read_error)?;
            if recovery
                .resume_through
                .is_some_and(|index| i64::try_from(index).unwrap_or(i64::MAX) > boundaries)
            {
                return Err(PortError::BackendUnavailable("recovery boundary index is invalid".into()));
            }
            let changed = tx
                .execute(
                    "INSERT INTO job_result_details(job_id,recovery_json) \
                     SELECT job_id,?1 FROM jobs WHERE job_id=?2 AND organization_id=?3 AND owner_principal_id=?4 \
                     ON CONFLICT(job_id) DO UPDATE SET recovery_json=excluded.recovery_json",
                    params![recovery_json,job_id.as_str(),ctx.organization_id.as_str(),ctx.principal_id.as_str()],
                )
                .map_err(db_write_error)?;
            if changed != 1 {
                return Err(PortError::NotFound(job_id.as_str().into()));
            }
            tx.execute(
                "UPDATE jobs SET pending_verification_reason=CASE WHEN ?1 IN ('pendingVerification','reject','requireManualReview') THEN ?2 ELSE pending_verification_reason END, \
                 updated_at=?3 WHERE job_id=?4 AND organization_id=?5 AND owner_principal_id=?6",
                params![verdict_text(recovery.verdict),recovery.reason_code,now.as_str(),job_id.as_str(),ctx.organization_id.as_str(),ctx.principal_id.as_str()],
            )
            .map_err(db_write_error)?;
            Ok(())
        })
    }

    async fn receipt_for_key(
        &self,
        ctx: &RequestContext,
        key: &IdempotencyKey,
    ) -> Result<Option<JobId>, PortError> {
        let key_hash = idempotency_hash(ctx, key.as_str());
        self.with_read(|conn| {
            conn.query_row(
                "SELECT job_id FROM job_idempotency_receipts WHERE organization_id=?1 AND owner_principal_id=?2 AND idempotency_key_hash=?3",
                params![ctx.organization_id.as_str(),ctx.principal_id.as_str(),key_hash],
                |row| row.get::<_,String>(0),
            )
            .optional()
            .map(|job| job.map(JobId::new))
            .map_err(db_read_error)
        })
    }

    async fn latest_checkpoint_for(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
    ) -> Result<Option<Checkpoint>, PortError> {
        self.with_read(|conn| {
            let _ = read_record(conn, ctx, job_id, &self.clock.now())?;
            conn.query_row(
                "SELECT checkpoint_json FROM job_checkpoints WHERE job_id=?1 ORDER BY state_version DESC LIMIT 1",
                params![job_id.as_str()],
                |row| row.get::<_,String>(0),
            )
            .optional()
            .map(|value| value.map(|json| decode(&json)).transpose())
            .map_err(db_read_error)?
        })
    }

    pub(super) async fn claim_job(
        &self,
        ctx: &RequestContext,
        job_id: JobId,
        worker: WorkerId,
    ) -> Result<JobClaim, PortError> {
        if !safe_identifier(worker.as_str()) {
            return Err(PortError::BackendUnavailable("worker id is invalid".into()));
        }
        let now = self.clock.now();
        let expiry = after_seconds(&now, self.claim_ttl_secs).map_err(PortError::from)?;
        self.with_tx(|tx| {
            let row = tx
                .query_row(
                    "SELECT state,worker_id,claim_generation,claim_expires_at,state_version \
                     FROM jobs WHERE job_id=?1 AND organization_id=?2 AND owner_principal_id=?3",
                    params![job_id.as_str(),ctx.organization_id.as_str(),ctx.principal_id.as_str()],
                    |row| Ok((row.get::<_,String>(0)?,row.get::<_,Option<String>>(1)?,row.get::<_,i64>(2)?,row.get::<_,Option<String>>(3)?,row.get::<_,i64>(4)?)),
                )
                .optional()
                .map_err(db_read_error)?
                .ok_or_else(|| PortError::NotFound(job_id.as_str().into()))?;
            if matches!(row.0.as_str(),"succeeded"|"failed"|"cancelled") {
                return Err(PortError::CasConflict { entity:"job",id:job_id.as_str().into() });
            }
            if row.0 == "running" && row.3.as_deref().is_some_and(|until| until > now.as_str()) {
                return Err(PortError::StaleClaim);
            }
            let generation = row.2.checked_add(1).ok_or_else(|| PortError::BackendUnavailable("claim generation overflow".into()))?;
            let stage_id = tx.query_row(
                "SELECT stage_id FROM job_stages WHERE job_id=?1 ORDER BY rowid DESC LIMIT 1",
                params![job_id.as_str()],
                |row| row.get::<_,String>(0),
            ).optional().map_err(db_read_error)?.unwrap_or_else(|| "*".into());
            let changed = tx.execute(
                "UPDATE jobs SET state='running',state_version=state_version+1,worker_id=?1,claim_stage_id=?2, \
                 claim_expires_at=?3,claim_generation=?4,updated_at=?5 WHERE job_id=?6 AND organization_id=?7 \
                 AND owner_principal_id=?8 AND state_version=?9",
                params![worker.as_str(),stage_id,expiry.as_str(),generation,now.as_str(),job_id.as_str(),ctx.organization_id.as_str(),ctx.principal_id.as_str(),row.4],
            ).map_err(db_write_error)?;
            if changed != 1 {
                return Err(PortError::CasConflict {entity:"job",id:job_id.as_str().into()});
            }
            Ok(JobClaim {
                job_id,
                stage_id: StageId::new(stage_id),
                worker_id: worker,
                claimed_at: now,
                claim_generation: datazen_platform_api::id::Counter::new(u64::try_from(generation).map_err(|_| PortError::BackendUnavailable("claim generation invalid".into()))?),
                expires_at: expiry,
            })
        })
    }

    pub(super) async fn renew_claim(&self, claim: &JobClaim) -> Result<JobClaim, PortError> {
        let now = self.clock.now();
        let expiry = after_seconds(&now, self.claim_ttl_secs).map_err(PortError::from)?;
        self.with_tx(|tx| {
            let row = tx.query_row(
                "SELECT state,worker_id,claim_generation,claim_expires_at FROM jobs WHERE job_id=?1",
                params![claim.job_id.as_str()],
                |row| Ok((row.get::<_,String>(0)?,row.get::<_,Option<String>>(1)?,row.get::<_,i64>(2)?,row.get::<_,Option<String>>(3)?)),
            ).optional().map_err(db_read_error)?.ok_or_else(||PortError::NotFound(claim.job_id.as_str().into()))?;
            if row.0 != "running"
                || row.1.as_deref() != Some(claim.worker_id.as_str())
                || row.2 != i64::try_from(claim.claim_generation.get()).unwrap_or(i64::MAX)
                || row.3.as_deref().map_or(true, |until| until <= now.as_str())
            {
                return Err(PortError::StaleClaim);
            }
            let changed = tx.execute(
                "UPDATE jobs SET claim_expires_at=?1,updated_at=?2 WHERE job_id=?3 AND worker_id=?4 AND claim_generation=?5",
                params![expiry.as_str(),now.as_str(),claim.job_id.as_str(),claim.worker_id.as_str(),row.2],
            ).map_err(db_write_error)?;
            if changed != 1 { return Err(PortError::StaleClaim); }
            Ok(JobClaim { claimed_at:now,expires_at:expiry,..claim.clone() })
        })
    }
}

#[async_trait::async_trait]
impl JobRuntimeRepository for SqliteJobRepository {
    async fn cancel_poll(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
    ) -> Result<CancelPollSnapshot, PortError> {
        self.cancel_poll_for(ctx, job_id).await
    }
    async fn request_cancel(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
    ) -> Result<JobRecord, PortError> {
        self.request_cancel_for(ctx, job_id).await
    }
    async fn confirm_cancelled_not_started(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
        outcome: EffectOutcome,
    ) -> Result<JobRecord, PortError> {
        self.confirm_cancelled_for(ctx, job_id, outcome).await
    }
    async fn mark_failed_unstarted(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
        reason_code: &str,
    ) -> Result<JobRecord, PortError> {
        self.mark_failed_unstarted_for(ctx, job_id, reason_code)
            .await
    }
    async fn mark_pending_verification(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
        reason_code: &str,
    ) -> Result<JobRecord, PortError> {
        self.mark_pending_for(ctx, job_id, reason_code).await
    }
    async fn record_progress(
        &self,
        ctx: &RequestContext,
        claim: &JobClaim,
        progress: JobProgress,
    ) -> Result<(), PortError> {
        self.record_progress_for(ctx, claim, progress).await
    }
    async fn record_artifacts(
        &self,
        ctx: &RequestContext,
        claim: &JobClaim,
        artifacts: &[ArtifactId],
    ) -> Result<(), PortError> {
        self.record_artifacts_for(ctx, claim, artifacts).await
    }
    async fn record_domain_result(
        &self,
        ctx: &RequestContext,
        claim: &JobClaim,
        result: JobDomainResult,
    ) -> Result<(), PortError> {
        self.write_domain_result(ctx, claim, result).await
    }
    async fn append_external_artifacts(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
        artifacts: &[ArtifactId],
    ) -> Result<(), PortError> {
        self.append_external_artifacts_for(ctx, job_id, artifacts)
            .await
    }
    async fn finish(
        &self,
        ctx: &RequestContext,
        claim: &JobClaim,
        expected: JobStateVersion,
        state: JobState,
        outcome: EffectOutcome,
        progress: JobProgress,
        error_code: Option<String>,
        pending_verification_reason: Option<String>,
    ) -> Result<JobRecord, PortError> {
        self.finish_job(
            ctx,
            claim,
            expected,
            state,
            outcome,
            progress,
            error_code,
            pending_verification_reason,
        )
        .await
    }
    async fn persist_recovery(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
        recovery: JobRecoveryResult,
    ) -> Result<(), PortError> {
        self.persist_recovery_for(ctx, job_id, recovery).await
    }
    async fn persist_recovery_verification(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
        expected: JobStateVersion,
        verification: JobRecoveryVerification,
    ) -> Result<(), PortError> {
        self.write_recovery_verification(ctx, job_id, expected, verification)
            .await
    }
    async fn receipt_for(
        &self,
        ctx: &RequestContext,
        key: &IdempotencyKey,
    ) -> Result<Option<JobId>, PortError> {
        self.receipt_for_key(ctx, key).await
    }
    async fn committed_boundaries(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
    ) -> Result<Vec<CommitBoundary>, PortError> {
        SqliteJobRepository::committed_boundaries(self, ctx, job_id).await
    }
    async fn latest_checkpoint(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
    ) -> Result<Option<Checkpoint>, PortError> {
        self.latest_checkpoint_for(ctx, job_id).await
    }
    async fn get_details(
        &self,
        ctx: &RequestContext,
        job_id: JobId,
    ) -> Result<JobDetails, PortError> {
        SqliteJobRepository::get_details(self, ctx, job_id).await
    }
}

fn not_found_or_conflict(
    conn: &rusqlite::Transaction<'_>,
    ctx: &RequestContext,
    job_id: &JobId,
) -> Result<PortError, PortError> {
    let exists = conn
        .query_row(
            "SELECT 1 FROM jobs WHERE job_id=?1 AND organization_id=?2 AND owner_principal_id=?3",
            params![
                job_id.as_str(),
                ctx.organization_id.as_str(),
                ctx.principal_id.as_str()
            ],
            |_| Ok(()),
        )
        .optional()
        .map_err(db_read_error)?;
    Ok(if exists.is_some() {
        PortError::CasConflict {
            entity: "job",
            id: job_id.as_str().into(),
        }
    } else {
        PortError::NotFound(job_id.as_str().into())
    })
}

fn validate_code(value: &str) -> Result<(), PortError> {
    if safe_identifier(value) && value.len() <= 96 {
        Ok(())
    } else {
        Err(PortError::BackendUnavailable(
            "job result code is invalid".into(),
        ))
    }
}

fn validate_artifacts(artifacts: &[ArtifactId]) -> Result<(), PortError> {
    if artifacts.len() <= 4096
        && artifacts
            .iter()
            .all(|artifact| safe_identifier(artifact.as_str()))
    {
        Ok(())
    } else {
        Err(PortError::BackendUnavailable(
            "job artifact reference is invalid".into(),
        ))
    }
}

fn enum_text<T: serde::Serialize>(value: &T) -> Result<String, PortError> {
    serde_json::to_value(value)
        .map_err(|_| PortError::BackendUnavailable("job enum encoding failed".into()))?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| PortError::BackendUnavailable("job enum has invalid shape".into()))
}

fn verdict_text(verdict: JobRecoveryVerdict) -> &'static str {
    match verdict {
        JobRecoveryVerdict::NotExecuted => "notExecuted",
        JobRecoveryVerdict::PendingVerification => "pendingVerification",
        JobRecoveryVerdict::ResumeAfterVerify => "resumeAfterVerify",
        JobRecoveryVerdict::Reject => "reject",
        JobRecoveryVerdict::RequireManualReview => "requireManualReview",
    }
}
