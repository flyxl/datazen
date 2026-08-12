//! Gated live integration test for `PostgresDriver::use_database` and
//! `get_tables` catalog targeting.
//!
//! Skips cleanly when PostgreSQL is unavailable. Credentials come from process
//! env and/or the repo-root `.env` file (`TEST_PG_*` keys, same as workflow tests).
//!
//! Run (skip if no Postgres):
//!   cargo test -p datazen-driver-postgres --test postgres_use_database -- --nocapture
//!
//! Force live run with env (example — use your own secrets, do not commit them):
//!   TEST_PG_HOST=127.0.0.1 TEST_PG_PORT=5432 TEST_PG_USER=goecoride \
//!   TEST_PG_PASSWORD= TEST_PG_DATABASE=goecoride \
//!   TEST_PG_DATABASE_B=postgres \
//!   cargo test -p datazen-driver-postgres --test postgres_use_database -- --nocapture
//!
//! Fixture assumption: database_a has a `users` table; database_b does not.

use std::collections::HashMap;
use std::path::PathBuf;

use datazen_driver_postgres::PostgresDriver;
use datazen_driver_api::{ConnectionConfig, DatabaseDriver, DriverError, Value};

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
            user: "goecoride".into(),
            password: String::new(),
            database_a: "goecoride".into(),
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
        if let Some((k, v)) = line.split_once('=') {
            map.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    map
}

fn env_or_file(file: &HashMap<String, String>, key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .filter(|v| !v.is_empty())
        .or_else(|| file.get(key).cloned().filter(|v| !v.is_empty()))
}

/// Gate on `TEST_PG_*` in process env or repo-root `.env`.
/// Password may be intentionally empty.
fn load_pg_config() -> Option<PgTestConfig> {
    let file = load_dotenv_file();
    let has_marker = std::env::vars().any(|(k, _)| k.starts_with("TEST_PG_"))
        || file.keys().any(|k| k.starts_with("TEST_PG_"));

    if !has_marker {
        eprintln!("⏭  Skipping postgres_use_database: no TEST_PG_* in env or .env");
        return None;
    }

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

fn connection_config(cfg: &PgTestConfig) -> ConnectionConfig {
    ConnectionConfig {
        id: "pg-use-database-it".into(),
        name: "postgres use_database integration".into(),
        database_type: "postgresql".into(),
        host: Some(cfg.host.clone()),
        port: Some(cfg.port),
        // Connect without a default database so use_database is the switcher
        // (driver falls back to `postgres` for the initial listing connection).
        database: None,
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
    }
}

fn cell_as_i64(value: &Option<Value>) -> Option<i64> {
    match value {
        Some(Value::Integer(i)) => Some(*i),
        Some(Value::String(s)) => s.parse().ok(),
        _ => None,
    }
}

async fn assert_users_visible(
    driver: &PostgresDriver,
    handle: &datazen_driver_api::ConnectionHandle,
    label: &str,
) {
    let result = driver
        .query(handle, "SELECT COUNT(*) FROM users")
        .await
        .unwrap_or_else(|e| panic!("{label}: unqualified users query failed: {e}"));
    assert_eq!(result.rows.len(), 1, "{label}: expected one count row");
    let count = cell_as_i64(&result.rows[0][0]).unwrap_or(-1);
    assert!(
        count >= 0,
        "{label}: expected non-negative users count, got {count}"
    );
}

#[tokio::test]
async fn use_database_switches_and_get_tables_respects_catalog() {
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

    // get_tables must target the *named* catalog even before use_database.
    let a_tables = driver
        .get_tables(&handle, &cfg.database_a)
        .await
        .unwrap_or_else(|e| panic!("get_tables({}): {e}", cfg.database_a));
    if !a_tables.iter().any(|t| t.name == "users") {
        let _ = driver.disconnect(handle).await;
        eprintln!(
            "⏭  Skipping: `{}.users` missing (needed to verify catalog targeting / use_database)",
            cfg.database_a
        );
        return;
    }
    let b_tables = driver
        .get_tables(&handle, &cfg.database_b)
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
        "▶  use_database live: {} → {} on {}:{}",
        cfg.database_a, cfg.database_b, cfg.host, cfg.port
    );

    driver
        .use_database(&handle, &cfg.database_a)
        .await
        .unwrap_or_else(|e| panic!("use_database({}) failed: {e}", cfg.database_a));

    for i in 0..5 {
        assert_users_visible(
            &driver,
            &handle,
            &format!("after use_database(A), pooled query #{i}"),
        )
        .await;
    }

    // After switching to A, get_tables(B) must still return B's catalog (not A's).
    let b_after = driver
        .get_tables(&handle, &cfg.database_b)
        .await
        .unwrap_or_else(|e| panic!("get_tables({}) after switch to A: {e}", cfg.database_b));
    assert!(
        !b_after.iter().any(|t| t.name == "users"),
        "get_tables(B) must not leak A's users table"
    );

    driver
        .use_database(&handle, &cfg.database_b)
        .await
        .unwrap_or_else(|e| panic!("use_database({}) failed: {e}", cfg.database_b));

    for i in 0..5 {
        let err = driver
            .query(&handle, "SELECT COUNT(*) FROM users")
            .await
            .expect_err(&format!(
                "after use_database(B), pooled query #{i} should not see {}.users",
                cfg.database_a
            ));
        assert!(
            matches!(err, DriverError::QueryFailed(_)),
            "pooled query #{i}: expected QueryFailed without users table, got {err:?}"
        );
    }

    driver
        .use_database(&handle, &cfg.database_a)
        .await
        .unwrap_or_else(|e| panic!("use_database({}) again failed: {e}", cfg.database_a));
    assert_users_visible(&driver, &handle, "after switching back to A").await;

    let err = driver
        .use_database(&handle, "nonexistent_db_xyz_f3_test")
        .await
        .expect_err("use_database(invalid) should error");
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

    // Active DB should remain A after failed switch.
    assert_users_visible(&driver, &handle, "after failed use_database(invalid)").await;

    let empty_err = driver
        .use_database(&handle, "   ")
        .await
        .expect_err("empty database name should be rejected");
    assert!(
        matches!(empty_err, DriverError::InvalidConfig(_)),
        "expected InvalidConfig for empty name, got: {empty_err:?}"
    );

    driver.disconnect(handle).await.expect("disconnect");
    println!("✅  PostgresDriver::use_database live checks passed");
}
