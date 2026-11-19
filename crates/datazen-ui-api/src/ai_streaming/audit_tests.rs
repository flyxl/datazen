//! The T088 audit round's pins: every mutation that survived the ported
//! suite red-handed (an implementation change the existing pins did not
//! notice) is pinned here, one test per surviving mutant, each citing the
//! React behaviour it defends.
//!
//! The audit method (per the established rounds): read
//! `packages/frontend/src/stores/ai/streaming.ts` line by line against the
//! port, temporarily mutate every suspicious decision point — swap a
//! priority, drop a guard, clear a field the spread keeps, replace instead
//! of append — and run the module's suite. Nineteen mutants survived; a
//! twentieth control mutant (inverting the registry gate's acceptance
//! check) was caught red before any pin, proving the harness detects real
//! deviations and the nineteen survivors are genuine blind spots.
//!
//! The survivors, by React line:
//!
//! | React lines | The dropped semantic | Pin |
//! | --- | --- | --- | --- |
//! | L173-178 / L270-275 | the chat-before-workflow routing tie | [`both_sessions_matching_a_payload_route_it_to_the_chat_session`] |
//! | L115-127, L227-229, L293-294 | `[...session.messages, marker]` appends, never replaces | [`terminal_arms_append_to_the_existing_transcript`] |
//! | L196-199 | `memoryUsed.length > 0` — an empty payload list is not an update (chat arm) | [`an_empty_payload_memory_list_falls_back_on_the_chat_arm`] |
//! | L155-156 | the same guard on the NL2SQL arm | [`an_empty_payload_memory_list_falls_back_on_the_nl2sql_arm`] |
//! | L195 + L234 | the terminal chunk still writes the computed egress summary | [`the_terminal_chunk_writes_the_egress_summary`] |
//! | L106-128 | `finalizeCancelledSession`'s spread keeps the egress summary | [`the_cancel_timeout_keeps_the_sessions_egress_summary`] |
//! | L219 | `newReasoning \|\| undefined` on the terminal message | [`a_terminal_chunk_without_reasoning_leaves_the_message_reasoning_absent`] |
//! | L262-268 | the error write spreads `...nl2sql` — only `isGenerating` flips | [`the_nl2sql_error_write_only_flips_the_generating_flag`] |
//! | L262-276 | an nl2sql-owned error leaves a differently-owned chat untouched | [`an_error_for_the_nl2sql_run_leaves_a_chat_run_untouched`] |
//! | L87-99 | a timeout for an unmatched request finalises nothing | [`a_timeout_for_an_unmatched_request_touches_nothing`] |
//! | L149-171 | NL2SQL chunks bypass the chat lifecycle gate entirely | [`nl2sql_chunks_bypass_the_chat_lifecycle_gate`] |
//! | L170 | the NL2SQL branch ends in `return` — sessions never see its chunks | [`a_chunk_for_the_nl2sql_run_never_reaches_the_sessions`] |
//! | L157-166 | the done write spreads `...nl2sql` — the slice keeps its request id | [`the_terminal_nl2sql_chunk_keeps_the_slice_request_id`] |
//! | L154 | `extractSqlStreaming` runs on the terminal chunk too | [`the_terminal_nl2sql_chunk_still_recomputes_the_preview`] |
//! | L199 | the fallback reads `streamMemoryUsed`, not the latest-run record | [`the_memory_fallback_reads_the_in_flight_slot_not_the_latest_run`] |
//! | L225, L236, L253 | a memoryless run writes absent, not an empty list | [`a_memoryless_run_leaves_the_memory_records_absent`] |
//! | L241-243 / L306-308 / L91-93 | only the chat target persists — the workflow arm never does | [`a_workflow_target_never_persists_even_when_the_visible_chat_shares_the_id`] |
//! | L245-255 | the mid-stream computed key can target the workflow slot | [`mid_stream_chunks_route_to_the_workflow_session`] |
//! | L185-188 | the gate input carries `{ done, cancelled }` verbatim | [`the_gate_input_carries_the_chunks_terminal_flags`] |
//!
//! Equivalent mutants (NOT pinned, with the reason): `payload.content ||
//! ''` / `session.streamContent || ''` and the `?? null` / `?? undefined`
//! folds are type-level in the port (`content: String`, `Option` slots —
//! the empty string and the absent Option are the only spellings), and
//! React's promise-deferred persist reporting was folded to the sync
//! T040 precedent at port time (the guard's re-read position is pinned in
//! `tests::persist_stream_session_skips_when_the_visible_session_is_not_this_one`).

