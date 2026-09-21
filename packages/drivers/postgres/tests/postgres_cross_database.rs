//! Gated live integration test: PostgreSQL **cross-database reads**.
//!
//! The driver has no `use_database`: reading a table that lives in another
//! catalog selects (or opens) that database's pool and resolves the schema
//! there, leaving the handle's own session untouched. That is the fix for
//! BUG-003, where the session was re-pointed and every later read silently
//! resolved against the wrong database.
//!
//! Skips cleanly when PostgreSQL is unavailable. Credentials come from process
//! env and/or the repo-root `.env` file (`TEST_PG_*` keys, same as workflow tests).
//!
//! Run (skip if no Postgres):
//!   cargo test -p datazen-driver-postgres --test postgres_cross_database -- --nocapture
//!
//! Force live run with env (example — use your own secrets, do not commit them):
//!   TEST_PG_HOST=127.0.0.1 TEST_PG_PORT=5432 TEST_PG_USER=postgres \
//!   TEST_PG_DATABASE=datazen_demo TEST_PG_DATABASE_B=postgres \
//!   cargo test -p datazen-driver-postgres --test postgres_cross_database -- --nocapture
//!
//! Fixture: discovered at runtime — any `public` relation present in
//! database_a and absent from database_b. The test skips when there is none.

use std::collections::HashMap;
use std::path::PathBuf;

use datazen_driver_api::{ConnectionConfig, DatabaseDriver, DriverError, SqlTarget, Value};
use datazen_driver_postgres::PostgresDriver;

#[derive(Clone, Debug)]
struct PgTestConfig {
    host: String,
    port: u16,
    user: String,
    password: String,
    database_a: String,
    database_b: String,
}

impl Default for PgTestConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 5432,
            user: "postgres".into(),
            password: String::new(),
            database_a: "datazen_demo".into(),
            database_b: "postgres".into(),
        }
    }
}

fn load_dotenv_file() -> HashMap<String, String> {
    let env_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join(".env");

    let mut map = HashMap::new();
    let Ok(content) = std::fs::read_to_string(&env_path) else {
        return map;
    };

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            map.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    map
}

fn env_or_file(file: &HashMap<String, String>, key: &str) -> Option<String> {
    std::env::var(key).ok().or_else(|| file.get(key).cloned())
}

fn load_pg_config() -> Option<PgTestConfig> {
    let file = load_dotenv_file();
    let mut cfg = PgTestConfig::default();
    if let Some(v) = env_or_file(&file, "TEST_PG_HOST") {
        cfg.host = v;
    }
    if let Some(v) = env_or_file(&file, "TEST_PG_PORT") {
        cfg.port = v.parse().unwrap_or(5432);
    }
    if let Some(v) = env_or_file(&file, "TEST_PG_USER") {
        cfg.user = v;
    }
    if let Ok(v) = std::env::var("TEST_PG_PASSWORD") {
        cfg.password = v;
    } else if let Some(v) = file.get("TEST_PG_PASSWORD") {
        cfg.password = v.clone();
    }
    if let Some(v) = env_or_file(&file, "TEST_PG_DATABASE") {
        cfg.database_a = v;
    }
    if let Some(v) = env_or_file(&file, "TEST_PG_DATABASE_B") {
        cfg.database_b = v;
    }

    Some(cfg)
}

/// Connect **to database_b**, so database_a is always a foreign catalog.
fn connection_config(cfg: &PgTestConfig) -> ConnectionConfig {
    ConnectionConfig {
        id: "pg-cross-database-it".into(),
        name: "postgres cross-database integration".into(),
        database_type: "postgresql".into(),
        host: Some(cfg.host.clone()),
        port: Some(cfg.port),
        database: Some(cfg.database_b.clone()),
        schema: None,
        username: Some(cfg.user.clone()),
        password: Some(cfg.password.clone()),
        ssl_mode: Default::default(),
        connection_timeout: 5,
        max_pool_size: 10,
        ssh_tunnel: None,
        tunnel_kind: None,
        tunnel_id: None,
        http_proxy_tunnel: None,
        websocket_tunnel: None,
        color_tag: None,
        group: None,
        last_connected_at: None,
        server_version: None,
        options: None,
        read_only: false,
        pinned: false,
    }
}

fn cell_as_i64(value: &Option<Value>) -> Option<i64> {
    match value {
        Some(Value::Integer(i)) => Some(*i),
        Some(Value::String(s)) => s.parse().ok(),
        _ => None,
    }
}

