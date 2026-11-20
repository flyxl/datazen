//! The `ai_*` and `prompt_*` legacy shapers — `commands/ai.ts`'s
//! `aiCommands` object, method for method. Payloads are key-for-key with
//! the React source (camelCase, `?? null` normalizations applied,
//! `undefined` keys dropped).

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

// ---- the request envelope -----------------------------------------------------

/// Whether the E2E stub harness may answer a captured call (React's
/// `e2eStubInvoke` vs `nativeInvoke` split).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum E2eStubPolicy {
    /// The E2E harness may capture the call and answer `''`
    /// (`ai_chat` / `ai_generate_sql` only).
    Capturable,
    /// Always reaches the native host: the stub's empty string would be
    /// an invalid receipt (the grant lifecycle commands).
    NativeOnly,
}

/// One shaped legacy command: the `legacyInvoke` name plus its payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AiRequest {
    /// The legacy command name (React `LegacyCommandName`).
    pub(crate) command: &'static str,
    /// The exact `args` object the React wrapper passes.
    pub(crate) value: Value,
    /// The stub policy (`e2eStubInvoke` reach).
    pub(crate) stub: E2eStubPolicy,
}

impl AiRequest {
    /// A plain (non-stub-capturable) legacy call.
    pub(crate) fn native(command: &'static str, value: Value) -> Self {
        Self { command, value, stub: E2eStubPolicy::NativeOnly }
    }
}

/// `ai_chat` — one of the two E2E-capturable commands.
pub(crate) const CHAT_COMMAND: &str = "ai_chat";
/// `ai_generate_sql` — the other E2E-capturable command.
pub(crate) const GENERATE_SQL_COMMAND: &str = "ai_generate_sql";

// ---- the prompt vocabulary (ai.ts's own exported types) ------------------------

/// React `PromptScenario`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum PromptScenario {
    #[serde(rename = "nl2sql")]
    Nl2sql,
    #[serde(rename = "diagnose")]
    Diagnose,
    #[serde(rename = "nl_filter")]
    NlFilter,
    #[serde(rename = "schema_doc_select_tables")]
    SchemaDocSelectTables,
    #[serde(rename = "schema_doc")]
    SchemaDoc,
    #[serde(rename = "connection_diagnose")]
    ConnectionDiagnose,
    #[serde(rename = "query_summary")]
    QuerySummary,
    #[serde(rename = "explain_analysis")]
    ExplainAnalysis,
    #[serde(rename = "chat")]
    Chat,
    #[serde(rename = "workflow_generate")]
    WorkflowGenerate,
}

/// React `PromptSource`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum PromptSource {
    #[serde(rename = "default")]
    Default,
    #[serde(rename = "driver")]
    Driver,
    #[serde(rename = "user")]
    User,
}

/// React `PromptInfo`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct PromptInfo {
    pub(crate) scenario: PromptScenario,
    pub(crate) label: String,
    pub(crate) source: PromptSource,
    pub(crate) system: String,
    #[serde(rename = "defaultSystem")]
    pub(crate) default_system: String,
}

/// React `PromptOverrideEntry`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct PromptOverrideEntry {
    #[serde(rename = "driverType")]
    pub(crate) driver_type: String,
    pub(crate) scenario: PromptScenario,
    pub(crate) system: String,
}

// ---- the stream event payloads -------------------------------------------------

/// React `StreamChunkPayload` (`types/ai.ts:579`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct StreamChunkPayload {
    #[serde(rename = "requestId")]
    pub(crate) request_id: String,
    pub(crate) content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) reasoning: Option<String>,
    pub(crate) done: bool,
    /// `done && cancelled` is the terminal chunk of a cancelled run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) cancelled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) usage: Option<StreamUsage>,
    /// FR-13 egress scope; the host sends it on the first chunk only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) egress_summary: Option<String>,
}

/// `StreamChunkPayload.usage`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct StreamUsage {
    #[serde(rename = "promptTokens")]
    pub(crate) prompt_tokens: i64,
    #[serde(rename = "completionTokens")]
    pub(crate) completion_tokens: i64,
    #[serde(rename = "totalTokens")]
    pub(crate) total_tokens: i64,
}

