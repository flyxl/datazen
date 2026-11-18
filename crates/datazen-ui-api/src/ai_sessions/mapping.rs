//! Pure mappers between the live chat session and its persisted/backend
//! shapes — all of sessions.ts's `chatSession*`/`message*`/`titleFor`/
//! export helpers, none of which perform IO.
//!
//! Every function mirrors its React counterpart one-for-one; the seams
//! React reaches for implicitly (`new Date().toISOString()`,
//! `crypto.randomUUID()`) are explicit parameters (`now_iso`, `new_id`) so
//! hosts and tests drive them deterministically. The
//! [`AiSessionStore`](super::store::AiSessionStore) methods pass their
//! injected clock through to these.

use serde_json::{Map as JsonMap, Value};

use super::local::StoredSession;
use super::model::{
    AiChatMessage, AiChatSession, AiMessageRole, AiMessageStatus, AiSessionDetail,
    AiSessionMessage, AiSessionMessageInput, AiSessionUpsertInput, ExportFormat, SessionScope,
    compute_session_key, normalize_loaded_skills,
};

/// React's private `messageStatus(message)`:
/// 1. a persisted status wins;
/// 2. the cancellation marker reads as `cancelled`;
/// 3. an assistant reply that *starts* with `Error:` reads as `failed`;
/// 4. everything else is `complete`.
pub fn message_status(message: &AiChatMessage) -> AiMessageStatus {
    if let Some(status) = message.status {
        return status;
    }
    if message.cancelled.unwrap_or(false) {
        return AiMessageStatus::Cancelled;
    }
    if message.role == AiMessageRole::Assistant && message.content.starts_with("Error:") {
        return AiMessageStatus::Failed;
    }
    AiMessageStatus::Complete
}

/// React's private `messageMetadata(message)`: the metadata object carried
/// into the AppDb — every pass-through field that is present (`!== undefined`
/// in React, `Some` here — including explicit nulls) lands in it; `None`
/// when nothing is set.
pub fn message_metadata(message: &AiChatMessage) -> Option<Value> {
    let mut metadata = JsonMap::new();
    if let Some(reasoning) = &message.reasoning {
        metadata.insert("reasoning".to_string(), reasoning.clone());
    }
    if let Some(questions) = &message.questions {
        metadata.insert("questions".to_string(), questions.clone());
    }
    if let Some(tool_calls) = &message.tool_calls {
        metadata.insert("toolCalls".to_string(), tool_calls.clone());
    }
    if let Some(tool_call_id) = &message.tool_call_id {
        metadata.insert("toolCallId".to_string(), tool_call_id.clone());
    }
    if let Some(cancelled) = message.cancelled {
        metadata.insert("cancelled".to_string(), Value::Bool(cancelled));
    }
    if let Some(memory_used) = &message.memory_used {
        metadata.insert("memoryUsed".to_string(), memory_used.clone());
    }
    if metadata.is_empty() {
        None
    } else {
        Some(Value::Object(metadata))
    }
}

