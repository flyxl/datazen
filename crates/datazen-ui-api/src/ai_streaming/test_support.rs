//! Shared test fixtures for the ai_streaming module (test-only).
//!
//! Mirrors the React suites' harnesses: [`ScriptedRegistry`] doubles the
//! `StreamRegistry` to the slice the adapter's gating exercises, seeded by
//! hand the way the send path's `begin()` would; [`ScriptedCanceller`] and
//! [`ScriptedCancelCommand`] double the cancel path ([`WiredCanceller`] is
//! the composition witness for the wiring documented on
//! [`SessionCancellerOps`](super::seams::SessionCancellerOps));
//! [`ScriptedStore`] is the zustand store double; [`ScriptedPersister`]
//! doubles the already-ported `persistChatSession`. [`TestEnv`] wires an
//! [`AiStreaming`] with a deterministic clock and a `gen-N` id counter.

#![allow(dead_code)]

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use crate::ai_sessions::{NewId, SessionPersistResult};
use crate::ai_types::{
    AiChatAttribution, AiChatSession, AiMemoryReference, AiQuestion, AiQuestionOption,
    AiToolCall, StreamChunkPayload, StreamErrorPayload,
};

use super::adapter::AiStreaming;
use super::seams::{
    AiCancelCommand, AiStoreView, AiTargetClaim, ApplyChunkGate, ApplyGateResult, CancelReceipt,
    ChatSessionPersister, ExtractionSeams, ExtractQuestionsOutcome, Nl2sqlStreamingSlice,
    SessionCancellerOps, StreamRegistryOps, StreamingClock,
};

/// The fixed `now` the test clock reports.
pub const NOW_ISO: &str = "2026-02-02T00:00:00.000Z";

/// The marker the questions stub strips from streamed content (the
/// `<ask_questions>` block's stand-in).
pub const RAW_MARKER: &str = "<raw>";

// ── The registry double ──────────────────────────────────────────────────

/// The slice of the React `StreamRegistry` state machine the adapter's
/// gating exercises: `has` → `applyChunk`/`applyError` → `accepted`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GatePhase {
    Starting,
    Streaming,
    Cancelling,
    Terminal,
}

/// The registry double: per-request phases plus call logs. Seeded by hand
/// the way the send path's `registry.begin(requestId)` would.
#[derive(Default)]
pub struct ScriptedRegistry {
    entries: RefCell<HashMap<String, GatePhase>>,
    chunk_calls: RefCell<Vec<(String, ApplyChunkGate)>>,
    error_calls: RefCell<Vec<String>>,
}

impl ScriptedRegistry {
    /// The send path's `registry.begin(requestId)`.
    pub fn begin(&self, request_id: &str) {
        self.entries
            .borrow_mut()
            .insert(request_id.to_owned(), GatePhase::Starting);
    }

    /// The canceller's `registry.markCancelling(requestId)`.
    pub fn mark_cancelling(&self, request_id: &str) {
        self.entries
            .borrow_mut()
            .insert(request_id.to_owned(), GatePhase::Cancelling);
    }

    /// The canceller's forced `registry.settle(requestId, …)`.
    pub fn force_terminal(&self, request_id: &str) {
        self.entries
            .borrow_mut()
            .insert(request_id.to_owned(), GatePhase::Terminal);
    }

    pub fn phase(&self, request_id: &str) -> Option<GatePhase> {
        self.entries.borrow().get(request_id).copied()
    }

    pub fn chunk_calls(&self) -> Vec<(String, ApplyChunkGate)> {
        self.chunk_calls.borrow().clone()
    }

    pub fn error_calls(&self) -> Vec<String> {
        self.error_calls.borrow().clone()
    }

