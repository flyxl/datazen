//! The seams `stores/ai/streaming.ts` reaches through its imports.
//!
//! Every sibling / cross-segment module becomes an injected trait or fn
//! seam here (split discipline: this module ports streaming.ts's semantics
//! only); the lean type mirrors carry the fields streaming.ts actually
//! touches, and each one's doc names the migration task that owns the real
//! port. The one pure helper, [`find_mcp_tool_name`], has no dependency and
//! is ported whole.

use crate::ai_sessions::{AiSessionStore, SessionPersistResult};
use crate::ai_types::{AiChatSession, AiMemoryReference, AiQuestion, AiToolCall};

/// React `findMcpToolName(toolCalls?)`: the first tool call whose name
/// starts with `mcp/`, or `None`. Pure — no seams, ported whole.
pub fn find_mcp_tool_name(tool_calls: Option<&[AiToolCall]>) -> Option<&str> {
    tool_calls?
        .iter()
        .find(|call| call.name.starts_with("mcp/"))
        .map(|call| call.name.as_str())
}

/// React `AiTargetClaim` (`ai/targetClaim.ts`) — lean mirror of the two
/// arms, kept to the three fields the attribution projection reads.
///
/// **Ownership**: the claim constructors (`independent()`,
/// `editorGrant()`, `isEditorGrant()`) and the grant-family types
/// (`AiGrantReceipt`, `AiSessionHandle`) belong to the `ai/targetClaim.ts`
/// port task (cross-segment, not started); when it lands it should extend
/// this mirror rather than fork it. Not a wire type for this module — the
/// serde profile is that task's call.
#[derive(Clone, Debug, PartialEq)]
pub enum AiTargetClaim {
    /// `{ kind: 'independent' }` — the run owns its own runtime context.
    Independent,
    /// `{ kind: 'editorGrant'; grantId; claimedClientInstanceId;
    /// editorSessionId }` — the run shares an editor session the user
    /// explicitly granted.
    EditorGrant {
        grant_id: String,
        claimed_client_instance_id: String,
        editor_session_id: String,
    },
}

/// React `CancelReceipt` (`ai/targetClaim.ts`) — the three-state answer to
/// `ai_cancel`, lean-mirrored to the shape streaming.ts's `cancelChat`
/// resolves with untouched.
///
/// **Ownership**: the receipt's interpretation — `marked` → the registry
/// enters `cancelling` and the 8s fallback arms, `alreadyFinished` → no
/// transition, `forbidden` → warn-only — is `ai/sessionCancel.ts`'s port
/// task (cross-segment, not started); when it lands it should extend this
/// mirror (serde for the wire, helpers) rather than fork it.
#[derive(Clone, Debug, PartialEq)]
pub enum CancelReceipt {
    /// The backend accepted the cancel and will stop the run.
    Marked { request_id: String },
    /// The run reached its terminal state before the cancel.
    AlreadyFinished { request_id: String },
    /// This caller/target may not cancel the run.
    Forbidden { request_id: String },
}

/// React `ApplyChunkInput` (`ai/streamRegistry.ts`) — lean-mirrored to the
/// two fields streaming.ts passes (`{ done, cancelled }`; the `error`
/// field is the registry's inline-error arm, unused here). Owned by the
/// `ai/streamRegistry.ts` port task alongside the registry itself.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ApplyChunkGate {
    /// Terminal marker of the backend chunk.
    pub done: bool,
    /// Terminal marker: `done && cancelled` means the run ended by cancel.
    pub cancelled: Option<bool>,
}

/// React `ApplyChunkResult` — lean-mirrored to the one field streaming.ts
/// reads (`applied.accepted`); the drop reason and the phase/generation
/// telemetry belong to the real registry port (`ai/streamRegistry.ts`,
/// cross-segment, not started).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ApplyGateResult {
    pub accepted: bool,
}

/// The `StreamRegistry` seam — the three calls streaming.ts makes on the
/// module-level singleton (`has`, `applyChunk`, `applyError`).
///
/// **Ownership**: the real registry — the phase machine
/// (`starting`/`streaming`/`cancelling`/`terminal`), the per-conversation
/// generation bump that supersedes runs, the drop reasons, the event
/// stream — is `ai/streamRegistry.ts`'s port task (cross-segment, not
/// started). Its adapter implements this trait over the real state machine
/// and is injected as `AiStreaming`'s registry field, keeping React's
/// same-instance-across-calls semantics. The `begin()` the send path calls
/// is deliberately **not** in the seam (streaming.ts never calls it): the
/// host keeps its own concrete registry handle from the construction site,
/// which is where the aiStore port (T128) reaches it.
pub trait StreamRegistryOps {
    /// `streamRegistry.has(requestId)`.
    fn has(&self, request_id: &str) -> bool;
    /// `streamRegistry.applyChunk(requestId, { done, cancelled })`.
    fn apply_chunk(&mut self, request_id: &str, chunk: &ApplyChunkGate) -> ApplyGateResult;
    /// `streamRegistry.applyError(requestId)` — idempotent terminal settle.
    fn apply_error(&mut self, request_id: &str) -> ApplyGateResult;
}

