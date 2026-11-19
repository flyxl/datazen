//! State machine ported from
//! `packages/frontend/src/lib/session/ExecutionProjection.ts`.
//!
//! # Immediate-mode adaptation (honest, no fake async)
//!
//! Rune has no promises and no `await`. Every injected async client call the
//! React class makes (`getExecution`, `readArtifact`, `subscribeEvents` and the
//! async iterator's `next`/`return`) becomes an emitted [`ProjectionEffect`];
//! the host executes it and feeds the outcome back through the `on_*` methods.
//! Nothing here blocks, spawns a thread or pretends a `Future` exists — exactly
//! the shape already used by `crate::session::editor_session_controller`.
//!
//! # What is ported faithfully
//!
//! * `sequence` dedupe / gap recovery with arbitrary-precision counters (the
//!   React `bigint` is a canonical decimal string here, via `counters.rs`).
//! * `streamResetRequired` clears the cursor *before* dedupe and rebuilds.
//! * `published()` UTF-8/JSON payload decoding and embedded `source` adoption
//!   (execution id + numeric statement index + writable mapping in
//!   `['verified','readOnly']`).
//! * `read()` identity check + key/sequence dedupe; a bad chunk never poisons a
//!   previously published prefix.
//! * `recover()` metadata read (`totalChunks ?? publishedChunkCount`) and the
//!   sequential `1..count` fan-out.
//! * `writableBinding` / `invalidateRuntimeBinding` epoch + execution identity.
//! * `consume()` generation guards, terminal-state (`succeeded`/`failed`/
//!   `cancelled`) resubscribe gate and error projection.
//!
//! # DTO provenance / removal stage
//!
//! `ExecutionReceipt`, `ExecutionState`, `ExecutionErrorCode`, `SessionHandle`,
//! `SessionView`, `ExecutionTarget` and `NamespaceTarget` are reused from T083
//! (`super::editor_session_controller`) — they are *not* duplicated. The view /
//! artifact / event DTOs the controller never needed are mirrored minimally
//! below. Consumer: the query-panel result view (not landed). Remove these
//! mirrors when that view wires the projection into `AppState` and exposes the
//! backend-client DTOs, then import them from `datazen-ui-api` instead.

#[path = "execution_projection_types.rs"]
pub(crate) mod types;

pub(crate) use types::*;

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet, VecDeque};

use serde_json::Value;

use super::counters::same_handle;

type Listener = Box<dyn FnMut()>;

#[derive(Clone, Debug)]
struct Subscription {
    call_id: u64,
    generation: u64,
}

/// What to do once an in-flight `recover()` finishes.
#[derive(Clone, Debug)]
enum Resume {
    None,
    /// `accept` hit a gap: process the original event, then advance its cursor.
    Accept(EventEnvelope),
    /// The stream iterator ended; resubscribe only for a non-terminal view.
    ConsumeDone { generation: u64 },
    /// `streamResetRequired` arrived inside `consume`; resubscribe from zero.
    Resubscribe { generation: u64 },
    /// `reconnect`: recover, then subscribe.
    ThenConsume,
}

/// What a resolved `readArtifact` continues with.
#[derive(Clone, Debug)]
enum ReadKind {
    /// `read()` from a `resultChunk` event; advance `sequence` on success.
    Accept {
        artifact_id: String,
        index: String,
        source: Option<StatementResultSource>,
        sequence: String,
    },
    /// Sequential chunk read inside `recover()`.
    Recover {
        artifact_id: String,
        index: String,
        total: String,
        metadata: bool,
        queue: VecDeque<String>,
        resume: Resume,
    },
}

#[derive(Clone, Debug)]
enum Pending {
    Execution { call_id: u64, resume: Resume },
    Read { call_id: u64, key: String, kind: ReadKind },
}

impl Pending {
    // consumer: the query-panel result view that resolves in-flight effects;
    // remove when that view lands and drives the port directly.
    #[allow(dead_code)]
    fn call_id(&self) -> u64 {
        match self {
            Self::Execution { call_id, .. } | Self::Read { call_id, .. } => *call_id,
        }
    }
}