    /// The gate the adapter's `applyChunk` consults — mirrors the real
    /// machine's arms: unknown-request, terminal (duplicate terminal
    /// chunk), and cancel-issued (late non-terminal chunk) drops.
    fn apply_chunk_inner(&self, request_id: &str, chunk: &ApplyChunkGate) -> ApplyGateResult {
        self.chunk_calls
            .borrow_mut()
            .push((request_id.to_owned(), *chunk));
        let mut entries = self.entries.borrow_mut();
        match entries.get_mut(request_id) {
            None => ApplyGateResult { accepted: false },
            Some(phase) => {
                let current = *phase;
                match current {
                    GatePhase::Terminal => ApplyGateResult { accepted: false },
                    GatePhase::Cancelling if !chunk.done => ApplyGateResult { accepted: false },
                    GatePhase::Cancelling => {
                        *phase = GatePhase::Terminal;
                        ApplyGateResult { accepted: true }
                    }
                    _ => {
                        *phase = if chunk.done {
                            GatePhase::Terminal
                        } else {
                            GatePhase::Streaming
                        };
                        ApplyGateResult { accepted: true }
                    }
                }
            }
        }
    }

    /// The gate the adapter's `applyError` consults (idempotent terminal).
    fn apply_error_inner(&self, request_id: &str) -> ApplyGateResult {
        self.error_calls.borrow_mut().push(request_id.to_owned());
        let mut entries = self.entries.borrow_mut();
        match entries.get_mut(request_id) {
            None => ApplyGateResult { accepted: false },
            Some(phase) => {
                if *phase == GatePhase::Terminal {
                    ApplyGateResult { accepted: false }
                } else {
                    *phase = GatePhase::Terminal;
                    ApplyGateResult { accepted: true }
                }
            }
        }
    }
}

/// Rc-sharing adapter so tests keep inspecting the double the adapter owns.
pub struct SharedRegistry(pub Rc<ScriptedRegistry>);

impl StreamRegistryOps for SharedRegistry {
    fn has(&self, request_id: &str) -> bool {
        self.0.entries.borrow().contains_key(request_id)
    }

    fn apply_chunk(&mut self, request_id: &str, chunk: &ApplyChunkGate) -> ApplyGateResult {
        self.0.apply_chunk_inner(request_id, chunk)
    }

    fn apply_error(&mut self, request_id: &str) -> ApplyGateResult {
        self.0.apply_error_inner(request_id)
    }
}

// ── The cancel-path doubles ──────────────────────────────────────────────

/// The canceller double: records `cancel` calls and returns the programmed
/// receipt (the real orchestrator is the ai/sessionCancel port task).
#[derive(Default)]
pub struct ScriptedCanceller {
    calls: RefCell<Vec<String>>,
    receipt: RefCell<Option<CancelReceipt>>,
}

impl ScriptedCanceller {
    pub fn program_receipt(&self, receipt: CancelReceipt) {
        *self.receipt.borrow_mut() = Some(receipt);
    }

    pub fn program_none(&self) {
        *self.receipt.borrow_mut() = None;
    }

    pub fn calls(&self) -> Vec<String> {
        self.calls.borrow().clone()
    }

    fn cancel_inner(&self, request_id: &str) -> Option<CancelReceipt> {
        self.calls.borrow_mut().push(request_id.to_owned());
        self.receipt.borrow().clone()
    }
}

/// Rc-sharing adapter so tests keep inspecting the double the adapter owns.
pub struct SharedCanceller(pub Rc<ScriptedCanceller>);

impl SessionCancellerOps for SharedCanceller {
    fn cancel(&mut self, request_id: &str) -> Option<CancelReceipt> {
        self.0.cancel_inner(request_id)
    }
}

/// The `aiCommands.cancel` double — the seam the future SessionCanceller
/// adapter will sit behind.
pub struct ScriptedCancelCommand {
    calls: RefCell<Vec<String>>,
    result: RefCell<Result<CancelReceipt, String>>,
}

impl Default for ScriptedCancelCommand {
    fn default() -> Self {
        // React's un-stubbed double rejects; nothing in the suite calls
        // cancel without programming it first.
        Self {
            calls: RefCell::default(),
            result: RefCell::new(Err("not programmed".to_owned())),
        }
    }
}

impl ScriptedCancelCommand {
    pub fn program_receipt(&self, receipt: CancelReceipt) {
        *self.result.borrow_mut() = Ok(receipt);
    }

