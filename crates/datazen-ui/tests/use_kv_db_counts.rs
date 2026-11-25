//! T576: `useKvDbCounts` logic — the pure derivation/refresh/dedup/update state
//! machine ported from `useKvDbCounts.ts`. No UI, no IPC: every branch is driven
//! through `KvDbCountsMachine::step` with host-fed events.

use datazen_ui::views::navigator::use_kv_db_counts::{
    js_db_key, parse_db_counts, CountRow, DbRef, KvCountsEffect, KvCountsEvent, KvDbCountsMachine,
};

fn rows() -> Vec<CountRow> {
    vec![
        CountRow { db: DbRef::Num(0.0), keys: 3.0 },
        CountRow { db: DbRef::Text("2".to_owned()), keys: 7.0 },
    ]
}

fn subscribe(session: &str, command: &str) -> KvCountsEvent {
    KvCountsEvent::Subscribe {
        db_session_id: Some(session.to_owned()),
        command: Some(command.to_owned()),
    }
}

// ── row → map derivation ─────────────────────────────────────────────────────

#[test]
fn db_key_coercion_matches_javascript_number() {
    assert_eq!(js_db_key(&DbRef::Num(2.0)), Some(2));
    assert_eq!(js_db_key(&DbRef::Text("2".to_owned())), Some(2));
    assert_eq!(js_db_key(&DbRef::Text(" 3 ".to_owned())), Some(3));
    assert_eq!(js_db_key(&DbRef::Text(String::new())), Some(0), "Number('') === 0");
    assert_eq!(js_db_key(&DbRef::Text("nope".to_owned())), None, "NaN key skipped");
}

#[test]
fn parse_rows_builds_the_count_map_and_last_wins() {
    let map = parse_db_counts(Some(&rows()));
    assert_eq!(map.get(&0), Some(&3));
    assert_eq!(map.get(&2), Some(&7));

    // A duplicate db overwrites the earlier count, like `map[key] = keys`.
    let duplicates = vec![
        CountRow { db: DbRef::Num(1.0), keys: 5.0 },
        CountRow { db: DbRef::Text("1".to_owned()), keys: 9.0 },
    ];
    assert_eq!(parse_db_counts(Some(&duplicates)).get(&1), Some(&9));
}

#[test]
fn non_array_payload_yields_empty_map() {
    assert!(parse_db_counts(None).is_empty());
    // Rows with an unparseable db are ignored.
    let bad = vec![CountRow { db: DbRef::Text("x".to_owned()), keys: 4.0 }];
    assert!(parse_db_counts(Some(&bad)).is_empty());
}

// ── fetch / dedup / refresh / invalidate ────────────────────────────────────

#[test]
fn subscribe_fetches_once_and_dedups_while_pending() {
    let mut machine = KvDbCountsMachine::default();
    assert_eq!(
        machine.step(subscribe("s1", "kv.dbCounts")),
        KvCountsEffect::Execute {
            db_session_id: "s1".to_owned(),
            command: "kv.dbCounts".to_owned(),
        }
    );
    assert!(machine.is_pending("s1"));
    assert_eq!(machine.active(), Some(("s1", "kv.dbCounts")));
    // A second effect for the same session reuses the pending request.
    assert_eq!(machine.step(subscribe("s1", "kv.dbCounts")), KvCountsEffect::None);
}

#[test]
fn different_sessions_fetch_independently() {
    let mut machine = KvDbCountsMachine::default();
    machine.step(subscribe("s1", "cmd"));
    assert_eq!(
        machine.step(subscribe("s2", "cmd")),
        KvCountsEffect::Execute {
            db_session_id: "s2".to_owned(),
            command: "cmd".to_owned(),
        }
    );
}

#[test]
fn blank_session_or_command_does_nothing() {
    let mut machine = KvDbCountsMachine::default();
    assert_eq!(
        machine.step(subscribe("", "cmd")),
        KvCountsEffect::None,
        "empty session never fetches"
    );
    assert_eq!(machine.active(), None);
    assert_eq!(
        machine.step(KvCountsEvent::Subscribe {
            db_session_id: Some("s1".to_owned()),
            command: None,
        }),
        KvCountsEffect::None,
        "missing command never fetches"
    );
    assert!(machine.active().is_none());
}

#[test]
fn reject_evicts_the_cache_entry_so_the_next_read_refetches() {
    let mut machine = KvDbCountsMachine::default();
    machine.step(subscribe("s1", "cmd"));
    assert_eq!(machine.step(KvCountsEvent::Reject { db_session_id: "s1".to_owned() }), KvCountsEffect::None);
    assert!(!machine.is_pending("s1"), "reject deleted the cache entry");
    assert_eq!(
        machine.step(KvCountsEvent::Fetch { db_session_id: "s1".to_owned() }),
        KvCountsEffect::Execute {
            db_session_id: "s1".to_owned(),
            command: "cmd".to_owned(),
        }
    );
}

