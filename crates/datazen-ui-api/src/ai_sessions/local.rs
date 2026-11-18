//! localStorage persistence half of the AI session store: the offline
//! fallback and the legacy-migration source.
//!
//! Ported from React `packages/frontend/src/stores/ai/sessions.ts`'s
//! `STORAGE_KEY`/`MIGRATION_MARKER_KEY`/limits block and the
//! `loadSessions`/`saveSessions` pair. React talks to the ambient
//! `localStorage` global with per-call `try`/`catch`; the port expresses
//! that IO through the [`SessionLocalStorage`] seam so the module stays
//! pure (the api crate compiles for wasm32 and the Rune event loop is sync —
//! a future host adapter maps it onto the platform's real key-value store,
//! T128/aiStore wiring).
//!
//! ## Ordered map, object-key semantics
//!
//! React's persisted shape is `Record<SessionKey, StoredSession>` — a JS
//! object whose key enumeration order is *insertion order* (all session keys
//! are non-integer strings). That order is observable: the legacy migration
//! upserts rows in enumeration order, and `saveSessions`' LRU eviction
//! tie-breaks on it (`Array.prototype.sort` is stable, equal `lastAccess`
//! keeps insertion order). [`SessionMap`] is therefore a `Vec` of entries —
//! assignment to an existing key keeps its position, a new key appends,
//! `delete` preserves the rest — and it deserializes from a JSON object in
//! **stream order** (a custom `Visitor`, so no `preserve_order` feature is
//! needed on the workspace's `serde_json` pin). A duplicate JSON key keeps
//! the first position with the last value, like `JSON.parse`.
//!
//! ## Corrupted-storage branches, as written
//!
//! `loadSessions`' actual branches: absent/empty raw → `{}` (nothing
//! removed); a JSON *parse* failure → remove the key and start fresh;
//! valid JSON that is not an object (array/number/string/null) → `{}`
//! **without** removal; otherwise the object is cast unchecked. The cast
//! is where Rust must decide something React never spells out, so the
//! port's tolerance policy is: *missing or `null` fields load as their
//! defaults* (the requested 缺字段容错 — matches React's own `?? 0`
//! tolerances in the eviction comparator); the `messages` list and its
//! entries degrade per-field (a non-array `messages` becomes empty, a
//! message violating the declared shape is dropped); and a scalar field
//! whose declared shape is violated (a non-string `id`) fails the whole
//! load through the same clear-and-fresh branch React uses for unparseable
//! data. React would instead keep the entry and crash at first use,
//! uncaught, in several of its consumers — there is no useful semantics
//! there to preserve, and the consolidation is pinned by the tests below.
//!
//! `saveSessions` is best-effort exactly as written: a storage `set` error
//! (quota/security) is swallowed, and the trimmed copy is computed on a
//! clone — the caller's map is never mutated by saving.

use serde::{Deserialize, de::{Deserializer, MapAccess, SeqAccess, Visitor}};
use serde::ser::{Serialize, SerializeMap, Serializer};
use serde_json::Value;

use super::model::{AiChatMessage, truncate_bytes};

/// React `STORAGE_KEY = 'datazen.aiSessions.v1'`.
pub const STORAGE_KEY: &str = "datazen.aiSessions.v1";
/// React `MIGRATION_MARKER_KEY = 'datazen.aiSessions.v1.migratedAt'`.
pub const MIGRATION_MARKER_KEY: &str = "datazen.aiSessions.v1.migratedAt";
/// React `MAX_SESSIONS = 20` — max distinct session keys kept in storage.
pub const MAX_SESSIONS: usize = 20;
/// React `MAX_MESSAGES_PER_SESSION = 50`.
pub const MAX_MESSAGES_PER_SESSION: usize = 50;
/// React `MAX_MESSAGE_BYTES = 4096`.
pub const MAX_MESSAGE_BYTES: usize = 4096;

/// The localStorage seam React reaches through its ambient global.
///
/// `Err` models the throw React's `try`/`catch` swallows (quota exceeded,
/// security error); the message is the display text React's `errorMessage`
/// would surface. `get`'s `Ok(None)` is `getItem`'s `null`.
pub trait SessionLocalStorage {
    fn get(&self, key: &str) -> Result<Option<String>, String>;
    fn set(&self, key: &str, value: &str) -> Result<(), String>;
    fn remove(&self, key: &str) -> Result<(), String>;
}