use crate::ai_types::{AiChatMessage, AiChatSession, AiMessageRole};

use super::seams::{ApplyChunkGate, Nl2sqlStreamingSlice};
use super::test_support::{
    SharedStore, TestEnv, chunk, env, error_payload, fixture_memory, fixture_session,
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

// ── Routing ───────────────────────────────────────────────────────────────

#[test]
fn both_sessions_matching_a_payload_route_it_to_the_chat_session() {
    // React L173-178 / L270-275: `chatSession && payload.requestId ===
    // chatSession.requestId ? 'chatSession' : workflowChat && …` — the chat
    // session is checked FIRST, so a requestId both live sessions carry
    // belongs to the chat.
    let mut test = env();
    test.store.seed_chat(fixture_session("s1", "r-tie"));
    test.store.seed_workflow(fixture_session("w1", "r-tie"));
    test.registry.begin("r-tie");
    test.streaming
        .handle_stream_chunk(&mut SharedStore(test.store.clone()), &chunk("r-tie", "x"));
    // The chat slot got the chunk…
    assert_eq!(visible_chat(&test).stream_content, "x");
    // …the workflow slot carrying the SAME request id did not.
    assert_eq!(visible_workflow(&test).stream_content, "");
    assert!(visible_workflow(&test).messages.is_empty());
    assert_eq!(test.store.sets(), vec!["chat_session"]);
    // And the error handler's twin routing (React L270-275) agrees.
    test.streaming.handle_stream_error(
        &mut SharedStore(test.store.clone()),
        &error_payload("r-tie", "boom"),
    );
    assert_eq!(chat_message(&test, 0).content, "Error: boom");
    assert!(visible_workflow(&test).messages.is_empty());
    assert_eq!(test.store.sets(), vec!["chat_session", "chat_session"]);
    assert_eq!(test.persister.calls().len(), 1);
}

#[test]
fn mid_stream_chunks_route_to_the_workflow_session() {
    // React L245-255: the else arm's computed `[targetSession]` key — a
    // mid-stream chunk for the workflow chat updates the workflow slot
    // (and only it): the accumulated text, the still-streaming state, the
    // intact request id, no message yet, no persist.
    let mut test = env();
    test.store.seed_chat(fixture_session("s1", "r-other"));
    let mut workflow = fixture_session("w1", "r-wf");
    workflow.stream_content = "ans".to_owned();
    test.store.seed_workflow(workflow);
    test.registry.begin("r-wf");
    test.streaming
        .handle_stream_chunk(&mut SharedStore(test.store.clone()), &chunk("r-wf", "wer"));
    let workflow = visible_workflow(&test);
    assert_eq!(workflow.stream_content, "answer");
    assert!(workflow.is_streaming);
    assert_eq!(workflow.request_id.as_deref(), Some("r-wf"));
    assert!(workflow.messages.is_empty());
    assert_eq!(visible_chat(&test).stream_content, "");
    assert!(visible_chat(&test).messages.is_empty());
    assert_eq!(test.store.sets(), vec!["workflow_chat"]);
    assert!(test.persister.calls().is_empty());
}

// ── The transcript ─────────────────────────────────────────────────────────

#[test]
fn terminal_arms_append_to_the_existing_transcript() {
    // React L115-127, L227-229, L293-294: every terminal arm builds
    // `messages: [...session.messages, marker]` — the new message is
    // appended, never a replacement for the transcript.
    let prior = || vec![AiChatMessage::new(AiMessageRole::User, "earlier turn")];
    // The done chunk's assistant marker…
    let mut test = env();
    let mut session = fixture_session("s1", "r-done");
    session.messages = prior();
    test.store.seed_chat(session);
    test.registry.begin("r-done");
    let mut payload = chunk("r-done", "answer");
    payload.done = true;
    test.streaming
        .handle_stream_chunk(&mut SharedStore(test.store.clone()), &payload);
    let messages = visible_chat(&test).messages;
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].content, "earlier turn");
    assert_eq!(messages[1].role, AiMessageRole::Assistant);
    // …the stream error's failed marker…
    let mut test = env();
    let mut session = fixture_session("s2", "r-err");
    session.messages = prior();
    test.store.seed_chat(session);
    test.registry.begin("r-err");
    test.streaming.handle_stream_error(
        &mut SharedStore(test.store.clone()),
        &error_payload("r-err", "boom"),
    );
    let messages = visible_chat(&test).messages;
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].content, "earlier turn");
    assert_eq!(messages[1].content, "Error: boom");
    // …and the cancel-timeout's cancelled marker.
    let mut test = env();
    let mut session = fixture_session("s3", "r-time");
    session.messages = prior();
    test.store.seed_chat(session);
    test.streaming
        .handle_cancel_timeout(&mut SharedStore(test.store.clone()), "r-time");
    let messages = visible_chat(&test).messages;
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].content, "earlier turn");
    assert_eq!(messages[1].cancelled, Some(true));
}

