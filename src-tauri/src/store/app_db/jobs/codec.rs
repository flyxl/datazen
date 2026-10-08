use datazen_platform_api::context::RequestContext;
use datazen_platform_api::dto::job::{
    JobDefinition, JobProgress, JobRecord, JobState, StageRecord,
};
use datazen_platform_api::error::PortError;
use datazen_platform_api::id::{
    ArtifactId, ExecutionId, JobId, JobStateVersion, StageId, Timestamp, WorkerId,
};
use datazen_platform_api::OwnerRef;
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::MAX_DURABLE_JOB_PLAN_BYTES;

pub(super) fn map_db_error(error: crate::store::AppDbError) -> PortError {
    match error {
        crate::store::AppDbError::NotFound(id) => PortError::NotFound(id),
        _ => PortError::BackendUnavailable("local job database operation failed".into()),
    }
}

pub(super) fn encode<T: Serialize>(value: &T) -> Result<String, PortError> {
    serde_json::to_string(value)
        .map_err(|_| PortError::BackendUnavailable("job data could not be encoded".into()))
}

pub(super) fn decode<T: DeserializeOwned>(value: &str) -> Result<T, PortError> {
    serde_json::from_str(value)
        .map_err(|_| PortError::BackendUnavailable("stored job data is invalid".into()))
}

pub(super) fn durable_plan(kind: &str, payload: &Value) -> Result<Value, PortError> {
    datazen_runtime::job::project_frozen_plan(kind, payload)
        .map_err(|_| PortError::BackendUnavailable("job plan is invalid".into()))?;
    let object = payload
        .as_object()
        .ok_or_else(|| PortError::BackendUnavailable("job plan must be an object".into()))?;
    let mut projected = serde_json::Map::new();
    for (key, value) in object {
        if !matches!(
            key.as_str(),
            "kind"
                | "planVersion"
                | "handlerVersion"
                | "checkpointVersion"
                | "selectionRevision"
                | "consumedPlanId"
                | "planId"
                | "planDigest"
                | "sourceTables"
                | "confirmedDestructive"
        ) {
            return Err(PortError::BackendUnavailable(
                "job plan contains a field outside the durable projection".into(),
            ));
        }
        validate_plan_value(key, value)?;
        projected.insert(key.clone(), value.clone());
    }
    let projected = Value::Object(projected);
    let bytes = serde_json::to_vec(&projected)
        .map_err(|_| PortError::BackendUnavailable("job plan could not be encoded".into()))?;
    if bytes.len() > MAX_DURABLE_JOB_PLAN_BYTES {
        return Err(PortError::QuotaExceeded(
            "job plan exceeds the local size limit".into(),
        ));
    }
    Ok(projected)
}

fn validate_plan_value(key: &str, value: &Value) -> Result<(), PortError> {
    let valid = match key {
        "kind" => value.as_str().is_some_and(safe_identifier),
        "consumedPlanId" | "planId" => value.as_str().is_some_and(safe_identifier),
        "planDigest" => value.as_str().is_some_and(|text| {
            text.len() <= 80
                && text.starts_with("sha256:")
                && text[7..].bytes().all(|byte| byte.is_ascii_hexdigit())
        }),
        "planVersion" | "handlerVersion" | "checkpointVersion" | "selectionRevision" => {
            value.as_u64().is_some()
        }
        "confirmedDestructive" => value.as_bool().is_some(),
        "sourceTables" => value.as_array().is_some_and(|items| {
            items.len() <= 100_000
                && items.iter().all(|item| {
                    item.as_str()
                        .is_some_and(|text| !text.is_empty() && text.len() <= 4096)
                })
        }),
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(PortError::BackendUnavailable(format!(
            "job plan field `{key}` has an unsupported durable value"
        )))
    }
}

pub(super) fn safe_identifier(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 512
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:-".contains(&byte))
}

pub(super) fn request_digest(kind: &str, payload: &Value) -> Result<String, PortError> {
    let plan = durable_plan(kind, payload)?;
    Ok(hex_digest(format!("{kind}|{plan}").as_bytes()))
}

pub(super) fn idempotency_hash(ctx: &RequestContext, key: &str) -> String {
    hex_digest(
        format!(
            "datazen-desktop-job-v1|{}|{}|{key}",
            ctx.organization_id.as_str(),
            ctx.principal_id.as_str()
        )
        .as_bytes(),
    )
}

pub(super) fn hex_digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(super) fn read_record(
    conn: &Connection,
    ctx: &RequestContext,
    job_id: &JobId,
    now: &Timestamp,
) -> Result<JobRecord, PortError> {
    let raw = conn
        .query_row(
            "SELECT kind,state,state_version,stage,owner_json,plan_json,execution_ids_json,\
             progress_json,effect_outcome,cancel_requested,\
             pending_verification_reason,result_error_code,created_at,updated_at \
             FROM jobs WHERE job_id=?1 AND organization_id=?2 AND owner_principal_id=?3",
            params![
                job_id.as_str(),
                ctx.organization_id.as_str(),
                ctx.principal_id.as_str()
            ],
            RawJob::from_row,
        )
        .optional()
        .map_err(|_| PortError::BackendUnavailable("local job query failed".into()))?
        .ok_or_else(|| PortError::NotFound(job_id.as_str().into()))?;
    build_record(conn, job_id, raw, now)
}