/// React `StreamErrorPayload` (`types/ai.ts:594`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct StreamErrorPayload {
    #[serde(rename = "requestId")]
    pub(crate) request_id: String,
    pub(crate) error: String,
}

// ---- provider config / skills --------------------------------------------------

/// `getProviders` → `ai_get_providers`.
pub(crate) fn get_providers_request() -> AiRequest {
    AiRequest::native("ai_get_providers", json!({}))
}

/// `fetchRemoteModels(protocol, endpoint, apiKey)` → `ai_fetch_remote_models`.
pub(crate) fn fetch_remote_models_request(protocol: &str, endpoint: &str, api_key: &str) -> AiRequest {
    AiRequest::native(
        "ai_fetch_remote_models",
        json!({ "protocol": protocol, "endpoint": endpoint, "apiKey": api_key }),
    )
}

/// `validateConfig(config)` → `ai_validate_config`.
pub(crate) fn validate_config_request(config: &Value) -> AiRequest {
    AiRequest::native("ai_validate_config", json!({ "config": config }))
}

/// `saveConfig(config)` → `ai_save_config`.
pub(crate) fn save_config_request(config: &Value) -> AiRequest {
    AiRequest::native("ai_save_config", json!({ "config": config }))
}

/// `getConfig` → `ai_get_config`.
pub(crate) fn get_config_request() -> AiRequest {
    AiRequest::native("ai_get_config", json!({}))
}

/// `listSkills` → `ai_list_skills`.
pub(crate) fn list_skills_request() -> AiRequest {
    AiRequest::native("ai_list_skills", json!({}))
}

/// `deleteConfig` → `ai_delete_config`.
pub(crate) fn delete_config_request() -> AiRequest {
    AiRequest::native("ai_delete_config", json!({}))
}

/// `getSettingsConfig` → `ai_get_settings_config`.
pub(crate) fn get_settings_config_request() -> AiRequest {
    AiRequest::native("ai_get_settings_config", json!({}))
}

/// `saveSettingsConfig(config)` → `ai_save_settings_config`.
pub(crate) fn save_settings_config_request(config: &Value) -> AiRequest {
    AiRequest::native("ai_save_settings_config", json!({ "config": config }))
}

/// `setActiveProfile(profileId)` → `ai_set_active_profile`.
pub(crate) fn set_active_profile_request(profile_id: &str) -> AiRequest {
    AiRequest::native("ai_set_active_profile", json!({ "profileId": profile_id }))
}

// ---- memory / sessions ----------------------------------------------------------

/// `memoryList(params = {})` → `ai_memory_list` (the params spread as-is).
pub(crate) fn memory_list_request(params: &Value) -> AiRequest {
    AiRequest::native("ai_memory_list", params.clone())
}

/// `orgMemoryList(params = {})` → `ai_org_memory_list`.
pub(crate) fn org_memory_list_request(params: &Value) -> AiRequest {
    AiRequest::native("ai_org_memory_list", params.clone())
}

/// `memorySearch(params & { query })` → `ai_memory_search`.
pub(crate) fn memory_search_request(params: &Value) -> AiRequest {
    AiRequest::native("ai_memory_search", params.clone())
}

/// `memoryStats` → `ai_memory_stats`.
pub(crate) fn memory_stats_request() -> AiRequest {
    AiRequest::native("ai_memory_stats", json!({}))
}

/// `memoryCreate(input)` → `ai_memory_create`.
pub(crate) fn memory_create_request(input: &Value) -> AiRequest {
    AiRequest::native("ai_memory_create", json!({ "input": input }))
}

/// `memoryCorrectionCandidate(input)` → `ai_memory_correction_candidate`.
pub(crate) fn memory_correction_candidate_request(input: &Value) -> AiRequest {
    AiRequest::native("ai_memory_correction_candidate", json!({ "input": input }))
}

/// `memoryExtractCandidates(input)` → `ai_memory_extract_candidates`.
pub(crate) fn memory_extract_candidates_request(input: &Value) -> AiRequest {
    AiRequest::native("ai_memory_extract_candidates", json!({ "input": input }))
}

