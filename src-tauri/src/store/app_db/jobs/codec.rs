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
    if kind == datazen_feature_core::restore_job::RESTORE_APPLY_KIND {
        let projected = super::restore_projection::project(payload)?;
        check_plan_size(&projected)?;
        return Ok(projected);
    }
    let object = payload
        .as_object()
        .ok_or_else(|| PortError::BackendUnavailable("job plan must be an object".into()))?;
    if kind == "monitorOccurrence" && !object.contains_key("monitorOccurrence") {
        return Err(PortError::BackendUnavailable(
            "monitor occurrence plan is missing its durable identity".into(),
        ));
    }
    let mut projected = serde_json::Map::new();
    for (key, value) in object {
        let common_field = matches!(
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
                | "recoveryTargets"
                | "targetBeforeFingerprint"
                | "recoveryPolicy"
                | "confirmedDestructive"
                | "useTransaction"
                | "requireRollback"
                | "profileRevisionDigest"
        );
        let monitor_field = key == "monitorOccurrence" && kind == "monitorOccurrence";
        if !common_field && !monitor_field {
            return Err(PortError::BackendUnavailable(
                "job plan contains a field outside the durable projection".into(),
            ));
        }
        validate_plan_value(kind, key, value)?;
        projected.insert(key.clone(), value.clone());
    }
    let projected = Value::Object(projected);
    check_plan_size(&projected)?;
    Ok(projected)
}

fn check_plan_size(projected: &Value) -> Result<(), PortError> {
    let bytes = serde_json::to_vec(projected)
        .map_err(|_| PortError::BackendUnavailable("job plan could not be encoded".into()))?;
    if bytes.len() > MAX_DURABLE_JOB_PLAN_BYTES {
        return Err(PortError::QuotaExceeded(
            "job plan exceeds the local size limit".into(),
        ));
    }
    Ok(())
}

