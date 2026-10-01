//! Live SQL Server **streaming** (`query_stream`) coverage.
//!
//! The driver rewrites `SELECT` to `SELECT TOP n+1` when the host sets a result
//! limit, then caps the accepted rows at `n` and flags `truncated` — these
//! tests assert that contract, the no-limit path, empty-result metadata and a
//! clean syntax error.
//!
//! Safety: all writes are scratch objects gated by
//! `TEST_SQLSERVER_ALLOW_WRITE=1` and dropped by a guard that also runs when
//! the body panics. `tests/` is the only directory touched.
//!
//! Configuration comes from the **process environment only**; a local env
//! file is read *only* when you opt in with `TEST_SQLSERVER_ENV_FILE=/path/to/your/env/file`.

mod common;

use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::{connect, drop_quietly, live_config, write_allowed, LiveConfig};
use datazen_driver_api::{
    ConnectionHandle, DatabaseDriver as _, DriverError, QueryStreamCallback, QueryStreamEvent,
    Value,
};
use datazen_driver_sqlserver::SqlServerDriver;

/// Run `body` on its own task and always clean up afterwards — the guard owns
/// the connection (a body must therefore not call `disconnect` itself).
async fn guarded_live<F, Fut, T>(cfg: &LiveConfig, cleanup: Vec<String>, body: F) -> T
where
    F: FnOnce(Arc<SqlServerDriver>, ConnectionHandle) -> Fut + Send + 'static,
    Fut: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    let (driver, handle) = connect(cfg).await;
    let body_driver = Arc::clone(&driver);
    let body_handle = handle.clone();
    let joined = tokio::spawn(async move { body(body_driver, body_handle).await }).await;
    for stmt in &cleanup {
        drop_quietly(&driver, &handle, stmt).await;
    }
    let _ = driver.disconnect(handle).await;
    match joined {
        Ok(value) => value,
        Err(error) => std::panic::resume_unwind(error.into_panic()),
    }
}

fn id_name_batch(table: &str, first_id: i64, count: i64) -> String {
    let mut sql = format!("INSERT INTO {table} ([id], [name]) VALUES ");
    for i in 0..count {
        if i > 0 {
            sql.push(',');
        }
        let id = first_id + i;
        sql.push_str(&format!("({id}, N'row-{id}')"));
    }
    sql
}

/// Event-log inspection helpers.
struct StreamLog;

impl StreamLog {
    fn callback(sink: &Arc<Mutex<Vec<QueryStreamEvent>>>) -> QueryStreamCallback {
        let sink = Arc::clone(sink);
        Arc::new(move |event| {
            sink.lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(event);
        })
    }

    /// Every `Rows` event in order, flattened.
    fn rows(events: &[QueryStreamEvent]) -> Vec<Vec<Option<Value>>> {
        events
            .iter()
            .filter_map(|event| match event {
                QueryStreamEvent::Rows { rows, .. } => Some(rows.clone()),
                _ => None,
            })
            .flatten()
            .collect()
    }

    /// `(index, sql, column names)` for every `StatementStart`.
    fn starts(events: &[QueryStreamEvent]) -> Vec<(usize, String, Vec<String>)> {
        events
            .iter()
            .filter_map(|event| match event {
                QueryStreamEvent::StatementStart {
                    index,
                    sql,
                    columns,
                } => Some((
                    *index,
                    sql.clone(),
                    columns.iter().map(|c| c.name.clone()).collect(),
                )),
                _ => None,
            })
            .collect()
    }

    /// `(index, rows_affected, truncated)` for every `StatementEnd`.
    fn ends(events: &[QueryStreamEvent]) -> Vec<(usize, Option<u64>, bool)> {
        events
            .iter()
            .filter_map(|event| match event {
                QueryStreamEvent::StatementEnd {
                    index,
                    rows_affected,
                    truncated,
                    ..
                } => Some((*index, *rows_affected, *truncated)),
                _ => None,
            })
            .collect()
    }

    fn row_ids(events: &[QueryStreamEvent]) -> Vec<i64> {
        Self::rows(events)
            .iter()
            .map(|row| common::cell_i64(&row[0]).expect("first column decodes as an integer"))
            .collect()
    }

    fn has_done(events: &[QueryStreamEvent]) -> bool {
        events
            .iter()
            .any(|event| matches!(event, QueryStreamEvent::Done { .. }))
    }
}

