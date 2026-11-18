//! Chat messages and sessions, backend session rows, injection events and
//! the stream payloads — React `types/ai.ts` lines 180–202, 269–283 and
//! 446–597.
//!
//! This is the family `stores/ai/sessions.ts` (T040,
//! [`crate::ai_sessions`]) consumes; the ten session-family types the T040
//! module mirrored minimally live here in full, and that module re-exports
//! them. The serde profiles of those ten are kept **byte-identical** to the
//! T040 mirrors (camelCase keys, `#[serde(default, skip_serializing_if =
//! "Option::is_none")]` on every optional, the `null`→`None` fold), so the
//! localStorage blob and the `aiCommands.sessions*` payloads round-trip
//! exactly as they did before the unification. What changes is typing, not
//! the wire: the pass-through message fields (`reasoning`, `questions`,
//! `toolCalls`, `toolCallId`, `memoryUsed`) are now the typed
//! [`AiQuestion`]/[`AiToolCall`]/[`AiMemoryReference`] shapes React
//! declares, instead of opaque JSON.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::memory::AiMemoryReference;

/// React `AiMessageStatus = 'complete' | 'cancelled' | 'failed'` — the
/// persisted terminal state of one message.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AiMessageStatus {
    Complete,
    Cancelled,
    Failed,
}

impl AiMessageStatus {
    /// The wire spelling (`'complete' | 'cancelled' | 'failed'`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
        }
    }
}

/// React's message role `'system' | 'user' | 'assistant' | 'tool'` — the
/// inline union `AiChatMessage`/`AiSessionMessage`/`AiSessionMessageInput`
/// all share.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AiMessageRole {
    System,
    User,
    Assistant,
    Tool,
}

impl AiMessageRole {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::Tool => "tool",
        }
    }
}

/// React `AiQuestionOption` — one answer of a question the assistant asks.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiQuestionOption {
    pub id: String,
    pub label: String,
}

/// React `AiQuestion` — a clarification question attached to a message.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiQuestion {
    pub id: String,
    pub prompt: String,
    pub options: Vec<AiQuestionOption>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_multiple: Option<bool>,
}

/// React `AiToolCall` — one tool invocation the model requested.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiToolCall {
    pub id: String,
    pub name: String,
    /// Raw JSON argument blob as a string (the wire keeps it unparsed).
    pub arguments: String,
}

/// React `AiToolResult` — one tool answer fed back to the model.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiToolResult {
    pub tool_call_id: String,
    pub content: String,
}

/// One cell of a frozen result preview — React's inline
/// `(string | number | boolean | null)` union. Untagged serde keeps the
/// JSON scalar spelling exactly; the number arm is `serde_json::Number`
/// (not `f64`) so an integer cell round-trips as `7`, not `7.0` — the
/// wire spelling React froze is part of the preview contract.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AiPreviewCell {
    String(String),
    Number(serde_json::Number),
    Bool(bool),
    Null,
}

/// React `AiChatResultPreview` — a result excerpt frozen into a chat turn;
/// the Rust side re-caps it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiChatResultPreview {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<AiPreviewCell>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_count: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elapsed_ms: Option<u64>,
}

/// React `AiChatContextSnapshot` — auto-attached editor/result facts,
/// frozen when the user sends.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiChatContextSnapshot {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub editor_sql: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_preview: Option<AiChatResultPreview>,
}

/// React `AiChatMessage` — one live chat turn.
///
/// The T040 mirror kept the five pass-through fields (`reasoning`,
/// `questions`, `toolCalls`, `toolCallId`, `memoryUsed`) as opaque
/// `serde_json::Value` because their shapes belonged to this task; here
/// they are the typed React shapes. Consequences of the typing, all
/// inert for sessions.ts's logic and documented: a JSON `null` for one of
/// them folds to `None` (the key then stays absent on re-serialization,
/// where React would keep an explicit null), and a malformed value (e.g. a
/// `toolCalls` entry missing `arguments`) fails deserialization instead
/// of being carried unchecked.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiChatMessage {
    /// Stable message identifier for stable React keys.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub role: AiMessageRole,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub questions: Option<Vec<AiQuestion>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<AiToolCall>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Terminal marker: the run ended because the user cancelled it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancelled: Option<bool>,
    /// Persisted terminal state; cancellation/failure survives a reload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<AiMessageStatus>,
    /// Stable creation time so AppDb replace updates keep message
    /// timestamps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// Active memories injected for this assistant turn, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_used: Option<Vec<AiMemoryReference>>,
}

