//! Backend-first restore: legacy migration, chat restore and the session
//! list — the `migrateLocalSessionsToBackend` / `restoreChatSession` /
//! `listChatSessions` half of React `stores/ai/sessions.ts`.
//!
//! All three read the **live** connection map (`useActiveConnectionStore
//! .getState().connections`) at call time, so each takes an
//! [`ActiveConnections`](super::store::ActiveConnections) snapshot argument;
//! everything else comes from the store's seams. The await points become
//! `Result` propagation: React's try/catch ladder is reproduced branch for
//! branch (see [`restore_chat_session`](Self::restore_chat_session)'s docs).

use super::local::{StoredSession, load_sessions, local_sessions_migrated};
use super::mapping::{
    chat_session_from_detail, chat_session_from_stored, message_metadata, message_status,
    new_chat_session as new_session,
};
use super::model::{
    AiSessionDetail, AiSessionSummary, AiSessionUpsertInput, MigrationOutcome,
    SessionRestoreResult, SessionScope, SessionKey, compute_session_key, iso_from_epoch_ms,
    split_legacy_key, AiSessionMessageInput,
};
use super::store::{ActiveConnections, AiSessionStore, SessionListParams};

/// React's private `legacyTargetKey` return shape.
struct LegacyTarget {
    connection_id: Option<String>,
    session_key: SessionKey,
}

/// React's private `normalizeDatabase(value)`: `value ?? ''` — the null/absent
/// fold the legacy lookup compares by.
fn normalize_database(value: Option<&str>) -> String {
    value.unwrap_or("").to_string()
}

impl AiSessionStore {
    /// React's private `listForScope(key, scope)`: the backend list with the
    /// `?? ''` spelling for the scope's connection/database.
    fn list_for_scope(
        &mut self,
        key: &str,
        scope: &SessionScope,
    ) -> Result<Vec<AiSessionSummary>, String> {
        let params = SessionListParams {
            connection_id: Some(scope.connection_id.clone().unwrap_or_default()),
            database: Some(scope.database.clone().unwrap_or_default()),
            session_key: Some(key.to_string()),
        };
        self.backend.sessions_list(&params)
    }

    /// React's private `activeDbSessionId(scope)`: the scope's own runtime
    /// handle, else the live connection map's current handle for the scope's
    /// connection (absent/disconnected → `None`).
    fn active_db_session_id(scope: &SessionScope, active: &ActiveConnections) -> Option<String> {
        if let Some(db_session_id) = scope.db_session_id.as_deref().filter(|id| !id.is_empty()) {
            return Some(db_session_id.to_string());
        }
        let connection_id = scope.connection_id.as_deref().filter(|id| !id.is_empty())?;
        active.db_session_id(connection_id).map(str::to_string)
    }

    /// React `migrateLocalSessionsToBackend(active)`: move every legacy
    /// localStorage session into the AppDb once.
    ///
    /// Deterministic ids (`legacy:{key}`, `legacy:{key}:{index}`) make a
    /// partially failed migration re-runnable idempotently; the old key is
    /// retained and only marked after **every** row succeeded, so a failed
    /// migration stays readable in the fallback path. An empty localStorage
    /// never writes the marker — React returns early before marking, so an
    /// empty store re-runs this as a no-op forever.
    ///
    /// React wraps the body in one try/catch whose catch keeps the partial
    /// count and returns the user-facing error; the sync port returns the
    /// same outcome directly at each failure point (backend error, or the
    /// stored `lastAccess` being finite-but-out-of-`Date`-range, where React's
    /// `toISOString()` throws `RangeError: Invalid time value`).
    pub fn migrate_local_sessions_to_backend(
        &mut self,
        active: &ActiveConnections,
    ) -> MigrationOutcome {
        if local_sessions_migrated(&*self.storage) {
            return MigrationOutcome { migrated: 0, error: None };
        }
        let sessions = load_sessions(&*self.storage);
        if sessions.is_empty() {
            return MigrationOutcome { migrated: 0, error: None };
        }
        let mut migrated: usize = 0;
        for (key, stored) in sessions.iter() {
            let scope = split_legacy_key(key);
            let target = legacy_target_key(key, scope.database.as_deref(), active);
            // `Number.isFinite(stored.lastAccess) ? new Date(stored.
            // lastAccess).toISOString() : new Date().toISOString()`.
            let last_access_iso = match stored.last_access.filter(|ms| ms.is_finite()) {
                Some(ms) => match iso_from_epoch_ms(ms) {
                    Some(iso) => iso,
                    // Finite but outside Date's range: React's toISOString
                    // throws here and the catch aborts with V8's message.
                    None => {
                        return MigrationOutcome {
                            migrated,
                            error: Some(
                                "AI session migration failed; using local history: Invalid time value"
                                    .to_string(),
                            ),
                        };
                    }
                },
                None => self.now_iso(),
            };
            let input = AiSessionUpsertInput {
                id: Some(format!("legacy:{key}")),
                session_key: target.session_key,
                connection_id: target.connection_id,
                // React writes `database: scope.database` here — the legacy
                // key's own database, not the rebound `databaseName`.
                database: scope.database.clone(),
                title: stored.title.clone(),
                metadata: stored.metadata.clone(),
                last_access_at: Some(last_access_iso.clone()),
                // The migration omits `append` (backend default), unlike
                // chatSessionToUpsertInput's explicit false.
                append: None,
                messages: stored
                    .messages
                    .iter()
                    .enumerate()
                    .map(|(index, message)| AiSessionMessageInput {
                        id: Some(
                            message
                                .id
                                .clone()
                                .unwrap_or_else(|| format!("legacy:{key}:{index}")),
                        ),
                        role: message.role,
                        content: message.content.clone(),
                        status: Some(message_status(message)),
                        created_at: Some(
                            message.created_at.clone().unwrap_or_else(|| last_access_iso.clone()),
                        ),
                        metadata: message_metadata(message),
                    })
                    .collect(),
            };
            if let Err(message) = self.backend.sessions_upsert(&input) {
                return MigrationOutcome {
                    migrated,
                    error: Some(format!("AI session migration failed; using local history: {message}")),
                };
            }
            migrated += 1;
        }
        self.mark_local_sessions_migrated();
        MigrationOutcome { migrated, error: None }
    }

