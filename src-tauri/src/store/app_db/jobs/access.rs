use datazen_platform_api::context::RequestContext;
use datazen_platform_api::dto::job::{
    CommitBoundary, JobDetails, JobFilter, JobRecord, JobRecoveryResult, JobState, RecoveryFilter,
};
use datazen_platform_api::error::PortError;
use datazen_platform_api::id::{JobId, Timestamp};
use rusqlite::{params, OptionalExtension};

use super::codec::{decode, read_record};
use super::SqliteJobRepository;

const MAX_JOB_PAGE: u32 = 500;

fn state_text(state: &JobState) -> Result<String, PortError> {
    serde_json::to_value(state)
        .map_err(|_| PortError::BackendUnavailable("job state encoding failed".into()))?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| PortError::BackendUnavailable("job state is invalid".into()))
}

impl SqliteJobRepository {
    pub(super) async fn get(
        &self,
        ctx: &RequestContext,
        job_id: JobId,
    ) -> Result<JobRecord, PortError> {
        let now = self.clock.now();
        self.with_tx(|tx| {
            prune_expired_artifacts(tx, &now)?;
            read_record(tx, ctx, &job_id, &now)
        })
    }

    pub(super) async fn list(
        &self,
        ctx: &RequestContext,
        filter: JobFilter,
    ) -> Result<Vec<JobRecord>, PortError> {
        let now = self.clock.now();
        self.with_tx(|tx| {
            prune_expired_artifacts(tx, &now)?;
            let mut statement = tx
                .prepare(
                    "SELECT job_id FROM jobs WHERE organization_id=?1 AND owner_principal_id=?2 ORDER BY created_at,job_id",
                )
                .map_err(db_read_error)?;
            let ids = statement
                .query_map(
                    params![ctx.organization_id.as_str(), ctx.principal_id.as_str()],
                    |row| row.get::<_, String>(0),
                )
                .map_err(db_read_error)?;
            let mut records = Vec::new();
            for id in ids {
                let id = id.map_err(db_read_error)?;
                let record = read_record(tx, ctx, &JobId::new(id), &now)?;
                if !filter.states.is_empty() && !filter.states.contains(&record.view.state) {
                    continue;
                }
                if let Some(owner) = &filter.owner {
                    if &record.definition.owner != owner {
                        continue;
                    }
                }
                records.push(record);
            }
            let offset = filter.after.map_or(0, |after| after.get());
            let start = usize::try_from(offset).unwrap_or(usize::MAX).min(records.len());
            let limit = filter.limit.unwrap_or(MAX_JOB_PAGE).min(MAX_JOB_PAGE) as usize;
            Ok(records.into_iter().skip(start).take(limit).collect())
        })
    }

    pub(super) async fn list_recoverable(
        &self,
        ctx: &RequestContext,
        filter: RecoveryFilter,
    ) -> Result<Vec<JobRecord>, PortError> {
        let now = self.clock.now();
        self.with_tx(|tx| {
            prune_expired_artifacts(tx, &now)?;
            let mut statement = tx
                .prepare(
                    "SELECT job_id FROM jobs WHERE organization_id=?1 AND owner_principal_id=?2 \
                     AND (state IN ('queued','running','cancelled') OR pending_verification_reason IS NOT NULL) \
                     AND (?3 IS NULL OR updated_at<=?3) ORDER BY created_at,job_id LIMIT ?4",
                )
                .map_err(db_read_error)?;
            let limit = filter.limit.unwrap_or(MAX_JOB_PAGE).min(MAX_JOB_PAGE);
            let ids = statement
                .query_map(
                    params![
                        ctx.organization_id.as_str(),
                        ctx.principal_id.as_str(),
                        filter.older_than.as_ref().map(Timestamp::as_str),
                        i64::from(limit),
                    ],
                    |row| row.get::<_, String>(0),
                )
                .map_err(db_read_error)?;
            let mut records = Vec::new();
            for id in ids {
                let id = id.map_err(db_read_error)?;
                let record = read_record(tx, ctx, &JobId::new(id), &now)?;
                if !filter.kinds.is_empty() && !filter.kinds.contains(&record.definition.kind) {
                    continue;
                }
                records.push(record);
            }
            Ok(records)
        })
    }