impl AiChatMessage {
    /// A minimal message with only the required fields — a Rust-side
    /// convenience (React builds the object literals inline); the test
    /// suites and host fixtures use it.
    pub fn new(role: AiMessageRole, content: impl Into<String>) -> Self {
        Self {
            id: None,
            role,
            content: content.into(),
            reasoning: None,
            questions: None,
            tool_calls: None,
            tool_call_id: None,
            cancelled: None,
            status: None,
            created_at: None,
            memory_used: None,
        }
    }
}

/// React `AiLoadedSkill` — a skill whose body was loaded into a session.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiLoadedSkill {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub loaded_at: String,
}

/// React `AiChatInjectionSnapshot` — the host's prompt material snapshot
/// for one run's chips.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiChatInjectionSnapshot {
    /// `string | null | undefined` in React: null and absent both fold to
    /// `None` (nothing in the UI branches on the distinction).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_hash: Option<String>,
    pub has_connection_notes: bool,
    pub has_global_instructions: bool,
}

/// React `AiChatInjectionEvent` — the host announces the run's prompt
/// material.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiChatInjectionEvent {
    pub request_id: String,
    pub snapshot: AiChatInjectionSnapshot,
}

/// React `AiMemoryCandidatesEvent` — the host announces the memories it
/// is about to inject.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiMemoryCandidatesEvent {
    pub request_id: String,
    pub candidates: Vec<AiMemoryReference>,
}

/// React `AiSessionMessage` — one AppDb session row (the persisted form;
/// distinct from the live [`AiChatMessage`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSessionMessage {
    pub id: String,
    pub role: AiMessageRole,
    pub content: String,
    pub status: AiMessageStatus,
    /// `Record<string, unknown> | null` — opaque pass-through bag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
    pub created_at: String,
}

/// React `AiSessionSummary` — one list entry of the backend session list.
///
/// `connectionId`/`database` are `string | null | undefined` in React; the
/// distinction is inert for every consumer (truthiness and `?? ''`
/// normalization only), so both fold to `None`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSessionSummary {
    pub id: String,
    pub session_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
    pub message_count: usize,
    pub preview: String,
    pub created_at: String,
    pub updated_at: String,
    pub last_access_at: String,
}

/// React `AiSessionDetail extends AiSessionSummary` — the full row with
/// messages. Flattened the same way the T040 mirror flattened it: the
/// detail JSON carries all summary fields at the top level.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSessionDetail {
    // ── AiSessionSummary (flattened base) ──
    pub id: String,
    pub session_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
    pub message_count: usize,
    pub preview: String,
    pub created_at: String,
    pub updated_at: String,
    pub last_access_at: String,
    // ── AiSessionDetail's own fields ──
    pub messages: Vec<AiSessionMessage>,
}

impl AiSessionDetail {
    /// The summary projection of this detail. React has no such helper —
    /// its test double builds one by spread — but adapters converting a
    /// detail row into a list entry need exactly this shape.
    pub fn to_summary(&self) -> AiSessionSummary {
        AiSessionSummary {
            id: self.id.clone(),
            session_key: self.session_key.clone(),
            connection_id: self.connection_id.clone(),
            database: self.database.clone(),
            title: self.title.clone(),
            metadata: self.metadata.clone(),
            message_count: self.message_count,
            preview: self.preview.clone(),
            created_at: self.created_at.clone(),
            updated_at: self.updated_at.clone(),
            last_access_at: self.last_access_at.clone(),
        }
    }
}

/// React `AiSessionMessageInput` — the upsert payload's message row.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSessionMessageInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub role: AiMessageRole,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<AiMessageStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

/// React `AiSessionUpsertInput` — the `aiCommands.sessionsUpsert` payload.
///
/// `append` stays `Option<bool>` because the producing side distinguishes
/// the two spellings: `chatSessionToUpsertInput` writes an explicit
/// `false`, the legacy migration omits the key entirely (backend default).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSessionUpsertInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub session_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_access_at: Option<String>,
    pub messages: Vec<AiSessionMessageInput>,
    /// `None` = key omitted (backend default false); `Some(false)` =
    /// explicit replace. `true` appends and de-duplicates ids.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub append: Option<bool>,
}