/// `memoryMarkStale(input)` → `ai_memory_mark_stale`.
pub(crate) fn memory_mark_stale_request(input: &Value) -> AiRequest {
    AiRequest::native("ai_memory_mark_stale", json!({ "input": input }))
}

/// `sessionsList`'s param bag (all three keys are `?? null` normalized).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub(crate) struct SessionsListParams {
    #[serde(rename = "connectionId", skip_serializing_if = "Option::is_none")]
    pub(crate) connection_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) database: Option<String>,
    #[serde(rename = "sessionKey", skip_serializing_if = "Option::is_none")]
    pub(crate) session_key: Option<String>,
}

/// `sessionsList(params = {})` → `ai_sessions_list` with all three keys
/// `?? null` normalized onto the wire.
pub(crate) fn sessions_list_request(params: &SessionsListParams) -> AiRequest {
    let mut payload = json!({});
    let map = payload.as_object_mut().expect("fresh object");
    map.insert("connectionId".into(), json!(params.connection_id));
    map.insert("database".into(), json!(params.database));
    map.insert("sessionKey".into(), json!(params.session_key));
    AiRequest::native("ai_sessions_list", payload)
}

/// `sessionsGet(id)` → `ai_sessions_get`.
pub(crate) fn sessions_get_request(id: &str) -> AiRequest {
    AiRequest::native("ai_sessions_get", json!({ "id": id }))
}

/// `sessionsUpsert(input)` → `ai_sessions_upsert`.
pub(crate) fn sessions_upsert_request(input: &Value) -> AiRequest {
    AiRequest::native("ai_sessions_upsert", json!({ "input": input }))
}

/// `sessionsDelete(id)` → `ai_sessions_delete`.
pub(crate) fn sessions_delete_request(id: &str) -> AiRequest {
    AiRequest::native("ai_sessions_delete", json!({ "id": id }))
}

/// `sessionsExport(id, format)` → `ai_sessions_export`.
pub(crate) fn sessions_export_request(id: &str, format: &str) -> AiRequest {
    AiRequest::native("ai_sessions_export", json!({ "id": id, "format": format }))
}

/// `memoryUpdate(id, patch)` → `ai_memory_update`.
pub(crate) fn memory_update_request(id: &str, patch: &Value) -> AiRequest {
    AiRequest::native("ai_memory_update", json!({ "id": id, "patch": patch }))
}

/// `memoryConfirm(id)` → `ai_memory_confirm`.
pub(crate) fn memory_confirm_request(id: &str) -> AiRequest {
    AiRequest::native("ai_memory_confirm", json!({ "id": id }))
}

/// `memoryReject(id)` → `ai_memory_reject`.
pub(crate) fn memory_reject_request(id: &str) -> AiRequest {
    AiRequest::native("ai_memory_reject", json!({ "id": id }))
}

/// `memoryDelete(id)` → `ai_memory_delete`.
pub(crate) fn memory_delete_request(id: &str) -> AiRequest {
    AiRequest::native("ai_memory_delete", json!({ "id": id }))
}

/// `memoryClear(params = {})` → `ai_memory_clear` (the params spread).
pub(crate) fn memory_clear_request(params: &Value) -> AiRequest {
    AiRequest::native("ai_memory_clear", params.clone())
}

/// `memoryExport(params)` → `ai_memory_export`.
pub(crate) fn memory_export_request(params: &Value) -> AiRequest {
    AiRequest::native("ai_memory_export", params.clone())
}

// ---- generate / chat / notes / filter -------------------------------------------

