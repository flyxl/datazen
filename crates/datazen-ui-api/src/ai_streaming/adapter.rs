//! The streaming adapter itself: the [`AiStreaming`] struct and the
//! operations React exports from `stores/ai/streaming.ts`.
//!
//! Every exported function of the React module becomes a method here; the
//! two module-private helpers (`finalizeCancelledSession`,
//! `persistStreamSession`) become private methods. The zustand
//! `get`/`set` pair a React caller passes in becomes the
//! [`AiStoreView`](super::seams::AiStoreView) argument each operation
//! takes, and the imports become the constructor's seams (see the module
//! docs for the ownership table).

use std::collections::HashMap;

use crate::ai_sessions::NewId;
use crate::ai_types::{
    AiChatAttribution, AiChatMessage, AiChatSession, AiMessageRole, AiMessageStatus,
    AiQuestion, StreamChunkPayload, StreamErrorPayload,
};

use super::seams::{
    AiStoreView, AiTargetClaim, ApplyChunkGate, CancelReceipt, ChatSessionPersister,
    ExtractionSeams, Nl2sqlStreamingSlice, SessionCancellerOps, StreamRegistryOps, StreamingClock,
    find_mcp_tool_name,
};

/// React's fallback literal in `persistStreamSession`'s error report
/// (`result.error ?? 'AI session backend write failed'`).
const PERSIST_ERROR_FALLBACK: &str = "AI session backend write failed";

/// Which live session a payload's requestId routes to — React's
/// `'chatSession' | 'workflowChat'` string union (the `[targetSession]`
/// computed key).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    Chat,
    Workflow,
}

/// The routing both handlers share: the chat session first, then the
/// workflow chat session, `None` when the requestId belongs to neither
/// (React's `targetSession` / `errTarget` conditional chains).
fn route_target(
    chat: &Option<AiChatSession>,
    workflow: &Option<AiChatSession>,
    request_id: &str,
) -> Option<Target> {
    if chat.as_ref().and_then(|session| session.request_id.as_deref()) == Some(request_id) {
        Some(Target::Chat)
    } else if workflow.as_ref().and_then(|session| session.request_id.as_deref())
        == Some(request_id)
    {
        Some(Target::Workflow)
    } else {
        None
    }
}

/// The streaming adapter: React's module-level state (the `streamRegistry`
/// singleton, the lazily-initialised `sessionCanceller`, and the
/// `requestIdToKey` routing map) becomes this struct's fields, so one
/// instance carries the same state across every call exactly the way the
/// React module does.
///
/// Construction injects every ambient dependency (see the module docs'
/// seam table); hosts keep the concrete registry handle for `begin()` (the
/// send path's call, which streaming.ts never makes).
pub struct AiStreaming {
    /// React's module-level `streamRegistry` singleton.
    registry: Box<dyn StreamRegistryOps>,
    /// React's `sessionCanceller` singleton — construction-time injection
    /// preserves the lazy-init's "same instance across calls" semantics.
    canceller: Box<dyn SessionCancellerOps>,
    /// React `attributionOf` (imported from `ai/targetClaim`).
    attribution_of: Box<dyn Fn(Option<&AiTargetClaim>) -> AiChatAttribution>,
    /// React's four extraction imports (`lib/extractQuestions`,
    /// `lib/extractSql`).
    extractions: ExtractionSeams,
    /// React `new Date().toISOString()`.
    clock: StreamingClock,
    /// React `crypto.randomUUID()`.
    new_id: NewId,
    /// React `persistChatSession` (already ported, T040).
    persister: Box<dyn ChatSessionPersister>,
    /// React's module-level `requestIdToKey` routing map.
    request_id_to_key: HashMap<String, String>,
}

