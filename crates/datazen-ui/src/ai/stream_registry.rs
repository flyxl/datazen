//! Streaming lifecycle state machine ported from `packages/frontend/src/ai/streamRegistry.ts`.
//!
//! ```text
//! starting ──first chunk──▶ streaming ──cancel marked──▶ cancelling
//!     │                         │                            │
//!     └──── terminal / error / timeout ────────────────────────┘
//!                                 ▼
//!                        terminal(done | cancelled | error)
//! ```
//!
//! One entry per `requestId`. Two invariants the UI depends on are kept verbatim:
//!
//! 1. **Generations are monotonic per conversation.** [`StreamRegistry::begin`]
//!    bumps the conversation generation, so a chunk from a superseded run can
//!    never be mistaken for the current run's output.
//! 2. **Late chunks are dropped, not merged.** After a generation change or a
//!    cancel, only the terminal chunk is accepted; every other late chunk is
//!    reported with a [`StreamDropReason`] and never mutates the entry.
//!
//! # Faithfulness
//!
//! Every field of the React `StreamEntry`, `ApplyChunkInput`,
//! `ApplyChunkResult`, every transition of `begin` / `applyChunk` /
//! `applyError` / `markCancelling` / `settle`, the `gate` drop order
//! (`unknown-request` → `stale-generation` → `terminal` → `cancel-issued`) and
//! the four `subscribe` event types (`started`, `streaming`, `cancelling`,
//! `settled`) have a counterpart here. React has **no pruning and no cap**:
//! entries and generations are retained forever, so this port has none either.
//! The 8s cancel fallback itself lives in `session_cancel`, exactly as in React.
//!
//! # Immediate-mode adaptation (honest)
//!
//! * The React `now?: () => number` clock is injected as `fn() -> i64` with a
//!   deterministic default of `0`. No wall clock is read and no timer is armed;
//!   `mark_cancelling_at` takes the explicit `atMs` the React tests pass.
//! * React `subscribe` returns an unsubscribe *closure*. A closure that borrows
//!   the machine cannot live beside it in a Rust `S`, so — as in the T097 port —
//!   [`StreamRegistry::subscribe`] returns a `u64` id and
//!   [`StreamRegistry::unsubscribe`] detaches it.
//! * Listener events carry the full entry snapshot, as React's
//!   `Readonly<StreamEntry>` does (T097's local mirror only reads `request_id`
//!   and `terminalKind`). `StreamEvent::request_id` / `::terminal_kind` expose
//!   those two fields for the future T097 swap.
//!
//! No colour, glyph or `oklch` value appears here; this is pure logic.

use std::collections::HashMap;

/// `StreamPhase` from `streamRegistry.ts` — the four states of one run.
#[allow(dead_code)] // consumer: AI chat panel (not landed). Delete when it lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamPhase {
    Starting,
    Streaming,
    Cancelling,
    Terminal,
}

/// `StreamTerminalKind` from `streamRegistry.ts`.
#[allow(dead_code)] // consumer: AI chat panel (not landed). Delete when it lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamTerminalKind {
    Done,
    Cancelled,
    Error,
}

/// `StreamDropReason` — why a chunk was not applied.
#[allow(dead_code)] // reported by apply_chunk/apply_error; delete with the registry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamDropReason {
    UnknownRequest,
    StaleGeneration,
    CancelIssued,
    Terminal,
}

#[allow(dead_code)] // string parity with the React union; delete with the enum.
impl StreamDropReason {
    pub fn as_str(self) -> &'static str {
        match self {
            StreamDropReason::UnknownRequest => "unknown-request",
            StreamDropReason::StaleGeneration => "stale-generation",
            StreamDropReason::CancelIssued => "cancel-issued",
            StreamDropReason::Terminal => "terminal",
        }
    }
}

