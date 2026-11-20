//! The types-family aggregate root — ported from React
//! `packages/frontend/src/types/index.ts` (tasklist T211, 588 lines).
//!
//! ## The aggregate story
//!
//! In React every consumer does `import type { X } from '../types'` — this
//! one file is the app's whole contract vocabulary. The file is an
//! **aggregate**: four-fifths of it re-exports vocabulary the sibling
//! modules already own (or that lives in `@datazen/backend-client` /
//! `@datazen/driver-sdk` / `@datazen/extension-points`), and the rest is
//! its own interface declarations. The port mirrors that shape: this
//! module is a **family root** that re-exports the already-ported sibling
//! modules as ONE vocabulary surface, and the submodules beside it carry
//! the members that were still missing — nothing is redefined, so the
//! vocabulary cannot fork.
//!
//! ## Naming decision: `types/mod.rs`, not `types_index.rs`
//!
//! The family's naming convention (`types/X.ts` → `X_types.rs`) would spell
//! this `index_types.rs`; the task's alternative was a flat
//! `types_index.rs`. Neither fits: the file's own declarations pull their
//! dependency vocabulary from **seven** upstream sources
//! (extension-points, backend-client's driverCommands/settings/workflow
//! files, driver-sdk, frontend `types/settings.ts` and
//! `lib/valuePresentation/settings.ts`), and the port of all of them under
//! the family's doc-and-test law would blow the repo's single-file limit
//! (AGENTS.md's 800-line rule). The Rust answer is the directory this
//! module heads — `src/types/` — one submodule per upstream source, this
//! `mod.rs` as the index. It also mirrors the React side's own shape:
//! `types/index.ts` indexes a directory, and so does this.
//!
//! ## Ownership map (who owns what; this root only re-exports)
//!
//! | Vocabulary | Rust owner |
//! |---|---|
//! | `DatabaseType`, `SslMode`, `DatabaseObjectKind`, `Value`, `TunnelKind`, `SshAuthMethod`, `HttpProxyTunnelConfig`, `WebSocketTunnelConfig`, `SavedTunnel`, `TableType`, `TableInfo`, `ColumnSchema`, `IndexInfo`, `ForeignKeyInfo`, `TableSchema`, `SshTunnelConfig`, `ConnectionConfig`, `ServerInfo`, `DriverCapabilities`, `DatabaseObject`, `PrivilegeGrant`, `ExplainPlanDetail/Node/Result`, `QueryHistoryEntry`, `ContextEntry` | `crate::connection_types` (T128, the `types/connection.ts` facade port — React's own NOTE records the two files' copies must not drift) |
//! | `ContextKind`, `ContextItem` | `crate::context_items` |
//! | `FilterOperator`, `EXISTING/SHARED/DIALECT_FILTER_OPERATORS`, `FilterCondition`, `FilterConnector` | `crate::filter_types` (T188) |
//! | `CursorValue`, `KeysetUnavailableReason`, `OrderColumn`, `PagePagingInfo`, `PagingMode` | `crate::keyset_types` (T234) |
//! | `HistorySort`, `QueryHistoryPage` | `crate::ai_memory_patterns` (T205's lean mirrors — see the reconciliation below) |
//! | `ConnectionDiagnosis`, `ConnectionSolution`, `QueryCategory`, `QueryAnalysis`, `McpServerConfig`, `McpClientStatus`, `McpToolInfo`, `StepStatus`, `StepExecutionResult`, `WorkflowExecutionResult`, `WorkflowListItem`, `WorkflowVariable` | `crate::ai_store_types` (T126's field-complete lean mirrors, whose docs name this port as their owner) |
//! | the `export * from './ai'` surface (~90 names) | `crate::ai_types` (T042) |
//! | `MonitorSettings` (the `AppSettings` field leg) | `crate::dashboard_types` (T159) — imported by React's index exactly the way this port imports it; **not** re-exported, because React's index does not re-export it either |
//! | `CountStrategy` | `types::extension_points` (new) |
//! | `TunnelSource`, `SavedTunnelSummary`, `TunnelUsage`, `TableOptionsSnapshot` | `types::driver_sdk` (new) |
//! | `ColumnInfo`, `QueryResult`, `StatementResult`, `MultiQueryResult`, `QueryStreamEvent` | `types::query` (new — the family T128's inventory deferred to "the aggregate that needs the names") |
//! | `CommandCategory`, `CommandAccessLevel`, `DriverSaveDialogSpec`, `DriverCommandMetadata`, `DriverCommandDefinition` | `types::driver_commands` (new) |
//! | `McpPermissionMode` (= backend-client's `SettingsMcpPermissionMode`), `SqlFormatOptions` (= `SettingsSqlFormatOptions`), `SqlExecutionStrategy` (= `SettingsSqlExecutionStrategy`) | `types::client_settings` (new — defined once under the backend-client names, re-exported here under the frontend's alias, exactly React's aliasing) |
//! | the nineteen still-missing workflow names + the two pre-lift aliases (`HistoryListItem`, `HistoryEntry`) | `types::workflow` (new) |
//! | `DefaultTableSort`, `GridDensity`, `RowCountMode` | `types::table_settings` (new) |
//! | `AppSettings` | `types::app_settings` (new — flattens `types::client_settings`' `UserPreferences`/`ExecutionSettings` legs the way TS interface extension does) |
//! | `SortCondition`, `TableDataResult`, `KeyEntry`, `KeyScanResult`, `KeyDetail` | `types::table_data` (new) |
//! | `TableCompareStatus`, `SyncObjectKind`, `TableComparison`, `ColumnDiffEntry`, `ChangedColumnDiff`, `CheckConstraintDiffEntry`, `TableOptionsDiff`, `TableSchemaDiff` | `types::sync_diff` (new) |
//! | `FavoriteQuery` | `types::favorites` (new) |
//! | `McpHttpStatus`, `McpApprovalRequest`, `McpApprovalDecision`, `McpAuditRecord`, `isValidMcpServerId` | `types::mcp` (new — `McpServerConfig`/`McpClientStatus`/`McpToolInfo` are the `ai_store_types` mirrors above, `platform_shared`'s `WebMcp*` is the web host's different vocabulary) |
//!
//! ## The `QueryHistoryPage` reconciliation
//!
//! React's index.ts declares `QueryHistoryPage { entries, total }` — and
//! T205's `ai_memory_patterns` lean mirror is **exactly that shape**
//! (`entries: Vec<QueryHistoryEntry>`, `total: u64`, with `HistorySort` as
//! its sibling): the index copy is *not* richer, so there is nothing to
//! fold. **One definition wins: the `ai_memory_patterns` one** (it is
//! field-complete, already at the crate root, and its docs say React reads
//! `entries` only while the field rides as the command's own contract);
//! this root re-exports it and adds no second definition. Should the
//! commands/query port later want a wire mirror, it re-exports the same
//! name.
//!
//! ## Surface law
//!
//! The root's `pub use` surface covers the React file's export list
//! exactly: every name importable from `'../types'` is importable from
//! `crate::types` (the test suite pins this as a compile-level identity —
//! see `the_aggregate_surface_reexports_every_react_export`). The only
//! extras are the family-law names React declares *inline* inside exported
//! shapes (`FilterConnector`, `McpHttpTransport`, `McpAuditDecision`,
//! `LogLevel`, `OnboardingState`, the session-context legs) and the
//! `SavedTunnelSshConfig` alias — Rust needs them nameable where React's
//! surface structurally requires them; types that React imports without
//! re-exporting (`UserPreferences`, `ExecutionSettings`,
//! `StoredValuePresentationSettings`, `MonitorSettings`,
//! `QueryResultArtifacts`) deliberately stay off the surface, reachable
//! via their submodule paths, mirroring React's own import-only
//! reachability.
//!
//! One-surface ergonomics: consumers import from `crate::types` (or the
//! crate root, which re-exports the aggregate's new vocabulary), never
//! from the individual family modules — the same single-import-path habit
//! the React app has.

