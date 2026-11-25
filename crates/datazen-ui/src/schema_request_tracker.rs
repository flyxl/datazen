//! Request identity and session lifecycle barriers, ported from React
//! `packages/frontend/src/stores/schemaRequestTracker.ts` (T421).
//!
//! No UI state lives here. React's class hands out closures that answer
//! "is the session/slot I captured still the live one?" by object
//! identity (`sessions.get(id) === session`, `session.get(slot) ===
//! request`). The port keeps the exact semantics with generation and
//! token numbers: a session's identity is its generation (a new
//! generation is minted whenever the session map is created), a slot's
//! identity is its request token; both are compared, never reused, and
//! `invalidate`/`reset` drop them so every barrier from before answers
//! `false` — the same "not the live one" outcome, without `Rc` pointer
//! identity.
//!
//! ## The three capture shapes
//!
//! * [`SchemaRequestTracker::capture`] — the session barrier: answers
//!   whether the db session is still the one the caller saw.
//! * [`SchemaRequestTracker::begin`] — starts a request in a slot,
//!   replacing any request the slot held (its barrier goes stale).
//! * [`SchemaRequestTracker::capture_slot`] — binds to the request the
//!   slot already holds, or begins one when the slot is empty.

use std::collections::HashMap;

/// The session barrier a `capture()` closure becomes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionBarrier {
    db_session_id: String,
    generation: u64,
}

/// The slot barrier a `begin()`/`captureSlot()` closure becomes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlotBarrier {
    db_session_id: String,
    slot: String,
    generation: u64,
    request_token: u64,
}

/// The tracker (`SchemaRequestTracker`).
#[derive(Default)]
pub struct SchemaRequestTracker {
    /// Per db session: the slot→request-token map, with the generation
    /// the session was created under.
    sessions: HashMap<String, (u64, HashMap<String, u64>)>,
    next_identity: u64,
}

impl SchemaRequestTracker {
    /// `capture`: ensure the db session's slot map exists (the same one
    /// is reused while the session lives) and return the session
    /// barrier.
    pub fn capture(&mut self, db_session_id: &str) -> SessionBarrier {
        let barrier = self.ensure_session(db_session_id);
        barrier
    }

    /// `begin`: capture the session and start a new request in the slot —
    /// any request the slot held is replaced, so its barrier goes stale.
    pub fn begin(&mut self, db_session_id: &str, slot: &str) -> SlotBarrier {
        let session = self.ensure_session(db_session_id);
        let request_token = self.fresh_identity();
        let (_, slots) = self
            .sessions
            .get_mut(db_session_id)
            .expect("the session was just ensured");
        slots.insert(slot.to_owned(), request_token);
        SlotBarrier {
            db_session_id: db_session_id.to_owned(),
            slot: slot.to_owned(),
            generation: session.generation,
            request_token,
        }
    }

    /// `captureSlot`: bind to the request the slot already holds; an
    /// empty slot (or a missing session) begins one.
    pub fn capture_slot(&mut self, db_session_id: &str, slot: &str) -> SlotBarrier {
        let session = self.ensure_session(db_session_id);
        let existing = self
            .sessions
            .get(db_session_id)
            .and_then(|(_, slots)| slots.get(slot))
            .copied();
        match existing {
            Some(request_token) => SlotBarrier {
                db_session_id: db_session_id.to_owned(),
                slot: slot.to_owned(),
                generation: session.generation,
                request_token,
            },
            None => self.begin(db_session_id, slot),
        }
    }

    /// The `capture()` closure: the session the caller saw is still the
    /// live one.
    pub fn session_alive(&self, barrier: &SessionBarrier) -> bool {
        self.sessions
            .get(&barrier.db_session_id)
            .is_some_and(|(generation, _)| *generation == barrier.generation)
    }

    /// The `begin()`/`captureSlot()` closure: the session is alive and
    /// the slot still holds the caller's request.
    pub fn slot_alive(&self, barrier: &SlotBarrier) -> bool {
        self.sessions
            .get(&barrier.db_session_id)
            .is_some_and(|(generation, slots)| {
                *generation == barrier.generation
                    && slots.get(&barrier.slot).is_some_and(|token| *token == barrier.request_token)
            })
    }

    /// `invalidate`: the db session's whole slot map (and every barrier
    /// from it) goes.
    pub fn invalidate(&mut self, db_session_id: &str) {
        self.sessions.remove(db_session_id);
    }

    /// `reset`: every session goes.
    pub fn reset(&mut self) {
        self.sessions.clear();
    }

    /// Ensure the session's slot map exists; a fresh one gets a fresh
    /// generation.
    fn ensure_session(&mut self, db_session_id: &str) -> SessionBarrier {
        let generation = match self.sessions.get(db_session_id) {
            Some((generation, _)) => *generation,
            None => {
                let generation = self.fresh_identity();
                self.sessions
                    .insert(db_session_id.to_owned(), (generation, HashMap::new()));
                generation
            }
        };
        SessionBarrier {
            db_session_id: db_session_id.to_owned(),
            generation,
        }
    }

    fn fresh_identity(&mut self) -> u64 {
        self.next_identity += 1;
        self.next_identity
    }
}