/// React `AiChatSession`'s `attribution` union — non-secret attribution of
/// the last run started from the session: which target it claimed, and
/// (for a shared run) the grant id. Never the runtime handle itself.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AiChatAttribution {
    /// `{ kind: 'independent' }` — the run claimed no editor target.
    Independent,
    /// `{ kind: 'editorGrant'; grantId: string; editorSessionId: string }`
    /// — the run shares an editor's session.
    #[serde(rename_all = "camelCase")]
    EditorGrant {
        grant_id: String,
        editor_session_id: String,
    },
}

/// React `AiChatSession` — the live session the chat panel renders.
///
/// Not a wire type: React never serializes it (the persisted forms are the
/// localStorage blob and the AppDb rows owned by
/// [`crate::ai_sessions`]), so no serde derive here — same as the T040
/// mirror. The streaming half is present because the store *constructs*
/// sessions with fixed defaults on every restore/new path; its
/// transitions belong to `stores/aiStore.ts` (T128) and
/// `stores/ai/streaming.ts`. `injection` and `attribution` are optional in
/// React and stay `None`/absent on every constructor this crate owns.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AiChatSession {
    pub id: String,
    /// Session isolation key used for localStorage persistence.
    pub session_key: String,
    /// Connection/database scope used by the backend session list.
    pub connection_id: Option<String>,
    pub database: Option<String>,
    pub title: Option<String>,
    /// Prompt template / memory policy metadata retained with the session.
    pub metadata: Option<Value>,
    pub messages: Vec<AiChatMessage>,
    pub is_streaming: bool,
    pub stream_content: String,
    pub stream_reasoning: String,
    /// Qualified MCP tool name (mcp/server/tool) shown during streaming
    /// tool execution.
    pub stream_mcp_tool_name: Option<String>,
    /// Host-provided egress scope from the first chunk of this run.
    pub egress_summary: Option<String>,
    /// Accumulated memory references for the in-flight run.
    pub stream_memory_used: Option<Vec<AiMemoryReference>>,
    /// Latest run's memory references; reset on send/clear/new session.
    pub memory_used: Option<Vec<AiMemoryReference>>,
    /// Skills whose bodies were loaded in this session (session-scoped).
    /// `?: AiLoadedSkill[]` in React folds to an empty vec because every
    /// constructor this crate owns materializes the empty list.
    pub loaded_skills: Vec<AiLoadedSkill>,
    /// Host-provided prompt material snapshot for this run's chips.
    pub injection: Option<AiChatInjectionSnapshot>,
    pub request_id: Option<String>,
    /// Non-secret attribution of the last run started from this session.
    pub attribution: Option<AiChatAttribution>,
}

/// React `StreamChunkPayload.usage` — the anonymous inline
/// `{ promptTokens: number; completionTokens: number; totalTokens:
/// number }` object, named here because Rust needs one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamChunkUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

/// React `StreamChunkPayload` — one SSE chunk of a streaming chat run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamChunkPayload {
    pub request_id: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    pub done: bool,
    /// `done && cancelled` is the terminal chunk of a cancelled run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancelled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<StreamChunkUsage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<AiToolCall>>,
    /// FR-13 egress scope; the host sends it on the first chunk only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub egress_summary: Option<String>,
    /// Active memories injected into this run; the host sends it once up
    /// front.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_used: Option<Vec<AiMemoryReference>>,
}