// ── MemoryUsed ─────────────────────────────────────────────────────────────

#[test]
fn an_empty_payload_memory_list_falls_back_on_the_chat_arm() {
    // React L196-199: `payload.memoryUsed && payload.memoryUsed.length > 0
    // ? payload.memoryUsed : (session.streamMemoryUsed ?? null)` — an empty
    // payload list is NOT a memory update; the in-flight record stays.
    let mut test = env();
    let mut session = fixture_session("s1", "r-e");
    session.stream_memory_used = Some(vec![fixture_memory("M-old")]);
    test.store.seed_chat(session);
    test.registry.begin("r-e");
    let mut payload = chunk("r-e", "x");
    payload.memory_used = Some(Vec::new());
    test.streaming
        .handle_stream_chunk(&mut SharedStore(test.store.clone()), &payload);
    let session = visible_chat(&test);
    assert_eq!(session.stream_memory_used, Some(vec![fixture_memory("M-old")]));
    assert_eq!(session.memory_used, Some(vec![fixture_memory("M-old")]));
}

#[test]
fn an_empty_payload_memory_list_falls_back_on_the_nl2sql_arm() {
    // React L155-156: the same `length > 0` guard on the NL2SQL branch —
    // an empty payload list leaves the slice's own memories.
    let mut test = env();
    test.store.seed_nl2sql(Nl2sqlStreamingSlice {
        request_id: Some("r-nl".to_owned()),
        streaming_sql: String::new(),
        streaming_preview: String::new(),
        generated_sql: String::new(),
        memory_used: vec![fixture_memory("M1")],
        is_generating: true,
    });
    let mut payload = chunk("r-nl", "SELE");
    payload.memory_used = Some(Vec::new());
    test.streaming
        .handle_stream_chunk(&mut SharedStore(test.store.clone()), &payload);
    assert_eq!(test.store.nl2sql_state().memory_used, vec![fixture_memory("M1")]);
}

