//! The stream lifecycle pins for [`AiStreaming`]: the chunk routing and
//! gating, the terminal assistant messages, the out-of-band error arms,
//! the 8s cancel-timeout finaliser, and the persist guard/report — one pin
//! per operation branch of React `stores/ai/streaming.ts`. Every fixture is
//! driven through the injected seams' doubles (clock, ids, registry stub,
//! store stub — all programmable); the pure wiring arms (the routing map,
//! the registry accessor, the attribution forward, cancelChat) live in
//! `wiring_tests`; see `test_support` for the harness.

use crate::ai_sessions::SessionPersistResult;
use crate::ai_types::{AiChatMessage, AiChatSession, AiMessageRole, AiMessageStatus};

use super::seams::{AiStoreView, Nl2sqlStreamingSlice};
use super::test_support::{
    GatePhase, SharedStore, TestEnv, chunk, env, error_payload, fixture_memory, fixture_session,
    fixture_tool_call, NOW_ISO,
};

fn visible_chat(test: &TestEnv) -> AiChatSession {
    match test.store.chat() {
        Some(session) => session,
        None => panic!("expected a visible chat session"),
    }
}

fn visible_workflow(test: &TestEnv) -> AiChatSession {
    match test.store.workflow() {
        Some(session) => session,
        None => panic!("expected a visible workflow chat session"),
    }
}

fn chat_message(test: &TestEnv, index: usize) -> AiChatMessage {
    match visible_chat(test).messages.get(index) {
        Some(message) => message.clone(),
        None => panic!("expected message {index} on the visible chat session"),
    }
}

fn first_question_id(test: &TestEnv) -> Option<String> {
    chat_message(&test, 0)
        .questions
        .as_ref()
        .and_then(|questions| questions.first())
        .map(|question| question.id.clone())
}

// ── handleStreamChunk: the NL2SQL branch ──────────────────────────────────

#[test]
fn nl2sql_chunks_accumulate_sql_and_preview_through_the_seams() {
    let mut test = env();
    test.store.seed_nl2sql(Nl2sqlStreamingSlice {
        request_id: Some("r-nl".to_owned()),
        streaming_sql: "SELE".to_owned(),
        streaming_preview: String::new(),
        generated_sql: String::new(),
        memory_used: vec![fixture_memory("M1")],
        is_generating: true,
    });
    let mut payload = chunk("r-nl", "CT 1");
    payload.reasoning = Some("ignored on this branch".to_owned());
    test.streaming
        .handle_stream_chunk(&mut SharedStore(test.store.clone()), &payload);
    let nl2sql = test.store.nl2sql_state();
    assert_eq!(nl2sql.streaming_sql, "SELECT 1");
    // The preview came from the streaming extraction seam on the
    // accumulated text.
    assert_eq!(nl2sql.streaming_preview, "preview:SELECT 1");
    // The payload carried no memories: the slice's own stay.
    assert_eq!(nl2sql.memory_used, vec![fixture_memory("M1")]);
    // Mid-stream: the generation flag and the generated sql are untouched.
    assert!(nl2sql.is_generating);
    assert_eq!(nl2sql.generated_sql, "");
    // And an empty-content chunk leaves the accumulation alone (React's
    // `payload.content ? a + c : a`).
    test.streaming
        .handle_stream_chunk(&mut SharedStore(test.store.clone()), &chunk("r-nl", ""));
    assert_eq!(test.store.nl2sql_state().streaming_sql, "SELECT 1");
    assert_eq!(test.store.sets().last().copied(), Some("nl2sql"));
}

