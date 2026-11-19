//! Shared value types for the panel slice — T009-S6.
//!
//! The `aiStore` call payloads [`AiSentMessage`] and
//! [`AiMemoryCreateRequest`] plus the two prop enums, kept apart from
//! [`super::ai_chat_panel_state::AiChatPanelState`] so the state file
//! stays under the 800-line cap. They are pure data: no store exists
//! yet (T088), so the panel records these instead of calling through.

// The panel view (S7) is the production consumer. The dead-code lint
// stays silenced here rather than in a crate-wide allow: the
// T088/T169-owned surface (store seeders, on_run_code / on_new_query
// pass-throughs) still has no production caller.
#![allow(dead_code)]

use super::ai_chat_header::AiAgentMode;
use super::ai_chat_panel::AiChatContextSnapshot;

/// React's `memoryMode` prop (`'off' | 'ask' | 'auto'`), plus the
/// `memoryModeFor` fallback: the settings memory switch off yields
/// `'off'`, on yields `'auto'` (`lib/aiMemorySettings.ts:34`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AiMemoryMode {
    Off,
    Ask,
    Auto,
}

/// React's `memoryScope` prop; the panel defaults it to `'user'`
/// (`AiChatPanel.tsx:114`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum AiMemoryScope {
    Organization,
    #[default]
    User,
    Connection,
    Database,
    Schema,
    Table,
    Column,
}

/// One `sendMessage` call the panel would make (`AiChatPanel.tsx:310–
/// 353` for `handleSend`, `:710–718` for `onAnswerQuestions`).
/// `content` is the composed message — the KV block appended after
/// `"\n\n"` while a KV context is in scope
/// (`composeKvAiMessage`, `lib/kvAiContext.ts:87`) — and `raw_content`
/// is the draft as typed, which is what the appended user bubble shows.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AiSentMessage {
    pub content: String,
    pub raw_content: String,
    pub agent_mode: AiAgentMode,
    pub template_id: Option<String>,
    pub memory_mode: AiMemoryMode,
    pub memory_scope: AiMemoryScope,
    pub auto_context: Option<AiChatContextSnapshot>,
    pub context_items: usize,
    /// Always `None`: `claimForSend()` from `useAiEditorGrant` is not
    /// wired (see the module docs).
    pub target_claim: Option<String>,
}

/// One `aiCommands.memoryCreate` call the panel would make
/// (`AiChatPanel.tsx:296–326` for `handleRemember`): a `preference`
/// memory, `explicit` source, `candidate` status, `normal`
/// sensitivity, `0.6` confidence, scoped to the connection when one is
/// in scope and to the user otherwise.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AiMemoryCreateRequest {
    pub scope_type: &'static str,
    pub connection_id: Option<String>,
    pub kind: &'static str,
    pub subject: String,
    pub content: String,
    pub source: &'static str,
    pub source_ref: Option<String>,
    pub status: &'static str,
    pub sensitivity: &'static str,
    pub confidence: f32,
}