    pub(super) async fn get_details(
        &self,
        ctx: &RequestContext,
        job_id: JobId,
    ) -> Result<JobDetails, PortError> {
        let now = self.clock.now();
        self.with_tx(|tx| {
            prune_expired_artifacts(tx, &now)?;
            let record = read_record(tx, ctx, &job_id, &now)?;
            let result_row = tx
                .query_row(
                    "SELECT plan_id,plan_digest,selection_revision,recovery_json \
                     FROM job_result_details WHERE job_id=?1",
                    params![job_id.as_str()],
                    |row| {
                        Ok((
                            row.get::<_, Option<String>>(0)?,
                            row.get::<_, Option<String>>(1)?,
                            row.get::<_, Option<i64>>(2)?,
                            row.get::<_, Option<String>>(3)?,
                        ))
                    },
                )
                .optional()
                .map_err(db_read_error)?;
            let (plan_id, plan_digest, selection_revision, recovery) =
                result_row.unwrap_or((None, None, None, None));
            let payload_plan_id = record
                .definition
                .payload
                .get("consumedPlanId")
                .or_else(|| record.definition.payload.get("planId"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned);
            let payload_digest = record
                .definition
                .payload
                .get("planDigest")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned);
            let payload_revision = record
                .definition
                .payload
                .get("selectionRevision")
                .and_then(serde_json::Value::as_u64);
            let committed = read_boundaries(tx, &job_id)?;
            let recovery = recovery
                .map(|json| decode::<JobRecoveryResult>(&json))
                .transpose()?;
            let selection_revision = match selection_revision {
                Some(revision) => Some(u64::try_from(revision).map_err(|_| {
                    PortError::BackendUnavailable("stored plan revision is invalid".into())
                })?),
                None => payload_revision,
            };
            Ok(JobDetails {
                job: record.view,
                plan_id: plan_id.or(payload_plan_id),
                plan_digest: plan_digest.or(payload_digest),
                selection_revision,
                commit_boundaries: committed,
                recovery,
            })
        })
    }

    pub(super) async fn committed_boundaries(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
    ) -> Result<Vec<CommitBoundary>, PortError> {
        let now = self.clock.now();
        self.with_read(|conn| {
            let _ = read_record(conn, ctx, job_id, &now)?;
            read_boundaries(conn, job_id)
        })
    }
}

fn read_boundaries(
    conn: &rusqlite::Connection,
    job_id: &JobId,
) -> Result<Vec<CommitBoundary>, PortError> {
    let mut statement = conn
        .prepare(
            "SELECT boundary_json FROM job_commit_boundaries WHERE job_id=?1 ORDER BY sequence",
        )
        .map_err(db_read_error)?;
    let rows = statement
        .query_map(params![job_id.as_str()], |row| row.get::<_, String>(0))
        .map_err(db_read_error)?;
    let mut boundaries = Vec::new();
    for row in rows {
        boundaries.push(decode(&row.map_err(db_read_error)?)?);
    }
    Ok(boundaries)
}

pub(super) fn prune_expired_artifacts(
    conn: &rusqlite::Connection,
    now: &Timestamp,
) -> Result<(), PortError> {
    conn.execute(
        "DELETE FROM job_artifact_refs WHERE expires_at<=?1",
        params![now.as_str()],
    )
    .map(|_| ())
    .map_err(db_write_error)
}

pub(super) fn db_read_error(_: rusqlite::Error) -> PortError {
    PortError::BackendUnavailable("local job database read failed".into())
}

pub(super) fn db_write_error(_: rusqlite::Error) -> PortError {
    PortError::BackendUnavailable("local job database write failed".into())
}
