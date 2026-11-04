//! Tests for the paged query-history read and single-entry delete.
//!
//! Split out of `history_db.rs` purely for file size; the assertions are
//! unchanged. These are the behaviours the global history dialog depends on:
//! an honest `total`, filters the backend can actually evaluate, and a delete
//! that removes one row instead of the table.

use super::fixtures::sample_query;
use super::*;
use chrono::Utc;

/// The truncation the global history dialog used to hide, pinned as numbers.
///
/// 30 rows exist; the old UI asked for a 200-row window and then filtered in
/// JS, so a needle 25 rows deep was unreachable. Here the same needle is
/// found because `search` is applied before `limit`, and the un-truncated
/// `total` still reports all 31 so the UI can say "showing N of M".
#[test]
fn page_search_finds_a_row_outside_the_window_and_total_reports_the_rest() {
    let dir = tempfile::tempdir().unwrap();
    let db = HistoryDb::open(dir.path()).unwrap();
    for i in 0..30 {
        db.add_query_history(sample_query(&format!("SELECT {i}"), i))
            .unwrap();
    }
    db.add_query_history(sample_query("SELECT 'NEEDLE' FROM t", 31))
        .unwrap();

    let page = db
        .query_history_page(&QueryHistoryFilter {
            limit: 10,
            search: Some("needle"),
            ..Default::default()
        })
        .unwrap();

    // The needle is the oldest row, so a limit of 10 would exclude it if
    // search ran after the page was cut. It is here because it did not.
    assert_eq!(page.entries.len(), 1, "needle must survive the limit");
    assert_eq!(page.entries[0].sql, "SELECT 'NEEDLE' FROM t");
    assert_eq!(page.total, 1, "one row matches, not the whole table");

    // A narrow page over a big table must still disclose the full count.
    let wide = db
        .query_history_page(&QueryHistoryFilter {
            limit: 10,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(wide.entries.len(), 10, "page is capped by limit");
    assert_eq!(
        wide.total, 31,
        "total must ignore the limit, or 'showing N of M' lies"
    );
}
#[test]
fn page_search_matches_case_insensitively_and_escapes_wildcards() {
    let dir = tempfile::tempdir().unwrap();
    let db = HistoryDb::open(dir.path()).unwrap();
    db.add_query_history(sample_query("select 'MixedCase'", 0))
        .unwrap();
    db.add_query_history(sample_query("SELECT * FROM t", 0))
        .unwrap();
    // A bare '%' must not act as a wildcard and match everything.
    db.add_query_history(sample_query("SELECT 50 % done", 0))
        .unwrap();
    // Decoys. They exist so the assertions below actually discriminate:
    // each contains the literal prefix the needle tests for, and each would
    // be swept in if the metacharacter reached LIKE unescaped. Without them
    // the needle would return 1 row whether or not escaping worked.
    db.add_query_history(sample_query("SELECT 50 units sold", 0))
        .unwrap();
    db.add_query_history(sample_query("SELECT 'MixedXCase'", 0))
        .unwrap();

    let hit = db
        .query_history_page(&QueryHistoryFilter {
            search: Some("mixedcase"),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(hit.entries.len(), 1, "case-insensitive match");
    assert_eq!(hit.entries[0].sql, "select 'MixedCase'");

    let pct = db
        .query_history_page(&QueryHistoryFilter {
            search: Some("50 %"),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(pct.entries.len(), 1, "'%' is literal, not a wildcard");
    assert_eq!(pct.entries[0].sql, "SELECT 50 % done");

    // '_' would match any char as a wildcard; as a literal it finds nothing,
    // even though 'MixedXCase' would satisfy the wildcard reading.
    let underscore = db
        .query_history_page(&QueryHistoryFilter {
            search: Some("Mixed_Case"),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        underscore.entries.len(),
        0,
        "'_' is literal, not a wildcard"
    );
}
#[test]
fn page_filters_by_connection_database_schema_and_time_range() {
    let dir = tempfile::tempdir().unwrap();
    let db = HistoryDb::open(dir.path()).unwrap();

    let mut a = sample_query("FROM a", 0);
    a.database = "app_db".into();
    a.schema = Some("sales".into());
    db.add_query_history(a).unwrap();

    let mut b = sample_query("FROM b", 0);
    b.connection_id = "cfg-b".into();
    b.database = "other_db".into();
    b.schema = None;
    db.add_query_history(b).unwrap();

    db.add_query_history(sample_query("FROM c", 30)).unwrap();

    let only_a = db
        .query_history_page(&QueryHistoryFilter {
            connection_id: Some("cfg1"),
            database: Some("app_db"),
            schema: Some("sales"),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(only_a.entries.len(), 1);
    assert_eq!(only_a.entries[0].sql, "FROM a");
    assert_eq!(only_a.total, 1);

    let no_schema = db
        .query_history_page(&QueryHistoryFilter {
            schema: Some(""),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(no_schema.total, 2, "empty schema means IS NULL");

    let cutoff = (Utc::now() - Duration::days(7)).to_rfc3339();
    let recent = db
        .query_history_page(&QueryHistoryFilter {
            since: Some(cutoff.as_str()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(recent.total, 2, "the 30-day-old row is outside the window");
    assert!(recent.entries.iter().all(|e| e.sql != "FROM c"));
}
#[test]
fn page_honours_sort_order() {
    let dir = tempfile::tempdir().unwrap();
    let db = HistoryDb::open(dir.path()).unwrap();
    db.add_query_history(sample_query("oldest", 5)).unwrap();
    db.add_query_history(sample_query("newest", 1)).unwrap();

    let mut slow = sample_query("slowest", 0);
    slow.execution_time_ms = 9_000;
    db.add_query_history(slow).unwrap();

    let recent = db
        .query_history_page(&QueryHistoryFilter {
            order: HistoryOrder::Recent,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(recent.entries[0].sql, "slowest", "ties break by recency");

    let oldest = db
        .query_history_page(&QueryHistoryFilter {
            order: HistoryOrder::Oldest,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(oldest.entries[0].sql, "oldest");

    let slowest = db
        .query_history_page(&QueryHistoryFilter {
            order: HistoryOrder::Slowest,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(slowest.entries[0].sql, "slowest");
    assert_eq!(slowest.entries[0].execution_time_ms, 9_000);
}
#[test]
fn delete_query_history_removes_one_row_and_reports_a_noop() {
    let dir = tempfile::tempdir().unwrap();
    let db = HistoryDb::open(dir.path()).unwrap();
    let keep = sample_query("keep me", 0);
    let drop = sample_query("drop me", 0);
    db.add_query_history(keep).unwrap();
    db.add_query_history(drop.clone()).unwrap();

    assert_eq!(db.delete_query_history(&drop.id).unwrap(), 1);

    let page = db
        .query_history_page(&QueryHistoryFilter::default())
        .unwrap();
    assert_eq!(page.total, 1, "only the targeted row went away");
    assert_eq!(page.entries[0].sql, "keep me");

    assert_eq!(
        db.delete_query_history("no-such-id").unwrap(),
        0,
        "an unknown id must report 0, not claim a delete"
    );
}
/// A `..Default::default()` caller must get a real page, not `LIMIT 0`.
///
/// Regression guard: a derived `Default` on the filter would have made
/// `limit: 0`, and an empty page is indistinguishable from "no history yet".
#[test]
fn default_filter_still_returns_a_page() {
    let dir = tempfile::tempdir().unwrap();
    let db = HistoryDb::open(dir.path()).unwrap();
    db.add_query_history(sample_query("only", 0)).unwrap();

    let page = db
        .query_history_page(&QueryHistoryFilter::default())
        .unwrap();
    assert_eq!(page.entries.len(), 1, "a default filter must not hide rows");
    assert_eq!(page.total, 1);
    assert_eq!(
        QueryHistoryFilter::default().limit,
        DEFAULT_HISTORY_PAGE_SIZE
    );
}