    pub fn program_error(&self, message: &str) {
        *self.result.borrow_mut() = Err(message.to_owned());
    }

    pub fn calls(&self) -> Vec<String> {
        self.calls.borrow().clone()
    }
}

impl AiCancelCommand for ScriptedCancelCommand {
    fn cancel(&self, request_id: &str) -> Result<CancelReceipt, String> {
        self.calls.borrow_mut().push(request_id.to_owned());
        self.result.borrow().clone()
    }
}

/// The composition witness for the wiring documented on
/// [`SessionCancellerOps`](super::seams::SessionCancellerOps): a canceller
/// adapter built over the cancel command — the `Err` arm folds to `None`
/// exactly where React's SessionCanceller leaves the receipt null (no
/// proof the host observed the cancel, so the run keeps streaming).
pub struct WiredCanceller {
    pub command: Rc<ScriptedCancelCommand>,
}

impl SessionCancellerOps for WiredCanceller {
    fn cancel(&mut self, request_id: &str) -> Option<CancelReceipt> {
        self.command.cancel(request_id).ok()
    }
}

// ── The store double ─────────────────────────────────────────────────────

/// The zustand store double: the three reads plus the four write shapes,
/// with a log of which setters ran (in call order).
#[derive(Default)]
pub struct ScriptedStore {
    chat: RefCell<Option<AiChatSession>>,
    workflow: RefCell<Option<AiChatSession>>,
    nl2sql_state: RefCell<Nl2sqlStreamingSlice>,
    nl2sql_error: RefCell<Option<String>>,
    session_migration_error: RefCell<Option<String>>,
    sets: RefCell<Vec<&'static str>>,
}

impl ScriptedStore {
    /// Seeds the visible chat session (no set is logged — seeding is the
    /// test's, not the adapter's).
    pub fn seed_chat(&self, session: AiChatSession) {
        *self.chat.borrow_mut() = Some(session);
    }

    pub fn seed_workflow(&self, session: AiChatSession) {
        *self.workflow.borrow_mut() = Some(session);
    }

    pub fn seed_nl2sql(&self, slice: Nl2sqlStreamingSlice) {
        *self.nl2sql_state.borrow_mut() = slice;
    }

    pub fn chat(&self) -> Option<AiChatSession> {
        self.chat.borrow().clone()
    }

    pub fn workflow(&self) -> Option<AiChatSession> {
        self.workflow.borrow().clone()
    }

    pub fn nl2sql_state(&self) -> Nl2sqlStreamingSlice {
        self.nl2sql_state.borrow().clone()
    }

    pub fn nl2sql_error(&self) -> Option<String> {
        self.nl2sql_error.borrow().clone()
    }

    pub fn session_migration_error(&self) -> Option<String> {
        self.session_migration_error.borrow().clone()
    }

    /// The setters that ran, in call order (`chat_session`,
    /// `workflow_chat`, `nl2sql`, `nl2sql_error`,
    /// `session_migration_error`).
    pub fn sets(&self) -> Vec<&'static str> {
        self.sets.borrow().clone()
    }
}

/// Rc-sharing adapter so the adapter's `&mut dyn AiStoreView` calls land
/// on the shared double while the test keeps reading it.
pub struct SharedStore(pub Rc<ScriptedStore>);

impl AiStoreView for SharedStore {
    fn chat_session(&self) -> Option<AiChatSession> {
        self.0.chat()
    }

    fn workflow_chat(&self) -> Option<AiChatSession> {
        self.0.workflow()
    }

    fn nl2sql(&self) -> Nl2sqlStreamingSlice {
        self.0.nl2sql_state()
    }

    fn set_chat_session(&mut self, session: AiChatSession) {
        self.0.sets.borrow_mut().push("chat_session");
        *self.0.chat.borrow_mut() = Some(session);
    }

    fn set_workflow_chat(&mut self, session: AiChatSession) {
        self.0.sets.borrow_mut().push("workflow_chat");
        *self.0.workflow.borrow_mut() = Some(session);
    }

