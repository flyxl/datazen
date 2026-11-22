//! React `windows/connection/navigator/useKvDbCounts.ts` — the lazy per-database
//! key-count loader — as a pure state machine (T576).
//!
//! React has a module-level promise cache keyed by `dbSessionId` (dedup), an
//! `invalidateKvDbCounts(session)` escape hatch (used after a flush), and a hook
//! effect that fetches only when both `dbSessionId` and `command` are present,
//! applies the result only while the effect is still `alive`, and ignores
//! failures (counts are best-effort). All of that is logic, not a view: this
//! port models it with [`KvDbCountsMachine::step`] over host-fed events, and
//! answers with a [`KvCountsEffect`] the host turns into the real driver
//! command. No IPC, promise, cache side effect or timer lives in the machine.
//!
//! ## Semantics kept 1:1
//!
//! * `fetchDbCounts`: one in-flight request per session; `Number(r.db)` keys and
//!   `Number(r.keys)` values; a non-array payload yields an empty map; a reject
//!   evicts the cache entry so the next read refetches.
//! * `useKvDbCounts`: no session or no command ⇒ no fetch and no state change;
//!   a resolve updates the exposed counts only while `alive`, and a reject is
//!   swallowed.
//! * `invalidateKvDbCounts`: evicts the pending entry; exposed counts stay until
//!   the next resolve (React only deletes from the promise cache).
//!
//! ## Consumer / removal stage
//!
//! No production navigator host wires this yet; the host will own one machine
//! per tree and translate `Execute` into `driverCommands.execute`. Every
//! reusable item carries a per-item `#[allow(dead_code)]` with that stage.

use std::collections::{BTreeMap, BTreeSet};

/// React `r.db` — the row's database id, a `number | string`.
#[allow(dead_code)] // VM mirror; supplied by the host driver result.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum DbRef {
    Num(f64),
    Text(String),
}

/// React `rows: Array<{ db: number | string; keys: number }>`.
#[allow(dead_code)] // VM mirror; supplied by the host driver result.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CountRow {
    pub db: DbRef,
    pub keys: f64,
}

/// React `Number(value)` for the `db` field, reduced to a map key. A trimmed
/// empty string coerces to 0 (JS `Number('') === 0`); non-numeric text is
/// skipped (the map would get a `NaN` key, which React never reads).
#[allow(dead_code)] // pure helper; consumed by the derivation and tests.
pub(crate) fn js_db_key(value: &DbRef) -> Option<i64> {
    match value {
        DbRef::Num(number) => Some(*number as i64),
        DbRef::Text(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return Some(0);
            }
            trimmed.parse::<f64>().ok().map(|number| number as i64)
        }
    }
}

/// React `fetchDbCounts`' row→map derivation. `None` means `res.data` was not
/// an array, which the hook treats as `[]`.
#[allow(dead_code)] // pure rule; consumed by the machine and tests.
pub(crate) fn parse_db_counts(rows: Option<&[CountRow]>) -> BTreeMap<i64, i64> {
    let mut map = BTreeMap::new();
    for row in rows.unwrap_or_default() {
        if let Some(db) = js_db_key(&row.db) {
            // Later rows with the same db overwrite earlier ones, exactly like
            // `map[Number(r.db)] = Number(r.keys)`.
            map.insert(db, row.keys as i64);
        }
    }
    map
}

/// Host-fed events standing in for the hook's effect lifecycle and the module
/// cache's promise callbacks.
#[allow(dead_code)] // host port; remove when the navigator host wires the machine.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum KvCountsEvent {
    /// The hook effect runs for `(dbSessionId, command)`.
    Subscribe {
        db_session_id: Option<String>,
        command: Option<String>,
    },
    /// The effect's cleanup: `alive = false`.
    Unsubscribe,
    /// An explicit read (the "next read refetches" step after an invalidate).
    Fetch { db_session_id: String },
    /// The driver command resolved with these rows (`None` = non-array data).
    Resolve {
        db_session_id: String,
        rows: Option<Vec<CountRow>>,
    },
    /// The driver command rejected (cache entry evicted, counts untouched).
    Reject { db_session_id: String },
    /// `invalidateKvDbCounts(dbSessionId)` — evict the pending entry.
    Invalidate { db_session_id: String },
}

