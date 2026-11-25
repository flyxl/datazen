//! The Data Sync `MethodMap` slice: one variant per domain method,
//! the payload shapers, and the option/selection VMs. Every payload
//! is key-for-key with the React wire (JS `undefined`-valued keys are
//! dropped by `JSON.stringify`, so "absent" is the wire form the
//! shapers reproduce — the `schema_diff::wire` convention).

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::commands::ai::workflow::{AiSubmissionTokenVm, with_submission_token};

/// The Data Sync `MethodMap` slice — one variant per domain method;
/// `wire_name` is the transport call name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncMethod {
    ClassifyDataSyncPair,
    GetSyncTasks,
    GetSyncProfiles,
    SaveSyncProfile,
    DeleteSyncProfile,
    DeleteSyncTask,
    CheckSyncConflicts,
    ExecuteDataSync,
    CancelDataSync,
    StartDataSyncPrepareJob,
    GetDataSyncJob,
    GetDataSyncJobPreview,
    StartDataSyncApplyJob,
    GetDataSyncJobDetails,
    ListDataSyncJobs,
    VerifyDataSyncRecovery,
    InspectDataSync,
    GenerateDataSyncSql,
}

impl SyncMethod {
    /// The transport call name (`transport.call(name, payload)`).
    pub fn wire_name(self) -> &'static str {
        match self {
            Self::ClassifyDataSyncPair => "classifyDataSyncPair",
            Self::GetSyncTasks => "getSyncTasks",
            Self::GetSyncProfiles => "getSyncProfiles",
            Self::SaveSyncProfile => "saveSyncProfile",
            Self::DeleteSyncProfile => "deleteSyncProfile",
            Self::DeleteSyncTask => "deleteSyncTask",
            Self::CheckSyncConflicts => "checkSyncConflicts",
            Self::ExecuteDataSync => "executeDataSync",
            Self::CancelDataSync => "cancelDataSync",
            Self::StartDataSyncPrepareJob => "startDataSyncPrepareJob",
            Self::GetDataSyncJob => "getDataSyncJob",
            Self::GetDataSyncJobPreview => "getDataSyncJobPreview",
            Self::StartDataSyncApplyJob => "startDataSyncApplyJob",
            Self::GetDataSyncJobDetails => "getDataSyncJobDetails",
            Self::ListDataSyncJobs => "listDataSyncJobs",
            Self::VerifyDataSyncRecovery => "verifyDataSyncRecovery",
            Self::InspectDataSync => "inspectDataSync",
            Self::GenerateDataSyncSql => "generateDataSyncSql",
        }
    }
}

/// `SyncOptions` (the execution options; `matchingStrategy` /
/// `largeValueMode` / `conflictPolicy` keep their wire spellings as
/// strings — the server owns the closed vocabularies).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncOptionsVm {
    pub insert: bool,
    pub update: bool,
    pub delete: bool,
    pub matching_strategy: String,
    pub batch_size: u64,
    pub large_value_mode: String,
    pub conflict_policy: String,
}

impl SyncOptionsVm {
    /// `DEFAULT_SYNC_OPTIONS`, the literal's fields verbatim.
    pub fn default_options() -> Self {
        Self {
            insert: true,
            update: true,
            delete: false,
            matching_strategy: "primaryKey".to_owned(),
            batch_size: 1000,
            large_value_mode: "full".to_owned(),
            conflict_policy: "abort".to_owned(),
        }
    }
}

/// `DataSyncSelectedRow` — one row of a reviewed selection.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataSyncSelectedRowVm {
    pub source_table: String,
    pub target_table: String,
    /// `INSERT` / `UPDATE` / `DELETE` (the server's vocabulary).
    pub operation: String,
    /// The row's key (an opaque value array).
    pub key: Vec<Value>,
}

/// `classifyDataSyncPair(sourceDatabaseType, targetDatabaseType)`.
pub fn classify_data_sync_pair_request(
    source_database_type: &str,
    target_database_type: &str,
) -> (SyncMethod, Value) {
    (
        SyncMethod::ClassifyDataSyncPair,
        json!({
            "sourceDatabaseType": source_database_type,
            "targetDatabaseType": target_database_type,
        }),
    )
}

/// `getSyncTasks()`.
pub fn get_sync_tasks_request() -> (SyncMethod, Value) {
    (SyncMethod::GetSyncTasks, json!({}))
}

/// `getSyncProfiles()`.
pub fn get_sync_profiles_request() -> (SyncMethod, Value) {
    (SyncMethod::GetSyncProfiles, json!({}))
}