#[test]
fn the_memory_fallback_reads_the_in_flight_slot_not_the_latest_run() {
    // React L199: the fallback is `(session.streamMemoryUsed ?? null)` —
    // the IN-FLIGHT record, not the latest-run one (`session.memoryUsed`),
    // fills a chunk that carries no memories.
    let mut test = env();
    let mut session = fixture_session("s1", "r-f");
    session.memory_used = Some(vec![fixture_memory("M-old")]);
    session.stream_memory_used = Some(vec![fixture_memory("M-new")]);
    test.store.seed_chat(session);
    test.registry.begin("r-f");
    test.streaming
        .handle_stream_chunk(&mut SharedStore(test.store.clone()), &chunk("r-f", "x"));
    let session = visible_chat(&test);
    assert_eq!(session.stream_memory_used, Some(vec![fixture_memory("M-new")]));
    assert_eq!(session.memory_used, Some(vec![fixture_memory("M-new")]));
}

#[test]
fn a_memoryless_run_leaves_the_memory_records_absent() {
    // React L225 (`memoryUsed ?? undefined` on the message) and L236 / L253
    // (`memoryUsed: memoryUsed ?? null` on the session): a run with no
    // memories anywhere writes absent, not an empty list.
    let mut test = env();
    test.store.seed_chat(fixture_session("s1", "r-d"));
    test.registry.begin("r-d");
    let mut payload = chunk("r-d", "answer");
    payload.done = true;
    test.streaming
        .handle_stream_chunk(&mut SharedStore(test.store.clone()), &payload);
    assert_eq!(chat_message(&test, 0).memory_used, None);
    let session = visible_chat(&test);
    assert_eq!(session.memory_used, None);
    assert_eq!(session.stream_memory_used, None);
    // The mid-stream arm folds the same way.
    let mut test = env();
    test.store.seed_chat(fixture_session("s2", "r-m"));
    test.registry.begin("r-m");
    test.streaming
        .handle_stream_chunk(&mut SharedStore(test.store.clone()), &chunk("r-m", "x"));
    let session = visible_chat(&test);
    assert_eq!(session.memory_used, None);
    assert_eq!(session.stream_memory_used, None);
}

// ── EgressSummary / reasoning ─────────────────────────────────────────────

#[test]
fn the_terminal_chunk_writes_the_egress_summary() {
    // React L195 + L234: `egressSummary` is computed before the done branch
    // and written into the terminal session — a one-shot run whose only
    // chunk is terminal still lands its trimmed payload scope…
    let mut test = env();
    test.store.seed_chat(fixture_session("s1", "r-a"));
    test.registry.begin("r-a");
    let mut payload = chunk("r-a", "done");
    payload.done = true;
    payload.egress_summary = Some("  one-shot  ".to_owned());
    test.streaming
        .handle_stream_chunk(&mut SharedStore(test.store.clone()), &payload);
    assert_eq!(visible_chat(&test).egress_summary.as_deref(), Some("one-shot"));
    // …and a session-level summary still wins over the payload's at
    // terminal time (`session.egressSummary || …`).
    let mut test = env();
    let mut session = fixture_session("s2", "r-b");
    session.egress_summary = Some("first".to_owned());
    test.store.seed_chat(session);
    test.registry.begin("r-b");
    let mut payload = chunk("r-b", "done");
    payload.done = true;
    payload.egress_summary = Some(" later ".to_owned());
    test.streaming
        .handle_stream_chunk(&mut SharedStore(test.store.clone()), &payload);
    assert_eq!(visible_chat(&test).egress_summary.as_deref(), Some("first"));
}

#[test]
fn the_cancel_timeout_keeps_the_sessions_egress_summary() {
    // React L106-128: `finalizeCancelledSession` spreads `...session`, so
    // the recorded egress scope survives the 8s fallback finaliser.
    let mut test = env();
    let mut session = fixture_session("s1", "r-t");
    session.egress_summary = Some("scope".to_owned());
    test.store.seed_chat(session);
    test.streaming
        .handle_cancel_timeout(&mut SharedStore(test.store.clone()), "r-t");
    let session = visible_chat(&test);
    assert_eq!(session.egress_summary.as_deref(), Some("scope"));
    assert_eq!(session.messages.len(), 1);
}