mod app_settings;
mod client_settings;
mod driver_commands;
mod driver_sdk;
mod extension_points;
mod favorites;
mod mcp;
mod query;
mod sync_diff;
mod table_data;
mod table_settings;
mod value_presentation;
mod workflow;

// ── Re-export group: `types/connection.ts`'s facade vocabulary (T128) ───────
// The aggregate's biggest block: everything index.ts re-exports (or
// re-declares as its own copy — the two files' NOTE-recorded twins) from
// the connection facade.

pub use crate::connection_types::{
    ConnectionConfig, ContextEntry, DatabaseObject, DatabaseObjectKind, DatabaseType,
    DriverCapabilities, ExplainPlanDetail, ExplainPlanNode, ExplainResult, ForeignKeyInfo,
    HttpProxyTunnelConfig, IndexInfo, PrivilegeGrant, QueryHistoryEntry, SavedTunnel, ServerInfo,
    SshAuthMethod, SshTunnelConfig, SslMode, TableInfo, TableSchema, TableType, TunnelKind, Value,
    WebSocketTunnelConfig, ColumnSchema,
};

// ── Re-export group: the context, filter and keyset families ───────────────

pub use crate::context_items::{ContextItem, ContextKind};

// React's `export * from './filters'` — everything `types/filters.ts`
// declares, plus `FilterConnector` (the Rust-given name of the inline
// `'and' | 'or'` union inside `FilterCondition`; T188 exported it at the
// crate root the same way).
pub use crate::filter_types::{
    DIALECT_FILTER_OPERATORS, EXISTING_FILTER_OPERATORS, FilterCondition, FilterConnector,
    FilterOperator, SHARED_FILTER_OPERATORS,
};