/// React `StoredSession` — one persisted offline conversation.
///
/// `lastAccess` is `Option` because React's own code tolerates its absence:
/// the eviction comparator reads `?? 0` and the migration checks
/// `Number.isFinite` (a missing or null value behaves as non-finite → now).
/// All other optional fields fold `null`/`undefined` to `None` — inert
/// here, see the model docs.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredSession {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    /// Last 50 messages only — enforced by [`prepare_for_storage`].
    pub messages: Vec<AiChatMessage>,
    /// Timestamp for LRU ordering; `None` = absent/null (non-finite).
    #[serde(
        serialize_with = "serialize_last_access",
        skip_serializing_if = "Option::is_none"
    )]
    pub last_access: Option<f64>,
}

impl StoredSession {
    /// A minimal stored session (tests and host fixtures).
    pub fn new(messages: Vec<AiChatMessage>, last_access: f64) -> Self {
        Self {
            id: None,
            title: None,
            metadata: None,
            connection_id: None,
            database: None,
            messages,
            last_access: Some(last_access),
        }
    }
}

impl<'de> serde::Deserialize<'de> for StoredSession {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Field-level tolerance: every field has a default or tolerant
        // reader, so only a non-object shape (or a structurally impossible
        // field type) can fail — the load's corrupted branch. React's
        // missing `id`/`title`/… load as `undefined`; `null` loads as
        // `null`, which every consumer coalesces — both map to `None` here.
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Raw {
            #[serde(default)]
            id: Option<String>,
            #[serde(default)]
            title: Option<String>,
            #[serde(default)]
            metadata: Option<Value>,
            #[serde(default)]
            connection_id: Option<String>,
            #[serde(default)]
            database: Option<String>,
            #[serde(default, deserialize_with = "messages_or_default")]
            messages: Vec<AiChatMessage>,
            #[serde(default, deserialize_with = "number_or_none")]
            last_access: Option<f64>,
        }
        let raw = Raw::deserialize(deserializer)?;
        Ok(Self {
            id: raw.id,
            title: raw.title,
            metadata: raw.metadata,
            connection_id: raw.connection_id,
            database: raw.database,
            messages: raw.messages,
            last_access: raw.last_access,
        })
    }
}

/// `messages` tolerance: a missing/null/non-array field (or an entry whose
/// declared shape is violated) degrades to fewer or no messages instead of
/// failing the load. React's unchecked cast would carry the entry and
/// crash at first use; the port keeps the load usable (documented policy,
/// module docs).
fn messages_or_default<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<AiChatMessage>, D::Error> {
    let value = Option::<Value>::deserialize(deserializer)?;
    let Some(Value::Array(entries)) = value else {
        return Ok(Vec::new());
    };
    Ok(entries
        .into_iter()
        .filter_map(|entry| serde_json::from_value::<AiChatMessage>(entry).ok())
        .collect())
}

/// `lastAccess` tolerance: a JSON number loads as `Some`, anything else
/// (missing/null/string/…) as `None` — mirroring how React's
/// `Number.isFinite` and `?? 0` treat non-numeric values.
fn number_or_none<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<f64>, D::Error> {
    let value = Option::<Value>::deserialize(deserializer)?;
    Ok(value.and_then(|value| value.as_f64()))
}

/// Serializes `lastAccess` the way `JSON.stringify` writes JS numbers:
/// integral values (within ±2^53) as integers (`1000`, not `1000.0`),
/// fractional values as floats, and non-finite ones as `null` — exactly
/// `JSON.stringify({ a: NaN })` → `{"a":null}`.
fn serialize_last_access<S: Serializer>(value: &Option<f64>, serializer: S) -> Result<S::Ok, S::Error> {
    match value {
        None => serializer.serialize_none(),
        Some(number) => {
            if let Some(int) = js_integer(*number) {
                serializer.serialize_i64(int)
            } else if number.is_finite() {
                serializer.serialize_f64(*number)
            } else {
                serializer.serialize_none()
            }
        }
    }
}