/// `StreamEntry` — `readonly` identity plus mutable lifecycle fields. `get`
/// always returns a clone, mirroring React's `snapshot(entry)` spread.
#[allow(dead_code)] // consumer: AI chat panel (not landed). Delete when it lands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamEntry {
    pub request_id: String,
    pub conversation_id: String,
    pub generation: u64,
    pub phase: StreamPhase,
    pub terminal_kind: Option<StreamTerminalKind>,
    pub cancel_issued_at_ms: Option<i64>,
}

/// Compatibility alias for the T097 cleanup: T097's local
/// `session_cancel::StreamEntrySnapshot` is a three-field projection of this
/// struct. Removal stage: when T097 is rewired to `StreamRegistry` directly.
#[allow(dead_code)] // swap shim only; delete with the T097 local trait.
pub type StreamEntrySnapshot = StreamEntry;

/// `ApplyChunkInput` — terminal markers of one backend chunk. An empty
/// `error` string is falsy in React and is therefore treated as no error.
#[allow(dead_code)] // consumer: AI chat panel (not landed). Delete when it lands.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ApplyChunkInput {
    pub done: bool,
    pub cancelled: bool,
    pub error: Option<String>,
}

#[allow(dead_code)] // convenience constructors; delete with the input type.
impl ApplyChunkInput {
    /// A non-terminal content chunk.
    pub fn content() -> Self {
        Self::default()
    }

    /// A terminal chunk (`done: true`).
    pub fn done() -> Self {
        Self {
            done: true,
            ..Self::default()
        }
    }

    /// A terminal chunk ended by cancel (`done && cancelled`).
    pub fn cancelled() -> Self {
        Self {
            done: true,
            cancelled: true,
            ..Self::default()
        }
    }

    /// An inline backend error, treated as a terminal `error`.
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            error: Some(message.into()),
            ..Self::default()
        }
    }
}

/// `ApplyChunkResult` — the synchronous outcome of applying a chunk. A dropped
/// chunk keeps the entry's current `phase` / `generation` / `terminalKind`
/// (all `null` for an unknown request), exactly like React.
#[allow(dead_code)] // consumer: AI chat panel (not landed). Delete when it lands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApplyChunkResult {
    pub accepted: bool,
    pub drop_reason: Option<StreamDropReason>,
    pub phase: Option<StreamPhase>,
    pub generation: Option<u64>,
    pub terminal_kind: Option<StreamTerminalKind>,
}

/// `StreamRegistryEvent` — the event shape delivered to `subscribe` listeners.
/// The first three variants carry `entry: Readonly<StreamEntry>` just like the
/// React union; `Settled` additionally carries `terminalKind`.
#[allow(dead_code)] // consumer: AI chat panel (not landed). Delete when it lands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StreamEvent {
    Started { entry: StreamEntry },
    Streaming { entry: StreamEntry },
    Cancelling { entry: StreamEntry },
    Settled {
        entry: StreamEntry,
        terminal_kind: StreamTerminalKind,
    },
}

#[allow(dead_code)] // observers use these; delete with StreamEvent.
impl StreamEvent {
    /// React `event.type` as a string.
    pub fn event_type(&self) -> &'static str {
        match self {
            StreamEvent::Started { .. } => "started",
            StreamEvent::Streaming { .. } => "streaming",
            StreamEvent::Cancelling { .. } => "cancelling",
            StreamEvent::Settled { .. } => "settled",
        }
    }

    /// The `requestId` of the entry this event describes (T097 swap helper).
    pub fn request_id(&self) -> &str {
        match self {
            StreamEvent::Started { entry }
            | StreamEvent::Streaming { entry }
            | StreamEvent::Cancelling { entry }
            | StreamEvent::Settled { entry, .. } => &entry.request_id,
        }
    }

    /// `Some(kind)` only for the `settled` arm (T097 swap helper).
    pub fn terminal_kind(&self) -> Option<StreamTerminalKind> {
        match self {
            StreamEvent::Settled { terminal_kind, .. } => Some(*terminal_kind),
            _ => None,
        }
    }
}