// React's keyset re-export block — the five names `types/index.ts` lists.
pub use crate::keyset_types::{
    CursorValue, KeysetUnavailableReason, OrderColumn, PagePagingInfo, PagingMode,
};

// ── Re-export group: query history (the T205 reconciliation above) ─────────

pub use crate::ai_memory_patterns::{HistorySort, QueryHistoryPage};

// ── Re-export group: the ai store's lean mirrors (T126) ─────────────────────
// The `types/index.ts` shapes the AI store port needed field-complete
// ahead of this port; their docs name this module as the owner, and this
// root re-exports them without redefining (single definitions, no fork).

pub use crate::ai_store_types::{
    ConnectionDiagnosis, ConnectionSolution, McpClientStatus, McpServerConfig, McpToolInfo,
    QueryAnalysis, QueryCategory, StepExecutionResult, StepStatus, WorkflowExecutionResult,
    WorkflowListItem, WorkflowVariable,
};

// ── Re-export group: `export * from './ai'` (T042) ──────────────────────────
// The full `types/ai.ts` export surface rides the aggregate, exactly the
// way React's star re-export carries it.

pub use crate::ai_types::{
    AiAgentMode, AiChatAttribution, AiChatContextSnapshot, AiChatInjectionEvent,
    AiChatInjectionSnapshot, AiChatMessage, AiChatResultPreview, AiChatSession, AiDataEgressLevel,
    AiLoadedSkill, AiMemoryCandidatesEvent, AiMemoryCloudPolicy, AiMemoryCorrectionInput,
    AiMemoryCreateInput, AiMemoryEntry, AiMemoryExtractionInput, AiMemoryKind, AiMemoryListParams,
    AiMemoryPage, AiMemoryReference, AiMemoryScopeKind, AiMemorySensitivity, AiMemorySettings,
    AiMemorySource, AiMemorySqlSample, AiMemoryStaleInput, AiMemoryStats, AiMemoryStatus,
    AiMemoryUpdateInput, AiMessageRole, AiMessageStatus, AiModelProfile, AiPreviewCell,
    AiProviderConfig, AiProviderType, AiQuestion, AiQuestionOption, AiSafetyGateConfig,
    AiSessionDetail, AiSessionMessage, AiSessionMessageInput, AiSessionSummary,
    AiSessionUpsertInput, AiSettingsConfig, AiSkillContent, AiSkillSource, AiSkillSummary,
    AiSkillsSettings, AiToolCall, AiToolPermissionPolicy, AiToolResult, AiWriteApplyReceipt,
    AiWriteAuditDecision, AiWriteAuditDecisionKind, AiWriteAuditRecord, AiWriteImpact,
    AiWriteProposal, AiWriteStatement, Bottleneck, BottleneckSeverity, DiagnosisResult,
    ExplainAnalysis, ExplainSuggestion, ModelInfo, PromptTemplate, ProviderListItem,
    StreamChunkPayload, StreamChunkUsage, StreamErrorPayload, DEFAULT_AI_MEMORY_SETTINGS,
    DEFAULT_AI_SKILLS_SETTINGS,
};

