//! T210 audit: the useColumnStats lifecycle, verified against the React
//! source re-read line by line and
//! `hooks/__tests__/useColumnStats.test.ts` (every case mirrored, with
//! the carried lib/columnStats.ts pure functions' own pins — the SQL
//! builder's dialect quoting, the averagable table, the row parser's
//! null honesty).
//!
//! Tester-written: every expectation below is re-derived from the React
//! files.

use serde_json::json;

use crate::components::data_table::column_stats_dialog::{ColumnStatsStatus, StatsValue};
use crate::hooks::use_column_stats::{
    ColumnStatsHook, ColumnStatsRequest, ColumnStatsTable, build_column_stats_sql,
    is_averagable_type, parse_column_stats_row,
};

fn request<'a>() -> ColumnStatsRequest<'a> {
    ColumnStatsRequest {
        table: ColumnStatsTable {
            database_type: "postgresql",
            schema: Some("public"),
            table_name: "orders",
            quote_char: '"',
        },
        db_session_id: "session-1",
        database: "shop",
    }
}

fn row(cells: &[serde_json::Value]) -> Vec<serde_json::Value> {
    cells.to_vec()
}

// ---- the carried pure functions (lib/columnStats.ts) -------------------------

#[test]
fn the_sql_is_dialect_quoted_schema_qualified_and_positional() {
    // The React test's exact expectation for a PG numeric column.
    let table = ColumnStatsTable {
        database_type: "postgresql",
        schema: Some("public"),
        table_name: "orders",
        quote_char: '"',
    };
    assert_eq!(
        build_column_stats_sql(&table, "price", true),
        "SELECT COUNT(*) AS row_count, COUNT(\"price\") AS non_null_count, \
         COUNT(DISTINCT \"price\") AS distinct_count, MIN(\"price\") AS min_value, \
         MAX(\"price\") AS max_value, AVG(\"price\") AS avg_value FROM \"public\".\"orders\""
    );
    // Text column: no AVG projection at all.
    let sql = build_column_stats_sql(&table, "note", false);
    assert!(!sql.contains("AVG"));
    assert!(sql.contains("COUNT(DISTINCT \"note\")"));
    // Quoting styles: backtick doubling, bracket doubling, none.
    let mysql = ColumnStatsTable {
        database_type: "mysql",
        schema: None,
        table_name: "or`ders",
        quote_char: '`',
    };
    assert!(build_column_stats_sql(&mysql, "co`l", false).contains("FROM `or``ders`"));
    assert!(build_column_stats_sql(&mysql, "co`l", false).contains("COUNT(`co``l`)"));
    let mssql = ColumnStatsTable {
        database_type: "sqlserver",
        schema: Some("dbo"),
        table_name: "or]ders",
        quote_char: '[',
    };
    assert!(build_column_stats_sql(&mssql, "col", false).contains("FROM [dbo].[or]]ders]"));
    let none = ColumnStatsTable {
        database_type: "redis",
        schema: None,
        table_name: "orders",
        quote_char: '\0',
    };
    assert!(build_column_stats_sql(&none, "col", false).contains("FROM orders"));
}

#[test]
fn averagable_types_decide_by_base_and_first_word() {
    // Numeric families, with width suffixes cut.
    assert!(is_averagable_type(Some("numeric(10,2)")));
    assert!(is_averagable_type(Some("INTEGER")));
    assert!(is_averagable_type(Some("bigint")));
    assert!(is_averagable_type(Some("double precision")));
    assert!(is_averagable_type(Some("int unsigned")));
    assert!(is_averagable_type(Some("  float8  ")));
    // Non-averagable and absent: five honest values, not six.
    assert!(!is_averagable_type(Some("text")));
    assert!(!is_averagable_type(Some("timestamp with time zone")));
    assert!(!is_averagable_type(Some("boolean")));
    assert!(!is_averagable_type(Some("")));
    assert!(!is_averagable_type(None));
}

#[test]
fn the_row_parser_is_positional_and_null_honest() {
    let cells = row(&[
        json!(1_000_000),
        json!(999_994),
        json!(7),
        json!("a"),
        json!("z"),
        json!(19.5),
    ]);
    let stats = parse_column_stats_row(Some(&cells), true);
    assert_eq!(stats.row_count, Some(1_000_000));
    assert_eq!(stats.non_null_count, Some(999_994));
    assert_eq!(stats.null_count, Some(6));
    assert_eq!(stats.distinct_count, Some(7));
    assert_eq!(stats.min, StatsValue::Text("a".to_owned()));
    assert_eq!(stats.max, StatsValue::Text("z".to_owned()));
    assert_eq!(stats.avg, StatsValue::Number(19.5));
    assert!(stats.supports_avg);

    // Without AVG: the sixth cell is ignored, avg reads null.
    let stats = parse_column_stats_row(Some(&cells), false);
    assert_eq!(stats.avg, StatsValue::Null);
    assert!(!stats.supports_avg);

    // Counts may arrive as decimal strings; unusable reads as None.
    let stringy = row(&[json!("1000"), json!(" 994 "), json!("n/a"), json!(null), json!(null)]);
    let stats = parse_column_stats_row(Some(&stringy), false);
    assert_eq!(stats.row_count, Some(1000));
    assert_eq!(stats.non_null_count, Some(994));
    assert_eq!(stats.null_count, Some(6));
    assert_eq!(stats.distinct_count, None, "'n/a' is not a count");
    assert_eq!(stats.min, StatsValue::Null);

    // A missing cell (a driver that skipped a value) reads as None.
    let short = row(&[json!(10), json!(10)]);
    let stats = parse_column_stats_row(Some(&short), false);
    assert_eq!(stats.row_count, Some(10));
    assert_eq!(stats.distinct_count, None);
    assert_eq!(stats.min, StatsValue::Null);

    // No row at all: everything absent, never zeros.
    let stats = parse_column_stats_row(None, false);
    assert_eq!(stats.row_count, None);
    assert_eq!(stats.null_count, None);
}

