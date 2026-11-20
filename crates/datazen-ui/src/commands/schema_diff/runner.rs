//! The `schemaDiffCommands` wrappers: the `MethodMap` payload shapers and
//! the submit→poll→resolve orchestration (`submitSchemaDiffPrepare` /
//! `executeDeploy` / `waitForSchemaDiffJob`) as an effect-driven state
//! machine — the `session_cancel.rs` / `ai_store.rs` pattern: no fake
//! async, the host runs the transport and delivers receipts back.
//!
//! Every payload here is key-for-key with the React wire assertions in
//! `commands/__tests__/schemaDiff.test.ts` (JS `undefined`-valued keys are
//! dropped by `JSON.stringify`, so "absent" is the wire form the shapers
//! reproduce).

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::wire::{
    ColumnTypeOverrideVm, SchemaDiffDeployResultVm, SchemaDiffJobAcceptedVm,
    SchemaDiffJobDetailsVm, SchemaDiffPlanVm, SchemaDiffPrepareEnvelopeVm,
    denormalize_plan_to_json, parse_job_accepted,
};
use crate::views::data_transfer::data_transfer_attached_jobs::JobState;
use crate::views::schema_diff::schema_diff_object_identity::SchemaDiffObjectIdentityVm;

/// The `MethodMap` slice (`SchemaDiffMethodMap`) — one variant per domain
/// method; `wire_name` is the transport call name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SchemaDiffMethod {
    ListProfiles,
    SaveProfile,
    DeleteProfile,
    CompareTableSchemas,
    PreparePlan,
    PrepareUnifiedPlan,
    ExecuteDeploy,
    ListJobs,
    GetJobDetails,
    CancelDeploy,
    VerifyRecovery,
}

impl SchemaDiffMethod {
    /// The transport call name (`transport.call(name, payload)`).
    pub(crate) fn wire_name(self) -> &'static str {
        match self {
            Self::ListProfiles => "listSchemaDiffProfiles",
            Self::SaveProfile => "saveSchemaDiffProfile",
            Self::DeleteProfile => "deleteSchemaDiffProfile",
            Self::CompareTableSchemas => "compareTableSchemas",
            Self::PreparePlan => "prepareSchemaDiffPlan",
            Self::PrepareUnifiedPlan => "prepareSchemaUnifiedPlan",
            Self::ExecuteDeploy => "executeSchemaDiffDeploy",
            Self::ListJobs => "listSchemaDiffJobs",
            Self::GetJobDetails => "getSchemaDiffJobDetails",
            Self::CancelDeploy => "cancelSchemaDiffDeploy",
            Self::VerifyRecovery => "verifySchemaDiffJobRecovery",
        }
    }
}

/// React `SubmissionToken` — the minted write-face token; `saveUserAsset`
/// is the intent every schema-diff token call uses
/// (`issueSubmissionToken('saveUserAsset', null)`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SchemaDiffSubmissionTokenVm {
    pub(crate) idempotency_key: String,
    pub(crate) expires_at: String,
}

/// The submission intent every schema-diff token mint uses.
pub(crate) const USER_ASSET_TOKEN_INTENT: &str = "saveUserAsset";

/// `transport.call(name, { …request, submissionToken })` — the token merge
/// the domain client performs.
pub(crate) fn with_submission_token(
    mut request: serde_json::Value,
    token: &SchemaDiffSubmissionTokenVm,
) -> serde_json::Value {
    if let serde_json::Value::Object(map) = &mut request {
        map.insert(
            "submissionToken".to_owned(),
            serde_json::to_value(token).unwrap_or_default(),
        );
    }
    request
}

// ---- the single-shot payload shapers -----------------------------------------

/// `getProfiles` → `listSchemaDiffProfiles`, empty payload.
pub(crate) fn get_profiles_request() -> serde_json::Value {
    json!({})
}

/// `saveProfile` → `saveSchemaDiffProfile`, `{ profile }`.
pub(crate) fn save_profile_request(profile: &super::wire::SchemaDiffProfileVm) -> serde_json::Value {
    match serde_json::to_value(profile) {
        Ok(value) => json!({ "profile": value }),
        Err(_) => json!({ "profile": serde_json::Value::Null }),
    }
}