// ── Re-export group: extension-points (`CountStrategy`) ─────────────────────

pub use crate::types::extension_points::CountStrategy;

// ── Re-export group: backend-client's query-stream half ─────────────────────

pub use crate::types::query::{
    ColumnInfo, MultiQueryResult, QueryResult, QueryStreamEvent, ResultCompleteness, StatementResult,
};

// ── Re-export group: backend-client's driver-command catalog ───────────────

pub use crate::types::driver_commands::{
    CommandAccessLevel, CommandCategory, DriverCommandDefinition, DriverCommandMetadata,
    DriverSaveDialogSpec,
};

// ── Re-export group: backend-client's settings trio, under React's alias ───
// index.ts re-exports `SettingsMcpPermissionMode as McpPermissionMode`
// (and the two siblings): one definition each in `types::client_settings`,
// alias re-exported here, exactly the aliasing React does. The leg types
// the aliases structurally need (`ThemePreferenceMode`, …) stay at their
// submodule path — React imports them without re-exporting them too.

pub use crate::types::client_settings::{
    SettingsMcpPermissionMode as McpPermissionMode, SettingsSqlExecutionStrategy as
    SqlExecutionStrategy, SettingsSqlFormatOptions as SqlFormatOptions,
};

// ── Re-export group: frontend `types/settings.ts` (the table-read trio) ─────

pub use crate::types::table_settings::{DefaultTableSort, GridDensity, RowCountMode};

// ── Re-export group: driver-sdk's tunnel and table-options members ──────────

pub use crate::types::driver_sdk::{
    SavedTunnelSummary, TableOptionsSnapshot, TunnelSource, TunnelUsage,
};

// `SavedTunnelSshConfig` — driver-sdk's own declaration, field-identical to
// `SshTunnelConfig` (T128's documented fold: the TS split existed for a
// package boundary this crate does not have). One definition, alias
// re-exported — the aggregate's ninth tunnel name.
pub use crate::connection_types::SshTunnelConfig as SavedTunnelSshConfig;

// ── The aggregate's own declarations, per submodule ─────────────────────────

pub use crate::types::app_settings::{AppSettings, LogLevel, OnboardingState};
pub use crate::types::favorites::FavoriteQuery;
pub use crate::types::mcp::{
    McpApprovalDecision, McpApprovalRequest, McpAuditDecision, McpAuditRecord, McpHttpStatus,
    McpHttpTransport, is_valid_mcp_server_id,
};
pub use crate::types::sync_diff::{
    ChangedColumnDiff, CheckConstraintDiffEntry, ColumnDiffEntry, SyncObjectKind, TableCompareStatus,
    TableComparison, TableOptionsDiff, TableSchemaDiff,
};
pub use crate::types::table_data::{
    KeyDetail, KeyEntry, KeyScanResult, SortCondition, TableDataResult,
};

// ── Re-export group: backend-client's workflow vocabulary (W8 刀3c) ─────────
// The nineteen names the aggregate still owed (T126 owns the other five of
// React's twenty-four — re-exported in the ai-store block above), plus the
// two pre-lift compatibility aliases React's index.ts declares:
// `HistoryListItem` (= `WorkflowRunListItem`) and `HistoryEntry`
// (= `WorkflowRunRecord`) — the 运行历史词汇's historical naming.

