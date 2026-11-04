//! Pagination regression coverage for the SQL Server driver.
//!
//! The host used to append `LIMIT n OFFSET m` to every paged table read, which
//! T-SQL rejects (`Incorrect syntax near 'LIMIT'`) — reproduced live in
//! `get_table_data`. These tests pin the driver-owned clause and prove the
//! pages it produces are correct, plus keep a negative control showing the old
//! statement really is invalid T-SQL.
//!
//! Configuration comes from the **process environment only**; a local env
//! file is read *only* when you opt in with `TEST_SQLSERVER_ENV_FILE=/path/to/your/env/file`.

mod common;

use common::{connect, drop_quietly, live_config, write_allowed};
use datazen_driver_api::DatabaseDriver as _;
use datazen_driver_api::DriverError;

/// Read-only: the driver's clause must be legal T-SQL on a real catalog view.
#[tokio::test]
async fn offset_fetch_clause_paginates_a_catalog_view() {
    let Some(cfg) = live_config() else { return };
    let (driver, handle) = connect(&cfg).await;

    let syntax = driver.pagination_syntax(5, 0);
    assert!(
        !syntax.clause.to_ascii_uppercase().contains("LIMIT"),
        "SQL Server has no LIMIT; got {:?}",
        syntax.clause
    );
    assert!(syntax.requires_order_by);

    let order = "ORDER BY [object_id] ASC";
    let page1 = driver
        .query(
            &handle,
            &format!(
                "SELECT [object_id] FROM sys.all_objects {order} {}",
                syntax.clause
            ),
        )
        .await
        .unwrap_or_else(|e| panic!("page 1 ({}): {e}", syntax.clause));

    if page1.rows.len() < 5 {
        eprintln!(
            "⏭  sys.all_objects returned only {} rows; skipping page comparison",
            page1.rows.len()
        );
        let _ = driver.disconnect(handle).await;
        return;
    }

    let second = driver.pagination_syntax(5, 5);
    let page2 = driver
        .query(
            &handle,
            &format!(
                "SELECT [object_id] FROM sys.all_objects {order} {}",
                second.clause
            ),
        )
        .await
        .unwrap_or_else(|e| panic!("page 2 ({}): {e}", second.clause));

    let ids = |result: &datazen_driver_api::QueryResult| -> Vec<i64> {
        result
            .rows
            .iter()
            .filter_map(|row| common::cell_i64(row.first().unwrap_or(&None)))
            .collect()
    };
    let first_ids = ids(&page1);
    let second_ids = ids(&page2);
    assert_eq!(first_ids.len(), 5, "page 1 must fill the page");
    assert!(
        second_ids.iter().all(|id| !first_ids.contains(id)),
        "OFFSET pages must not overlap: {first_ids:?} vs {second_ids:?}"
    );

    let _ = driver.disconnect(handle).await;
}

/// Read-only: the driver's neutral ordering makes an otherwise unordered
/// `SELECT` pageable on T-SQL (`OFFSET … FETCH` requires `ORDER BY`).
#[tokio::test]
async fn driver_order_by_fallback_is_accepted_by_tsql() {
    let Some(cfg) = live_config() else { return };
    let (driver, handle) = connect(&cfg).await;

    let syntax = driver.pagination_syntax(3, 0);
    let fallback = syntax
        .order_by_fallback
        .expect("SQL Server must supply an ORDER BY fallback");
    let sql = format!(
        "SELECT [object_id] FROM sys.all_objects ORDER BY {fallback} {}",
        syntax.clause
    );
    let result = driver
        .query(&handle, &sql)
        .await
        .unwrap_or_else(|e| panic!("unordered paging failed: {e}\nSQL: {sql}"));
    assert_eq!(result.rows.len(), 3, "SQL: {sql}");

    let _ = driver.disconnect(handle).await;
}

