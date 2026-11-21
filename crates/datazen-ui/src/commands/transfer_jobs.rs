//! Port of `packages/frontend/src/commands/transferJobs.ts` (T424) —
//! the Job command layer for Data Transfer: the caller for the
//! prepare/apply split the backend Job chain introduces.
//!
//! This file is the **seam** the other two migration tools copy
//! rather than reinvent: `prepare` admits and freezes a plan, `apply`
//! spends it exactly once, and the reply is a Job verdict
//! (`CommitBoundary` + derived `EffectOutcome`) instead of the old
//! flat success/failure boolean. Every request goes through exactly
//! one manager end to end; the legacy `transferCommands.execute`
//! stays exported for the rollback path but the window no longer
//! calls it.
//!
//! ## What maps where
//!
//! - `LOCAL_BACKEND_SCOPE` is the literal (`local-desktop-backend`),
//!   mirroring the desktop's own `LOCAL_BACKEND_SCOPE`. Both
//!   endpoints must present it — no default, because "I forgot to
//!   declare a scope" is precisely the input the rule exists to
//!   reject.
//! - `localBackendScope()` is [`local_backend_scope`]: the same
//!   object on BOTH the database and SQL-file branches (a SQL-file
//!   run carries no target connection and the backend skips the
//!   target check — but the field is mandatory on the wire; one code
//!   path means the SQL-file branch cannot drift into an
//!   under-specified request).
//! - The six `invoke` wrappers are [`TransferJobMethod`] + the
//!   `*_request` shapers.
//! - `parseJobView` (backend-client) is the host's decode; the
//!   `getDetails` wrapper's `{ ...details, job: parseJobView(...) }`
//!   restitch is documented as the host seam (the details VM's `job`
//!   field carries the parsed view).
//! - The `VITE_E2E` capture window (`__dataTransferJobCalls`) is the
//!   E2E harness's global — no Rust counterpart; the adapter
//!   observes the calls (the same delta `commands/transfer.rs`
//!   records).
//!
//! Known deltas: `JobState` / `JobProgress` reuse the
//! `data_transfer_attached_jobs` vocabulary (the P5 port's own
//! types); `CommitBoundary` / `JobRecoveryVerdict` /
//! `EffectOutcome` ride the apply view as their JSON wire (the
//! migration-verdict engine `lib/migrationJobVerdict.ts` is its own
//! task) with the string spellings kept.

// No production consumer yet (the data-transfer window track wires
// the faces); this crate's tests are the callers meanwhile.
#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::commands::transfer::{TransferJob, TransferPreview};
use crate::views::data_transfer::data_transfer_attached_jobs::JobState;

/// `LOCAL_BACKEND_SCOPE` — the scope string the local desktop
/// backend declares (mirrors the desktop's own constant).
pub(crate) const LOCAL_BACKEND_SCOPE: &str = "local-desktop-backend";

/// `TransferBackendScope`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TransferBackendScope {
    pub source_backend_scope: String,
    pub target_backend_scope: String,
    /// Profile scopes, if any. The backend rejects a foreign entry
    /// here, so the local window sends an empty list rather than
    /// inventing one.
    pub profile_backend_scopes: Vec<String>,
}

/// `localBackendScope()` — the scope every migration tool in this
/// window declares, the same object on both branches.
pub(crate) fn local_backend_scope() -> TransferBackendScope {
    TransferBackendScope {
        source_backend_scope: LOCAL_BACKEND_SCOPE.to_owned(),
        target_backend_scope: LOCAL_BACKEND_SCOPE.to_owned(),
        profile_backend_scopes: Vec::new(),
    }
}

/// `TransferPrepareJobRequest`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TransferPrepareJobRequest {
    pub job: TransferJob,
    pub backend_scope: TransferBackendScope,
    /// Replaying the same key returns the recorded prepare Job.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
}

/// `TransferApplyJobRequest`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TransferApplyJobRequest {
    pub plan_id: String,
    pub plan_digest: String,
    pub selection_revision: u64,
    pub selection: TransferApplySelection,
    pub confirmed_destructive: bool,
    pub backend_scope: TransferBackendScope,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
}

/// `selection: { sourceTables?: string[] | null }`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TransferApplySelection {
    pub source_tables: Option<Vec<String>>,
}

/// `TransferPrepareJobView` — the admitted, frozen plan's receipt.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TransferPrepareJobView {
    pub job_id: String,
    pub kind: String,
    pub state: String,
    /// The derived `EffectOutcome` (the wire's string vocabulary).
    pub effect_outcome: String,
    pub plan_id: String,
    pub plan_digest: String,
    pub plan_version: u32,
    pub handler_version: u32,
    pub checkpoint_version: u32,
    pub selection_revision: u64,
    pub expires_at: String,
    pub can_execute: bool,
    pub block_reason: Option<String>,
    pub review: TransferPreview,
}