    fn set_nl2sql(&mut self, nl2sql: Nl2sqlStreamingSlice) {
        self.0.sets.borrow_mut().push("nl2sql");
        *self.0.nl2sql_state.borrow_mut() = nl2sql;
    }

    fn set_nl2sql_error(&mut self, nl2sql: Nl2sqlStreamingSlice, error: String) {
        self.0.sets.borrow_mut().push("nl2sql_error");
        *self.0.nl2sql_state.borrow_mut() = nl2sql;
        *self.0.nl2sql_error.borrow_mut() = Some(error);
    }

    fn set_session_migration_error(&mut self, error: String) {
        self.0.sets.borrow_mut().push("session_migration_error");
        *self.0.session_migration_error.borrow_mut() = Some(error);
    }
}

// ── The persist double ───────────────────────────────────────────────────

/// The `persistChatSession` double: records the sessions it is asked to
/// persist and returns the programmed result. Defaults to a successful
/// write (the suites' `mockResolvedValue` posture) so unprogrammed paths
/// report no migration error.
pub struct ScriptedPersister {
    calls: RefCell<Vec<AiChatSession>>,
    result: RefCell<SessionPersistResult>,
}

impl Default for ScriptedPersister {
    fn default() -> Self {
        Self {
            calls: RefCell::default(),
            result: RefCell::new(SessionPersistResult { ok: true, error: None, detail: None }),
        }
    }
}

impl ScriptedPersister {
    pub fn program_result(&self, result: SessionPersistResult) {
        *self.result.borrow_mut() = result;
    }

    pub fn calls(&self) -> Vec<AiChatSession> {
        self.calls.borrow().clone()
    }

    fn persist_inner(&self, session: &AiChatSession) -> SessionPersistResult {
        self.calls.borrow_mut().push(session.clone());
        self.result.borrow().clone()
    }
}

/// Rc-sharing adapter so tests keep inspecting the double the adapter owns.
pub struct SharedPersister(pub Rc<ScriptedPersister>);

impl ChatSessionPersister for SharedPersister {
    fn persist_chat_session(&mut self, session: &AiChatSession) -> SessionPersistResult {
        self.0.persist_inner(session)
    }
}

// ── The extraction / attribution stubs ───────────────────────────────────

fn xml_question() -> AiQuestion {
    AiQuestion {
        id: "q-xml".to_owned(),
        prompt: "Which database?".to_owned(),
        options: vec![AiQuestionOption { id: "o1".to_owned(), label: "shop".to_owned() }],
        allow_multiple: None,
    }
}

fn tool_question() -> AiQuestion {
    AiQuestion {
        id: "q-tool".to_owned(),
        prompt: "Which scope?".to_owned(),
        options: Vec::new(),
        allow_multiple: None,
    }
}

/// The questions stub: strips the [`RAW_MARKER`] block from the content and
/// yields one question when the text carries `ASK` anywhere (the
/// `<ask_questions>` marker's stand-in).
fn stub_extract_questions(content: &str) -> ExtractQuestionsOutcome {
    ExtractQuestionsOutcome {
        clean_content: content.replace(RAW_MARKER, ""),
        questions: if content.contains("ASK") {
            vec![xml_question()]
        } else {
            Vec::new()
        },
    }
}

/// The tool-call questions stub: only the `ask_questions` tool call carries
/// questions.
fn stub_parse_tool_call_questions(tool_calls: &[AiToolCall]) -> Vec<AiQuestion> {
    if tool_calls.iter().any(|call| call.name == "ask_questions") {
        vec![tool_question()]
    } else {
        Vec::new()
    }
}

/// The final-SQL stub (`extractSqlFromResponse`'s stand-in).
fn stub_extract_sql_from_response(raw: &str) -> String {
    format!("final:{raw}")
}

/// The streaming-preview stub (`extractSqlStreaming`'s stand-in).
fn stub_extract_sql_streaming(raw: &str) -> String {
    format!("preview:{raw}")
}

