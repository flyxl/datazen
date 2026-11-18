//! The AI session store: injected seams plus the write-path operations.
//!
//! Ported from React `packages/frontend/src/stores/ai/sessions.ts`'s
//! backend-first half. React reaches four ambient IO points; the port
//! turns each into an injection so the module stays pure and synchronous
//! (the api crate compiles for wasm32; the async half is the consumer's
//! bridge):
//!
//! - **`aiCommands.sessions*`** → the [`AiSessionBackend`] trait. Errors
//!   are `String` *display messages* — React's `errorMessage(error)`
//!   (`error instanceof Error ? error.message : String(error)`) runs at the
//!   adapter boundary, so this port composes them into the exact user-
//!   visible strings (`"AI session was saved locally only: {msg}"`, …) it
//!   builds. The commands/ai.ts port owns the real adapter.
//! - **`localStorage`** → the [`SessionLocalStorage`](super::local)
//!   trait (also injected here as a field).
//! - **`Date.now()` / `new Date().toISOString()`** → [`SessionClock`].
//! - **`crypto.randomUUID()`** → [`NewId`].
//!
//! React's one piece of module state — the `sessionWriteQueue` promise
//! chain serializing AppDb writes so a slow user-message upsert cannot
//! land after the terminal assistant-message upsert — has no sync
//! counterpart: every method takes `&mut self`, so calls are inherently
//! serialized. The async bridge (T128's aiStore port) re-adds the queue
//! when it wires the adapter's poll.
//!
//! `useActiveConnectionStore.getState().connections` is *not* a field: it
//! is a live snapshot React re-reads per call, so the restore-side
//! operations take an [`ActiveConnections`] argument per call — the host
//! passes its current state (see `restore.rs`).

use serde::{Deserialize, Serialize};

use super::local::{
    SessionMap, StoredSession, load_sessions, local_sessions_migrated, mark_local_sessions_migrated,
    save_sessions, SessionLocalStorage,
};
use super::mapping::{
    chat_session_from_stored, chat_session_to_upsert_input as to_upsert_input, export_filename,
    local_detail, markdown_for, new_chat_session as new_session,
};
use super::model::{
    AiChatSession, AiSessionDetail, AiSessionSummary, AiSessionUpsertInput, ExportFormat,
    SessionScope,
};

/// React's two clock reads: `Date.now()` (epoch ms, LRU stamps) and
/// `new Date().toISOString()` (timestamps written into payloads and
/// metadata). Tests wire deterministic cells; hosts wire their frame clock.
pub struct SessionClock {
    pub now_ms: Box<dyn Fn() -> f64>,
    pub now_iso: Box<dyn Fn() -> String>,
}

/// React's `crypto.randomUUID()` — ids minted at the seams where a stored
/// message or session lacks one.
pub type NewId = Box<dyn FnMut() -> String>;

/// `aiCommands.sessionsList`'s `{ connectionId, database, sessionKey }`.
///
/// `None` is the null spelling (the legacy lookup sends explicit nulls);
/// `Some("")` is the `?? ''` spelling `listForScope` builds when the scope
/// omits a dimension.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SessionListParams {
    pub connection_id: Option<String>,
    pub database: Option<String>,
    pub session_key: Option<String>,
}

/// The backend seam: React's `aiCommands` session surface
/// (`ai_sessions_list/get/upsert/delete/export`).
///
/// The methods mirror the commands' signatures; `Err(String)` is the
/// display message React's `errorMessage()` would produce for the thrown
/// error. The adapter for the real AppDb belongs to the commands/ai.ts port
/// (async half bridged there).
pub trait AiSessionBackend {
    fn sessions_list(&self, params: &SessionListParams) -> Result<Vec<AiSessionSummary>, String>;
    fn sessions_get(&self, id: &str) -> Result<AiSessionDetail, String>;
    fn sessions_upsert(&self, input: &AiSessionUpsertInput) -> Result<AiSessionDetail, String>;
    fn sessions_delete(&self, id: &str) -> Result<bool, String>;
    fn sessions_export(&self, id: &str, format: ExportFormat) -> Result<String, String>;
}