#[test]
fn a_terminal_chunk_without_reasoning_leaves_the_message_reasoning_absent() {
    // React L219: `reasoning: newReasoning || undefined` — a run that never
    // streamed reasoning keeps the field absent on the terminal message.
    let mut test = env();
    test.store.seed_chat(fixture_session("s1", "r-r"));
    test.registry.begin("r-r");
    let mut payload = chunk("r-r", "answer");
    payload.done = true;
    test.streaming
        .handle_stream_chunk(&mut SharedStore(test.store.clone()), &payload);
    assert_eq!(chat_message(&test, 0).reasoning, None);
}

// ── The NL2SQL branch ──────────────────────────────────────────────────────

#[test]
fn nl2sql_chunks_bypass_the_chat_lifecycle_gate() {
    // React L149-171: the NL2SQL branch runs and returns BEFORE the
    // registry gate — the slice is "not part of the chat lifecycle
    // registry", so even a run the registry has already settled must not
    // drop its chunks.
    let mut test = env();
    test.store.seed_nl2sql(Nl2sqlStreamingSlice {
        request_id: Some("r-nl".to_owned()),
        ..Default::default()
    });
    test.registry.force_terminal("r-nl");
    test.streaming
        .handle_stream_chunk(&mut SharedStore(test.store.clone()), &chunk("r-nl", "SELE"));
    assert_eq!(test.store.nl2sql_state().streaming_sql, "SELE");
    assert_eq!(test.store.sets(), vec!["nl2sql"]);
    // The gate was never even consulted.
    assert!(test.registry.chunk_calls().is_empty());
}

#[test]
fn a_chunk_for_the_nl2sql_run_never_reaches_the_sessions() {
    // React L170: the NL2SQL branch ends in `return` — a chat session that
    // happens to carry the same requestId never sees the chunk.
    let mut test = env();
    test.store.seed_nl2sql(Nl2sqlStreamingSlice {
        request_id: Some("r-shared".to_owned()),
        ..Default::default()
    });
    let mut chat = fixture_session("s1", "r-shared");
    chat.stream_content = "chat".to_owned();
    test.store.seed_chat(chat);
    test.streaming
        .handle_stream_chunk(&mut SharedStore(test.store.clone()), &chunk("r-shared", "x"));
    assert_eq!(test.store.nl2sql_state().streaming_sql, "x");
    assert_eq!(visible_chat(&test).stream_content, "chat");
    assert!(visible_chat(&test).messages.is_empty());
    assert_eq!(test.store.sets(), vec!["nl2sql"]);
}

#[test]
fn the_terminal_nl2sql_chunk_keeps_the_slice_request_id() {
    // React L157-166: the done write spreads `...nl2sql` — the slice's own
    // request id survives the terminal chunk (only the route map entry
    // unregisters, L168).
    let mut test = env();
    test.store.seed_nl2sql(Nl2sqlStreamingSlice {
        request_id: Some("r-nl".to_owned()),
        streaming_sql: "SELECT 1".to_owned(),
        ..Default::default()
    });
    test.streaming.register_request_id("r-nl", "nl2sql");
    let mut payload = chunk("r-nl", " FROM t");
    payload.done = true;
    test.streaming
        .handle_stream_chunk(&mut SharedStore(test.store.clone()), &payload);
    let nl2sql = test.store.nl2sql_state();
    assert_eq!(nl2sql.request_id.as_deref(), Some("r-nl"));
    assert!(!nl2sql.is_generating);
    // …while the route map entry did go.
    assert_eq!(test.streaming.unregister_request_id("r-nl"), None);
}

#[test]
fn the_terminal_nl2sql_chunk_still_recomputes_the_preview() {
    // React L154: `extractSqlStreaming(accumulated)` runs on EVERY chunk —
    // the terminal one included (the preview is not frozen at the last
    // mid-stream value).
    let mut test = env();
    test.store.seed_nl2sql(Nl2sqlStreamingSlice {
        request_id: Some("r-nl".to_owned()),
        streaming_sql: "SELECT 1".to_owned(),
        streaming_preview: "preview:SELECT 1".to_owned(),
        ..Default::default()
    });
    let mut payload = chunk("r-nl", " FROM t");
    payload.done = true;
    test.streaming
        .handle_stream_chunk(&mut SharedStore(test.store.clone()), &payload);
    assert_eq!(
        test.store.nl2sql_state().streaming_preview,
        "preview:SELECT 1 FROM t"
    );
}