/// Listener id returned by [`StreamRegistry::subscribe`]. `u64` ids replace
/// React's returned unsubscribe closure for the reason in the module doc.
#[allow(dead_code)] // swap shim; delete with the T097 local trait.
pub type ListenerId = u64;

/// `StreamRegistryListener` from `streamRegistry.ts`.
#[allow(dead_code)] // observer API; delete with the registry.
pub type RegistryListener = Box<dyn FnMut(StreamEvent)>;

/// `StreamRegistry` — the ported class. Holds the three React maps, the
/// listener list and the injected clock. All state lives here; the host may
/// embed it in its own `S`.
#[allow(dead_code)] // consumer: AI chat panel (not landed). Delete when it lands.
pub struct StreamRegistry {
    entries: HashMap<String, StreamEntry>,
    generations: HashMap<String, u64>,
    current_request: HashMap<String, String>,
    listeners: Vec<(ListenerId, RegistryListener)>,
    next_listener_id: ListenerId,
    now: fn() -> i64,
}

#[allow(dead_code)] // constructors; delete with the registry.
impl Default for StreamRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(dead_code)] // consumer: AI chat panel (not landed). Delete when it lands.
impl StreamRegistry {
    /// A registry with a deterministic zero clock (pure tests / immediate mode).
    pub fn new() -> Self {
        Self::with_now(|| 0)
    }

    /// React `constructor(options)` with an injected `now()`.
    pub fn with_now(now: fn() -> i64) -> Self {
        StreamRegistry {
            entries: HashMap::new(),
            generations: HashMap::new(),
            current_request: HashMap::new(),
            listeners: Vec::new(),
            next_listener_id: 1,
            now,
        }
    }

    /// React `begin(conversationId, requestId)`.
    pub fn begin(&mut self, conversation_id: &str, request_id: &str) -> StreamEntry {
        // Duplicated send of a still-live request: no-op, no event, no new generation.
        if let Some(existing) = self.entries.get(request_id) {
            if existing.phase != StreamPhase::Terminal {
                return existing.clone();
            }
        }

        // A previous non-terminal run of this conversation is superseded and
        // settled as cancelled; its late chunks become `stale-generation`.
        if let Some(previous) = self.current_request.get(conversation_id).cloned() {
            if previous != request_id {
                let supersede = self
                    .entries
                    .get(&previous)
                    .is_some_and(|entry| entry.phase != StreamPhase::Terminal);
                if supersede {
                    self.settle(&previous, StreamTerminalKind::Cancelled);
                }
            }
        }

        let generation = self.generations.get(conversation_id).copied().unwrap_or(0) + 1;
        self.generations.insert(conversation_id.to_owned(), generation);
        self.current_request
            .insert(conversation_id.to_owned(), request_id.to_owned());

        let entry = StreamEntry {
            request_id: request_id.to_owned(),
            conversation_id: conversation_id.to_owned(),
            generation,
            phase: StreamPhase::Starting,
            terminal_kind: None,
            cancel_issued_at_ms: None,
        };
        self.entries.insert(request_id.to_owned(), entry.clone());
        self.emit(StreamEvent::Started {
            entry: entry.clone(),
        });
        entry
    }

    /// React `has(requestId)`. Terminal entries are still present (no pruning).
    pub fn has(&self, request_id: &str) -> bool {
        self.entries.contains_key(request_id)
    }

    /// React `get(requestId)` — always a fresh snapshot.
    pub fn get(&self, request_id: &str) -> Option<StreamEntry> {
        self.entries.get(request_id).cloned()
    }

    /// React `conversationGeneration(conversationId)`.
    pub fn conversation_generation(&self, conversation_id: &str) -> u64 {
        self.generations.get(conversation_id).copied().unwrap_or(0)
    }