    /// React `restoreChatSession(key, scope, active)` — never fails; every
    /// backend error folds into the returned result. The branch ladder, in
    /// React order:
    ///
    /// 1. run the legacy migration once; its error/success is carried (an
    ///    error zeroes the success count — `if (!migration.error)
    ///    migrationSuccess = migration.migrated`);
    /// 2. list the backend for the key — an error, or a `sessionsGet` error
    ///    on the first summary, drops to the **outer catch**: localStorage
    ///    fallback (or a new session), `backendAvailable: false`, and the
    ///    migration error kept if it was already set (else the
    ///    "backend unavailable" message);
    /// 3. a backend row wins: the detail restores the live session;
    /// 4. else the legacy `dbSessionId::database` lookup — its *own* error is
    ///    swallowed into `migrationError ??= "AI legacy session lookup
    ///    failed: …"` and the ladder continues; a hit is rebound under the
    ///    current key, persisted (backend-first) and re-listed (a re-list
    ///    error degrades to empty summaries), with a persist failure
    ///    reported as "found but could not be bound";
    /// 5. else — backend reachable, no row — a local-only conversation
    ///    survives (e.g. written while the backend was down) instead of
    ///    being discarded, or a fresh session is minted.
    pub fn restore_chat_session(
        &mut self,
        key: &str,
        scope: &SessionScope,
        active: &ActiveConnections,
    ) -> SessionRestoreResult {
        let mut migration_error: Option<String>;
        let mut migration_success: usize = 0;
        let migration = self.migrate_local_sessions_to_backend(active);
        migration_error = migration.error;
        if migration_error.is_none() {
            migration_success = migration.migrated;
        }

        let summaries = match self.list_for_scope(key, scope) {
            Ok(summaries) => summaries,
            Err(message) => {
                return self.restore_unavailable_fallback(
                    key,
                    scope,
                    migration_error,
                    migration_success,
                    message,
                );
            }
        };
        if !summaries.is_empty() {
            match self.backend.sessions_get(&summaries[0].id) {
                Ok(detail) => {
                    return SessionRestoreResult {
                        session: Some(chat_session_from_detail(&detail)),
                        summaries,
                        migration_error,
                        migration_success,
                        backend_available: true,
                    };
                }
                Err(message) => {
                    return self.restore_unavailable_fallback(
                        key,
                        scope,
                        migration_error,
                        migration_success,
                        message,
                    );
                }
            }
        }

        let legacy = match self.find_legacy_detail_for_scope(key, scope, active) {
            Ok(legacy) => legacy,
            Err(message) => {
                // React: `migrationError ??= 'AI legacy session lookup
                // failed: …'` — only fills when unset.
                if migration_error.is_none() {
                    migration_error =
                        Some(format!("AI legacy session lookup failed: {message}"));
                }
                None
            }
        };
        if let Some(legacy) = legacy {
            let mut rebound = chat_session_from_detail(&legacy);
            rebound.session_key = key.to_string();
            rebound.connection_id = scope.connection_id.clone();
            rebound.database = scope.database.clone();
            let saved = self.persist_chat_session(&rebound);
            // React: `(await listForScope(...).catch(() => [])) ?? []`.
            let summaries = self.list_for_scope(key, scope).unwrap_or_default();
            return SessionRestoreResult {
                migration_error: if saved.ok {
                    migration_error
                } else {
                    Some(format!(
                        "AI legacy session was found but could not be bound: {}",
                        saved.error.as_deref().unwrap_or("unknown error")
                    ))
                },
                session: Some(rebound),
                summaries,
                migration_success,
                backend_available: true,
            };
        }

        // Backend reachable but no row: keep a local-only conversation.
        let local = load_sessions(&*self.storage).get(key).cloned();
        let session = self.session_from_local_or_fresh(key, scope, local.as_ref());
        SessionRestoreResult {
            session: Some(session),
            summaries: Vec::new(),
            migration_error,
            migration_success,
            backend_available: true,
        }
    }

    /// React `listChatSessions(scope, key?, active)`.
    ///
    /// The first list's error propagates to the caller (React's function
    /// rejects); when the key has rows the list is returned as-is; a
    /// missing-connection scope or a scope with no resolvable runtime
    /// handle returns the (empty) list without legacy work. Otherwise the
    /// legacy row is looked up, rebound, persisted and re-listed — any
    /// failure of that inner ladder returns the original summaries.
    pub fn list_chat_sessions(
        &mut self,
        scope: &SessionScope,
        key: Option<&str>,
        active: &ActiveConnections,
    ) -> Result<Vec<AiSessionSummary>, String> {
        let derived_key;
        let target_key = match key {
            Some(key) => key,
            None => {
                derived_key = compute_session_key(
                    scope.connection_id.as_deref(),
                    scope.db_session_id.as_deref(),
                    scope.database.as_deref(),
                );
                derived_key.as_str()
            }
        };
        let summaries = self.list_for_scope(target_key, scope)?;
        let connection_id = scope.connection_id.as_deref().filter(|id| !id.is_empty());
        if !summaries.is_empty()
            || connection_id.is_none()
            || Self::active_db_session_id(scope, active).is_none()
        {
            return Ok(summaries);
        }
        let legacy = match self.find_legacy_detail_for_scope(target_key, scope, active) {
            Ok(legacy) => legacy,
            Err(_) => return Ok(summaries.clone()),
        };
        let Some(legacy) = legacy else {
            return Ok(summaries.clone());
        };
        let mut rebound = chat_session_from_detail(&legacy);
        rebound.session_key = target_key.to_string();
        // Guarded truthy above (`!scope.connectionId` returned early).
        rebound.connection_id = connection_id.map(str::to_string);
        rebound.database = scope.database.clone();
        let saved = self.persist_chat_session(&rebound);
        if !saved.ok {
            return Ok(summaries.clone());
        }
        match self.list_for_scope(target_key, scope) {
            Ok(listed) => Ok(listed),
            // The inner `catch` returns the original summaries, not the
            // re-list's — a distinction React makes with `.catch` on the
            // outer try.
            Err(_) => Ok(summaries),
        }
    }