#[test]
fn nl2sql_done_finalizes_the_sql_and_unregisters_the_route() {
    let mut test = env();
    test.store.seed_nl2sql(Nl2sqlStreamingSlice {
        request_id: Some("r-nl".to_owned()),
        streaming_sql: "SELECT 1".to_owned(),
        streaming_preview: "preview:SELECT 1".to_owned(),
        generated_sql: String::new(),
        memory_used: Vec::new(),
        is_generating: true,
    });
    test.streaming.register_request_id("r-nl", "nl2sql");
    let mut payload = chunk("r-nl", " FROM t");
    payload.done = true;
    payload.memory_used = Some(vec![fixture_memory("M9")]);
    test.streaming
        .handle_stream_chunk(&mut SharedStore(test.store.clone()), &payload);
    let nl2sql = test.store.nl2sql_state();
    assert_eq!(nl2sql.streaming_sql, "SELECT 1 FROM t");
    // done: the final extraction ran on the accumulated text.
    assert_eq!(nl2sql.generated_sql, "final:SELECT 1 FROM t");
    assert!(!nl2sql.is_generating);
    // A non-empty payload memory replaces the slice's.
    assert_eq!(nl2sql.memory_used, vec![fixture_memory("M9")]);
    // The route went away and the error slot was never touched.
    assert_eq!(test.streaming.unregister_request_id("r-nl"), None);
    assert_eq!(test.store.nl2sql_error(), None);
    // The chat/workflow sessions were never consulted.
    assert!(test
        .store
        .sets()
        .iter()
        .all(|set| *set != "chat_session" && *set != "workflow_chat"));
}

#[test]
fn a_chunk_for_an_unknown_request_touches_nothing() {
    let mut test = env();
    test.store.seed_chat(fixture_session("s1", "r-chat"));
    test.streaming
        .handle_stream_chunk(&mut SharedStore(test.store.clone()), &chunk("r-elsewhere", "hi"));
    // Neither session matched, so no set ran at all and the registry was
    // never consulted.
    assert!(test.store.sets().is_empty());
    assert!(test.registry.chunk_calls().is_empty());
    // The error arm's routing is the same shape.
    test.streaming.handle_stream_error(
        &mut SharedStore(test.store.clone()),
        &error_payload("r-elsewhere", "boom"),
    );
    assert!(test.store.sets().is_empty());
    assert!(test.registry.error_calls().is_empty());
}

// ── handleStreamChunk: the chat / workflow branch ────────────────────────

#[test]
fn chunks_route_to_the_chat_session_and_accumulate_the_stream() {
    let mut test = env();
    let mut session = fixture_session("s1", "r-chat");
    session.stream_content = "Hel".to_owned();
    session.stream_reasoning = "think".to_owned();
    test.store.seed_chat(session);
    test.registry.begin("r-chat");
    let mut payload = chunk("r-chat", "lo");
    payload.reasoning = Some(" more".to_owned());
    payload.tool_calls =
        Some(vec![fixture_tool_call("mcp/server/tool"), fixture_tool_call("other")]);
    payload.egress_summary = Some("  scope  ".to_owned());
    payload.memory_used = Some(vec![fixture_memory("M2")]);
    test.streaming
        .handle_stream_chunk(&mut SharedStore(test.store.clone()), &payload);
    let session = visible_chat(&test);
    assert_eq!(session.stream_content, "Hello");
    assert_eq!(session.stream_reasoning, "think more");
    // The first mcp/-prefixed call wins (findMcpToolName).
    assert_eq!(session.stream_mcp_tool_name.as_deref(), Some("mcp/server/tool"));
    // The first chunk's trimmed egress landed.
    assert_eq!(session.egress_summary.as_deref(), Some("scope"));
    // The payload's memory list becomes both the in-flight and the
    // latest-run records.
    assert_eq!(session.stream_memory_used, Some(vec![fixture_memory("M2")]));
    assert_eq!(session.memory_used, Some(vec![fixture_memory("M2")]));
    // Mid-stream: still streaming, the messages and request id untouched.
    assert!(session.is_streaming);
    assert!(session.messages.is_empty());
    assert_eq!(session.request_id.as_deref(), Some("r-chat"));
    // The registry entry advanced starting → streaming.
    assert_eq!(test.registry.phase("r-chat"), Some(GatePhase::Streaming));
    // A later chunk without tool calls keeps the session's mcp tool name.
    test.streaming
        .handle_stream_chunk(&mut SharedStore(test.store.clone()), &chunk("r-chat", "!"));
    let session = visible_chat(&test);
    assert_eq!(session.stream_content, "Hello!");
    assert_eq!(session.stream_mcp_tool_name.as_deref(), Some("mcp/server/tool"));
}