    /// React `applyChunk(requestId, chunk)`.
    pub fn apply_chunk(
        &mut self,
        request_id: &str,
        chunk: ApplyChunkInput,
    ) -> ApplyChunkResult {
        let entry = match self.gate(request_id) {
            Ok(entry) => entry,
            Err(drop) => return drop,
        };

        // React checks `chunk.error` (truthy) before `chunk.done`.
        if chunk.error.as_deref().is_some_and(|message| !message.is_empty()) {
            return self.settle_entry(request_id, StreamTerminalKind::Error);
        }
        if chunk.done {
            let kind = if chunk.cancelled {
                StreamTerminalKind::Cancelled
            } else {
                StreamTerminalKind::Done
            };
            return self.settle_entry(request_id, kind);
        }

        // A cancel was marked: everything except the terminal chunk is late.
        if entry.phase == StreamPhase::Cancelling {
            return self.drop_result(&entry, StreamDropReason::CancelIssued);
        }

        if entry.phase == StreamPhase::Starting {
            if let Some(stored) = self.entries.get_mut(request_id) {
                stored.phase = StreamPhase::Streaming;
                let snapshot = stored.clone();
                self.emit(StreamEvent::Streaming { entry: snapshot });
            }
        }
        let after = self.entries.get(request_id).cloned().unwrap_or(entry);
        self.accepted(&after)
    }

    /// React `applyError(requestId)` — an out-of-band terminal error.
    pub fn apply_error(&mut self, request_id: &str) -> ApplyChunkResult {
        match self.gate(request_id) {
            Ok(_) => self.settle_entry(request_id, StreamTerminalKind::Error),
            Err(drop) => drop,
        }
    }

    /// React `markCancelling(requestId, atMs = this.now())`.
    pub fn mark_cancelling(&mut self, request_id: &str) -> bool {
        let at_ms = (self.now)();
        self.mark_cancelling_at(request_id, at_ms)
    }

    /// The explicit-timestamp form the React tests pass directly.
    pub fn mark_cancelling_at(&mut self, request_id: &str, at_ms: i64) -> bool {
        let Some(entry) = self.entries.get(request_id) else {
            return false;
        };
        if entry.phase == StreamPhase::Terminal {
            return false;
        }
        if self.generations.get(&entry.conversation_id).copied() != Some(entry.generation) {
            return false;
        }
        if entry.phase == StreamPhase::Cancelling {
            return true;
        }
        if let Some(stored) = self.entries.get_mut(request_id) {
            stored.phase = StreamPhase::Cancelling;
            stored.cancel_issued_at_ms = Some(at_ms);
            let snapshot = stored.clone();
            self.emit(StreamEvent::Cancelling { entry: snapshot });
        }
        true
    }

    /// React `settle(requestId, kind)`. Idempotent: `false` for terminal/unknown.
    pub fn settle(&mut self, request_id: &str, kind: StreamTerminalKind) -> bool {
        let Some(entry) = self.entries.get(request_id) else {
            return false;
        };
        if entry.phase == StreamPhase::Terminal {
            return false;
        }
        self.terminalize(request_id, kind);
        true
    }

    /// React `subscribe(listener)`. Returns a listener id; pair with
    /// [`StreamRegistry::unsubscribe`] (see the module doc for why not a closure).
    pub fn subscribe(&mut self, listener: RegistryListener) -> ListenerId {
        let id = self.next_listener_id;
        self.next_listener_id = self.next_listener_id.wrapping_add(1);
        self.listeners.push((id, listener));
        id
    }

    /// Detach the listener previously returned by [`StreamRegistry::subscribe`].
    pub fn unsubscribe(&mut self, id: ListenerId) {
        self.listeners.retain(|(listener_id, _)| *listener_id != id);
    }