/// What the host must do; only `Execute` reaches the driver.
#[allow(dead_code)] // host effect; remove when the navigator host wires the machine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum KvCountsEffect {
    None,
    Execute {
        db_session_id: String,
        command: String,
    },
}

/// The whole pure state: module cache + hook state in one machine.
#[allow(dead_code)] // state machine; consumer: the navigator tree host (not wired).
#[derive(Default)]
pub(crate) struct KvDbCountsMachine {
    /// Sessions with an in-flight fetch — the promise cache's dedup set.
    pending: BTreeSet<String>,
    /// The active effect `(dbSessionId, command)`; `None` means not alive.
    active: Option<(String, String)>,
    /// The counts currently exposed by the hook.
    counts: BTreeMap<i64, i64>,
}

impl KvDbCountsMachine {
    #[allow(dead_code)] // accessor; consumed by the host and tests.
    pub(crate) fn counts(&self) -> &BTreeMap<i64, i64> {
        &self.counts
    }

    #[allow(dead_code)] // accessor; consumed by tests.
    pub(crate) fn is_pending(&self, db_session_id: &str) -> bool {
        self.pending.contains(db_session_id)
    }

    #[allow(dead_code)] // accessor; consumed by tests.
    pub(crate) fn active(&self) -> Option<(&str, &str)> {
        self.active
            .as_ref()
            .map(|(session, command)| (session.as_str(), command.as_str()))
    }

    /// The `fetchDbCounts` cache check: issue one request per session, dedup
    /// while one is in flight.
    fn begin_fetch(&mut self, db_session_id: &str, command: &str) -> KvCountsEffect {
        if self.pending.contains(db_session_id) {
            return KvCountsEffect::None;
        }
        self.pending.insert(db_session_id.to_owned());
        KvCountsEffect::Execute {
            db_session_id: db_session_id.to_owned(),
            command: command.to_owned(),
        }
    }

    /// Drive one transition and return the host effect it implies.
    #[allow(dead_code)] // state machine step; remove with the host wiring.
    pub(crate) fn step(&mut self, event: KvCountsEvent) -> KvCountsEffect {
        match event {
            KvCountsEvent::Subscribe {
                db_session_id,
                command,
            } => {
                // React `if (!dbSessionId || !command) return;` — a blank
                // session or command is not even alive.
                match (db_session_id, command) {
                    (Some(session), Some(command))
                        if !session.is_empty() && !command.is_empty() =>
                    {
                        self.active = Some((session.clone(), command.clone()));
                        self.begin_fetch(&session, &command)
                    }
                    _ => {
                        self.active = None;
                        KvCountsEffect::None
                    }
                }
            }
            KvCountsEvent::Unsubscribe => {
                self.active = None;
                KvCountsEffect::None
            }
            KvCountsEvent::Fetch { db_session_id } => {
                let command = self
                    .active
                    .as_ref()
                    .filter(|(session, _)| session == &db_session_id)
                    .map(|(_, command)| command.clone())
                    .unwrap_or_default();
                self.begin_fetch(&db_session_id, &command)
            }
            KvCountsEvent::Resolve {
                db_session_id,
                rows,
            } => {
                // The settled promise leaves the cache regardless of `alive`.
                self.pending.remove(&db_session_id);
                let alive = self
                    .active
                    .as_ref()
                    .is_some_and(|(session, _)| session == &db_session_id);
                if alive {
                    self.counts = parse_db_counts(rows.as_deref());
                }
                KvCountsEffect::None
            }
            KvCountsEvent::Reject { db_session_id } => {
                // React's `.catch` calls `cache.delete(dbSessionId)` and the
                // hook swallows the error — counts stay as they were.
                self.pending.remove(&db_session_id);
                KvCountsEffect::None
            }
            KvCountsEvent::Invalidate { db_session_id } => {
                self.pending.remove(&db_session_id);
                KvCountsEffect::None
            }
        }
    }
}
