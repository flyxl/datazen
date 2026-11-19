//! The zustand AiStore's type contract — ported from React
//! `packages/frontend/src/stores/ai/types.ts` (tasklist T126, 261 lines).
//!
//! types.ts is the contract file `stores/aiStore.ts` (T128) implements and
//! every AI panel consumes: [`Nl2SqlState`] with its [`initial_nl2sql`]
//! literal, and the giant `AiStore` interface — **50 state fields and 55
//! action signatures** — plus the two message re-exports on the file's
//! tail. Pure contract — no IO, no state — so the module compiles for
//! wasm32 exactly like its `*_types` siblings.
//!
//! ## The interface's two halves
//!
//! React's `AiStore` is one interface that conflates the state a component
//! reads with the actions it calls. The port splits it the way the T088
//! streaming port already split its lean view:
//!
//! - the state half → [`AiStoreState`], a struct with all 50 fields;
//! - the action half → [`AiStoreActions`], a trait with all 55
//!   signatures.
//!
//! The implementer is the aiStore.ts port (T128): it owns a struct that
//! carries the [`AiStoreState`] and implements the trait. The store's
//! `create(initialState)` initial values are that task's file, not this
//! one's — [`AiStoreState`]'s `Default` is just the per-type zero
//! (`None`/`false`/empty + [`initial_nl2sql`]), a construction
//! convenience that makes no claim about React's initial values.
//!
//! ## Dependency inventory
//!
//! | React import | Rust side | decision |
//! |---|---|---|
//! | `ai/targetClaim` (`AiTargetClaim`, `CancelReceipt`) | `crate::ai_streaming`'s lean mirrors (T088 defined them for streaming.ts's imports of the same file) | **shared lean mirror** — imported from there, not forked; both modules' docs name the sharing, and the real `ai/targetClaim.ts` port (cross-segment, not started) will extend those definitions in place |
//! | `types/ai` barrel: `AiAgentMode`, `AiChatContextSnapshot`, `AiChatMessage`, `AiChatSession`, `AiMemoryReference`, `AiModelProfile`, `AiProviderConfig`, `AiSessionSummary`, `AiSettingsConfig`, `DiagnosisResult`, `ExplainAnalysis`, `ModelInfo`, `ProviderListItem`, `StreamChunkPayload` | [`crate::ai_types`] (T042) | direct `use` — the whole point of the barrel |
//! | `types` `FilterCondition` | [`crate::filter_types`] (T188) | direct `use` |
//! | `types/index.ts` shapes not ported yet: `ConnectionDiagnosis`, `QueryAnalysis`, `McpServerConfig`, `McpClientStatus`, `McpToolInfo`, `WorkflowListItem`, `WorkflowExecutionResult` (plus the nested `ConnectionSolution`, `QueryCategory`, `McpServerTransport`, `WorkflowVariable`, `StepExecutionResult`, `StepStatus`) | nothing yet | **lean mirrors here** — field-complete, each one's doc names the owning task (the `types/index.ts` port, T213) and the unification path (the T040→`ai_types` precedent: single source, the other side re-exports). `platform_shared`'s `WebMcpServerConfig` is mcpClient.ts's wire shape — a **different** type, deliberately not used |
//! | the inline `'json' \| 'markdown'` of `exportChatSession` | [`crate::ai_sessions::ExportFormat`] (T040) | reuse — the same vocabulary, already ported for the same action |
//!
//! The inline string-literal unions of `generateSql` and
//! `sendChatMessage` (`memoryMode?: 'off' \| 'ask' \| 'auto'`,
//! `memoryScope?: 'organization' \| 'user' \| …`) become the enums
//! [`AiMemoryMode`] and [`AiMemoryScope`] per the crate's standing rule.
//! The same unions repeat in `commands/ai.ts` and the AI panels, so those
//! ports should re-use these enums rather than fork the literals. Note the
//! near-miss: [`crate::ai_types::AiMemoryScopeKind`] is the memory
//! **entry**'s scope-column vocabulary (eight literals, including `org`);
//! [`AiMemoryScope`] is the **run**'s scope filter — the seven-literal
//! subset the panels pass, without `org`.
//!
//! ## Serde ruling
//!
//! The file is a **store-internal shape**, not a wire protocol — the
//! zustand store never serializes its own state. The ruling, applied
//! consistently across the two precedents this crate already set:
//!
//! - [`Nl2SqlState`], [`AiStoreState`], the action param structs and
//!   [`MemoryCandidateNotice`] are in-process shapes → **no serde
//!   derives** — the same call `ai_types::chat::AiChatSession` and
//!   `ai_streaming`'s lean mirrors made ("not a wire type; the wire
//!   profile is whoever serializes it, if anyone").
//! - The lean mirrors of `types/index.ts` shapes that cross the commands
//!   boundary in React (connection diagnosis, query analysis, the MCP
//!   client config/status/tool list, the workflow list/execution result)
//!   **are** wire types → serde derives with camelCase keys and
//!   `#[serde(default, skip_serializing_if = "Option::is_none")]`
//!   optionals, the [`crate::filter_types`] / [`crate::ai_types`]
//!   precedent.
//! - Vocabulary enums ([`AiMemoryMode`], [`AiMemoryScope`],
//!   [`McpServerTransport`], [`StepStatus`]) derive serde and pin their
//!   literals through `as_str` — a string-literal union is a wire
//!   spelling the commands port will send.
//! - `Record<string, string>` follows the split: wire mirrors take
//!   `BTreeMap` (deterministic serialization — the `ai_types` rule);
//!   the store-internal `mcpServerErrors` takes `HashMap` (never
//!   serialized, read by key).
//!
//! ## Await → Result
//!
//! Every React `Promise<T>` action returns `Result<T, String>` — `Err`
//! is the rejection's display message, the T040/T088 precedent.
//! `Promise<T | null>` folds the null into `Option` **inside** the
//! `Ok`: `Promise<CancelReceipt | null>` →
//! `Result<Option<CancelReceipt>, String>` (the T088 adapter resolves
//! the same value as a bare `Option<CancelReceipt>` because
//! streaming.ts's promise never rejects — the store-level contract keeps
//! the uniform channel, and T128's implementer folds the transport
//! failure into `Ok(None)`). Sync actions stay sync.
//! `setupEventListeners`' `Promise<() => void>` resolves with the
//! unmount cleanup function, spelled [`EventListenerTeardown`].
//!
//! ## Cross-references
//!
//! - The T088 `ai_streaming` sibling anticipated this module: its
//!   `Nl2sqlStreamingSlice` is exactly this file's [`Nl2SqlState`] minus
//!   the `input` field (the six streaming-relevant fields align name for
//!   name — pinned by a test below), and its `AiStoreView` trait is the
//!   lean read/write view the streaming adapter needs. T128 implements
//!   [`AiStoreActions`] over the full [`AiStoreState`] and merges writes
//!   into both halves the way React's `{ ...nl2sql, … }` spread does.
//! - The file's tail `export type { AiChatMessage, AiChatSession }`
//!   (store consumers importing from the aiStore path) ports as this
//!   module's `pub use` re-export of the two `ai_types` names; the crate
//!   root already carries them from `ai_types`, so only this module's
//!   new names ride the root `pub use`.
//! - No production implementer exists yet (T128 lands it). The
//!   crate-root re-exports keep every name live for the dead-code lint
//!   the same way `ai_streaming`'s seam traits are live, so no
//!   `#[allow(dead_code)]` allowance is needed here (the
//!   `components/chart/mod.rs` "production caller not landed yet"
//!   allowance pattern covers `pub(crate)` items, not re-exported ones).
//!
//! ## Argument passing
//!
//! An action whose React signature takes an inline object literal gets a
//! named param struct **by value** (the caller builds the literal at the
//! call site, exactly as React does); single named values are passed by
//! reference (`&str`, `Option<&str>`, `&AiSettingsConfig`,
//! `&[McpServerConfig]`, `&StreamChunkPayload`), matching the T088 seam
//! style. `Record<string, unknown>` stays the opaque
//! [`serde_json::Value`] bag per the `ai_types` rule.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ai_sessions::ExportFormat;
// Shared lean mirrors: T088's streaming port defined these two for its own
// import of `ai/targetClaim`; this module re-imports them instead of
// forking a second copy (see the dependency inventory).
use crate::ai_streaming::{AiTargetClaim, CancelReceipt};
use crate::ai_types::{
    AiAgentMode, AiChatContextSnapshot, AiMemoryReference, AiModelProfile, AiProviderConfig,
    AiSessionSummary, AiSettingsConfig, DiagnosisResult, ExplainAnalysis, ModelInfo,
    ProviderListItem, StreamChunkPayload,
};
use crate::filter_types::FilterCondition;

// React tail: `export type { AiChatMessage, AiChatSession }` — the path
// convenience for store consumers that import the message types from the
// aiStore path. The crate root already carries both names from `ai_types`,
// so inside this (private) module the `AiChatMessage` half has no
// non-test reader yet — the allowance documents exactly that, the same
// "production caller not landed yet" pattern the chart modules use.
#[allow(unused_imports)]
pub use crate::ai_types::{AiChatMessage, AiChatSession};

// ── Nl2SqlState + initialNl2Sql ────────────────────────────────────────────

/// React `Nl2SqlState` — the NL2SQL generation slice of the store.
///
/// The T088 streaming port holds the six streaming-relevant fields as
/// `ai_streaming::Nl2sqlStreamingSlice`; this is the full seven-field
/// shape (its doc already names this module as the owner). Not a wire
/// type — the zustand store is memory-only — so no serde derives.
#[derive(Clone, Debug, PartialEq)]
pub struct Nl2SqlState {
    /// The natural-language input the panel edits.
    pub input: String,
    /// Raw accumulated content during streaming (before extractSql
    /// cleanup).
    pub streaming_sql: String,
    /// Extracted SQL preview for display during streaming.
    pub streaming_preview: String,
    /// Final cleaned SQL after generation completes.
    pub generated_sql: String,
    /// Memories the host injected for the latest generation.
    pub memory_used: Vec<AiMemoryReference>,
    pub is_generating: bool,
    /// The in-flight generation's request id (`null` when idle).
    pub request_id: Option<String>,
}

/// React `initialNl2Sql` — the literal the store creates the slice with,
/// field for field.
pub fn initial_nl2sql() -> Nl2SqlState {
    Nl2SqlState {
        input: String::new(),
        streaming_sql: String::new(),
        streaming_preview: String::new(),
        generated_sql: String::new(),
        memory_used: Vec::new(),
        is_generating: false,
        request_id: None,
    }
}

impl Default for Nl2SqlState {
    fn default() -> Self {
        initial_nl2sql()
    }
}

// ── The inline string-literal unions of the send/generate params ──────────

/// React inline `memoryMode?: 'off' | 'ask' | 'auto'` — how the run may use
/// the memory store. Both `generateSql` and `sendChatMessage` carry it;
/// `commands/ai.ts` repeats the union, and its port should re-use this
/// enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AiMemoryMode {
    Off,
    Ask,
    Auto,
}