/// The integer a JS Number round-trips exactly (fraction and range guard).
fn js_integer(number: f64) -> Option<i64> {
    if !number.is_finite() || number.fract() != 0.0 || number.abs() >= 9.007_199_254_740_992e15 {
        return None;
    }
    Some(number as i64)
}

/// `Record<SessionKey, StoredSession>` with JS object-key semantics.
///
/// Insertion-ordered (see module docs): [`insert`](Self::insert) replaces
/// in place or appends, [`remove`](Self::remove) deletes while keeping the
/// rest in order, iteration walks insertion order. Serialized as a JSON
/// object in that order; deserialized from any JSON (non-objects load as an
/// empty map — React's `typeof parsed === 'object'` guard).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SessionMap {
    entries: Vec<(String, StoredSession)>,
}

impl SessionMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.entries.iter().any(|(k, _)| k == key)
    }

    pub fn get(&self, key: &str) -> Option<&StoredSession> {
        self.entries.iter().find(|(k, _)| k == key).map(|(_, session)| session)
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut StoredSession> {
        self.entries
            .iter_mut()
            .find(|(k, _)| k == key)
            .map(|(_, session)| session)
    }

    /// `map[key] = value`: an existing key keeps its position with the new
    /// value; a new key appends.
    pub fn insert(&mut self, key: impl Into<String>, session: StoredSession) {
        let key = key.into();
        match self.entries.iter_mut().find(|(k, _)| *k == key) {
            Some(slot) => slot.1 = session,
            None => self.entries.push((key, session)),
        }
    }

    /// `delete map[key]` — returns the removed session, if any.
    pub fn remove(&mut self, key: &str) -> Option<StoredSession> {
        let index = self.entries.iter().position(|(k, _)| k == key)?;
        Some(self.entries.remove(index).1)
    }

    /// Insertion-order walk (React `Object.entries`).
    pub fn iter(&self) -> impl Iterator<Item = (&str, &StoredSession)> {
        self.entries.iter().map(|(key, session)| (key.as_str(), session))
    }

    /// Insertion-order keys (React `Object.keys`).
    pub fn keys(&self) -> Vec<String> {
        self.entries.iter().map(|(key, _)| key.clone()).collect()
    }
}

impl Serialize for SessionMap {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.entries.len()))?;
        for (key, session) in &self.entries {
            map.serialize_entry(key, session)?;
        }
        map.end()
    }
}

impl<'de> serde::Deserialize<'de> for SessionMap {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct SessionMapVisitor;
        impl<'de> Visitor<'de> for SessionMapVisitor {
            type Value = SessionMap;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a JSON object of stored sessions (any other JSON value loads empty)")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<Self::Value, A::Error> {
                let mut entries: Vec<(String, StoredSession)> =
                    Vec::with_capacity(access.size_hint().unwrap_or(0));
                while let Some((key, session)) = access.next_entry::<String, StoredSession>()? {
                    // JSON.parse keeps the first position with the last
                    // value for duplicate keys.
                    match entries.iter_mut().find(|(k, _)| *k == key) {
                        Some(slot) => slot.1 = session,
                        None => entries.push((key, session)),
                    }
                }
                Ok(SessionMap { entries })
            }

            // React's `parsed && typeof parsed === 'object' && !Array.isArray`
            // guard: any non-object JSON loads as an empty map, no removal.
            fn visit_seq<A: SeqAccess<'de>>(self, _access: A) -> Result<Self::Value, A::Error> {
                Ok(SessionMap::new())
            }

            fn visit_bool<E>(self, _value: bool) -> Result<Self::Value, E> {
                Ok(SessionMap::new())
            }

            fn visit_i64<E>(self, _value: i64) -> Result<Self::Value, E> {
                Ok(SessionMap::new())
            }

            fn visit_u64<E>(self, _value: u64) -> Result<Self::Value, E> {
                Ok(SessionMap::new())
            }

            fn visit_f64<E>(self, _value: f64) -> Result<Self::Value, E> {
                Ok(SessionMap::new())
            }

            fn visit_str<E>(self, _value: &str) -> Result<Self::Value, E> {
                Ok(SessionMap::new())
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E> {
                Ok(SessionMap::new())
            }
        }
        deserializer.deserialize_any(SessionMapVisitor)
    }
}