/// React `StreamErrorPayload` — the terminal error chunk of a run.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamErrorPayload {
    pub request_id: String,
    pub error: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode<T: serde::de::DeserializeOwned>(wire: serde_json::Value) -> T {
        serde_json::from_value(wire)
            .unwrap_or_else(|error| panic!("wire decode failed: {error}"))
    }

    #[test]
    fn message_statuses_keep_the_react_literals() {
        let table = [
            (AiMessageStatus::Complete, "complete"),
            (AiMessageStatus::Cancelled, "cancelled"),
            (AiMessageStatus::Failed, "failed"),
        ];
        for (value, literal) in table {
            assert_eq!(value.as_str(), literal);
            let parsed: AiMessageStatus = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(serde_json::to_value(parsed).ok(), Some(serde_json::json!(literal)));
        }
    }

    #[test]
    fn message_roles_keep_the_react_literals() {
        let table = [
            (AiMessageRole::System, "system"),
            (AiMessageRole::User, "user"),
            (AiMessageRole::Assistant, "assistant"),
            (AiMessageRole::Tool, "tool"),
        ];
        for (value, literal) in table {
            assert_eq!(value.as_str(), literal);
            let parsed: AiMessageRole = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(serde_json::to_value(parsed).ok(), Some(serde_json::json!(literal)));
        }
        // A role outside the vocabulary is a contract violation (the T040
        // corrupted-storage branch drops such messages).
        assert!(serde_json::from_value::<AiMessageRole>(serde_json::json!("robot")).is_err());
    }

    /// A fully-loaded live message round-trips: the pass-through fields
    /// carry their typed shapes under React's camelCase keys.
    #[test]
    fn a_chat_message_round_trips_its_typed_pass_through_fields() {
        let wire = serde_json::json!({
            "id": "m1",
            "role": "assistant",
            "content": "Here is the plan.",
            "reasoning": "the user asked for a plan",
            "questions": [{
                "id": "q1",
                "prompt": "Which database?",
                "options": [{ "id": "o1", "label": "shop" }, { "id": "o2", "label": "crm" }],
                "allowMultiple": true
            }],
            "toolCalls": [{
                "id": "call-1",
                "name": "query_tool",
                "arguments": "{\"sql\":\"select 1\"}"
            }],
            "toolCallId": "call-1",
            "cancelled": false,
            "status": "complete",
            "createdAt": "2026-01-01T00:00:00Z",
            "memoryUsed": [{
                "reference": "M1",
                "id": "mem-1",
                "subject": "prefer lowercase",
                "kind": "preference",
                "scope": "user",
                "score": 0.5
            }]
        });
        let message: AiChatMessage = decode(wire.clone());
        assert_eq!(message.role, AiMessageRole::Assistant);
        assert_eq!(message.reasoning.as_deref(), Some("the user asked for a plan"));
        assert_eq!(message.questions.as_ref().map(Vec::len), Some(1));
        assert_eq!(
            message.questions.as_ref().and_then(|questions| questions[0].allow_multiple),
            Some(true)
        );
        assert_eq!(
            message.tool_calls.as_ref().map(|calls| calls[0].name.as_str()),
            Some("query_tool")
        );
        assert_eq!(message.tool_call_id.as_deref(), Some("call-1"));
        assert_eq!(message.status, Some(AiMessageStatus::Complete));
        assert_eq!(message.memory_used.as_ref().map(Vec::len), Some(1));
        assert_eq!(
            message.memory_used.as_ref().map(|memories| memories[0].reference.as_str()),
            Some("M1")
        );
        assert_eq!(serde_json::to_value(&message).ok(), Some(wire));
    }

    /// A bare message keeps the T040 wire spelling exactly: required keys
    /// only, every optional key absent.
    #[test]
    fn a_bare_chat_message_matches_the_t040_wire_bytes() {
        let message = AiChatMessage::new(AiMessageRole::User, "hello");
        assert_eq!(
            serde_json::to_value(&message).ok(),
            Some(serde_json::json!({ "role": "user", "content": "hello" }))
        );
        // And the folded-null rule: a stored explicit null reads as
        // absent (documented deviation from React's unchecked null).
        let folded: AiChatMessage = decode(serde_json::json!({
            "role": "assistant",
            "content": "r",
            "reasoning": null
        }));
        assert_eq!(folded.reasoning, None);
        assert_eq!(
            serde_json::to_value(&folded).ok(),
            Some(serde_json::json!({ "role": "assistant", "content": "r" }))
        );
        // A malformed pass-through value fails the message decode instead
        // of being carried unchecked (the drop policy's declared shape is
        // now the full twin).
        assert!(serde_json::from_value::<AiChatMessage>(serde_json::json!({
            "role": "assistant",
            "content": "r",
            "toolCalls": [{ "name": "t" }]
        }))
        .is_err());
    }

    /// The backend session family round-trips byte-identically to the
    /// T040 mirrors (this is the regression that matters for
    /// `ai_sessions`: the stored/IPC shapes must not move).
    #[test]
    fn a_session_detail_round_trips_the_t040_wire() {
        let wire = serde_json::json!({
            "id": "s1",
            "sessionKey": "conn-1::db",
            "connectionId": "conn-1",
            "database": "db",
            "title": "Saved conversation",
            "metadata": { "templateId": "default" },
            "messageCount": 1,
            "preview": "hello",
            "createdAt": "2026-01-01T00:00:00Z",
            "updatedAt": "2026-01-01T00:00:01Z",
            "lastAccessAt": "2026-01-01T00:00:01Z",
            "messages": [{
                "id": "m1",
                "role": "user",
                "content": "hello",
                "status": "complete",
                "metadata": { "reasoning": "thinking", "cancelled": true },
                "createdAt": "2026-01-01T00:00:00Z"
            }]
        });
        let detail: AiSessionDetail = decode(wire.clone());
        assert_eq!(detail.message_count, 1);
        assert_eq!(detail.messages[0].status, AiMessageStatus::Complete);
        assert_eq!(serde_json::to_value(&detail).ok(), Some(wire));
        // The summary projection keeps the base fields.
        let summary = detail.to_summary();
        assert_eq!(summary.id, "s1");
        assert_eq!(summary.session_key, "conn-1::db");
        assert_eq!(summary.message_count, 1);
        assert_eq!(summary.metadata, detail.metadata);
        assert_eq!(summary.preview, "hello");
    }

    #[test]
    fn an_upsert_input_round_trips_the_append_spellings() {
        let wire = serde_json::json!({
            "sessionKey": "conn-1::db",
            "messages": [
                { "role": "user", "content": "hi" },
                { "id": "m2", "role": "assistant", "content": "r",
                  "status": "failed", "createdAt": "2026-01-01T00:00:00Z",
                  "metadata": { "cancelled": true } }
            ]
        });
        let input: AiSessionUpsertInput = decode(wire.clone());
        assert_eq!(input.append, None);
        assert_eq!(input.messages[0].id, None);
        assert_eq!(input.messages[1].status, Some(AiMessageStatus::Failed));
        assert_eq!(serde_json::to_value(&input).ok(), Some(wire));

        // The explicit replace spelling keeps the key present.
        let replace: AiSessionUpsertInput =
            decode(serde_json::json!({ "sessionKey": "k", "messages": [], "append": false }));
        assert_eq!(replace.append, Some(false));
    }

    #[test]
    fn a_live_session_defaults_to_the_constructor_shape() {
        let session = AiChatSession::default();
        assert!(!session.is_streaming);
        assert_eq!(session.stream_content, "");
        assert_eq!(session.stream_reasoning, "");
        assert_eq!(session.stream_mcp_tool_name, None);
        assert_eq!(session.egress_summary, None);
        assert_eq!(session.stream_memory_used, None);
        assert_eq!(session.memory_used, None);
        assert!(session.loaded_skills.is_empty());
        assert_eq!(session.injection, None);
        assert_eq!(session.request_id, None);
        assert_eq!(session.attribution, None);
        assert!(session.messages.is_empty());
    }

    /// The attribution union round-trips React's discriminated spellings.
    #[test]
    fn attributions_round_trip_their_discriminated_spellings() {
        let independent: AiChatAttribution = decode(serde_json::json!({ "kind": "independent" }));
        assert_eq!(independent, AiChatAttribution::Independent);
        assert_eq!(
            serde_json::to_value(&independent).ok(),
            Some(serde_json::json!({ "kind": "independent" }))
        );

        let grant: AiChatAttribution = decode(serde_json::json!({
            "kind": "editorGrant",
            "grantId": "g-1",
            "editorSessionId": "editor-9"
        }));
        assert_eq!(
            grant,
            AiChatAttribution::EditorGrant {
                grant_id: "g-1".to_string(),
                editor_session_id: "editor-9".to_string()
            }
        );
        assert_eq!(
            serde_json::to_value(&grant).ok(),
            Some(serde_json::json!({
                "kind": "editorGrant",
                "grantId": "g-1",
                "editorSessionId": "editor-9"
            }))
        );
    }

    /// The preview cells keep the four scalar spellings (untagged), and
    /// the context snapshot omits absent facts.
    #[test]
    fn preview_cells_and_context_snapshots_round_trip() {
        let wire = serde_json::json!({
            "columns": ["id", "name"],
            "rows": [
                ["1", 2.5, true, null],
                [null, "x", 7, false]
            ],
            "rowCount": 2,
            "elapsedMs": 31
        });
        let preview: AiChatResultPreview = decode(wire.clone());
        assert_eq!(preview.columns, vec!["id".to_string(), "name".to_string()]);
        assert_eq!(preview.rows[0][0], AiPreviewCell::String("1".to_string()));
        assert_eq!(preview.rows[0][2], AiPreviewCell::Bool(true));
        assert_eq!(preview.rows[0][3], AiPreviewCell::Null);
        // Number cells keep their exact wire spelling: the float stays a
        // float, the integer stays an integer (no 7 → 7.0 re-spelling).
        assert_eq!(serde_json::to_value(&preview.rows[0][1]).ok(), Some(serde_json::json!(2.5)));
        assert_eq!(serde_json::to_value(&preview.rows[1][2]).ok(), Some(serde_json::json!(7)));
        assert_eq!(preview.row_count, Some(2));
        assert_eq!(preview.elapsed_ms, Some(31));
        assert_eq!(serde_json::to_value(&preview).ok(), Some(wire));

        let snapshot: AiChatContextSnapshot = decode(serde_json::json!({
            "editorSql": "select 1",
            "lastError": "boom"
        }));
        assert_eq!(snapshot.editor_sql.as_deref(), Some("select 1"));
        assert_eq!(snapshot.selection, None);
        assert_eq!(snapshot.result_preview, None);
        assert_eq!(
            serde_json::to_value(&snapshot).ok(),
            Some(serde_json::json!({ "editorSql": "select 1", "lastError": "boom" }))
        );
    }

    #[test]
    fn stream_payloads_round_trip() {
        let wire = serde_json::json!({
            "requestId": "req-1",
            "content": "partial",
            "reasoning": "step",
            "done": false,
            "usage": { "promptTokens": 10, "completionTokens": 5, "totalTokens": 15 },
            "toolCalls": [{ "id": "call-1", "name": "t", "arguments": "{}" }],
            "egressSummary": "shop",
            "memoryUsed": [{
                "reference": "M1", "id": "mem-1", "subject": "s",
                "kind": "glossary", "scope": "user", "score": 0.9
            }]
        });
        let chunk: StreamChunkPayload = decode(wire.clone());
        assert_eq!(chunk.request_id, "req-1");
        assert!(!chunk.done);
        assert_eq!(chunk.cancelled, None);
        assert_eq!(chunk.usage.as_ref().map(|usage| usage.total_tokens), Some(15));
        assert_eq!(chunk.tool_calls.as_ref().map(Vec::len), Some(1));
        assert_eq!(chunk.egress_summary.as_deref(), Some("shop"));
        assert_eq!(chunk.memory_used.as_ref().map(Vec::len), Some(1));
        assert_eq!(serde_json::to_value(&chunk).ok(), Some(wire));

        // The terminal cancelled chunk: `done && cancelled`.
        let terminal: StreamChunkPayload = decode(serde_json::json!({
            "requestId": "req-1",
            "content": "",
            "done": true,
            "cancelled": true
        }));
        assert!(terminal.done);
        assert_eq!(terminal.cancelled, Some(true));
        assert_eq!(terminal.usage, None);

        let error: StreamErrorPayload =
            decode(serde_json::json!({ "requestId": "req-1", "error": "boom" }));
        assert_eq!(error.error, "boom");
    }

    /// Injection events and memory candidates carry the run's chips.
    #[test]
    fn injection_and_candidates_events_round_trip() {
        let event: AiChatInjectionEvent = decode(serde_json::json!({
            "requestId": "req-1",
            "snapshot": {
                "templateId": "default",
                "templateName": "Default",
                "templateHash": "h1",
                "hasConnectionNotes": true,
                "hasGlobalInstructions": false
            }
        }));
        assert_eq!(event.request_id, "req-1");
        let snapshot = event.snapshot;
        assert_eq!(snapshot.template_id.as_deref(), Some("default"));
        assert!(snapshot.has_connection_notes);
        assert!(!snapshot.has_global_instructions);

        let candidates: AiMemoryCandidatesEvent = decode(serde_json::json!({
            "requestId": "req-1",
            "candidates": [{
                "reference": "M1", "id": "mem-1", "subject": "s",
                "kind": "glossary", "scope": "user", "score": 0.9
            }]
        }));
        assert_eq!(candidates.candidates.len(), 1);
        assert_eq!(candidates.candidates[0].reference, "M1");
    }

    /// Loaded skills round-trip the partial shape the loader produces.
    #[test]
    fn loaded_skills_round_trip() {
        let wire = serde_json::json!({ "name": "sql helper", "loadedAt": "2026-01-01T00:00:00Z" });
        let skill: AiLoadedSkill = decode(wire.clone());
        assert_eq!(skill.name, "sql helper");
        assert_eq!(skill.id, None);
        assert_eq!(skill.source, None);
        assert_eq!(serde_json::to_value(&skill).ok(), Some(wire));
    }

    /// Tool results and question options keep their camelCase keys.
    #[test]
    fn tool_results_round_trip() {
        let result: AiToolResult =
            decode(serde_json::json!({ "toolCallId": "call-1", "content": "ok" }));
        assert_eq!(result.tool_call_id, "call-1");
        assert_eq!(
            serde_json::to_value(&result).ok(),
            Some(serde_json::json!({ "toolCallId": "call-1", "content": "ok" }))
        );
    }
}