struct RawJob {
    kind: String,
    state: String,
    state_version: i64,
    stage: Option<String>,
    owner: String,
    payload: String,
    execution_ids: String,
    progress: String,
    effect_outcome: Option<String>,
    cancel_requested: bool,
    pending_reason: Option<String>,
    error: Option<String>,
    created_at: String,
    updated_at: String,
}

impl RawJob {
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            kind: row.get(0)?,
            state: row.get(1)?,
            state_version: row.get(2)?,
            stage: row.get(3)?,
            owner: row.get(4)?,
            payload: row.get(5)?,
            execution_ids: row.get(6)?,
            progress: row.get(7)?,
            effect_outcome: row.get(8)?,
            cancel_requested: row.get(9)?,
            pending_reason: row.get(10)?,
            error: row.get(11)?,
            created_at: row.get(12)?,
            updated_at: row.get(13)?,
        })
    }
}

fn build_record(
    conn: &Connection,
    job_id: &JobId,
    raw: RawJob,
    now: &Timestamp,
) -> Result<JobRecord, PortError> {
    let state: JobState = decode_enum(&raw.state)?;
    let effect_outcome = raw.effect_outcome.as_deref().map(decode_enum).transpose()?;
    let owner: OwnerRef = decode(&raw.owner)?;
    let payload: Value = decode(&raw.payload)?;
    let execution_ids: Vec<ExecutionId> = decode(&raw.execution_ids)?;
    let artifact_ids = read_artifacts(conn, job_id, now)?;
    let progress: JobProgress = decode(&raw.progress)?;
    let stages = read_stages(conn, job_id)?;
    Ok(JobRecord {
        view: datazen_platform_api::dto::job::JobView {
            job_id: job_id.clone(),
            kind: raw.kind.clone(),
            state,
            stage: raw.stage,
            execution_ids,
            artifact_ids,
            created_at: Timestamp::new(raw.created_at.clone()),
            updated_at: Timestamp::new(raw.updated_at),
            effect_outcome,
            cancel_requested: raw.cancel_requested,
            pending_verification_reason: raw.pending_reason,
            error: raw.error,
            progress,
        },
        definition: JobDefinition {
            job_id: job_id.clone(),
            kind: raw.kind,
            owner,
            payload,
            created_at: Timestamp::new(raw.created_at),
        },
        stages,
        state_version: JobStateVersion::new(
            u64::try_from(raw.state_version)
                .map_err(|_| PortError::BackendUnavailable("invalid job state version".into()))?,
        ),
    })
}

fn read_artifacts(
    conn: &Connection,
    job_id: &JobId,
    now: &Timestamp,
) -> Result<Vec<ArtifactId>, PortError> {
    let mut statement = conn
        .prepare(
            "SELECT artifact_id FROM job_artifact_refs WHERE job_id=?1 AND expires_at>?2 ORDER BY created_at,artifact_id",
        )
        .map_err(|_| PortError::BackendUnavailable("local job artifact query failed".into()))?;
    let rows = statement
        .query_map(params![job_id.as_str(), now.as_str()], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|_| PortError::BackendUnavailable("local job artifact query failed".into()))?;
    let mut artifacts = Vec::new();
    for row in rows {
        let artifact = row
            .map_err(|_| PortError::BackendUnavailable("local job artifact read failed".into()))?;
        artifacts.push(ArtifactId::new(artifact));
    }
    Ok(artifacts)
}

fn read_stages(conn: &Connection, job_id: &JobId) -> Result<Vec<StageRecord>, PortError> {
    let mut statement = conn
        .prepare(
            "SELECT stage_id,kind,claimed_by,execution_ids_json,started_at,finished_at \
             FROM job_stages WHERE job_id=?1 ORDER BY rowid",
        )
        .map_err(|_| PortError::BackendUnavailable("local job stage query failed".into()))?;
    let rows = statement
        .query_map(params![job_id.as_str()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })
        .map_err(|_| PortError::BackendUnavailable("local job stage query failed".into()))?;
    let mut stages = Vec::new();
    for row in rows {
        let (stage_id, kind, worker, execution_ids, started, finished) =
            row.map_err(|_| PortError::BackendUnavailable("local job stage read failed".into()))?;
        stages.push(StageRecord {
            job_id: job_id.clone(),
            stage_id: StageId::new(stage_id),
            kind,
            claimed_by: worker.map(WorkerId::new),
            execution_ids: decode(&execution_ids)?,
            started_at: started.map(Timestamp::new),
            finished_at: finished.map(Timestamp::new),
        });
    }
    Ok(stages)
}

fn decode_enum<T: DeserializeOwned>(value: &str) -> Result<T, PortError> {
    serde_json::from_value(json!(value))
        .map_err(|_| PortError::BackendUnavailable("stored Job enum is invalid".into()))
}