#[test]
fn the_first_chunks_egress_sticks_against_later_chunks() {
    let mut test = env();
    let mut session = fixture_session("s1", "r-chat");
    session.egress_summary = Some("first".to_owned());
    test.store.seed_chat(session);
    test.registry.begin("r-chat");
    let mut payload = chunk("r-chat", "x");
    payload.egress_summary = Some(" later ".to_owned());
    test.streaming
        .handle_stream_chunk(&mut SharedStore(test.store.clone()), &payload);
    assert_eq!(visible_chat(&test).egress_summary.as_deref(), Some("first"));
    // A blank session-level summary lets a non-empty payload summary land…
    let mut session = fixture_session("s2", "r-blank");
    session.egress_summary = Some(String::new());
    test.store.seed_chat(session);
    test.registry.begin("r-blank");
    let mut payload = chunk("r-blank", "x");
    payload.egress_summary = Some("  fresh  ".to_owned());
    test.streaming
        .handle_stream_chunk(&mut SharedStore(test.store.clone()), &payload);
    assert_eq!(visible_chat(&test).egress_summary.as_deref(), Some("fresh"));
    // …but a whitespace-only payload summary reads back null.
    test.store.seed_chat(fixture_session("s3", "r-ws"));
    test.registry.begin("r-ws");
    let mut payload = chunk("r-ws", "x");
    payload.egress_summary = Some("   ".to_owned());
    test.streaming
        .handle_stream_chunk(&mut SharedStore(test.store.clone()), &payload);
    assert_eq!(visible_chat(&test).egress_summary, None);
}

#[test]
fn late_chunks_are_dropped_by_the_registry_gate() {
    let mut test = env();
    let mut session = fixture_session("s1", "r-chat");
    session.stream_content = "kept".to_owned();
    test.store.seed_chat(session);
    // The run carries a cancel mark: every non-terminal chunk is late.
    test.registry.begin("r-chat");
    test.registry.mark_cancelling("r-chat");
    test.streaming
        .handle_stream_chunk(&mut SharedStore(test.store.clone()), &chunk("r-chat", " late"));
    // Dropped before the store: the rendered text never moved.
    assert_eq!(visible_chat(&test).stream_content, "kept");
    assert!(test.store.sets().is_empty());
    // The gate DID consult the registry.
    assert_eq!(test.registry.chunk_calls().len(), 1);
    // A terminal duplicate is dropped the same way once the run settled.
    test.registry.force_terminal("r-chat");
    let mut payload = chunk("r-chat", "done");
    payload.done = true;
    test.streaming
        .handle_stream_chunk(&mut SharedStore(test.store.clone()), &payload);
    assert_eq!(visible_chat(&test).stream_content, "kept");
    assert!(visible_chat(&test).messages.is_empty());
    assert_eq!(test.registry.chunk_calls().len(), 2);
    // And an untracked request passes ungated (React only gates when
    // `registry.has(requestId)` holds).
    test.store.seed_chat(fixture_session("s2", "r-ghost"));
    test.streaming
        .handle_stream_chunk(&mut SharedStore(test.store.clone()), &chunk("r-ghost", " go"));
    assert_eq!(visible_chat(&test).stream_content, " go");
    assert_eq!(test.registry.chunk_calls().len(), 2);
}