#[test]
fn the_nl2sql_error_write_only_flips_the_generating_flag() {
    // React L262-268: the error arm writes `{ ...nl2sql, isGenerating:
    // false }` — the spread keeps the accumulated SQL, the preview, the
    // memories and the slice's own request id; only the flag flips (and
    // the paired `nl2sqlError` lands).
    let mut test = env();
    test.store.seed_nl2sql(Nl2sqlStreamingSlice {
        request_id: Some("r-nl".to_owned()),
        streaming_sql: "SELECT".to_owned(),
        streaming_preview: "preview:SELECT".to_owned(),
        generated_sql: String::new(),
        memory_used: vec![fixture_memory("M1")],
        is_generating: true,
    });
    test.streaming.register_request_id("r-nl", "nl2sql");
    test.streaming.handle_stream_error(
        &mut SharedStore(test.store.clone()),
        &error_payload("r-nl", "boom"),
    );
    let nl2sql = test.store.nl2sql_state();
    assert_eq!(nl2sql.streaming_sql, "SELECT");
    assert_eq!(nl2sql.streaming_preview, "preview:SELECT");
    assert_eq!(nl2sql.memory_used, vec![fixture_memory("M1")]);
    assert_eq!(nl2sql.request_id.as_deref(), Some("r-nl"));
    assert!(!nl2sql.is_generating);
    assert_eq!(test.store.nl2sql_error().as_deref(), Some("boom"));
    // The route map entry did go (React L263 unregisters) and nothing else
    // ran: no session write, no registry consult, no persist.
    assert_eq!(test.streaming.unregister_request_id("r-nl"), None);
    assert_eq!(test.store.sets(), vec!["nl2sql_error"]);
    assert!(test.registry.error_calls().is_empty());
    assert!(test.persister.calls().is_empty());
}

#[test]
fn an_error_for_the_nl2sql_run_leaves_a_chat_run_untouched() {
    // React L262-276: the nl2sql arm's write is the whole story for an
    // nl2sql-owned requestId no chat session carries — the fall-through
    // finds no target and stops before a differently-owned chat run is
    // touched.
    let mut test = env();
    test.store.seed_nl2sql(Nl2sqlStreamingSlice {
        request_id: Some("r-nl".to_owned()),
        ..Default::default()
    });
    let mut chat = fixture_session("s1", "r-chat");
    chat.stream_content = "draft".to_owned();
    test.store.seed_chat(chat);
    test.streaming.register_request_id("r-chat", "chat::a");
    test.streaming.handle_stream_error(
        &mut SharedStore(test.store.clone()),
        &error_payload("r-nl", "boom"),
    );
    assert_eq!(test.store.nl2sql_error().as_deref(), Some("boom"));
    let chat = visible_chat(&test);
    assert_eq!(chat.stream_content, "draft");
    assert_eq!(chat.request_id.as_deref(), Some("r-chat"));
    assert!(chat.is_streaming);
    assert!(chat.messages.is_empty());
    assert_eq!(test.store.sets(), vec!["nl2sql_error"]);
    assert!(test.persister.calls().is_empty());
}

// ── handleCancelTimeout ────────────────────────────────────────────────────