/// `generateSql`'s param bag (the E2E-capturable command).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub(crate) struct GenerateSqlParams {
    #[serde(rename = "dbSessionId")]
    pub(crate) db_session_id: String,
    pub(crate) database: String,
    #[serde(rename = "naturalLanguage")]
    pub(crate) natural_language: String,
    #[serde(rename = "requestId")]
    pub(crate) request_id: String,
    #[serde(rename = "currentTable", skip_serializing_if = "Option::is_none")]
    pub(crate) current_table: Option<String>,
    #[serde(rename = "recentQueries", skip_serializing_if = "Option::is_none")]
    pub(crate) recent_queries: Option<Vec<String>>,
    #[serde(rename = "contextFiles", skip_serializing_if = "Option::is_none")]
    pub(crate) context_files: Option<Vec<String>>,
    #[serde(rename = "contextTables", skip_serializing_if = "Option::is_none")]
    pub(crate) context_tables: Option<Vec<String>>,
    #[serde(rename = "connectionId", skip_serializing_if = "Option::is_none")]
    pub(crate) connection_id: Option<String>,
    #[serde(rename = "memoryMode", skip_serializing_if = "Option::is_none")]
    pub(crate) memory_mode: Option<String>,
    #[serde(rename = "memoryScope", skip_serializing_if = "Option::is_none")]
    pub(crate) memory_scope: Option<String>,
}

/// `generateSql(params)` → `ai_generate_sql` (E2E-capturable).
pub(crate) fn generate_sql_request(params: &GenerateSqlParams) -> AiRequest {
    AiRequest {
        command: GENERATE_SQL_COMMAND,
        value: serde_json::to_value(params).unwrap_or_default(),
        stub: E2eStubPolicy::Capturable,
    }
}

/// `diagnoseError`'s param bag.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct DiagnoseErrorParams {
    #[serde(rename = "dbSessionId")]
    pub(crate) db_session_id: String,
    pub(crate) database: String,
    pub(crate) sql: String,
    #[serde(rename = "errorMessage")]
    pub(crate) error_message: String,
}

/// `diagnoseError(params)` → `ai_diagnose_error`.
pub(crate) fn diagnose_error_request(params: &DiagnoseErrorParams) -> AiRequest {
    AiRequest::native("ai_diagnose_error", serde_json::to_value(params).unwrap_or_default())
}

/// `analyzeExplain`'s param bag.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct AnalyzeExplainParams {
    #[serde(rename = "dbSessionId")]
    pub(crate) db_session_id: String,
    #[serde(rename = "explainOutput")]
    pub(crate) explain_output: String,
    #[serde(rename = "originalSql")]
    pub(crate) original_sql: String,
}

/// `analyzeExplain(params)` → `ai_analyze_explain`.
pub(crate) fn analyze_explain_request(params: &AnalyzeExplainParams) -> AiRequest {
    AiRequest::native("ai_analyze_explain", serde_json::to_value(params).unwrap_or_default())
}

/// `chat`'s param bag — the non-scalar members (`messages`, `targetClaim`,
/// `resultPreview`) ride as `Value`s: their VMs belong to the chat panel
/// (`ai_chat_panel_types`), and the shaper's contract is the wire bag.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub(crate) struct ChatParams {
    #[serde(rename = "dbSessionId", skip_serializing_if = "Option::is_none")]
    pub(crate) db_session_id: Option<String>,
    #[serde(rename = "connectionId", skip_serializing_if = "Option::is_none")]
    pub(crate) connection_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) database: Option<String>,
    /// `messages: AiChatMessage[]`.
    pub(crate) messages: Value,
    #[serde(rename = "requestId")]
    pub(crate) request_id: String,
    #[serde(rename = "includeSchema", skip_serializing_if = "Option::is_none")]
    pub(crate) include_schema: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) scenario: Option<PromptScenario>,
    #[serde(rename = "agentMode", skip_serializing_if = "Option::is_none")]
    pub(crate) agent_mode: Option<Value>,
    #[serde(rename = "loadedSkillIds", skip_serializing_if = "Option::is_none")]
    pub(crate) loaded_skill_ids: Option<Vec<String>>,
    #[serde(rename = "contextFiles", skip_serializing_if = "Option::is_none")]
    pub(crate) context_files: Option<Vec<String>>,
    #[serde(rename = "contextTables", skip_serializing_if = "Option::is_none")]
    pub(crate) context_tables: Option<Vec<String>>,
    #[serde(rename = "targetClaim", skip_serializing_if = "Option::is_none")]
    pub(crate) target_claim: Option<Value>,
    #[serde(rename = "editorSql", skip_serializing_if = "Option::is_none")]
    pub(crate) editor_sql: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) selection: Option<String>,
    #[serde(rename = "lastError", skip_serializing_if = "Option::is_none")]
    pub(crate) last_error: Option<String>,
    #[serde(rename = "resultPreview", skip_serializing_if = "Option::is_none")]
    pub(crate) result_preview: Option<Value>,
    #[serde(rename = "templateId", skip_serializing_if = "Option::is_none")]
    pub(crate) template_id: Option<String>,
    #[serde(rename = "databaseType", skip_serializing_if = "Option::is_none")]
    pub(crate) database_type: Option<String>,
    #[serde(rename = "memoryMode", skip_serializing_if = "Option::is_none")]
    pub(crate) memory_mode: Option<String>,
    #[serde(rename = "memoryScope", skip_serializing_if = "Option::is_none")]
    pub(crate) memory_scope: Option<String>,
}