    /// React's private `findLegacyDetailForScope(key, scope, active)`: find
    /// an AppDb row still stored under the old `dbSessionId::database` key
    /// and confirm it belongs to the current database/session before
    /// adopting it. The query deliberately ignores `connection_id`
    /// (migration ran before the connection was known) — the key and the
    /// database are the identity proof; a bound connection that matches
    /// neither the runtime handle nor the current connection rejects.
    ///
    /// `Err` is React's awaited throw — callers catch it per their own
    /// try/catch shape (restore: into `migrationError`; list: swallowed).
    fn find_legacy_detail_for_scope(
        &mut self,
        key: &str,
        scope: &SessionScope,
        active: &ActiveConnections,
    ) -> Result<Option<AiSessionDetail>, String> {
        let db_session_id = Self::active_db_session_id(scope, active);
        let connection_id = scope.connection_id.as_deref().filter(|id| !id.is_empty());
        // React: `if (!scope.connectionId || !dbSessionId) return null`.
        let (connection_id, db_session_id) = match (connection_id, db_session_id.as_deref()) {
            (Some(connection), Some(db_session)) => (connection, db_session),
            _ => return Ok(None),
        };
        let legacy_key = compute_session_key(None, Some(db_session_id), scope.database.as_deref());
        if legacy_key == key {
            return Ok(None);
        }
        let params = SessionListParams {
            connection_id: None,
            database: None,
            session_key: Some(legacy_key.clone()),
        };
        let summaries = self.backend.sessions_list(&params)?;
        let summary = summaries.into_iter().find(|candidate| {
            candidate.session_key == legacy_key
                && normalize_database(candidate.database.as_deref())
                    == normalize_database(scope.database.as_deref())
        });
        let Some(summary) = summary else {
            return Ok(None);
        };
        let detail = self.backend.sessions_get(&summary.id)?;
        if detail.session_key != legacy_key {
            return Ok(None);
        }
        if normalize_database(detail.database.as_deref())
            != normalize_database(scope.database.as_deref())
        {
            return Ok(None);
        }
        if let Some(bound) = detail
            .connection_id
            .as_deref()
            .filter(|connection| !connection.is_empty())
        {
            if bound != db_session_id && bound != connection_id {
                return Ok(None);
            }
        }
        Ok(Some(detail))
    }

    /// React's outer catch in `restoreChatSession`: the localStorage fallback
    /// (or a new session) with `backendAvailable: false`. The pre-existing
    /// migration error wins over the "backend unavailable" message
    /// (`migrationError ?? …`).
    fn restore_unavailable_fallback(
        &mut self,
        key: &str,
        scope: &SessionScope,
        migration_error: Option<String>,
        migration_success: usize,
        message: String,
    ) -> SessionRestoreResult {
        let migration_error = migration_error
            .or_else(|| Some(format!("AI session backend unavailable; using local history: {message}")));
        let local = load_sessions(&*self.storage).get(key).cloned();
        let session = self.session_from_local_or_fresh(key, scope, local.as_ref());
        SessionRestoreResult {
            session: Some(session),
            summaries: Vec::new(),
            migration_error,
            migration_success,
            backend_available: false,
        }
    }

    /// The shared fallback body: `local ? chatSessionFromStored(...) :
    /// newChatSession(scope, key)`.
    fn session_from_local_or_fresh(
        &mut self,
        key: &str,
        scope: &SessionScope,
        local: Option<&StoredSession>,
    ) -> super::model::AiChatSession {
        let now_iso = self.now_iso();
        match local {
            Some(stored) => {
                chat_session_from_stored(key, stored, scope, &now_iso, &mut self.new_id)
            }
            None => new_session(scope, Some(key), &now_iso, &mut self.new_id),
        }
    }
}

/// React's private `legacyTargetKey(key, database, active)`: where a legacy
/// row should land.
///
/// If the legacy key's first segment is a runtime `dbSessionId` the live
/// connection map already maps to a persistent connection, the row migrates
/// directly under the new key (`{connectionId}::{dbSessionId}:{database}`
/// via `computeSessionKey(entry.connectionId, legacyConnection,
/// databaseName)`); without a live mapping the row stays under the legacy
/// key with its first segment as the connection id, so restore can bind it
/// later once the session id is known.
fn legacy_target_key(key: &str, database: Option<&str>, active: &ActiveConnections) -> LegacyTarget {
    // JS `key.split('::')` + two-element destructure: only the first two
    // segments are read.
    let mut segments = key.split("::");
    let legacy_connection = segments.next().unwrap_or("").to_string();
    let legacy_database = segments.next().unwrap_or("").to_string();
    // React: `database ?? legacyDatabase` — the caller's scope database
    // (undefined for empty legacy segments) falls back to the legacy one.
    let database_name = database.map(str::to_string).unwrap_or(legacy_database);
    match active.connection_id_for(&legacy_connection) {
        Some(entry_connection) => LegacyTarget {
            connection_id: Some(entry_connection.to_string()),
            session_key: compute_session_key(
                Some(entry_connection),
                Some(&legacy_connection),
                Some(&database_name),
            ),
        },
        None => LegacyTarget {
            // React: `legacyConnection || undefined`.
            connection_id: if legacy_connection.is_empty() {
                None
            } else {
                Some(legacy_connection.clone())
            },
            session_key: key.to_string(),
        },
    }
}

/// SessionMap entries walk in insertion order — pinned by the tests
/// below via `SessionMap::keys`.

#[cfg(test)]
mod tests {
    use super::super::local::SessionMap;
    use super::super::model::{AiMessageRole, AiMessageStatus, ExportFormat, SessionScope};
    use super::super::store::{AiSessionBackend, SessionClock, SessionListParams};
    use super::super::test_support::{NOW_ISO, env, fixture_summary, legacy_local_json};
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    /// The rebind suite's active-connection map.
    fn active_conn_1() -> ActiveConnections {
        ActiveConnections::from_pairs(vec![("conn-1".to_string(), "db-sess".to_string())])
    }

    fn scope(connection: Option<&str>, db_session: Option<&str>, database: Option<&str>) -> SessionScope {
        SessionScope::new(
            connection.map(str::to_string),
            db_session.map(str::to_string),
            database.map(str::to_string),
        )
    }