/// `saveSyncProfile(profile)` — the body-carrying write face (the
/// `saveUserAsset` submission token rides the request).
pub fn save_sync_profile_request(
    profile: &Value,
    token: &AiSubmissionTokenVm,
) -> (SyncMethod, Value) {
    (
        SyncMethod::SaveSyncProfile,
        with_submission_token(json!({ "profile": profile }), token),
    )
}

/// `deleteSyncProfile(profileId)`.
pub fn delete_sync_profile_request(profile_id: &str) -> (SyncMethod, Value) {
    (
        SyncMethod::DeleteSyncProfile,
        json!({ "profileId": profile_id }),
    )
}

/// `deleteSyncTask(taskId)`.
pub fn delete_sync_task_request(task_id: &str) -> (SyncMethod, Value) {
    (SyncMethod::DeleteSyncTask, json!({ "taskId": task_id }))
}

/// `checkSyncConflicts(taskId)`.
pub fn check_sync_conflicts_request(task_id: &str) -> (SyncMethod, Value) {
    (
        SyncMethod::CheckSyncConflicts,
        json!({ "taskId": task_id }),
    )
}

/// The `DataSyncRunRequest` payload (executeDataSync's request body).
pub fn data_sync_run_request(
    plan_id: &str,
    selection: &Value,
    options: &SyncOptionsVm,
    job_id: Option<&str>,
) -> Value {
    // `jobId: jobId ?? null` — the null spelling is the wire form.
    json!({
        "planId": plan_id,
        "selection": selection,
        "options": options,
        "jobId": job_id,
    })
}

/// `executeDataSync({ request, ...(profile ? { profile } : {}) })` —
/// the profile spread is an ABSENT key without one.
pub fn execute_data_sync_request(
    request: &Value,
    profile: Option<&Value>,
) -> (SyncMethod, Value) {
    let mut payload = json!({ "request": request });
    if let Some(profile) = profile {
        payload["profile"] = profile.clone();
    }
    (SyncMethod::ExecuteDataSync, payload)
}

/// `cancelDataSync({ jobId }, token)` — the second body-carrying
/// write face.
pub fn cancel_data_sync_request(
    job_id: &str,
    token: &AiSubmissionTokenVm,
) -> (SyncMethod, Value) {
    (
        SyncMethod::CancelDataSync,
        with_submission_token(json!({ "jobId": job_id }), token),
    )
}

/// `startDataSyncPrepareJob({ … })` — the accepted-receipt arm of the
/// compare chain. `tables`/`options`/`filters` (and the database /
/// schema qualifiers) are the explicit-null wire spellings.
pub fn start_data_sync_prepare_job_request(
    source_db_session_id: &str,
    target_db_session_id: &str,
    tables: Option<&[String]>,
    job_id: &str,
    source_database: Option<&str>,
    target_database: Option<&str>,
    source_schema: Option<&str>,
    target_schema: Option<&str>,
    options: Option<&SyncOptionsVm>,
    filters: Option<&Value>,
) -> (SyncMethod, Value) {
    (
        SyncMethod::StartDataSyncPrepareJob,
        json!({
            "sourceDbSessionId": source_db_session_id,
            "targetDbSessionId": target_db_session_id,
            "tables": tables,
            "jobId": job_id,
            "sourceDatabase": source_database,
            "targetDatabase": target_database,
            "sourceSchema": source_schema,
            "targetSchema": target_schema,
            "options": options,
            "filters": filters,
        }),
    )
}

/// `getDataSyncJob({ jobId })` — the poll read.
pub fn get_data_sync_job_request(job_id: &str) -> (SyncMethod, Value) {
    (SyncMethod::GetDataSyncJob, json!({ "jobId": job_id }))
}

/// `getDataSyncJobPreview({ jobId })` — the reviewed-plan read.
pub fn get_data_sync_job_preview_request(job_id: &str) -> (SyncMethod, Value) {
    (
        SyncMethod::GetDataSyncJobPreview,
        json!({ "jobId": job_id }),
    )
}

/// `startDataSyncApplyJob({ sourceDbSessionId, targetDbSessionId,
/// request })` — the durable apply's accepted receipt.
pub fn start_data_sync_apply_job_request(
    source_db_session_id: &str,
    target_db_session_id: &str,
    request: &Value,
) -> (SyncMethod, Value) {
    (
        SyncMethod::StartDataSyncApplyJob,
        json!({
            "sourceDbSessionId": source_db_session_id,
            "targetDbSessionId": target_db_session_id,
            "request": request,
        }),
    )
}