impl AiMemoryMode {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Ask => "ask",
            Self::Auto => "auto",
        }
    }
}

/// React inline `memoryScope?: 'organization' | 'user' | 'connection' |
/// 'database' | 'schema' | 'table' | 'column'` — the scope the run's
/// memory search is narrowed to. The **run** filter subset (seven
/// literals, no `org` — see the module docs for the
/// `ai_types::AiMemoryScopeKind` near-miss); `commands/ai.ts` repeats the
/// union, and its port should re-use this enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AiMemoryScope {
    Organization,
    User,
    Connection,
    Database,
    Schema,
    Table,
    Column,
}

impl AiMemoryScope {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Organization => "organization",
            Self::User => "user",
            Self::Connection => "connection",
            Self::Database => "database",
            Self::Schema => "schema",
            Self::Table => "table",
            Self::Column => "column",
        }
    }
}

// ── The state half's inline shapes ────────────────────────────────────────

/// React `memoryCandidateNotice: { requestId: string; count: number } |
/// null` — the anonymous inline shape, named per the module's
/// inline-object rule.
///
/// Non-blocking Ask-mode candidate notice; cleared by the user or a new
/// run. In-process shape — no serde (the ruling above).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryCandidateNotice {
    pub request_id: String,
    /// Count of candidate memories the host is about to inject.
    pub count: usize,
}

// ── Lean mirrors of the not-yet-ported `types/index.ts` shapes ─────────────
//
// Wire shapes (serde per the ruling) but not this task's to own: each
// mirror is field-complete against its React interface so the store
// state/params hold the real shape, and the owner is the `types/index.ts`
// port (T213). When that lands, it should absorb these definitions the
// way the T040 session mirrors were absorbed into `ai_types` — single
// source, this module re-exports.

/// React `ConnectionDiagnosis` (`types/index.ts`) — what the connection
/// diagnosis helper reports.
///
/// **Ownership**: the `types/index.ts` port (T213); when it lands it should
/// absorb this mirror (single source, this module re-exports).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionDiagnosis {
    pub diagnosis: String,
    pub possible_causes: Vec<String>,
    pub solutions: Vec<ConnectionSolution>,
    pub category: String,
}

/// React `ConnectionSolution` (`types/index.ts`) — one fix the diagnosis
/// suggests.
///
/// **Ownership**: T213 (see [`ConnectionDiagnosis`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionSolution {
    pub description: String,
    /// The shell command the user may run (`command?: string`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
}

/// React `QueryAnalysis` (`types/index.ts`) — the query history analysis
/// report.
///
/// **Ownership**: the `types/index.ts` port (T213); absorb there, re-export
/// here.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryAnalysis {
    pub summary: String,
    pub categories: Vec<QueryCategory>,
    pub insights: Vec<String>,
    pub frequent_tables: Vec<String>,
    pub recommendations: Vec<String>,
}

/// React `QueryCategory` (`types/index.ts`) — one clustered query family.
///
/// **Ownership**: T213 (see [`QueryAnalysis`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryCategory {
    pub name: String,
    /// How many history entries fell in this family.
    pub count: usize,
    pub examples: Vec<String>,
}

/// React `McpServerConfig.transport = 'stdio'` — the single-literal union
/// (an enum still pins it; a future transport literal lands as a variant).
///
/// **Ownership**: T213 (see [`McpServerConfig`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum McpServerTransport {
    Stdio,
}

impl McpServerTransport {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stdio => "stdio",
        }
    }
}

/// React `McpServerConfig` (`types/index.ts`) — one configured MCP client
/// server, as `saveMcpClientServers` persists it.
///
/// **Ownership**: the `types/index.ts` port (T213); absorb there,
/// re-export here. (Not `platform_shared`'s `WebMcpServerConfig` — that
/// is mcpClient.ts's wire shape for the *web* MCP host, a different type.)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerConfig {
    pub id: String,
    pub name: String,
    pub transport: McpServerTransport,
    /// The executable `stdio` transport launches (`command?: string`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    /// Environment for the spawned process. `Record<string, string>` →
    /// `BTreeMap` (name-keyed, never order-observed — the `ai_types`
    /// rule; sorted iteration is inert).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<BTreeMap<String, String>>,
    pub enabled: bool,
    /// When `false`, tools from this server are excluded from AI Chat;
    /// React's default is `true`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled_for_ai: Option<bool>,
}

/// React `McpClientStatus` (`types/index.ts`) — one connected client
/// server as the store's `mcpServers` list holds it.
///
/// **Ownership**: the `types/index.ts` port (T213); absorb there,
/// re-export here.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpClientStatus {
    pub server_id: String,
    pub server_name: String,
    /// Number of tools the connected server exposed.
    pub tools_count: usize,
}

/// React `McpToolInfo` (`types/index.ts`) — one MCP tool the chat's tool
/// picker lists.
///
/// **Ownership**: the `types/index.ts` port (T213); absorb there,
/// re-export here. (Not `platform_shared`'s `WebMcpToolInfo` — the web
/// host's wire shape, a different type.)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolInfo {
    pub server_id: String,
    pub server_name: String,
    pub tool_name: String,
    /// Qualified name (`mcp/server/tool`) the model sees.
    pub qualified_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The JSON-schema bag (`Record<string, unknown>` → opaque
    /// [`Value`], the `ai_types` rule).
    #[serde(default)]
    pub input_schema: Value,
}

/// React `WorkflowVariable` (`@datazen/backend-client` `types/workflow.ts`,
/// re-exported by `types/index.ts`) — one declared workflow variable.
///
/// **Ownership**: the `types/index.ts` port (T213); absorb there,
/// re-export here.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowVariable {
    pub name: String,
    /// `type: string` — `'string' | 'number' | 'connection'` plus
    /// whatever a definition adds; kept the open string React declares.
    /// (Rust needs `r#type` for the keyword; serde's camelCase rename
    /// keeps the wire key exactly `type`.)
    pub r#type: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
}

/// React `WorkflowListItem` (`@datazen/backend-client` via
/// `types/index.ts`) — one workflow of the store's `workflows` list.
///
/// **Ownership**: the `types/index.ts` port (T213); absorb there,
/// re-export here.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowListItem {
    pub id: String,
    pub name: String,
    pub description: String,
    pub variables: Vec<WorkflowVariable>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheduled: Option<bool>,
}

/// React `StepStatus = 'success' | 'failed' | 'skipped' | 'timed_out'`
/// (`@datazen/backend-client` `types/workflow.ts`).
///
/// **Ownership**: T213 (see [`WorkflowExecutionResult`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Success,
    Failed,
    Skipped,
    TimedOut,
}

impl StepStatus {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
            Self::TimedOut => "timed_out",
        }
    }
}

/// React `StepExecutionResult` (`@datazen/backend-client` via
/// `types/index.ts`) — one step's outcome inside an execution result.
///
/// **Ownership**: the `types/index.ts` port (T213); absorb there,
/// re-export here.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepExecutionResult {
    pub step_id: String,
    pub step_type: String,
    pub status: StepStatus,
    /// `result?: Record<string, unknown>` — opaque [`Value`] bag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// Step wall time.
    pub execution_time_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sql_executed: Option<String>,
}

/// React `WorkflowExecutionResult` (`@datazen/backend-client` via
/// `types/index.ts`) — the outcome `executeWorkflow` reports.
///
/// **Ownership**: the `types/index.ts` port (T213); when it lands it
/// should absorb this mirror (single source, this module re-exports).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowExecutionResult {
    pub success: bool,
    pub final_output: String,
    pub steps: Vec<StepExecutionResult>,
    /// Whole-run wall time.
    pub total_time_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

// ── The state half ────────────────────────────────────────────────────────

/// React `AiStore`'s state half — all 50 fields, in the file's declaration
/// order. The action half is [`AiStoreActions`]; the implementer is the
/// aiStore.ts port (T128).
///
/// In-process shape — no serde (the ruling above); `Record<string,
/// string>` (`mcpServerErrors`) takes `HashMap` because it is read by key
/// and never serialized. `Default` is the per-type zero, **not** a claim
/// about the zustand `create(initialState)` values — those live in
/// aiStore.ts (T128).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AiStoreState {
    // ── Provider config half ──
    pub config: Option<AiProviderConfig>,
    pub settings_config: Option<AiSettingsConfig>,
    pub is_configured: bool,
    pub providers: Vec<ProviderListItem>,
    pub config_loading: bool,
    pub config_error: Option<String>,
    pub validating: bool,
    pub saving: bool,

    // ── NL2SQL + diagnosis + explain half ──
    pub nl2sql: Nl2SqlState,
    pub nl2sql_error: Option<String>,
    pub diagnosis: Option<DiagnosisResult>,
    pub is_diagnosing: bool,
    pub diagnosis_error: Option<String>,
    pub explain_analysis: Option<ExplainAnalysis>,
    pub is_analyzing_explain: bool,
    pub explain_error: Option<String>,

    // ── Chat session half ──
    pub chat_session: Option<AiChatSession>,
    /// Sessions for the current connection/database, newest first.
    pub chat_sessions: Vec<AiSessionSummary>,
    pub session_history_loading: bool,
    pub session_history_error: Option<String>,
    /// Visible failure of legacy localStorage -> AppDb
    /// migration/fallback.
    pub session_migration_error: Option<String>,
    /// Count of legacy conversations migrated successfully on this load.
    pub session_migration_success: usize,
    /// Non-blocking Ask-mode candidate notice; cleared by the user or a
    /// new run.
    pub memory_candidate_notice: Option<MemoryCandidateNotice>,

    // ── NL filter half ──
    pub nl_filter_input: String,
    pub parsed_filters: Option<Vec<FilterCondition>>,
    pub is_parsing_filter: bool,
    pub nl_filter_error: Option<String>,

    // ── Remote model list ──
    pub remote_models: Vec<ModelInfo>,
    pub fetching_remote_models: bool,

    // ── Workflow chat ──
    pub workflow_chat: Option<AiChatSession>,

    // ── Workflow execution half ──
    pub workflows: Vec<WorkflowListItem>,
    pub workflows_loading: bool,
    pub workflow_execution_result: Option<WorkflowExecutionResult>,
    pub is_executing_workflow: bool,
    pub workflow_error: Option<String>,

    // ── Schema doc half ──
    pub schema_doc: Option<String>,
    pub is_generating_schema_doc: bool,
    pub schema_doc_error: Option<String>,

    // ── Connection diagnosis half ──
    pub connection_diagnosis: Option<ConnectionDiagnosis>,
    pub is_diagnosing_connection: bool,
    pub connection_diagnosis_error: Option<String>,

    // ── Query analysis half ──
    pub query_analysis: Option<QueryAnalysis>,
    pub is_analyzing_queries: bool,
    pub query_analysis_error: Option<String>,

    // ── MCP client half ──
    pub mcp_servers: Vec<McpClientStatus>,
    pub mcp_tools: Vec<McpToolInfo>,
    pub mcp_connecting: bool,
    /// The server a connect/disconnect is currently in flight for.
    pub mcp_connecting_server_id: Option<String>,
    pub mcp_error: Option<String>,
    /// Per-server error strings, keyed by server id.
    pub mcp_server_errors: HashMap<String, String>,
}

