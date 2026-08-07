//! Gated live integration test for `MysqlDriver::use_database`.
//!
//! Skips cleanly when MySQL is unavailable. Credentials come from process env
//! and/or the repo-root `.env` file (same `TEST_MYSQL_*` keys as workflow tests).
//!
//! Run (skip if no MySQL):
//!   cargo test -p datazen --test mysql_use_database -- --nocapture
//!
//! Force live run with env (example — use your own secrets, do not commit them):
//!   TEST_MYSQL_HOST=127.0.0.1 TEST_MYSQL_PORT=3306 TEST_MYSQL_USER=root \
//!   TEST_MYSQL_PASSWORD= TEST_MYSQL_DATABASE=datazen_test \
//!   TEST_MYSQL_DATABASE_B=datazen_sync_mysql_tgt \
//!   cargo test -p datazen --test mysql_use_database -- --nocapture
//!
//! Note: do not verify the active schema with prepared `SELECT DATABASE()` —
//! MySQL can return the database from PREPARE time after a later `USE`. This
//! test uses unqualified table access (and text-protocol checks in the driver).

use std::collections::HashMap;
use std::path::PathBuf;

use datazen::db::mysql::MysqlDriver;
use datazen::db::{ConnectionConfig, DatabaseDriver, DriverError, Value};

#[derive(Clone, Debug)]
struct MysqlTestConfig {
    host: String,
    port: u16,
    user: String,
    password: String,
    database_a: String,
    database_b: String,
}

impl Default for MysqlTestConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 3306,
            user: "root".into(),
            password: String::new(),
            database_a: "datazen_test".into(),
            database_b: "datazen_sync_mysql_tgt".into(),
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

/// Gate on `TEST_MYSQL_*` in process env or repo-root `.env`.
/// Password may be intentionally empty.
fn load_mysql_config() -> Option<MysqlTestConfig> {
    let file = load_dotenv_file();
    let has_marker = std::env::vars().any(|(k, _)| k.starts_with("TEST_MYSQL_"))
        || file.keys().any(|k| k.starts_with("TEST_MYSQL_"));

    if !has_marker {
        eprintln!("⏭  Skipping mysql_use_database: no TEST_MYSQL_* in env or .env");
        return None;
    }

    let mut cfg = MysqlTestConfig::default();

    // Prefer explicit TEST_MYSQL_* (process env wins over .env).
    if let Some(v) = env_or_file(&file, "TEST_MYSQL_HOST") {
        cfg.host = v;
    }
    if let Some(v) = env_or_file(&file, "TEST_MYSQL_PORT") {
        cfg.port = v.parse().unwrap_or(3306);
    }
    if let Some(v) = env_or_file(&file, "TEST_MYSQL_USER") {
        cfg.user = v;
    }
    // Empty password is valid; allow override from env including empty string.
    if let Ok(v) = std::env::var("TEST_MYSQL_PASSWORD") {
        cfg.password = v;
    } else if let Some(v) = file.get("TEST_MYSQL_PASSWORD") {
        cfg.password = v.clone();
    }
    if let Some(v) = env_or_file(&file, "TEST_MYSQL_DATABASE") {
        cfg.database_a = v;
    }
    if let Some(v) = env_or_file(&file, "TEST_MYSQL_DATABASE_B") {
        cfg.database_b = v;
    }

    Some(cfg)
}

fn connection_config(cfg: &MysqlTestConfig) -> ConnectionConfig {
    ConnectionConfig {
        id: "mysql-use-database-it".into(),
        name: "mysql use_database integration".into(),
        database_type: "mysql".into(),
        host: Some(cfg.host.clone()),
        port: Some(cfg.port),
        // Connect without a default database so use_database is the switcher.
        database: None,
        schema: None,
        username: Some(cfg.user.clone()),
        password: Some(cfg.password.clone()),
        ssl_mode: Default::default(),
        connection_timeout: 5,
        ssh_tunnel: None,
        color_tag: None,
        group: None,
        last_connected_at: None,
        server_version: None,
    }
}