/// `getDataSyncJobDetails({ jobId })`.
pub fn get_data_sync_job_details_request(job_id: &str) -> (SyncMethod, Value) {
    (
        SyncMethod::GetDataSyncJobDetails,
        json!({ "jobId": job_id }),
    )
}

/// `listDataSyncJobs()`.
pub fn list_data_sync_jobs_request() -> (SyncMethod, Value) {
    (SyncMethod::ListDataSyncJobs, json!({}))
}

/// `verifyDataSyncRecovery(request)`.
pub fn verify_data_sync_recovery_request(request: &Value) -> (SyncMethod, Value) {
    (SyncMethod::VerifyDataSyncRecovery, request.clone())
}

/// `inspectDataSync({ … })` — the mapping inspection (the `tables`
/// spread is an ABSENT key without mappings).
pub fn inspect_data_sync_request(
    source_db_session_id: &str,
    target_db_session_id: &str,
    source_database: Option<&str>,
    target_database: Option<&str>,
    source_schema: Option<&str>,
    target_schema: Option<&str>,
    mappings: Option<&Value>,
) -> (SyncMethod, Value) {
    let mut payload = json!({
        "sourceDbSessionId": source_db_session_id,
        "targetDbSessionId": target_db_session_id,
        "sourceDatabase": source_database,
        "targetDatabase": target_database,
        "sourceSchema": source_schema,
        "targetSchema": target_schema,
    });
    if let Some(mappings) = mappings {
        payload["tables"] = mappings.clone();
    }
    (SyncMethod::InspectDataSync, payload)
}

/// `generateDataSyncSql({ planId, selection, options })` — only the
/// selected rows; failures never trigger another write path.
pub fn generate_data_sync_sql_request(
    plan_id: &str,
    selection: &Value,
    options: &SyncOptionsVm,
) -> (SyncMethod, Value) {
    (
        SyncMethod::GenerateDataSyncSql,
        json!({
            "planId": plan_id,
            "selection": selection,
            "options": options,
        }),
    )
}

/// A legacy-rail command (the `legacyInvoke` calls): the method name
/// and the payload as the React file sends them.
#[derive(Clone, Debug, PartialEq)]
pub struct SyncLegacyRequest {
    /// The legacy command name.
    pub command: &'static str,
    /// The payload object.
    pub payload: Value,
}

/// `get_data_sync_comparison_page` — the plan_store row-level paged
/// read. Contract §6.16's dataSync domain has NO method for it (the
/// `getDataSyncJobPreview` summary projection is not a page), so the
/// command stays on the desktop legacy rail; the web form keeps
/// declaring it a capability gap via `unsupportedMethods`.
///
/// `limit ?? activeComparisonPlan.pageSize ?? 100`.
pub fn get_data_sync_comparison_page_legacy_request(
    plan_id: &str,
    source_table: &str,
    target_table: &str,
    cursor: Option<&str>,
    limit: u64,
) -> SyncLegacyRequest {
    SyncLegacyRequest {
        command: "get_data_sync_comparison_page",
        payload: json!({
            "request": {
                "planId": plan_id,
                "sourceTable": source_table,
                "targetTable": target_table,
                "cursor": cursor,
                "limit": limit,
            }
        }),
    }
}

