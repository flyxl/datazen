use datazen_platform_api::context::RequestContext;
use datazen_platform_api::dto::job::{JobDefinition, JobProgress, JobRecord};
use datazen_platform_api::error::PortError;
use datazen_platform_api::id::{IdempotencyKey, JobId, JobStateVersion};
use datazen_runtime::job::project_frozen_plan;
use rusqlite::{params, OptionalExtension};
use serde_json::json;

use super::access::{db_read_error, db_write_error};
use super::codec::{
    durable_plan, encode, idempotency_hash, read_record, request_digest, safe_identifier,
};
use super::SqliteJobRepository;

impl SqliteJobRepository {
    pub(super) async fn admit_job(
        &self,
        ctx: &RequestContext,
        mut definition: JobDefinition,
        idem: &IdempotencyKey,
    ) -> Result<JobRecord, PortError> {
        if idem.is_empty() || idem.as_str().len() > 1024 {
            return Err(PortError::BackendUnavailable(
                "idempotency key is outside the local size limit".into(),
            ));
        }
        if !safe_identifier(definition.job_id.as_str())
            || !safe_identifier(&definition.kind)
            || definition.job_id.is_empty()
        {
            return Err(PortError::BackendUnavailable(
                "job identity is outside the local format".into(),
            ));
        }
        let plan =
            project_frozen_plan(&definition.kind, &definition.payload).map_err(PortError::from)?;
        let payload = durable_plan(&definition.kind, &definition.payload)?;
        let digest = request_digest(&definition.kind, &payload)?;
        let key_hash = idempotency_hash(ctx, idem.as_str());
        let now = self.clock.now();
        definition.created_at = now.clone();
        let owner_json = encode(&definition.owner)?;
        let payload_json = encode(&payload)?;
        let progress_json = encode(&JobProgress::default())?;
        let consumed_plan_id = plan.consumed_plan_id;
        let plan_id = payload
            .get("planId")
            .or_else(|| payload.get("consumedPlanId"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        let plan_digest = payload
            .get("planDigest")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        let selection_revision = payload
            .get("selectionRevision")
            .and_then(serde_json::Value::as_u64)
            .map(i64::try_from)
            .transpose()
            .map_err(|_| {
                PortError::BackendUnavailable("plan revision exceeds local range".into())
            })?;
        let job_id = definition.job_id.clone();

        self.with_tx(|tx| {
            let receipt = tx
                .query_row(
                    "SELECT request_digest,job_id FROM job_idempotency_receipts \
                     WHERE organization_id=?1 AND owner_principal_id=?2 AND idempotency_key_hash=?3",
                    params![
                        ctx.organization_id.as_str(),
                        ctx.principal_id.as_str(),
                        key_hash,
                    ],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()
                .map_err(db_read_error)?;
            if let Some((recorded_digest, recorded_id)) = receipt {
                if recorded_digest != digest {
                    return Err(PortError::IdempotencyConflict);
                }
                return read_record(tx, ctx, &JobId::new(recorded_id), &now);
            }
            if let Some(plan_id) = &consumed_plan_id {
                let consumed = tx
                    .query_row(
                        "SELECT job_id FROM jobs WHERE organization_id=?1 AND consumed_plan_id=?2",
                        params![ctx.organization_id.as_str(), plan_id],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()
                    .map_err(db_read_error)?;
                if consumed.is_some() {
                    return Err(PortError::PlanAlreadyConsumed(plan_id.clone()));
                }
            }
            tx.execute(
                "INSERT INTO jobs (job_id,organization_id,owner_principal_id,kind,state,state_version,\
                 owner_json,plan_json,progress_json,created_at,updated_at,consumed_plan_id) \
                 VALUES (?1,?2,?3,?4,'queued',1,?5,?6,?7,?8,?8,?9)",
                params![
                    job_id.as_str(),
                    ctx.organization_id.as_str(),
                    ctx.principal_id.as_str(),
                    definition.kind,
                    owner_json,
                    payload_json,
                    progress_json,
                    now.as_str(),
                    consumed_plan_id,
                ],
            )
            .map_err(db_write_error)?;
            let receipt_projection = encode(&json!({
                "jobId": job_id.as_str(),
                "requestDigest": digest,
            }))?;
            tx.execute(
                "INSERT INTO job_idempotency_receipts \
                 (organization_id,owner_principal_id,idempotency_key_hash,request_digest,receipt_projection,job_id,created_at) \
                 VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![
                    ctx.organization_id.as_str(),
                    ctx.principal_id.as_str(),
                    key_hash,
                    digest,
                    receipt_projection,
                    job_id.as_str(),
                    now.as_str(),
                ],
            )
            .map_err(db_write_error)?;
            tx.execute(
                "INSERT INTO job_result_details(job_id,plan_id,plan_digest,selection_revision) \
                 VALUES (?1,?2,?3,?4)",
                params![job_id.as_str(), plan_id, plan_digest, selection_revision],
            )
            .map_err(db_write_error)?;
            read_record(tx, ctx, &job_id, &now)
        })
    }
}

#[async_trait::async_trait]
impl datazen_platform_api::ports::job::JobRepository for SqliteJobRepository {
    async fn accept(
        &self,
        ctx: &RequestContext,
        definition: JobDefinition,
        idem: &IdempotencyKey,
    ) -> Result<JobRecord, PortError> {
        self.admit_job(ctx, definition, idem).await
    }

    async fn get(&self, ctx: &RequestContext, job_id: JobId) -> Result<JobRecord, PortError> {
        SqliteJobRepository::get(self, ctx, job_id).await
    }

    async fn list(
        &self,
        ctx: &RequestContext,
        filter: datazen_platform_api::dto::job::JobFilter,
    ) -> Result<Vec<JobRecord>, PortError> {
        SqliteJobRepository::list(self, ctx, filter).await
    }

    async fn record_stage(
        &self,
        ctx: &RequestContext,
        claim: &datazen_platform_api::dto::job::JobClaim,
        stage: datazen_platform_api::dto::job::StageRecord,
    ) -> Result<(), PortError> {
        self.write_stage(ctx, claim, stage).await
    }

    async fn record_commit_boundary(
        &self,
        ctx: &RequestContext,
        claim: &datazen_platform_api::dto::job::JobClaim,
        boundary: datazen_platform_api::dto::job::CommitBoundary,
    ) -> Result<(), PortError> {
        self.write_boundary(ctx, claim, boundary).await
    }

    async fn compare_and_set_state(
        &self,
        ctx: &RequestContext,
        claim: &datazen_platform_api::dto::job::JobClaim,
        expected: JobStateVersion,
        next: datazen_platform_api::dto::job::JobState,
    ) -> Result<JobRecord, PortError> {
        self.cas_state(ctx, claim, expected, next).await
    }

    async fn claim(
        &self,
        ctx: &RequestContext,
        job_id: JobId,
        worker: datazen_platform_api::id::WorkerId,
    ) -> Result<datazen_platform_api::dto::job::JobClaim, PortError> {
        self.claim_job(ctx, job_id, worker).await
    }

    async fn renew(
        &self,
        claim: &datazen_platform_api::dto::job::JobClaim,
    ) -> Result<datazen_platform_api::dto::job::JobClaim, PortError> {
        self.renew_claim(claim).await
    }

    async fn save_checkpoint(
        &self,
        ctx: &RequestContext,
        claim: &datazen_platform_api::dto::job::JobClaim,
        cp: datazen_platform_api::dto::job::Checkpoint,
    ) -> Result<(), PortError> {
        self.write_checkpoint(ctx, claim, cp).await
    }

    async fn list_recoverable(
        &self,
        ctx: &RequestContext,
        filter: datazen_platform_api::dto::job::RecoveryFilter,
    ) -> Result<Vec<JobRecord>, PortError> {
        SqliteJobRepository::list_recoverable(self, ctx, filter).await
    }
}