    #[test]
    fn restores_cancelled_failed_messages_and_memory_metadata_from_app_db() {
        // React suite 'restores cancelled/failed messages and memory
        // metadata from AppDb'.
        let mut test = env();
        test.backend.program_list_summaries(vec![fixture_summary()]);
        test.backend.program_get_detail(super::super::test_support::fixture_detail());
        let store = &mut test.store;
        let restored =
            store.restore_chat_session("conn-1::db", &scope(Some("conn-1"), None, Some("db")), &ActiveConnections::default());
        assert!(restored.backend_available);
        let session = restored.session.as_ref();
        assert_eq!(session.map(|session| session.id.as_str()), Some("s1"));
        let session = session.cloned().unwrap_or_default();
        assert_eq!(session.messages[1].status, Some(AiMessageStatus::Cancelled));
        assert_eq!(session.messages[2].status, Some(AiMessageStatus::Failed));
        assert_eq!(
            session.messages[0].memory_used,
            Some(serde_json::json!([{ "id": "mem-1" }]))
        );
        assert_eq!(
            session.messages[1].reasoning,
            Some(serde_json::Value::String("thinking".to_string()))
        );
        assert_eq!(restored.migration_error, None);
        assert_eq!(restored.migration_success, 0);
        // The list call carries the `?? ''` spelling of the scope.
        assert_eq!(
            test.backend.list_params(),
            vec![SessionListParams {
                connection_id: Some("conn-1".to_string()),
                database: Some("db".to_string()),
                session_key: Some("conn-1::db".to_string()),
            }]
        );
    }

    #[test]
    fn keeps_a_local_only_conversation_when_the_backend_is_reachable_but_empty() {
        // React suite (same name). The migration write fails but the
        // restore still lands on the local draft with backendAvailable true.
        let mut test = env();
        test.seed_storage(&serde_json::to_string(&serde_json::json!({
            "conn-1::db": { "messages": [{ "role": "user", "content": "offline draft" }], "lastAccess": 5 }
        })).unwrap_or_default());
        test.backend.program_upsert_error("write failed");
        let store = &mut test.store;
        let restored =
            store.restore_chat_session("conn-1::db", &scope(Some("conn-1"), None, Some("db")), &ActiveConnections::default());
        assert!(restored.backend_available);
        assert!(restored.summaries.is_empty());
        let session = restored.session.as_ref().cloned().unwrap_or_default();
        assert_eq!(session.messages[0].content, "offline draft");
        // The failed migration is reported but does not fail the restore.
        assert_eq!(
            restored.migration_error.as_deref(),
            Some("AI session migration failed; using local history: write failed")
        );
        assert_eq!(restored.migration_success, 0);
    }

    #[test]
    fn falls_back_to_localstorage_without_cross_connection_bleed_when_backend_fails() {
        // React suite (same name).
        let mut test = env();
        test.seed_storage(&serde_json::to_string(&serde_json::json!({
            "conn-1::db": {
                "messages": [
                    { "role": "user", "content": "one" },
                    { "role": "assistant", "content": "stopped", "cancelled": true }
                ],
                "lastAccess": 5
            },
            "conn-2::db": { "messages": [{ "role": "user", "content": "other connection" }], "lastAccess": 6 }
        })).unwrap_or_default());
        test.backend.program_list_error("offline");
        let store = &mut test.store;
        let restored =
            store.restore_chat_session("conn-1::db", &scope(Some("conn-1"), None, Some("db")), &ActiveConnections::default());
        assert!(!restored.backend_available);
        assert!(restored.summaries.is_empty());
        let session = restored.session.as_ref().cloned().unwrap_or_default();
        let contents: Vec<&str> = session.messages.iter().map(|m| m.content.as_str()).collect();
        assert_eq!(contents, vec!["one", "stopped"]);
        assert_eq!(session.messages[1].status, Some(AiMessageStatus::Cancelled));
        // The migration ran (default upsert) but its error is null, so the
        // outer catch fills the backend-unavailable message.
        assert_eq!(
            restored.migration_error.as_deref(),
            Some("AI session backend unavailable; using local history: offline")
        );
        assert!(restored.migration_error.as_deref().unwrap_or_default().contains("offline"));
        // migrationSuccess survives the backend failure with the count.
        assert_eq!(restored.migration_success, 2);
    }