/// `deleteProfile` → `deleteSchemaDiffProfile`, `{ profileId }`.
pub(crate) fn delete_profile_request(profile_id: &str) -> serde_json::Value {
    json!({ "profileId": profile_id })
}

/// `compareTableSchemas`: endpoint-specific table names; the schemas ride
/// the wire ALWAYS (`sourceSchema ?? null`), null when absent.
pub(crate) fn compare_table_schemas_request(
    source_db_session_id: &str,
    target_db_session_id: &str,
    source_table_name: &str,
    target_table_name: &str,
    source_schema: Option<&str>,
    target_schema: Option<&str>,
) -> serde_json::Value {
    json!({
        "sourceDbSessionId": source_db_session_id,
        "targetDbSessionId": target_db_session_id,
        "sourceTableName": source_table_name,
        "targetTableName": target_table_name,
        "sourceSchema": source_schema,
        "targetSchema": target_schema,
    })
}

/// `getJobDetails` → `getSchemaDiffJobDetails`, `{ jobId }`.
pub(crate) fn get_job_details_request(job_id: &str) -> serde_json::Value {
    json!({ "jobId": job_id })
}

/// `listJobs` → `listSchemaDiffJobs`, `{ states: null, limit: 100 }`.
pub(crate) fn list_jobs_request() -> serde_json::Value {
    json!({ "states": null, "limit": 100 })
}

/// `cancelDeploy` → `cancelSchemaDiffDeploy`, `{ jobId }` (+token).
pub(crate) fn cancel_deploy_request(job_id: &str) -> serde_json::Value {
    json!({ "jobId": job_id })
}

/// `verifyRecovery` → `verifySchemaDiffJobRecovery`,
/// `{ jobId, targetDbSessionId }` (+token).
pub(crate) fn verify_recovery_request(job_id: &str, target_db_session_id: &str) -> serde_json::Value {
    json!({ "jobId": job_id, "targetDbSessionId": target_db_session_id })
}

// ---- the prepare / deploy parameter shapes -----------------------------------

/// `schemaDiffCommands.preparePlan`'s params.
#[derive(Clone, Debug, Default)]
pub(crate) struct PreparePlanParamsVm {
    pub(crate) source_db_session_id: String,
    pub(crate) target_db_session_id: String,
    pub(crate) table_names: Vec<String>,
    pub(crate) target_table_names: Option<Vec<String>>,
    pub(crate) target_only_table_names: Option<Vec<String>>,
    pub(crate) source_schema: Option<String>,
    pub(crate) target_schema: Option<String>,
    pub(crate) allow_destructive: bool,
    pub(crate) include_indexes: Option<bool>,
    pub(crate) type_overrides: Option<Vec<ColumnTypeOverrideVm>>,
}

/// `SchemaDiffPreparePlanRequest` on the wire. `targetOnlyTableNames` is
/// spread ONLY when non-empty (React's conditional spread in
/// `schemaDiffCommands.preparePlan`); the other optionals ride when
/// present (`JSON.stringify` drops `undefined`).
pub(crate) fn prepare_plan_request(params: &PreparePlanParamsVm) -> serde_json::Value {
    let mut request = json!({
        "sourceDbSessionId": params.source_db_session_id,
        "targetDbSessionId": params.target_db_session_id,
        "tableNames": params.table_names,
        "allowDestructive": params.allow_destructive,
    });
    // `json!` always builds an object here; a non-object would mean a
    // shaper bug, and returning the unextended request is the safe branch.
    let Some(map) = request.as_object_mut() else {
        return request;
    };
    if let Some(names) = &params.target_table_names {
        map.insert("targetTableNames".to_owned(), json!(names));
    }
    if let Some(names) = &params.target_only_table_names {
        if !names.is_empty() {
            map.insert("targetOnlyTableNames".to_owned(), json!(names));
        }
    }
    if let Some(schema) = &params.source_schema {
        map.insert("sourceSchema".to_owned(), json!(schema));
    }
    if let Some(schema) = &params.target_schema {
        map.insert("targetSchema".to_owned(), json!(schema));
    }
    if let Some(include) = params.include_indexes {
        map.insert("includeIndexes".to_owned(), json!(include));
    }
    if let Some(overrides) = &params.type_overrides {
        map.insert(
            "typeOverrides".to_owned(),
            serde_json::to_value(overrides).unwrap_or_default(),
        );
    }
    request
}

