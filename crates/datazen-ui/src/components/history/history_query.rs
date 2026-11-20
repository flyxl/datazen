//! Port of `packages/frontend/src/components/history/historyQuery.ts`.
//!
//! Pure helpers behind the global query-history dialog: the query-state
//! defaults, the range cut, the `get_query_history_page` argument bag,
//! the client-side status filter, the truncation honesty reports and the
//! `.sql` export builder. Kept free of the tree and of the adapter so
//! the decision the UI actually gets wrong — "is what I'm showing the
//! whole truth, or a slice of it?" — stays a plain function a test can
//! pin without a DOM.
//!
//! ## What maps where
//!
//! - `HistoryQueryState` / `HistoryRange` / `HistorySort` / the status
//!   union live in `history_filter_bar` (T056's vocabulary, the shapes
//!   React imports from `../../types`); this module is the helpers file
//!   only.
//! - `new Date(now.getTime() - days * 86_400_000).toISOString()` becomes
//!   [`crate::support::formatters::iso_from_epoch_millis`] (the same
//!   `YYYY-MM-DDTHH:MM:SS.sssZ` JS emits), with `now` as epoch
//!   milliseconds. `today` is the last 24h, not the local calendar day —
//!   the column is stamped in UTC, so a calendar-midnight cut would
//!   silently drop this morning's queries for anyone off UTC (the React
//!   doc note, kept verbatim in behavior).
//! - `buildHistoryArgs`' record becomes a JSON object: the `'all'`
//!   sentinel the backend must not receive drops to `null`, blank-trimmed
//!   strings drop to `null`, `since` is `null` when unbounded, and
//!   `order` carries the sort spelling (`recent`/`oldest`/`slowest`).
//! - `applyStatusFilter` is generic over the row: the dialog filters its
//!   `GlobalHistoryEntry` rows through a `success` accessor — the status
//!   filter is the one predicate the backend has no column for, so it
//!   stays client-side (and can shrink the page below the backend's own
//!   `limit`, which is a different thing from the server having
//!   truncated).
//! - `isTruncated` / `truncationNotice` take the page's
//!   `(entries_len, total)` pair: the dialog's carried copy delegates
//!   here now (its private `truncation` was this file's helper, kept
//!   until the task landed).
//! - `buildSqlExport`'s `sql.trimEnd().replace(/;?\s*$/, ';')` is
//!   trim-end plus "append `;` unless the trimmed text already ends with
//!   one" — the regex normalizes at most the single trailing
//!   semicolon-or-whitespace run (`SELECT 1 ;;` survives as `;;`,
//!   `SELECT 1 ;` keeps its space: the JS match is anchored at the very
//!   end).
//!
//! Known deltas: none behavioural — every branch is the React file's
//! verbatim, pinned by the React test's cases.

use crate::components::history::history_filter_bar::{
    HistoryQueryState, HistoryRange, HistorySort, HistoryStatus,
};
use crate::components::history::history_entry_card::HistoryEntryVm;
use crate::support::formatters::iso_from_epoch_millis;

/// React `RANGE_DAYS` (the `all` arm is unbounded).
fn range_days(range: HistoryRange) -> Option<i64> {
    match range {
        HistoryRange::All => None,
        HistoryRange::Today => Some(1),
        HistoryRange::D7 => Some(7),
        HistoryRange::D30 => Some(30),
        HistoryRange::D90 => Some(90),
    }
}

/// React `rangeSinceIso`: the range's lower bound as RFC3339, or `None`
/// for unbounded.
pub(crate) fn range_since_iso(range: HistoryRange, now_ms: i64) -> Option<String> {
    range_days(range).map(|days| iso_from_epoch_millis(now_ms - days * 86_400_000))
}

/// React `buildHistoryArgs`: the `get_query_history_page` argument bag,
/// sentinels and blanks dropped to `null`.
pub(crate) fn build_history_args(state: &HistoryQueryState, limit: i64, now_ms: i64) -> serde_json::Value {
    let search = state.search.trim();
    let database = state.database.trim();
    let schema = state.schema.trim();
    serde_json::json!({
        "limit": limit,
        "connectionId": if state.connection_id == "all" { None } else { Some(&state.connection_id) },
        "database": if database.is_empty() { None } else { Some(database) },
        "schema": if schema.is_empty() { None } else { Some(schema) },
        "search": if search.is_empty() { None } else { Some(search) },
        "since": range_since_iso(state.range, now_ms),
        "order": state.sort.as_value(),
    })
}