/// A scratch table lives in a scratch schema; both are dropped by the guard.
async fn create_scratch_table(
    driver: &SqlServerDriver,
    handle: &ConnectionHandle,
    schema: &str,
    table: &str,
    columns: &str,
) {
    driver
        .execute(handle, &format!("CREATE SCHEMA [{schema}]"))
        .await
        .unwrap_or_else(|e| panic!("CREATE SCHEMA [{schema}]: {e}"));
    driver
        .execute(handle, &format!("CREATE TABLE {table} ({columns})"))
        .await
        .unwrap_or_else(|e| panic!("CREATE TABLE {table}: {e}"));
}

/// 250 rows in 5 batches of 50.
async fn seed(driver: &SqlServerDriver, handle: &ConnectionHandle, table: &str) {
    for batch in 0..5i64 {
        let inserted = driver
            .execute(handle, &id_name_batch(table, batch * 50 + 1, 50))
            .await
            .unwrap_or_else(|e| panic!("seed batch {batch}: {e}"));
        assert_eq!(inserted, 50, "batch {batch} must insert 50 rows");
    }
}

// ── C1: stream with a limit ────────────────────────────────────────

#[tokio::test]
async fn stream_with_limit_truncates_and_never_exceeds_it() {
    let Some(cfg) = live_config() else { return };
    if !write_allowed(&cfg, "streaming limit over a scratch table") {
        return;
    }
    let schema = cfg.scratch("stream");
    let table = format!("[{schema}].[rows]");
    let cleanup = vec![
        format!("DROP TABLE IF EXISTS {table}"),
        format!("DROP SCHEMA IF EXISTS [{schema}]"),
    ];

    guarded_live(&cfg, cleanup, move |driver, handle| async move {
        create_scratch_table(
            &driver,
            &handle,
            &schema,
            &table,
            "[id] INT NOT NULL PRIMARY KEY, [name] NVARCHAR(50) NULL",
        )
        .await;
        seed(&driver, &handle, &table).await;

        let observed: Arc<Mutex<Vec<QueryStreamEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let callback = StreamLog::callback(&observed);
        let sql = format!("SELECT [id] FROM {table} ORDER BY [id] ASC");

        driver
            .query_stream(&handle, &sql, Some(50), callback)
            .await
            .expect("query_stream with limit");

        let events = observed.lock().unwrap().clone();
        assert_eq!(
            StreamLog::row_ids(&events),
            (1..=50).collect::<Vec<_>>(),
            "the SQL limit must deliver a stable truncated prefix of exactly 50 rows"
        );

        let starts = StreamLog::starts(&events);
        assert_eq!(starts.len(), 1, "one statement → one StatementStart");
        assert_eq!(starts[0].0, 0, "statement index");
        assert_eq!(
            starts[0].2,
            vec!["id".to_string()],
            "column metadata must arrive before the rows"
        );
        assert_eq!(
            starts[0].1, sql,
            "StatementStart must echo the caller's SQL, not the rewritten TOP form"
        );

        let ends = StreamLog::ends(&events);
        assert_eq!(ends.len(), 1, "one statement → one StatementEnd");
        let (index, rows_affected, truncated) = ends[0];
        assert_eq!(index, 0);
        assert!(
            truncated,
            "fetching TOP 51 for limit 50 must set the truncation flag"
        );
        assert_eq!(
            rows_affected,
            Some(50),
            "rows affected must report the delivered rows"
        );
        assert!(
            StreamLog::has_done(&events),
            "a successful stream ends with Done"
        );
        assert!(
            matches!(events.last(), Some(QueryStreamEvent::Done { .. })),
            "Done must be the last event"
        );
        eprintln!(
            "ℹ️  limit=50 → {} rows, truncated={truncated}",
            StreamLog::rows(&events).len()
        );
    })
    .await;
}

// ── C2: stream without a limit ─────────────────────────────────────