/// `schemaDiffCommands.prepareUnifiedPlan`'s params — the table plan plus
/// the explicit object identities.
#[derive(Clone, Debug, Default)]
pub(crate) struct PrepareUnifiedParamsVm {
    pub(crate) source_db_session_id: String,
    pub(crate) target_db_session_id: String,
    pub(crate) table_names: Vec<String>,
    pub(crate) target_table_names: Option<Vec<String>>,
    pub(crate) target_only_table_names: Option<Vec<String>>,
    pub(crate) source_schema: Option<String>,
    pub(crate) target_schema: Option<String>,
    pub(crate) allow_destructive: bool,
    pub(crate) include_indexes: Option<bool>,
    pub(crate) type_overrides: Option<Vec<ColumnTypeOverrideVm>>,
    pub(crate) source_objects: Vec<SchemaDiffObjectIdentityVm>,
    pub(crate) target_objects: Vec<SchemaDiffObjectIdentityVm>,
}

/// React's `exactIdentity`: the six fields with the optionals
/// `?? null`-filled — the identity keys ride the wire ALWAYS, null when
/// absent (the object-kind spelling is the wire string).
fn exact_identity(object: &SchemaDiffObjectIdentityVm) -> serde_json::Value {
    json!({
        "kind": object.kind.wire_str(),
        "schema": object.schema,
        "name": object.name,
        "signature": object.signature,
        "targetSchema": object.target_schema,
        "targetName": object.target_name,
    })
}

/// `SchemaDiffPrepareUnifiedRequest` on the wire. Unlike the table plan,
/// `targetOnlyTableNames` rides whenever present (React passes
/// `params.targetOnlyTableNames` through unconditionally — an empty array
/// stays an empty array); the identities are `exactIdentity`-mapped.
pub(crate) fn prepare_unified_plan_request(params: &PrepareUnifiedParamsVm) -> serde_json::Value {
    let mut request = json!({
        "sourceDbSessionId": params.source_db_session_id,
        "targetDbSessionId": params.target_db_session_id,
        "tableNames": params.table_names,
        "allowDestructive": params.allow_destructive,
    });
    // `json!` always builds an object here; a non-object would mean a
    // shaper bug, and returning the unextended request is the safe branch.
    let Some(map) = request.as_object_mut() else {
        return request;
    };
    if let Some(names) = &params.target_table_names {
        map.insert("targetTableNames".to_owned(), json!(names));
    }
    if let Some(names) = &params.target_only_table_names {
        map.insert("targetOnlyTableNames".to_owned(), json!(names));
    }
    if let Some(schema) = &params.source_schema {
        map.insert("sourceSchema".to_owned(), json!(schema));
    }
    if let Some(schema) = &params.target_schema {
        map.insert("targetSchema".to_owned(), json!(schema));
    }
    if let Some(include) = params.include_indexes {
        map.insert("includeIndexes".to_owned(), json!(include));
    }
    if let Some(overrides) = &params.type_overrides {
        map.insert(
            "typeOverrides".to_owned(),
            serde_json::to_value(overrides).unwrap_or_default(),
        );
    }
    map.insert(
        "sourceObjects".to_owned(),
        json!(params.source_objects.iter().map(exact_identity).collect::<Vec<_>>()),
    );
    map.insert(
        "targetObjects".to_owned(),
        json!(params.target_objects.iter().map(exact_identity).collect::<Vec<_>>()),
    );
    request
}

/// `schemaDiffCommands.executeDeploy`'s `profile` reference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DeployProfileRefVm {
    pub(crate) id: String,
    pub(crate) revision: String,
}

/// `schemaDiffCommands.executeDeploy`'s params. `target_database` /
/// `target_schema` keep JS's three states apart: `None` is `undefined`
/// (key absent), `Some(None)` is the explicit `null` the window sends
/// (`endpoints.targetDatabase || null`), `Some(Some(_))` a value.
#[derive(Clone, Debug, Default)]
pub(crate) struct ExecuteDeployParamsVm {
    pub(crate) target_db_session_id: String,
    pub(crate) plan: Option<SchemaDiffPlanVm>,
    pub(crate) use_transaction: Option<bool>,
    pub(crate) require_rollback: Option<bool>,
    pub(crate) confirm_destructive: Option<String>,
    pub(crate) job_id: Option<String>,
    pub(crate) target_database: Option<Option<String>>,
    pub(crate) target_schema: Option<Option<String>>,
    pub(crate) profile: Option<DeployProfileRefVm>,
    /// P5: the planId the apply job consumes (from a prepare envelope).
    pub(crate) plan_id: Option<String>,
    /// P5: the review version (from a prepare envelope).
    pub(crate) selection_revision: Option<i64>,
}