/// The `SessionCanceller` seam — streaming.ts's `cancelChat` delegates to
/// the canceller's `cancel`, which resolves with the receipt or `null`.
///
/// **Ownership**: the real orchestrator is `ai/sessionCancel.ts`'s port
/// task (cross-segment, not started) — the receipt translation (`marked` →
/// `markCancelling` + the 8s fallback timer, `alreadyFinished` → nothing,
/// `forbidden` → warn-only, transport failure → `null`) is that task's,
/// not this module's. The wiring streaming.ts itself contributes — React's
/// `cancellerFor` — is documented here because the future adapter must be
/// built with it: construct the canceller **once** with
/// (a) the same registry instance `AiStreaming` owns,
/// (b) an [`AiCancelCommand`] (the `aiCommands.cancel` seam — `Result`
/// per the T040 await→`Result` precedent; the commands/ai port task owns
/// the adapter), and
/// (c) `AiStreaming::handle_cancel_timeout` as the `onCancelTimeout`
/// callback the 8s fallback fires.
/// Construction-time injection of the field preserves React's
/// lazy-singleton semantics — the same instance across every call.
pub trait SessionCancellerOps {
    /// `canceller.cancel(requestId)` — `None` is React's `null` (the IPC
    /// call itself failed; the run keeps streaming).
    fn cancel(&mut self, request_id: &str) -> Option<CancelReceipt>;
}

/// The `aiCommands.cancel(requestId)` seam (`commands/ai.ts`, commands
/// segment — not ported).
///
/// React's promise maps to `Result` exactly the way the T040 sessions port
/// mapped `aiCommands.sessions*`: `Ok` is the host receipt, `Err` is the
/// rejection's display message. **No production consumer lives in this
/// module yet** — the seam exists because streaming.ts's `cancellerFor`
/// wiring is where the boundary gets its name; the real
/// [`SessionCancellerOps`] adapter (the ai/sessionCancel port task)
/// consumes it, and the commands/ai port task provides the implementation.
/// `test_support`'s `WiredCanceller` demonstrates the composition.
pub trait AiCancelCommand {
    /// `aiCommands.cancel(requestId)` — await → `Result`.
    fn cancel(&self, request_id: &str) -> Result<CancelReceipt, String>;
}

/// The streaming-relevant slice of React's `Nl2SqlState`
/// (`stores/ai/types.ts`) — the six fields streaming.ts actually reads and
/// writes: `requestId`, `streamingSql`, `streamingPreview`, `generatedSql`,
/// `memoryUsed`, `isGenerating`.
///
/// **Ownership**: the full state (`input`, and whatever later tasks add)
/// belongs to the AiStore port (T126, this segment; wiring T128): the host
/// adapter merges each `set_nl2sql*` write into its full slice the way
/// React's `{ ...nl2sql, … }` spread does. Not a wire type — plain
/// struct.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Nl2sqlStreamingSlice {
    /// The in-flight NL2SQL generation's request id (`null` when idle).
    pub request_id: Option<String>,
    /// Raw accumulated content during streaming (before extraction).
    pub streaming_sql: String,
    /// Extracted SQL preview for display during streaming.
    pub streaming_preview: String,
    /// Final cleaned SQL after generation completes.
    pub generated_sql: String,
    /// Memories the host injected for the latest generation.
    pub memory_used: Vec<AiMemoryReference>,
    pub is_generating: bool,
}

/// The lean zustand `AiStore` view streaming.ts consumes: `get` is the
/// three reads (`chatSession`, `workflowChat`, `nl2sql`) and `set` is the
/// four partial shapes the module writes (`{ chatSession }`,
/// `{ workflowChat }`, `{ nl2sql }` / `{ nl2sql, nl2sqlError }`, and
/// `{ sessionMigrationError }`).
///
/// Reads return owned snapshots (React reads the live store by reference;
/// a clone-on-read view is the pragmatic seam spelling). **Ownership**:
/// the full `AiStore` type is T126's (this segment) and the host wiring is
/// T128's — the real adapter implements this trait over the ported store
/// when those land.
pub trait AiStoreView {
    /// `get().chatSession` — `AiChatSession | null`.
    fn chat_session(&self) -> Option<AiChatSession>;
    /// `get().workflowChat` — `AiChatSession | null`.
    fn workflow_chat(&self) -> Option<AiChatSession>;
    /// `get().nl2sql` — the streaming-relevant slice.
    fn nl2sql(&self) -> Nl2sqlStreamingSlice;
    /// `set({ chatSession })`.
    fn set_chat_session(&mut self, session: AiChatSession);
    /// `set({ workflowChat })`.
    fn set_workflow_chat(&mut self, session: AiChatSession);
    /// `set({ nl2sql })` — the chunk arm's shape (`nl2sqlError` untouched).
    fn set_nl2sql(&mut self, nl2sql: Nl2sqlStreamingSlice);
    /// `set({ nl2sql, nl2sqlError })` — the error arm's paired shape.
    fn set_nl2sql_error(&mut self, nl2sql: Nl2sqlStreamingSlice, error: String);
    /// `set({ sessionMigrationError })` — the persist-failure report.
    fn set_session_migration_error(&mut self, error: String);
}