/// Negative control: the exact statement shape the host used to emit is
/// invalid T-SQL. If this ever passes, the regression test above lost its
/// meaning (e.g. the server started accepting MySQL syntax).
#[tokio::test]
async fn limit_clause_is_rejected_by_tsql() {
    let Some(cfg) = live_config() else { return };
    let (driver, handle) = connect(&cfg).await;

    let error = driver
        .query(
            &handle,
            "SELECT [object_id] FROM sys.all_objects ORDER BY [object_id] ASC LIMIT 5 OFFSET 0",
        )
        .await
        .expect_err("T-SQL must reject LIMIT");
    match error {
        DriverError::QueryFailed(message) => assert!(
            message.to_ascii_uppercase().contains("LIMIT"),
            "expected a LIMIT syntax error, got: {message}"
        ),
        other => panic!("expected QueryFailed, got {other:?}"),
    }

    let _ = driver.disconnect(handle).await;
}

/// Write-gated: end-to-end paging over a real 100-row table, exactly in the
/// shape the host now generates (`ORDER BY pk ASC <driver clause>`).
#[tokio::test]
async fn pages_over_a_real_table_are_complete_and_ordered() {
    let Some(cfg) = live_config() else { return };
    if !write_allowed(&cfg, "paged read over a scratch table") {
        return;
    }
    let (driver, handle) = connect(&cfg).await;

    let schema = cfg.scratch("pg");
    let table = format!("{schema}.rows");
    drop_quietly(
        &driver,
        &handle,
        &format!("DROP SCHEMA IF EXISTS [{schema}]"),
    )
    .await;
    driver
        .execute(&handle, &format!("CREATE SCHEMA [{schema}]"))
        .await
        .unwrap_or_else(|e| panic!("CREATE SCHEMA failed: {e}"));
    driver
        .execute(
            &handle,
            &format!(
                "CREATE TABLE [{schema}].[rows] ([id] INT NOT NULL PRIMARY KEY, [name] NVARCHAR(50) NULL)"
            ),
        )
        .await
        .unwrap_or_else(|e| panic!("CREATE TABLE failed: {e}"));
    let inserted = driver
        .execute(
            &handle,
            &format!(
                "INSERT INTO [{schema}].[rows] ([id], [name]) \
                 SELECT TOP (100) ROW_NUMBER() OVER (ORDER BY (SELECT NULL)), \
                        CONCAT(N'row-', ROW_NUMBER() OVER (ORDER BY (SELECT NULL))) \
                 FROM sys.all_objects"
            ),
        )
        .await
        .unwrap_or_else(|e| panic!("INSERT failed: {e}"));
    assert_eq!(inserted, 100, "expected 100 seeded rows");

    let page_size = 25u64;
    let mut seen: Vec<i64> = Vec::new();
    for page in 0..4u64 {
        let syntax = driver.pagination_syntax(page_size, page * page_size);
        let sql = format!(
            "SELECT [id], [name] FROM {table} ORDER BY [id] ASC {}",
            syntax.clause
        );
        let result = driver
            .query(&handle, &sql)
            .await
            .unwrap_or_else(|e| panic!("page {page} failed: {e}\nSQL: {sql}"));
        assert_eq!(result.rows.len(), 25, "page {page} must be full: {sql}");
        assert_eq!(result.columns.len(), 2, "page {page} column metadata");
        for row in &result.rows {
            let id = common::cell_i64(&row[0]).expect("id column must decode as an integer");
            seen.push(id);
            let name = common::cell_string(&row[1]).expect("name column must decode as text");
            assert_eq!(name, format!("row-{id}"), "row {id} name mismatch");
        }
    }
    let expected: Vec<i64> = (1..=100).collect();
    assert_eq!(
        seen, expected,
        "pages must cover 1..=100 in order, exactly once"
    );

    // Past the end: a full page request must come back empty, not error.
    let beyond = driver.pagination_syntax(page_size, 100);
    let empty = driver
        .query(
            &handle,
            &format!(
                "SELECT [id] FROM {table} ORDER BY [id] ASC {}",
                beyond.clause
            ),
        )
        .await
        .unwrap_or_else(|e| panic!("past-the-end page failed: {e}"));
    assert!(empty.rows.is_empty(), "page beyond the end must be empty");

    drop_quietly(&driver, &handle, &format!("DROP TABLE IF EXISTS {table}")).await;
    drop_quietly(
        &driver,
        &handle,
        &format!("DROP SCHEMA IF EXISTS [{schema}]"),
    )
    .await;
    let _ = driver.disconnect(handle).await;
}