// ── The action half's param structs (React inline object literals) ────────

/// React `generateSql`'s params object.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GenerateSqlParams {
    pub db_session_id: String,
    pub database: String,
    /// The table the editor currently focuses on, if any.
    pub current_table: Option<String>,
    /// Recently run queries fed as context.
    pub recent_queries: Option<Vec<String>>,
    /// Context file names fed as context.
    pub context_files: Option<Vec<String>>,
    /// Context table names fed as context.
    pub context_tables: Option<Vec<String>>,
    /// Owning persisted connection, when known.
    pub connection_id: Option<String>,
    pub memory_mode: Option<AiMemoryMode>,
    pub memory_scope: Option<AiMemoryScope>,
}

/// React `diagnoseError`'s params object.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DiagnoseErrorParams {
    pub db_session_id: String,
    pub database: String,
    /// The failing statement.
    pub sql: String,
    /// The error text the driver reported.
    pub error_message: String,
}

/// React `analyzeExplain`'s params object.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AnalyzeExplainParams {
    pub db_session_id: String,
    /// The driver's EXPLAIN output.
    pub explain_output: String,
    /// The statement the plan explains.
    pub original_sql: String,
}

/// React `parseFilter`'s params object.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParseFilterParams {
    pub db_session_id: String,
    pub database: String,
    /// The table whose columns the filter targets.
    pub table: String,
}

/// React `sendChatMessage`'s params object — the largest one.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SendChatMessageParams {
    pub db_session_id: Option<String>,
    pub connection_id: Option<String>,
    pub database: Option<String>,
    /// The user's message text.
    pub content: String,
    /// Whether to attach the schema context.
    pub include_schema: Option<bool>,
    pub agent_mode: Option<AiAgentMode>,
    pub context_files: Option<Vec<String>>,
    pub context_tables: Option<Vec<String>>,
    /// Frozen editor/error/result snapshot for this turn.
    pub auto_context: Option<AiChatContextSnapshot>,
    /// Explicit Prompt template selection; omitted = database default.
    pub template_id: Option<String>,
    /// Database type used to pick a default Prompt template.
    pub database_type: Option<String>,
    /// Target this run claims; omitted = independent.
    pub target_claim: Option<AiTargetClaim>,
    pub memory_mode: Option<AiMemoryMode>,
    pub memory_scope: Option<AiMemoryScope>,
}

/// React `sendWorkflowChatMessage`'s params object.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SendWorkflowChatMessageParams {
    pub db_session_id: Option<String>,
    pub database: Option<String>,
    pub content: String,
    pub include_schema: Option<bool>,
    pub context_files: Option<Vec<String>>,
    pub context_tables: Option<Vec<String>>,
}

/// React `executeWorkflow`'s params object.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExecuteWorkflowParams {
    pub workflow_id: String,
    /// `variables: Record<string, unknown>` — opaque [`Value`] bag.
    pub variables: Value,
    pub connection_id: Option<String>,
}

/// React `generateSchemaDoc`'s params object.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GenerateSchemaDocParams {
    pub db_session_id: String,
    pub database: String,
}

/// React `diagnoseConnection`'s params object.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DiagnoseConnectionParams {
    pub connection_id: String,
    /// The failure text to diagnose.
    pub error_message: String,
}

/// React `analyzeQueries`'s params object.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AnalyzeQueriesParams {
    /// The session whose history to analyze; omitted = the active one.
    pub db_session_id: Option<String>,
}

/// React `callMcpTool`'s params object.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CallMcpToolParams {
    pub server_id: String,
    pub tool_name: String,
    /// `arguments: Record<string, unknown>` — opaque [`Value`] bag.
    pub arguments: Value,
}

/// React `setupEventListeners`' resolution value — the unmount cleanup
/// function `() => void` the promise hands back.
pub type EventListenerTeardown = Box<dyn FnOnce()>;

// ── The action half ──────────────────────────────────────────────────────

/// React `AiStore`'s action half — all 55 signatures, in the file's
/// declaration order.
///
/// **Ownership / implementer**: the aiStore.ts port (T128) — a store
/// struct carrying the [`AiStoreState`] implements this trait. No
/// production implementer exists yet; the crate-root re-export keeps the
/// trait live for the dead-code lint (see the module docs — no
/// `#[allow(dead_code)]` needed).
///
/// Every `Promise<T>` returns `Result<T, String>` (`Err` = the
/// rejection's display message — the T040/T088 precedent); `Promise<T |
/// null>` folds the null into `Option` inside the `Ok`; sync actions stay
/// sync. Argument-bag structs are passed by value; single named values
/// by reference (the module's argument-passing rule).
pub trait AiStoreActions {
    // ── Provider config ──
    /// `loadConfig()`.
    fn load_config(&mut self) -> Result<(), String>;
    /// `loadSettingsConfig()`.
    fn load_settings_config(&mut self) -> Result<(), String>;
    /// `saveSettingsConfig(config)`.
    fn save_settings_config(&mut self, config: &AiSettingsConfig) -> Result<bool, String>;
    /// `setActiveProfile(profileId)`.
    fn set_active_profile(&mut self, profile_id: &str) -> Result<bool, String>;
    /// `saveProfile(profile)`.
    fn save_profile(&mut self, profile: &AiModelProfile) -> Result<bool, String>;
    /// `deleteProfile(profileId)`.
    fn delete_profile(&mut self, profile_id: &str) -> Result<bool, String>;
    /// `loadProviders()`.
    fn load_providers(&mut self) -> Result<(), String>;
    /// `fetchRemoteModels(protocol, endpoint, apiKey)`.
    fn fetch_remote_models(
        &mut self,
        protocol: &str,
        endpoint: &str,
        api_key: &str,
    ) -> Result<Vec<ModelInfo>, String>;
    /// `validateConfig(config)`.
    fn validate_config(&mut self, config: &AiProviderConfig) -> Result<bool, String>;
    /// `saveConfig(config)`.
    fn save_config(&mut self, config: &AiProviderConfig) -> Result<bool, String>;
    /// `deleteConfig()`.
    fn delete_config(&mut self) -> Result<(), String>;
    /// `clearError()`.
    fn clear_error(&mut self);

    // ── NL2SQL ──
    /// `setNl2SqlInput(input)`.
    fn set_nl2sql_input(&mut self, input: &str);
    /// `generateSql(params)`.
    fn generate_sql(&mut self, params: GenerateSqlParams) -> Result<(), String>;
    /// `clearNl2Sql()`.
    fn clear_nl2sql(&mut self);

    // ── Diagnosis / explain ──
    /// `diagnoseError(params)`.
    fn diagnose_error(&mut self, params: DiagnoseErrorParams) -> Result<(), String>;
    /// `clearDiagnosis()`.
    fn clear_diagnosis(&mut self);
    /// `analyzeExplain(params)`.
    fn analyze_explain(&mut self, params: AnalyzeExplainParams) -> Result<(), String>;
    /// `clearExplainAnalysis()`.
    fn clear_explain_analysis(&mut self);

    // ── NL filter ──
    /// `setNlFilterInput(input)`.
    fn set_nl_filter_input(&mut self, input: &str);
    /// `parseFilter(params)` — the parsed conditions, or `null` when the
    /// input parses to no filter.
    fn parse_filter(
        &mut self,
        params: ParseFilterParams,
    ) -> Result<Option<Vec<FilterCondition>>, String>;
    /// `clearNlFilter()`.
    fn clear_nl_filter(&mut self);

    // ── Chat sessions ──
    /// `initChatSession(dbSessionId?, database?, connectionId?)`.
    fn init_chat_session(
        &mut self,
        db_session_id: Option<&str>,
        database: Option<&str>,
        connection_id: Option<&str>,
    ) -> Result<(), String>;
    /// `refreshChatSessions()`.
    fn refresh_chat_sessions(&mut self) -> Result<(), String>;
    /// `selectChatSession(id)`.
    fn select_chat_session(&mut self, id: &str) -> Result<(), String>;
    /// `newChatSession()`.
    fn new_chat_session(&mut self);
    /// `deleteChatSession(id)`.
    fn delete_chat_session(&mut self, id: &str) -> Result<(), String>;
    /// `exportChatSession(format)` — the format is the T040
    /// `ai_sessions::ExportFormat` for the same `'json' | 'markdown'`
    /// union.
    fn export_chat_session(&mut self, format: ExportFormat) -> Result<(), String>;
    /// `sendChatMessage(params)`.
    fn send_chat_message(&mut self, params: SendChatMessageParams) -> Result<(), String>;
    /// `clearChat()`.
    fn clear_chat(&mut self);
    /// `dismissMemoryCandidateNotice()`.
    fn dismiss_memory_candidate_notice(&mut self);
    /// `cancelChat(requestId?)` — cancel the in-flight chat run; resolves
    /// with the host receipt (or `null`). The shared receipt shape comes
    /// from `ai_streaming` (T088's lean mirror of `ai/targetClaim`).
    fn cancel_chat(
        &mut self,
        request_id: Option<&str>,
    ) -> Result<Option<CancelReceipt>, String>;
    /// `handleCancelTimeout(requestId)` — called by the session
    /// canceller's 8s fallback.
    fn handle_cancel_timeout(&mut self, request_id: &str);

    // ── Workflow chat ──
    /// `initWorkflowChat()`.
    fn init_workflow_chat(&mut self);
    /// `sendWorkflowChatMessage(params)`.
    fn send_workflow_chat_message(
        &mut self,
        params: SendWorkflowChatMessageParams,
    ) -> Result<(), String>;
    /// `clearWorkflowChat()`.
    fn clear_workflow_chat(&mut self);

    // ── Stream wiring ──
    /// `handleStreamChunk(payload)` — the T088 streaming adapter's entry.
    fn handle_stream_chunk(&mut self, payload: &StreamChunkPayload);
    /// `setupEventListeners()` — resolves with the unmount cleanup
    /// function ([`EventListenerTeardown`]).
    fn setup_event_listeners(&mut self) -> Result<EventListenerTeardown, String>;

    // ── Workflows ──
    /// `loadWorkflows()`.
    fn load_workflows(&mut self) -> Result<(), String>;
    /// `executeWorkflow(params)`.
    fn execute_workflow(&mut self, params: ExecuteWorkflowParams) -> Result<(), String>;
    /// `clearWorkflowResult()`.
    fn clear_workflow_result(&mut self);

