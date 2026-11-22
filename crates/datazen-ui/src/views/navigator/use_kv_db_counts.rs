//! React `windows/connection/navigator/useKvDbCounts.ts` — lazily fetched
//! per-database key counts for a key-value connection, shared and deduped
//! per session through a module cache (T576).
//!
//! The React shape is a hook plus a module-level `Map<dbSessionId, Promise>`:
//! every row that asks for the same session's counts joins the *same*
//! promise, and a failure evicts the entry so the next read refetches (e.g.
//! after a flush). The Rust port keeps exactly that shape as a cache machine
//! the navigator host owns: [`KvDbCountsCache::maybe_begin`] is the hook's
//! effect (it returns `true` only when a *new* fetch must run — the host then
//! executes the driver's `dbCountsCommand` and reports back through
//! [`KvDbCountsCache::apply_rows`] or [`KvDbCountsCache::fetch_failed`]),
//! and [`KvDbCountsCache::counts_of`] is what a row reads while painting.
//!
//! Counts are best-effort: a failed fetch leaves the reading row without
//! them, never with an error — the hook's `.catch` swallows, and so does the
//! host contract here.

use std::collections::HashMap;

use serde_json::Value;

/// React `DbCountsMap = Record<number, number>` — db index to key count.
pub(crate) type DbCountsMap = HashMap<i64, i64>;

/// One cache entry: the shared promise's lifecycle.
#[derive(Clone, Debug, PartialEq)]
enum Entry {
    /// The promise exists; subscribers are waiting.
    Pending,
    /// The promise resolved; subscribers already got the map.
    Ready(DbCountsMap),
}

/// The module cache, verbatim: one entry per `dbSessionId`.
#[derive(Default)]
pub(crate) struct KvDbCountsCache {
    cache: HashMap<String, Entry>,
}

impl KvDbCountsCache {
    /// React `useKvDbCounts(dbSessionId, command)`'s effect half: start a
    /// fetch when both halves are present, joining the shared entry when one
    /// is already in flight. Returns `true` only for the caller whose request
    /// actually created the entry — that caller (the host) runs the driver
    /// command; everyone else dedupes onto it.
    pub(crate) fn maybe_begin(&mut self, db_session_id: Option<&str>, command: Option<&str>) -> bool {
        let (Some(session), Some(command)) = (db_session_id, command) else {
            return false;
        };
        if command.is_empty() {
            return false;
        }
        if self.cache.contains_key(session) {
            return false;
        }
        self.cache.insert(session.to_owned(), Entry::Pending);
        true
    }

    /// The promise's `.then`: turn the command's `res.data` rows
    /// (`{ db, keys }`) into the map, keyed and valued as numbers the way
    /// `Number(r.db)` / `Number(r.keys)` coerce in React — db may arrive as
    /// a number or a numeric string, and a row that coerces to nothing
    /// (React's `NaN` key, which no lookup ever matches) is dropped.
    pub(crate) fn apply_rows(&mut self, db_session_id: &str, data: &Value) -> DbCountsMap {
        let mut map = DbCountsMap::new();
        if let Value::Array(rows) = data {
            for row in rows {
                let Some(obj) = row.as_object() else { continue };
                let Some(db) = coerce_number(obj.get("db")) else { continue };
                let keys = coerce_number(obj.get("keys")).unwrap_or(0);
                map.insert(db, keys);
            }
        }
        self.cache
            .insert(db_session_id.to_owned(), Entry::Ready(map.clone()));
        map
    }

    /// The promise's `.catch`: evict the entry so the next read refetches.
    /// The reading row keeps whatever it had — counts are best-effort.
    pub(crate) fn fetch_failed(&mut self, db_session_id: &str) {
        self.cache.remove(db_session_id);
    }

    /// React `invalidateKvDbCounts` — drop cached counts for a session so the
    /// next read refetches (e.g. after a flush).
    pub(crate) fn invalidate(&mut self, db_session_id: &str) {
        self.cache.remove(db_session_id);
    }

    /// What a row paints: the resolved map, or empty while pending /
    /// missing / failed.
    pub(crate) fn counts_of(&self, db_session_id: &str) -> DbCountsMap {
        match self.cache.get(db_session_id) {
            Some(Entry::Ready(map)) => map.clone(),
            _ => DbCountsMap::new(),
        }
    }

    /// Whether an entry exists at all (pending or ready) — for hosts that
    /// need to distinguish "never asked" from "asked and empty".
    pub(crate) fn is_cached(&self, db_session_id: &str) -> bool {
        self.cache.contains_key(db_session_id)
    }
}

/// `Number(x)` for the two wire shapes a KV driver sends: a JSON number
/// (int or float — `14` and `14.0` are the same number, read via `as_f64`
/// so serde's int/float split never bites) or a numeric string.
/// `None` is React's `NaN`, which no key lookup can ever match.
fn coerce_number(value: Option<&Value>) -> Option<i64> {
    match value {
        // `as_f64` covers every JSON number (serde splits int/float; React's
        // `Number()` never did), truncated the way an index read expects.
        Some(Value::Number(n)) => n.as_f64().map(|f| f as i64),
        Some(Value::String(s)) => s.trim().parse::<f64>().ok().map(|f| f as i64),
        _ => None,
    }
}
