//! AI provider, chat, and analysis types — ported from React
//! `packages/frontend/src/types/ai.ts` (tasklist T042, 597 lines).
//!
//! This is the AI type family the whole frontend consumes through the
//! `types/index.ts` barrel (`export * from './ai'`): the provider/settings
//! vocabulary (`stores/aiStore.ts` port T128), the chat-session vocabulary
//! (`stores/ai/sessions.ts`, already ported as [`crate::ai_sessions`]), the
//! T24 write-proposal vocabulary, the memory vocabulary, and the analysis
//! result shapes. Pure contracts — no IO, no state — so the module compiles
//! for wasm32 exactly like its siblings.
//!
//! ## Layout (one submodule per React type family)
//!
//! - [`providers`] — provider identity and presets, the safety gate, model
//!   profiles and the persisted settings config.
//! - [`skills`] — the skills settings and skill summaries/contents.
//! - [`memory`] — the AI memory entry vocabulary (scope/kind/status/source
//!   enums, entries, pages, stats, inputs).
//! - [`chat`] — chat messages and sessions, backend session rows, injection
//!   events, and the stream chunk/error payloads.
//! - [`write`] — the T24 reviewable write-proposal family.
//! - [`analysis`] — diagnosis and explain-plan analysis results.
//!
//! ## Mirror rules applied throughout
//!
//! - A React string-literal union becomes a Rust enum whose `as_str` pins
//!   the exact React literal, and whose serde representation **is** that
//!   literal (the wire the backend speaks is the React spelling). The full
//!   literal tables are pinned by tests, one row per variant.
//! - `interface` → struct; `readonly` is the Rust default (no mutation
//!   through the field); optional (`?:`) fields become `Option` with
//!   `#[serde(default, skip_serializing_if = "Option::is_none")]` so an
//!   absent key round-trips as an absent key.
//! - JS `null` is folded into `None` (deserializing `null` into `Option`
//!   yields `None`; serializing `None` **omits** the key, where React would
//!   write an explicit `null` for `x: null`-typed fields). Required-but-
//!   nullable fields (`x: string | null` with no `?`) keep the null shape:
//!   `Option` without `skip_serializing_if`, so `None` serializes as `null`
//!   and the key stays present.
//! - `Record<string, unknown>` stays opaque `serde_json::Value`;
//!   `Record<string, number>` / `Record<string, string>` become `BTreeMap`
//!   (JS object key order is insertion order; the maps here are count/name
//!   lookups read by key, never order-observed, so the sorted iteration is
//!   inert — noted on each field).
//! - `unknown` (`AiWriteApplyReceipt.result`) is `serde_json::Value`.
//! - Open string unions (`'proposal' | 'applied' | 'denied' | string` in
//!   the audit record) are enums with a catch-all variant that round-trips
//!   any other string verbatim.
//! - Cross-file `extends` is expressed by flattening the base's fields
//!   (the wire JSON carries them at the top level) with the inheritance
//!   noted on the derived struct.
//! - Anonymous inline object types get a named struct with the React shape
//!   documented (e.g. [`chat::StreamChunkUsage`] for StreamChunkPayload's
//!   inline `usage`).
//!
//! ## Kernel twins, deliberately not re-exported
//!
//! The kernel crate has field-for-field equivalents of the provider half
//! (`datazen::ai::domain::types` / `safety`: `AiProviderType` — with extra
//! legacy wire aliases like `minimax`/`open-code` this mirror does not take,
//! `AiSafetyGateConfig`, `AiModelProfile`, `PromptTemplate`,
//! `AiMemorySettings`, `AiSettingsConfig`, `ModelInfo`, `DiagnosisResult`,
//! `ExplainAnalysis`, `Bottleneck`). But `datazen` is a
//! `cfg(not(target_arch = "wasm32"))`-only dependency of this crate while
//! the wasm host compiles these pure modules — the same trade
//! [`crate::metadata`] and [`crate::connection_views`] made (T080/T029).
//! So this module keeps **UI contract mirrors** pinned to React's wire
//! spellings; the adapter task that wires kernel DTOs to the UI should
//! convert *into* these types and leave the wire spellings single-sourced
//! here.
//!
//! ## Ownership of the halves not in this file
//!
//! The default constants (`defaultAiMemorySettings`,
//! `defaultAiSkillsSettings`) port as [`DEFAULT_AI_MEMORY_SETTINGS`] /
//! [`DEFAULT_AI_SKILLS_SETTINGS`]. The `ai_sessions` module (T040) carried
//! minimal mirrors of the session family; those ten types are now defined
//! here and re-exported by it — the single source is this module.

mod analysis;
mod chat;
mod memory;
mod providers;
mod skills;
mod write;

pub use analysis::{
    Bottleneck, BottleneckSeverity, DiagnosisResult, ExplainAnalysis, ExplainSuggestion,
};
pub use chat::{
    AiChatAttribution, AiChatContextSnapshot, AiChatInjectionEvent, AiChatInjectionSnapshot,
    AiChatMessage, AiChatResultPreview, AiChatSession, AiLoadedSkill, AiMemoryCandidatesEvent,
    AiMessageRole, AiMessageStatus, AiPreviewCell, AiQuestion, AiQuestionOption,
    AiSessionDetail, AiSessionMessage, AiSessionMessageInput, AiSessionSummary,
    AiSessionUpsertInput, AiToolCall, AiToolResult, StreamChunkPayload, StreamChunkUsage,
    StreamErrorPayload,
};
pub use memory::{
    AiMemoryCloudPolicy, AiMemoryCorrectionInput, AiMemoryCreateInput, AiMemoryEntry,
    AiMemoryExtractionInput, AiMemoryKind, AiMemoryListParams, AiMemoryPage, AiMemoryReference,
    AiMemoryScopeKind, AiMemorySensitivity, AiMemorySettings, AiMemorySqlSample, AiMemoryStaleInput,
    AiMemoryStats, AiMemoryStatus, AiMemoryUpdateInput, DEFAULT_AI_MEMORY_SETTINGS,
};
pub use providers::{
    AiAgentMode, AiDataEgressLevel, AiModelProfile, AiProviderConfig, AiProviderType,
    AiSafetyGateConfig, AiSettingsConfig, AiToolPermissionPolicy, ModelInfo, ProviderListItem,
    PromptTemplate,
};
pub use skills::{
    AiSkillContent, AiSkillSource, AiSkillSummary, AiSkillsSettings, DEFAULT_AI_SKILLS_SETTINGS,
};
pub use write::{
    AiWriteApplyReceipt, AiWriteAuditDecision, AiWriteAuditDecisionKind, AiWriteAuditRecord,
    AiWriteImpact, AiWriteProposal, AiWriteStatement,
};