#[test]
fn done_builds_the_assistant_message_and_persists_the_chat() {
    let mut test = env();
    let mut session = fixture_session("s1", "r-chat");
    session.stream_content = "Hel".to_owned();
    session.stream_reasoning = "think".to_owned();
    test.store.seed_chat(session);
    test.registry.begin("r-chat");
    test.streaming.register_request_id("r-chat", "chat::a");
    let mut payload = chunk("r-chat", "lo<raw>");
    payload.reasoning = Some(" more".to_owned());
    payload.done = true;
    payload.tool_calls = Some(vec![fixture_tool_call("ask_questions")]);
    payload.memory_used = Some(vec![fixture_memory("M3")]);
    test.streaming
        .handle_stream_chunk(&mut SharedStore(test.store.clone()), &payload);
    let session = visible_chat(&test);
    // The message: cleaned content (the questions seam stripped the raw
    // block), accumulated reasoning, tool calls, seam-minted id, seam
    // clock timestamp.
    assert_eq!(session.messages.len(), 1);
    let message = chat_message(&test, 0);
    assert_eq!(message.id.as_deref(), Some("gen-1"));
    assert_eq!(message.role, AiMessageRole::Assistant);
    assert_eq!(message.content, "Hello");
    assert_eq!(message.reasoning.as_deref(), Some("think more"));
    // No text questions: the tool-call questions filled in.
    assert_eq!(first_question_id(&test), Some("q-tool".to_owned()));
    assert_eq!(message.tool_calls.as_ref().map(|calls| calls.len()), Some(1));
    assert_eq!(message.cancelled, Some(false));
    assert_eq!(message.status, Some(AiMessageStatus::Complete));
    assert_eq!(message.created_at.as_deref(), Some(NOW_ISO));
    assert_eq!(message.memory_used, Some(vec![fixture_memory("M3")]));
    // The session: stream fields cleared, memory records the run, request
    // gone.
    assert!(!session.is_streaming);
    assert_eq!(session.stream_content, "");
    assert_eq!(session.stream_reasoning, "");
    assert_eq!(session.stream_mcp_tool_name, None);
    assert_eq!(session.stream_memory_used, None);
    assert_eq!(session.memory_used, Some(vec![fixture_memory("M3")]));
    assert_eq!(session.request_id, None);
    // The route is gone and the persist ran with the terminal session.
    assert_eq!(test.streaming.unregister_request_id("r-chat"), None);
    assert_eq!(test.persister.calls().len(), 1);
    assert_eq!(
        test.persister.calls().first().map(|persisted| persisted.id.clone()),
        Some("s1".to_owned())
    );
    // No migration error was reported.
    assert_eq!(test.store.session_migration_error(), None);
}

#[test]
fn a_cancelled_done_chunk_marks_the_message_cancelled() {
    let mut test = env();
    test.store.seed_chat(fixture_session("s1", "r-run"));
    test.registry.begin("r-run");
    let mut payload = chunk("r-run", "partial");
    payload.done = true;
    payload.cancelled = Some(true);
    test.streaming
        .handle_stream_chunk(&mut SharedStore(test.store.clone()), &payload);
    let message = chat_message(&test, 0);
    assert_eq!(message.cancelled, Some(true));
    assert_eq!(message.status, Some(AiMessageStatus::Cancelled));
    assert_eq!(message.content, "partial");
    // The registry settled through the gate.
    assert_eq!(test.registry.phase("r-run"), Some(GatePhase::Terminal));
}

#[test]
fn done_routes_to_the_workflow_session_without_persisting() {
    let mut test = env();
    let mut session = fixture_session("w1", "r-wf");
    session.stream_content = "answer".to_owned();
    test.store.seed_workflow(session);
    // A chat session exists but does not match: the workflow is the target.
    test.store.seed_chat(fixture_session("s1", "r-other"));
    test.registry.begin("r-wf");
    let mut payload = chunk("r-wf", "!");
    payload.done = true;
    test.streaming
        .handle_stream_chunk(&mut SharedStore(test.store.clone()), &payload);
    assert_eq!(
        visible_workflow(&test).messages.first().map(|message| message.content.clone()),
        Some("answer!".to_owned())
    );
    // The chat session never moved and nothing persisted.
    assert!(visible_chat(&test).messages.is_empty());
    assert!(test.persister.calls().is_empty());
    // Only the workflow slot was written.
    assert_eq!(test.store.sets(), vec!["workflow_chat"]);
}