    // ── Schema doc / connection diagnosis / query analysis ──
    /// `generateSchemaDoc(params)`.
    fn generate_schema_doc(&mut self, params: GenerateSchemaDocParams) -> Result<(), String>;
    /// `clearSchemaDoc()`.
    fn clear_schema_doc(&mut self);
    /// `diagnoseConnection(params)`.
    fn diagnose_connection(&mut self, params: DiagnoseConnectionParams) -> Result<(), String>;
    /// `clearConnectionDiagnosis()`.
    fn clear_connection_diagnosis(&mut self);
    /// `analyzeQueries(params)`.
    fn analyze_queries(&mut self, params: AnalyzeQueriesParams) -> Result<(), String>;
    /// `clearQueryAnalysis()`.
    fn clear_query_analysis(&mut self);

    // ── MCP client ──
    /// `connectMcpServer(serverId)`.
    fn connect_mcp_server(&mut self, server_id: &str) -> Result<(), String>;
    /// `disconnectMcpServer(serverId)`.
    fn disconnect_mcp_server(&mut self, server_id: &str) -> Result<(), String>;
    /// `saveMcpClientServers(configs)`.
    fn save_mcp_client_servers(&mut self, configs: &[McpServerConfig]) -> Result<(), String>;
    /// `loadMcpServers()`.
    fn load_mcp_servers(&mut self) -> Result<(), String>;
    /// `loadMcpTools()`.
    fn load_mcp_tools(&mut self) -> Result<(), String>;
    /// `callMcpTool(params)` — resolves with the tool's text result.
    fn call_mcp_tool(&mut self, params: CallMcpToolParams) -> Result<String, String>;
    /// `clearMcpError()`.
    fn clear_mcp_error(&mut self);
    /// `clearMcpServerError(serverId)`.
    fn clear_mcp_server_error(&mut self, server_id: &str);
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::ai_streaming::Nl2sqlStreamingSlice;
    use crate::ai_types::{
        AiMessageRole, AiProviderType, DEFAULT_AI_MEMORY_SETTINGS,
    };

    /// Test decode helper: fails the test with the serde error instead of
    /// silently defaulting (the wire round-trips must be exact).
    fn decode<T: serde::de::DeserializeOwned>(wire: serde_json::Value) -> T {
        serde_json::from_value(wire)
            .unwrap_or_else(|error| panic!("wire decode failed: {error}"))
    }

    fn sample_provider_config() -> AiProviderConfig {
        AiProviderConfig {
            provider_type: AiProviderType::OpenAi,
            api_key: Some("sk-test".to_owned()),
            endpoint: None,
            model: "gpt-4o-mini".to_owned(),
            max_tokens: Some(4096),
            extra: None,
        }
    }

    fn sample_settings_config() -> AiSettingsConfig {
        AiSettingsConfig {
            active_profile_id: "profile-1".to_owned(),
            profiles: Vec::new(),
            global_instructions: String::new(),
            prompt_templates: Vec::new(),
            memory: DEFAULT_AI_MEMORY_SETTINGS,
            skills: None,
        }
    }

    // ── Nl2SqlState + initialNl2Sql ──

    /// The literal, field for field — every one of the seven fields at
    /// its React value.
    #[test]
    fn initial_nl2sql_pins_the_react_literal() {
        let initial = initial_nl2sql();
        assert_eq!(initial.input, "");
        assert_eq!(initial.streaming_sql, "");
        assert_eq!(initial.streaming_preview, "");
        assert_eq!(initial.generated_sql, "");
        assert_eq!(initial.memory_used, Vec::<AiMemoryReference>::new());
        assert!(!initial.is_generating);
        assert_eq!(initial.request_id, None);
    }

    /// `Default` and `initial_nl2sql()` are the same literal, so the
    /// derive-side and the constant-side cannot drift.
    #[test]
    fn initial_nl2sql_is_the_default() {
        assert_eq!(Nl2SqlState::default(), initial_nl2sql());
    }

    /// The T088 `Nl2sqlStreamingSlice` is this state's six
    /// streaming-relevant fields (`input` is the only one it lacks): this
    /// construction compiles only while the names stay aligned, and a
    /// populated state flows through it unchanged.
    #[test]
    fn nl2sql_state_covers_the_streaming_slice_field_for_field() {
        let state = initial_nl2sql();
        let slice = Nl2sqlStreamingSlice {
            request_id: state.request_id.clone(),
            streaming_sql: state.streaming_sql.clone(),
            streaming_preview: state.streaming_preview.clone(),
            generated_sql: state.generated_sql.clone(),
            memory_used: state.memory_used.clone(),
            is_generating: state.is_generating,
        };
        assert_eq!(slice, Nl2sqlStreamingSlice::default());

        let mut state = initial_nl2sql();
        state.request_id = Some("req-1".to_owned());
        state.streaming_sql = "select 1".to_owned();
        state.is_generating = true;
        let slice = Nl2sqlStreamingSlice {
            request_id: state.request_id,
            streaming_sql: state.streaming_sql,
            streaming_preview: state.streaming_preview,
            generated_sql: state.generated_sql,
            memory_used: state.memory_used,
            is_generating: state.is_generating,
        };
        assert_eq!(slice.request_id.as_deref(), Some("req-1"));
        assert_eq!(slice.streaming_sql, "select 1");
        assert!(slice.is_generating);
    }

    // ── The state half ──

    /// All 50 fields construct in one literal (the compile itself is the
    /// completeness pin — a dropped or renamed field fails to build), and
    /// the key field types hold their values.
    #[test]
    fn ai_store_state_constructs_all_fifty_fields() {
        let mut mcp_server_errors = HashMap::new();
        mcp_server_errors.insert("srv-1".to_owned(), "connection refused".to_owned());
        let state = AiStoreState {
            config: Some(sample_provider_config()),
            settings_config: Some(sample_settings_config()),
            is_configured: true,
            providers: vec![ProviderListItem {
                provider_type: AiProviderType::OpenAi,
                display_name: "OpenAI".to_owned(),
                supports_streaming: true,
                supports_tools: true,
                default_endpoint: "https://api.openai.com/v1".to_owned(),
                default_protocol: "openai".to_owned(),
                default_model: Some("gpt-4o-mini".to_owned()),
            }],
            config_loading: false,
            config_error: None,
            validating: false,
            saving: false,
            nl2sql: initial_nl2sql(),
            nl2sql_error: None,
            diagnosis: Some(DiagnosisResult {
                explanation: "Full scan on orders.".to_owned(),
                suggested_sql: None,
                changes: Vec::new(),
            }),
            is_diagnosing: false,
            diagnosis_error: None,
            explain_analysis: Some(ExplainAnalysis {
                summary: "Sort spills.".to_owned(),
                bottlenecks: Vec::new(),
                suggestions: Vec::new(),
            }),
            is_analyzing_explain: false,
            explain_error: None,
            chat_session: Some(AiChatSession::default()),
            chat_sessions: vec![AiSessionSummary {
                id: "s-1".to_owned(),
                session_key: "local/conn/db/s-1".to_owned(),
                connection_id: None,
                database: None,
                title: "Queries".to_owned(),
                metadata: None,
                message_count: 2,
                preview: "select 1".to_owned(),
                created_at: "2026-11-18T00:00:00Z".to_owned(),
                updated_at: "2026-11-18T00:00:00Z".to_owned(),
                last_access_at: "2026-11-18T00:00:00Z".to_owned(),
            }],
            session_history_loading: false,
            session_history_error: None,
            session_migration_error: None,
            session_migration_success: 3,
            memory_candidate_notice: Some(MemoryCandidateNotice {
                request_id: "req-2".to_owned(),
                count: 4,
            }),
            nl_filter_input: "city is Paris".to_owned(),
            parsed_filters: Some(vec![FilterCondition {
                column: "city".to_owned(),
                operator: crate::filter_types::FilterOperator::Eq,
                value: Some(serde_json::json!("Paris")),
                connector: None,
            }]),
            is_parsing_filter: false,
            nl_filter_error: None,
            remote_models: vec![ModelInfo {
                id: "gpt-4o-mini".to_owned(),
                display_name: "GPT-4o mini".to_owned(),
                context_window: 128_000,
                supports_streaming: true,
                supports_tools: true,
            }],
            fetching_remote_models: false,
            workflow_chat: None,
            workflows: vec![WorkflowListItem {
                id: "wf-1".to_owned(),
                name: "Nightly cleanup".to_owned(),
                description: "Deletes stale rows.".to_owned(),
                variables: vec![WorkflowVariable {
                    name: "days".to_owned(),
                    r#type: "number".to_owned(),
                    description: "Retention window".to_owned(),
                    required: Some(true),
                    default: Some(serde_json::json!(30)),
                }],
                scheduled: Some(true),
            }],
            workflows_loading: false,
            workflow_execution_result: Some(WorkflowExecutionResult {
                success: true,
                final_output: "42 rows deleted".to_owned(),
                steps: vec![StepExecutionResult {
                    step_id: "step-1".to_owned(),
                    step_type: "query".to_owned(),
                    status: StepStatus::TimedOut,
                    result: None,
                    execution_time_ms: 1200,
                    error: None,
                    connection_name: Some("prod".to_owned()),
                    sql_executed: None,
                }],
                total_time_ms: 1200,
                error: None,
            }),
            is_executing_workflow: false,
            workflow_error: None,
            schema_doc: Some("# Schema".to_owned()),
            is_generating_schema_doc: false,
            schema_doc_error: None,
            connection_diagnosis: Some(ConnectionDiagnosis {
                diagnosis: "The server is not listening.".to_owned(),
                possible_causes: vec!["Postgres is down.".to_owned()],
                solutions: vec![ConnectionSolution {
                    description: "Start the server.".to_owned(),
                    command: Some("systemctl start postgresql".to_owned()),
                }],
                category: "connect_refused".to_owned(),
            }),
            is_diagnosing_connection: false,
            connection_diagnosis_error: None,
            query_analysis: Some(QueryAnalysis {
                summary: "Read-heavy workload.".to_owned(),
                categories: vec![QueryCategory {
                    name: "point lookups".to_owned(),
                    count: 12,
                    examples: vec!["select * from t where id = ?".to_owned()],
                }],
                insights: vec!["Missing index on t.id.".to_owned()],
                frequent_tables: vec!["t".to_owned()],
                recommendations: vec!["Add the index.".to_owned()],
            }),
            is_analyzing_queries: false,
            query_analysis_error: None,
            mcp_servers: vec![McpClientStatus {
                server_id: "srv-1".to_owned(),
                server_name: "filesystem".to_owned(),
                tools_count: 2,
            }],
            mcp_tools: vec![McpToolInfo {
                server_id: "srv-1".to_owned(),
                server_name: "filesystem".to_owned(),
                tool_name: "read_file".to_owned(),
                qualified_name: "mcp/srv-1/read_file".to_owned(),
                description: Some("Reads a file.".to_owned()),
                input_schema: serde_json::json!({ "type": "object" }),
            }],
            mcp_connecting: true,
            mcp_connecting_server_id: Some("srv-1".to_owned()),
            mcp_error: None,
            mcp_server_errors,
        };

        // Spot-check the typed half the panels branch on.
        assert!(state.config.as_ref().unwrap().api_key.is_some());
        assert_eq!(state.memory_candidate_notice.as_ref().unwrap().count, 4);
        assert_eq!(state.session_migration_success, 3);
        assert_eq!(
            state.mcp_server_errors.get("srv-1").map(String::as_str),
            Some("connection refused")
        );
        assert_eq!(
            state.workflow_execution_result.as_ref().unwrap().steps[0].status,
            StepStatus::TimedOut
        );
        assert!(state.chat_sessions[0].message_count == 2);
        let _notice: MemoryCandidateNotice = state.memory_candidate_notice.unwrap();
        let _: Vec<McpClientStatus> = state.mcp_servers;
        let _errors: HashMap<String, String> = state.mcp_server_errors;
    }

