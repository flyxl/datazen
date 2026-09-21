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
//! Fixture assumption: database_a has a `users` table; database_b does not.

use std::collections::HashMap;
use std::path::PathBuf;

use datazen_driver_api::{ConnectionConfig, DatabaseDriver, DriverError, Value};
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
    label: &str,
) {
    let err = driver
        .query(handle, "SELECT COUNT(*) FROM users")
        .await
        .expect_err(&format!(
            "{label}: the handle's session must stay on {} and not see {}.users",
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
    if !a_tables.iter().any(|t| t.name == "users") {
        let _ = driver.disconnect(handle).await;
        eprintln!(
            "⏭  Skipping: `{}.users` missing (needed for the cross-database check)",
            cfg.database_a
        );
        return;
    }
    let b_tables = driver
        .get_tables(&handle, &cfg.database_b, None)
        .await
        .unwrap_or_else(|e| panic!("get_tables({}): {e}", cfg.database_b));
    if b_tables.iter().any(|t| t.name == "users") {
        let _ = driver.disconnect(handle).await;
        eprintln!(
            "⏭  Skipping: `{}.users` unexpectedly exists (need empty-ish B for negative check)",
            cfg.database_b
        );
        return;
    }

    println!(
        "▶  cross-database live: handle on {}, reading {} on {}:{}",
        cfg.database_b, cfg.database_a, cfg.host, cfg.port
    );

    // ── the BUG-003 regression: reading a foreign catalog's table structure ──
    let users_schema = driver
        .get_table_schema(&handle, "users", &cfg.database_a, Some("public"))
        .await
        .unwrap_or_else(|e| panic!("get_table_schema({}.users): {e}", cfg.database_a));
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
    println!("   {}.users columns: {col_names:?}", cfg.database_a);

    let (cols, _pks) = driver
        .get_columns(&handle, "users", &cfg.database_a, Some("public"))
        .await
        .unwrap_or_else(|e| panic!("get_columns({}.users): {e}", cfg.database_a));
    assert_eq!(cols.len(), users_schema.columns.len());

    // ── the session never moved ──
    assert_handle_session_untouched(&driver, &handle, &cfg, "after cross-database reads").await;

    // ── repeat to prove the foreign pool is stable and reused ──
    for i in 0..5 {
        let schema = driver
            .get_table_schema(&handle, "users", &cfg.database_a, Some("public"))
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
        "after repeated cross-database reads",
    )
    .await;

    // ── schema is mandatory for single-table resolution on PostgreSQL ──
    let err = driver
        .get_table_schema(&handle, "users", &cfg.database_a, None)
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
    assert!(err.iter().any(|t| t.name == "users"));

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

    // ── closing the foreign database pool drops only that pool ──
    assert!(
        driver
            .close_database(&handle, &cfg.database_a)
            .await
            .expect("close foreign pool"),
        "the foreign pool should have been open"
    );
    let schema_after_close = driver
        .get_table_schema(&handle, "users", &cfg.database_a, Some("public"))
        .await
        .expect("reopening the foreign pool on demand");
    assert!(!schema_after_close.columns.is_empty());

    driver.disconnect(handle).await.expect("disconnect");
    println!("✅  PostgreSQL cross-database live checks passed");
}