enum PayloadOutcome {
    Advance,
    /// A read is in flight; `on_read_artifact` advances the cursor.
    DeferRead,
    /// React's early `return`: do not advance the cursor.
    Stop,
}
/// `ExecutionProjection` — one instance per execution receipt.
pub(crate) struct ExecutionProjection {
    backend_id: String,
    receipt: ExecutionReceipt,
    original_handle: Option<SessionHandle>,
    sequence: Option<String>,
    runtime_invalidated: bool,
    generation: u64,
    view: Option<ExecutionView>,
    error: Option<ProjectionError>,
    chunks_by_key: HashMap<String, PublishedResultChunk>,
    reads_in_flight: HashSet<String>,
    listeners: Vec<Option<Listener>>,
    publish_count: u64,
    next_call_id: u64,
    pending: Option<Pending>,
    subscription: Option<Subscription>,
    on_session: Option<Box<dyn FnMut(SessionView)>>,
    read_phases: Vec<ReadPhase>,
}

#[allow(dead_code)] // host-driven port; the query-panel result view is not landed yet.
impl ExecutionProjection {
    pub(crate) fn new(
        backend_id: String,
        receipt: ExecutionReceipt,
        original_handle: Option<SessionHandle>,
    ) -> Self {
        Self {
            backend_id,
            receipt,
            original_handle,
            sequence: None,
            runtime_invalidated: false,
            generation: 0,
            view: None,
            error: None,
            chunks_by_key: HashMap::new(),
            reads_in_flight: HashSet::new(),
            listeners: Vec::new(),
            publish_count: 0,
            next_call_id: 0,
            pending: None,
            subscription: None,
            on_session: None,
            read_phases: Vec::new(),
        }
    }

    /// Install the `onSession` callback (the event-payload variant, mirrored as
    /// the session itself).
    #[allow(dead_code)] // host observer; remove with the result view.
    pub(crate) fn set_on_session(&mut self, callback: Box<dyn FnMut(SessionView)>) {
        self.on_session = Some(callback);
    }

    /// Subscribe to publish notifications; the returned id can be released.
    pub(crate) fn subscribe(&mut self, listener: Listener) -> usize {
        let id = self.listeners.len();
        self.listeners.push(Some(listener));
        id
    }

    #[allow(dead_code)] // symmetric with subscribe; remove with the result view.
    pub(crate) fn unsubscribe_listener(&mut self, id: usize) {
        if let Some(slot) = self.listeners.get_mut(id) {
            *slot = None;
        }
    }

    fn publish(&mut self) {
        self.publish_count += 1;
        for listener in self.listeners.iter_mut().flatten() {
            listener();
        }
    }

    fn set_error(&mut self, error: ProjectionError) {
        self.error = Some(error);
        self.publish();
    }

    fn alloc_call(&mut self) -> u64 {
        self.next_call_id += 1;
        self.next_call_id
    }

    fn record_phase(&mut self, phase: ReadPhase) {
        self.read_phases.push(phase);
    }

    // --- read-only surface ---------------------------------------------------

    pub(crate) fn view(&self) -> Option<&ExecutionView> {
        self.view.as_ref()
    }

    pub(crate) fn error(&self) -> Option<&ProjectionError> {
        self.error.as_ref()
    }

    pub(crate) fn sequence(&self) -> Option<&str> {
        self.sequence.as_deref()
    }

    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    pub(crate) fn publish_count(&self) -> u64 {
        self.publish_count
    }

    pub(crate) fn read_phases(&self) -> &[ReadPhase] {
        &self.read_phases
    }

    /// `chunks` getter: sorted by `artifactId.localeCompare`, then the numeric
    /// chunk index. Rust's `Ord` on ASCII ids equals JS `localeCompare` for the
    /// wire ids used here.
    pub(crate) fn chunks(&self) -> Vec<&PublishedResultChunk> {
        let mut chunks: Vec<&PublishedResultChunk> = self.chunks_by_key.values().collect();
        chunks.sort_by(|a, b| {
            a.chunk
                .artifact_id
                .cmp(&b.chunk.artifact_id)
                .then_with(|| {
                    compare(&a.chunk.chunk_index, &b.chunk.chunk_index).unwrap_or(Ordering::Equal)
                })
        });
        chunks
    }

    /// React `key()`: `JSON.stringify([backendId, executionId, artifactId,
    /// index.toString()])` — built with serde_json so escaping stays identical.
    fn key(&self, artifact_id: &str, index: &str) -> String {
        serde_json::to_string(&serde_json::json!([
            self.backend_id.as_str(),
            self.receipt.execution_id.as_str(),
            artifact_id,
            index,
        ]))
        .unwrap_or_default()
    }