#[tokio::test]
async fn stream_without_limit_delivers_every_row_untruncated() {
    let Some(cfg) = live_config() else { return };
    if !write_allowed(&cfg, "unlimited streaming over a scratch table") {
        return;
    }
    let schema = cfg.scratch("streamall");
    let table = format!("[{schema}].[rows]");
    let cleanup = vec![
        format!("DROP TABLE IF EXISTS {table}"),
        format!("DROP SCHEMA IF EXISTS [{schema}]"),
    ];

    guarded_live(&cfg, cleanup, move |driver, handle| async move {
        create_scratch_table(
            &driver,
            &handle,
            &schema,
            &table,
            "[id] INT NOT NULL PRIMARY KEY, [name] NVARCHAR(50) NULL",
        )
        .await;
        seed(&driver, &handle, &table).await;

        let observed: Arc<Mutex<Vec<QueryStreamEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let callback = StreamLog::callback(&observed);
        driver
            .query_stream(
                &handle,
                &format!("SELECT [id], [name] FROM {table} ORDER BY [id] ASC"),
                None,
                callback,
            )
            .await
            .expect("query_stream without limit");

        let events = observed.lock().unwrap().clone();
        let rows = StreamLog::rows(&events);
        assert_eq!(rows.len(), 250, "every row must arrive");
        let ids: Vec<i64> = rows
            .iter()
            .map(|row| common::cell_i64(&row[0]).expect("id decodes"))
            .collect();
        assert_eq!(ids, (1..=250).collect::<Vec<_>>(), "order must be stable");
        for (row, id) in rows.iter().zip(ids.iter()) {
            assert_eq!(
                common::cell_string(&row[1]).as_deref(),
                Some(format!("row-{id}").as_str()),
                "row {id} payload"
            );
        }

        let ends = StreamLog::ends(&events);
        assert_eq!(ends.len(), 1);
        assert_eq!(ends[0].1, Some(250), "rows affected = 250");
        assert!(
            !ends[0].2,
            "without a SQL limit nothing may be flagged as truncated"
        );
        assert!(StreamLog::has_done(&events));

        // Batching is a transfer detail: all rows arrive regardless of how many
        // `Rows` events they were split across (batch size is 500, so one here).
        let row_events = events
            .iter()
            .filter(|event| matches!(event, QueryStreamEvent::Rows { .. }))
            .count();
        eprintln!(
            "ℹ️  unlimited stream: 250 rows across {} Rows event(s)",
            row_events
        );
    })
    .await;
}

// ── C3: zero rows still carry metadata ─────────────────────────────

#[tokio::test]
async fn stream_zero_rows_still_delivers_column_metadata() {
    let Some(cfg) = live_config() else { return };
    if !write_allowed(&cfg, "empty-result streaming over a scratch table") {
        return;
    }
    let schema = cfg.scratch("stream0");
    let table = format!("[{schema}].[rows]");
    let cleanup = vec![
        format!("DROP TABLE IF EXISTS {table}"),
        format!("DROP SCHEMA IF EXISTS [{schema}]"),
    ];

    guarded_live(&cfg, cleanup, move |driver, handle| async move {
        create_scratch_table(
            &driver,
            &handle,
            &schema,
            &table,
            "[id] INT NOT NULL PRIMARY KEY, [name] NVARCHAR(50) NULL",
        )
        .await;
        seed(&driver, &handle, &table).await;

        let observed: Arc<Mutex<Vec<QueryStreamEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let callback = StreamLog::callback(&observed);
        driver
            .query_stream(
                &handle,
                &format!("SELECT [id], [name] FROM {table} WHERE 1 = 0"),
                None,
                callback,
            )
            .await
            .expect("an empty result set is not an error");

        let events = observed.lock().unwrap().clone();
        assert!(
            StreamLog::rows(&events).is_empty(),
            "no rows may be delivered for an empty result"
        );
        let starts = StreamLog::starts(&events);
        assert_eq!(starts.len(), 1);
        assert_eq!(
            starts[0].2,
            vec!["id".to_string(), "name".to_string()],
            "an empty stream must still deliver column metadata, got {:?}",
            starts[0].2
        );
        let ends = StreamLog::ends(&events);
        assert_eq!(ends.len(), 1);
        assert_eq!(ends[0].1, Some(0), "zero rows affected");
        assert!(!ends[0].2, "an empty result is not a truncation");
        assert!(StreamLog::has_done(&events));
        eprintln!("ℹ️  empty stream → columns {:?}", starts[0].2);
    })
    .await;
}

// ── C4: a broken statement fails cleanly ───────────────────────────

