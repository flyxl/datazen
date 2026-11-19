//! AI chat session store (ported from React
//! `packages/frontend/src/stores/ai/sessions.ts`, tasklist T040 — the row
//! is marked `split`, and the split marker's scope applies here: this
//! module owns **sessions.ts's** semantics only; the sibling React files
//! `stores/ai/streaming.ts`, `stores/ai/types.ts` and `stores/aiStore.ts`
//! are separate tasks and are not ported by it).
//!
//! ## What the file is (and therefore what this module is)
//!
//! sessions.ts is a 760-line module of exported functions over four
//! ambient IO points — despite living in `stores/`, it is *not* a zustand
//! `create()` store (its sibling `aiStore.ts` is the zustand half, T128).
//! The port is accordingly: pure functions plus an
//! [`AiSessionStore`](store::AiSessionStore) struct whose constructor
//! injects every ambient IO as a seam, so the module compiles for wasm32
//! with no IO of its own.
//!
//! ## The seams (IO as injections, no real IO introduced)
//!
//! - **backend** — React's `aiCommands.sessions*` (AppDb over IPC) → the
//!   [`AiSessionBackend`](store::AiSessionBackend) trait. `Err(String)` is
//!   the *display message* React's `errorMessage()` would produce, so the
//!   port composes the exact user-visible strings React builds. The real
//!   adapter belongs to the commands/ai.ts port task.
//! - **storage** — React's `localStorage` (offline fallback + legacy
//!   migration source) → the
//!   [`SessionLocalStorage`](local::SessionLocalStorage) trait.
//! - **clock** — `Date.now()` / `new Date().toISOString()` →
//!   [`SessionClock`](store::SessionClock); the pure conversion of *stored*
//!   timestamps is [`iso_from_epoch_ms`](model::iso_from_epoch_ms).
//! - **ids** — `crypto.randomUUID()` → [`NewId`](store::NewId).
//! - **live connection map** — `useActiveConnectionStore.getState()
//!   .connections`, re-read per call in React → the
//!   [`ActiveConnections`](store::ActiveConnections) snapshot passed as an
//!   argument to the restore-side operations.
//!
//! ## Async half
//!
//! Every `await` in sessions.ts becomes `Result` propagation; the two
//! genuinely-async halves are documented at their owners:
//! `persistChatSession`'s `sessionWriteQueue` serialization is inherent in
//! the sync port (`&mut self`) and the async bridge re-adds it when the
//! aiStore port (T128) wires the adapter's poll; subscription/reactivity
//! is likewise T128's. `downloadSessionExport` (a DOM-only browser
//! download trigger) has no Rune counterpart in this crate — the host
//! shell's download wiring owns it.
//!
//! ## Layout
//!
//! - [`model`] — the sessions.ts-specific types (scope, results, export
//!   format) and pure derivations: session key, legacy key split, byte
//!   truncation, epoch→ISO conversion, loaded-skills normalization. The
//!   message/session vocabulary itself is the T042 twin, re-exported from
//!   [`crate::ai_types`] (one source since the unification).
//! - [`mapping`] — the pure mappers between the live session and its
//!   stored/backend shapes (status derivation, metadata projection, title,
//!   upsert payload, local detail, markdown/filename export).
//! - [`local`] — the localStorage half: the ordered session map, the
//!   corrupted-storage branches, and the LRU/message truncation policy.
//! - [`store`] — the seam types, the store struct and the write-path
//!   operations (persist/delete/export/load/new/touch).
//! - [`restore`] — the backend-first restore ladder: legacy migration,
//!   restore, list, and the `dbSessionId`-key rebind.
//! - [`test_support`] — the React suites' doubles (test-only).
//!
//! ## Known deviations from React-as-written (each pinned by a test)
//!
//! - `touchSession` on an absent key: React's spread-of-`undefined` leaves
//!   a messages-less entry that later throws inside its own writers; the
//!   port inserts the benign empty-messages equivalent.
//! - Structurally invalid stored entries (a `messages` that is not an
//!   array, a `role` outside the vocabulary) take the whole load down
//!   React's clear-and-fresh corrupted branch instead of crashing later
//!   consumers uncaught; missing/null fields load as defaults (React's own
//!   `?? 0` tolerances).
//! - An out-of-`Date`-range-but-finite stored `lastAccess` aborts the
//!   migration with `RangeError: Invalid time value`'s message, matching
//!   where React throws; the port reports it through the same
//!   `{migrated, error}` return instead of a rejected promise.
//! - `JSON.stringify` pretty-printing in the offline export emits BTree
//!   key order for metadata objects (semantic equality preserved; JS
//!   insertion order is not observable through this module's tests).

mod local;
mod mapping;
mod model;
mod restore;
mod store;
#[cfg(test)]
mod test_support;

pub use local::{
    MAX_MESSAGES_PER_SESSION, MAX_MESSAGE_BYTES, MAX_SESSIONS, MIGRATION_MARKER_KEY, STORAGE_KEY,
    SessionLocalStorage, SessionMap, StoredSession,
};
pub use mapping::{
    chat_session_from_detail, chat_session_from_stored, chat_session_to_upsert_input,
    export_filename, local_detail, markdown_for, message_from_stored, message_metadata,
    message_status, new_chat_session, title_for,
};
pub use model::{
    ExportFormat, MigrationOutcome, SessionDeleteResult, SessionExport, SessionKey,
    SessionPersistResult, SessionRestoreResult, SessionScope, compute_session_key,
    iso_from_epoch_ms, normalize_loaded_skills, split_legacy_key, truncate_bytes,
};
pub use store::{
    ActiveConnectionEntry, ActiveConnections, AiSessionBackend, AiSessionStore, NewId,
    SessionClock, SessionListParams,
};
