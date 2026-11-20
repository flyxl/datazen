//! Port of `packages/frontend/src/commands/ai.ts` (T094, split) — the AI
//! command family's typed IPC shell.
//!
//! The React file is one `aiCommands` object of ~70 `legacyInvoke`
//! wrappers plus the workflow slice over `@datazen/backend-client`'s
//! domain client and three cross-window event subscriptions. On the Rust
//! side the IPC boundary collapses into the host's transport: this module
//! is the **payload shaper layer** (the `schema_diff` runner pattern) —
//! every wrapper becomes a function returning the exact wire request
//! ([`AiRequest`]: command name + camelCase payload, key-for-key with the
//! React source), and the host executes it against
//! `DesktopAdapter`/the domain-client seam, delivering typed receipts.
//!
//! ## Split
//!
//! - [`wire`] — the `ai_*` and `prompt_*` legacy shapers, the
//!   E2E-stub policy and the prompt vocabulary types.
//! - [`mcp`] — the `mcp_*` host/client/approval shapers.
//! - [`workflow`] — the workflow domain-client shapers (the
//!   `MethodMap` payloads, the `saveUserAsset` submission token, and the
//!   desktop-only `workflow_get_dir` legacy command).
//!
//! ## What maps where
//!
//! - `legacyInvoke(cmd, args)` → [`AiRequest`] (command + payload); the
//!   `result.data as T` casts become the host's typed serde decode.
//! - `e2eStubInvoke` (only `ai_chat` and `ai_generate_sql` reach it) is
//!   [`E2eStubPolicy::Capturable`]: the E2E harness may answer a captured
//!   call with `''`. The grant lifecycle (`grantEditorSession` /
//!   `revokeEditorGrant` / `touchEditorGrant`) goes through
//!   `nativeInvoke` and is [`E2eStubPolicy::NativeOnly`] — the stub's
//!   empty string would be an invalid receipt and must never be mistaken
//!   for a granted session (the React file's own warning, kept as the
//!   policy the host's stub layer reads).
//! - The `?? null` normalizations (`driverType`, `limit`,
//!   `sessionsList`'s three keys) are applied by the shapers, exactly as
//!   the React wrappers write them onto the wire.
//! - JS `undefined`-valued keys are dropped by `JSON.stringify`: the
//!   optional members of the big param structs
//!   ([`GenerateSqlParams`], [`ChatParams`]) are
//!   `skip_serializing_if = "Option::is_none"`, reproducing the wire.
//! - The three event subscriptions (`onAiStreamChunk` /
//!   `onAiStreamError` / `onAiConfigChanged`) become the channel-name
//!   consts plus the [`StreamChunkPayload`] / [`StreamErrorPayload`]
//!   serde shapes the host's event edge deserializes before feeding the
//!   store — the cross-window edge itself lives outside this crate (the
//!   `ai_write_proposal_host` precedent), and `AiStore::note_listener_installed`
//!   is the armed-listener bit the host sets once it subscribed.
//! - `mergeDraftCommandFields` (the draft bridge `workflowSave` applies
//!   before sending) is `windows/workflow/draftBridge.ts` — this
//!   segment's T482. The [`workflow::save_workflow_request`] shaper takes
//!   the definition as given; the merge is the caller's (UI-side
//!   orchestration, kept in front of the command face exactly as the
//!   React comment rules).
//!
//! Known deltas: none behavioural — every command name, payload key and
//! normalization is the React file's verbatim.

pub(crate) mod mcp;
pub(crate) mod wire;
pub(crate) mod workflow;

pub(crate) use mcp::*;
pub(crate) use wire::{
    AiRequest, CHAT_COMMAND, GENERATE_SQL_COMMAND, PromptInfo, PromptOverrideEntry,
    PromptScenario, PromptSource, StreamChunkPayload, StreamErrorPayload, StreamUsage,
};
pub(crate) use workflow::{
    WorkflowMethod, save_workflow_yaml_request, with_submission_token, workflow_get_dir_request,
};

/// `onAiStreamChunk`'s channel (`requirePlatform().events.listenCrossWindow`).
pub(crate) const AI_STREAM_CHUNK_CHANNEL: &str = "ai:stream-chunk";
/// `onAiStreamError`'s channel.
pub(crate) const AI_STREAM_ERROR_CHANNEL: &str = "ai:stream-error";
/// `onAiConfigChanged`'s channel (the payload is the bare `isConfigured`
/// boolean).
pub(crate) const AI_CONFIG_CHANGED_CHANNEL: &str = "ai:config-changed";