#[test]
fn question_precedence_runs_xml_then_tool_calls() {
    let mut test = env();
    // Text questions win when present…
    test.store.seed_chat(fixture_session("s1", "r-xml"));
    test.registry.begin("r-xml");
    let mut payload = chunk("r-xml", "answer ASK now");
    payload.done = true;
    payload.tool_calls = Some(vec![fixture_tool_call("ask_questions")]);
    test.streaming
        .handle_stream_chunk(&mut SharedStore(test.store.clone()), &payload);
    assert_eq!(first_question_id(&test), Some("q-xml".to_owned()));
    // …without text questions the tool call's fill in…
    test.store.seed_chat(fixture_session("s2", "r-tool"));
    test.registry.begin("r-tool");
    let mut payload = chunk("r-tool", "answer");
    payload.done = true;
    payload.tool_calls = Some(vec![fixture_tool_call("ask_questions")]);
    test.streaming
        .handle_stream_chunk(&mut SharedStore(test.store.clone()), &payload);
    assert_eq!(first_question_id(&test), Some("q-tool".to_owned()));
    // …and neither source leaves the field absent.
    test.store.seed_chat(fixture_session("s3", "r-none"));
    test.registry.begin("r-none");
    let mut payload = chunk("r-none", "answer");
    payload.done = true;
    payload.tool_calls = Some(vec![fixture_tool_call("query_db")]);
    test.streaming
        .handle_stream_chunk(&mut SharedStore(test.store.clone()), &payload);
    let message = chat_message(&test, 0);
    assert_eq!(message.questions, None);
    // The non-questions tool call is still carried on the message.
    assert_eq!(message.tool_calls.as_ref().map(|calls| calls.len()), Some(1));
}

#[test]
fn an_empty_tool_call_list_reads_as_absent() {
    let mut test = env();
    test.store.seed_chat(fixture_session("s1", "r-e"));
    test.registry.begin("r-e");
    let mut payload = chunk("r-e", "x");
    payload.done = true;
    payload.tool_calls = Some(Vec::new());
    test.streaming
        .handle_stream_chunk(&mut SharedStore(test.store.clone()), &payload);
    // React: `payload.toolCalls && payload.toolCalls.length > 0 ? … : undefined`.
    assert_eq!(chat_message(&test, 0).tool_calls, None);
}

// ── handleStreamError ────────────────────────────────────────────────────

#[test]
fn stream_error_finalizes_nl2sql_and_falls_through_to_the_chat() {
    let mut test = env();
    test.store.seed_nl2sql(Nl2sqlStreamingSlice {
        request_id: Some("r-nl".to_owned()),
        streaming_sql: "SELECT".to_owned(),
        streaming_preview: String::new(),
        generated_sql: String::new(),
        memory_used: Vec::new(),
        is_generating: true,
    });
    // The same request id owns the visible chat session: React's error arm
    // does NOT return after the nl2sql write — the chat arm runs too.
    test.store.seed_chat(fixture_session("s1", "r-nl"));
    test.registry.begin("r-nl");
    test.streaming.register_request_id("r-nl", "chat::a");
    test.streaming.handle_stream_error(
        &mut SharedStore(test.store.clone()),
        &error_payload("r-nl", "boom"),
    );
    // The nl2sql slice stopped generating and carries the error.
    assert!(!test.store.nl2sql_state().is_generating);
    assert_eq!(test.store.nl2sql_error().as_deref(), Some("boom"));
    // …and the chat session got the failed marker too.
    let message = chat_message(&test, 0);
    assert_eq!(message.content, "Error: boom");
    assert_eq!(message.status, Some(AiMessageStatus::Failed));
    // The route went away exactly once (the first unregister).
    assert_eq!(test.streaming.unregister_request_id("r-nl"), None);
}