/// `chat(params)` → `ai_chat` (E2E-capturable).
pub(crate) fn chat_request(params: &ChatParams) -> AiRequest {
    AiRequest {
        command: CHAT_COMMAND,
        value: serde_json::to_value(params).unwrap_or_default(),
        stub: E2eStubPolicy::Capturable,
    }
}

/// `connectionNotesGet(connectionId)` → `ai_connection_notes_get`.
pub(crate) fn connection_notes_get_request(connection_id: &str) -> AiRequest {
    AiRequest::native("ai_connection_notes_get", json!({ "connectionId": connection_id }))
}

/// `connectionNotesSet(connectionId, content)` → `ai_connection_notes_set`.
pub(crate) fn connection_notes_set_request(connection_id: &str, content: &str) -> AiRequest {
    AiRequest::native(
        "ai_connection_notes_set",
        json!({ "connectionId": connection_id, "content": content }),
    )
}

/// `connectionContextCounts(connectionId)` → `ai_connection_context_counts`.
pub(crate) fn connection_context_counts_request(connection_id: &str) -> AiRequest {
    AiRequest::native("ai_connection_context_counts", json!({ "connectionId": connection_id }))
}

/// `parseFilter`'s param bag.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct ParseFilterParams {
    #[serde(rename = "dbSessionId")]
    pub(crate) db_session_id: String,
    pub(crate) database: String,
    pub(crate) table: String,
    #[serde(rename = "naturalLanguage")]
    pub(crate) natural_language: String,
}

/// `parseFilter(params)` → `ai_parse_filter`.
pub(crate) fn parse_filter_request(params: &ParseFilterParams) -> AiRequest {
    AiRequest::native("ai_parse_filter", serde_json::to_value(params).unwrap_or_default())
}

// ---- the write face --------------------------------------------------------------

/// `aiProposeWrite`'s param bag.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct ProposeWriteParams {
    #[serde(rename = "connectionId")]
    pub(crate) connection_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) database: Option<String>,
    pub(crate) sql: String,
    #[serde(rename = "previousSql", skip_serializing_if = "Option::is_none")]
    pub(crate) previous_sql: Option<String>,
}

/// `aiProposeWrite(params)` → `ai_propose_write`.
pub(crate) fn ai_propose_write_request(params: &ProposeWriteParams) -> AiRequest {
    AiRequest::native("ai_propose_write", serde_json::to_value(params).unwrap_or_default())
}

/// `aiApplyWrite`'s param bag.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct ApplyWriteParams {
    #[serde(rename = "proposalId")]
    pub(crate) proposal_id: String,
    #[serde(rename = "connectionId")]
    pub(crate) connection_id: String,
    pub(crate) database: String,
    pub(crate) sql: String,
}

/// `aiApplyWrite(params)` → `ai_apply_write`.
pub(crate) fn ai_apply_write_request(params: &ApplyWriteParams) -> AiRequest {
    AiRequest::native("ai_apply_write", serde_json::to_value(params).unwrap_or_default())
}

/// `aiDenyWrite(proposalId)` → `ai_deny_write`.
pub(crate) fn ai_deny_write_request(proposal_id: &str) -> AiRequest {
    AiRequest::native("ai_deny_write", json!({ "proposalId": proposal_id }))
}