pub use crate::types::workflow::{
    ErrorHandlingConfig, ErrorHandlingStrategy, MergeSource, TransformColumn, WorkflowBlock,
    WorkflowBlockRetry, WorkflowBlockType, WorkflowDefinition, WorkflowExecuteRequest,
    WorkflowIdRequest, WorkflowOutput, WorkflowRunGetRequest, WorkflowRunListItem,
    WorkflowRunRecord, WorkflowRunsListRequest, WorkflowSaveRequest, WorkflowSchedule, WorkflowStep,
    WorkflowStepType, WorkflowVisibility, WorkflowYamlSaveRequest,
};

pub use crate::types::workflow::WorkflowRunListItem as HistoryListItem;
pub use crate::types::workflow::WorkflowRunRecord as HistoryEntry;

// ── Re-export group: the session legs of the query-stream half ──────────────
// Rust-given names for unions/shapes React declares inline inside the
// re-exported `StatementResult`/`QueryStreamEvent` legs — exported so the
// surface's own types are nameable, the `FilterConnector` precedent.

pub use crate::types::query::{
    Counter, Id, RuntimeResultBinding, ResultProvenance, SessionContext, SessionContextConfidence,
    SessionHandle, Timestamp, TransactionState, WritableMapping,
};

#[cfg(test)]
mod tests {
    // The aggregate's own pins: the surface itself. The sibling modules'
    // literal tables and wire pins live with their definitions; this module
    // only pins that the ONE surface carries every name React's
    // `types/index.ts` exports (plus the documented inline-name extras) —
    // a compile-level identity, since a missing re-export fails to compile
    // here before any runtime assertion runs.

    /// Forces the type to be nameable through the aggregate path: the whole
    /// test is the name resolution.
    fn pin<T>() {}