#[test]
fn stream_error_appends_a_failed_assistant_message_and_persists() {
    let mut test = env();
    let mut session = fixture_session("s1", "r-chat");
    session.stream_content = "partial".to_owned();
    session.stream_reasoning = "why".to_owned();
    session.stream_memory_used = Some(vec![fixture_memory("M4")]);
    test.store.seed_chat(session);
    test.registry.begin("r-chat");
    test.streaming.register_request_id("r-chat", "chat::a");
    test.streaming.handle_stream_error(
        &mut SharedStore(test.store.clone()),
        &error_payload("r-chat", "engine stalled"),
    );
    let session = visible_chat(&test);
    let message = chat_message(&test, 0);
    assert_eq!(message.id.as_deref(), Some("gen-1"));
    assert_eq!(message.role, AiMessageRole::Assistant);
    assert_eq!(message.content, "Error: engine stalled");
    // The failed marker carries neither reasoning, questions, tool calls,
    // cancellation, nor memory (React's literal).
    assert_eq!(message.reasoning, None);
    assert_eq!(message.questions, None);
    assert_eq!(message.tool_calls, None);
    assert_eq!(message.cancelled, None);
    assert_eq!(message.memory_used, None);
    assert_eq!(message.status, Some(AiMessageStatus::Failed));
    assert_eq!(message.created_at.as_deref(), Some(NOW_ISO));
    // The session's stream state is wiped and the memory records reset to
    // null (even though a run's memories were in flight).
    assert!(!session.is_streaming);
    assert_eq!(session.stream_content, "");
    assert_eq!(session.stream_reasoning, "");
    assert_eq!(session.stream_mcp_tool_name, None);
    assert_eq!(session.stream_memory_used, None);
    assert_eq!(session.memory_used, None);
    assert_eq!(session.request_id, None);
    // Persisted once with the failed session.
    assert_eq!(test.persister.calls().len(), 1);
    assert_eq!(
        test.persister.calls().first().map(|persisted| persisted.id.clone()),
        Some("s1".to_owned())
    );
}

#[test]
fn stream_error_repeats_are_ignored_once_the_request_id_clears() {
    let mut test = env();
    test.store.seed_chat(fixture_session("s1", "r-chat"));
    test.registry.begin("r-chat");
    let payload = error_payload("r-chat", "boom");
    test.streaming
        .handle_stream_error(&mut SharedStore(test.store.clone()), &payload);
    assert_eq!(visible_chat(&test).messages.len(), 1);
    // The retry finds no target (the session's request id is gone) and
    // stops before the registry or the persist.
    test.streaming
        .handle_stream_error(&mut SharedStore(test.store.clone()), &payload);
    assert_eq!(visible_chat(&test).messages.len(), 1);
    assert_eq!(test.persister.calls().len(), 1);
    assert_eq!(test.registry.error_calls(), vec!["r-chat".to_owned()]);
}

#[test]
fn stream_error_is_gated_when_the_registry_already_settled() {
    let mut test = env();
    // The visible session still carries the request id, but the registry
    // has already settled the run (the timeout path): the error is a
    // repeat.
    test.store.seed_chat(fixture_session("s1", "r-done"));
    test.registry.begin("r-done");
    test.registry.force_terminal("r-done");
    test.streaming.register_request_id("r-done", "chat::a");
    test.streaming.handle_stream_error(
        &mut SharedStore(test.store.clone()),
        &error_payload("r-done", "late boom"),
    );
    // No message, no persist — and the route survived the early return
    // (React returns before unregisterRequestId on a gated error).
    assert!(visible_chat(&test).messages.is_empty());
    assert!(test.persister.calls().is_empty());
    assert_eq!(test.streaming.unregister_request_id("r-done").as_deref(), Some("chat::a"));
}

// ── handleCancelTimeout ─────────────────────────────────────────────────