/// React's private `titleFor(session)`: the trimmed title when non-blank,
/// else the first user message's content trimmed and cut to 80 characters,
/// else the empty string.
pub fn title_for(session: &AiChatSession) -> String {
    if let Some(title) = session.title.as_deref() {
        let trimmed = title.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    session
        .messages
        .iter()
        .find(|message| message.role == AiMessageRole::User)
        .and_then(|message| Some(message.content.trim()))
        .map(|content| content.chars().take(80).collect())
        .unwrap_or_default()
}

/// React's private `messageFromStored(message)`: `{...message}` with `id`,
/// `status` and `createdAt` defaulted.
pub fn message_from_stored(
    message: &AiChatMessage,
    now_iso: &str,
    new_id: &mut dyn FnMut() -> String,
) -> AiChatMessage {
    let mut restored = message.clone();
    if restored.id.is_none() {
        restored.id = Some(new_id());
    }
    restored.status = Some(message_status(message));
    if restored.created_at.is_none() {
        restored.created_at = Some(now_iso.to_string());
    }
    restored
}

/// React `chatSessionFromDetail(detail)`: the live session a backend detail
/// restores into.
///
/// Per message, React spreads the row's metadata over the message first
/// (`...extra`) and then overrides the detail's own fields; the spread is
/// observable only through the pass-through keys, which land verbatim —
/// including `null` values (kept as `Some(Value::Null)`). `cancelled` is
/// always computed: the row's status, or a metadata `cancelled: true`.
pub fn chat_session_from_detail(detail: &AiSessionDetail) -> AiChatSession {
    AiChatSession {
        id: detail.id.clone(),
        session_key: detail.session_key.clone(),
        connection_id: detail.connection_id.clone(),
        database: detail.database.clone(),
        title: Some(detail.title.clone()),
        metadata: detail.metadata.clone(),
        messages: detail
            .messages
            .iter()
            .map(|message| {
                let extra = message.metadata.as_ref().and_then(Value::as_object);
                let pass_through = |key: &str| extra.and_then(|fields| fields.get(key)).cloned();
                AiChatMessage {
                    reasoning: pass_through("reasoning"),
                    questions: pass_through("questions"),
                    tool_calls: pass_through("toolCalls"),
                    tool_call_id: pass_through("toolCallId"),
                    memory_used: pass_through("memoryUsed"),
                    id: Some(message.id.clone()),
                    role: message.role,
                    content: message.content.clone(),
                    status: Some(message.status),
                    created_at: Some(message.created_at.clone()),
                    cancelled: Some(
                        message.status == AiMessageStatus::Cancelled
                            || extra.and_then(|fields| fields.get("cancelled")) == Some(&Value::Bool(true)),
                    ),
                }
            })
            .collect(),
        is_streaming: false,
        stream_content: String::new(),
        stream_reasoning: String::new(),
        stream_mcp_tool_name: None,
        egress_summary: None,
        stream_memory_used: None,
        memory_used: None,
        loaded_skills: normalize_loaded_skills(
            detail.metadata.as_ref().and_then(|metadata| metadata.get("loadedSkills")),
            // React reads now only when an entry lacks loadedAt; the caller
            // of this pure mapper has no clock, and a missing loadedAt in a
            // backend row is not a path sessions.ts can observe, so the
            // timestamp is the empty marker here. T042/T128 own the live
            // normalization with a real clock.
            "",
        ),
        request_id: None,
        attribution: None,
    }
}

/// React's private `chatSessionFromStored(key, stored, scope)`.
///
/// `stored.connectionId ?? scope.connectionId ?? null` (and database): the
/// stored value wins, the scope backfills. `title` copies verbatim (no
/// scope backfill — that is `titleFor`'s job at the next write).
pub fn chat_session_from_stored(
    key: &str,
    stored: &StoredSession,
    scope: &SessionScope,
    now_iso: &str,
    new_id: &mut dyn FnMut() -> String,
) -> AiChatSession {
    let loaded_skills = normalize_loaded_skills(
        stored.metadata.as_ref().and_then(|metadata| metadata.get("loadedSkills")),
        now_iso,
    );
    AiChatSession {
        id: stored.id.clone().unwrap_or_else(|| new_id()),
        session_key: key.to_string(),
        connection_id: stored.connection_id.clone().or_else(|| scope.connection_id.clone()),
        database: stored.database.clone().or_else(|| scope.database.clone()),
        title: stored.title.clone(),
        metadata: stored.metadata.clone(),
        messages: stored
            .messages
            .iter()
            .map(|message| message_from_stored(message, now_iso, new_id))
            .collect(),
        is_streaming: false,
        stream_content: String::new(),
        stream_reasoning: String::new(),
        stream_mcp_tool_name: None,
        egress_summary: None,
        stream_memory_used: None,
        memory_used: None,
        loaded_skills,
        request_id: None,
        attribution: None,
    }
}

/// React `chatSessionToUpsertInput(session)`: the explicit replace payload
/// (`append: false` — always `Some(false)` here, never the omitted key).
pub fn chat_session_to_upsert_input(session: &AiChatSession, now_iso: &str) -> AiSessionUpsertInput {
    AiSessionUpsertInput {
        id: Some(session.id.clone()),
        session_key: session.session_key.clone(),
        connection_id: session.connection_id.clone(),
        database: session.database.clone(),
        title: Some(title_for(session)),
        metadata: session.metadata.clone(),
        last_access_at: Some(now_iso.to_string()),
        append: Some(false),
        messages: session
            .messages
            .iter()
            .enumerate()
            .map(|(index, message)| AiSessionMessageInput {
                id: Some(
                    message
                        .id
                        .clone()
                        .unwrap_or_else(|| format!("{}:m{index}", session.id)),
                ),
                role: message.role,
                content: message.content.clone(),
                status: Some(message_status(message)),
                created_at: Some(message.created_at.clone().unwrap_or(now_iso.to_string())),
                metadata: message_metadata(message),
            })
            .collect(),
    }
}

/// React `newChatSession(scope, key?)`: a fresh unsaved session with
/// `metadata.startedAt` stamped by the injected now.
pub fn new_chat_session(
    scope: &SessionScope,
    key: Option<&str>,
    now_iso: &str,
    new_id: &mut dyn FnMut() -> String,
) -> AiChatSession {
    AiChatSession {
        id: new_id(),
        session_key: key
            .map(str::to_string)
            .unwrap_or_else(|| {
                compute_session_key(
                    scope.connection_id.as_deref(),
                    scope.db_session_id.as_deref(),
                    scope.database.as_deref(),
                )
            }),
        connection_id: scope.connection_id.clone(),
        database: scope.database.clone(),
        title: None,
        metadata: {
            let mut metadata = JsonMap::new();
            metadata.insert("startedAt".to_string(), Value::String(now_iso.to_string()));
            Some(Value::Object(metadata))
        },
        messages: Vec::new(),
        is_streaming: false,
        stream_content: String::new(),
        stream_reasoning: String::new(),
        stream_mcp_tool_name: None,
        egress_summary: None,
        stream_memory_used: None,
        memory_used: None,
        loaded_skills: Vec::new(),
        request_id: None,
        attribution: None,
    }
}

/// React's private `localDetail(session)`: the detail shape the offline
/// export path serializes.
pub fn local_detail(session: &AiChatSession, now_iso: &str) -> AiSessionDetail {
    AiSessionDetail {
        id: session.id.clone(),
        session_key: session.session_key.clone(),
        connection_id: session.connection_id.clone(),
        database: session.database.clone(),
        title: title_for(session),
        metadata: session.metadata.clone(),
        message_count: session.messages.len(),
        preview: session
            .messages
            .iter()
            .find(|message| message.role == AiMessageRole::User)
            .map(|message| message.content.chars().take(120).collect())
            .unwrap_or_default(),
        created_at: session
            .messages
            .first()
            .and_then(|message| message.created_at.clone())
            .unwrap_or_else(|| now_iso.to_string()),
        updated_at: now_iso.to_string(),
        last_access_at: now_iso.to_string(),
        messages: session
            .messages
            .iter()
            .enumerate()
            .map(|(index, message)| AiSessionMessage {
                id: message
                    .id
                    .clone()
                    .unwrap_or_else(|| format!("{}:m{index}", session.id)),
                role: message.role,
                content: message.content.clone(),
                status: message_status(message),
                metadata: message_metadata(message),
                created_at: message.created_at.clone().unwrap_or_else(|| now_iso.to_string()),
            })
            .collect(),
    }
}

/// React's private `markdownFor(detail)`: the offline Markdown export body.
///
/// Message roles capitalize their first letter; only non-`complete`
/// statuses annotate the heading with ` (status)`; connection/database
/// lines appear only when non-empty.
pub fn markdown_for(detail: &AiSessionDetail) -> String {
    let mut lines: Vec<String> = Vec::new();
    lines.push(format!(
        "# {}",
        if detail.title.is_empty() { "AI conversation" } else { detail.title.as_str() }
    ));
    lines.push(String::new());
    lines.push(format!("- Session: `{}`", detail.session_key));
    if let Some(connection) = detail.connection_id.as_deref().filter(|id| !id.is_empty()) {
        lines.push(format!("- Connection: `{connection}`"));
    }
    if let Some(database) = detail.database.as_deref().filter(|db| !db.is_empty()) {
        lines.push(format!("- Database: `{database}`"));
    }
    lines.push(format!("- Created: {}", detail.created_at));
    lines.push(format!("- Updated: {}", detail.updated_at));
    lines.push(String::new());
    for message in &detail.messages {
        let role: String = match message.role {
            AiMessageRole::System => "System".to_string(),
            AiMessageRole::User => "User".to_string(),
            AiMessageRole::Assistant => "Assistant".to_string(),
            AiMessageRole::Tool => "Tool".to_string(),
        };
        let suffix = if message.status == AiMessageStatus::Complete {
            String::new()
        } else {
            format!(" ({})", message.status.as_str())
        };
        lines.push(format!("## {role}{suffix}"));
        lines.push(String::new());
        lines.push(message.content.clone());
        lines.push(String::new());
    }
    lines.join("\n")
}

/// React's private `exportFilename(detail, format)`:
/// `(detail.title || detail.id).trim()` → disallowed characters collapsed to
/// single `-` runs → leading/trailing `-` runs stripped → first 60 characters
/// → `ai-session` when everything vanished → `.<json|md>`.
///
/// Allowed characters mirror the regex `[^\w\u4e00-\u9fff.-]`: ASCII
/// letters, digits, underscore, CJK Unified Ideographs, `.` and `-`.
/// Character slicing counts chars (JS counts UTF-16 units; a difference
/// only appears for astral-plane titles, none of which the UI produces).
pub fn export_filename(detail: &AiSessionDetail, format: ExportFormat) -> String {
    let raw = if detail.title.is_empty() { &detail.id } else { &detail.title };
    let mut cleaned = String::new();
    let mut in_disallowed_run = false;
    for character in raw.trim().chars() {
        let allowed = character.is_ascii_alphanumeric()
            || character == '_'
            || character == '.'
            || character == '-'
            || ('\u{4e00}'..='\u{9fff}').contains(&character);
        if allowed {
            cleaned.push(character);
            in_disallowed_run = false;
        } else if !in_disallowed_run {
            cleaned.push('-');
            in_disallowed_run = true;
        }
    }
    let base: String = cleaned.trim_matches('-').chars().take(60).collect();
    let base = if base.is_empty() { "ai-session" } else { base.as_str() };
    format!("{base}.{}", format.extension())
}

#[cfg(test)]
mod tests {
    use super::super::model::{AiMessageStatus, SessionScope};
    use super::super::local::{StoredSession};
    use super::*;

    /// The React suite's `detail` fixture (sessions.backend.test.ts).
    fn fixture_detail() -> AiSessionDetail {
        AiSessionDetail {
            id: "s1".to_string(),
            session_key: "conn-1::db".to_string(),
            connection_id: Some("conn-1".to_string()),
            database: Some("db".to_string()),
            title: "Saved conversation".to_string(),
            metadata: Some(serde_json::json!({ "templateId": "default" })),
            message_count: 3,
            preview: "hello".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:01Z".to_string(),
            last_access_at: "2026-01-01T00:00:01Z".to_string(),
            messages: vec![
                AiSessionMessage {
                    id: "m1".to_string(),
                    role: AiMessageRole::User,
                    content: "hello".to_string(),
                    status: AiMessageStatus::Complete,
                    metadata: Some(serde_json::json!({ "memoryUsed": [{ "id": "mem-1" }] })),
                    created_at: "2026-01-01T00:00:00Z".to_string(),
                },
                AiSessionMessage {
                    id: "m2".to_string(),
                    role: AiMessageRole::Assistant,
                    content: "stopped".to_string(),
                    status: AiMessageStatus::Cancelled,
                    metadata: Some(serde_json::json!({ "reasoning": "thinking", "cancelled": true })),
                    created_at: "2026-01-01T00:00:01Z".to_string(),
                },
                AiSessionMessage {
                    id: "m3".to_string(),
                    role: AiMessageRole::Assistant,
                    content: "Error: boom".to_string(),
                    status: AiMessageStatus::Failed,
                    metadata: None,
                    created_at: "2026-01-01T00:00:02Z".to_string(),
                },
            ],
        }
    }

    fn counter_id() -> impl FnMut() -> String {
        let counter = std::cell::Cell::new(0u32);
        move || {
            let next = counter.get() + 1;
            counter.set(next);
            format!("gen-{next}")
        }
    }

    #[test]
    fn message_status_prefers_the_persisted_status() {
        let mut message = AiChatMessage::new(AiMessageRole::User, "done");
        message.status = Some(AiMessageStatus::Failed);
        assert_eq!(message_status(&message), AiMessageStatus::Failed);
        // Even when the cancellation marker disagrees.
        message.cancelled = Some(true);
        assert_eq!(message_status(&message), AiMessageStatus::Failed);
    }

    #[test]
    fn message_status_derives_cancelled_failed_and_complete() {
        let mut cancelled = AiChatMessage::new(AiMessageRole::User, "stopped");
        cancelled.cancelled = Some(true);
        assert_eq!(message_status(&cancelled), AiMessageStatus::Cancelled);

        let failed = AiChatMessage::new(AiMessageRole::Assistant, "Error: boom");
        assert_eq!(message_status(&failed), AiMessageStatus::Failed);
        // Only assistant messages with the Error: prefix; a user echo is
        // just complete.
        let user_echo = AiChatMessage::new(AiMessageRole::User, "Error: boom");
        assert_eq!(message_status(&user_echo), AiMessageStatus::Complete);
        // A non-prefixed assistant message is complete.
        let plain = AiChatMessage::new(AiMessageRole::Assistant, "all good");
        assert_eq!(message_status(&plain), AiMessageStatus::Complete);
        // The prefix must be at position zero.
        let mid = AiChatMessage::new(AiMessageRole::Assistant, "x Error: y");
        assert_eq!(message_status(&mid), AiMessageStatus::Complete);
    }

    #[test]
    fn message_metadata_carries_every_present_pass_through_field() {
        let mut message = AiChatMessage::new(AiMessageRole::Assistant, "reply");
        message.reasoning = Some(Value::String("thinking".to_string()));
        message.tool_calls = Some(serde_json::json!([{ "name": "t" }]));
        message.tool_call_id = Some(Value::String("call-1".to_string()));
        message.cancelled = Some(false);
        message.memory_used = Some(serde_json::json!([{ "id": "mem-1" }]));
        let metadata = message_metadata(&message);
        let expected = serde_json::json!({
            "reasoning": "thinking",
            "toolCalls": [{ "name": "t" }],
            "toolCallId": "call-1",
            "cancelled": false,
            "memoryUsed": [{ "id": "mem-1" }]
        });
        assert_eq!(metadata, Some(expected));
        // A bare message yields None (React's undefined check).
        assert_eq!(message_metadata(&AiChatMessage::new(AiMessageRole::User, "hi")), None);
    }

    #[test]
    fn message_metadata_keeps_explicit_nulls() {
        // React: `!== undefined` — a null reasoning lands in the metadata.
        let mut message = AiChatMessage::new(AiMessageRole::Assistant, "r");
        message.reasoning = Some(Value::Null);
        assert_eq!(
            message_metadata(&message),
            Some(serde_json::json!({ "reasoning": null }))
        );
    }

    #[test]
    fn title_for_uses_the_trimmed_title_first() {
        let mut session = chat_session_from_detail(&fixture_detail());
        session.title = Some("  Trimmed title  ".to_string());
        assert_eq!(title_for(&session), "Trimmed title");
    }

    #[test]
    fn title_for_falls_back_to_the_first_user_message_capped_at_80() {
        let mut session = chat_session_from_detail(&fixture_detail());
        session.title = Some("   ".to_string());
        session.messages[0].content = "x".repeat(120);
        let title = title_for(&session);
        assert_eq!(title.len(), 80);
        assert_eq!(title, "x".repeat(80));
        // Trimmed before slicing: surrounding blanks never enter the title.
        session.messages[0].content = format!("  {}", "y".repeat(85));
        assert_eq!(title_for(&session).chars().count(), 80);
        // No user message at all → empty.
        session.messages.clear();
        assert_eq!(title_for(&session), "");
    }

    #[test]
    fn chat_session_from_detail_restores_statuses_and_metadata() {
        // React suite 'restores cancelled/failed messages and memory
        // metadata from AppDb' (the mapping half).
        let session = chat_session_from_detail(&fixture_detail());
        assert_eq!(session.id, "s1");
        assert_eq!(session.session_key, "conn-1::db");
        assert_eq!(session.connection_id.as_deref(), Some("conn-1"));
        assert_eq!(session.database.as_deref(), Some("db"));
        assert_eq!(session.title.as_deref(), Some("Saved conversation"));
        assert_eq!(session.metadata, Some(serde_json::json!({ "templateId": "default" })));
        assert_eq!(session.messages.len(), 3);
        assert_eq!(session.messages[0].memory_used, Some(serde_json::json!([{ "id": "mem-1" }])));
        assert_eq!(session.messages[1].reasoning, Some(Value::String("thinking".to_string())));
        assert_eq!(session.messages[1].status, Some(AiMessageStatus::Cancelled));
        assert!(session.messages[1].cancelled.unwrap_or(false));
        assert_eq!(session.messages[2].status, Some(AiMessageStatus::Failed));
        // No metadata spread for the metadata-less row: all pass-through
        // fields stay absent.
        assert_eq!(session.messages[2].reasoning, None);
        assert_eq!(session.messages[2].memory_used, None);
        assert_eq!(session.messages[2].tool_call_id, None);
        assert_eq!(session.is_streaming, false);
        assert!(session.loaded_skills.is_empty());
        assert_eq!(session.request_id, None);
    }

    #[test]
    fn chat_session_from_detail_marks_cancelled_from_metadata_only_when_true() {
        // `extra.cancelled === true` is a strict boolean check: 1 or "true"
        // do not count (and a `cancelled` status does).
        let mut detail = fixture_detail();
        detail.messages[0].metadata = Some(serde_json::json!({ "cancelled": 1 }));
        detail.messages[0].status = AiMessageStatus::Complete;
        let session = chat_session_from_detail(&detail);
        assert!(!session.messages[0].cancelled.unwrap_or(false));

        detail.messages[0].metadata = Some(serde_json::json!({ "cancelled": true }));
        let session = chat_session_from_detail(&detail);
        assert!(session.messages[0].cancelled.unwrap_or(false));
    }

    #[test]
    fn chat_session_from_detail_reads_loaded_skills_from_metadata() {
        let mut detail = fixture_detail();
        detail.metadata = Some(serde_json::json!({
            "loadedSkills": [{ "id": "sk-1", "name": "sql helper" }]
        }));
        let session = chat_session_from_detail(&detail);
        assert_eq!(session.loaded_skills.len(), 1);
        assert_eq!(session.loaded_skills[0].name, "sql helper");
    }

    #[test]
    fn chat_session_to_upsert_input_maps_the_replace_payload() {
        // React suite 'maps the full chat session to an explicit replace
        // payload'.
        let session = chat_session_from_detail(&fixture_detail());
        let input = chat_session_to_upsert_input(&session, "2026-02-02T00:00:00.000Z");
        assert_eq!(input.id.as_deref(), Some("s1"));
        assert_eq!(input.session_key, "conn-1::db");
        assert_eq!(input.connection_id.as_deref(), Some("conn-1"));
        assert_eq!(input.database.as_deref(), Some("db"));
        // titleFor keeps the session's own non-blank title.
        assert_eq!(input.title.as_deref(), Some("Saved conversation"));
        assert_eq!(input.last_access_at.as_deref(), Some("2026-02-02T00:00:00.000Z"));
        assert_eq!(input.append, Some(false));
        assert_eq!(input.messages.len(), 3);
        assert_eq!(input.messages[0].id.as_deref(), Some("m1"));
        // m1's cancelled is explicitly false after from-detail (the
        // always-computed field), and messageMetadata's `!== undefined`
        // check carries it — exactly React's payload.
        assert_eq!(
            input.messages[0].metadata,
            Some(serde_json::json!({ "memoryUsed": [{ "id": "mem-1" }], "cancelled": false }))
        );
        assert_eq!(input.messages[1].role, AiMessageRole::Assistant);
        assert_eq!(input.messages[1].status, Some(AiMessageStatus::Cancelled));
        assert_eq!(input.messages[2].status, Some(AiMessageStatus::Failed));
        // Message ids and createdAt fall back to deterministic derivations.
        let mut bare = chat_session_from_detail(&fixture_detail());
        bare.id = "sess".to_string();
        bare.messages = vec![AiChatMessage::new(AiMessageRole::User, "q")];
        let input = chat_session_to_upsert_input(&bare, "NOW");
        assert_eq!(input.messages[0].id.as_deref(), Some("sess:m0"));
        assert_eq!(input.messages[0].created_at.as_deref(), Some("NOW"));
        assert_eq!(input.messages[0].status, Some(AiMessageStatus::Complete));
        assert_eq!(input.messages[0].metadata, None);
    }

    #[test]
    fn chat_session_from_stored_backfills_the_scope_only_where_stored_is_silent() {
        let stored = StoredSession {
            id: Some("local-1".to_string()),
            title: Some("Local draft".to_string()),
            metadata: Some(serde_json::json!({ "startedAt": "2026-01-01T00:00:00Z" })),
            connection_id: None,
            database: Some("stored-db".to_string()),
            messages: vec![
                AiChatMessage::new(AiMessageRole::User, "one"),
                {
                    let mut message = AiChatMessage::new(AiMessageRole::Assistant, "stopped");
                    message.cancelled = Some(true);
                    message
                },
            ],
            last_access: Some(5.0),
        };
        let scope = SessionScope::new(Some("conn-1".to_string()), None, Some("db".to_string()));
        let mut ids = counter_id();
        let session = chat_session_from_stored("conn-1::db", &stored, &scope, "NOW", &mut ids);
        assert_eq!(session.id, "local-1");
        assert_eq!(session.session_key, "conn-1::db");
        // stored.connectionId is absent → scope backfills; stored.database
        // wins over the scope.
        assert_eq!(session.connection_id.as_deref(), Some("conn-1"));
        assert_eq!(session.database.as_deref(), Some("stored-db"));
        assert_eq!(session.title.as_deref(), Some("Local draft"));
        assert_eq!(session.metadata, Some(serde_json::json!({ "startedAt": "2026-01-01T00:00:00Z" })));
        assert_eq!(session.messages[1].status, Some(AiMessageStatus::Cancelled));
        // A stored entry without an id mints one at the seam.
        let mut ids = counter_id();
        let anonymous = StoredSession::new(vec![AiChatMessage::new(AiMessageRole::User, "hi")], 1.0);
        let session = chat_session_from_stored("k", &anonymous, &SessionScope::default(), "NOW", &mut ids);
        assert_eq!(session.id, "gen-1");
        assert_eq!(session.messages[0].id.as_deref(), Some("gen-2"));
        assert_eq!(session.messages[0].created_at.as_deref(), Some("NOW"));
    }

    #[test]
    fn new_chat_session_builds_a_fresh_session_for_the_scope() {
        let scope = SessionScope::new(Some("conn-1".to_string()), None, Some("db".to_string()));
        let mut ids = counter_id();
        let session = new_chat_session(&scope, None, "2026-01-01T00:00:00.000Z", &mut ids);
        assert_eq!(session.id, "gen-1");
        assert_eq!(session.session_key, "conn-1::db");
        assert_eq!(session.connection_id.as_deref(), Some("conn-1"));
        assert_eq!(session.database.as_deref(), Some("db"));
        assert_eq!(session.metadata, Some(serde_json::json!({ "startedAt": "2026-01-01T00:00:00.000Z" })));
        assert!(session.messages.is_empty());
        assert!(session.loaded_skills.is_empty());
        // An explicit key overrides the derived one.
        let mut ids = counter_id();
        let session = new_chat_session(&scope, Some("custom::key"), "NOW", &mut ids);
        assert_eq!(session.session_key, "custom::key");
    }

    #[test]
    fn local_detail_projects_the_session_into_the_detail_shape() {
        let session = chat_session_from_detail(&fixture_detail());
        let detail = local_detail(&session, "2026-02-02T00:00:00.000Z");
        assert_eq!(detail.id, "s1");
        assert_eq!(detail.session_key, "conn-1::db");
        // titleFor keeps the session's own non-blank title.
        assert_eq!(detail.title, "Saved conversation");
        assert_eq!(detail.message_count, 3);
        assert_eq!(detail.preview, "hello");
        assert_eq!(detail.created_at, "2026-01-01T00:00:00Z");
        assert_eq!(detail.updated_at, "2026-02-02T00:00:00.000Z");
        assert_eq!(detail.last_access_at, "2026-02-02T00:00:00.000Z");
        assert_eq!(detail.messages.len(), 3);
        assert_eq!(detail.messages[1].status, AiMessageStatus::Cancelled);
        assert_eq!(detail.messages[1].metadata, Some(serde_json::json!({ "reasoning": "thinking", "cancelled": true })));
        // An empty session: preview empty, timestamps default to now, ids
        // derive deterministically.
        let scope = SessionScope::new(Some("conn-1".to_string()), None, None);
        let mut ids = counter_id();
        let empty = new_chat_session(&scope, None, "NOW", &mut ids);
        let detail = local_detail(&empty, "NOW2");
        assert_eq!(detail.preview, "");
        assert_eq!(detail.created_at, "NOW2");
        assert_eq!(detail.title, "");
        assert!(detail.messages.is_empty());
        // Preview takes the first user message's first 120 chars.
        let mut chatty = empty.clone();
        chatty.messages.push(AiChatMessage::new(AiMessageRole::Assistant, "first"));
        chatty.messages.push(AiChatMessage::new(AiMessageRole::User, "z".repeat(200)));
        let detail = local_detail(&chatty, "NOW");
        assert_eq!(detail.preview.chars().count(), 120);
    }

    #[test]
    fn markdown_for_renders_roles_and_status_annotations() {
        // React suite 'exports backend JSON/Markdown and falls back locally
        // offline' (the markdown half).
        let session = chat_session_from_detail(&fixture_detail());
        let detail = local_detail(&session, "2026-01-01T00:00:00Z");
        let markdown = markdown_for(&detail);
        assert!(markdown.starts_with("# Saved conversation\n\n- Session: `conn-1::db`\n- Connection: `conn-1`\n- Database: `db`\n- Created: 2026-01-01T00:00:00Z\n- Updated: 2026-01-01T00:00:00Z\n\n"));
        assert!(markdown.contains("## User\n\nhello\n"));
        assert!(markdown.contains("## Assistant (cancelled)\n\nstopped\n"));
        assert!(markdown.contains("## Assistant (failed)\n\nError: boom\n"));
        // An empty title falls back to the fixed heading; connection and
        // database lines disappear when unset.
        let mut bare = fixture_detail();
        bare.title = String::new();
        bare.connection_id = None;
        bare.database = None;
        bare.messages.clear();
        let markdown = markdown_for(&bare);
        assert!(markdown.starts_with("# AI conversation\n\n- Session: `conn-1::db`\n- Created:"));
        assert!(!markdown.contains("- Connection:"));
        assert!(!markdown.contains("- Database:"));
    }

    #[test]
    fn export_filename_sanitizes_and_defaults() {
        // React suite: 'Saved conversation' → 'Saved-conversation'.
        let detail = fixture_detail();
        assert_eq!(export_filename(&detail, ExportFormat::Json), "Saved-conversation.json");
        assert_eq!(export_filename(&detail, ExportFormat::Markdown), "Saved-conversation.md");
        // Disallowed runs collapse to a single dash; leading/trailing runs
        // are stripped; CJK, dots and dashes survive.
        let mut detail = fixture_detail();
        detail.title = "  My..  Session!! 数据库  ".to_string();
        assert_eq!(export_filename(&detail, ExportFormat::Json), "My..-Session-数据库.json");
        detail.title = "---===---".to_string();
        assert_eq!(export_filename(&detail, ExportFormat::Json), "ai-session.json");
        // Empty title falls back to the id.
        detail.title = String::new();
        detail.id = "row/42".to_string();
        assert_eq!(export_filename(&detail, ExportFormat::Json), "row-42.json");
        // 60-character cap.
        detail.title = "a".repeat(80);
        assert_eq!(export_filename(&detail, ExportFormat::Json), format!("{}.json", "a".repeat(60)));
    }
}