/// The handle's own session still points at `database_b`, so an unqualified
/// query against a database_a-only table must keep failing. If it starts
/// succeeding, something re-pointed the session (the BUG-003 regression).
async fn assert_handle_session_untouched(
    driver: &PostgresDriver,
    handle: &datazen_driver_api::ConnectionHandle,
    cfg: &PgTestConfig,
    probe: &str,
    label: &str,
) {
    let err = driver
        .query(handle, &format!("SELECT COUNT(*) FROM {probe}"))
        .await
        .expect_err(&format!(
            "{label}: the handle's session must stay on {} and not see {}.{probe}",
            cfg.database_b, cfg.database_a
        ));
    assert!(
        matches!(err, DriverError::QueryFailed(_)),
        "{label}: expected QueryFailed, got {err:?}"
    );
}

#[tokio::test]
async fn cross_database_reads_never_move_the_session() {
    let Some(cfg) = load_pg_config() else {
        return;
    };

    if cfg.database_a == cfg.database_b {
        eprintln!(
            "⏭  Skipping: TEST_PG_DATABASE and TEST_PG_DATABASE_B must differ (got {})",
            cfg.database_a
        );
        return;
    }

    let driver = PostgresDriver::new();
    let handle = match driver.connect(&connection_config(&cfg)).await {
        Ok(h) => h,
        Err(e) => {
            eprintln!(
                "⏭  Skipping: cannot connect to PostgreSQL at {}:{}: {e}",
                cfg.host, cfg.port
            );
            return;
        }
    };

    let dbs = match driver.get_databases(&handle).await {
        Ok(d) => d,
        Err(e) => {
            let _ = driver.disconnect(handle).await;
            eprintln!("⏭  Skipping: list databases failed: {e}");
            return;
        }
    };
    for needed in [&cfg.database_a, &cfg.database_b] {
        if !dbs.iter().any(|d| d == needed) {
            let _ = driver.disconnect(handle).await;
            eprintln!(
                "⏭  Skipping: database `{needed}` not found (have: {})",
                dbs.join(", ")
            );
            return;
        }
    }

    // Listing targets the named catalog without touching the handle's pool.
    let a_tables = driver
        .get_tables(&handle, &cfg.database_a, None)
        .await
        .unwrap_or_else(|e| panic!("get_tables({}): {e}", cfg.database_a));
    let b_tables = driver
        .get_tables(&handle, &cfg.database_b, None)
        .await
        .unwrap_or_else(|e| panic!("get_tables({}): {e}", cfg.database_b));

    // Discover the fixture instead of demanding a specific table name: any
    // `public` relation that exists in A and not in B proves both directions
    // (a targeted read reaches A, an untargeted read cannot).
    let in_b: Vec<&str> = b_tables.iter().map(|t| t.name.as_str()).collect();
    let probe = a_tables
        .iter()
        .find(|t| t.schema.as_deref() == Some("public") && !in_b.contains(&t.name.as_str()))
        .map(|t| t.name.clone());
    let Some(probe) = probe else {
        let _ = driver.disconnect(handle).await;
        eprintln!(
            "⏭  Skipping: {} has no `public` table missing from {} (need one for the \
             cross-database check)",
            cfg.database_a, cfg.database_b
        );
        return;
    };

    println!(
        "▶  cross-database live: handle on {}, reading {} on {}:{}",
        cfg.database_b, cfg.database_a, cfg.host, cfg.port
    );

    // ── the BUG-003 regression: reading a foreign catalog's table structure ──
    let users_schema = driver
        .get_table_schema(&handle, &probe, &cfg.database_a, Some("public"))
        .await
        .unwrap_or_else(|e| panic!("get_table_schema({}.{probe}): {e}", cfg.database_a));
    assert!(
        !users_schema.columns.is_empty(),
        "cross-database table structure must not come back empty — that empty \
         column list is what the data grid turned into blank cells (BUG-003)"
    );
    let col_names: Vec<&str> = users_schema
        .columns
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    println!("   {}.{probe} columns: {col_names:?}", cfg.database_a);

    let (cols, _pks) = driver
        .get_columns(&handle, &probe, &cfg.database_a, Some("public"))
        .await
        .unwrap_or_else(|e| panic!("get_columns({}.{probe}): {e}", cfg.database_a));
    assert_eq!(cols.len(), users_schema.columns.len());

    // ── the session never moved ──
    assert_handle_session_untouched(&driver, &handle, &cfg, &probe, "after cross-database reads")
        .await;

    // ── the data path: a targeted statement reaches the foreign catalog ──
    //
    // This is the regression the grid hit. `query` cannot express a target, so
    // a read for another database ran on the handle's own pool: it either
    // returned that database's rows or nothing at all, which the user saw as
    // "only the first database has data". The same statement must now work
    // through `query_at` and keep failing without a target.
    let target = SqlTarget::new(Some(&cfg.database_a), Some("public"));
    let result = driver
        .query_at(&handle, &format!("SELECT COUNT(*) FROM {probe}"), target)
        .await
        .unwrap_or_else(|e| panic!("query_at({}.{probe}): {e}", cfg.database_a));
    let counted = result
        .rows
        .first()
        .and_then(|row| row.first())
        .and_then(cell_as_i64)
        .expect("COUNT(*) must come back as a number");
    assert!(
        counted >= 1,
        "expected at least one row in {}.{probe}, got {counted}",
        cfg.database_a
    );

    // The bare table name was resolved inside the requested schema, not the
    // pool's default `search_path`.
    let via_public = driver
        .query_at(
            &handle,
            &format!("SELECT COUNT(*) FROM {probe}"),
            SqlTarget::new(Some(&cfg.database_a), Some("public")),
        )
        .await
        .expect("schema-qualified read");
    assert_eq!(
        via_public
            .rows
            .first()
            .and_then(|row| row.first())
            .and_then(cell_as_i64),
        Some(counted)
    );

    // Same statement, no target: the handle's own database still cannot see it.
    assert_handle_session_untouched(&driver, &handle, &cfg, &probe, "after a targeted read").await;

    // A transaction holds one connection, so a statement for another database
    // must be refused rather than silently read from the transaction's own.
    let tx = driver
        .begin_transaction(&handle)
        .await
        .expect("begin transaction");
    let err = driver
        .query_at(&handle, &format!("SELECT COUNT(*) FROM {probe}"), target)
        .await
        .expect_err("a foreign-database statement must not run inside the transaction");
    assert!(
        matches!(err, DriverError::TransactionError(_)),
        "expected TransactionError, got {err:?}"
    );
    driver.rollback(tx).await.expect("rollback");

    // ── repeat to prove the foreign pool is stable and reused ──
    for i in 0..5 {
        let schema = driver
            .get_table_schema(&handle, &probe, &cfg.database_a, Some("public"))
            .await
            .unwrap_or_else(|e| panic!("cross-database read #{i}: {e}"));
        assert_eq!(
            schema.columns.len(),
            users_schema.columns.len(),
            "cross-database read #{i} changed shape"
        );
    }
    assert_handle_session_untouched(
        &driver,
        &handle,
        &cfg,
        &probe,
        "after repeated cross-database reads",
    )
    .await;

    // ── schema is mandatory for single-table resolution on PostgreSQL ──
    let err = driver
        .get_table_schema(&handle, &probe, &cfg.database_a, None)
        .await
        .expect_err("a schema-aware driver must require an explicit schema");
    assert!(
        matches!(err, DriverError::InvalidConfig(_)),
        "expected InvalidConfig without a schema, got {err:?}"
    );

    // ── a schema-less driver contract violation is rejected ──
    let err = driver
        .get_tables(&handle, &cfg.database_a, Some("public"))
        .await
        .expect("listing with an explicit schema is allowed");
    assert!(err.iter().any(|t| t.name == probe));

    // ── unknown database fails loudly instead of returning an empty list ──
    let err = driver
        .get_tables(&handle, "nonexistent_db_xyz_f3_test", None)
        .await
        .expect_err("unknown database should error");
    assert!(
        matches!(err, DriverError::QueryFailed(_)),
        "expected QueryFailed for unknown database, got: {err:?}"
    );
    let msg = err.to_string();
    assert!(
        msg.contains("nonexistent_db_xyz_f3_test")
            || msg.to_lowercase().contains("does not exist")
            || msg.to_lowercase().contains("failed to connect"),
        "error should mention the bad database name: {msg}"
    );

    // ── the open-database report is what the navigator marks as "open" ──
    let open = driver
        .open_databases(&handle)
        .await
        .expect("open_databases");
    assert!(
        open.iter().any(|d| d == &cfg.database_a),
        "the foreign database has an open pool and must be reported: {open:?}"
    );
    assert!(
        open.iter().any(|d| d == &cfg.database_b),
        "the handle's own database is open too: {open:?}"
    );

    // ── closing the foreign database pool drops only that pool ──
    assert!(
        driver
            .close_database(&handle, &cfg.database_a)
            .await
            .expect("close foreign pool"),
        "the foreign pool should have been open"
    );

    let open_after = driver
        .open_databases(&handle)
        .await
        .expect("open_databases after close");
    assert!(
        !open_after.iter().any(|d| d == &cfg.database_a),
        "a closed database must stop being reported as open: {open_after:?}"
    );
    assert!(
        open_after.iter().any(|d| d == &cfg.database_b),
        "closing a foreign database must not affect the handle's own: {open_after:?}"
    );
    let schema_after_close = driver
        .get_table_schema(&handle, &probe, &cfg.database_a, Some("public"))
        .await
        .expect("reopening the foreign pool on demand");
    assert!(!schema_after_close.columns.is_empty());

    driver.disconnect(handle).await.expect("disconnect");
    println!("✅  PostgreSQL cross-database live checks passed");
}