/// `apply_data_sync` — the desktop legacy old-shape apply (no
/// reviewed plan). The desktop command itself is a PERMANENTLY
/// REFUSED guide stub ("legacy apply has no reviewed plan; compare
/// and submit the reviewed selection as a durable Data Sync job");
/// the command stays on the legacy rail for signature compatibility
/// only — every call gets that refusal.
pub fn apply_data_sync_legacy_request(
    source_db_session_id: &str,
    target_db_session_id: &str,
    tables: &[String],
    job_id: Option<&str>,
    source_database: Option<&str>,
    target_database: Option<&str>,
    source_schema: Option<&str>,
    target_schema: Option<&str>,
    options: Option<&SyncOptionsVm>,
) -> SyncLegacyRequest {
    SyncLegacyRequest {
        command: "apply_data_sync",
        payload: json!({
            "sourceDbSessionId": source_db_session_id,
            "targetDbSessionId": target_db_session_id,
            "tables": tables,
            "jobId": job_id,
            "sourceDatabase": source_database,
            "targetDatabase": target_database,
            "sourceSchema": source_schema,
            "targetSchema": target_schema,
            "options": options,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token() -> AiSubmissionTokenVm {
        AiSubmissionTokenVm {
            idempotency_key: "idem-1".to_owned(),
            expires_at: "2026-01-01T00:00:00Z".to_owned(),
        }
    }

    #[test]
    fn the_wire_names_are_the_transport_calls() {
        assert_eq!(SyncMethod::ClassifyDataSyncPair.wire_name(), "classifyDataSyncPair");
        assert_eq!(SyncMethod::StartDataSyncPrepareJob.wire_name(), "startDataSyncPrepareJob");
        assert_eq!(SyncMethod::GetDataSyncJobPreview.wire_name(), "getDataSyncJobPreview");
        assert_eq!(SyncMethod::StartDataSyncApplyJob.wire_name(), "startDataSyncApplyJob");
        assert_eq!(SyncMethod::ExecuteDataSync.wire_name(), "executeDataSync");
        assert_eq!(SyncMethod::GenerateDataSyncSql.wire_name(), "generateDataSyncSql");
        assert_eq!(SyncMethod::VerifyDataSyncRecovery.wire_name(), "verifyDataSyncRecovery");
    }

    #[test]
    fn default_options_are_the_react_literal() {
        let options = SyncOptionsVm::default_options();
        assert!(options.insert);
        assert!(options.update);
        assert!(!options.delete);
        assert_eq!(options.matching_strategy, "primaryKey");
        assert_eq!(options.batch_size, 1000);
        assert_eq!(options.large_value_mode, "full");
        assert_eq!(options.conflict_policy, "abort");
        // The wire spelling is camelCase.
        let wire = serde_json::to_value(&options).unwrap();
        assert_eq!(wire["matchingStrategy"], "primaryKey");
        assert_eq!(wire["largeValueMode"], "full");
        assert_eq!(wire["conflictPolicy"], "abort");
    }

    #[test]
    fn the_write_faces_carry_the_submission_token() {
        let (method, payload) = save_sync_profile_request(
            &serde_json::json!({ "id": "p1" }),
            &token(),
        );
        assert_eq!(method, SyncMethod::SaveSyncProfile);
        assert_eq!(payload["profile"]["id"], "p1");
        assert_eq!(payload["submissionToken"]["idempotencyKey"], "idem-1");

        let (method, payload) = cancel_data_sync_request("job-7", &token());
        assert_eq!(method, SyncMethod::CancelDataSync);
        assert_eq!(payload["jobId"], "job-7");
        assert!(payload.get("submissionToken").is_some());
    }

    #[test]
    fn the_profile_spread_is_an_absent_key_without_one() {
        let request = serde_json::json!({ "planId": "plan-1" });
        let (_, with) = execute_data_sync_request(&request, Some(&serde_json::json!({ "id": "p", "revision": "r" })));
        assert_eq!(with["profile"]["id"], "p");
        let (_, without) = execute_data_sync_request(&request, None);
        assert!(without.get("profile").is_none(), "no profile key without one");
    }

    #[test]
    fn the_run_request_spells_the_null_job_id() {
        let options = SyncOptionsVm::default_options();
        let request = data_sync_run_request("plan-1", &serde_json::json!({}), &options, None);
        assert_eq!(request["jobId"], serde_json::Value::Null);
        let request = data_sync_run_request("plan-1", &serde_json::json!({}), &options, Some("job-9"));
        assert_eq!(request["jobId"], "job-9");
    }

    #[test]
    fn the_legacy_faces_keep_their_payload_shapes() {
        let page = get_data_sync_comparison_page_legacy_request(
            "plan-1", "src", "tgt", Some("cursor-2"), 50,
        );
        assert_eq!(page.command, "get_data_sync_comparison_page");
        assert_eq!(page.payload["request"]["planId"], "plan-1");
        assert_eq!(page.payload["request"]["sourceTable"], "src");
        assert_eq!(page.payload["request"]["targetTable"], "tgt");
        assert_eq!(page.payload["request"]["cursor"], "cursor-2");
        assert_eq!(page.payload["request"]["limit"], 50);

        let apply = apply_data_sync_legacy_request(
            "s1", "t1", &["a".to_owned(), "b".to_owned()], None, None, None, None, None, None,
        );
        assert_eq!(apply.command, "apply_data_sync");
        assert_eq!(apply.payload["tables"][0], "a");
        assert_eq!(apply.payload["jobId"], serde_json::Value::Null);
        assert_eq!(apply.payload["options"], serde_json::Value::Null);
    }

    #[test]
    fn the_inspect_spread_is_an_absent_key_without_mappings() {
        let (_, with) = inspect_data_sync_request(
            "s1", "t1", None, None, None, None, Some(&serde_json::json!([{ "source": "a" }])),
        );
        assert_eq!(with["tables"][0]["source"], "a");
        let (_, without) = inspect_data_sync_request("s1", "t1", None, None, None, None, None);
        assert!(without.get("tables").is_none());
    }
}