/// `aiListWriteAudit(limit?)` → `ai_list_write_audit` (`limit ?? null`).
pub(crate) fn ai_list_write_audit_request(limit: Option<i64>) -> AiRequest {
    AiRequest::native("ai_list_write_audit", json!({ "limit": limit }))
}

// ---- the schema/analysis faces ----------------------------------------------------

/// `generateSchemaDoc`'s param bag.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct GenerateSchemaDocParams {
    #[serde(rename = "dbSessionId")]
    pub(crate) db_session_id: String,
    pub(crate) database: String,
}

/// `generateSchemaDoc(params)` → `ai_generate_schema_doc`.
pub(crate) fn generate_schema_doc_request(params: &GenerateSchemaDocParams) -> AiRequest {
    AiRequest::native("ai_generate_schema_doc", serde_json::to_value(params).unwrap_or_default())
}

/// `diagnoseConnection`'s param bag.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct DiagnoseConnectionParams {
    #[serde(rename = "connectionId")]
    pub(crate) connection_id: String,
    #[serde(rename = "errorMessage")]
    pub(crate) error_message: String,
}

/// `diagnoseConnection(params)` → `ai_diagnose_connection`.
pub(crate) fn diagnose_connection_request(params: &DiagnoseConnectionParams) -> AiRequest {
    AiRequest::native("ai_diagnose_connection", serde_json::to_value(params).unwrap_or_default())
}

/// `analyzeQueries({ dbSessionId? })` → `ai_analyze_queries`; the params
/// object spreads, so an absent `dbSessionId` is dropped by
/// `JSON.stringify` (the wire is `{}`, not `{ dbSessionId: null }`).
pub(crate) fn analyze_queries_request(db_session_id: Option<&str>) -> AiRequest {
    match db_session_id {
        Some(id) => AiRequest::native("ai_analyze_queries", json!({ "dbSessionId": id })),
        None => AiRequest::native("ai_analyze_queries", json!({})),
    }
}

// ---- cancel + the grant lifecycle --------------------------------------------------

/// `cancel(requestId)` → `ai_cancel` (the three-state `CancelReceipt`).
pub(crate) fn cancel_request(request_id: &str) -> AiRequest {
    AiRequest::native("ai_cancel", json!({ "requestId": request_id }))
}

/// `grantEditorSession(params)` → `ai_grant_editor_session` — always
/// native: the E2E stub's `''` would be an invalid receipt.
pub(crate) fn grant_editor_session_request(params: &Value) -> AiRequest {
    AiRequest::native("ai_grant_editor_session", params.clone())
}

/// `revokeEditorGrant(grantId)` → `ai_revoke_editor_grant` (native).
pub(crate) fn revoke_editor_grant_request(grant_id: &str) -> AiRequest {
    AiRequest::native("ai_revoke_editor_grant", json!({ "grantId": grant_id }))
}

/// `touchEditorGrant(grantId)` → `ai_touch_editor_grant` (native).
pub(crate) fn touch_editor_grant_request(grant_id: &str) -> AiRequest {
    AiRequest::native("ai_touch_editor_grant", json!({ "grantId": grant_id }))
}

// ---- prompts -----------------------------------------------------------------------

/// `promptList(driverType?)` → `prompt_list` (`driverType ?? null`).
pub(crate) fn prompt_list_request(driver_type: Option<&str>) -> AiRequest {
    AiRequest::native("prompt_list", json!({ "driverType": driver_type }))
}

/// `promptSetOverride(entry)` → `prompt_set_override`.
pub(crate) fn prompt_set_override_request(entry: &PromptOverrideEntry) -> AiRequest {
    AiRequest::native("prompt_set_override", json!({ "entry": entry }))
}

/// `promptRemoveOverride(driverType, scenario)` → `prompt_remove_override`.
pub(crate) fn prompt_remove_override_request(driver_type: &str, scenario: PromptScenario) -> AiRequest {
    AiRequest::native(
        "prompt_remove_override",
        json!({ "driverType": driver_type, "scenario": scenario }),
    )
}