fn cell_as_i64(value: &Option<Value>) -> Option<i64> {
    match value {
        Some(Value::Integer(i)) => Some(*i),
        Some(Value::String(s)) => s.parse().ok(),
        _ => None,
    }
}

/// `datazen_test.users` exists; proves unqualified names resolve after USE.
async fn assert_users_visible(driver: &MysqlDriver, handle: &datazen::db::ConnectionHandle, label: &str) {
    let result = driver
        .query(handle, "SELECT COUNT(*) FROM users")
        .await
        .unwrap_or_else(|e| panic!("{label}: unqualified users query failed: {e}"));
    assert_eq!(result.rows.len(), 1, "{label}: expected one count row");
    let count = cell_as_i64(&result.rows[0][0]).unwrap_or(-1);
    assert!(count >= 0, "{label}: expected non-negative users count, got {count}");
}

#[tokio::test]
async fn use_database_switches_and_rejects_invalid() {
    let Some(cfg) = load_mysql_config() else {
        return;
    };

    if cfg.database_a == cfg.database_b {
        eprintln!(
            "⏭  Skipping: TEST_MYSQL_DATABASE and TEST_MYSQL_DATABASE_B must differ (got {})",
            cfg.database_a
        );
        return;
    }

    let driver = MysqlDriver::new(false);
    let handle = match driver.connect(&connection_config(&cfg)).await {
        Ok(h) => h,
        Err(e) => {
            eprintln!("⏭  Skipping: cannot connect to MySQL at {}:{}: {e}", cfg.host, cfg.port);
            return;
        }
    };

    // Confirm both target databases exist (skip if fixture DBs missing).
    let dbs = match driver.get_databases(&handle).await {
        Ok(d) => d,
        Err(e) => {
            let _ = driver.disconnect(handle).await;
            eprintln!("⏭  Skipping: SHOW DATABASES failed: {e}");
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

    // Fixture assumption: database_a has `users`; database_b does not.
    let a_tables = driver
        .get_tables(&handle, &cfg.database_a)
        .await
        .unwrap_or_else(|e| panic!("get_tables({}): {e}", cfg.database_a));
    if !a_tables.iter().any(|t| t.name == "users") {
        let _ = driver.disconnect(handle).await;
        eprintln!(
            "⏭  Skipping: `{}.users` missing (needed to verify unqualified USE)",
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

    // Pool re-apply: several acquires must all resolve unqualified `users`.
    for i in 0..5 {
        assert_users_visible(
            &driver,
            &handle,
            &format!("after use_database(A), pooled query #{i}"),
        )
        .await;
    }

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

    // Switch back to A and confirm pool re-apply again.
    driver
        .use_database(&handle, &cfg.database_a)
        .await
        .unwrap_or_else(|e| panic!("use_database({}) again failed: {e}", cfg.database_a));
    assert_users_visible(&driver, &handle, "after switching back to A").await;

    // Invalid database → QueryFailed (mapped from MySQL unknown-database).
    let err = driver
        .use_database(&handle, "nonexistent_db_xyz_f1_test")
        .await
        .expect_err("use_database(invalid) should error");
    assert!(
        matches!(err, DriverError::QueryFailed(_)),
        "expected QueryFailed for unknown database, got: {err:?}"
    );
    let msg = err.to_string();
    assert!(
        msg.contains("nonexistent_db_xyz_f1_test") || msg.to_lowercase().contains("unknown"),
        "error should mention the bad database name: {msg}"
    );

    // Active DB should remain A after failed switch (unqualified users still works).
    assert_users_visible(&driver, &handle, "after failed use_database(invalid)").await;

    // Empty name → InvalidConfig (no round-trip to server required).
    let empty_err = driver
        .use_database(&handle, "   ")
        .await
        .expect_err("empty database name should be rejected");
    assert!(
        matches!(empty_err, DriverError::InvalidConfig(_)),
        "expected InvalidConfig for empty name, got: {empty_err:?}"
    );

    driver.disconnect(handle).await.expect("disconnect");
    println!("✅  MysqlDriver::use_database live checks passed");
}