#[tokio::test]
async fn stream_syntax_error_is_a_clean_error() {
    let Some(cfg) = live_config() else { return };
    let (driver, handle) = connect(&cfg).await;

    let observed: Arc<Mutex<Vec<QueryStreamEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let callback = StreamLog::callback(&observed);
    let error = driver
        .query_stream(&handle, "SELECT FROM WHERE", None, callback)
        .await
        .expect_err("a syntax error must surface as an error");
    let text = error.to_string();
    assert!(
        matches!(error, DriverError::QueryFailed(_)),
        "expected QueryFailed, got {error:?}"
    );
    assert!(
        !text.trim().is_empty(),
        "the streaming error must carry the server message"
    );
    assert!(
        !text.contains(&cfg.password),
        "the streaming error must not leak the credential"
    );

    let events = observed.lock().unwrap().clone();
    assert!(
        !StreamLog::has_done(&events),
        "a failed statement must not emit Done: {events:?}"
    );
    assert!(
        !StreamLog::ends(&events)
            .iter()
            .any(|(_, _, truncated)| *truncated),
        "a failed statement must not claim truncation"
    );
    eprintln!("ℹ️  syntax error surfaced as: {text}");

    // The failed stream must leave the connection usable.
    let alive = common::scalar(&driver, &handle, "SELECT 1")
        .await
        .expect("connection must survive a failed stream");
    assert_eq!(common::cell_i64(&alive), Some(1));

    let _ = driver.disconnect(handle).await;
}

// ── C5: multiple statements keep independent indices ───────────────

#[tokio::test]
async fn stream_multi_statement_batches_keep_independent_indices() {
    let Some(cfg) = live_config() else { return };
    if !write_allowed(&cfg, "multi-statement streaming over a scratch table") {
        return;
    }
    let schema = cfg.scratch("streammulti");
    let table = format!("[{schema}].[rows]");
    let cleanup = vec![
        format!("DROP TABLE IF EXISTS {table}"),
        format!("DROP SCHEMA IF EXISTS [{schema}]"),
    ];

    guarded_live(&cfg, cleanup, move |driver, handle| async move {
        create_scratch_table(
            &driver,
            &handle,
            &schema,
            &table,
            "[id] INT NOT NULL PRIMARY KEY, [name] NVARCHAR(50) NULL",
        )
        .await;
        seed(&driver, &handle, &table).await;

        let observed: Arc<Mutex<Vec<QueryStreamEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let callback = StreamLog::callback(&observed);
        driver
            .query_stream(
                &handle,
                &format!(
                    "SELECT TOP 2 [id] FROM {table} ORDER BY [id] ASC; \
                     SELECT TOP 3 [id] FROM {table} ORDER BY [id] DESC"
                ),
                None,
                callback,
            )
            .await
            .expect("multi-statement stream");

        let events = observed.lock().unwrap().clone();
        let count_for = |index: usize| -> usize {
            events
                .iter()
                .filter_map(|event| match event {
                    QueryStreamEvent::Rows { index: i, rows } if *i == index => Some(rows.len()),
                    _ => None,
                })
                .sum()
        };
        assert_eq!(count_for(0), 2, "statement 0 must deliver its own 2 rows");
        assert_eq!(count_for(1), 3, "statement 1 must deliver its own 3 rows");

        let starts = StreamLog::starts(&events);
        assert_eq!(starts.len(), 2);
        assert_eq!((starts[0].0, starts[1].0), (0, 1));
        let ends = StreamLog::ends(&events);
        assert_eq!(ends.len(), 2);
        assert_eq!((ends[0].0, ends[1].0), (0, 1));
        assert!(StreamLog::has_done(&events));
        eprintln!(
            "ℹ️  multi-statement stream → statement 0: {} row(s), statement 1: {} row(s)",
            count_for(0),
            count_for(1)
        );
    })
    .await;
}

// ── C6: streaming is bounded (never hangs) ─────────────────────────

#[tokio::test]
async fn stream_completes_within_a_bounded_time() {
    let Some(cfg) = live_config() else { return };
    if !write_allowed(&cfg, "bounded streaming run") {
        return;
    }
    let schema = cfg.scratch("streambound");
    let table = format!("[{schema}].[rows]");
    let cleanup = vec![
        format!("DROP TABLE IF EXISTS {table}"),
        format!("DROP SCHEMA IF EXISTS [{schema}]"),
    ];

    guarded_live(&cfg, cleanup, move |driver, handle| async move {
        create_scratch_table(
            &driver,
            &handle,
            &schema,
            &table,
            "[id] INT NOT NULL PRIMARY KEY, [name] NVARCHAR(50) NULL",
        )
        .await;
        seed(&driver, &handle, &table).await;

        let observed: Arc<Mutex<Vec<QueryStreamEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let callback = StreamLog::callback(&observed);
        let sql = format!("SELECT [id] FROM {table} ORDER BY [id] ASC");
        tokio::time::timeout(
            Duration::from_secs(60),
            driver.query_stream(&handle, &sql, None, callback),
        )
        .await
        .expect("streaming 250 rows must finish within 60s")
        .expect("streaming must succeed");

        assert_eq!(StreamLog::rows(&observed.lock().unwrap()).len(), 250);
    })
    .await;
}