/// React `applyStatusFilter`: keep only successes / only failures /
/// everything. Generic over the row type; `success_of` reads the flag.
pub(crate) fn apply_status_filter<'a, T>(
    entries: &'a [T],
    status: HistoryStatus,
    success_of: impl Fn(&T) -> bool,
) -> Vec<&'a T> {
    match status {
        HistoryStatus::Success => entries.iter().filter(|e| success_of(e)).collect(),
        HistoryStatus::Failed => entries.iter().filter(|e| !success_of(e)).collect(),
        HistoryStatus::All => entries.iter().collect(),
    }
}

/// React `isTruncated`: the backend had more matches than it returned.
/// The dialog must not present a truncated page as a complete list — a
/// user who searched for a query that fell outside the window would
/// otherwise see "no matches" and conclude the query was never run.
pub(crate) fn is_truncated(entries_len: usize, total: usize) -> bool {
    total > entries_len
}

/// React `truncationNotice`: the honest count line, or `None` when
/// nothing is being hidden.
pub(crate) fn truncation_notice(entries_len: usize, total: usize) -> Option<(usize, usize)> {
    is_truncated(entries_len, total).then_some((entries_len, total))
}

/// `sql.trimEnd().replace(/;?\s*$/, ';')` — terminate the statement with
/// exactly the trailing run the regex normalizes: trailing whitespace
/// goes, a final `;` stays, a missing one is appended.
fn terminate_statement(sql: &str) -> String {
    let trimmed = sql.trim_end();
    if trimmed.ends_with(';') {
        trimmed.to_owned()
    } else {
        format!("{trimmed};")
    }
}

/// React `buildSqlExport`: render selected history rows as one `.sql`
/// file. Rows are emitted newest-first with a header each, so
/// re-importing the file preserves the order a human reads them in.
pub(crate) fn build_sql_export(
    entries: &[HistoryEntryVm],
    connection_name: Option<&str>,
) -> String {
    if entries.is_empty() {
        return String::new();
    }
    let mut parts: Vec<String> = Vec::new();
    if let Some(connection_name) = connection_name {
        parts.push(format!("-- {connection_name}"));
    }
    for e in entries {
        let status = if e.success { "ok" } else { "FAILED" };
        let rows = e.rows_affected.map_or(String::new(), |n| format!(", {n} rows"));
        let database = e.database.as_deref().filter(|d| !d.is_empty()).unwrap_or("default");
        parts.push(format!(
            "-- {} · {} · {}ms · {}{}",
            e.executed_at, database, e.execution_time_ms, status, rows
        ));
        parts.push(terminate_statement(&e.sql));
        parts.push(String::new());
    }
    parts.join("\n")
}

