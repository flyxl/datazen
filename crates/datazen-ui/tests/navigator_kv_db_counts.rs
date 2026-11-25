//! T576: `navigator/useKvDbCounts.ts` port — the per-session KV db-counts
//! cache. No React suite pins the hook directly (it is 60 lines consumed by
//! the tree row); these tests pin the contract its own comments state:
//! shared/deduped per session, best-effort on failure, invalidated on demand,
//! and coerced the way `Number()` coerces.

use serde_json::json;

use datazen_ui::views::navigator::use_kv_db_counts::KvDbCountsCache;

#[test]
fn a_fetch_starts_only_once_per_session() {
    let mut cache = KvDbCountsCache::default();
    assert!(cache.maybe_begin(Some("s1"), Some("DBSIZE")));
    // The second row on the same session joins the shared promise.
    assert!(!cache.maybe_begin(Some("s1"), Some("DBSIZE")));
    assert!(!cache.maybe_begin(Some("s1"), Some("OTHER")));
    assert!(cache.is_cached("s1"));
    // A different session is a different promise.
    assert!(cache.maybe_begin(Some("s2"), Some("DBSIZE")));
}

#[test]
fn a_missing_session_or_command_never_fetches() {
    let mut cache = KvDbCountsCache::default();
    assert!(!cache.maybe_begin(None, Some("DBSIZE")));
    assert!(!cache.maybe_begin(Some("s1"), None));
    // An empty command is falsy in React — same rule here.
    assert!(!cache.maybe_begin(Some("s1"), Some("")));
    assert!(!cache.is_cached("s1"));
}

#[test]
fn rows_become_a_number_keyed_map() {
    let mut cache = KvDbCountsCache::default();
    assert!(cache.maybe_begin(Some("s1"), Some("DBSIZE")));
    let map = cache.apply_rows(
        "s1",
        &json!([
            { "db": 0, "keys": 12 },
            { "db": 1, "keys": 34 },
            { "db": "2", "keys": "56" },
        ]),
    );
    assert_eq!(map.len(), 3);
    assert_eq!(map.get(&0), Some(&12));
    assert_eq!(map.get(&1), Some(&34));
    // String-typed values coerce exactly like React's `Number(...)`.
    assert_eq!(map.get(&2), Some(&56));
    // And the row read sees the same map.
    assert_eq!(cache.counts_of("s1"), map);
}

#[test]
fn serde_int_and_float_numbers_are_the_same_number() {
    // serde_json's `Number(14)` and `Number(14.0)` are distinct tokens but
    // one number — the trap this port avoids by reading through `as_f64`.
    let mut cache = KvDbCountsCache::default();
    let _ = cache.maybe_begin(Some("s1"), Some("DBSIZE"));
    let map = cache.apply_rows(
        "s1",
        &json!([
            { "db": 14, "keys": 1 },
            { "db": 14.0, "keys": 2 },
        ]),
    );
    assert_eq!(map.len(), 1);
    assert_eq!(map.get(&14), Some(&2), "the later row wins, one key");
}

#[test]
fn non_array_data_and_unreadable_rows_read_as_empty() {
    let mut cache = KvDbCountsCache::default();
    let _ = cache.maybe_begin(Some("s1"), Some("DBSIZE"));
    // `res.data` not an array (React's `Array.isArray` guard) — empty map,
    // but the entry is Ready, so no refetch happens on the next begin. The
    // object deliberately carries db/keys: a reader that treats a non-array
    // as a single row would pick them up.
    let map = cache.apply_rows("s1", &json!({ "db": 7, "keys": 9, "error": "nope" }));
    assert!(map.is_empty());
    assert!(!cache.maybe_begin(Some("s1"), Some("DBSIZE")));

    // A row whose db does not coerce (React's NaN key) is dropped; a row
    // with no keys field counts as 0 the way `Number(undefined)` is NaN ->
    // our truncation keeps the row at 0 rather than inventing NaN.
    let mut cache = KvDbCountsCache::default();
    let _ = cache.maybe_begin(Some("s2"), Some("DBSIZE"));
    let map = cache.apply_rows(
        "s2",
        &json!([
            { "db": "not-a-number", "keys": 5 },
            { "db": 3 },
        ]),
    );
    assert_eq!(map.len(), 1);
    assert_eq!(map.get(&3), Some(&0));
}

#[test]
fn a_failed_fetch_evicts_the_entry_so_the_next_read_refetches() {
    let mut cache = KvDbCountsCache::default();
    assert!(cache.maybe_begin(Some("s1"), Some("DBSIZE")));
    cache.fetch_failed("s1");
    assert!(!cache.is_cached("s1"));
    assert!(cache.counts_of("s1").is_empty());
    // The retry is a fresh fetch, not a join.
    assert!(cache.maybe_begin(Some("s1"), Some("DBSIZE")));
}

#[test]
fn invalidate_drops_ready_counts_too() {
    let mut cache = KvDbCountsCache::default();
    let _ = cache.maybe_begin(Some("s1"), Some("DBSIZE"));
    let map = cache.apply_rows("s1", &json!([{ "db": 0, "keys": 9 }]));
    assert_eq!(map.len(), 1);
    cache.invalidate("s1");
    assert!(cache.counts_of("s1").is_empty());
    assert!(cache.maybe_begin(Some("s1"), Some("DBSIZE")));
}

#[test]
fn a_pending_session_reads_as_empty_and_keeps_its_place() {
    let mut cache = KvDbCountsCache::default();
    assert!(cache.maybe_begin(Some("s1"), Some("DBSIZE")));
    // While the promise is in flight the row paints without counts.
    assert!(cache.counts_of("s1").is_empty());
    // And it still dedupes.
    assert!(!cache.maybe_begin(Some("s1"), Some("DBSIZE")));
}