/// The attribution stub implements the real React projection
/// (`attributionOf`: the editor-grant arm keeps grantId + editorSessionId
/// and drops the client instance id; everything else reads independent),
/// so the forwarding tests can pin both arms.
fn stub_attribution_of(claim: Option<&AiTargetClaim>) -> AiChatAttribution {
    match claim {
        Some(AiTargetClaim::EditorGrant { grant_id, editor_session_id, .. }) => {
            AiChatAttribution::EditorGrant {
                grant_id: grant_id.clone(),
                editor_session_id: editor_session_id.clone(),
            }
        }
        _ => AiChatAttribution::Independent,
    }
}

// ── Fixtures ─────────────────────────────────────────────────────────────

/// One memory reference fixture (`M<reference>`).
pub fn fixture_memory(reference: &str) -> AiMemoryReference {
    AiMemoryReference {
        reference: reference.to_owned(),
        id: format!("mem-{reference}"),
        subject: format!("subject {reference}"),
        kind: "preference".to_owned(),
        scope: "user".to_owned(),
        score: 0.5,
    }
}

/// One tool-call fixture.
pub fn fixture_tool_call(name: &str) -> AiToolCall {
    AiToolCall {
        id: format!("tc-{name}"),
        name: name.to_owned(),
        arguments: "{}".to_owned(),
    }
}

/// A chat session mid-run: request `request_id`, streaming, empty messages.
pub fn fixture_session(id: &str, request_id: &str) -> AiChatSession {
    AiChatSession {
        id: id.to_owned(),
        session_key: format!("{id}::key"),
        request_id: Some(request_id.to_owned()),
        is_streaming: true,
        ..Default::default()
    }
}

/// A chunk payload with the routing fields set and the rest defaulted.
pub fn chunk(request_id: &str, content: &str) -> StreamChunkPayload {
    StreamChunkPayload {
        request_id: request_id.to_owned(),
        content: content.to_owned(),
        reasoning: None,
        done: false,
        cancelled: None,
        usage: None,
        tool_calls: None,
        egress_summary: None,
        memory_used: None,
    }
}

/// An error payload with the routing fields set.
pub fn error_payload(request_id: &str, error: &str) -> StreamErrorPayload {
    StreamErrorPayload {
        request_id: request_id.to_owned(),
        error: error.to_owned(),
    }
}

// ── The env ──────────────────────────────────────────────────────────────

/// An [`AiStreaming`] wired to the doubles: fixed now ([`NOW_ISO`]), a
/// `gen-N` id counter, the stub extractions and the stub attribution
/// projection.
pub struct TestEnv {
    pub streaming: AiStreaming,
    pub store: Rc<ScriptedStore>,
    pub registry: Rc<ScriptedRegistry>,
    pub canceller: Rc<ScriptedCanceller>,
    pub persister: Rc<ScriptedPersister>,
    pub id_counter: Rc<Cell<u32>>,
}

/// Builds a fresh [`TestEnv`].
pub fn env() -> TestEnv {
    let registry = Rc::new(ScriptedRegistry::default());
    let canceller = Rc::new(ScriptedCanceller::default());
    let store = Rc::new(ScriptedStore::default());
    let persister = Rc::new(ScriptedPersister::default());
    let id_counter = Rc::new(Cell::new(0u32));
    let new_id: NewId = Box::new({
        let id_counter = id_counter.clone();
        move || {
            let next = id_counter.get() + 1;
            id_counter.set(next);
            format!("gen-{next}")
        }
    });
    let streaming = AiStreaming::new(
        Box::new(SharedRegistry(registry.clone())),
        Box::new(SharedCanceller(canceller.clone())),
        Box::new(stub_attribution_of),
        ExtractionSeams {
            extract_questions: Box::new(stub_extract_questions),
            parse_tool_call_questions: Box::new(stub_parse_tool_call_questions),
            extract_sql_from_response: Box::new(stub_extract_sql_from_response),
            extract_sql_streaming: Box::new(stub_extract_sql_streaming),
        },
        StreamingClock {
            now_iso: Box::new(|| NOW_ISO.to_owned()),
        },
        new_id,
        Box::new(SharedPersister(persister.clone())),
    );
    TestEnv {
        streaming,
        store,
        registry,
        canceller,
        persister,
        id_counter,
    }
}