/// `TransferApplyJobView` — the spent plan's Job verdict.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TransferApplyJobView {
    pub job_id: String,
    pub kind: String,
    pub state: String,
    pub effect_outcome: String,
    /// The P5 five-bucket progress (the wire's JSON).
    pub progress: Value,
    pub plan_id: String,
    pub plan_digest: String,
    pub selection_revision: u64,
    /// Durable write markers; the only trustworthy record of what
    /// landed (the wire's JSON — the verdict engine's own task).
    pub commit_boundaries: Vec<Value>,
    /// SQL-file runs return `transfer-sql-<sha256hex>` here.
    pub artifact_ids: Vec<String>,
    /// Cancel intent. Never equivalent to "cancelled".
    pub cancelled: bool,
    pub partial: bool,
    /// True when this reply came from the idempotent receipt, not a
    /// fresh write.
    pub replayed: bool,
    pub error: Option<String>,
    pub recovery_verdict: Option<String>,
    pub recovery_resume_through: Option<u64>,
    pub recovery_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<Value>,
}

/// The Job-path legacy `invoke` commands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TransferJobMethod {
    Prepare,
    Apply,
    GetJob,
    GetDetails,
    ListJobs,
    Cancel,
}

impl TransferJobMethod {
    /// The legacy command name.
    pub(crate) fn wire_name(self) -> &'static str {
        match self {
            Self::Prepare => "prepare_data_transfer_job",
            Self::Apply => "apply_data_transfer_job",
            Self::GetJob => "get_job",
            Self::GetDetails => "get_transfer_job_details",
            Self::ListJobs => "list_jobs",
            Self::Cancel => "cancel_data_transfer",
        }
    }
}

/// `prepare(request)` — admits the plan, freezes evidence, returns
/// the plan digest to spend.
pub(crate) fn prepare_request(request: &TransferPrepareJobRequest) -> (TransferJobMethod, Value) {
    (
        TransferJobMethod::Prepare,
        json!({ "request": request }),
    )
}

/// `apply(request)` — one planId, one Job, one idempotent write
/// attempt.
pub(crate) fn apply_request(request: &TransferApplyJobRequest) -> (TransferJobMethod, Value) {
    (TransferJobMethod::Apply, json!({ "request": request }))
}

/// `getJob(jobId)` — the live P5 Job projection after an accepted
/// apply receipt.
pub(crate) fn get_job_request(job_id: &str) -> (TransferJobMethod, Value) {
    (TransferJobMethod::GetJob, json!({ "jobId": job_id }))
}

/// `getDetails(jobId)` — durable terminal evidence after a window
/// reopens. The `{ ...details, job: parseJobView(details.job) }`
/// restitch is the host's decode seam (the details' `job` field
/// carries the parsed view).
pub(crate) fn get_details_request(job_id: &str) -> (TransferJobMethod, Value) {
    (
        TransferJobMethod::GetDetails,
        json!({ "jobId": job_id }),
    )
}

/// `listJobs(filter = {})` — Job projections for window hydration
/// and lost-receipt recovery.
pub(crate) fn list_jobs_request(filter: Option<&Value>) -> (TransferJobMethod, Value) {
    (
        TransferJobMethod::ListJobs,
        filter.cloned().unwrap_or_else(|| json!({})),
    )
}