    #[test]
    fn the_aggregate_surface_reexports_every_react_export() {
        // ── extensions/generated + extension-points ──
        pin::<crate::types::DatabaseType>();
        pin::<crate::types::SslMode>();
        pin::<crate::types::DatabaseObjectKind>();
        pin::<crate::types::CountStrategy>();

        // ── backend-client's query-stream half ──
        pin::<crate::types::Value>();
        pin::<crate::types::ColumnInfo>();
        pin::<crate::types::QueryResult>();
        pin::<crate::types::StatementResult>();
        pin::<crate::types::MultiQueryResult>();
        pin::<crate::types::QueryStreamEvent>();

        // ── backend-client's driver-command catalog ──
        pin::<crate::types::CommandCategory>();
        pin::<crate::types::CommandAccessLevel>();
        pin::<crate::types::DriverSaveDialogSpec>();
        pin::<crate::types::DriverCommandMetadata>();
        pin::<crate::types::DriverCommandDefinition>();

        // ── driver-sdk's tunnel vocabulary ──
        pin::<crate::types::TunnelKind>();
        pin::<crate::types::TunnelSource>();
        pin::<crate::types::SshAuthMethod>();
        pin::<crate::types::HttpProxyTunnelConfig>();
        pin::<crate::types::WebSocketTunnelConfig>();
        pin::<crate::types::SavedTunnel>();
        pin::<crate::types::SavedTunnelSshConfig>();
        pin::<crate::types::SavedTunnelSummary>();
        pin::<crate::types::TunnelUsage>();

        // ── `export * from './filters'` ──
        pin::<crate::types::FilterCondition>();
        pin::<crate::types::FilterOperator>();
        pin::<crate::types::FilterConnector>();
        let _ = crate::types::EXISTING_FILTER_OPERATORS;
        let _ = crate::types::SHARED_FILTER_OPERATORS;
        let _ = crate::types::DIALECT_FILTER_OPERATORS;

        // ── `./keyset` ──
        pin::<crate::types::CursorValue>();
        pin::<crate::types::KeysetUnavailableReason>();
        pin::<crate::types::OrderColumn>();
        pin::<crate::types::PagePagingInfo>();
        pin::<crate::types::PagingMode>();

        // ── the connection facade's own declarations ──
        pin::<crate::types::SshTunnelConfig>();
        pin::<crate::types::ConnectionConfig>();
        pin::<crate::types::ServerInfo>();
        pin::<crate::types::DriverCapabilities>();
        pin::<crate::types::DatabaseObject>();
        pin::<crate::types::PrivilegeGrant>();
        pin::<crate::types::ExplainPlanDetail>();
        pin::<crate::types::ExplainPlanNode>();
        pin::<crate::types::ExplainResult>();
        pin::<crate::types::QueryHistoryEntry>();
        pin::<crate::types::HistorySort>();
        pin::<crate::types::QueryHistoryPage>();
        pin::<crate::types::FavoriteQuery>();
        pin::<crate::types::ContextEntry>();
        pin::<crate::types::ContextKind>();
        pin::<crate::types::ContextItem>();

        // ── the aliased settings trio ──
        pin::<crate::types::McpPermissionMode>();
        pin::<crate::types::SqlFormatOptions>();
        pin::<crate::types::SqlExecutionStrategy>();

        // ── `./settings` ──
        pin::<crate::types::DefaultTableSort>();
        pin::<crate::types::GridDensity>();
        pin::<crate::types::RowCountMode>();

        // ── the app settings blob ──
        pin::<crate::types::AppSettings>();

        // ── the table-data and key-value shapes ──
        pin::<crate::types::SortCondition>();
        pin::<crate::types::KeyEntry>();
        pin::<crate::types::KeyScanResult>();
        pin::<crate::types::KeyDetail>();
        pin::<crate::types::TableDataResult>();

        // ── the driver-sdk catalog block ──
        pin::<crate::types::TableType>();
        pin::<crate::types::TableInfo>();
        pin::<crate::types::ColumnSchema>();
        pin::<crate::types::IndexInfo>();
        pin::<crate::types::ForeignKeyInfo>();
        pin::<crate::types::TableSchema>();
        pin::<crate::types::TableOptionsSnapshot>();

        // ── `export * from './ai'` (the whole ai.ts surface) ──
        pin::<crate::types::AiAgentMode>();
        pin::<crate::types::AiChatAttribution>();
        pin::<crate::types::AiChatContextSnapshot>();
        pin::<crate::types::AiChatInjectionEvent>();
        pin::<crate::types::AiChatInjectionSnapshot>();
        pin::<crate::types::AiChatMessage>();
        pin::<crate::types::AiChatResultPreview>();
        pin::<crate::types::AiChatSession>();
        pin::<crate::types::AiDataEgressLevel>();
        pin::<crate::types::AiLoadedSkill>();
        pin::<crate::types::AiMemoryCandidatesEvent>();
        pin::<crate::types::AiMemoryCloudPolicy>();
        pin::<crate::types::AiMemoryCorrectionInput>();
        pin::<crate::types::AiMemoryCreateInput>();
        pin::<crate::types::AiMemoryEntry>();
        pin::<crate::types::AiMemoryExtractionInput>();
        pin::<crate::types::AiMemoryKind>();
        pin::<crate::types::AiMemoryListParams>();
        pin::<crate::types::AiMemoryPage>();
        pin::<crate::types::AiMemoryReference>();
        pin::<crate::types::AiMemoryScopeKind>();
        pin::<crate::types::AiMemorySensitivity>();
        pin::<crate::types::AiMemorySettings>();
        pin::<crate::types::AiMemorySource>();
        pin::<crate::types::AiMemorySqlSample>();
        pin::<crate::types::AiMemoryStaleInput>();
        pin::<crate::types::AiMemoryStats>();
        pin::<crate::types::AiMemoryStatus>();
        pin::<crate::types::AiMemoryUpdateInput>();
        pin::<crate::types::AiMessageRole>();
        pin::<crate::types::AiMessageStatus>();
        pin::<crate::types::AiModelProfile>();
        pin::<crate::types::AiPreviewCell>();
        pin::<crate::types::AiProviderConfig>();
        pin::<crate::types::AiProviderType>();
        pin::<crate::types::AiQuestion>();
        pin::<crate::types::AiQuestionOption>();
        pin::<crate::types::AiSafetyGateConfig>();
        pin::<crate::types::AiSessionDetail>();
        pin::<crate::types::AiSessionMessage>();
        pin::<crate::types::AiSessionMessageInput>();
        pin::<crate::types::AiSessionSummary>();
        pin::<crate::types::AiSessionUpsertInput>();
        pin::<crate::types::AiSettingsConfig>();
        pin::<crate::types::AiSkillContent>();
        pin::<crate::types::AiSkillSource>();
        pin::<crate::types::AiSkillSummary>();
        pin::<crate::types::AiSkillsSettings>();
        pin::<crate::types::AiToolCall>();
        pin::<crate::types::AiToolPermissionPolicy>();
        pin::<crate::types::AiToolResult>();
        pin::<crate::types::AiWriteApplyReceipt>();
        pin::<crate::types::AiWriteAuditDecision>();
        pin::<crate::types::AiWriteAuditDecisionKind>();
        pin::<crate::types::AiWriteAuditRecord>();
        pin::<crate::types::AiWriteImpact>();
        pin::<crate::types::AiWriteProposal>();
        pin::<crate::types::AiWriteStatement>();
        pin::<crate::types::Bottleneck>();
        pin::<crate::types::BottleneckSeverity>();
        pin::<crate::types::DiagnosisResult>();
        pin::<crate::types::ExplainAnalysis>();
        pin::<crate::types::ExplainSuggestion>();
        pin::<crate::types::ModelInfo>();
        pin::<crate::types::PromptTemplate>();
        pin::<crate::types::ProviderListItem>();
        pin::<crate::types::StreamChunkPayload>();
        pin::<crate::types::StreamChunkUsage>();
        pin::<crate::types::StreamErrorPayload>();
        let _ = crate::types::DEFAULT_AI_MEMORY_SETTINGS;
        let _ = crate::types::DEFAULT_AI_SKILLS_SETTINGS;

        // ── the workflow block (twenty-four names) + the pre-lift aliases ──
        pin::<crate::types::ErrorHandlingConfig>();
        pin::<crate::types::MergeSource>();
        pin::<crate::types::StepExecutionResult>();
        pin::<crate::types::StepStatus>();
        pin::<crate::types::TransformColumn>();
        pin::<crate::types::WorkflowBlock>();
        pin::<crate::types::WorkflowBlockRetry>();
        pin::<crate::types::WorkflowBlockType>();
        pin::<crate::types::WorkflowDefinition>();
        pin::<crate::types::WorkflowExecuteRequest>();
        pin::<crate::types::WorkflowExecutionResult>();
        pin::<crate::types::WorkflowIdRequest>();
        pin::<crate::types::WorkflowListItem>();
        pin::<crate::types::WorkflowOutput>();
        pin::<crate::types::WorkflowRunGetRequest>();
        pin::<crate::types::WorkflowRunListItem>();
        pin::<crate::types::WorkflowRunRecord>();
        pin::<crate::types::WorkflowRunsListRequest>();
        pin::<crate::types::WorkflowSaveRequest>();
        pin::<crate::types::WorkflowSchedule>();
        pin::<crate::types::WorkflowStep>();
        pin::<crate::types::WorkflowStepType>();
        pin::<crate::types::WorkflowVariable>();
        pin::<crate::types::WorkflowYamlSaveRequest>();
        pin::<crate::types::HistoryListItem>();
        pin::<crate::types::HistoryEntry>();

        // ── the diagnosis/analysis block ──
        pin::<crate::types::ConnectionDiagnosis>();
        pin::<crate::types::ConnectionSolution>();
        pin::<crate::types::QueryCategory>();
        pin::<crate::types::QueryAnalysis>();

        // ── the data-sync block ──
        pin::<crate::types::TableCompareStatus>();
        pin::<crate::types::SyncObjectKind>();
        pin::<crate::types::TableComparison>();
        pin::<crate::types::ColumnDiffEntry>();
        pin::<crate::types::ChangedColumnDiff>();
        pin::<crate::types::CheckConstraintDiffEntry>();
        pin::<crate::types::TableOptionsDiff>();
        pin::<crate::types::TableSchemaDiff>();

        // ── the MCP client/status/approval/audit block ──
        pin::<crate::types::McpServerConfig>();
        pin::<crate::types::McpClientStatus>();
        pin::<crate::types::McpHttpStatus>();
        pin::<crate::types::McpToolInfo>();
        pin::<crate::types::McpApprovalRequest>();
        pin::<crate::types::McpApprovalDecision>();
        pin::<crate::types::McpAuditRecord>();
        // The aggregate's one value export.
        let _ = crate::types::is_valid_mcp_server_id;

        // ── the documented inline-name extras ride the same surface ──
        pin::<crate::types::FilterConnector>();
        pin::<crate::types::McpHttpTransport>();
        pin::<crate::types::McpAuditDecision>();
        pin::<crate::types::LogLevel>();
        pin::<crate::types::OnboardingState>();
        pin::<crate::types::ErrorHandlingStrategy>();
        pin::<crate::types::WorkflowVisibility>();
        pin::<crate::types::ResultCompleteness>();
        pin::<crate::types::TransactionState>();
        pin::<crate::types::SessionContextConfidence>();
        pin::<crate::types::WritableMapping>();
        pin::<crate::types::ResultProvenance>();
        pin::<crate::types::RuntimeResultBinding>();
        pin::<crate::types::SessionContext>();
        pin::<crate::types::SessionHandle>();
        pin::<crate::types::Counter>();
    }