#[test]
fn cancel_timeout_marks_the_chat_session_cancelled_and_persists() {
    let mut test = env();
    let mut session = fixture_session("s1", "r-run");
    session.stream_content = "partial answer".to_owned();
    session.stream_reasoning = "because".to_owned();
    session.stream_memory_used = Some(vec![fixture_memory("M5")]);
    test.store.seed_chat(session);
    test.streaming.register_request_id("r-run", "chat::a");
    test.streaming
        .handle_cancel_timeout(&mut SharedStore(test.store.clone()), "r-run");
    let session = visible_chat(&test);
    let message = chat_message(&test, 0);
    // The marker: the partial content, the reasoning, cancelled + status.
    assert_eq!(message.id.as_deref(), Some("gen-1"));
    assert_eq!(message.role, AiMessageRole::Assistant);
    assert_eq!(message.content, "partial answer");
    assert_eq!(message.reasoning.as_deref(), Some("because"));
    assert_eq!(message.cancelled, Some(true));
    assert_eq!(message.status, Some(AiMessageStatus::Cancelled));
    assert_eq!(message.memory_used, Some(vec![fixture_memory("M5")]));
    assert_eq!(message.created_at.as_deref(), Some(NOW_ISO));
    // The session: the in-flight memory became the latest-run record.
    assert!(!session.is_streaming);
    assert_eq!(session.stream_memory_used, None);
    assert_eq!(session.memory_used, Some(vec![fixture_memory("M5")]));
    assert_eq!(session.request_id, None);
    assert_eq!(session.stream_content, "");
    // The chat arm persists (React's order: set, unregister, persist).
    assert_eq!(test.store.sets(), vec!["chat_session"]);
    assert_eq!(test.streaming.unregister_request_id("r-run"), None);
    assert_eq!(test.persister.calls().len(), 1);
}

#[test]
fn a_cancel_before_the_first_token_still_marks_the_session() {
    let mut test = env();
    // Empty stream state: the marker message is still appended so the UI
    // can say "Generation stopped." instead of silently resetting.
    test.store.seed_chat(fixture_session("s1", "r-empty"));
    test.streaming
        .handle_cancel_timeout(&mut SharedStore(test.store.clone()), "r-empty");
    let message = chat_message(&test, 0);
    assert_eq!(message.content, "");
    assert_eq!(message.reasoning, None);
    assert_eq!(message.cancelled, Some(true));
    assert_eq!(message.status, Some(AiMessageStatus::Cancelled));
}

#[test]
fn cancel_timeout_finalizes_the_workflow_session_without_persisting() {
    let mut test = env();
    let mut session = fixture_session("w1", "r-wf");
    session.stream_content = "partial".to_owned();
    test.store.seed_workflow(session);
    test.streaming.register_request_id("r-wf", "workflow::a");
    test.streaming
        .handle_cancel_timeout(&mut SharedStore(test.store.clone()), "r-wf");
    let session = visible_workflow(&test);
    assert_eq!(
        session.messages.first().map(|message| message.content.clone()),
        Some("partial".to_owned())
    );
    assert_eq!(session.request_id, None);
    assert_eq!(test.streaming.unregister_request_id("r-wf"), None);
    // The workflow arm never persists.
    assert!(test.persister.calls().is_empty());
    assert_eq!(test.store.sets(), vec!["workflow_chat"]);
}

#[test]
fn cancel_timeout_prefers_the_chat_session_when_both_match() {
    let mut test = env();
    let mut chat = fixture_session("s1", "r-same");
    chat.stream_content = "chat partial".to_owned();
    test.store.seed_chat(chat);
    let mut workflow = fixture_session("w1", "r-same");
    workflow.stream_content = "workflow partial".to_owned();
    test.store.seed_workflow(workflow);
    test.streaming
        .handle_cancel_timeout(&mut SharedStore(test.store.clone()), "r-same");
    // The chat arm ran (and persisted); the workflow is untouched.
    assert_eq!(
        visible_chat(&test).messages.first().map(|message| message.content.clone()),
        Some("chat partial".to_owned())
    );
    assert!(visible_workflow(&test).messages.is_empty());
    assert_eq!(test.persister.calls().len(), 1);
    assert_eq!(test.store.sets(), vec!["chat_session"]);
}