    /// `Default` is the per-type zero — the doc's claim, pinned so a
    /// future derive change cannot quietly invent initial values.
    #[test]
    fn ai_store_state_default_is_the_type_zeros() {
        let state = AiStoreState::default();
        assert_eq!(state.config, None);
        assert_eq!(state.settings_config, None);
        assert!(!state.is_configured);
        assert!(state.providers.is_empty());
        assert_eq!(state.nl2sql, initial_nl2sql());
        assert_eq!(state.chat_session, None);
        assert!(state.chat_sessions.is_empty());
        assert_eq!(state.session_migration_success, 0);
        assert_eq!(state.memory_candidate_notice, None);
        assert_eq!(state.nl_filter_input, "");
        assert_eq!(state.parsed_filters, None);
        assert_eq!(state.workflow_chat, None);
        assert_eq!(state.schema_doc, None);
        assert_eq!(state.mcp_connecting_server_id, None);
        assert!(state.mcp_server_errors.is_empty());
    }

    // ── The inline string-literal unions ──

    #[test]
    fn memory_modes_keep_the_react_literals() {
        let table = [
            (AiMemoryMode::Off, "off"),
            (AiMemoryMode::Ask, "ask"),
            (AiMemoryMode::Auto, "auto"),
        ];
        for (value, literal) in table {
            assert_eq!(value.as_str(), literal);
            let parsed: AiMemoryMode = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(
                serde_json::to_value(parsed).ok(),
                Some(serde_json::json!(literal))
            );
        }
    }

    #[test]
    fn memory_scopes_keep_the_react_literals() {
        // The seven-literal run filter, in the React union's order — no
        // `org` (that one belongs to `ai_types::AiMemoryScopeKind`).
        let table = [
            (AiMemoryScope::Organization, "organization"),
            (AiMemoryScope::User, "user"),
            (AiMemoryScope::Connection, "connection"),
            (AiMemoryScope::Database, "database"),
            (AiMemoryScope::Schema, "schema"),
            (AiMemoryScope::Table, "table"),
            (AiMemoryScope::Column, "column"),
        ];
        for (value, literal) in table {
            assert_eq!(value.as_str(), literal);
            let parsed: AiMemoryScope = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(
                serde_json::to_value(parsed).ok(),
                Some(serde_json::json!(literal))
            );
        }
    }

    // ── T126 audit round: near-miss rejection pins ──
    //
    // The accept tables above only prove the seven/three React literals
    // decode. They stay green while the union is *widened* — an extra
    // variant (the T234 lesson: a widened literal table decodes its new
    // spellings all the same). The React unions are closed, so the
    // near-miss spellings must be pinned to *fail*.

    /// The literal unions of `generateSql`/`sendChatMessage`
    /// (`types.ts` 112/174: `'off' | 'ask' | 'auto'`) are closed at
    /// three values: the plausible widen spellings must not decode.
    /// Verified blind before this round: adding a variant survived
    /// every existing test.
    #[test]
    fn memory_mode_rejects_the_widen_near_misses() {
        for near_miss in ["on", "always", "manual", "none", "off "] {
            let err = serde_json::from_value::<AiMemoryMode>(serde_json::json!(near_miss));
            assert!(err.is_err(), "AiMemoryMode decoded near-miss {near_miss:?}");
        }
        // The scope union's own literals must not leak into the mode
        // union either (the two enums share a param object).
        let err = serde_json::from_value::<AiMemoryMode>(serde_json::json!("organization"));
        assert!(err.is_err());
    }

    /// `types.ts` 113-120/175-182: the run scope union is the
    /// **seven-literal subset without `org`** — the module docs name the
    /// near-miss (`ai_types::AiMemoryScopeKind`'s eighth literal). A
    /// variant copy-paste from that enum must be caught: `"org"` is the
    /// one spelling the widened table would decode. Verified blind
    /// before this round (the `Org` variant survived every test).
    #[test]
    fn memory_scope_rejects_the_widen_near_misses() {
        for near_miss in ["org", "db", "global", "workspace"] {
            let err = serde_json::from_value::<AiMemoryScope>(serde_json::json!(near_miss));
            assert!(err.is_err(), "AiMemoryScope decoded near-miss {near_miss:?}");
        }
        // The mode union's literals must not leak into the scope union.
        let err = serde_json::from_value::<AiMemoryScope>(serde_json::json!("ask"));
        assert!(err.is_err());
    }

    /// `types.ts` 156: `exportChatSession(format: 'json' | 'markdown')`
    /// re-uses T040's `ai_sessions::ExportFormat` — an in-process enum
    /// with **no serde derives** (the T040 ruling; the format never
    /// crosses a wire), whose two spellings are already pinned by that
    /// module's own `as_str` table (`ai_sessions/model.rs` 514-519).
    /// This module's surface is the reference itself — the compile is
    /// the pin, and an enum with no wire face has no observable widen
    /// seam here (equivalence exemption, noted in the audit report).

    /// `types/index.ts` 164: `StepStatus = 'success' | 'failed' |
    /// 'skipped' | 'timed_out'`. Only `timed_out`/`failed` had wire
    /// coverage before this round — a `Success` → `Succeeded` rename
    /// survived every test (verified by mutation). This table pins all
    /// four literals both ways, plus the widened-rename near-misses.
    #[test]
    fn step_statuses_keep_the_react_literals() {
        let table = [
            (StepStatus::Success, "success"),
            (StepStatus::Failed, "failed"),
            (StepStatus::Skipped, "skipped"),
            (StepStatus::TimedOut, "timed_out"),
        ];
        for (value, literal) in table {
            assert_eq!(value.as_str(), literal);
            let parsed: StepStatus = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(
                serde_json::to_value(parsed).ok(),
                Some(serde_json::json!(literal))
            );
        }
    }

    #[test]
    fn step_status_rejects_the_widen_near_misses() {
        for near_miss in [
            "timeout",     // the un-snaked spelling
            "timedout",    // the joined spelling
            "succeeded",   // the rename drift of `Success`
            "running",
            "cancelled",
        ] {
            let err = serde_json::from_value::<StepStatus>(serde_json::json!(near_miss));
            assert!(err.is_err(), "StepStatus decoded near-miss {near_miss:?}");
        }
    }

    /// `types/index.ts` 513: `transport: 'stdio'` — a single-literal
    /// union. Widening it (the web host's `'http'`, the classic `'sse'`)
    /// or a case drift must not decode: verified blind before this
    /// round (adding an `Sse` variant survived every test).
    #[test]
    fn mcp_server_transport_rejects_the_widen_near_misses() {
        for near_miss in ["sse", "http", "Stdio", "STDIO"] {
            let err =
                serde_json::from_value::<McpServerTransport>(serde_json::json!(near_miss));
            assert!(err.is_err(), "McpServerTransport decoded near-miss {near_miss:?}");
        }
    }

    // ── T126 audit round: Option serde semantics ──

    /// `types/index.ts` 430-433 `ConnectionSolution.command?: string` —
    /// the module ruling's "missing key = None vs explicit null"
    /// distinction: both spellings fold to `None`, and the serializer
    /// drops the key either way (round-trips to the missing-key wire).
    /// Without this pin a `#[serde(default)]` removal only breaks the
    /// missing-key half, leaving the explicit-null half unpinned.
    #[test]
    fn wire_optionals_fold_explicit_null_like_a_missing_key() {
        let with_key = serde_json::json!({
            "diagnosis": "refused",
            "possibleCauses": [],
            "solutions": [
                { "description": "Start it.", "command": null }
            ],
            "category": "connect_refused"
        });
        let solution: ConnectionDiagnosis = decode(with_key.clone());
        assert_eq!(solution.solutions[0].command, None);
        // Null folds to None, and None skips — the explicit-null wire
        // round-trips to the missing-key spelling.
        let round = serde_json::to_value(&solution).unwrap();
        assert_eq!(round, serde_json::json!({
            "diagnosis": "refused",
            "possibleCauses": [],
            "solutions": [ { "description": "Start it." } ],
            "category": "connect_refused"
        }));
        assert_ne!(round, with_key);

        // The same fold for `McpServerConfig.enabledForAi?`
        // (`types/index.ts` 519) and `WorkflowListItem.scheduled?`
        // (backend-client workflow.ts 161): explicit null ≡ missing.
        let config: McpServerConfig = decode(serde_json::json!({
            "id": "srv-1", "name": "n", "transport": "stdio",
            "command": null, "enabled": true, "enabledForAi": null
        }));
        assert_eq!(config.command, None);
        assert_eq!(config.enabled_for_ai, None);
        let item: WorkflowListItem = decode(serde_json::json!({
            "id": "wf-1", "name": "n", "description": "d",
            "variables": [], "scheduled": null
        }));
        assert_eq!(item.scheduled, None);
    }

    /// `McpToolInfo.inputSchema` (`types/index.ts` 554) is **required**
    /// in React (`Record<string, unknown>`, no `?`), but this mirror
    /// decodes a missing key leniently via its explicit
    /// `#[serde(default)]`: the pin locks the committed behavior — the
    /// key may be absent on decode (folding to `Value::Null`), and the
    /// serializer always emits the key (no skip annotation, so `Null`
    /// serializes as `"inputSchema": null`). Removing the `default`
    /// silently (verified surviving every test by mutation) would
    /// change this contract unannounced.
    #[test]
    fn mcp_tool_input_schema_is_leniently_defaulted_when_absent() {
        let without_key = serde_json::json!({
            "serverId": "srv-1",
            "serverName": "filesystem",
            "toolName": "read_file",
            "qualifiedName": "mcp/srv-1/read_file"
        });
        let tool: McpToolInfo = decode(without_key);
        assert_eq!(tool.input_schema, serde_json::Value::Null);
        // The two annotation strategies side by side, pinned: the
        // `Option` half (`description`) **skips** its key when `None`
        // (its `skip_serializing_if`), while the defaulted `Value` half
        // (`input_schema`, no skip) always emits the key — `null` when
        // the default fired.
        let round = serde_json::to_value(&tool).unwrap();
        assert_eq!(
            round,
            serde_json::json!({
                "serverId": "srv-1",
                "serverName": "filesystem",
                "toolName": "read_file",
                "qualifiedName": "mcp/srv-1/read_file",
                "inputSchema": null
            })
        );
        assert!(!round.as_object().unwrap().contains_key("description"));
    }