/// One entry of the active-connection map React consults: the persistent
/// connection id and its **current runtime** database-session handle
/// (empty when disconnected — React's `markDisconnected` writes `''`).
#[derive(Clone, Debug, PartialEq)]
pub struct ActiveConnectionEntry {
    pub connection_id: String,
    pub db_session_id: String,
}

/// React's `useActiveConnectionStore.getState().connections` snapshot —
/// passed per call, because React re-reads the live store on every access.
///
/// A `Vec` (not a map) preserves `Object.values`'s enumeration order for
/// the reverse lookup; entries are keyed by a unique `connection_id`
/// (React's Record keys are unique — hosts pass one entry per connection).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ActiveConnections {
    entries: Vec<ActiveConnectionEntry>,
}

impl ActiveConnections {
    pub fn from_entries(entries: Vec<ActiveConnectionEntry>) -> Self {
        Self { entries }
    }

    /// Convenience constructor from `(connectionId, dbSessionId)` pairs.
    pub fn from_pairs(pairs: Vec<(String, String)>) -> Self {
        Self::from_entries(
            pairs
                .into_iter()
                .map(|(connection_id, db_session_id)| ActiveConnectionEntry {
                    connection_id,
                    db_session_id,
                })
                .collect(),
        )
    }

    /// `connections[connectionId]?.dbSessionId || undefined` — `None` when
    /// the connection is unknown or disconnected (the empty handle).
    pub fn db_session_id(&self, connection_id: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|entry| entry.connection_id == connection_id)
            .map(|entry| entry.db_session_id.as_str())
            .filter(|id| !id.is_empty())
    }

    /// `Object.values(connections).find(c => c.dbSessionId !== '' &&
    /// c.dbSessionId === wanted)` — the first entry (enumeration order)
    /// whose non-empty runtime handle matches.
    pub fn connection_id_for(&self, db_session_id: &str) -> Option<&str> {
        if db_session_id.is_empty() {
            // The `dbSessionId !== ''` guard can never match an empty target.
            return None;
        }
        self.entries
            .iter()
            .find(|entry| !entry.db_session_id.is_empty() && entry.db_session_id == db_session_id)
            .map(|entry| entry.connection_id.as_str())
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// The sessions.ts operations, bound to their seams. Construction injects
/// the backend, the storage, the clock and the id generator; every public
/// method mirrors one React export.
pub struct AiSessionStore {
    pub(crate) backend: Box<dyn AiSessionBackend>,
    pub(crate) storage: Box<dyn SessionLocalStorage>,
    pub(crate) clock: SessionClock,
    pub(crate) new_id: NewId,
}

impl AiSessionStore {
    pub fn new(
        backend: Box<dyn AiSessionBackend>,
        storage: Box<dyn SessionLocalStorage>,
        clock: SessionClock,
        new_id: NewId,
    ) -> Self {
        Self { backend, storage, clock, new_id }
    }

    pub(crate) fn now_iso(&self) -> String {
        (self.clock.now_iso)()
    }

    /// React `loadSessions()`.
    pub fn load_sessions(&mut self) -> SessionMap {
        load_sessions(&*self.storage)
    }

    /// React `saveSessions(sessions)`.
    pub fn save_sessions(&mut self, sessions: &SessionMap) {
        save_sessions(&*self.storage, sessions);
    }

    /// React `touchSession(sessions, key)`: stamp the key's LRU timestamp.
    ///
    /// React spreads `...existing` over `undefined` when the key is absent,
    /// producing a messages-*less* entry that then throws inside its own
    /// writers (silently caught there, dropping that save). The port keeps
    /// the entry benign instead — empty messages — so the mirror never gets
    /// poisoned; documented deviation, pinned by a test.
    pub fn touch_session(&self, sessions: &mut SessionMap, key: &str) {
        let now = (self.clock.now_ms)();
        match sessions.get_mut(key) {
            Some(session) => session.last_access = Some(now),
            None => sessions.insert(key, StoredSession::new(Vec::new(), now)),
        }
    }

    /// React `localSessionsMigrated()`.
    pub fn local_sessions_migrated(&mut self) -> bool {
        local_sessions_migrated(&*self.storage)
    }

    /// React's private `markLocalSessionsMigrated()`.
    pub(crate) fn mark_local_sessions_migrated(&mut self) {
        let now_iso = self.now_iso();
        mark_local_sessions_migrated(&*self.storage, &now_iso);
    }

    /// React `localChatSessionForScope(key, scope)`: the synchronous
    /// localStorage view used to paint immediately while AppDb loads.
    pub fn local_chat_session_for_scope(
        &mut self,
        key: &str,
        scope: &SessionScope,
    ) -> Option<AiChatSession> {
        let stored = load_sessions(&*self.storage).get(key).cloned()?;
        let now_iso = self.now_iso();
        Some(chat_session_from_stored(key, &stored, scope, &now_iso, &mut self.new_id))
    }

    /// React `newChatSession(scope, key?)`.
    pub fn new_chat_session(&mut self, scope: &SessionScope, key: Option<&str>) -> AiChatSession {
        let now_iso = self.now_iso();
        new_session(scope, key, &now_iso, &mut self.new_id)
    }

    /// React `chatSessionToUpsertInput(session)`.
    pub fn chat_session_to_upsert_input(&self, session: &AiChatSession) -> AiSessionUpsertInput {
        to_upsert_input(session, &self.now_iso())
    }

    /// React `loadChatSession(id)` — the backend read; `Err` is the
    /// rejection React's caller catches.
    pub fn load_chat_session(&mut self, id: &str) -> Result<AiSessionDetail, String> {
        self.backend.sessions_get(id)
    }

    /// React `persistChatSession(session)`: backend-first, then mirrored
    /// into localStorage as the offline fallback — a backend failure never
    /// discards the visible conversation.
    ///
    /// React chains this through `sessionWriteQueue` to serialize writes;
    /// the sync port is serialized by `&mut self` (module docs).
    pub fn persist_chat_session(&mut self, session: &AiChatSession) -> super::model::SessionPersistResult {
        self.persist_chat_session_now(session)
    }

    /// React's private `persistChatSessionNow(session)`.
    fn persist_chat_session_now(
        &mut self,
        session: &AiChatSession,
    ) -> super::model::SessionPersistResult {
        let now_iso = self.now_iso();
        let input = to_upsert_input(session, &now_iso);
        let mut backend_error: Option<String> = None;
        let mut detail: Option<AiSessionDetail> = None;
        match self.backend.sessions_upsert(&input) {
            Ok(returned) => detail = Some(returned),
            Err(message) => backend_error = Some(message),
        }
        // Best-effort localStorage mirror (React's second try/catch): the
        // map write and the save each swallow their own storage errors.
        let mut sessions = load_sessions(&*self.storage);
        sessions.insert(
            session.session_key.clone(),
            StoredSession {
                id: Some(session.id.clone()),
                title: session.title.clone(),
                // React: `session.metadata ?? undefined` — null drops.
                metadata: session.metadata.as_ref().filter(|metadata| !metadata.is_null()).cloned(),
                connection_id: session.connection_id.clone(),
                database: session.database.clone(),
                messages: session.messages.clone(),
                last_access: Some((self.clock.now_ms)()),
            },
        );
        save_sessions(&*self.storage, &sessions);
        match backend_error {
            None => super::model::SessionPersistResult { ok: true, error: None, detail },
            Some(message) => super::model::SessionPersistResult {
                ok: false,
                error: Some(format!("AI session was saved locally only: {message}")),
                detail,
            },
        }
    }

    /// React `deleteChatSession(id, key)`: delete backend-first; the local
    /// mirror goes only when the stored entry is the same session (or has
    /// no id of its own) — a different session's mirror survives.
    pub fn delete_chat_session(&mut self, id: &str, key: &str) -> super::model::SessionDeleteResult {
        let mut error: Option<String> = None;
        match self.backend.sessions_delete(id) {
            Ok(_) => {}
            Err(message) => error = Some(message),
        }
        let mut sessions = load_sessions(&*self.storage);
        // React: `sessions[key]?.id === id || sessions[key]?.id ===
        // undefined` — an absent key short-circuits to the delete branch.
        let stored_id = sessions.get(key).and_then(|stored| stored.id.clone());
        if stored_id.as_deref() == Some(id) || stored_id.is_none() {
            sessions.remove(key);
            save_sessions(&*self.storage, &sessions);
        }
        super::model::SessionDeleteResult {
            ok: error.is_none(),
            error,
        }
    }

    /// React `exportChatSession(session, format)`: the backend export with
    /// the offline fallback serializing the local detail (markdown, or
    /// pretty JSON).
    pub fn export_chat_session(
        &mut self,
        session: &AiChatSession,
        format: ExportFormat,
    ) -> super::model::SessionExport {
        // React builds the local detail (with the current clock) on both
        // paths — the filename always comes from it.
        let detail = local_detail(session, &self.now_iso());
        let filename = export_filename(&detail, format);
        let mime = format.mime().to_string();
        let content = match self.backend.sessions_export(&session.id, format) {
            Ok(content) => content,
            Err(_) => match format {
                ExportFormat::Json => {
                    // JSON.stringify(detail, null, 2); serde's pretty
                    // printer is the 2-space form. Object key order is the
                    // BTree order of `serde_json::Map`, not JS insertion
                    // order — semantically equal, byte order differs.
                    serde_json::to_string_pretty(&detail).unwrap_or_default()
                }
                ExportFormat::Markdown => markdown_for(&detail),
            },
        };
        super::model::SessionExport { content, filename, mime }
    }
}

#[cfg(test)]
mod tests {
    use super::super::model::{AiMessageRole, AiMessageStatus, SessionScope};
    use super::super::test_support::{env, fixture_session, NOW_ISO};
    use super::*;

    #[test]
    fn load_and_save_round_trip_through_the_seams() {
        let mut test = env();
        let store = &mut test.store;
        assert!(store.load_sessions().is_empty());
        let mut sessions = SessionMap::new();
        sessions.insert("k", StoredSession::new(Vec::new(), 7.0));
        store.save_sessions(&sessions);
        let loaded = store.load_sessions();
        assert_eq!(loaded.get("k").and_then(|session| session.last_access), Some(7.0));
    }

    #[test]
    fn touch_updates_the_lru_timestamp() {
        // React suite 'updates lastAccess timestamp'.
        let test = env();
        let mut sessions = SessionMap::new();
        sessions.insert("k", StoredSession::new(Vec::new(), 100.0));
        test.store.touch_session(&mut sessions, "k");
        let touched = sessions.get("k").and_then(|session| session.last_access);
        assert!(touched.unwrap_or(0.0) > 100.0);
    }

    #[test]
    fn touch_on_an_absent_key_inserts_a_benign_entry() {
        // Documented deviation (method docs): React's spread-of-undefined
        // leaves a messages-less entry that poisons its own writers; the
        // port inserts the empty-messages equivalent so the mirror stays
        // writable.
        let mut test = env();
        let mut sessions = SessionMap::new();
        test.store.touch_session(&mut sessions, "absent");
        let session = sessions.get("absent").cloned().unwrap_or_default();
        assert!(session.messages.is_empty());
        assert_eq!(session.last_access, Some(1000.0));
        // And the entry does not poison a subsequent save.
        test.store.touch_session(&mut sessions, "absent");
        test.store.save_sessions(&sessions);
        assert!(test.store.load_sessions().get("absent").is_some());
    }

    #[test]
    fn persist_writes_the_backend_then_mirrors_locally() {
        let mut test = env();
        let store = &mut test.store;
        let session = fixture_session();
        let result = store.persist_chat_session(&session);
        assert!(result.ok);
        assert_eq!(result.error, None);
        assert!(result.detail.is_some());
        // One upsert with the full replace payload.
        let upserts = test.backend.upsert_inputs();
        assert_eq!(upserts.len(), 1);
        assert_eq!(upserts[0].id.as_deref(), Some("s1"));
        assert_eq!(upserts[0].session_key, "conn-1::db");
        assert_eq!(upserts[0].append, Some(false));
        // The mirror: id, title, messages, fresh lastAccess from the clock.
        let local = store.load_sessions();
        let mirror = local.get("conn-1::db").cloned().unwrap_or_default();
        assert_eq!(mirror.id.as_deref(), Some("s1"));
        assert_eq!(mirror.messages.len(), 3);
        assert_eq!(mirror.last_access, Some(1000.0));
        assert!(test.raw_storage().is_some());
    }

    #[test]
    fn persist_mirrors_to_localstorage_when_the_backend_write_fails() {
        // React suite 'mirrors to localStorage when the backend write fails'.
        let mut test = env();
        test.backend.program_upsert_error("db locked");
        let store = &mut test.store;
        let result = store.persist_chat_session(&fixture_session());
        assert!(!result.ok);
        assert!(result.error.as_deref().unwrap_or_default().contains("db locked"));
        assert_eq!(
            result.error.as_deref(),
            Some("AI session was saved locally only: db locked")
        );
        assert_eq!(result.detail, None);
        let local = store.load_sessions();
        let mirror = local.get("conn-1::db").cloned().unwrap_or_default();
        assert_eq!(mirror.id.as_deref(), Some("s1"));
        assert_eq!(mirror.messages[1].status, Some(AiMessageStatus::Cancelled));
    }

    #[test]
    fn persist_replaces_the_mirror_for_the_same_key() {
        let mut test = env();
        let store = &mut test.store;
        // Seed a different session under the same key, then overwrite.
        let mut first = fixture_session();
        first.id = "old".to_string();
        first.title = Some("First".to_string());
        store.persist_chat_session(&first);
        let mut second = fixture_session();
        second.title = Some("Second".to_string());
        store.persist_chat_session(&second);
        let mirror = store.load_sessions().get("conn-1::db").cloned().unwrap_or_default();
        assert_eq!(mirror.title.as_deref(), Some("Second"));
        // The mirror never outgrows the 50-message cap through saves.
        let mut chatty = fixture_session();
        chatty.messages = (0..60)
            .map(|index| {
                let mut message = super::super::model::AiChatMessage::new(AiMessageRole::User, "m");
                message.id = Some(format!("g{index}"));
                message
            })
            .collect();
        store.persist_chat_session(&chatty);
        let mirror = store.load_sessions().get("conn-1::db").cloned().unwrap_or_default();
        assert_eq!(mirror.messages.len(), 50);
    }

    #[test]
    fn delete_removes_the_backend_row_and_the_matching_mirror() {
        let mut test = env();
        let store = &mut test.store;
        store.persist_chat_session(&fixture_session());
        let result = store.delete_chat_session("s1", "conn-1::db");
        assert!(result.ok);
        assert_eq!(result.error, None);
        assert_eq!(test.backend.delete_ids(), vec!["s1".to_string()]);
        assert!(store.load_sessions().get("conn-1::db").is_none());
    }

    #[test]
    fn delete_keeps_a_mirror_that_belongs_to_a_different_session() {
        let mut test = env();
        let store = &mut test.store;
        store.persist_chat_session(&fixture_session());
        // Deleting another id leaves the stored mirror (its id is s1, not
        // the deleted id) — no cross-session deletion.
        let result = store.delete_chat_session("other-id", "conn-1::db");
        assert!(result.ok);
        assert!(store.load_sessions().get("conn-1::db").is_some());
        // A stored entry with no id of its own is deletable by any id.
        let mut sessions = SessionMap::new();
        sessions.insert("anonymous", StoredSession::new(Vec::new(), 1.0));
        store.save_sessions(&sessions);
        let result = store.delete_chat_session("whatever", "anonymous");
        assert!(result.ok);
        assert!(store.load_sessions().get("anonymous").is_none());
    }

    #[test]
    fn delete_on_an_unknown_key_leaves_the_other_mirrors_alone() {
        // React's condition (`?.id === id || ?.id === undefined`) treats an
        // absent key as deletable — a no-op delete that still re-saves the
        // unchanged map; the surviving mirrors are untouched either way.
        let mut test = env();
        let store = &mut test.store;
        store.persist_chat_session(&fixture_session());
        let result = store.delete_chat_session("s1", "missing-key");
        assert!(result.ok);
        assert!(store.load_sessions().get("conn-1::db").is_some());
    }

    #[test]
    fn delete_reports_the_backend_error_to_the_caller() {
        struct FailingDelete;
        impl AiSessionBackend for FailingDelete {
            fn sessions_list(
                &self,
                _params: &SessionListParams,
            ) -> Result<Vec<AiSessionSummary>, String> {
                Ok(Vec::new())
            }
            fn sessions_get(&self, _id: &str) -> Result<AiSessionDetail, String> {
                Err("unused".to_string())
            }
            fn sessions_upsert(
                &self,
                _input: &AiSessionUpsertInput,
            ) -> Result<AiSessionDetail, String> {
                Err("unused".to_string())
            }
            fn sessions_delete(&self, _id: &str) -> Result<bool, String> {
                Err("row locked".to_string())
            }
            fn sessions_export(
                &self,
                _id: &str,
                _format: ExportFormat,
            ) -> Result<String, String> {
                Err("unused".to_string())
            }
        }
        let clock = SessionClock {
            now_ms: Box::new(|| 0.0),
            now_iso: Box::new(|| NOW_ISO.to_string()),
        };
        let mut store = AiSessionStore::new(
            Box::new(FailingDelete),
            Box::new(super::super::test_support::SharedStorage(std::rc::Rc::new(
                super::super::test_support::MemoryStorage::default(),
            ))),
            clock,
            Box::new(|| "id".to_string()),
        );
        let result = store.delete_chat_session("s1", "conn-1::db");
        assert!(!result.ok);
        assert_eq!(result.error.as_deref(), Some("row locked"));
    }

    #[test]
    fn export_uses_the_backend_content_and_the_local_detail_filename() {
        // React suite 'exports backend JSON/Markdown and falls back locally
        // offline' (the backend halves).
        let mut test = env();
        test.backend.program_export_content("{\"id\":\"s1\"}");
        let store = &mut test.store;
        let json = store.export_chat_session(&fixture_session(), ExportFormat::Json);
        assert_eq!(json.content, "{\"id\":\"s1\"}");
        assert_eq!(json.filename, "Saved-conversation.json");
        assert_eq!(json.mime, "application/json");
        assert_eq!(test.backend.export_ids(), vec![("s1".to_string(), ExportFormat::Json)]);
    }

    #[test]
    fn export_falls_back_to_the_local_markdown_and_json_when_offline() {
        // React suite (the offline halves).
        let mut test = env();
        test.backend.program_export_error("offline");
        let store = &mut test.store;
        let markdown = store.export_chat_session(&fixture_session(), ExportFormat::Markdown);
        assert!(markdown.content.contains("## Assistant (cancelled)"));
        assert!(markdown.content.contains("## Assistant (failed)"));
        assert_eq!(markdown.filename, "Saved-conversation.md");
        assert_eq!(markdown.mime, "text/markdown");

        let json = store.export_chat_session(&fixture_session(), ExportFormat::Json);
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&json.content);
        assert!(parsed.is_ok());
        let parsed = parsed.unwrap_or(serde_json::Value::Null);
        assert_eq!(parsed.get("id").and_then(serde_json::Value::as_str), Some("s1"));
        assert_eq!(json.filename, "Saved-conversation.json");
        // Both paths build the detail with the injected clock.
        assert!(json.content.contains("2026-02-02T00:00:00.000Z"));
        assert!(markdown.content.contains("2026-02-02T00:00:00.000Z"));
    }

    #[test]
    fn local_chat_session_for_scope_reads_the_synchronous_view() {
        let mut test = env();
        test.seed_storage(&super::super::test_support::legacy_local_json(false));
        let store = &mut test.store;
        let scope = SessionScope::new(Some("conn-1".to_string()), None, Some("db".to_string()));
        // An unstored key reads as absent.
        assert!(store.local_chat_session_for_scope("missing::key", &scope).is_none());
        let found =
            store.local_chat_session_for_scope("db-sess::db", &SessionScope::default());
        assert!(found.is_some());
        let found = found.unwrap_or_default();
        assert_eq!(found.messages.len(), 1);
        assert_eq!(found.messages[0].content, "old question");
        // The scope backfills the connection/database when the stored entry
        // is silent.
        assert_eq!(found.connection_id, None);
        let scoped = store
            .local_chat_session_for_scope(
                "db-sess::db",
                &SessionScope::new(Some("conn-1".to_string()), None, Some("db".to_string())),
            )
            .unwrap_or_default();
        assert_eq!(scoped.connection_id.as_deref(), Some("conn-1"));
        assert_eq!(scoped.database.as_deref(), Some("db"));
    }

    #[test]
    fn new_chat_session_mints_ids_and_stamps_the_started_at_metadata() {
        let mut test = env();
        let store = &mut test.store;
        let scope = SessionScope::new(Some("conn-1".to_string()), None, Some("db".to_string()));
        let session = store.new_chat_session(&scope, None);
        assert_eq!(session.id, "gen-1");
        assert_eq!(
            session.metadata,
            Some(serde_json::json!({ "startedAt": NOW_ISO }))
        );
    }

    #[test]
    fn to_upsert_input_and_load_chat_session_pass_through_the_seams() {
        let mut test = env();
        let input = test.store.chat_session_to_upsert_input(&fixture_session());
        assert_eq!(input.last_access_at.as_deref(), Some(NOW_ISO));
        test.backend.program_get_detail(super::super::test_support::fixture_detail());
        let store = &mut test.store;
        match store.load_chat_session("s1") {
            Ok(detail) => {
                assert_eq!(detail.id, "s1");
                assert_eq!(detail.messages.len(), 3);
            }
            Err(message) => panic!("expected the programmed detail, got {message}"),
        }
        test.backend.program_get_error("not found");
        assert!(store.load_chat_session("nope").is_err());
    }

    #[test]
    fn active_connections_lookups_mirror_the_react_store_shape() {
        let active = ActiveConnections::from_pairs(vec![
            ("conn-1".to_string(), "db-sess".to_string()),
            ("conn-2".to_string(), String::new()),
        ]);
        // Forward: known + connected → the handle; unknown → None.
        assert_eq!(active.db_session_id("conn-1"), Some("db-sess"));
        // Disconnected (empty handle) reads as absent (`|| undefined`).
        assert_eq!(active.db_session_id("conn-2"), None);
        assert_eq!(active.db_session_id("conn-9"), None);
        // Reverse: first entry whose non-empty handle matches.
        assert_eq!(active.connection_id_for("db-sess"), Some("conn-1"));
        assert_eq!(active.connection_id_for("db-other"), None);
        // An empty handle can never be matched (the `!== ''` guard).
        assert_eq!(active.connection_id_for(""), None);
        assert!(ActiveConnections::default().is_empty());
    }
}