    #[test]
    fn binds_the_legacy_row_when_the_session_list_is_refreshed_before_chat_restore() {
        // React suite (same name).
        let mut test = env();
        test.seed_storage(&legacy_local_json(false));
        test.backend.install_rows();
        let store = &mut test.store;
        let migrated = store.migrate_local_sessions_to_backend(&ActiveConnections::default());
        assert_eq!(migrated.migrated, 1);
        assert_eq!(migrated.error, None);

        let listed = store.list_chat_sessions(
            &scope(Some("conn-1"), Some("db-sess"), Some("db")),
            Some("conn-1::db"),
            &ActiveConnections::default(),
        );
        let listed = listed.unwrap_or_default();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].session_key, "conn-1::db");
        assert_eq!(listed[0].connection_id.as_deref(), Some("conn-1"));
        assert_eq!(test.backend.row_count(), 1);
    }

    #[test]
    fn migrates_directly_to_the_current_connection_key_when_the_active_map_is_known() {
        // React suite (same name).
        let mut test = env();
        test.seed_storage(&legacy_local_json(false));
        let active = active_conn_1();
        let store = &mut test.store;
        let migrated = store.migrate_local_sessions_to_backend(&active);
        assert_eq!(migrated.migrated, 1);
        assert_eq!(migrated.error, None);
        let upserts = test.backend.upsert_inputs();
        assert_eq!(upserts.len(), 1);
        assert_eq!(upserts[0].id.as_deref(), Some("legacy:db-sess::db"));
        assert_eq!(upserts[0].session_key, "conn-1::db");
        assert_eq!(upserts[0].connection_id.as_deref(), Some("conn-1"));
    }

    #[test]
    fn binds_an_old_db_session_id_key_to_the_current_connection_without_duplicating_it() {
        // React suite (same name).
        let mut test = env();
        test.seed_storage(&legacy_local_json(true));
        test.backend.install_rows();
        let store = &mut test.store;

        let migrated = store.migrate_local_sessions_to_backend(&ActiveConnections::default());
        assert_eq!(migrated.migrated, 1);
        let upserts = test.backend.upsert_inputs();
        assert_eq!(upserts.len(), 1);
        assert_eq!(upserts[0].id.as_deref(), Some("legacy:db-sess::db"));
        assert_eq!(upserts[0].session_key, "db-sess::db");
        assert_eq!(upserts[0].connection_id.as_deref(), Some("db-sess"));

        let restore_scope = scope(Some("conn-1"), Some("db-sess"), Some("db"));
        let first = store.restore_chat_session("conn-1::db", &restore_scope, &ActiveConnections::default());
        let session = first.session.as_ref().cloned().unwrap_or_default();
        let contents: Vec<&str> = session.messages.iter().map(|m| m.content.as_str()).collect();
        assert_eq!(contents, vec!["old question", "old answer"]);
        assert_eq!(session.messages[1].status, Some(AiMessageStatus::Cancelled));
        assert_eq!(session.session_key, "conn-1::db");
        assert_eq!(session.connection_id.as_deref(), Some("conn-1"));
        assert!(first.backend_available);
        let rows = test.backend.rows();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "legacy:db-sess::db");
        assert_eq!(rows[0].session_key, "conn-1::db");
        assert_eq!(rows[0].connection_id.as_deref(), Some("conn-1"));

        let reopened = store.restore_chat_session("conn-1::db", &restore_scope, &ActiveConnections::default());
        let session = reopened.session.as_ref().cloned().unwrap_or_default();
        let contents: Vec<&str> = session.messages.iter().map(|m| m.content.as_str()).collect();
        assert_eq!(contents, vec!["old question", "old answer"]);
        assert_eq!(session.messages[1].status, Some(AiMessageStatus::Cancelled));
        assert_eq!(test.backend.row_count(), 1);
    }

    #[test]
    fn resolves_the_runtime_session_from_the_active_connection_when_scope_lacks_db_session_id() {
        // React suite (same name).
        let mut test = env();
        test.seed_storage(&legacy_local_json(false));
        test.backend.install_rows();
        let store = &mut test.store;
        let migrated = store.migrate_local_sessions_to_backend(&ActiveConnections::default());
        assert_eq!(migrated.migrated, 1);

        let restored = store.restore_chat_session(
            "conn-1::db",
            &scope(Some("conn-1"), None, Some("db")),
            &active_conn_1(),
        );
        let session = restored.session.as_ref().cloned().unwrap_or_default();
        assert_eq!(session.messages[0].content, "old question");
        assert_eq!(session.session_key, "conn-1::db");
    }

    #[test]
    fn does_not_bind_that_legacy_key_to_a_different_connection_or_database() {
        // React suite (same name).
        let mut test = env();
        test.seed_storage(&legacy_local_json(false));
        test.backend.install_rows();
        let store = &mut test.store;
        let migrated = store.migrate_local_sessions_to_backend(&ActiveConnections::default());
        assert_eq!(migrated.migrated, 1);

        let other_connection = store.restore_chat_session(
            "conn-2::db",
            &scope(Some("conn-2"), Some("other-db"), Some("db")),
            &ActiveConnections::default(),
        );
        assert!(other_connection.session.as_ref().cloned().unwrap_or_default().messages.is_empty());

        let other_database = store.restore_chat_session(
            "conn-1::other",
            &scope(Some("conn-1"), Some("db-sess"), Some("other")),
            &ActiveConnections::default(),
        );
        assert!(other_database.session.as_ref().cloned().unwrap_or_default().messages.is_empty());
    }

    #[test]
    fn upserts_deterministic_ids_preserves_status_then_marks_the_old_key() {
        // React suite 'upserts deterministic ids, preserves status, then
        // marks the old key'.
        let mut test = env();
        test.seed_storage(&serde_json::to_string(&serde_json::json!({
            "conn-1::db": {
                "messages": [
                    { "role": "user", "content": "hello" },
                    { "role": "assistant", "content": "done", "cancelled": true },
                    { "role": "assistant", "content": "Error: x", "status": "failed" }
                ],
                "lastAccess": 1000
            }
        })).unwrap_or_default());
        let store = &mut test.store;
        let first = store.migrate_local_sessions_to_backend(&ActiveConnections::default());
        assert_eq!(first.error, None);
        assert_eq!(first.migrated, 1);
        let upserts = test.backend.upsert_inputs();
        assert_eq!(upserts.len(), 1);
        assert_eq!(upserts[0].id.as_deref(), Some("legacy:conn-1::db"));
        assert_eq!(upserts[0].session_key, "conn-1::db");
        assert_eq!(upserts[0].connection_id.as_deref(), Some("conn-1"));
        assert_eq!(upserts[0].database.as_deref(), Some("db"));
        let statuses: Vec<Option<AiMessageStatus>> =
            upserts[0].messages.iter().map(|message| message.status).collect();
        assert_eq!(
            statuses,
            vec![
                Some(AiMessageStatus::Complete),
                Some(AiMessageStatus::Cancelled),
                Some(AiMessageStatus::Failed)
            ]
        );
        // The legacy data survives in localStorage (only the marker moves).
        assert!(test.storage.raw(super::super::local::STORAGE_KEY)
            .as_deref()
            .unwrap_or_default()
            .contains("hello"));
        assert!(test
            .storage
            .raw(super::super::local::MIGRATION_MARKER_KEY)
            .is_some());
        // Idempotent: the marked store does not upsert again.
        let second = store.migrate_local_sessions_to_backend(&ActiveConnections::default());
        assert_eq!(second.migrated, 0);
        assert_eq!(second.error, None);
        assert_eq!(test.backend.upsert_inputs().len(), 1);
    }

    #[test]
    fn keeps_legacy_data_and_reports_failure_when_migration_cannot_write() {
        // React suite (same name).
        let mut test = env();
        test.seed_storage(&serde_json::to_string(&serde_json::json!({
            "conn-1::db": { "messages": [{ "role": "user", "content": "hello" }], "lastAccess": 1 }
        })).unwrap_or_default());
        test.backend.program_upsert_error("write failed");
        test.backend.program_list_error("offline");
        let store = &mut test.store;
        let restored =
            store.restore_chat_session("conn-1::db", &scope(Some("conn-1"), None, Some("db")), &ActiveConnections::default());
        assert!(restored
            .migration_error
            .as_deref()
            .unwrap_or_default()
            .contains("write failed"));
        let session = restored.session.as_ref().cloned().unwrap_or_default();
        assert_eq!(session.messages[0].content, "hello");
        assert!(!restored.backend_available);
        // The marker stays unset and the legacy data stays readable.
        assert!(test.storage.raw(super::super::local::MIGRATION_MARKER_KEY).is_none());
        assert!(test
            .storage
            .raw(super::super::local::STORAGE_KEY)
            .as_deref()
            .unwrap_or_default()
            .contains("hello"));
    }

    #[test]
    fn an_empty_local_store_migrates_nothing_and_never_marks_the_marker() {
        // React's `entries.length === 0` early return precedes the marker
        // write, so an empty store re-runs the (empty) migration forever.
        let mut test = env();
        let store = &mut test.store;
        let migrated = store.migrate_local_sessions_to_backend(&ActiveConnections::default());
        assert_eq!(migrated.migrated, 0);
        assert_eq!(migrated.error, None);
        assert!(test.storage.raw(super::super::local::MIGRATION_MARKER_KEY).is_none());
        assert!(test.backend.upsert_inputs().is_empty());
        // A marked store skips everything, even with data present.
        test.storage.cells.borrow_mut().insert(
            super::super::local::STORAGE_KEY.to_string(),
            legacy_local_json(false),
        );
        test.storage.cells.borrow_mut().insert(
            super::super::local::MIGRATION_MARKER_KEY.to_string(),
            "marked".to_string(),
        );
        let migrated = store.migrate_local_sessions_to_backend(&ActiveConnections::default());
        assert_eq!(migrated.migrated, 0);
        assert!(test.backend.upsert_inputs().is_empty());
    }

    #[test]
    fn migration_upserts_walk_storage_insertion_order() {
        // Object.entries order is the upsert order — the ordered-map mirror
        // keeps it. The seed is a raw JSON string because the `json!`
        // macro itself builds a BTree-backed object (order would be lost
        // before the store ever saw it); a real adapter reads the raw
        // stream in order, which is what the visitor preserves.
        let mut test = env();
        test.seed_storage(
            r#"{"b::db": {"messages": [{"role": "user", "content": "b"}], "lastAccess": 2}, "a::db": {"messages": [{"role": "user", "content": "a"}], "lastAccess": 1}}"#,
        );
        let store = &mut test.store;
        let migrated = store.migrate_local_sessions_to_backend(&ActiveConnections::default());
        assert_eq!(migrated.migrated, 2);
        let ids: Vec<String> = test
            .backend
            .upsert_inputs()
            .iter()
            .filter_map(|input| input.id.clone())
            .collect();
        assert_eq!(ids, vec!["legacy:b::db".to_string(), "legacy:a::db".to_string()]);
    }

    #[test]
    fn migration_converts_a_finite_stored_last_access_and_defaults_the_rest_to_now() {
        let mut test = env();
        test.seed_storage(&serde_json::to_string(&serde_json::json!({
            "conn-1::db": { "messages": [{ "role": "user", "content": "hi" }], "lastAccess": 1000 }
        })).unwrap_or_default());
        let store = &mut test.store;
        store.migrate_local_sessions_to_backend(&ActiveConnections::default());
        let upsert = test.backend.upsert_inputs().first().cloned().unwrap_or_default();
        // new Date(1000).toISOString().
        assert_eq!(
            upsert.last_access_at.as_deref(),
            Some("1970-01-01T00:00:01.000Z")
        );
        // The message's missing createdAt falls back to the same stamp.
        assert_eq!(upsert.messages[0].created_at.as_deref(), Some("1970-01-01T00:00:01.000Z"));

        test.seed_storage(&serde_json::to_string(&serde_json::json!({
            "conn-2::db": { "messages": [{ "role": "user", "content": "hi" }], "lastAccess": null }
        })).unwrap_or_default());
        test.storage.cells.borrow_mut().remove(super::super::local::MIGRATION_MARKER_KEY);
        let store = &mut test.store;
        store.migrate_local_sessions_to_backend(&ActiveConnections::default());
        let upserts = test.backend.upsert_inputs();
        let upsert = upserts.last().cloned().unwrap_or_default();
        // null (non-finite) → new Date().toISOString() = the injected now.
        assert_eq!(upsert.last_access_at.as_deref(), Some(NOW_ISO));
        assert_eq!(upsert.append, None);
    }

    #[test]
    fn an_out_of_range_stored_last_access_aborts_the_migration_like_react_range_error() {
        // Finite-but-out-of-Date-range: toISOString throws in React and the
        // catch aborts with the partial count.
        let mut test = env();
        test.seed_storage(&serde_json::to_string(&serde_json::json!({
            "conn-1::db": { "messages": [{ "role": "user", "content": "a" }], "lastAccess": 1 },
            "conn-2::db": { "messages": [{ "role": "user", "content": "b" }], "lastAccess": 1e20 }
        })).unwrap_or_default());
        let store = &mut test.store;
        let migrated = store.migrate_local_sessions_to_backend(&ActiveConnections::default());
        assert_eq!(migrated.migrated, 1);
        assert_eq!(
            migrated.error.as_deref(),
            Some("AI session migration failed; using local history: Invalid time value")
        );
        assert!(test.marker().is_none());
    }

    #[test]
    fn a_legacy_lookup_failure_fills_the_migration_error_and_continues() {
        // The inner try/catch around findLegacyDetailForScope: its error
        // becomes `AI legacy session lookup failed: …` and the ladder
        // continues to the local-only branch (empty store → new session).
        let mut test = env();
        // One-shot list responses in call order: the main list is empty,
        // the legacy lookup's list fails.
        test.backend.push_list_once_ok(Vec::new());
        test.backend.push_list_once_error("legacy list broke");
        let store = &mut test.store;
        let restored = store.restore_chat_session(
            "conn-1::db",
            &scope(Some("conn-1"), Some("db-sess"), Some("db")),
            &ActiveConnections::default(),
        );
        assert!(restored.backend_available);
        assert_eq!(
            restored.migration_error.as_deref(),
            Some("AI legacy session lookup failed: legacy list broke")
        );
        assert!(restored.session.as_ref().cloned().unwrap_or_default().messages.is_empty());
    }

    #[test]
    fn a_rebound_legacy_session_that_cannot_be_persisted_reports_the_binding_error() {
        // Seed the legacy row directly (migration already marked) so the
        // rebind path hits a failing persist.
        let mut test = env();
        test.seed_storage(&legacy_local_json(false));
        test.backend.install_rows();
        {
            let store = &mut test.store;
            store.migrate_local_sessions_to_backend(&ActiveConnections::default());
        }
        test.backend.program_upsert_error("db locked");
        let store = &mut test.store;
        let restored = store.restore_chat_session(
            "conn-1::db",
            &scope(Some("conn-1"), Some("db-sess"), Some("db")),
            &ActiveConnections::default(),
        );
        assert!(restored.backend_available);
        assert_eq!(
            restored.migration_error.as_deref(),
            Some("AI legacy session was found but could not be bound: AI session was saved locally only: db locked")
        );
        // The rebound session is still the restored one (visible
        // conversation preserved).
        let session = restored.session.as_ref().cloned().unwrap_or_default();
        assert_eq!(session.messages[0].content, "old question");
        assert_eq!(session.session_key, "conn-1::db");
        // The re-list failure degrades to empty summaries, not an error.
        assert!(restored.summaries.is_empty());
    }

    #[test]
    fn a_bound_legacy_row_for_another_connection_is_not_adopted() {
        // The identity proof: a row under the legacy key bound to a third
        // connection (neither the runtime handle nor the current one) is
        // rejected by findLegacyDetailForScope.
        let mut test = env();
        let mut detail = super::super::test_support::fixture_detail();
        detail.id = "legacy:db-sess::db".to_string();
        detail.session_key = "db-sess::db".to_string();
        detail.connection_id = Some("conn-3".to_string());
        detail.database = Some("db".to_string());
        detail.messages = vec![super::super::model::AiSessionMessage {
            id: "m1".to_string(),
            role: AiMessageRole::User,
            content: "old question".to_string(),
            status: AiMessageStatus::Complete,
            metadata: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
        }];
        test.backend.seed_row(detail);
        test.storage
            .cells
            .borrow_mut()
            .insert(super::super::local::MIGRATION_MARKER_KEY.to_string(), "marked".to_string());
        let store = &mut test.store;
        let restored = store.restore_chat_session(
            "conn-1::db",
            &scope(Some("conn-1"), Some("db-sess"), Some("db")),
            &ActiveConnections::default(),
        );
        assert!(restored.backend_available);
        assert!(restored.session.as_ref().cloned().unwrap_or_default().messages.is_empty());
        assert_eq!(restored.migration_error, None);
    }

    #[test]
    fn a_database_mismatch_on_the_legacy_row_is_not_adopted() {
        // The summary matches the legacy key and the scope's database, but
        // the row's database disagrees → no adoption.
        let mut test = env();
        // One-shot list responses in call order: main list empty, legacy
        // list returns the matching summary.
        test.backend.push_list_once_ok(Vec::new());
        let mut summary = fixture_summary();
        summary.id = "legacy:db-sess::db".to_string();
        summary.session_key = "db-sess::db".to_string();
        summary.database = Some("db".to_string());
        test.backend.push_list_once_ok(vec![summary]);
        let mut detail = super::super::test_support::fixture_detail();
        detail.id = "legacy:db-sess::db".to_string();
        detail.session_key = "db-sess::db".to_string();
        detail.database = Some("other".to_string());
        test.backend.program_get_detail(detail);
        test.storage
            .cells
            .borrow_mut()
            .insert(super::super::local::MIGRATION_MARKER_KEY.to_string(), "marked".to_string());
        let store = &mut test.store;
        let restored = store.restore_chat_session(
            "conn-1::db",
            &scope(Some("conn-1"), Some("db-sess"), Some("db")),
            &ActiveConnections::default(),
        );
        assert!(restored.session.as_ref().cloned().unwrap_or_default().messages.is_empty());
        assert!(restored.backend_available);
        assert_eq!(restored.migration_error, None);
    }

    #[test]
    fn a_get_failure_on_the_first_summary_takes_the_outer_catch() {
        // sessionsGet throwing on the summary row → backendAvailable false,
        // local fallback (no local → new session).
        let mut test = env();
        test.backend.program_list_summaries(vec![fixture_summary()]);
        test.backend.program_get_error("row missing");
        let store = &mut test.store;
        let restored =
            store.restore_chat_session("conn-1::db", &scope(Some("conn-1"), None, Some("db")), &ActiveConnections::default());
        assert!(!restored.backend_available);
        assert_eq!(
            restored.migration_error.as_deref(),
            Some("AI session backend unavailable; using local history: row missing")
        );
        assert!(restored.session.as_ref().cloned().unwrap_or_default().messages.is_empty());
        assert!(restored.summaries.is_empty());
    }

    #[test]
    fn list_chat_sessions_without_a_key_derives_it_and_returns_early_without_a_connection() {
        let mut test = env();
        test.backend.program_list_summaries(vec![fixture_summary()]);
        let store = &mut test.store;
        let listed = store.list_chat_sessions(
            &scope(Some("conn-1"), None, Some("db")),
            None,
            &ActiveConnections::default(),
        );
        assert_eq!(listed.as_ref().map(Vec::len), Ok(1));
        // Without a connection the legacy path is skipped entirely — the
        // second call still lists (and early-returns on the summaries).
        let listed = store.list_chat_sessions(
            &scope(None, None, Some("db")),
            None,
            &ActiveConnections::default(),
        );
        assert_eq!(listed.as_ref().map(Vec::len), Ok(1));
        // First call's params: the connection scope with its derived key;
        // second call's: the `?? ''` spelling with the default identity.
        assert_eq!(
            test.backend.list_params()[0],
            SessionListParams {
                connection_id: Some("conn-1".to_string()),
                database: Some("db".to_string()),
                session_key: Some("conn-1::db".to_string()),
            }
        );
        assert_eq!(test.backend.list_params()[1].connection_id, Some(String::new()));
        assert_eq!(
            test.backend.list_params()[1].session_key,
            Some("default::db".to_string())
        );
    }

    #[test]
    fn list_chat_sessions_propagates_the_first_list_error() {
        let mut test = env();
        test.backend.program_list_error("offline");
        let store = &mut test.store;
        let listed = store.list_chat_sessions(&scope(Some("conn-1"), None, Some("db")), None, &ActiveConnections::default());
        assert_eq!(listed.err().as_deref(), Some("offline"));
    }

    #[test]
    fn list_chat_sessions_swallows_the_inner_legacy_failures() {
        // Every failure inside the legacy ladder returns the original
        // (empty) summaries: a lookup error, no legacy row, and a failing
        // persist all keep Ok([]).
        let mut test = env();
        let scope_with_handle = scope(Some("conn-1"), Some("db-sess"), Some("db"));
        // (a) The legacy lookup itself fails.
        test.backend.push_list_once_ok(Vec::new());
        test.backend.push_list_once_error("legacy lookup broke");
        let store = &mut test.store;
        let listed = store.list_chat_sessions(
            &scope_with_handle,
            Some("conn-1::db"),
            &ActiveConnections::default(),
        );
        assert_eq!(listed.as_ref().map(Vec::len), Ok(0));
        // (b) No legacy row in the backend → same early result.
        let listed = store.list_chat_sessions(
            &scope_with_handle,
            Some("conn-1::db"),
            &ActiveConnections::default(),
        );
        assert_eq!(listed.as_ref().map(Vec::len), Ok(0));
        // (c) A legacy row is found but the rebind persist fails → the
        // original summaries come back, not an error.
        let mut legacy = super::super::test_support::fixture_detail();
        legacy.id = "legacy:db-sess::db".to_string();
        legacy.session_key = "db-sess::db".to_string();
        legacy.connection_id = Some("db-sess".to_string());
        legacy.database = Some("db".to_string());
        test.backend.seed_row(legacy);
        test.backend.install_rows();
        test.backend.program_upsert_error("db locked");
        let listed = store.list_chat_sessions(
            &scope_with_handle,
            Some("conn-1::db"),
            &ActiveConnections::default(),
        );
        assert_eq!(listed.as_ref().map(Vec::len), Ok(0));
        // No upsert logged by the failed persist path? It is — but the
        // rebind never re-lists, and the caller sees the original list.
        assert!(test.backend.upsert_inputs().len() >= 1);
    }

    #[test]
    fn legacy_target_key_keeps_the_legacy_key_without_a_live_mapping() {
        // Without the active mapping the row stays under the legacy key with
        // the first segment as connectionId; '' collapses to None.
        let target = legacy_target_key("db-sess::db", Some("db"), &ActiveConnections::default());
        assert_eq!(target.connection_id.as_deref(), Some("db-sess"));
        assert_eq!(target.session_key, "db-sess::db");
        // The database falls back to the legacy segment when the scope is
        // silent (splitLegacyKey's `|| undefined`).
        let target = legacy_target_key("db-sess::db", None, &ActiveConnections::default());
        assert_eq!(target.session_key, "db-sess::db");
        let target = legacy_target_key("db-sess::", None, &ActiveConnections::default());
        assert_eq!(target.session_key, "db-sess::");
        // An empty connection segment maps to None (React's `|| undefined`).
        let target = legacy_target_key("::db", None, &ActiveConnections::default());
        assert_eq!(target.connection_id, None);
        // With a live mapping the row lands under the current key.
        let target = legacy_target_key("db-sess::db", None, &active_conn_1());
        assert_eq!(target.connection_id.as_deref(), Some("conn-1"));
        assert_eq!(target.session_key, "conn-1::db");
    }

    #[test]
    fn session_map_keys_walk_insertion_order() {
        let mut sessions = SessionMap::new();
        sessions.insert("z", StoredSession::new(Vec::new(), 1.0));
        sessions.insert("a", StoredSession::new(Vec::new(), 2.0));
        assert_eq!(sessions.keys(), vec!["z".to_string(), "a".to_string()]);
    }

    #[test]
    fn the_sync_port_serializes_writes_by_construction() {
        // The write-queue half: `&mut self` serializes persist calls in the
        // sync port; the module docs assign the async re-introduction to the
        // T128 bridge. Two sequential persists land in call order.
        let mut test = env();
        let store = &mut test.store;
        let mut first = super::super::test_support::fixture_session();
        first.id = "a".to_string();
        let mut second = super::super::test_support::fixture_session();
        second.id = "b".to_string();
        assert!(store.persist_chat_session(&first).ok);
        assert!(store.persist_chat_session(&second).ok);
        let ids: Vec<String> = test
            .backend
            .upsert_inputs()
            .iter()
            .filter_map(|input| input.id.clone())
            .collect();
        assert_eq!(ids, vec!["a".to_string(), "b".to_string()]);
    }

    // A counting backend for the re-list failure edge of
    // list_chat_sessions: one-shot queues on the shared double consume in
    // call order, so a per-position failure needs a purpose-built double.
    #[test]
    fn list_chat_sessions_returns_the_first_summaries_when_the_relist_fails() {
        struct RelistFails {
            list_calls: Cell<usize>,
            detail: RefCell<Option<AiSessionDetail>>,
        }
        impl AiSessionBackend for RelistFails {
            fn sessions_list(
                &self,
                params: &SessionListParams,
            ) -> Result<Vec<AiSessionSummary>, String> {
                self.list_calls.set(self.list_calls.get() + 1);
                // First list (main): empty. Second (legacy): the row.
                // Third (re-list after the rebind): error.
                let legacy_row = self.detail.borrow().clone();
                match self.list_calls.get() {
                    1 => Ok(Vec::new()),
                    2 => Ok(legacy_row
                        .iter()
                        .filter(|row| row.session_key == params.session_key.clone().unwrap_or_default())
                        .map(AiSessionDetail::to_summary)
                        .collect()),
                    _ => Err("relist failed".to_string()),
                }
            }
            fn sessions_get(&self, _id: &str) -> Result<AiSessionDetail, String> {
                self.detail
                    .borrow()
                    .clone()
                    .ok_or_else(|| "no row".to_string())
            }
            fn sessions_upsert(
                &self,
                _input: &AiSessionUpsertInput,
            ) -> Result<AiSessionDetail, String> {
                let mut guard = self.detail.borrow_mut();
                let rebound = guard.as_mut().map(|row| {
                    row.session_key = "conn-1::db".to_string();
                    row.clone()
                });
                Ok(rebound.unwrap_or_default())
            }
            fn sessions_delete(&self, _id: &str) -> Result<bool, String> {
                Ok(true)
            }
            fn sessions_export(
                &self,
                _id: &str,
                _format: ExportFormat,
            ) -> Result<String, String> {
                Err("unused".to_string())
            }
        }
        let mut legacy = super::super::test_support::fixture_detail();
        legacy.id = "legacy:db-sess::db".to_string();
        legacy.session_key = "db-sess::db".to_string();
        legacy.connection_id = Some("db-sess".to_string());
        legacy.database = Some("db".to_string());
        let backend = RelistFails { list_calls: Cell::new(0), detail: RefCell::new(Some(legacy)) };
        let clock = SessionClock {
            now_ms: Box::new(|| 0.0),
            now_iso: Box::new(|| NOW_ISO.to_string()),
        };
        let mut store = AiSessionStore::new(
            Box::new(backend),
            Box::new(super::super::test_support::SharedStorage(Rc::new(
                super::super::test_support::MemoryStorage::default(),
            ))),
            clock,
            Box::new(|| "id".to_string()),
        );
        let listed = store.list_chat_sessions(
            &scope(Some("conn-1"), Some("db-sess"), Some("db")),
            Some("conn-1::db"),
            &ActiveConnections::default(),
        );
        // The inner catch returns the original (empty) summaries.
        assert_eq!(listed.as_ref().map(Vec::len), Ok(0));
    }
}