    // ── T126 audit round: number-fold and container-choice pins ──

    /// React `number` folds to unsigned integers on the wire mirrors
    /// (`McpClientStatus.toolsCount` — `types/index.ts` 530;
    /// `StepExecutionResult.executionTimeMs` / `WorkflowExecutionResult
    /// .totalTimeMs` — workflow.ts 171/181). The fold is a decision,
    /// not an accident: negative and fractional counts/times must be
    /// rejected (a `f64`/`i64` fold would accept them).
    #[test]
    fn unsigned_number_folds_reject_negative_and_fractional() {
        let base = serde_json::json!({
            "serverId": "srv-1", "serverName": "n", "toolsCount": 1
        });
        let mut negative = base.clone();
        negative["toolsCount"] = serde_json::json!(-1);
        assert!(serde_json::from_value::<McpClientStatus>(negative).is_err());
        let mut fractional = base;
        fractional["toolsCount"] = serde_json::json!(1.5);
        assert!(serde_json::from_value::<McpClientStatus>(fractional).is_err());

        let step = serde_json::json!({
            "stepId": "s", "stepType": "query", "status": "success",
            "executionTimeMs": -5
        });
        assert!(serde_json::from_value::<StepExecutionResult>(step).is_err());
        let run = serde_json::json!({
            "success": true, "finalOutput": "", "steps": [],
            "totalTimeMs": -1
        });
        assert!(serde_json::from_value::<WorkflowExecutionResult>(run).is_err());
    }

    /// `McpServerConfig.env?: Record<string, string>`
    /// (`types/index.ts` 516) takes `BTreeMap` — the deterministic
    /// serialization rule the module docs inherit from `ai_types`
    /// (contrasted with the store-internal `mcpServerErrors` taking
    /// `HashMap`). A `HashMap` swap survives every round-trip (verified
    /// by mutation — single-key wires cannot observe key order), so the
    /// container choice is pinned as a type identity here, next to the
    /// `mcp_server_errors` `HashMap` pin above.
    #[test]
    fn mcp_server_env_is_the_btreemap_wire_choice() {
        // Type identity first: the field is the deterministic-serialization
        // container the ai_types rule prescribes for wire `Record`s (the
        // store-internal `mcpServerErrors` is the HashMap counterpart).
        let config = McpServerConfig {
            id: "srv-1".to_owned(),
            name: "n".to_owned(),
            transport: McpServerTransport::Stdio,
            command: None,
            args: None,
            env: Some(BTreeMap::from([("NO_COLOR".to_owned(), "1".to_owned())])),
            enabled: true,
            enabled_for_ai: None,
        };
        let env: std::collections::BTreeMap<String, String> = config.env.clone().unwrap();
        assert_eq!(env.get("NO_COLOR").map(String::as_str), Some("1"));
        // Deterministic order on the wire: a two-key env serializes its
        // keys sorted, whatever the insertion order was (a HashMap swap
        // here survives every round-trip — verified by mutation).
        let mut sorted_env = BTreeMap::new();
        sorted_env.insert("Z".to_owned(), "z".to_owned());
        sorted_env.insert("A".to_owned(), "a".to_owned());
        let config = McpServerConfig {
            env: Some(sorted_env),
            ..config
        };
        let wire = serde_json::to_value(&config).unwrap();
        let keys: Vec<&str> = wire["env"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, vec!["A", "Z"]);
    }

    // ── T126 audit round: the crate-root re-export manifest ──

    /// React's file tail is `export type { AiChatMessage, AiChatSession
    /// }` — a re-export manifest. The Rust counterpart is this module's
    /// `pub use` **plus** the crate root's 32-name `pub use
    /// ai_store_types::{…}` (lib.rs 56-65). The root manifest is a
    /// list, and lists drift: deleting a name from it survives every
    /// test in this module (verified by mutation — these tests import
    /// through `super::*`, not the root). This pin resolves every name
    /// through the crate root, so a dropped or renamed root re-export
    /// stops the build.
    #[test]
    fn the_crate_root_re_exports_every_module_name() {
        // State + action halves.
        let state: crate::AiStoreState = crate::AiStoreState::default();
        assert_eq!(state.nl2sql, crate::initial_nl2sql());
        let _nl2sql: crate::Nl2SqlState = state.nl2sql;
        fn implements_actions<T: crate::AiStoreActions>() {}
        implements_actions::<DummyRootStore>();
        // The two inline unions.
        let _: crate::AiMemoryMode = crate::AiMemoryMode::Off;
        let _: crate::AiMemoryScope = crate::AiMemoryScope::Organization;
        // The inline state shape.
        let _: crate::MemoryCandidateNotice = crate::MemoryCandidateNotice {
            request_id: String::new(),
            count: 0,
        };
        // The lean mirrors (types/index.ts + backend-client workflow.ts).
        let _: crate::ConnectionDiagnosis = crate::ConnectionDiagnosis {
            diagnosis: String::new(),
            possible_causes: Vec::new(),
            solutions: Vec::new(),
            category: String::new(),
        };
        let _: crate::ConnectionSolution = crate::ConnectionSolution {
            description: String::new(),
            command: None,
        };
        let _: crate::QueryAnalysis = crate::QueryAnalysis {
            summary: String::new(),
            categories: Vec::new(),
            insights: Vec::new(),
            frequent_tables: Vec::new(),
            recommendations: Vec::new(),
        };
        let _: crate::QueryCategory = crate::QueryCategory {
            name: String::new(),
            count: 0,
            examples: Vec::new(),
        };
        let _: crate::McpServerConfig = crate::McpServerConfig {
            id: String::new(),
            name: String::new(),
            transport: crate::McpServerTransport::Stdio,
            command: None,
            args: None,
            env: None,
            enabled: true,
            enabled_for_ai: None,
        };
        let _: crate::McpClientStatus = crate::McpClientStatus {
            server_id: String::new(),
            server_name: String::new(),
            tools_count: 0,
        };
        let _: crate::McpToolInfo = crate::McpToolInfo {
            server_id: String::new(),
            server_name: String::new(),
            tool_name: String::new(),
            qualified_name: String::new(),
            description: None,
            input_schema: serde_json::Value::Null,
        };
        let _: crate::WorkflowListItem = crate::WorkflowListItem {
            id: String::new(),
            name: String::new(),
            description: String::new(),
            variables: Vec::new(),
            scheduled: None,
        };
        let _: crate::WorkflowVariable = crate::WorkflowVariable {
            name: String::new(),
            r#type: String::new(),
            description: String::new(),
            required: None,
            default: None,
        };
        let _: crate::WorkflowExecutionResult = crate::WorkflowExecutionResult {
            success: true,
            final_output: String::new(),
            steps: Vec::new(),
            total_time_ms: 0,
            error: None,
        };
        let _: crate::StepExecutionResult = crate::StepExecutionResult {
            step_id: String::new(),
            step_type: String::new(),
            status: crate::StepStatus::Success,
            result: None,
            execution_time_ms: 0,
            error: None,
            connection_name: None,
            sql_executed: None,
        };
        // The action param structs + the teardown alias.
        let _: crate::GenerateSqlParams = crate::GenerateSqlParams::default();
        let _: crate::DiagnoseErrorParams = crate::DiagnoseErrorParams::default();
        let _: crate::AnalyzeExplainParams = crate::AnalyzeExplainParams::default();
        let _: crate::ParseFilterParams = crate::ParseFilterParams::default();
        let _: crate::SendChatMessageParams = crate::SendChatMessageParams::default();
        let _: crate::SendWorkflowChatMessageParams =
            crate::SendWorkflowChatMessageParams::default();
        let _: crate::ExecuteWorkflowParams = crate::ExecuteWorkflowParams::default();
        let _: crate::GenerateSchemaDocParams = crate::GenerateSchemaDocParams::default();
        let _: crate::DiagnoseConnectionParams = crate::DiagnoseConnectionParams::default();
        let _: crate::AnalyzeQueriesParams = crate::AnalyzeQueriesParams::default();
        let _: crate::CallMcpToolParams = crate::CallMcpToolParams::default();
        let teardown: crate::EventListenerTeardown = Box::new(|| {});
        teardown();

        // The React tail's two message re-exports ride the root through
        // `ai_types` (already re-exported there); the module-level `pub
        // use` this file adds is the direct counterpart.
        let _: Option<crate::AiChatMessage> = None;
        let _: Option<crate::AiChatSession> = None;
    }

    /// A root-path-visible implementor for the manifest test's trait
    /// bound (the module-local fake above also implements it, but this
    /// one is reachable without `super::`).
    struct DummyRootStore;