/// React `exportFileName`: the export dialog's default file name.
pub(crate) fn export_file_name(count: usize) -> String {
    if count == 1 {
        "query.sql".to_owned()
    } else {
        format!("queries-{count}.sql")
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn entry(sql: &str) -> HistoryEntryVm {
        HistoryEntryVm {
            id: "h1".to_owned(),
            success: true,
            database: Some("app".to_owned()),
            schema: None,
            execution_time_ms: 12,
            rows_affected: Some(3),
            executed_at: "2026-01-01T00:00:00Z".to_owned(),
            sql: sql.to_owned(),
            error_message: None,
        }
    }

    /// 2026-06-15T12:00:00Z (the React test's NOW).
    const NOW_MS: i64 = 1_781_524_800_000;

    #[test]
    fn range_steps_back_the_right_number_of_days() {
        assert_eq!(range_since_iso(HistoryRange::All, NOW_MS), None);
        assert_eq!(
            range_since_iso(HistoryRange::Today, NOW_MS).as_deref(),
            Some("2026-06-14T12:00:00.000Z")
        );
        assert_eq!(
            range_since_iso(HistoryRange::D7, NOW_MS).as_deref(),
            Some("2026-06-08T12:00:00.000Z")
        );
        assert_eq!(
            range_since_iso(HistoryRange::D30, NOW_MS).as_deref(),
            Some("2026-05-16T12:00:00.000Z")
        );
        assert_eq!(
            range_since_iso(HistoryRange::D90, NOW_MS).as_deref(),
            Some("2026-03-17T12:00:00.000Z")
        );
    }

    #[test]
    fn args_drop_sentinels_and_blanks() {
        let args = build_history_args(&HistoryQueryState::default(), 200, NOW_MS);
        assert_eq!(
            args,
            serde_json::json!({
                "limit": 200,
                "connectionId": null,
                "database": null,
                "schema": null,
                "search": null,
                "since": null,
                "order": "recent",
            })
        );
    }

    #[test]
    fn args_pass_choices_through_and_trim() {
        let state = HistoryQueryState {
            connection_id: "c9".to_owned(),
            database: "  sales ".to_owned(),
            schema: "public".to_owned(),
            search: "  users  ".to_owned(),
            range: HistoryRange::D7,
            sort: HistorySort::Slowest,
            ..HistoryQueryState::default()
        };
        let args = build_history_args(&state, 50, NOW_MS);
        assert_eq!(args["connectionId"], serde_json::json!("c9"));
        assert_eq!(args["database"], serde_json::json!("sales"));
        assert_eq!(args["schema"], serde_json::json!("public"));
        assert_eq!(args["search"], serde_json::json!("users"));
        assert_eq!(args["limit"], serde_json::json!(50));
        assert_eq!(args["order"], serde_json::json!("slowest"));
        assert_eq!(args["since"], serde_json::json!("2026-06-08T12:00:00.000Z"));
        // Whitespace-only search drops to null.
        let blank = HistoryQueryState { search: "   ".to_owned(), ..HistoryQueryState::default() };
        assert_eq!(build_history_args(&blank, 10, NOW_MS)["search"], serde_json::json!(null));
    }

    #[test]
    fn status_filter_keeps_successes_failures_or_everything() {
        let rows = vec![entry("a"), HistoryEntryVm { success: false, ..entry("b") }, entry("c")];
        assert_eq!(apply_status_filter(&rows, HistoryStatus::All, |e| e.success).len(), 3);
        let success_sql: Vec<&str> = apply_status_filter(&rows, HistoryStatus::Success, |e| e.success)
            .iter()
            .map(|e| e.sql.as_str())
            .collect();
        assert_eq!(success_sql, vec!["a", "c"]);
        let failed: Vec<&str> = apply_status_filter(&rows, HistoryStatus::Failed, |e| e.success)
            .iter()
            .map(|e| e.sql.as_str())
            .collect();
        assert_eq!(failed, vec!["b"]);
    }

    #[test]
    fn truncation_reports_the_honest_counts() {
        assert!(!is_truncated(1, 1));
        assert_eq!(truncation_notice(1, 1), None);
        assert!(is_truncated(1, 57));
        assert_eq!(truncation_notice(1, 57), Some((1, 57)));
        assert!(!is_truncated(0, 0));
        assert_eq!(truncation_notice(0, 0), None);
    }

    #[test]
    fn sql_export_terminates_annotates_and_preserves_order() {
        assert_eq!(build_sql_export(&[], None), "");
        let rows = vec![
            HistoryEntryVm { executed_at: "2026-01-02T00:00:00Z".to_owned(), ..entry("SELECT 1") },
            HistoryEntryVm { executed_at: "2026-01-01T00:00:00Z".to_owned(), ..entry("SELECT 2") },
        ];
        let sql = build_sql_export(&rows, Some("local-pg"));
        let lines: Vec<&str> = sql.split('\n').collect();
        assert_eq!(lines[0], "-- local-pg");
        assert!(sql.find("SELECT 1").unwrap_or(0) < sql.find("SELECT 2").unwrap_or(0));
        assert!(sql.contains("SELECT 1;"));
        assert!(sql.contains("2026-01-02"));
        // Exactly the trailing run: a missing semicolon is appended, an
        // existing one is not doubled, trailing whitespace goes first.
        assert!(build_sql_export(&[entry("SELECT 1;")], None).contains("SELECT 1;\n"));
        assert!(build_sql_export(&[entry("SELECT 1")], None).contains("SELECT 1;\n"));
        assert!(!build_sql_export(&[entry("SELECT 1;   ")], None).contains("SELECT 1;;"));
        // The inner `;?` only matches the very end: ` ;` keeps its space.
        assert!(build_sql_export(&[entry("SELECT 1 ;")], None).contains("SELECT 1 ;"));
        // A failed statement is marked, not dropped.
        let failed = HistoryEntryVm { success: false, ..entry("SELECT bad") };
        assert!(build_sql_export(&[failed], None).contains("FAILED"));
        // No database falls back to the named placeholder.
        let no_db = HistoryEntryVm { database: Some(String::new()), ..entry("SELECT 1") };
        assert!(build_sql_export(&[no_db], None).contains("default"));
        // No connection name: the header line is the first entry's.
        assert!(build_sql_export(&[entry("SELECT 1")], None).starts_with("-- 2026-01-01"));
    }

    #[test]
    fn file_names_are_singular_or_counted() {
        assert_eq!(export_file_name(1), "query.sql");
        assert_eq!(export_file_name(4), "queries-4.sql");
    }
}