/// The `{ cleanContent, questions }` return of React `extractQuestions`
/// (`lib/extractQuestions.ts`, T503 — not ported). Lean mirror: the real
/// implementation lands with that task and is injected through
/// [`ExtractionSeams`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExtractQuestionsOutcome {
    /// The content with `<ask_questions>` blocks removed.
    pub clean_content: String,
    /// The parsed questions (empty when none parsed).
    pub questions: Vec<AiQuestion>,
}

/// The four extraction imports streaming.ts calls, injected as fn seams
/// (`lib/`, this segment — not ported):
///
/// - `extractQuestions` / `parseToolCallQuestions` → T503
///   (`lib/extractQuestions.ts`),
/// - `extractSqlFromResponse` / `extractSqlStreaming` → T505
///   (`lib/extractSql.ts`).
///
/// The real implementations land with those tasks and are wired at
/// construction; nothing in this module hard-codes an extraction rule.
pub struct ExtractionSeams {
    /// `extractQuestions(content)` → `{ cleanContent, questions }`.
    pub extract_questions: Box<dyn Fn(&str) -> ExtractQuestionsOutcome>,
    /// `parseToolCallQuestions(toolCalls)` → questions or empty.
    pub parse_tool_call_questions: Box<dyn Fn(&[AiToolCall]) -> Vec<AiQuestion>>,
    /// `extractSqlFromResponse(raw)` — the terminal SQL cleanup.
    pub extract_sql_from_response: Box<dyn Fn(&str) -> String>,
    /// `extractSqlStreaming(accumulated)` — the un-trimmed streaming
    /// preview (handles unclosed fences).
    pub extract_sql_streaming: Box<dyn Fn(&str) -> String>,
}

/// React's `new Date().toISOString()` (three message constructions) — the
/// ai_sessions `SessionClock` precedent, lean to the one read this module
/// makes (streaming.ts never reads `Date.now()`, so there is no `now_ms`
/// seam).
pub struct StreamingClock {
    pub now_iso: Box<dyn Fn() -> String>,
}

/// The `persistChatSession` seam — **already ported** (T040:
/// `ai_sessions::AiSessionStore::persist_chat_session`), so the blanket
/// impl below reuses that store directly. Hosts inject the real store; the
/// streaming-side logic that *is* this module's (the visible-session guard
/// and the `sessionMigrationError` report) lives in the adapter.
pub trait ChatSessionPersister {
    /// React `persistChatSession(session)` — the
    /// [`SessionPersistResult`] the promise resolves with.
    fn persist_chat_session(&mut self, session: &AiChatSession) -> SessionPersistResult;
}

impl ChatSessionPersister for AiSessionStore {
    fn persist_chat_session(&mut self, session: &AiChatSession) -> SessionPersistResult {
        AiSessionStore::persist_chat_session(self, session)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(name: &str) -> AiToolCall {
        AiToolCall {
            id: format!("tc-{name}"),
            name: name.to_owned(),
            arguments: String::new(),
        }
    }

    #[test]
    fn find_mcp_tool_name_picks_the_first_mcp_prefixed_call() {
        // React: `toolCalls?.find((tc) => tc.name.startsWith('mcp/'))?.name ?? null`.
        let calls = vec![call("query_db"), call("mcp/server/tool"), call("mcp/other")];
        assert_eq!(find_mcp_tool_name(Some(&calls)), Some("mcp/server/tool"));
        // The prefix alone qualifies — an unprefixed `mcp-anything` does not.
        let calls = vec![call("mcp-x"), call("plain")];
        assert_eq!(find_mcp_tool_name(Some(&calls)), None);
        // The bare prefix itself matches (`startsWith`, not a full-name eq).
        let calls = vec![call("mcp/")];
        assert_eq!(find_mcp_tool_name(Some(&calls)), Some("mcp/"));
    }

    #[test]
    fn find_mcp_tool_name_is_none_without_any_calls() {
        assert_eq!(find_mcp_tool_name(None), None);
        assert_eq!(find_mcp_tool_name(Some(&[])), None);
    }
}