    impl crate::AiStoreActions for DummyRootStore {
        fn load_config(&mut self) -> Result<(), String> {
            Ok(())
        }
        fn load_settings_config(&mut self) -> Result<(), String> {
            Ok(())
        }
        fn save_settings_config(&mut self, _config: &AiSettingsConfig) -> Result<bool, String> {
            Ok(true)
        }
        fn set_active_profile(&mut self, _profile_id: &str) -> Result<bool, String> {
            Ok(true)
        }
        fn save_profile(&mut self, _profile: &AiModelProfile) -> Result<bool, String> {
            Ok(true)
        }
        fn delete_profile(&mut self, _profile_id: &str) -> Result<bool, String> {
            Ok(true)
        }
        fn load_providers(&mut self) -> Result<(), String> {
            Ok(())
        }
        fn fetch_remote_models(
            &mut self,
            _protocol: &str,
            _endpoint: &str,
            _api_key: &str,
        ) -> Result<Vec<ModelInfo>, String> {
            Ok(Vec::new())
        }
        fn validate_config(&mut self, _config: &AiProviderConfig) -> Result<bool, String> {
            Ok(true)
        }
        fn save_config(&mut self, _config: &AiProviderConfig) -> Result<bool, String> {
            Ok(true)
        }
        fn delete_config(&mut self) -> Result<(), String> {
            Ok(())
        }
        fn clear_error(&mut self) {}
        fn set_nl2sql_input(&mut self, _input: &str) {}
        fn generate_sql(&mut self, _params: GenerateSqlParams) -> Result<(), String> {
            Ok(())
        }
        fn clear_nl2sql(&mut self) {}
        fn diagnose_error(&mut self, _params: DiagnoseErrorParams) -> Result<(), String> {
            Ok(())
        }
        fn clear_diagnosis(&mut self) {}
        fn analyze_explain(&mut self, _params: AnalyzeExplainParams) -> Result<(), String> {
            Ok(())
        }
        fn clear_explain_analysis(&mut self) {}
        fn set_nl_filter_input(&mut self, _input: &str) {}
        fn parse_filter(
            &mut self,
            _params: ParseFilterParams,
        ) -> Result<Option<Vec<FilterCondition>>, String> {
            Ok(None)
        }
        fn clear_nl_filter(&mut self) {}
        fn init_chat_session(
            &mut self,
            _db_session_id: Option<&str>,
            _database: Option<&str>,
            _connection_id: Option<&str>,
        ) -> Result<(), String> {
            Ok(())
        }
        fn refresh_chat_sessions(&mut self) -> Result<(), String> {
            Ok(())
        }
        fn select_chat_session(&mut self, _id: &str) -> Result<(), String> {
            Ok(())
        }
        fn new_chat_session(&mut self) {}
        fn delete_chat_session(&mut self, _id: &str) -> Result<(), String> {
            Ok(())
        }
        fn export_chat_session(&mut self, _format: ExportFormat) -> Result<(), String> {
            Ok(())
        }
        fn send_chat_message(&mut self, _params: SendChatMessageParams) -> Result<(), String> {
            Ok(())
        }
        fn clear_chat(&mut self) {}
        fn dismiss_memory_candidate_notice(&mut self) {}
        fn cancel_chat(
            &mut self,
            _request_id: Option<&str>,
        ) -> Result<Option<CancelReceipt>, String> {
            Ok(None)
        }
        fn handle_cancel_timeout(&mut self, _request_id: &str) {}
        fn init_workflow_chat(&mut self) {}
        fn send_workflow_chat_message(
            &mut self,
            _params: SendWorkflowChatMessageParams,
        ) -> Result<(), String> {
            Ok(())
        }
        fn clear_workflow_chat(&mut self) {}
        fn handle_stream_chunk(&mut self, _payload: &StreamChunkPayload) {}
        fn setup_event_listeners(&mut self) -> Result<EventListenerTeardown, String> {
            Ok(Box::new(|| {}))
        }
        fn load_workflows(&mut self) -> Result<(), String> {
            Ok(())
        }
        fn execute_workflow(&mut self, _params: ExecuteWorkflowParams) -> Result<(), String> {
            Ok(())
        }
        fn clear_workflow_result(&mut self) {}
        fn generate_schema_doc(
            &mut self,
            _params: GenerateSchemaDocParams,
        ) -> Result<(), String> {
            Ok(())
        }
        fn clear_schema_doc(&mut self) {}
        fn diagnose_connection(
            &mut self,
            _params: DiagnoseConnectionParams,
        ) -> Result<(), String> {
            Ok(())
        }
        fn clear_connection_diagnosis(&mut self) {}
        fn analyze_queries(&mut self, _params: AnalyzeQueriesParams) -> Result<(), String> {
            Ok(())
        }
        fn clear_query_analysis(&mut self) {}
        fn connect_mcp_server(&mut self, _server_id: &str) -> Result<(), String> {
            Ok(())
        }
        fn disconnect_mcp_server(&mut self, _server_id: &str) -> Result<(), String> {
            Ok(())
        }
        fn save_mcp_client_servers(&mut self, _configs: &[McpServerConfig]) -> Result<(), String> {
            Ok(())
        }
        fn load_mcp_servers(&mut self) -> Result<(), String> {
            Ok(())
        }
        fn load_mcp_tools(&mut self) -> Result<(), String> {
            Ok(())
        }
        fn call_mcp_tool(&mut self, _params: CallMcpToolParams) -> Result<String, String> {
            Ok(String::new())
        }
        fn clear_mcp_error(&mut self) {}
        fn clear_mcp_server_error(&mut self, _server_id: &str) {}
    }

    // ── Lean mirrors: wire round-trips (field names + camelCase pins) ──

    #[test]
    fn connection_diagnosis_round_trips_the_react_wire() {
        let wire = serde_json::json!({
            "diagnosis": "The connection was refused.",
            "possibleCauses": ["The server is not listening."],
            "solutions": [
                { "description": "Start the server.", "command": "systemctl start postgresql" },
                { "description": "Check the firewall." }
            ],
            "category": "connect_refused"
        });
        let diagnosis: ConnectionDiagnosis = decode(wire.clone());
        assert_eq!(diagnosis.category, "connect_refused");
        assert_eq!(diagnosis.solutions.len(), 2);
        assert_eq!(diagnosis.solutions[1].command, None);
        assert_eq!(serde_json::to_value(&diagnosis).ok(), Some(wire));
    }

    #[test]
    fn query_analysis_round_trips_the_react_wire() {
        let wire = serde_json::json!({
            "summary": "Read-heavy workload.",
            "categories": [
                { "name": "point lookups", "count": 12, "examples": ["select * from t where id = ?"] }
            ],
            "insights": ["Missing index on t.id."],
            "frequentTables": ["t"],
            "recommendations": ["Add the index."]
        });
        let analysis: QueryAnalysis = decode(wire.clone());
        assert_eq!(analysis.categories[0].count, 12);
        assert_eq!(serde_json::to_value(&analysis).ok(), Some(wire));
    }