// ── C7: the editor row cap must not break a self-paging statement ──

/// The Visual Query Builder emits `ORDER BY … OFFSET n ROWS FETCH NEXT m ROWS
/// ONLY`, and the editor adds a row cap to every `SELECT` **without** a limit.
/// The cap used to be injected as `TOP`, which T-SQL rejects next to `OFFSET`
/// (error 10741: "A TOP can not be used in the same query or sub-query as a
/// OFFSET") — so a paginated statement that the builder produced failed on
/// execute. This pins both the cap and the pagination working together, over
/// both execution paths (`query_stream` and `query_multi`).
#[tokio::test]
async fn row_cap_coexists_with_offset_fetch_pagination() {
    let Some(cfg) = live_config() else { return };
    if !write_allowed(&cfg, "row cap over a paginated scratch table") {
        return;
    }
    let schema = cfg.scratch("cap");
    let table = format!("[{schema}].[rows]");
    let cleanup = vec![
        format!("DROP TABLE IF EXISTS {table}"),
        format!("DROP SCHEMA IF EXISTS [{schema}]"),
    ];

    guarded_live(&cfg, cleanup, move |driver, handle| async move {
        create_scratch_table(
            &driver,
            &handle,
            &schema,
            &table,
            "[id] INT NOT NULL PRIMARY KEY, [name] NVARCHAR(50) NULL",
        )
        .await;
        seed(&driver, &handle, &table).await;

        // Exactly what the query builder produces: no TOP, OFFSET/FETCH.
        let paged = format!(
            "SELECT [id], [name] FROM {table} \
             ORDER BY (SELECT NULL) OFFSET 5 ROWS FETCH NEXT 10 ROWS ONLY"
        );

        // 1. Streaming path (what the editor's Execute uses).
        let observed: Arc<Mutex<Vec<QueryStreamEvent>>> = Arc::new(Mutex::new(Vec::new()));
        driver
            .query_stream(&handle, &paged, Some(100), StreamLog::callback(&observed))
            .await
            .unwrap_or_else(|e| panic!("paged statement with a row cap must run: {e}"));
        let events = observed.lock().unwrap().clone();
        let ids = StreamLog::row_ids(&events);
        assert_eq!(ids.len(), 10, "the page itself bounds the result: {ids:?}");
        assert!(
            ids.iter().all(|id| (1..=250).contains(id)),
            "page rows must come from the seeded range: {ids:?}"
        );
        let ends = StreamLog::ends(&events);
        assert_eq!(
            ends.first().map(|e| e.2),
            Some(false),
            "a statement that pages itself is not truncated by the cap"
        );

        // 2. The cap still applies to a statement without its own limit.
        let capped: Arc<Mutex<Vec<QueryStreamEvent>>> = Arc::new(Mutex::new(Vec::new()));
        driver
            .query_stream(
                &handle,
                &format!("SELECT [id] FROM {table} ORDER BY [id] ASC"),
                Some(3),
                StreamLog::callback(&capped),
            )
            .await
            .expect("query_stream with cap");
        let capped_events = capped.lock().unwrap().clone();
        assert_eq!(
            StreamLog::row_ids(&capped_events),
            vec![1, 2, 3],
            "the cap delivers exactly `limit` rows (limit+1 is only fetched to detect truncation)"
        );
        assert_eq!(
            StreamLog::ends(&capped_events).first().map(|e| e.2),
            Some(true),
            "limit+1 rows means the cap truncated"
        );

        // 3. Multi-statement path (`query_multi`) shares the same rewrite.
        let multi = driver
            .query_multi(&handle, &paged, Some(100))
            .await
            .unwrap_or_else(|e| panic!("query_multi on a paged statement: {e}"));
        assert_eq!(multi.results.len(), 1, "one statement → one result");
        assert_eq!(
            multi.results[0].rows.len(),
            10,
            "the page bounds the result"
        );
        assert!(
            !multi.results[0].truncated,
            "a self-paging statement is not reported as truncated"
        );
    })
    .await;
}