/// `cancel(jobId)` — the cancel REQUEST; the same command the legacy
/// path used (the Job chain routes it to the Job repository first).
/// `false` means "no such Job", which the caller must surface rather
/// than treat as a successful cancel.
pub(crate) fn cancel_request(job_id: &str) -> (TransferJobMethod, Value) {
    (TransferJobMethod::Cancel, json!({ "jobId": job_id }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wire_names_are_the_job_commands() {
        assert_eq!(TransferJobMethod::Prepare.wire_name(), "prepare_data_transfer_job");
        assert_eq!(TransferJobMethod::Apply.wire_name(), "apply_data_transfer_job");
        assert_eq!(TransferJobMethod::GetJob.wire_name(), "get_job");
        assert_eq!(TransferJobMethod::GetDetails.wire_name(), "get_transfer_job_details");
        assert_eq!(TransferJobMethod::ListJobs.wire_name(), "list_jobs");
        assert_eq!(TransferJobMethod::Cancel.wire_name(), "cancel_data_transfer");
    }

    #[test]
    fn the_local_scope_is_declared_on_both_endpoints() {
        assert_eq!(LOCAL_BACKEND_SCOPE, "local-desktop-backend");
        let scope = local_backend_scope();
        assert_eq!(scope.source_backend_scope, "local-desktop-backend");
        assert_eq!(scope.target_backend_scope, "local-desktop-backend");
        assert!(scope.profile_backend_scopes.is_empty(), "no invented profile scopes");
        // The wire spelling.
        let wire = serde_json::to_value(&scope).unwrap();
        assert_eq!(wire["sourceBackendScope"], "local-desktop-backend");
        assert_eq!(wire["targetBackendScope"], "local-desktop-backend");
        assert_eq!(wire["profileBackendScopes"], json!([]));
    }

    #[test]
    fn the_prepare_request_carries_the_scope_and_the_optional_key() {
        let job = TransferJob {
            source: crate::commands::transfer::TransferEndpoint {
                db_session_id: "s1".to_owned(),
                database: "db".to_owned(),
                schema: None,
            },
            target: None,
            sql_file_target: None,
            mode: crate::commands::transfer::TransferMode::Data,
            write_mode: crate::commands::transfer::WriteMode::Insert,
            tables: Vec::new(),
            options: crate::commands::transfer::DEFAULT_TRANSFER_OPTIONS,
        };
        let request = TransferPrepareJobRequest {
            job,
            backend_scope: local_backend_scope(),
            idempotency_key: None,
        };
        let (_, payload) = prepare_request(&request);
        assert!(payload["request"]["job"]["source"]["dbSessionId"] == "s1");
        assert!(payload["request"]["backendScope"]["sourceBackendScope"] == "local-desktop-backend");
        // The optional key stays absent without one.
        assert!(payload["request"].get("idempotencyKey").is_none());
        // With one it rides.
        let mut request = request;
        request.idempotency_key = Some("idem-1".to_owned());
        let (_, payload) = prepare_request(&request);
        assert_eq!(payload["request"]["idempotencyKey"], "idem-1");
    }

    #[test]
    fn the_apply_request_spells_its_fields() {
        let request = TransferApplyJobRequest {
            plan_id: "plan-1".to_owned(),
            plan_digest: "digest".to_owned(),
            selection_revision: 3,
            selection: TransferApplySelection {
                source_tables: Some(vec!["a".to_owned()]),
            },
            confirmed_destructive: true,
            backend_scope: local_backend_scope(),
            idempotency_key: None,
        };
        let (_, payload) = apply_request(&request);
        assert_eq!(payload["request"]["planId"], "plan-1");
        assert_eq!(payload["request"]["planDigest"], "digest");
        assert_eq!(payload["request"]["selectionRevision"], 3);
        assert_eq!(payload["request"]["selection"]["sourceTables"][0], "a");
        assert_eq!(payload["request"]["confirmedDestructive"], true);
        assert_eq!(
            payload["request"]["backendScope"]["targetBackendScope"],
            "local-desktop-backend"
        );
    }

    #[test]
    fn the_views_round_trip_their_camel_case() {
        let view = TransferPrepareJobView {
            job_id: "j1".to_owned(),
            kind: "data_transfer_prepare".to_owned(),
            state: "succeeded".to_owned(),
            effect_outcome: "committed".to_owned(),
            plan_id: "plan-1".to_owned(),
            plan_digest: "digest".to_owned(),
            plan_version: 1,
            handler_version: 2,
            checkpoint_version: 3,
            selection_revision: 4,
            expires_at: "2026-01-01T00:00:00Z".to_owned(),
            can_execute: true,
            block_reason: None,
            review: TransferPreview {
                plan_id: "plan-1".to_owned(),
                pairing_path: "postgres::postgres".to_owned(),
                mode: crate::commands::transfer::TransferMode::Data,
                write_mode: crate::commands::transfer::WriteMode::Insert,
                ddl: Vec::new(),
                write_plans: Vec::new(),
                warnings: Vec::new(),
                can_execute: true,
                block_reason: None,
            },
        };
        let wire = serde_json::to_value(&view).unwrap();
        assert_eq!(wire["jobId"], "j1");
        assert_eq!(wire["effectOutcome"], "committed");
        assert_eq!(wire["planVersion"], 1);
        assert_eq!(wire["selectionRevision"], 4);
        assert_eq!(wire["review"]["planId"], "plan-1");
        assert!(wire.get("blockReason").is_none() || wire["blockReason"].is_null());
    }

    #[test]
    fn the_job_state_vocabulary_is_shared() {
        // The P5 port's own type answers the wire spellings.
        assert_eq!(JobState::from_str("succeeded"), Some(JobState::Succeeded));
        assert_eq!(JobState::from_str("running"), Some(JobState::Running));
        assert_eq!(JobState::from_str("nonsense"), None);
    }

    #[test]
    fn the_simple_shapers_carry_their_keys() {
        let (_, payload) = get_job_request("job-1");
        assert_eq!(payload["jobId"], "job-1");
        let (_, payload) = get_details_request("job-1");
        assert_eq!(payload["jobId"], "job-1");
        let (_, payload) = list_jobs_request(None);
        assert_eq!(payload, json!({}));
        let (_, payload) = list_jobs_request(Some(&json!({ "kind": "data_transfer" })));
        assert_eq!(payload["kind"], "data_transfer");
        let (_, payload) = cancel_request("job-1");
        assert_eq!(payload["jobId"], "job-1");
    }
}