/// React `loadSessions()`: the stored map, with the corrupted-data
/// branches above. `Err` from `get` is React's caught `getItem` throw —
/// same clear-and-fresh path as a parse failure.
pub(crate) fn load_sessions(storage: &dyn SessionLocalStorage) -> SessionMap {
    let raw = match storage.get(STORAGE_KEY) {
        Ok(raw) => raw,
        Err(_) => {
            let _ = storage.remove(STORAGE_KEY);
            return SessionMap::new();
        }
    };
    let Some(raw) = raw else {
        return SessionMap::new();
    };
    // React's `if (!raw) return {}` — the falsy guard also catches an empty
    // string, which would otherwise be a parse error.
    if raw.is_empty() {
        return SessionMap::new();
    }
    match serde_json::from_str::<SessionMap>(&raw) {
        Ok(map) => map,
        Err(_) => {
            let _ = storage.remove(STORAGE_KEY);
            SessionMap::new()
        }
    }
}

/// React `localSessionsMigrated()`: `true` iff the marker key exists (an
/// empty marker value still counts — `getItem(...) !== null`); a storage
/// throw reads as `false`.
pub(crate) fn local_sessions_migrated(storage: &dyn SessionLocalStorage) -> bool {
    match storage.get(MIGRATION_MARKER_KEY) {
        Ok(Some(_)) => true,
        _ => false,
    }
}

/// React's private `markLocalSessionsMigrated()`: best-effort marker write
/// with the injected now as the value (React: `new Date().toISOString()`).
pub(crate) fn mark_local_sessions_migrated(storage: &dyn SessionLocalStorage, now_iso: &str) {
    let _ = storage.set(MIGRATION_MARKER_KEY, now_iso);
}

/// React `saveSessions(sessions)`'s three trimming steps on a copy, exposed
/// for direct testing:
///
/// 1. each session's messages keep only the last `MAX_MESSAGES_PER_SESSION`;
/// 2. LRU eviction — only when there are more than `MAX_SESSIONS` keys,
///    oldest `lastAccess` first (ties keep insertion order: stable sort,
///    `?? 0` for missing stamps);
/// 3. each message's content is byte-truncated to `MAX_MESSAGE_BYTES`.
pub(crate) fn prepare_for_storage(sessions: &SessionMap) -> SessionMap {
    // 1. Per-session message truncation, in entry order.
    let mut trimmed: Vec<(String, StoredSession)> = sessions
        .iter()
        .map(|(key, session)| {
            let mut session = session.clone();
            if session.messages.len() > MAX_MESSAGES_PER_SESSION {
                let split = session.messages.len() - MAX_MESSAGES_PER_SESSION;
                session.messages = session.messages.split_off(split);
            }
            (key.to_string(), session)
        })
        .collect();

    // 2. LRU eviction, stable on insertion order for equal stamps.
    if trimmed.len() > MAX_SESSIONS {
        let mut order: Vec<usize> = (0..trimmed.len()).collect();
        order.sort_by(|&a, &b| {
            let left = trimmed[a].1.last_access.unwrap_or(0.0);
            let right = trimmed[b].1.last_access.unwrap_or(0.0);
            left.partial_cmp(&right).unwrap_or(std::cmp::Ordering::Equal)
        });
        let excess = trimmed.len() - MAX_SESSIONS;
        let evicted: Vec<String> =
            order.iter().take(excess).map(|&index| trimmed[index].0.clone()).collect();
        trimmed.retain(|(key, _)| !evicted.contains(key));
    }

    // 3. Per-message content byte truncation.
    for (_, session) in &mut trimmed {
        for message in &mut session.messages {
            message.content = truncate_bytes(&message.content, MAX_MESSAGE_BYTES);
        }
    }

    SessionMap::from(trimmed)
}

impl From<Vec<(String, StoredSession)>> for SessionMap {
    fn from(entries: Vec<(String, StoredSession)>) -> Self {
        let mut map = SessionMap::new();
        for (key, session) in entries {
            map.insert(key, session);
        }
        map
    }
}