/// `SchemaDiffDeployRequest` on the wire: the base keys always, the plan
/// body denormalized when present, and the conditional spreads for
/// `profile` / `planId` / `selectionRevision` (`!= null`, so a
/// `selectionRevision` of 0 still rides).
pub(crate) fn execute_deploy_request(params: &ExecuteDeployParamsVm) -> serde_json::Value {
    let mut request = json!({
        "targetDbSessionId": params.target_db_session_id,
    });
    // `json!` always builds an object here; a non-object would mean a
    // shaper bug, and returning the unextended request is the safe branch.
    let Some(map) = request.as_object_mut() else {
        return request;
    };
    if let Some(plan) = &params.plan {
        map.insert("plan".to_owned(), denormalize_plan_to_json(plan));
    }
    if let Some(value) = params.use_transaction {
        map.insert("useTransaction".to_owned(), json!(value));
    }
    if let Some(value) = params.require_rollback {
        map.insert("requireRollback".to_owned(), json!(value));
    }
    if let Some(value) = &params.confirm_destructive {
        map.insert("confirmDestructive".to_owned(), json!(value));
    }
    if let Some(value) = &params.job_id {
        map.insert("jobId".to_owned(), json!(value));
    }
    if let Some(value) = &params.target_database {
        map.insert("targetDatabase".to_owned(), json!(value));
    }
    if let Some(value) = &params.target_schema {
        map.insert("targetSchema".to_owned(), json!(value));
    }
    if let Some(profile) = &params.profile {
        map.insert("profile".to_owned(), json!({ "id": profile.id, "revision": profile.revision }));
    }
    if let Some(plan_id) = &params.plan_id {
        map.insert("planId".to_owned(), json!(plan_id));
    }
    if let Some(revision) = params.selection_revision {
        map.insert("selectionRevision".to_owned(), json!(revision));
    }
    request
}

// ---- the submit→poll→resolve machine -----------------------------------------

/// React's poll cadence: `wait(400)` between `getJobDetails` calls.
pub(crate) const SCHEMA_DIFF_POLL_INTERVAL_MS: i64 = 400;

/// What the resolved run produced (`result.prepared` / `detail.deployResult`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SchemaDiffJobRunOutcome {
    /// A prepare run's `result.prepared`.
    Prepared(SchemaDiffPrepareEnvelopeVm),
    /// An apply run's `detail.deployResult`.
    Deployed(SchemaDiffDeployResultVm),
}

/// What the host must run next — the await points of
/// `submitSchemaDiffPrepare` / `executeDeploy`, as effects.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SchemaDiffEffect {
    /// `userAssetToken()`: mint the `saveUserAsset` submission token.
    IssueUserAssetToken,
    /// One domain-client call: the method and its exact wire payload.
    Call { method: SchemaDiffMethod, request: serde_json::Value },
    /// The poll wait: `tx.wake_at(until_ms / 1000)`.
    Wait { until_ms: i64 },
    /// `publishSchemaDiffJobUpdate` — route to the observer bus.
    Publish(super::SchemaDiffJobUpdateVm),
}

/// Which orchestration a run performs (the error wording depends on it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SchemaDiffRunKind {
    Prepare,
    Apply,
}

/// The poll arming: due immediately (the loop polls before its first
/// wait), in flight (awaiting the details receipt), or waiting until the
/// 400ms deadline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PollArmed {
    Due,
    InFlight,
    At(i64),
}

/// One in-flight orchestration. The host drives: [`SchemaDiffJobRun::begin`]
/// → token → call → accepted → (poll → details)* → resolved.
#[derive(Debug)]
pub(crate) struct SchemaDiffJobRun {
    kind: SchemaDiffRunKind,
    phase: RunPhase,
    armed: PollArmed,
    pending_call: Option<(SchemaDiffMethod, serde_json::Value)>,
}