impl AiStreaming {
    /// Wires the adapter with its seams: the registry singleton, the
    /// canceller orchestrator, the attribution projection, the four
    /// extraction functions, the clock, the id mint, and the
    /// already-ported session persister.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        registry: Box<dyn StreamRegistryOps>,
        canceller: Box<dyn SessionCancellerOps>,
        attribution_of: Box<dyn Fn(Option<&AiTargetClaim>) -> AiChatAttribution>,
        extractions: ExtractionSeams,
        clock: StreamingClock,
        new_id: NewId,
        persister: Box<dyn ChatSessionPersister>,
    ) -> Self {
        Self {
            registry,
            canceller,
            attribution_of,
            extractions,
            clock,
            new_id,
            persister,
            request_id_to_key: HashMap::new(),
        }
    }

    /// React `getStreamRegistry()`: the registry singleton, exposed so the
    /// send path (the aiStore port, T128) can reach the same instance the
    /// chunk/error handlers gate through.
    pub fn registry(&mut self) -> &mut dyn StreamRegistryOps {
        self.registry.as_mut()
    }

    /// React `registerRequestId(requestId, key)`: maps an active requestId
    /// to the session key it belongs to.
    pub fn register_request_id(&mut self, request_id: &str, key: &str) {
        self.request_id_to_key
            .insert(request_id.to_owned(), key.to_owned());
    }

    /// React `unregisterRequestId(requestId)` — returns the key it carried
    /// (`undefined` when it had none).
    pub fn unregister_request_id(&mut self, request_id: &str) -> Option<String> {
        self.request_id_to_key.remove(request_id)
    }

    /// React `attributionFor(targetClaim?)` — the pure forward to the
    /// injected `attributionOf` seam (`ai/targetClaim.ts`, cross-segment):
    /// the attribution metadata recorded with the session when a run starts.
    pub fn attribution_for(&self, target_claim: Option<&AiTargetClaim>) -> AiChatAttribution {
        (self.attribution_of)(target_claim)
    }

    /// React `cancelChat(get, set, requestId?)`: ask the host to cancel the
    /// in-flight chat run.
    ///
    /// The receipt drives the UI — `marked` leaves the stream registry in
    /// `cancelling` (the terminal chunk or the 8s fallback finishes it),
    /// `alreadyFinished` needs no transition, and `forbidden` only warns —
    /// but that translation is the canceller's (its seam's owner task);
    /// this method only resolves the request id and delegates. React's
    /// promise resolves with the receipt or `null` on transport failure,
    /// so the port returns `Option<CancelReceipt>`.
    pub fn cancel_chat(
        &mut self,
        store: &dyn AiStoreView,
        request_id: Option<&str>,
    ) -> Option<CancelReceipt> {
        // `requestId ?? get().chatSession?.requestId ?? null` — `??` does
        // not fold an empty string, but the `!rid` check below does.
        let rid: Option<String> = match request_id {
            Some(explicit) => Some(explicit.to_owned()),
            None => store.chat_session().and_then(|session| session.request_id.clone()),
        };
        // `if (!rid) return null`.
        let rid = rid.filter(|rid| !rid.is_empty())?;
        self.canceller.cancel(&rid)
    }

    /// React `handleCancelTimeout(get, set, requestId)` — the 8s
    /// fallback's callback: the backend never delivered its terminal
    /// chunk, so finalise the visible session locally as cancelled and the
    /// stop button cannot spin forever. The chat arm persists; the workflow
    /// arm does not. The future canceller adapter wires this method as its
    /// `onCancelTimeout` (see the [`SessionCancellerOps`] docs).
    pub fn handle_cancel_timeout(&mut self, store: &mut dyn AiStoreView, request_id: &str) {
        if let Some(session) = store.chat_session() {
            if session.request_id.as_deref() == Some(request_id) {
                let next = self.finalize_cancelled_session(&session);
                store.set_chat_session(next.clone());
                self.unregister_request_id(request_id);
                self.persist_stream_session(store, &next);
                return;
            }
        }
        if let Some(session) = store.workflow_chat() {
            if session.request_id.as_deref() == Some(request_id) {
                let next = self.finalize_cancelled_session(&session);
                store.set_workflow_chat(next);
                self.unregister_request_id(request_id);
            }
        }
    }

    /// React `handleStreamChunk(get, set, payload)` — apply a stream
    /// chunk, dropping late/duplicate chunks through the registry.
    pub fn handle_stream_chunk(
        &mut self,
        store: &mut dyn AiStoreView,
        payload: &StreamChunkPayload,
    ) {
        // React's single `get()` destructure: one snapshot for the whole
        // handler.
        let nl2sql = store.nl2sql();
        let chat = store.chat_session();
        let workflow = store.workflow_chat();

        // ── NL2SQL routing (not part of the chat lifecycle registry) ──
        if nl2sql.request_id.as_deref() == Some(payload.request_id.as_str()) {
            // `payload.content ? nl2sql.streamingSql + payload.content
            // : nl2sql.streamingSql` — an empty chunk leaves the
            // accumulation untouched.
            let mut accumulated = nl2sql.streaming_sql.clone();
            if !payload.content.is_empty() {
                accumulated.push_str(&payload.content);
            }
            let streaming_preview = (self.extractions.extract_sql_streaming)(&accumulated);
            // `payload.memoryUsed && payload.memoryUsed.length > 0 ?
            // payload.memoryUsed : nl2sql.memoryUsed`.
            let memory_used = match payload.memory_used.as_ref() {
                Some(list) if !list.is_empty() => list.clone(),
                _ => nl2sql.memory_used.clone(),
            };
            let generated_sql = if payload.done {
                (self.extractions.extract_sql_from_response)(&accumulated)
            } else {
                nl2sql.generated_sql.clone()
            };
            let is_generating = if payload.done { false } else { nl2sql.is_generating };
            store.set_nl2sql(Nl2sqlStreamingSlice {
                request_id: nl2sql.request_id.clone(),
                streaming_sql: accumulated,
                streaming_preview,
                generated_sql,
                memory_used,
                is_generating,
            });
            if payload.done {
                self.unregister_request_id(&payload.request_id);
            }
            return;
        }

        let Some(target) = route_target(&chat, &workflow, &payload.request_id) else {
            return;
        };

        // Generation / cancel / terminal gates. A dropped chunk must not
        // touch the rendered text; the terminal chunk itself is still
        // accepted at most once. (The gate applies only when the registry
        // knows the request — an untracked requestId passes ungated.)
        if self.registry.has(&payload.request_id) {
            let applied = self.registry.apply_chunk(
                &payload.request_id,
                &ApplyChunkGate { done: payload.done, cancelled: payload.cancelled },
            );
            if !applied.accepted {
                return;
            }
        }

        // React's `(targetSession === 'chatSession' ? chatSession :
        // workflowChat)!` — the routing match read the same captured
        // snapshot, so the entry is present by construction; the `else` is
        // the type-level spelling of the TS `!` and never runs.
        let Some(mut session) = (match target {
            Target::Chat => chat,
            Target::Workflow => workflow,
        }) else {
            return;
        };

        // The host sends the scope on the first chunk; later chunks must
        // not replace it within the same run.
        // `session.egressSummary || payload.egressSummary?.trim() || null`.
        let egress_summary = session
            .egress_summary
            .clone()
            .filter(|summary| !summary.is_empty())
            .or_else(|| {
                payload
                    .egress_summary
                    .as_ref()
                    .map(|summary| summary.trim().to_owned())
                    .filter(|trimmed| !trimmed.is_empty())
            });
        // `payload.memoryUsed && payload.memoryUsed.length > 0 ?
        // payload.memoryUsed : (session.streamMemoryUsed ?? null)`.
        let memory_used = match payload.memory_used.as_ref() {
            Some(list) if !list.is_empty() => Some(list.clone()),
            _ => session.stream_memory_used.clone(),
        };
        // `(session.streamContent || '') + (payload.content || '')` and
        // the reasoning twin.
        let new_content = format!("{}{}", session.stream_content, payload.content);
        let new_reasoning = format!(
            "{}{}",
            session.stream_reasoning,
            payload.reasoning.as_deref().unwrap_or_default()
        );
        // `findMcpToolName(payload.toolCalls) ?? session.streamMcpToolName ?? null`.
        let mcp_tool_name = find_mcp_tool_name(payload.tool_calls.as_deref())
            .map(str::to_owned)
            .or_else(|| session.stream_mcp_tool_name.clone());

        if payload.done {
            self.unregister_request_id(&payload.request_id);
            // `extractQuestions(newContent)` — the seam's parse.
            let outcome = (self.extractions.extract_questions)(&new_content);
            // `payload.toolCalls && payload.toolCalls.length > 0 ?
            // payload.toolCalls : undefined`.
            let tool_calls = payload.tool_calls.clone().filter(|calls| !calls.is_empty());
            // xmlQuestions win; the tool-call questions fill in only when
            // the text carried none.
            let mut questions: Option<Vec<AiQuestion>> =
                (!outcome.questions.is_empty()).then_some(outcome.questions);
            if questions.is_none() {
                if let Some(calls) = tool_calls.as_ref() {
                    let tool_questions = (self.extractions.parse_tool_call_questions)(calls);
                    if !tool_questions.is_empty() {
                        questions = Some(tool_questions);
                    }
                }
            }
            let cancelled = payload.cancelled == Some(true);
            session.messages.push(AiChatMessage {
                id: Some((self.new_id)()),
                role: AiMessageRole::Assistant,
                content: outcome.clean_content,
                // `newReasoning || undefined`.
                reasoning: (!new_reasoning.is_empty()).then_some(new_reasoning),
                questions,
                tool_calls,
                tool_call_id: None,
                cancelled: Some(cancelled),
                status: Some(if cancelled {
                    AiMessageStatus::Cancelled
                } else {
                    AiMessageStatus::Complete
                }),
                created_at: Some((self.clock.now_iso)()),
                // `memoryUsed ?? undefined`.
                memory_used: memory_used.clone(),
            });
            session.is_streaming = false;
            session.stream_content = String::new();
            session.stream_reasoning = String::new();
            session.stream_mcp_tool_name = None;
            session.egress_summary = egress_summary;
            session.stream_memory_used = None;
            session.memory_used = memory_used;
            session.request_id = None;
            match target {
                Target::Chat => {
                    store.set_chat_session(session.clone());
                    self.persist_stream_session(store, &session);
                }
                Target::Workflow => store.set_workflow_chat(session),
            }
        } else {
            session.stream_content = new_content;
            session.stream_reasoning = new_reasoning;
            session.stream_mcp_tool_name = mcp_tool_name;
            session.egress_summary = egress_summary;
            session.stream_memory_used = memory_used.clone();
            session.memory_used = memory_used;
            match target {
                Target::Chat => store.set_chat_session(session),
                Target::Workflow => store.set_workflow_chat(session),
            }
        }
    }

    /// React `handleStreamError(get, set, payload)` — out-of-band stream
    /// error: terminal once, idempotent on repeats.
    pub fn handle_stream_error(
        &mut self,
        store: &mut dyn AiStoreView,
        payload: &StreamErrorPayload,
    ) {
        let nl2sql = store.nl2sql();
        let chat = store.chat_session();
        let workflow = store.workflow_chat();
        // The nl2sql arm deliberately does NOT return: React falls through
        // to the chat/workflow resolution on the same payload.
        if nl2sql.request_id.as_deref() == Some(payload.request_id.as_str()) {
            self.unregister_request_id(&payload.request_id);
            let mut next = nl2sql.clone();
            next.is_generating = false;
            store.set_nl2sql_error(next, payload.error.clone());
        }

        let Some(target) = route_target(&chat, &workflow, &payload.request_id) else {
            return;
        };

        // Terminal once — the registry gate settles the run and drops
        // repeats (and once the session's requestId clears, the routing
        // itself drops them).
        if self.registry.has(&payload.request_id) {
            let applied = self.registry.apply_error(&payload.request_id);
            if !applied.accepted {
                return;
            }
        }

        self.unregister_request_id(&payload.request_id);
        // React's `(...)!` — see handle_stream_chunk's twin comment.
        let Some(mut session) = (match target {
            Target::Chat => chat,
            Target::Workflow => workflow,
        }) else {
            return;
        };
        session.is_streaming = false;
        session.stream_content = String::new();
        session.stream_reasoning = String::new();
        session.stream_mcp_tool_name = None;
        session.stream_memory_used = None;
        session.memory_used = None;
        session.request_id = None;
        session.messages.push(AiChatMessage {
            id: Some((self.new_id)()),
            role: AiMessageRole::Assistant,
            // `Error: ${payload.error}` — the failed marker carries neither
            // reasoning, questions, tool calls, cancellation nor memory.
            content: format!("Error: {}", payload.error),
            reasoning: None,
            questions: None,
            tool_calls: None,
            tool_call_id: None,
            cancelled: None,
            status: Some(AiMessageStatus::Failed),
            created_at: Some((self.clock.now_iso)()),
            memory_used: None,
        });
        match target {
            Target::Chat => {
                store.set_chat_session(session.clone());
                self.persist_stream_session(store, &session);
            }
            Target::Workflow => store.set_workflow_chat(session),
        }
    }

    /// React `finalizeCancelledSession` (module-private): the cancelled-run
    /// marker.
    ///
    /// The marker is the point — even a run cancelled before its first
    /// token gets an assistant message, so the UI can say "Generation
    /// stopped." instead of silently returning to the input with no trace
    /// of the cancelled run.
    fn finalize_cancelled_session(&mut self, session: &AiChatSession) -> AiChatSession {
        // `memoryUsed: session.streamMemoryUsed ?? null` — the in-flight
        // memory references become the session's latest-run record.
        let memory_used = session.stream_memory_used.clone();
        let mut next = session.clone();
        next.is_streaming = false;
        next.stream_content = String::new();
        next.stream_reasoning = String::new();
        next.stream_mcp_tool_name = None;
        next.request_id = None;
        next.stream_memory_used = None;
        next.memory_used = memory_used.clone();
        next.messages.push(AiChatMessage {
            id: Some((self.new_id)()),
            role: AiMessageRole::Assistant,
            // `session.streamContent || ''` — the string field folds to
            // itself; the empty stream still gets the marker message.
            content: session.stream_content.clone(),
            // `session.streamReasoning || undefined`.
            reasoning: (!session.stream_reasoning.is_empty())
                .then(|| session.stream_reasoning.clone()),
            questions: None,
            tool_calls: None,
            tool_call_id: None,
            cancelled: Some(true),
            status: Some(AiMessageStatus::Cancelled),
            created_at: Some((self.clock.now_iso)()),
            // `session.streamMemoryUsed ?? undefined`.
            memory_used,
        });
        next
    }

    /// React `persistStreamSession` (module-private): the visible-session
    /// guard, the already-ported `persistChatSession`, and the
    /// `sessionMigrationError` report.
    ///
    /// React chains the persist through a promise and reports in a
    /// `.then`; the sync port (the T040 precedent) calls it inline and
    /// reports inline — the guard still re-reads the store *after* the
    /// terminal set, exactly where React re-reads it.
    fn persist_stream_session(&mut self, store: &mut dyn AiStoreView, session: &AiChatSession) {
        // `get().chatSession?.id !== session.id` — skip when the visible
        // session is no longer this one.
        match store.chat_session() {
            Some(visible) if visible.id == session.id => {}
            _ => return,
        }
        let result = self.persister.persist_chat_session(session);
        if !result.ok {
            // `result.error ?? 'AI session backend write failed'`.
            store.set_session_migration_error(
                result.error.unwrap_or_else(|| PERSIST_ERROR_FALLBACK.to_owned()),
            );
        }
    }
}