#[test]
fn a_timeout_for_an_unmatched_request_touches_nothing() {
    // React L87-99: both arms compare the requestId — a timeout for a run
    // neither visible session owns finalises nothing and consumes no route.
    let mut test = env();
    test.store.seed_chat(fixture_session("s1", "r-a"));
    test.store.seed_workflow(fixture_session("w1", "r-b"));
    test.streaming.register_request_id("r-c", "k");
    test.streaming
        .handle_cancel_timeout(&mut SharedStore(test.store.clone()), "r-c");
    assert!(test.store.sets().is_empty());
    let chat = visible_chat(&test);
    assert_eq!(chat.request_id.as_deref(), Some("r-a"));
    assert!(chat.is_streaming);
    assert!(chat.messages.is_empty());
    let workflow = visible_workflow(&test);
    assert_eq!(workflow.request_id.as_deref(), Some("r-b"));
    assert!(workflow.messages.is_empty());
    // The route survived — a miss must not unregister (React L92/L98 only
    // run inside a matching arm).
    assert_eq!(test.streaming.unregister_request_id("r-c").as_deref(), Some("k"));
    assert!(test.persister.calls().is_empty());
}

// ── The persist rule ──────────────────────────────────────────────────────

#[test]
fn a_workflow_target_never_persists_even_when_the_visible_chat_shares_the_id() {
    // React L241-243, L306-308, L91-93: only the chat target persists — the
    // workflow arm never does. The visible chat session is seeded with the
    // SAME id as the workflow session so the persist guard (which compares
    // ids) could not mask a stray workflow persist.
    // The stream error's workflow arm:
    let mut test = env();
    test.store.seed_chat(fixture_session("w1", "r-other"));
    test.store.seed_workflow(fixture_session("w1", "r-w"));
    test.registry.begin("r-w");
    test.streaming.handle_stream_error(
        &mut SharedStore(test.store.clone()),
        &error_payload("r-w", "boom"),
    );
    assert_eq!(visible_workflow(&test).messages.len(), 1);
    assert!(test.persister.calls().is_empty());
    assert_eq!(test.store.sets(), vec!["workflow_chat"]);
    // The done chunk's workflow arm:
    let mut test = env();
    test.store.seed_chat(fixture_session("w1", "r-other"));
    test.store.seed_workflow(fixture_session("w1", "r-w"));
    test.registry.begin("r-w");
    let mut payload = chunk("r-w", "answer");
    payload.done = true;
    test.streaming
        .handle_stream_chunk(&mut SharedStore(test.store.clone()), &payload);
    assert_eq!(visible_workflow(&test).messages.len(), 1);
    assert!(test.persister.calls().is_empty());
    assert_eq!(test.store.sets(), vec!["workflow_chat"]);
    // And the cancel-timeout's workflow arm:
    let mut test = env();
    test.store.seed_chat(fixture_session("w1", "r-other"));
    test.store.seed_workflow(fixture_session("w1", "r-w"));
    test.streaming
        .handle_cancel_timeout(&mut SharedStore(test.store.clone()), "r-w");
    assert_eq!(visible_workflow(&test).messages.len(), 1);
    assert!(test.persister.calls().is_empty());
    assert_eq!(test.store.sets(), vec!["workflow_chat"]);
}

// ── The registry gate input ───────────────────────────────────────────────

#[test]
fn the_gate_input_carries_the_chunks_terminal_flags() {
    // React L185-188: `applyChunk(requestId, { done, cancelled })` — the
    // gate sees the chunk's own terminal flags verbatim.
    let mut test = env();
    test.store.seed_chat(fixture_session("s1", "r-g"));
    test.registry.begin("r-g");
    let mut mid = chunk("r-g", "x");
    mid.cancelled = Some(false);
    test.streaming
        .handle_stream_chunk(&mut SharedStore(test.store.clone()), &mid);
    let mut done = chunk("r-g", "");
    done.done = true;
    done.cancelled = Some(true);
    test.streaming
        .handle_stream_chunk(&mut SharedStore(test.store.clone()), &done);
    assert_eq!(
        test.registry.chunk_calls(),
        vec![
            ("r-g".to_owned(), ApplyChunkGate { done: false, cancelled: Some(false) }),
            ("r-g".to_owned(), ApplyChunkGate { done: true, cancelled: Some(true) }),
        ]
    );
}