// ---- the lifecycle (React tests 1-6) ----------------------------------------

/// React test 1: the quoted aggregate streams without recording history,
/// then reports ready with the six values.
#[test]
fn open_streams_the_aggregate_then_reports_ready() {
    let mut hook = ColumnStatsHook::new();
    assert_eq!(hook.state().status, ColumnStatsStatus::Idle);

    let stream = hook.open(&request(), "price", Some("numeric(10,2)"));
    assert_eq!(hook.state().status, ColumnStatsStatus::Loading);
    assert_eq!(hook.state().column.as_deref(), Some("price"));
    // One round trip: the exact stream call.
    assert_eq!(
        stream.sql,
        "SELECT COUNT(*) AS row_count, COUNT(\"price\") AS non_null_count, \
         COUNT(DISTINCT \"price\") AS distinct_count, MIN(\"price\") AS min_value, \
         MAX(\"price\") AS max_value, AVG(\"price\") AS avg_value FROM \"public\".\"orders\""
    );
    assert_eq!(stream.db_session_id, "session-1");
    assert!(!stream.apply_result_limit);
    assert!(!stream.record_history);
    assert_eq!(stream.database, "shop");
    assert_eq!(stream.schema.as_deref(), Some("public"));
    assert!(stream.include_avg);

    // The server answers one row.
    hook.on_rows(
        1,
        &[vec![
            json!(1_000_000),
            json!(999_994),
            json!(7),
            json!("a"),
            json!("z"),
            json!(19.5),
        ]],
    );
    assert_eq!(hook.state().status, ColumnStatsStatus::Ready);
    let stats = hook.state().stats.as_ref().expect("the aggregates");
    assert_eq!(stats.row_count, Some(1_000_000));
    assert_eq!(stats.null_count, Some(6));
    assert_eq!(stats.avg, StatsValue::Number(19.5));
    // The stream's completion after a ready row changes nothing.
    hook.on_stream_done(1);
    assert_eq!(hook.state().status, ColumnStatsStatus::Ready);
}

/// React test 2: a text column omits the AVG projection.
#[test]
fn text_columns_omit_avg() {
    let mut hook = ColumnStatsHook::new();
    let stream = hook.open(&request(), "note", Some("text"));
    assert!(!stream.sql.contains("AVG"));
    assert!(stream.sql.contains("COUNT(DISTINCT \"note\")"));
    assert!(!stream.include_avg);
}

/// React test 3: a rejected stream reports the error and keeps the column.
#[test]
fn a_rejected_stream_reports_the_error() {
    let mut hook = ColumnStatsHook::new();
    hook.open(&request(), "note", Some("integer"));
    hook.on_stream_error(1, "avg(text) does not exist");
    assert_eq!(hook.state().status, ColumnStatsStatus::Error);
    assert!(hook.state().error.as_deref().unwrap_or_default().contains("avg(text)"));
    assert_eq!(hook.state().column.as_deref(), Some("note"));
    assert!(hook.state().stats.is_none());
}

/// React test 4: a late answer for a superseded request lands nowhere.
#[test]
fn a_superseded_request_lands_nowhere() {
    let mut hook = ColumnStatsHook::new();
    hook.open(&request(), "price", Some("integer")); // revision 1, gated
    let second = hook.open(&request(), "note", Some("text")); // revision 2

    // The second column answers first.
    hook.on_rows(2, &[vec![json!(5), json!(5), json!(1), json!("x"), json!("x")]]);
    assert_eq!(hook.state().status, ColumnStatsStatus::Ready);
    assert_eq!(hook.state().column.as_deref(), Some("note"));

    // The first column finally answers — over a replaced request.
    hook.on_rows(1, &[vec![json!(5), json!(5), json!(1), json!("x"), json!("x")]]);
    hook.on_stream_done(1);
    assert_eq!(hook.state().column.as_deref(), Some("note"));
    assert_eq!(hook.state().status, ColumnStatsStatus::Ready);
    let _ = second;
}

/// React test 5: dismiss returns to idle and a late answer reopens nothing.
#[test]
fn dismiss_forgets_and_late_answers_reopen_nothing() {
    let mut hook = ColumnStatsHook::new();
    hook.open(&request(), "price", Some("integer"));
    hook.dismiss();
    assert_eq!(hook.state().status, ColumnStatsStatus::Idle);
    assert!(hook.state().column.is_none());

    // The gated stream finally answers.
    hook.on_rows(1, &[vec![json!(5), json!(5), json!(1), json!("x"), json!("x")]]);
    assert_eq!(hook.state().status, ColumnStatsStatus::Idle);
    hook.on_stream_done(1);
    assert_eq!(hook.state().status, ColumnStatsStatus::Idle);
}

/// React test 6: a stream that ends without a row is an error, never
/// zeros (the server's own contract says an aggregate always answers one).
#[test]
fn an_empty_stream_is_an_error_never_zeros() {
    let mut hook = ColumnStatsHook::new();
    hook.open(&request(), "price", Some("integer"));
    // The rows event carries no row: nothing changes yet.
    hook.on_rows(1, &[]);
    assert_eq!(hook.state().status, ColumnStatsStatus::Loading);
    // The stream completes without a ready row.
    hook.on_stream_done(1);
    assert_eq!(hook.state().status, ColumnStatsStatus::Error);
    assert!(hook.state().error.as_deref().unwrap_or_default().contains("empty"));
    assert!(hook.state().stats.is_none());
}