#[test]
fn invalidate_evicts_but_keeps_exposed_counts() {
    let mut machine = KvDbCountsMachine::default();
    machine.step(subscribe("s1", "cmd"));
    machine.step(KvCountsEvent::Resolve {
        db_session_id: "s1".to_owned(),
        rows: Some(rows()),
    });
    assert_eq!(machine.counts().get(&2), Some(&7));

    assert_eq!(
        machine.step(KvCountsEvent::Invalidate { db_session_id: "s1".to_owned() }),
        KvCountsEffect::None
    );
    assert!(!machine.is_pending("s1"));
    assert_eq!(machine.counts().get(&2), Some(&7), "React only evicts the promise cache");
    assert_eq!(
        machine.step(KvCountsEvent::Fetch { db_session_id: "s1".to_owned() }),
        KvCountsEffect::Execute {
            db_session_id: "s1".to_owned(),
            command: "cmd".to_owned(),
        }
    );
}

// ── the `alive` update guard ─────────────────────────────────────────────────

#[test]
fn resolve_updates_counts_only_while_alive() {
    let mut machine = KvDbCountsMachine::default();
    machine.step(subscribe("s1", "cmd"));
    machine.step(KvCountsEvent::Resolve {
        db_session_id: "s1".to_owned(),
        rows: Some(rows()),
    });
    assert_eq!(machine.counts().get(&0), Some(&3));

    // Effect cleanup marks the old promise as stale: a late resolve is ignored.
    machine.step(KvCountsEvent::Unsubscribe);
    let stale = vec![CountRow { db: DbRef::Num(9.0), keys: 99.0 }];
    machine.step(KvCountsEvent::Resolve {
        db_session_id: "s1".to_owned(),
        rows: Some(stale),
    });
    assert_eq!(machine.counts().get(&9), None);
    assert_eq!(machine.counts().get(&0), Some(&3));
    assert!(machine.is_pending("s1"), "a resolved promise stays cached until reject/invalidate");
}

#[test]
fn resolve_for_a_different_session_is_ignored() {
    let mut machine = KvDbCountsMachine::default();
    machine.step(subscribe("s1", "cmd"));
    machine.step(KvCountsEvent::Resolve {
        db_session_id: "s2".to_owned(),
        rows: Some(rows()),
    });
    assert!(machine.counts().is_empty(), "no live effect for s2");
}

#[test]
fn session_change_refetches_and_updates_the_active_key() {
    let mut machine = KvDbCountsMachine::default();
    machine.step(subscribe("s1", "cmd"));
    assert_eq!(
        machine.step(subscribe("s2", "cmd")),
        KvCountsEffect::Execute {
            db_session_id: "s2".to_owned(),
            command: "cmd".to_owned(),
        }
    );
    assert_eq!(machine.active(), Some(("s2", "cmd")));
    machine.step(KvCountsEvent::Resolve {
        db_session_id: "s2".to_owned(),
        rows: Some(rows()),
    });
    assert_eq!(machine.counts().get(&2), Some(&7));
    assert!(machine.is_pending("s1"), "s1's request was not evicted by a session switch");
}

// ── T576 independent audit (appended; reuses this module's helpers) ──────

/// React's `fetchDbCounts` caches the promise and `cache.delete` runs ONLY in
/// `.catch` (`useKvDbCounts.ts:22-27`). A resolved promise stays in the module
/// cache, so a later subscribe for the same session reuses it and the driver
/// command is NOT re-issued. The port's `Resolve` removes the pending entry, so
/// it refetches on the next subscribe — a semantic divergence.
#[test]
fn audit_resolved_fetch_stays_cached_until_invalidated() {
    let mut machine = KvDbCountsMachine::default();
    assert_eq!(
        machine.step(subscribe("s1", "cmd")),
        KvCountsEffect::Execute { db_session_id: "s1".to_owned(), command: "cmd".to_owned() }
    );
    machine.step(KvCountsEvent::Resolve {
        db_session_id: "s1".to_owned(),
        rows: Some(rows()),
    });
    // The resolved entry must still be cached; no new command may be issued.
    assert_eq!(
        machine.step(subscribe("s1", "cmd")),
        KvCountsEffect::None,
        "React keeps the resolved promise cached; only reject/invalidate evict it"
    );

    // After an explicit invalidate, the next subscribe must refetch.
    machine.step(KvCountsEvent::Invalidate { db_session_id: "s1".to_owned() });
    assert_eq!(
        machine.step(subscribe("s1", "cmd")),
        KvCountsEffect::Execute { db_session_id: "s1".to_owned(), command: "cmd".to_owned() }
    );
}

/// After a successful resolve, React's cache is also keyed by session only — a
/// subscribe with a DIFFERENT command for the same session still reuses the
/// resolved promise (no refetch). The port must not re-run the driver command.
#[test]
fn audit_resolved_fetch_is_reused_across_command_change() {
    let mut machine = KvDbCountsMachine::default();
    machine.step(subscribe("s1", "cmdA"));
    machine.step(KvCountsEvent::Resolve {
        db_session_id: "s1".to_owned(),
        rows: Some(rows()),
    });
    assert_eq!(
        machine.step(subscribe("s1", "cmdB")),
        KvCountsEffect::None,
        "cache key is the session only; a resolved entry is reused across commands"
    );
}