/// React `saveSessions(sessions)`: serialize the trimmed copy and write it,
/// swallowing every error (best effort — the AppDb write is authoritative).
pub(crate) fn save_sessions(storage: &dyn SessionLocalStorage, sessions: &SessionMap) {
    let prepared = prepare_for_storage(sessions);
    match serde_json::to_string(&prepared) {
        Ok(json) => {
            let _ = storage.set(STORAGE_KEY, &json);
        }
        // React's `JSON.stringify` throw lands in the same best-effort
        // catch; unreachable for these shapes but mirrored for symmetry.
        Err(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::super::model::AiMessageRole;
    use super::*;

    /// In-memory localStorage with optional per-op failures, mirroring the
    /// React suite's `vi.stubGlobal('localStorage', {...})` store.
    #[derive(Default)]
    struct MemoryStorage {
        cells: std::cell::RefCell<std::collections::HashMap<String, String>>,
        fail_get: std::cell::Cell<bool>,
        fail_set: std::cell::Cell<bool>,
    }

    impl MemoryStorage {
        fn raw(&self, key: &str) -> Option<String> {
            self.cells.borrow().get(key).cloned()
        }
    }

    impl SessionLocalStorage for MemoryStorage {
        fn get(&self, key: &str) -> Result<Option<String>, String> {
            if self.fail_get.get() {
                return Err("storage unavailable".to_string());
            }
            Ok(self.cells.borrow().get(key).cloned())
        }

        fn set(&self, key: &str, value: &str) -> Result<(), String> {
            if self.fail_set.get() {
                return Err("quota exceeded".to_string());
            }
            self.cells.borrow_mut().insert(key.to_string(), value.to_string());
            Ok(())
        }

        fn remove(&self, key: &str) -> Result<(), String> {
            self.cells.borrow_mut().remove(key);
            Ok(())
        }
    }

    fn message(content: &str) -> AiChatMessage {
        AiChatMessage::new(AiMessageRole::User, content)
    }

    #[test]
    fn loads_empty_when_nothing_is_stored() {
        // React: `const raw = localStorage.getItem(...); if (!raw) return {}`.
        let mut storage = MemoryStorage::default();
        assert!(load_sessions(&mut storage).is_empty());
    }

    #[test]
    fn round_trips_sessions_through_save_and_load() {
        // React suite 'round-trips sessions through save/load'.
        let mut storage = MemoryStorage::default();
        let mut sessions = SessionMap::new();
        sessions.insert("conn-1::db", StoredSession::new(vec![message("hello")], 1000.0));
        save_sessions(&mut storage, &sessions);

        let loaded = load_sessions(&mut storage);
        let stored = loaded.get("conn-1::db");
        assert!(stored.is_some());
        let stored = stored.cloned().unwrap_or_default();
        assert_eq!(stored.messages.len(), 1);
        assert_eq!(stored.messages[0].content, "hello");
        assert_eq!(stored.last_access, Some(1000.0));
        // The JSON spelling keeps JS integer form (1000, not 1000.0) and
        // the camelCase field names React wrote.
        assert_eq!(
            storage.raw(STORAGE_KEY).as_deref(),
            Some(r#"{"conn-1::db":{"messages":[{"role":"user","content":"hello"}],"lastAccess":1000}}"#)
        );
    }

    #[test]
    fn saving_truncates_each_session_to_the_last_fifty_messages() {
        // React suite 'truncates to MAX_MESSAGES_PER_SESSION (50)'.
        let messages: Vec<AiChatMessage> =
            (0..60).map(|index| message(&format!("msg-{index}"))).collect();
        let mut sessions = SessionMap::new();
        sessions.insert("key", StoredSession::new(messages, 1000.0));
        let prepared = prepare_for_storage(&sessions);
        let stored = prepared.get("key").cloned().unwrap_or_default();
        assert_eq!(stored.messages.len(), 50);
        // Keeps the LAST 50: msg-10 .. msg-59.
        assert_eq!(stored.messages[0].content, "msg-10");
        assert_eq!(stored.messages[49].content, "msg-59");
        // The caller's map is never mutated by saving (React's slice copies).
        assert_eq!(
            sessions.get("key").cloned().unwrap_or_default().messages.len(),
            60
        );
    }

    #[test]
    fn saving_evicts_the_oldest_lru_entries_above_twenty_keys() {
        // React suite 'evicts oldest LRU when exceeding MAX_SESSIONS (20)'.
        let mut sessions = SessionMap::new();
        for index in 0..22 {
            sessions.insert(
                format!("key-{index}"),
                StoredSession::new(vec![message(&format!("msg-{index}"))], index as f64 * 100.0),
            );
        }
        let prepared = prepare_for_storage(&sessions);
        assert_eq!(prepared.len(), 20);
        // Oldest (0, 1) evicted; newest survives.
        assert!(prepared.get("key-0").is_none());
        assert!(prepared.get("key-1").is_none());
        assert!(prepared.get("key-21").is_some());
    }

    #[test]
    fn exactly_twenty_keys_never_evict() {
        // React's strict `keys.length > MAX_SESSIONS` gate.
        let mut sessions = SessionMap::new();
        for index in 0..20 {
            sessions.insert(
                format!("key-{index}"),
                StoredSession::new(vec![message("m")], index as f64),
            );
        }
        let prepared = prepare_for_storage(&sessions);
        assert_eq!(prepared.len(), 20);
        assert!(prepared.get("key-0").is_some());
    }

    #[test]
    fn lru_ties_keep_insertion_order() {
        // Equal lastAccess stamps: stable sort keeps the enumeration order,
        // so the earliest-inserted entries are the ones evicted (JS
        // Array.prototype.sort is stable too).
        let mut sessions = SessionMap::new();
        for name in ["t-0", "t-1", "t-2", "t-3"] {
            sessions.insert(name, StoredSession::new(vec![message("m")], 5.0));
        }
        // 4 entries under MAX_SESSIONS: force eviction by re-testing with
        // the map grown to MAX+1 ties via a direct call on 21 entries.
        let mut tied = SessionMap::new();
        for index in 0..21 {
            tied.insert(format!("t-{index}"), StoredSession::new(vec![], 5.0));
        }
        let prepared = prepare_for_storage(&tied);
        assert_eq!(prepared.len(), 20);
        assert!(prepared.get("t-0").is_none());
        assert!(prepared.get("t-1").is_some());
        assert_eq!(sessions.len(), 4);
    }

    #[test]
    fn a_missing_last_access_counts_as_zero_in_the_lru_order() {
        // React's comparator: `trimmed[a].lastAccess ?? 0`.
        let mut sessions = SessionMap::new();
        sessions.insert("missing", StoredSession { last_access: None, ..StoredSession::new(vec![], 1.0) });
        sessions.insert("older", StoredSession::new(vec![], 0.0));
        let mut rest = SessionMap::new();
        for index in 0..20 {
            rest.insert(format!("k-{index}"), StoredSession::new(vec![], 100.0 + index as f64));
        }
        // 2 + 20 = 22 keys: both zero-stamped entries evict (missing first —
        // the None stamp sorts as 0 and ties keep insertion order).
        let mut all = SessionMap::new();
        for (key, session) in sessions.iter() {
            all.insert(key.to_string(), session.clone());
        }
        for (key, session) in rest.iter() {
            all.insert(key.to_string(), session.clone());
        }
        let prepared = prepare_for_storage(&all);
        assert_eq!(prepared.len(), 20);
        assert!(prepared.get("missing").is_none());
        assert!(prepared.get("older").is_none());
        assert!(prepared.get("k-0").is_some());
        assert!(prepared.get("k-19").is_some());
    }

    #[test]
    fn saving_truncates_oversized_message_content_by_bytes() {
        // MAX_MESSAGE_BYTES = 4096; 5000 ASCII chars → 4096 kept.
        let long = "x".repeat(5000);
        let mut sessions = SessionMap::new();
        sessions.insert("key", StoredSession::new(vec![message(&long)], 1.0));
        let prepared = prepare_for_storage(&sessions);
        let stored = prepared.get("key").cloned().unwrap_or_default();
        assert_eq!(stored.messages[0].content.len(), 4096);
        // Multi-byte content truncates on characters, not bytes.
        let wide = "é".repeat(3000); // 6000 bytes
        let mut sessions = SessionMap::new();
        sessions.insert("key", StoredSession::new(vec![message(&wide)], 1.0));
        let prepared = prepare_for_storage(&sessions);
        let stored = prepared.get("key").cloned().unwrap_or_default();
        assert_eq!(stored.messages[0].content.as_bytes().len(), 4096);
        assert_eq!(stored.messages[0].content.chars().count(), 2048);
    }

    #[test]
    fn a_storage_get_error_reads_as_corrupted_and_clears_the_key() {
        // React's catch around the whole load: getItem throwing clears the
        // stored value and starts fresh.
        let mut storage = MemoryStorage::default();
        storage.cells.borrow_mut().insert(STORAGE_KEY.to_string(), "{\"k\":{}}".to_string());
        storage.fail_get.set(true);
        assert!(load_sessions(&mut storage).is_empty());
        // The catch's removeItem best-effort ran.
        assert_eq!(storage.raw(STORAGE_KEY), None);
    }

    #[test]
    fn invalid_json_clears_the_storage_and_starts_fresh() {
        // React: JSON.parse throws → removeItem → {}.
        let mut storage = MemoryStorage::default();
        storage.cells.borrow_mut().insert(STORAGE_KEY.to_string(), "not json".to_string());
        assert!(load_sessions(&mut storage).is_empty());
        assert_eq!(storage.raw(STORAGE_KEY), None);
    }

    #[test]
    fn valid_non_object_json_loads_empty_without_removal() {
        // React's `typeof parsed === 'object' && !Array.isArray(parsed)`
        // guard: arrays, numbers, strings and null load {} and the stored
        // value stays (no removal on this branch).
        for raw in ["[]", "5", "\"text\"", "null", "true"] {
            let mut storage = MemoryStorage::default();
            storage.cells.borrow_mut().insert(STORAGE_KEY.to_string(), raw.to_string());
            assert!(load_sessions(&mut storage).is_empty(), "raw = {raw}");
            assert_eq!(storage.raw(STORAGE_KEY).as_deref(), Some(raw), "raw = {raw}");
        }
    }

    #[test]
    fn an_empty_stored_string_loads_empty_without_removal() {
        // React's falsy `raw` check precedes the parse: '' → {}.
        let mut storage = MemoryStorage::default();
        storage.cells.borrow_mut().insert(STORAGE_KEY.to_string(), String::new());
        assert!(load_sessions(&mut storage).is_empty());
        assert_eq!(storage.raw(STORAGE_KEY).as_deref(), Some(""));
    }

    #[test]
    fn structurally_invalid_entries_take_the_corrupted_branch() {
        // Documented consolidation (module docs): a scalar field whose
        // declared shape is violated (an id that is not a string) fails
        // the whole load through React's clear-and-fresh branch instead of
        // crashing later consumers uncaught.
        let mut storage = MemoryStorage::default();
        storage.cells.borrow_mut().insert(
            STORAGE_KEY.to_string(),
            r#"{"k": {"id": 5}}"#.to_string(),
        );
        assert!(load_sessions(&mut storage).is_empty());
        assert_eq!(storage.raw(STORAGE_KEY), None);

        // The messages list and its entries degrade per-field instead: a
        // non-array messages becomes empty, and a message with a role
        // outside the vocabulary is dropped (module docs policy).
        storage.cells.borrow_mut().insert(
            STORAGE_KEY.to_string(),
            r#"{"k": {"messages": "nope", "lastAccess": 1}}"#.to_string(),
        );
        let loaded = load_sessions(&mut storage);
        assert!(loaded.contains_key("k"));
        assert!(loaded.get("k").cloned().unwrap_or_default().messages.is_empty());
        assert_eq!(storage.raw(STORAGE_KEY).as_deref(), Some(r#"{"k": {"messages": "nope", "lastAccess": 1}}"#));

        storage.cells.borrow_mut().insert(
            STORAGE_KEY.to_string(),
            r#"{"k": {"messages": [{"role": "robot", "content": "x"}], "lastAccess": 1}}"#
                .to_string(),
        );
        let loaded = load_sessions(&mut storage);
        assert!(loaded.contains_key("k"));
        assert!(loaded.get("k").cloned().unwrap_or_default().messages.is_empty());
    }

    #[test]
    fn missing_and_null_fields_load_as_their_defaults() {
        // The requested 缺字段容错: optional fields absent/null → None,
        // messages absent/null → empty, lastAccess null → None (non-finite).
        let mut storage = MemoryStorage::default();
        storage.cells.borrow_mut().insert(
            STORAGE_KEY.to_string(),
            r#"{
                "empty": {},
                "nulls": {"id": null, "title": null, "messages": null, "lastAccess": null},
                "partial": {"id": "s1", "messages": [{"role": "user", "content": "hi"}], "lastAccess": 5}
            }"#
            .to_string(),
        );
        let loaded = load_sessions(&mut storage);
        let empty = loaded.get("empty").cloned().unwrap_or_default();
        assert_eq!(empty, StoredSession { last_access: None, ..StoredSession::new(vec![], 0.0) });
        let nulls = loaded.get("nulls").cloned().unwrap_or_default();
        assert_eq!(nulls.id, None);
        assert!(nulls.messages.is_empty());
        assert_eq!(nulls.last_access, None);
        let partial = loaded.get("partial").cloned().unwrap_or_default();
        assert_eq!(partial.id.as_deref(), Some("s1"));
        assert_eq!(partial.messages.len(), 1);
        assert_eq!(partial.messages[0].content, "hi");
        assert_eq!(partial.last_access, Some(5.0));
    }

    #[test]
    fn the_map_keeps_insertion_order_across_replace_and_delete() {
        let mut map = SessionMap::new();
        map.insert("a", StoredSession::new(vec![], 1.0));
        map.insert("b", StoredSession::new(vec![], 2.0));
        map.insert("c", StoredSession::new(vec![], 3.0));
        // Replace keeps position.
        map.insert("b", StoredSession::new(vec![], 20.0));
        assert_eq!(map.keys(), vec!["a", "b", "c"]);
        assert_eq!(map.get("b").map(|session| session.last_access), Some(Some(20.0)));
        // Delete keeps the rest in order; appending a new key goes last.
        map.remove("a");
        map.insert("d", StoredSession::new(vec![], 4.0));
        assert_eq!(map.keys(), vec!["b", "c", "d"]);
        assert_eq!(map.len(), 3);
    }

    #[test]
    fn duplicate_json_keys_keep_the_first_position_with_the_last_value() {
        let mut storage = MemoryStorage::default();
        storage.cells.borrow_mut().insert(
            STORAGE_KEY.to_string(),
            r#"{"a": {"lastAccess": 1}, "a": {"lastAccess": 2}}"#.to_string(),
        );
        let loaded = load_sessions(&mut storage);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded.get("a").and_then(|session| session.last_access), Some(2.0));
    }

    #[test]
    fn save_swallows_a_quota_error_best_effort() {
        // React: `catch {}` around setItem — the map is simply not persisted.
        let mut storage = MemoryStorage::default();
        storage.fail_set.set(true);
        let mut sessions = SessionMap::new();
        sessions.insert("k", StoredSession::new(vec![message("hello")], 1.0));
        save_sessions(&mut storage, &sessions);
        assert_eq!(storage.raw(STORAGE_KEY), None);
    }

    #[test]
    fn the_migration_marker_reads_and_writes_best_effort() {
        let mut storage = MemoryStorage::default();
        assert!(!local_sessions_migrated(&mut storage));
        mark_local_sessions_migrated(&mut storage, "2026-01-01T00:00:00.000Z");
        assert!(local_sessions_migrated(&mut storage));
        // An empty marker value still counts (React: getItem !== null).
        storage.cells.borrow_mut().insert(MIGRATION_MARKER_KEY.to_string(), String::new());
        assert!(local_sessions_migrated(&mut storage));
        // A get throw reads as not migrated.
        storage.fail_get.set(true);
        assert!(!local_sessions_migrated(&mut storage));
        // The marker write is best effort: a failing set leaves the
        // previous raw value untouched and no error surfaces.
        storage.fail_set.set(true);
        mark_local_sessions_migrated(&mut storage, "now");
        assert_eq!(storage.raw(MIGRATION_MARKER_KEY).as_deref(), Some(""));
    }
}