    #[test]
    fn the_two_query_history_definitions_are_one() {
        // The reconciliation's compile-level proof: the aggregate's
        // `QueryHistoryPage` IS `ai_memory_patterns`' lean mirror (and its
        // `HistorySort`), not a second definition — type identity, not
        // just field parity.
        fn same<T>(_: &T, _: &T) {}
        let page = crate::ai_memory_patterns::QueryHistoryPage::default();
        let via_aggregate: crate::types::QueryHistoryPage = page.clone();
        same(&page, &via_aggregate);
        let sort = crate::ai_memory_patterns::HistorySort::Recent;
        let via_aggregate_sort: crate::types::HistorySort = sort;
        assert_eq!(sort.as_str(), via_aggregate_sort.as_str());
        assert_eq!(via_aggregate.entries.len(), 0);
        assert_eq!(via_aggregate.total, 0);
    }

    #[test]
    fn the_alias_reexports_are_type_identities() {
        // `SavedTunnelSshConfig` is `SshTunnelConfig` (T128's fold), and
        // the pre-lift history aliases are the workflow run types — one
        // definition each, alias re-exported.
        fn same<T>(_: &T, _: &T) {}
        let tunnel = crate::connection_types::SshTunnelConfig {
            enabled: false,
            host: "h".into(),
            port: 22,
            username: "u".into(),
            auth_method: crate::connection_types::SshAuthMethod::Password,
            password: None,
            private_key_path: None,
            passphrase: None,
            jump: None,
        };
        let as_ssh_leg: crate::types::SavedTunnelSshConfig = tunnel.clone();
        same(&tunnel, &as_ssh_leg);

        let item = crate::types::workflow::WorkflowRunListItem {
            id: "r".into(),
            workflow_id: "w".into(),
            workflow_name: "n".into(),
            success: true,
            total_time_ms: 1,
            created_at: "t".into(),
        };
        let as_history_list_item: crate::types::HistoryListItem = item.clone();
        same(&item, &as_history_list_item);
        assert_eq!(as_history_list_item.workflow_name, "n");

        let record = crate::types::workflow::WorkflowRunRecord {
            id: "r".into(),
            workflow_id: "w".into(),
            workflow_name: "n".into(),
            variables: Default::default(),
            result: crate::ai_store_types::WorkflowExecutionResult {
                success: true,
                final_output: String::new(),
                steps: Vec::new(),
                total_time_ms: 1,
                error: None,
            },
            created_at: "t".into(),
        };
        let as_history_entry: crate::types::HistoryEntry = record.clone();
        same(&record, &as_history_entry);
        assert_eq!(as_history_entry.result.success, true);
    }
}