/// Pins the exact clause shapes the Visual Query Builder (SQL Editor Pro) now
/// emits for SQL Server, so its generator and this driver cannot drift:
/// `OFFSET n ROWS`, `OFFSET n ROWS FETCH NEXT m ROWS ONLY`, the same with
/// `FETCH NEXT 0 ROWS ONLY`, and the `ORDER BY` requirement they share.
#[tokio::test]
async fn query_builder_clause_shapes_are_legal_tsql() {
    let Some(cfg) = live_config() else { return };
    let (driver, handle) = connect(&cfg).await;

    let base = "SELECT [object_id] FROM sys.all_objects";
    let order = "ORDER BY (SELECT NULL)";

    // 1. offset without FETCH (a page beyond the first one, unbounded width).
    let offset_only = format!("{base} {order} OFFSET 2 ROWS");
    let rows = driver
        .query(&handle, &offset_only)
        .await
        .unwrap_or_else(|e| panic!("offset-only paging failed: {e}\nSQL: {offset_only}"));
    assert!(!rows.rows.is_empty(), "SQL: {offset_only}");

    // 2. the standard bounded page.
    let bounded = format!("{base} {order} OFFSET 2 ROWS FETCH NEXT 3 ROWS ONLY");
    let page = driver
        .query(&handle, &bounded)
        .await
        .unwrap_or_else(|e| panic!("bounded paging failed: {e}\nSQL: {bounded}"));
    assert_eq!(page.rows.len(), 3, "SQL: {bounded}");

    // 3. `FETCH NEXT 0 ROWS ONLY` is **not** legal T-SQL (error 10744), which is
    //    why the query builder refuses an explicit zero-row page instead of
    //    emitting one; the driver's own clause never produces it (`limit` is a
    //    page size, so it is always ≥ 1).
    let zero = format!("{base} {order} OFFSET 0 ROWS FETCH NEXT 0 ROWS ONLY");
    let error = driver
        .query(&handle, &zero)
        .await
        .expect_err("FETCH NEXT 0 ROWS ONLY must be rejected");
    match error {
        DriverError::QueryFailed(message) => assert!(
            message.contains("10744") || message.to_ascii_uppercase().contains("GREATER THEN ZERO"),
            "expected the zero-row FETCH error, got: {message}"
        ),
        other => panic!("expected QueryFailed, got {other:?}"),
    }

    // 4. Negative control: without `ORDER BY` T-SQL rejects the same clause,
    //    which is why the builder must inject its fallback ordering. The server
    //    reports code 102 ("Incorrect syntax near '0'") here — it never reaches
    //    the OFFSET-specific 10741 diagnostic, so only the rejection is pinned.
    let unordered = format!("{base} OFFSET 0 ROWS FETCH NEXT 3 ROWS ONLY");
    let error = driver
        .query(&handle, &unordered)
        .await
        .expect_err("OFFSET/FETCH without ORDER BY must be rejected");
    match error {
        DriverError::QueryFailed(message) => assert!(
            message.contains("102")
                || message.to_ascii_uppercase().contains("OFFSET")
                || message.to_ascii_uppercase().contains("ORDER"),
            "expected a syntax error for the unordered page, got: {message}"
        ),
        other => panic!("expected QueryFailed, got {other:?}"),
    }

    let _ = driver.disconnect(handle).await;
}
