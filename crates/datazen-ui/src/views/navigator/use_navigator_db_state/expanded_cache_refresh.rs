//! React `windows/connection/schema-tree/useExpandedDbCacheRefresh.ts` —
//! the schema-surface watch the navigator's db state mounts (T580's port of
//! it; the formal task is T604, and this moves when it lands).
//!
//! Watches the schema store for genuine schema-surface changes per
//! connection — a fingerprint of the database list + schema epoch — and
//! refreshes the caller's per-database caches for exactly those
//! connections. Root-cause fix for "sidebar refresh flips my SQL session":
//! the previous inline implementations reloaded each expanded database
//! through `useDatabase` + `listTables`, so the LAST reloaded database
//! silently became the session's active database. The reloads here are
//! session-neutral reads.
//!
//! Ordering / cancellation semantics: for every detected fingerprint change
//! this performs ONE synchronous invalidate-then-schedule pass — the clear
//! runs first, and the table + object-category reloads for that connection
//! are scheduled in the same pass with nothing in between. A clear can
//! therefore never leave caches empty waiting for an unrelated reload to
//! (maybe) win a race against it: every wipe carries its own recovery wave.
//! Explicit user refreshes may still issue duplicate fetches afterwards;
//! those writes are equally fresh, so they can never resurrect stale
//! content over the wave.

use std::collections::HashMap;

use super::{DbEffect, DbStateInputs, NavigatorDbState};

/// React `schemaFingerprint` — `(databases ?? []).join('\\0') + '|' + epoch`.
/// A change means the connection's schema surface genuinely changed
/// (databases added/removed, or an admin op bumped the epoch).
fn schema_fingerprint(databases: &[String], epoch: u64) -> String {
    let mut fp = databases.join("\0");
    fp.push('|');
    fp.push_str(&epoch.to_string());
    fp
}

/// One pass of the watch. The previous fingerprints live in the state
/// machine (the `prevFpRef`); the first pass records them and does nothing —
/// a mount is not a change.
pub fn cache_refresh_pass(
    state: &mut NavigatorDbState,
    inputs: &DbStateInputs<'_>,
) -> Vec<DbEffect> {
    // The next fingerprint set: one per live session with schema state.
    let mut next_fp: HashMap<String, String> = HashMap::new();
    let mut session_owner: HashMap<String, String> = HashMap::new();
    for (connection_id, entry) in inputs.active_connections {
        let Some(db_session_id) = entry.db_session_id.as_deref() else {
            continue;
        };
        let Some(schema) = inputs.schemas.get(db_session_id) else {
            continue;
        };
        next_fp.insert(
            db_session_id.to_owned(),
            schema_fingerprint(&schema.databases, schema.schema_epoch),
        );
        // Map back to the persistent connection id the expanded keys use.
        session_owner.insert(db_session_id.to_owned(), connection_id.clone());
    }

    let prev = state.take_fingerprints(next_fp.clone());
    if prev.is_empty() {
        // First pass: record, do nothing.
        return Vec::new();
    }

    let mut effects = Vec::new();
    for (db_session_id, fp) in &next_fp {
        if prev.get(db_session_id).map(String::as_str) == Some(fp.as_str()) {
            continue;
        }
        let Some(connection_id) = session_owner.get(db_session_id) else {
            continue;
        };

        // Invalidate first, then immediately schedule the recovery
        // reloads — synchronously within this pass (see the module doc).
        state.clear_session_caches(db_session_id, Some(connection_id));

        let prefix = format!("{connection_id}::");
        let mut db_names: Vec<&str> = inputs
            .expanded_dbs
            .iter()
            .filter_map(|k| k.strip_prefix(&prefix))
            .collect();
        db_names.sort_unstable();
        for db_name in db_names {
            effects.extend(state.reload_db_tables(db_session_id, db_name, None));
        }
        let mut cat_keys: Vec<&String> = inputs.expanded_cats.iter().collect();
        cat_keys.sort_unstable();
        for cat_key in cat_keys {
            if !cat_key.starts_with(&prefix) {
                continue;
            }
            let cat_id = cat_key.rsplit("::").next().unwrap_or_default();
            if cat_id.is_empty() || cat_id == "tables" || cat_id == "views" {
                continue;
            }
            let known_databases = inputs
                .schemas
                .get(db_session_id)
                .map(|s| s.databases.clone())
                .unwrap_or_default();
            // React's Set: known databases plus the expanded db keys, deduped.
            let mut database_names: Vec<String> = known_databases;
            for db_key in inputs.expanded_dbs {
                if let Some(name) = db_key.strip_prefix(&prefix) {
                    database_names.push(name.to_owned());
                }
            }
            let database = database_names
                .into_iter()
                .filter(|name| cat_key.starts_with(&format!("{prefix}{name}::")))
                .max_by_key(|name| name.len());
            let Some(database) = database else {
                continue;
            };
            effects.extend(
                state.reload_db_object_category(db_session_id, cat_key, cat_id, &database),
            );
        }
    }
    effects
}