// ── persistStreamSession's guard and report ──────────────────────────────

/// A store view whose `set_chatSession` installs a *different* visible
/// session than the one the stream finalized — the model for the async gap
/// React's guard covers: by the time the persist runs, the visible session
/// is no longer this one, so the backend write is skipped.
struct SupersededStore {
    visible: Option<AiChatSession>,
    chat_sets: usize,
    migration_error: Option<String>,
}

impl AiStoreView for SupersededStore {
    fn chat_session(&self) -> Option<AiChatSession> {
        self.visible.clone()
    }

    fn workflow_chat(&self) -> Option<AiChatSession> {
        None
    }

    fn nl2sql(&self) -> Nl2sqlStreamingSlice {
        Nl2sqlStreamingSlice::default()
    }

    fn set_chat_session(&mut self, _session: AiChatSession) {
        // The terminal write lands elsewhere: the visible session moves on.
        self.chat_sets += 1;
        self.visible = Some(fixture_session("s-other", "r-next"));
    }

    fn set_workflow_chat(&mut self, _session: AiChatSession) {}

    fn set_nl2sql(&mut self, _nl2sql: Nl2sqlStreamingSlice) {}

    fn set_nl2sql_error(&mut self, _nl2sql: Nl2sqlStreamingSlice, _error: String) {}

    fn set_session_migration_error(&mut self, error: String) {
        self.migration_error = Some(error);
    }
}

#[test]
fn persist_stream_session_skips_when_the_visible_session_is_not_this_one() {
    let mut test = env();
    let mut view = SupersededStore {
        visible: Some(fixture_session("s1", "r-chat")),
        chat_sets: 0,
        migration_error: None,
    };
    test.registry.begin("r-chat");
    let mut payload = chunk("r-chat", "done");
    payload.done = true;
    test.streaming.handle_stream_chunk(&mut view, &payload);
    // The terminal write was attempted on the view…
    assert_eq!(view.chat_sets, 1);
    // …but the visible session moved on, so the persist was skipped
    // entirely — and no migration error was reported for a write that
    // never happened.
    assert!(test.persister.calls().is_empty());
    assert_eq!(view.migration_error, None);
}

#[test]
fn persist_stream_session_reports_the_error_with_the_fallback() {
    let mut payload = chunk("r-chat", "done");
    payload.done = true;
    // The backend write failed with no message of its own: the literal
    // fallback lands.
    let mut test = env();
    test.store.seed_chat(fixture_session("s1", "r-chat"));
    test.registry.begin("r-chat");
    test.persister
        .program_result(SessionPersistResult { ok: false, error: None, detail: None });
    test.streaming
        .handle_stream_chunk(&mut SharedStore(test.store.clone()), &payload);
    assert_eq!(
        test.store.session_migration_error().as_deref(),
        Some("AI session backend write failed")
    );
    // …and the backend's own message when it has one.
    let mut test = env();
    test.store.seed_chat(fixture_session("s1", "r-chat"));
    test.registry.begin("r-chat");
    test.persister.program_result(SessionPersistResult {
        ok: false,
        error: Some("db locked".to_owned()),
        detail: None,
    });
    test.streaming
        .handle_stream_chunk(&mut SharedStore(test.store.clone()), &payload);
    assert_eq!(test.store.session_migration_error().as_deref(), Some("db locked"));
    // A successful write reports nothing.
    let mut test = env();
    test.store.seed_chat(fixture_session("s1", "r-chat"));
    test.registry.begin("r-chat");
    test.persister
        .program_result(SessionPersistResult { ok: true, error: None, detail: None });
    test.streaming
        .handle_stream_chunk(&mut SharedStore(test.store.clone()), &payload);
    assert_eq!(test.store.session_migration_error(), None);
}
