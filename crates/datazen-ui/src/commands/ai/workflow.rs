//! The workflow slice — `commands/ai.ts`'s `useBackendClient()` calls over
//! the Workflow domain client (`workflowClient.ts`'s `MethodMap`), plus
//! the desktop-only `workflow_get_dir` legacy command.
//!
//! Write faces that carry a body (save / save-yaml / execute) fold the
//! `saveUserAsset` submission token (`{ ...request, submissionToken }`,
//! contract §6.14 — user assets, unbound from sessions); the bodyless
//! DELETE faces (delete / clear-runs) carry none. `saveWorkflow`'s
//! definition rides inside the `{ workflow }` bag; `workflowSave`'s
//! `mergeDraftCommandFields` merge is `windows/workflow/draftBridge.ts`
//! (this segment's T482) and stays in front of this face — the shaper
//! takes the definition as given.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// React `SubmissionToken` (the `saveUserAsset` mint).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiSubmissionTokenVm {
    #[serde(rename = "idempotencyKey")]
    pub idempotency_key: String,
    #[serde(rename = "expiresAt")]
    pub expires_at: String,
}

/// The submission intent every workflow token mint uses
/// (`issueSubmissionToken('saveUserAsset', null)`).
pub const USER_ASSET_TOKEN_INTENT: &str = "saveUserAsset";

/// `transport.call(name, { …request, submissionToken })` — the token merge
/// the domain client performs for the three body-carrying write faces.
pub fn with_submission_token(
    mut request: serde_json::Value,
    token: &AiSubmissionTokenVm,
) -> serde_json::Value {
    if let serde_json::Value::Object(map) = &mut request {
        map.insert(
            "submissionToken".to_owned(),
            serde_json::to_value(token).unwrap_or_default(),
        );
    }
    request
}

/// The Workflow `MethodMap` slice — one variant per domain method;
/// `wire_name` is the transport call name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkflowMethod {
    ListWorkflows,
    GetWorkflow,
    GetWorkflowYaml,
    SaveWorkflow,
    SaveWorkflowYaml,
    DeleteWorkflow,
    ExecuteWorkflow,
    ReloadWorkflows,
    ListWorkflowRuns,
    GetWorkflowRun,
    ClearWorkflowRuns,
}

impl WorkflowMethod {
    /// The transport call name (`transport.call(name, payload)`).
    pub fn wire_name(self) -> &'static str {
        match self {
            Self::ListWorkflows => "listWorkflows",
            Self::GetWorkflow => "getWorkflow",
            Self::GetWorkflowYaml => "getWorkflowYaml",
            Self::SaveWorkflow => "saveWorkflow",
            Self::SaveWorkflowYaml => "saveWorkflowYaml",
            Self::DeleteWorkflow => "deleteWorkflow",
            Self::ExecuteWorkflow => "executeWorkflow",
            Self::ReloadWorkflows => "reloadWorkflows",
            Self::ListWorkflowRuns => "listWorkflowRuns",
            Self::GetWorkflowRun => "getWorkflowRun",
            Self::ClearWorkflowRuns => "clearWorkflowRuns",
        }
    }
}

/// `workflowList()` → `listWorkflows` `{}`.
pub fn workflow_list_request() -> (WorkflowMethod, Value) {
    (WorkflowMethod::ListWorkflows, json!({}))
}

/// `workflowGet(workflowId)` → `getWorkflow` `{ workflowId }`.
pub fn workflow_get_request(workflow_id: &str) -> (WorkflowMethod, Value) {
    (WorkflowMethod::GetWorkflow, json!({ "workflowId": workflow_id }))
}

/// `workflowGetYaml(workflowId)` → `getWorkflowYaml` `{ workflowId }`.
pub fn workflow_get_yaml_request(workflow_id: &str) -> (WorkflowMethod, Value) {
    (WorkflowMethod::GetWorkflowYaml, json!({ "workflowId": workflow_id }))
}

/// `workflowSave(workflow)` → `saveWorkflow` with the `{ workflow }` bag
/// (the draft-bridge merge already applied by the caller) plus the token.
pub fn save_workflow_request(
    workflow: &Value,
    token: &AiSubmissionTokenVm,
) -> (WorkflowMethod, Value) {
    (
        WorkflowMethod::SaveWorkflow,
        with_submission_token(json!({ "workflow": workflow }), token),
    )
}

/// `workflowSaveYaml(yaml)` → `saveWorkflowYaml` `{ yaml }` plus the token.
pub fn save_workflow_yaml_request(
    yaml: &str,
    token: &AiSubmissionTokenVm,
) -> (WorkflowMethod, Value) {
    (
        WorkflowMethod::SaveWorkflowYaml,
        with_submission_token(json!({ "yaml": yaml }), token),
    )
}

/// `workflowDelete(workflowId)` → `deleteWorkflow` (bodyless DELETE face,
/// no token).
pub fn workflow_delete_request(workflow_id: &str) -> (WorkflowMethod, Value) {
    (WorkflowMethod::DeleteWorkflow, json!({ "workflowId": workflow_id }))
}

/// `workflowExecute`'s param bag.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct WorkflowExecuteParams {
    #[serde(rename = "workflowId")]
    pub workflow_id: String,
    pub variables: Value,
    #[serde(rename = "connectionId", skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<String>,
}

/// `workflowExecute(params)` → `executeWorkflow` plus the token.
pub fn workflow_execute_request(
    params: &WorkflowExecuteParams,
    token: &AiSubmissionTokenVm,
) -> (WorkflowMethod, Value) {
    (
        WorkflowMethod::ExecuteWorkflow,
        with_submission_token(serde_json::to_value(params).unwrap_or_default(), token),
    )
}

/// `workflowReload()` → `reloadWorkflows` `{}`.
pub fn workflow_reload_request() -> (WorkflowMethod, Value) {
    (WorkflowMethod::ReloadWorkflows, json!({}))
}

/// `workflowHistoryList(workflowId?)` → `listWorkflowRuns`; an absent id
/// means the whole recorded history (the `{}` call, exactly React's
/// `workflowId == null ? {} : { workflowId }`).
pub fn workflow_history_list_request(workflow_id: Option<&str>) -> (WorkflowMethod, Value) {
    match workflow_id {
        Some(id) => (WorkflowMethod::ListWorkflowRuns, json!({ "workflowId": id })),
        None => (WorkflowMethod::ListWorkflowRuns, json!({})),
    }
}

/// `workflowHistoryGet(historyId)` → `getWorkflowRun` `{ historyId }`.
pub fn workflow_history_get_request(history_id: &str) -> (WorkflowMethod, Value) {
    (WorkflowMethod::GetWorkflowRun, json!({ "historyId": history_id }))
}

/// `workflowHistoryClear(workflowId?)` → `clearWorkflowRuns` (bodyless
/// DELETE face, no token; absent id = the whole history).
pub fn workflow_history_clear_request(workflow_id: Option<&str>) -> (WorkflowMethod, Value) {
    match workflow_id {
        Some(id) => (WorkflowMethod::ClearWorkflowRuns, json!({ "workflowId": id })),
        None => (WorkflowMethod::ClearWorkflowRuns, json!({})),
    }
}

/// `workflowGetDir()` → the desktop-only `workflow_get_dir` legacy command
/// (filesystem semantics; deliberately not a domain-client method — the
/// web form reports the gap as `CapabilityUnsupported`).
pub fn workflow_get_dir_request() -> super::wire::AiRequest {
    super::wire::AiRequest::native("workflow_get_dir", json!({}))
}