    /// `writableBinding(currentHandle)`: the published binding is writable only
    /// when its execution id and *both* session handles (original and current)
    /// agree — an epoch change kills it.
    pub(crate) fn writable_binding(
        &self,
        current_handle: Option<&SessionHandle>,
    ) -> Option<&RuntimeResultBinding> {
        let binding = self.view.as_ref()?.runtime_binding.as_ref()?;
        if binding.execution_id != self.receipt.execution_id {
            return None;
        }
        if !same_handle(Some(&binding.handle), self.original_handle.as_ref()) {
            return None;
        }
        if !same_handle(Some(&binding.handle), current_handle) {
            return None;
        }
        Some(binding)
    }

    /// `invalidateRuntimeBinding()`: permanently drop the view's runtime
    /// binding; later `recover()`/`executionChanged` feeds stay nulled.
    pub(crate) fn invalidate_runtime_binding(&mut self) {
        self.runtime_invalidated = true;
        if let Some(view) = self.view.as_mut() {
            view.runtime_binding = None;
        }
        self.publish();
    }

    // --- published() decoding ------------------------------------------------

    fn published(&self, chunk: &ArtifactChunk, source: Option<StatementResultSource>) -> PublishedResultChunk {
        // `new TextDecoder().decode(bytes)` then `JSON.parse`; a failure leaves
        // the payload opaque (null) — binary artifacts stay opaque.
        let payload = std::str::from_utf8(&chunk.bytes)
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(text).ok())
            .unwrap_or(Value::Null);
        let source = if source.is_some() { source } else { self.embedded_source(&payload) };
        PublishedResultChunk { chunk: chunk.clone(), source, payload }
    }

    /// React's `source` adoption branch: object payload with a `source` object
    /// carrying our execution id, a numeric `statementIndex` and a writable
    /// mapping of `verified` / `readOnly`.
    fn embedded_source(&self, payload: &Value) -> Option<StatementResultSource> {
        let candidate = payload.as_object()?.get("source")?.as_object()?;
        let execution_id = candidate.get("executionId")?.as_str()?;
        if execution_id != self.receipt.execution_id {
            return None;
        }
        let statement_index = candidate.get("statementIndex")?.as_number()?.clone();
        let writable_mapping = match candidate.get("writableMapping")?.as_str()? {
            "verified" => WritableMapping::Verified,
            "readOnly" => WritableMapping::ReadOnly,
            _ => return None,
        };
        Some(StatementResultSource { execution_id: execution_id.to_owned(), statement_index, writable_mapping })
    }

    // --- read() / recover() --------------------------------------------------

    /// Emit a `ReadArtifact` effect, honouring the React key/in-flight dedupe
    /// (unless `force` is set for a recovery metadata read). Returns whether a
    /// read was actually emitted.
    #[allow(clippy::too_many_arguments)]
    fn emit_read(
        &mut self,
        artifact_id: &str,
        index: &str,
        source: Option<StatementResultSource>,
        kind: ReadKind,
        force: bool,
        effects: &mut Vec<ProjectionEffect>,
    ) -> bool {
        self.record_phase(ReadPhase::ArtifactRead);
        let key = self.key(artifact_id, index);
        if !force && (self.chunks_by_key.contains_key(&key) || self.reads_in_flight.contains(&key)) {
            return false;
        }
        let call_id = self.alloc_call();
        self.reads_in_flight.insert(key.clone());
        self.pending = Some(Pending::Read { call_id, key, kind });
        effects.push(ProjectionEffect::ReadArtifact {
            call_id,
            artifact_id: artifact_id.to_owned(),
            // `index` is already a canonical decimal string, which is exactly
            // the wire form `wire_counter` would render from a `BigCounter`.
            chunk_index: index.to_owned(),
        });
        true
    }

    fn chunk_matches(&self, chunk: &ArtifactChunk, artifact_id: &str, index: &str) -> bool {
        if chunk.artifact_id != artifact_id {
            return false;
        }
        match canonical(&chunk.chunk_index) {
            Ok(actual) => actual == index,
            Err(_) => false,
        }
    }

    fn recover_with(&mut self, resume: Resume, effects: &mut Vec<ProjectionEffect>) {
        self.record_phase(ReadPhase::ExecutionView);
        let call_id = self.alloc_call();
        self.pending = Some(Pending::Execution { call_id, resume });
        effects.push(ProjectionEffect::GetExecution {
            call_id,
            execution_id: self.receipt.execution_id.clone(),
        });
    }

    pub(crate) fn recover(&mut self) -> Vec<ProjectionEffect> {
        let mut effects = Vec::new();
        self.recover_with(Resume::None, &mut effects);
        effects
    }

    /// Begin (or continue) the per-artifact chunk loop. `queue` holds artifacts
    /// whose chunk 0 has not been fetched for metadata yet.
    fn continue_recover_artifacts(
        &mut self,
        mut queue: VecDeque<String>,
        resume: Resume,
        effects: &mut Vec<ProjectionEffect>,
    ) {
        let Some(artifact_id) = queue.pop_front() else {
            self.finish_recover(resume, effects);
            return;
        };
        let kind = ReadKind::Recover {
            artifact_id: artifact_id.clone(),
            index: "0".to_owned(),
            total: "0".to_owned(),
            metadata: true,
            queue,
            resume,
        };
        // Recovery always re-reads chunk 0 for fresh metadata even when it was
        // displayed before a disconnect — so dedupe is forced off here.
        self.emit_read(&artifact_id, "0", None, kind, true, effects);
    }

    /// Emit the next `1..count` recovery read, skipping chunks already cached.
    fn emit_recover_read(
        &mut self,
        artifact_id: String,
        mut index: String,
        total: String,
        queue: VecDeque<String>,
        resume: Resume,
        effects: &mut Vec<ProjectionEffect>,
    ) {
        loop {
            let kind = ReadKind::Recover {
                artifact_id: artifact_id.clone(),
                index: index.clone(),
                total: total.clone(),
                metadata: false,
                queue: queue.clone(),
                resume: resume.clone(),
            };
            if self.emit_read(&artifact_id, &index, None, kind, false, effects) {
                return;
            }
            let next = increment(&index);
            if matches!(compare(&next, &total), Ok(Ordering::Less)) {
                index = next;
                continue;
            }
            self.continue_recover_artifacts(queue, resume, effects);
            return;
        }
    }

    fn finish_recover(&mut self, resume: Resume, effects: &mut Vec<ProjectionEffect>) {
        self.error = None;
        self.publish();
        match resume {
            Resume::None => {}
            Resume::Accept(event) => {
                let next = match canonical(&event.sequence) {
                    Ok(next) => next,
                    Err(error) => {
                        self.set_error(error);
                        return;
                    }
                };
                let outcome = self.process_payload(&event, &next, effects);
                if matches!(outcome, PayloadOutcome::Advance) {
                    self.sequence = Some(next);
                }
            }
            Resume::ConsumeDone { generation } => {
                let terminal = self.view.as_ref().is_some_and(|v| is_terminal(v.state));
                if !terminal && generation == self.generation {
                    self.start_subscribe(effects);
                }
            }
            Resume::Resubscribe { generation } => {
                if generation == self.generation {
                    self.start_subscribe(effects);
                }
            }
            Resume::ThenConsume => self.start_subscribe(effects),
        }
    }

    // --- accept() ------------------------------------------------------------

    /// `accept(event)`: stream/execution filters, reset-before-dedupe, dedupe
    /// and gap recovery, then payload dispatch. The cursor advances only after
    /// any required read succeeds.
    pub(crate) fn accept(&mut self, event: EventEnvelope, effects: &mut Vec<ProjectionEffect>) {
        if event.stream_id != self.receipt.stream_id {
            return;
        }
        if event.execution_id.as_deref().is_some_and(|id| id != self.receipt.execution_id) {
            return;
        }
        if matches!(event.payload, ConnectionEvent::StreamResetRequired { .. }) {
            // A reset invalidates the cursor itself; its sequence may be below
            // the old cursor (stream restart), so handle it before dedupe.
            self.sequence = None;
            self.recover_with(Resume::None, effects);
            return;
        }
        let next = match canonical(&event.sequence) {
            Ok(next) => next,
            Err(error) => {
                self.set_error(error);
                return;
            }
        };
        if let Some(sequence) = self.sequence.clone() {
            match compare(&next, &sequence) {
                Ok(Ordering::Greater) => {}
                Ok(_) => return, // next <= sequence: duplicate / replay
                Err(error) => {
                    self.set_error(error);
                    return;
                }
            }
            if next != increment(&sequence) {
                self.recover_with(Resume::Accept(event), effects);
                return;
            }
        }
        let outcome = self.process_payload(&event, &next, effects);
        if matches!(outcome, PayloadOutcome::Advance) {
            self.sequence = Some(next);
        }
    }

    fn process_payload(
        &mut self,
        event: &EventEnvelope,
        next: &str,
        effects: &mut Vec<ProjectionEffect>,
    ) -> PayloadOutcome {
        match &event.payload {
            ConnectionEvent::SessionChanged { session } => {
                if same_handle(Some(&session.handle), self.original_handle.as_ref())
                    && same_handle(event.session_handle.as_ref(), self.original_handle.as_ref())
                {
                    if let Some(callback) = self.on_session.as_mut() {
                        callback(session.clone());
                    }
                }
                PayloadOutcome::Advance
            }
            ConnectionEvent::ExecutionChanged { execution } => {
                if execution.execution_id != self.receipt.execution_id {
                    return PayloadOutcome::Stop;
                }
                self.view = Some(if self.runtime_invalidated {
                    with_binding_null(execution.clone())
                } else {
                    execution.clone()
                });
                self.publish();
                PayloadOutcome::Advance
            }
            ConnectionEvent::ResultChunk { artifact_id, chunk_index, source } => {
                if source.execution_id != self.receipt.execution_id {
                    return PayloadOutcome::Stop;
                }
                let index = match canonical(chunk_index) {
                    Ok(index) => index,
                    Err(error) => {
                        self.set_error(error);
                        return PayloadOutcome::Stop;
                    }
                };
                let kind = ReadKind::Accept {
                    artifact_id: artifact_id.clone(),
                    index: index.clone(),
                    source: Some(source.clone()),
                    sequence: next.to_owned(),
                };
                if !self.emit_read(artifact_id, &index, Some(source.clone()), kind, false, effects) {
                    // Cached: React's `read()` returns immediately, then the
                    // cursor advances below.
                    self.sequence = Some(next.to_owned());
                }
                PayloadOutcome::DeferRead
            }
            ConnectionEvent::StreamResetRequired { .. } => PayloadOutcome::Stop,
        }
    }

    // --- host feeds ----------------------------------------------------------

    pub(crate) fn on_get_execution(
        &mut self,
        call_id: u64,
        result: Result<ExecutionView, ProjectionError>,
        effects: &mut Vec<ProjectionEffect>,
    ) {
        let resume = match self.pending.take() {
            Some(Pending::Execution { call_id: id, resume }) if id == call_id => resume,
            other => {
                self.pending = other;
                return;
            }
        };
        let view = match result {
            Ok(view) => view,
            Err(error) => {
                self.set_error(error);
                return;
            }
        };
        if view.execution_id != self.receipt.execution_id {
            self.set_error(ProjectionError::ExecutionIdentityMismatch);
            return;
        }
        self.view = Some(if self.runtime_invalidated {
            with_binding_null(view)
        } else {
            view
        });
        self.publish();
        let queue: VecDeque<String> = self
            .view
            .as_ref()
            .map(|v| v.artifact_ids.iter().cloned().collect())
            .unwrap_or_default();
        self.continue_recover_artifacts(queue, resume, effects);
    }

    pub(crate) fn on_read_artifact(
        &mut self,
        call_id: u64,
        result: Result<ArtifactChunk, ProjectionError>,
        effects: &mut Vec<ProjectionEffect>,
    ) {
        let (key, kind) = match self.pending.take() {
            Some(Pending::Read { call_id: id, key, kind }) if id == call_id => (key, kind),
            other => {
                self.pending = other;
                return;
            }
        };
        self.reads_in_flight.remove(&key);
        let chunk = match result {
            Ok(chunk) => chunk,
            Err(error) => {
                self.set_error(error);
                return;
            }
        };
        match kind {
            ReadKind::Accept { artifact_id, index, source, sequence } => {
                if !self.chunk_matches(&chunk, &artifact_id, &index) {
                    self.set_error(ProjectionError::ArtifactIdentityMismatch);
                    return;
                }
                let published = self.published(&chunk, source);
                self.chunks_by_key.insert(key, published);
                self.publish();
                // "Advance only after reads succeed."
                self.sequence = Some(sequence);
            }
            ReadKind::Recover { artifact_id, index, total, metadata, queue, resume } => {
                if !self.chunk_matches(&chunk, &artifact_id, &index) {
                    // A bad read never poisons a previously published prefix.
                    self.set_error(ProjectionError::ArtifactIdentityMismatch);
                    return;
                }
                if metadata {
                    let count = chunk
                        .total_chunks
                        .as_ref()
                        .or(chunk.published_chunk_count.as_ref())
                        .and_then(|value| canonical(value).ok());
                    let count = match count {
                        Some(count) => count,
                        None => {
                            self.set_error(ProjectionError::PublicationMetadataUnavailable);
                            return;
                        }
                    };
                    if matches!(compare(&count, "0"), Ok(Ordering::Greater))
                        && !self.chunks_by_key.contains_key(&key)
                    {
                        let published = self.published(&chunk, None);
                        self.chunks_by_key.insert(key, published);
                        self.publish();
                    }
                    if matches!(compare("1", &count), Ok(Ordering::Less)) {
                        self.emit_recover_read(
                            artifact_id, "1".to_owned(), count, queue, resume, effects,
                        );
                    } else {
                        self.continue_recover_artifacts(queue, resume, effects);
                    }
                } else {
                    let published = self.published(&chunk, None);
                    self.chunks_by_key.insert(key, published);
                    self.publish();
                    let next = increment(&index);
                    if matches!(compare(&next, &total), Ok(Ordering::Less)) {
                        self.emit_recover_read(
                            artifact_id, next, total, queue, resume, effects,
                        );
                    } else {
                        self.continue_recover_artifacts(queue, resume, effects);
                    }
                }
            }
        }
    }

    // --- consume() / lifecycle ----------------------------------------------

    fn start_subscribe(&mut self, effects: &mut Vec<ProjectionEffect>) {
        let call_id = self.alloc_call();
        self.subscription = Some(Subscription { call_id, generation: self.generation });
        effects.push(ProjectionEffect::SubscribeEvents {
            call_id,
            stream_id: self.receipt.stream_id.clone(),
            after_sequence: self.sequence.clone(),
        });
    }

    pub(crate) fn consume(&mut self) -> Vec<ProjectionEffect> {
        let mut effects = Vec::new();
        // React `consume` begins with `await this.unsubscribe()`.
        self.unsubscribe(&mut effects);
        self.start_subscribe(&mut effects);
        effects
    }

    pub(crate) fn on_event(
        &mut self,
        call_id: u64,
        item: StreamItem,
        effects: &mut Vec<ProjectionEffect>,
    ) {
        let subscription = match self.subscription.as_ref() {
            Some(subscription)
                if subscription.call_id == call_id && subscription.generation == self.generation =>
            {
                subscription.clone()
            }
            _ => return,
        };
        match item {
            StreamItem::Event(event) => {
                let reset = matches!(event.payload, ConnectionEvent::StreamResetRequired { .. });
                self.accept(event, effects);
                if reset {
                    // The reset's `recover()` should be followed by a fresh
                    // subscription once it completes (React breaks, then
                    // resubscribes after its 100ms breathing pause).
                    if let Some(Pending::Execution { resume, .. }) = self.pending.as_mut() {
                        *resume = Resume::Resubscribe { generation: subscription.generation };
                    }
                    self.subscription = None;
                }
            }
            StreamItem::Done => {
                self.subscription = None;
                self.recover_with(
                    Resume::ConsumeDone { generation: subscription.generation },
                    effects,
                );
            }
            StreamItem::Error(error) => {
                self.subscription = None;
                self.set_error(error);
            }
        }
    }

    pub(crate) fn reconnect(&mut self) -> Vec<ProjectionEffect> {
        let mut effects = Vec::new();
        self.unsubscribe(&mut effects);
        self.recover_with(Resume::ThenConsume, &mut effects);
        effects
    }

    /// `unsubscribe()`: invalidate the current generation and stop the stream.
    pub(crate) fn unsubscribe(&mut self, effects: &mut Vec<ProjectionEffect>) {
        self.generation += 1;
        if let Some(subscription) = self.subscription.take() {
            let call_id = self.alloc_call();
            effects.push(ProjectionEffect::CloseStream {
                call_id,
                stream_call_id: subscription.call_id,
            });
        }
    }

    pub(crate) fn dispose(&mut self) -> Vec<ProjectionEffect> {
        self.listeners.clear();
        let mut effects = Vec::new();
        self.unsubscribe(&mut effects);
        effects
    }
}