#[derive(Debug)]
enum RunPhase {
    /// Awaiting the minted token.
    AwaitingToken,
    /// Awaiting the prepare/execute accepted receipt.
    AwaitingAccepted,
    /// Polling `getJobDetails` until terminal.
    Polling { job_id: String },
    /// The promise settled.
    Done(Option<Result<SchemaDiffJobRunOutcome, String>>),
}

impl SchemaDiffJobRun {
    /// `submitSchemaDiffPrepare` / `executeDeploy`'s entry: mint the token
    /// first (`userAssetToken()`), then fire the call.
    pub(crate) fn begin(
        kind: SchemaDiffRunKind,
        method: SchemaDiffMethod,
        request: serde_json::Value,
    ) -> Self {
        Self {
            kind,
            phase: RunPhase::AwaitingToken,
            armed: PollArmed::Due,
            pending_call: Some((method, request)),
        }
    }

    /// `on_token`: merge the token into the pending request and fire the
    /// call.
    pub(crate) fn on_token(&mut self, token: &SchemaDiffSubmissionTokenVm) -> Vec<SchemaDiffEffect> {
        match std::mem::replace(&mut self.phase, RunPhase::Done(None)) {
            RunPhase::AwaitingToken => {
                if let Some((method, request)) = self.pending_call.take() {
                    self.phase = RunPhase::AwaitingAccepted;
                    vec![SchemaDiffEffect::Call {
                        method,
                        request: with_submission_token(request, token),
                    }]
                } else {
                    self.phase = RunPhase::Done(None);
                    Vec::new()
                }
            }
            other => {
                self.phase = other;
                Vec::new()
            }
        }
    }

    /// The accepted receipt: publish it, then enter the poll loop — the
    /// first `getJobDetails` fires immediately (React's `for(;;)` polls
    /// before its first `wait`), armed `InFlight` until the receipt.
    pub(crate) fn on_accepted(&mut self, accepted: SchemaDiffJobAcceptedVm) -> Vec<SchemaDiffEffect> {
        let job_id = accepted.job_id.clone();
        match std::mem::replace(&mut self.phase, RunPhase::Done(None)) {
            RunPhase::AwaitingAccepted => {
                self.phase = RunPhase::Polling { job_id: job_id.clone() };
                self.armed = PollArmed::InFlight;
                vec![
                    SchemaDiffEffect::Publish(super::SchemaDiffJobUpdateVm::Accepted(accepted)),
                    SchemaDiffEffect::Call {
                        method: SchemaDiffMethod::GetJobDetails,
                        request: get_job_details_request(&job_id),
                    },
                ]
            }
            other => {
                self.phase = other;
                Vec::new()
            }
        }
    }

    /// The accepted receipt from its raw wire JSON (host convenience).
    pub(crate) fn on_accepted_json(&mut self, accepted: serde_json::Value) -> Vec<SchemaDiffEffect> {
        match parse_job_accepted(accepted) {
            Ok(accepted) => self.on_accepted(accepted),
            Err(error) => {
                self.fail(error);
                Vec::new()
            }
        }
    }

    /// The clock tick: fire the due poll call. A `PollArmed::At` deadline
    /// that has passed becomes due (the `wake_at` the host armed); a poll
    /// already in flight does not re-fire (React is `await`ing it).
    pub(crate) fn tick(&mut self, now_ms: i64) -> Vec<SchemaDiffEffect> {
        if !matches!(self.phase, RunPhase::Polling { .. }) {
            return Vec::new();
        }
        let due = match self.armed {
            PollArmed::Due => true,
            PollArmed::At(until) if now_ms >= until => true,
            _ => false,
        };
        if !due {
            return Vec::new();
        }
        if let RunPhase::Polling { job_id } = &self.phase {
            self.armed = PollArmed::InFlight;
            let job_id = job_id.clone();
            return vec![SchemaDiffEffect::Call {
                method: SchemaDiffMethod::GetJobDetails,
                request: get_job_details_request(&job_id),
            }];
        }
        Vec::new()
    }