    /// React's private `gate`: the three drop cases in source order.
    fn gate(&self, request_id: &str) -> Result<StreamEntry, ApplyChunkResult> {
        let Some(entry) = self.entries.get(request_id) else {
            return Err(ApplyChunkResult {
                accepted: false,
                drop_reason: Some(StreamDropReason::UnknownRequest),
                phase: None,
                generation: None,
                terminal_kind: None,
            });
        };
        if self.generations.get(&entry.conversation_id).copied() != Some(entry.generation) {
            return Err(ApplyChunkResult {
                accepted: false,
                drop_reason: Some(StreamDropReason::StaleGeneration),
                phase: Some(entry.phase),
                generation: Some(entry.generation),
                terminal_kind: entry.terminal_kind,
            });
        }
        if entry.phase == StreamPhase::Terminal {
            return Err(ApplyChunkResult {
                accepted: false,
                drop_reason: Some(StreamDropReason::Terminal),
                phase: Some(entry.phase),
                generation: Some(entry.generation),
                terminal_kind: entry.terminal_kind,
            });
        }
        Ok(entry.clone())
    }

    /// React's private `accepted`.
    fn accepted(&self, entry: &StreamEntry) -> ApplyChunkResult {
        ApplyChunkResult {
            accepted: true,
            drop_reason: None,
            phase: Some(entry.phase),
            generation: Some(entry.generation),
            terminal_kind: entry.terminal_kind,
        }
    }

    /// React's private `drop`.
    fn drop_result(&self, entry: &StreamEntry, reason: StreamDropReason) -> ApplyChunkResult {
        ApplyChunkResult {
            accepted: false,
            drop_reason: Some(reason),
            phase: Some(entry.phase),
            generation: Some(entry.generation),
            terminal_kind: entry.terminal_kind,
        }
    }

    /// React's private `settleEntry` (terminal is idempotent).
    fn settle_entry(&mut self, request_id: &str, kind: StreamTerminalKind) -> ApplyChunkResult {
        let Some(entry) = self.entries.get(request_id) else {
            return ApplyChunkResult {
                accepted: false,
                drop_reason: Some(StreamDropReason::UnknownRequest),
                phase: None,
                generation: None,
                terminal_kind: None,
            };
        };
        if entry.phase == StreamPhase::Terminal {
            let snapshot = entry.clone();
            return self.drop_result(&snapshot, StreamDropReason::Terminal);
        }
        self.terminalize(request_id, kind);
        match self.entries.get(request_id) {
            Some(after) => self.accepted(after),
            None => ApplyChunkResult {
                accepted: false,
                drop_reason: Some(StreamDropReason::UnknownRequest),
                phase: None,
                generation: None,
                terminal_kind: None,
            },
        }
    }

    /// React's private `terminalize` — sets the phase/kind and emits `settled`.
    fn terminalize(&mut self, request_id: &str, kind: StreamTerminalKind) {
        if let Some(entry) = self.entries.get_mut(request_id) {
            entry.phase = StreamPhase::Terminal;
            entry.terminal_kind = Some(kind);
            let snapshot = entry.clone();
            self.emit(StreamEvent::Settled {
                entry: snapshot,
                terminal_kind: kind,
            });
        }
    }

    /// React's private `emit` — listeners are notified in subscription order.
    fn emit(&mut self, event: StreamEvent) {
        for (_, listener) in &mut self.listeners {
            listener(event.clone());
        }
    }
}

// Leftover L1 (removal stage: when the AI chat panel lands and T097 is rewired
// to this module): delete `session_cancel`'s local `StreamPhase`,
// `StreamTerminalKind`, `StreamEntrySnapshot`, `StreamEvent`, `RegistryListener`
// and `StreamRegistry` trait, import the types above, and adapt the two places
// that pattern-match `StreamEvent::Settled { request_id, terminal_kind }` to use
// `event.request_id()` / `event.terminal_kind()`. `mark_cancelling`, `get`,
// `settle`, `subscribe` and `unsubscribe` already have matching signatures.
// No fake async, timer or IPC is created here; the host owns all side effects.