    #[test]
    fn mcp_server_config_round_trips_the_react_wire() {
        let wire = serde_json::json!({
            "id": "srv-1",
            "name": "filesystem",
            "transport": "stdio",
            "command": "npx",
            "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"],
            "env": { "NO_COLOR": "1" },
            "enabled": true
        });
        let config: McpServerConfig = decode(wire.clone());
        assert_eq!(config.transport, McpServerTransport::Stdio);
        assert_eq!(config.transport.as_str(), "stdio");
        assert_eq!(config.enabled_for_ai, None);
        assert_eq!(
            config.env.as_ref().and_then(|env| env.get("NO_COLOR")).map(String::as_str),
            Some("1")
        );
        assert_eq!(serde_json::to_value(&config).ok(), Some(wire));
    }

    #[test]
    fn mcp_client_status_and_tool_info_round_trip_the_react_wire() {
        let status: McpClientStatus = decode(serde_json::json!({
            "serverId": "srv-1",
            "serverName": "filesystem",
            "toolsCount": 2
        }));
        assert_eq!(status.tools_count, 2);
        assert_eq!(
            serde_json::to_value(&status).ok(),
            Some(serde_json::json!({
                "serverId": "srv-1",
                "serverName": "filesystem",
                "toolsCount": 2
            }))
        );

        let wire = serde_json::json!({
            "serverId": "srv-1",
            "serverName": "filesystem",
            "toolName": "read_file",
            "qualifiedName": "mcp/srv-1/read_file",
            "description": "Reads a file.",
            "inputSchema": { "type": "object", "properties": {} }
        });
        let tool: McpToolInfo = decode(wire.clone());
        assert_eq!(tool.qualified_name, "mcp/srv-1/read_file");
        assert_eq!(serde_json::to_value(&tool).ok(), Some(wire));
    }

    #[test]
    fn workflow_list_item_round_trips_the_react_wire() {
        let wire = serde_json::json!({
            "id": "wf-1",
            "name": "Nightly cleanup",
            "description": "Deletes stale rows.",
            "variables": [
                {
                    "name": "days",
                    "type": "number",
                    "description": "Retention window",
                    "required": true,
                    "default": 30
                }
            ],
            "scheduled": true
        });
        let item: WorkflowListItem = decode(wire.clone());
        assert_eq!(item.variables[0].r#type, "number");
        assert_eq!(item.variables[0].default, Some(serde_json::json!(30)));
        assert_eq!(serde_json::to_value(&item).ok(), Some(wire));
    }

    #[test]
    fn workflow_execution_result_round_trips_the_react_wire() {
        let wire = serde_json::json!({
            "success": false,
            "finalOutput": "",
            "steps": [
                {
                    "stepId": "step-1",
                    "stepType": "query",
                    "status": "timed_out",
                    "executionTimeMs": 8000
                },
                {
                    "stepId": "step-2",
                    "stepType": "query",
                    "status": "failed",
                    "result": { "rows": 0 },
                    "executionTimeMs": 12,
                    "error": "syntax error",
                    "connectionName": "prod",
                    "sqlExecuted": "delete from t"
                }
            ],
            "totalTimeMs": 8012,
            "error": "step-1 timed out"
        });
        let result: WorkflowExecutionResult = decode(wire.clone());
        assert_eq!(result.steps[0].status, StepStatus::TimedOut);
        assert_eq!(result.steps[0].status.as_str(), "timed_out");
        assert_eq!(result.steps[1].status, StepStatus::Failed);
        assert_eq!(result.steps[1].connection_name.as_deref(), Some("prod"));
        assert_eq!(result.steps[1].sql_executed.as_deref(), Some("delete from t"));
        assert_eq!(serde_json::to_value(&result).ok(), Some(wire));
    }

    // ── The tail re-exports ──

    /// The React tail re-export keeps the message types importable from
    /// the aiStore path; the Rust spelling is this module's `pub use`
    /// (both names resolve here, and the crate root carries them from
    /// `ai_types`).
    #[test]
    fn the_tail_re_exports_resolve_from_this_module_path() {
        let message: AiChatMessage = AiChatMessage::new(AiMessageRole::User, "hi");
        assert_eq!(message.content, "hi");
        let session: AiChatSession = AiChatSession::default();
        assert!(session.messages.is_empty());
    }

    // ── The action half: param structs + a fake implementation ──

    /// Every action param struct constructs with all its fields named —
    /// the compile is the completeness pin; the optionals stay `None`
    /// exactly where React's `?:` leaves them unset.
    #[test]
    fn every_action_param_struct_constructs_field_for_field() {
        let generate = GenerateSqlParams {
            db_session_id: "dbs-1".to_owned(),
            database: "app".to_owned(),
            current_table: Some("orders".to_owned()),
            recent_queries: Some(vec!["select 1".to_owned()]),
            context_files: Some(vec!["notes.md".to_owned()]),
            context_tables: Some(vec!["orders".to_owned()]),
            connection_id: Some("conn-1".to_owned()),
            memory_mode: Some(AiMemoryMode::Ask),
            memory_scope: Some(AiMemoryScope::User),
        };
        assert_eq!(generate.memory_mode, Some(AiMemoryMode::Ask));

        let diagnose = DiagnoseErrorParams {
            db_session_id: "dbs-1".to_owned(),
            database: "app".to_owned(),
            sql: "select 1".to_owned(),
            error_message: "boom".to_owned(),
        };
        assert_eq!(diagnose.error_message, "boom");

        let explain = AnalyzeExplainParams {
            db_session_id: "dbs-1".to_owned(),
            explain_output: "Seq Scan".to_owned(),
            original_sql: "select 1".to_owned(),
        };
        assert_eq!(explain.explain_output, "Seq Scan");

        let parse = ParseFilterParams {
            db_session_id: "dbs-1".to_owned(),
            database: "app".to_owned(),
            table: "orders".to_owned(),
        };
        assert_eq!(parse.table, "orders");

        let send = SendChatMessageParams {
            db_session_id: Some("dbs-1".to_owned()),
            connection_id: Some("conn-1".to_owned()),
            database: Some("app".to_owned()),
            content: "explain the schema".to_owned(),
            include_schema: Some(true),
            agent_mode: Some(AiAgentMode::Agent),
            context_files: Some(vec!["notes.md".to_owned()]),
            context_tables: Some(vec!["orders".to_owned()]),
            auto_context: Some(AiChatContextSnapshot {
                editor_sql: Some("select 1".to_owned()),
                selection: None,
                last_error: None,
                result_preview: None,
            }),
            template_id: Some("tpl-1".to_owned()),
            database_type: Some("postgresql".to_owned()),
            target_claim: Some(AiTargetClaim::EditorGrant {
                grant_id: "g-1".to_owned(),
                claimed_client_instance_id: "ci-1".to_owned(),
                editor_session_id: "es-1".to_owned(),
            }),
            memory_mode: Some(AiMemoryMode::Auto),
            memory_scope: Some(AiMemoryScope::Connection),
        };
        assert_eq!(send.target_claim, Some(AiTargetClaim::EditorGrant {
            grant_id: "g-1".to_owned(),
            claimed_client_instance_id: "ci-1".to_owned(),
            editor_session_id: "es-1".to_owned(),
        }));

        let workflow_message = SendWorkflowChatMessageParams {
            db_session_id: Some("dbs-1".to_owned()),
            database: Some("app".to_owned()),
            content: "run the cleanup".to_owned(),
            include_schema: Some(false),
            context_files: None,
            context_tables: None,
        };
        assert!(!workflow_message.include_schema.unwrap());

        let execute = ExecuteWorkflowParams {
            workflow_id: "wf-1".to_owned(),
            variables: serde_json::json!({ "days": 30 }),
            connection_id: Some("conn-1".to_owned()),
        };
        assert_eq!(execute.variables, serde_json::json!({ "days": 30 }));

        let schema_doc = GenerateSchemaDocParams {
            db_session_id: "dbs-1".to_owned(),
            database: "app".to_owned(),
        };
        assert_eq!(schema_doc.database, "app");

        let diagnose_connection = DiagnoseConnectionParams {
            connection_id: "conn-1".to_owned(),
            error_message: "timeout".to_owned(),
        };
        assert_eq!(diagnose_connection.error_message, "timeout");

        let analyze_queries = AnalyzeQueriesParams {
            db_session_id: Some("dbs-1".to_owned()),
        };
        assert_eq!(analyze_queries.db_session_id.as_deref(), Some("dbs-1"));

        let call_tool = CallMcpToolParams {
            server_id: "srv-1".to_owned(),
            tool_name: "read_file".to_owned(),
            arguments: serde_json::json!({ "path": "/tmp/x" }),
        };
        assert_eq!(call_tool.arguments, serde_json::json!({ "path": "/tmp/x" }));
    }

    /// The minimal fake implementation: proves the trait is implementable
    /// and object-safe, and that every signature is self-consistent (a
    /// stub that fails to type-check fails the build). A representative
    /// subset is then driven through `&mut dyn AiStoreActions`.
    struct FakeAiStore {
        calls: Vec<&'static str>,
    }

    impl AiStoreActions for FakeAiStore {
        fn load_config(&mut self) -> Result<(), String> {
            self.calls.push("load_config");
            Ok(())
        }
        fn load_settings_config(&mut self) -> Result<(), String> {
            Ok(())
        }
        fn save_settings_config(&mut self, _config: &AiSettingsConfig) -> Result<bool, String> {
            Ok(true)
        }
        fn set_active_profile(&mut self, _profile_id: &str) -> Result<bool, String> {
            Ok(true)
        }
        fn save_profile(&mut self, _profile: &AiModelProfile) -> Result<bool, String> {
            Ok(true)
        }
        fn delete_profile(&mut self, _profile_id: &str) -> Result<bool, String> {
            Ok(true)
        }
        fn load_providers(&mut self) -> Result<(), String> {
            Ok(())
        }
        fn fetch_remote_models(
            &mut self,
            protocol: &str,
            _endpoint: &str,
            _api_key: &str,
        ) -> Result<Vec<ModelInfo>, String> {
            assert_eq!(protocol, "openai");
            Ok(Vec::new())
        }
        fn validate_config(&mut self, _config: &AiProviderConfig) -> Result<bool, String> {
            Ok(true)
        }
        fn save_config(&mut self, _config: &AiProviderConfig) -> Result<bool, String> {
            Ok(true)
        }
        fn delete_config(&mut self) -> Result<(), String> {
            Ok(())
        }
        fn clear_error(&mut self) {
            self.calls.push("clear_error");
        }
        fn set_nl2sql_input(&mut self, _input: &str) {}
        fn generate_sql(&mut self, _params: GenerateSqlParams) -> Result<(), String> {
            self.calls.push("generate_sql");
            Ok(())
        }
        fn clear_nl2sql(&mut self) {}
        fn diagnose_error(&mut self, _params: DiagnoseErrorParams) -> Result<(), String> {
            Ok(())
        }
        fn clear_diagnosis(&mut self) {}
        fn analyze_explain(&mut self, _params: AnalyzeExplainParams) -> Result<(), String> {
            Ok(())
        }
        fn clear_explain_analysis(&mut self) {}
        fn set_nl_filter_input(&mut self, _input: &str) {}
        fn parse_filter(
            &mut self,
            _params: ParseFilterParams,
        ) -> Result<Option<Vec<FilterCondition>>, String> {
            Ok(None)
        }
        fn clear_nl_filter(&mut self) {}
        fn init_chat_session(
            &mut self,
            db_session_id: Option<&str>,
            _database: Option<&str>,
            _connection_id: Option<&str>,
        ) -> Result<(), String> {
            assert_eq!(db_session_id, Some("dbs-1"));
            Ok(())
        }
        fn refresh_chat_sessions(&mut self) -> Result<(), String> {
            Ok(())
        }
        fn select_chat_session(&mut self, _id: &str) -> Result<(), String> {
            Ok(())
        }
        fn new_chat_session(&mut self) {}
        fn delete_chat_session(&mut self, _id: &str) -> Result<(), String> {
            Ok(())
        }
        fn export_chat_session(&mut self, _format: ExportFormat) -> Result<(), String> {
            Ok(())
        }
        fn send_chat_message(&mut self, _params: SendChatMessageParams) -> Result<(), String> {
            self.calls.push("send_chat_message");
            Ok(())
        }
        fn clear_chat(&mut self) {}
        fn dismiss_memory_candidate_notice(&mut self) {}
        fn cancel_chat(
            &mut self,
            request_id: Option<&str>,
        ) -> Result<Option<CancelReceipt>, String> {
            Ok(request_id.map(|request_id| CancelReceipt::Marked {
                request_id: request_id.to_owned(),
            }))
        }
        fn handle_cancel_timeout(&mut self, _request_id: &str) {}
        fn init_workflow_chat(&mut self) {}
        fn send_workflow_chat_message(
            &mut self,
            _params: SendWorkflowChatMessageParams,
        ) -> Result<(), String> {
            Ok(())
        }
        fn clear_workflow_chat(&mut self) {}
        fn handle_stream_chunk(&mut self, payload: &StreamChunkPayload) {
            assert_eq!(payload.request_id, "req-1");
        }
        fn setup_event_listeners(&mut self) -> Result<EventListenerTeardown, String> {
            Ok(Box::new(|| {}))
        }
        fn load_workflows(&mut self) -> Result<(), String> {
            Ok(())
        }
        fn execute_workflow(&mut self, _params: ExecuteWorkflowParams) -> Result<(), String> {
            Ok(())
        }
        fn clear_workflow_result(&mut self) {}
        fn generate_schema_doc(&mut self, _params: GenerateSchemaDocParams) -> Result<(), String> {
            Ok(())
        }
        fn clear_schema_doc(&mut self) {}
        fn diagnose_connection(
            &mut self,
            _params: DiagnoseConnectionParams,
        ) -> Result<(), String> {
            Ok(())
        }
        fn clear_connection_diagnosis(&mut self) {}
        fn analyze_queries(&mut self, _params: AnalyzeQueriesParams) -> Result<(), String> {
            Ok(())
        }
        fn clear_query_analysis(&mut self) {}
        fn connect_mcp_server(&mut self, _server_id: &str) -> Result<(), String> {
            Ok(())
        }
        fn disconnect_mcp_server(&mut self, _server_id: &str) -> Result<(), String> {
            Ok(())
        }
        fn save_mcp_client_servers(&mut self, _configs: &[McpServerConfig]) -> Result<(), String> {
            Ok(())
        }
        fn load_mcp_servers(&mut self) -> Result<(), String> {
            Ok(())
        }
        fn load_mcp_tools(&mut self) -> Result<(), String> {
            Ok(())
        }
        fn call_mcp_tool(&mut self, _params: CallMcpToolParams) -> Result<String, String> {
            Ok("tool output".to_owned())
        }
        fn clear_mcp_error(&mut self) {}
        fn clear_mcp_server_error(&mut self, _server_id: &str) {}
    }

    #[test]
    fn the_action_trait_admits_a_minimal_fake_implementation() {
        let mut fake = FakeAiStore { calls: Vec::new() };
        // Object-safety: the whole contract dispatches through `dyn`.
        let store: &mut dyn AiStoreActions = &mut fake;

        assert_eq!(store.load_config(), Ok(()));
        store.clear_error();
        assert_eq!(
            store.fetch_remote_models("openai", "https://api.openai.com/v1", "sk-test"),
            Ok(Vec::new())
        );
        assert_eq!(
            store.generate_sql(GenerateSqlParams {
                db_session_id: "dbs-1".to_owned(),
                database: "app".to_owned(),
                ..GenerateSqlParams::default()
            }),
            Ok(())
        );
        assert_eq!(
            store.init_chat_session(Some("dbs-1"), None, None),
            Ok(())
        );
        assert_eq!(
            store.send_chat_message(SendChatMessageParams {
                content: "hi".to_owned(),
                ..SendChatMessageParams::default()
            }),
            Ok(())
        );
        assert_eq!(
            store.cancel_chat(Some("req-9")),
            Ok(Some(CancelReceipt::Marked {
                request_id: "req-9".to_owned()
            }))
        );
        assert_eq!(store.cancel_chat(None), Ok(None));
        store.handle_stream_chunk(&StreamChunkPayload {
            request_id: "req-1".to_owned(),
            content: "chunk".to_owned(),
            reasoning: None,
            done: false,
            cancelled: None,
            usage: None,
            tool_calls: None,
            egress_summary: None,
            memory_used: None,
        });
        let teardown = store.setup_event_listeners().unwrap();
        teardown();
        assert_eq!(
            store.parse_filter(ParseFilterParams {
                db_session_id: "dbs-1".to_owned(),
                database: "app".to_owned(),
                table: "orders".to_owned(),
            }),
            Ok(None)
        );
        assert_eq!(
            store.call_mcp_tool(CallMcpToolParams {
                server_id: "srv-1".to_owned(),
                tool_name: "read_file".to_owned(),
                arguments: serde_json::json!({}),
            }),
            Ok("tool output".to_owned())
        );
        assert_eq!(
            store.export_chat_session(ExportFormat::Json),
            Ok(())
        );
        store.clear_mcp_server_error("srv-1");

        assert_eq!(
            fake.calls,
            vec!["load_config", "clear_error", "generate_sql", "send_chat_message"]
        );
    }
}