    /// The details receipt (host-decoded): publish it, then settle —
    /// terminal resolves the run (prepare → `result.prepared` or the
    /// plan-unavailable error; apply → `detail.deployResult` or the apply
    /// error), non-terminal re-arms the 400ms wait.
    pub(crate) fn on_details(&mut self, now_ms: i64, details: SchemaDiffJobDetailsVm) -> Vec<SchemaDiffEffect> {
        if !matches!(self.phase, RunPhase::Polling { .. }) {
            return Vec::new();
        }
        let mut effects =
            vec![SchemaDiffEffect::Publish(super::SchemaDiffJobUpdateVm::Details(details.clone()))];
        if details.job_terminal() {
            let settled = self.settle(&details);
            self.phase = RunPhase::Done(Some(settled));
        } else {
            let until = now_ms + SCHEMA_DIFF_POLL_INTERVAL_MS;
            self.armed = PollArmed::At(until);
            effects.push(SchemaDiffEffect::Wait { until_ms: until });
        }
        effects
    }

    /// The details receipt from its raw wire JSON — the domain client's
    /// `getJobDetails` path (fold the tagged requirements, then continue).
    pub(crate) fn on_details_json(&mut self, now_ms: i64, raw: serde_json::Value) -> Vec<SchemaDiffEffect> {
        match super::wire::normalize_job_details(raw) {
            Ok(details) => self.on_details(now_ms, details),
            Err(error) => {
                self.fail(error);
                Vec::new()
            }
        }
    }

    /// A transport failure on any in-flight call: the promise rejects
    /// (the guard's throw path).
    pub(crate) fn on_call_error(&mut self, error: String) {
        self.fail(error);
    }

    /// The run's resolution, once terminal.
    pub(crate) fn result(&self) -> Option<&Result<SchemaDiffJobRunOutcome, String>> {
        match &self.phase {
            RunPhase::Done(settled) => settled.as_ref(),
            _ => None,
        }
    }

    /// Whether the run has settled.
    pub(crate) fn is_done(&self) -> bool {
        matches!(self.phase, RunPhase::Done(_))
    }

    /// The poll deadline the host should `wake_at` (`None` when no wait
    /// is armed).
    pub(crate) fn next_wake_ms(&self) -> Option<i64> {
        match self.armed {
            PollArmed::At(until) if matches!(self.phase, RunPhase::Polling { .. }) => Some(until),
            _ => None,
        }
    }

    /// The poll loop's terminal resolution, verbatim:
    /// prepare reads `result.prepared`, else
    /// `PlanUnavailableAfterRestart: compare again before applying` /
    /// `Schema Diff plan job ended in {state} ({reason})` with
    /// `reason = details.job.error ?? 'planUnavailable'`; apply reads
    /// `detail.deployResult`, else `Schema Diff apply job ended in {state}`.
    fn settle(&self, details: &SchemaDiffJobDetailsVm) -> Result<SchemaDiffJobRunOutcome, String> {
        match self.kind {
            SchemaDiffRunKind::Prepare => {
                if let Some(envelope) = &details.prepared {
                    return Ok(SchemaDiffJobRunOutcome::Prepared(envelope.clone()));
                }
                if details.plan_unavailable_after_restart {
                    return Err(
                        "PlanUnavailableAfterRestart: compare again before applying".to_owned(),
                    );
                }
                let reason = details
                    .details
                    .error
                    .clone()
                    .unwrap_or_else(|| "planUnavailable".to_owned());
                Err(format!(
                    "Schema Diff plan job ended in {} ({})",
                    details.details.state.as_str(),
                    reason
                ))
            }
            SchemaDiffRunKind::Apply => {
                if let Some(result) = &details.deploy_result {
                    return Ok(SchemaDiffJobRunOutcome::Deployed(result.clone()));
                }
                Err(format!(
                    "Schema Diff apply job ended in {}",
                    details.details.state.as_str()
                ))
            }
        }
    }

    fn fail(&mut self, error: String) {
        self.phase = RunPhase::Done(Some(Err(error)));
    }
}

/// The `schemaDiffPrepare` / `schemaDiffApply` accepted kinds, as the wire
/// spells them (for host-side kind routing).
pub(crate) const SCHEMA_DIFF_PREPARE_KIND: &str = "schemaDiffPrepare";
pub(crate) const SCHEMA_DIFF_APPLY_KIND: &str = "schemaDiffApply";