fn validate_plan_value(kind: &str, key: &str, value: &Value) -> Result<(), PortError> {
    let valid = match key {
        "kind" => value.as_str().is_some_and(safe_identifier),
        "consumedPlanId" | "planId" => value.as_str().is_some_and(safe_identifier),
        "planDigest" => value.as_str().is_some_and(|text| safe_digest(text)),
        "profileRevisionDigest" => value.as_str().is_some_and(safe_digest),
        "targetBeforeFingerprint" => value.as_str().is_some_and(safe_digest),
        "recoveryPolicy" => value.as_str().is_some_and(safe_identifier),
        "planVersion" | "handlerVersion" | "checkpointVersion" | "selectionRevision" => {
            value.as_u64().is_some()
        }
        "confirmedDestructive" | "useTransaction" | "requireRollback" => value.as_bool().is_some(),
        "sourceTables" => value.as_array().is_some_and(|items| {
            items.len() <= 100_000
                && items.iter().all(|item| {
                    item.as_str()
                        .is_some_and(|text| !text.is_empty() && text.len() <= 4096)
                })
        }),
        "recoveryTargets" => validate_recovery_targets(value),
        "monitorOccurrence" => kind == "monitorOccurrence" && validate_monitor_occurrence(value),
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

fn validate_monitor_occurrence(value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    const FIELDS: &[&str] = &[
        "scheduleId",
        "definitionRevision",
        "scheduledAt",
        "refreshIntervalSeconds",
        "dashboardId",
        "widgetId",
        "workflowId",
        "workflowRevision",
        "grantId",
        "grantVersion",
        "scopes",
    ];
    if !has_exact_keys(object, FIELDS)
        || !bounded_text(&object["scheduleId"], 1_536)
        || !safe_digest_text(&object["definitionRevision"])
        || object["scheduledAt"].as_i64().is_none()
        || !object["refreshIntervalSeconds"]
            .as_u64()
            .is_some_and(|seconds| (1..=31_536_000).contains(&seconds))
        || !bounded_text(&object["dashboardId"], 512)
        || !bounded_text(&object["widgetId"], 512)
        || !bounded_text(&object["workflowId"], 512)
        || !monitor_workflow_revision(&object["workflowRevision"])
        || !bounded_text(&object["grantId"], 512)
        || !object["grantVersion"]
            .as_u64()
            .is_some_and(|version| version > 0)
    {
        return false;
    }
    let expected_schedule_id = format!(
        "monitor:{}:{}",
        object["dashboardId"].as_str().unwrap_or_default(),
        object["widgetId"].as_str().unwrap_or_default()
    );
    if object["scheduleId"].as_str() != Some(expected_schedule_id.as_str()) {
        return false;
    }
    let scheduled_at = object["scheduledAt"].as_i64().unwrap_or_default();
    let refresh_interval_seconds = object["refreshIntervalSeconds"]
        .as_u64()
        .unwrap_or_default();
    let Ok(refresh_interval_seconds) = i64::try_from(refresh_interval_seconds) else {
        return false;
    };
    if scheduled_at.rem_euclid(refresh_interval_seconds) != 0 {
        return false;
    }
    let Some(scopes) = object["scopes"].as_array() else {
        return false;
    };
    !scopes.is_empty()
        && scopes.len() <= 128
        && scopes.iter().all(validate_monitor_scope)
        && monitor_definition_revision(object, scopes, refresh_interval_seconds)
            .is_some_and(|revision| object["definitionRevision"].as_str() == Some(&revision))
}

// Production grants::revision stores an untagged SHA256. Keep the previous
// tagged durable form readable, while accepting only an exact digest for grants.
fn monitor_workflow_revision(value: &Value) -> bool {
    value.as_str().is_some_and(|text| {
        (text.len() == 64 && text.bytes().all(|byte| byte.is_ascii_hexdigit())) || safe_digest(text)
    })
}

fn monitor_definition_revision(
    occurrence: &serde_json::Map<String, Value>,
    scopes: &[Value],
    refresh_interval_seconds: i64,
) -> Option<String> {
    let mut scopes = scopes.to_vec();
    scopes.sort_by(|left, right| monitor_scope_sort_key(left).cmp(&monitor_scope_sort_key(right)));
    let identity = json!({
        "dashboardId": occurrence["dashboardId"],
        "widgetId": occurrence["widgetId"],
        "workflowId": occurrence["workflowId"],
        "workflowRevision": occurrence["workflowRevision"],
        "grantId": occurrence["grantId"],
        "grantVersion": occurrence["grantVersion"],
        "refreshIntervalSeconds": refresh_interval_seconds,
        "scopes": scopes,
    });
    let bytes = serde_json::to_vec(&identity).ok()?;
    Some(format!("sha256:{:x}", Sha256::digest(bytes)))
}

fn monitor_scope_sort_key(scope: &Value) -> (String, String, String, u64) {
    let target = &scope["target"];
    let namespace = &target["namespace"];
    let mut namespace_parts = vec![
        namespace["database"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        namespace["catalog"].as_str().unwrap_or_default().to_owned(),
        namespace["schema"].as_str().unwrap_or_default().to_owned(),
    ];
    namespace_parts.extend(
        namespace["path"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned),
    );
    (
        target["connectionId"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        namespace_parts.join("/"),
        scope["command"].as_str().unwrap_or_default().to_owned(),
        scope["configRevision"].as_u64().unwrap_or_default(),
    )
}

fn validate_monitor_scope(value: &Value) -> bool {
    let Some(scope) = value.as_object() else {
        return false;
    };
    if !has_exact_keys(scope, &["target", "configRevision", "command"])
        || !scope["configRevision"]
            .as_u64()
            .is_some_and(|revision| revision > 0)
        || scope["command"].as_str() != Some("query")
    {
        return false;
    }
    let Some(target) = scope["target"].as_object() else {
        return false;
    };
    if !has_exact_keys(target, &["connectionId", "namespace", "object"])
        || !bounded_text(&target["connectionId"], 512)
    {
        return false;
    }
    if !target["object"].is_null() {
        return false;
    }
    let Some(namespace) = target["namespace"].as_object() else {
        return false;
    };
    const NAMESPACE_FIELDS: &[&str] = &["database", "catalog", "schema", "path"];
    if !has_exact_keys(namespace, NAMESPACE_FIELDS) {
        return false;
    }
    let name_is_valid = |value: &Value| value.is_null() || bounded_text(value, 4_096);
    if !name_is_valid(&namespace["database"])
        || !name_is_valid(&namespace["catalog"])
        || !name_is_valid(&namespace["schema"])
    {
        return false;
    }
    let Some(path) = namespace["path"].as_array() else {
        return false;
    };
    path.len() <= 32 && path.iter().all(|item| bounded_text(item, 4_096))
}

fn has_exact_keys(object: &serde_json::Map<String, Value>, expected: &[&str]) -> bool {
    object.len() == expected.len() && expected.iter().all(|key| object.contains_key(*key))
}

fn bounded_text(value: &Value, max_bytes: usize) -> bool {
    value.as_str().is_some_and(|text| {
        !text.is_empty() && text.len() <= max_bytes && !text.chars().any(char::is_control)
    })
}

fn safe_digest_text(value: &Value) -> bool {
    value.as_str().is_some_and(|text| safe_digest(text))
}

fn safe_digest(text: &str) -> bool {
    text.len() <= 71
        && text.len() > 7
        && text.starts_with("sha256:")
        && text[7..].bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn validate_recovery_targets(value: &Value) -> bool {
    let Some(targets) = value.as_array() else {
        return false;
    };
    targets.len() <= 128
        && targets.iter().all(|target| {
            let Some(object) = target.as_object() else {
                return false;
            };
            object.len() == 2
                && object.contains_key("connectionId")
                && object.contains_key("objectIds")
                && object
                    .get("connectionId")
                    .and_then(Value::as_str)
                    .is_some_and(safe_identifier)
                && object.get("objectIds").is_some_and(|ids| {
                    ids.as_array().is_some_and(|items| {
                        items.len() <= 10_000
                            && items
                                .iter()
                                .all(|item| item.as_str().is_some_and(safe_identifier))
                    })
                })
        })
}

pub(super) fn safe_identifier(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 512
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:-".contains(&byte))
}

#[cfg(test)]
mod monitor_occurrence_tests {
    use super::durable_plan;
    use serde_json::json;
    use sha2::{Digest, Sha256};

    fn payload() -> serde_json::Value {
        let workflow_revision = crate::monitor::grants::revision("id: actual-monitor-workflow");
        let scopes = json!([{
            "target": {
                "connectionId": "connection-1",
                "namespace": {
                    "database": "db",
                    "catalog": null,
                    "schema": "public",
                    "path": []
                },
                "object": null
            },
            "configRevision": 1,
            "command": "query"
        }]);
        let identity = json!({
            "dashboardId": "dashboard-1",
            "widgetId": "widget-1",
            "workflowId": "workflow-1",
            "workflowRevision": workflow_revision,
            "grantId": "grant-1",
            "grantVersion": 1,
            "refreshIntervalSeconds": 60,
            "scopes": scopes,
        });
        let definition_revision = format!(
            "sha256:{:x}",
            Sha256::digest(serde_json::to_vec(&identity).expect("revision identity encodes"))
        );
        json!({
            "planVersion": 1,
            "handlerVersion": 1,
            "checkpointVersion": 1,
            "monitorOccurrence": {
                "scheduleId": "monitor:dashboard-1:widget-1",
                "definitionRevision": definition_revision,
                "scheduledAt": 1_800,
                "refreshIntervalSeconds": 60,
                "dashboardId": "dashboard-1",
                "widgetId": "widget-1",
                "workflowId": "workflow-1",
                "workflowRevision": workflow_revision,
                "grantId": "grant-1",
                "grantVersion": 1,
                "scopes": scopes
            }
        })
    }

    #[test]
    fn workflow_revision_accepts_the_production_digest_without_relaxing_other_text() {
        let actual = crate::monitor::grants::revision("actual persisted workflow");
        assert!(super::monitor_workflow_revision(&json!(actual)));
        assert!(super::monitor_workflow_revision(&json!(format!(
            "sha256:{}",
            "a".repeat(64)
        ))));
        for invalid in [
            "a".repeat(63),
            "a".repeat(65),
            "g".repeat(64),
            "workflow-name".into(),
        ] {
            assert!(!super::monitor_workflow_revision(&json!(invalid)));
        }
    }

    #[test]
    fn monitor_occurrence_is_included_in_a_strict_durable_projection() {
        let plan = durable_plan("monitorOccurrence", &payload()).expect("monitor plan admitted");
        assert_eq!(
            plan["monitorOccurrence"]["scheduleId"],
            "monitor:dashboard-1:widget-1"
        );
    }

    #[test]
    fn monitor_occurrence_rejects_unknown_fields_and_wrong_job_kind() {
        let mut with_private_field = payload();
        with_private_field["monitorOccurrence"]["password"] = json!("must-not-persist");
        assert!(durable_plan("monitorOccurrence", &with_private_field).is_err());

        assert!(durable_plan("schemaDiffApply", &payload()).is_err());
    }

    #[test]
    fn monitor_occurrence_rejects_inconsistent_revision_and_schedule_time() {
        let mut changed_definition = payload();
        changed_definition["monitorOccurrence"]["workflowRevision"] =
            json!(format!("sha256:{}", "c".repeat(64)));
        assert!(durable_plan("monitorOccurrence", &changed_definition).is_err());

        let mut misaligned_timestamp = payload();
        misaligned_timestamp["monitorOccurrence"]["scheduledAt"] = json!(1_801);
        assert!(durable_plan("monitorOccurrence", &misaligned_timestamp).is_err());
    }

    #[test]
    fn monitor_occurrence_requires_identity_projection() {
        let mut missing = payload();
        missing
            .as_object_mut()
            .expect("payload object")
            .remove("monitorOccurrence");
        assert!(durable_plan("monitorOccurrence", &missing).is_err());
    }
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
