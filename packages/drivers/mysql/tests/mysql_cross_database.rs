//! Gated live integration test: MySQL **cross-database reads**.
//!
//! The driver has no `use_database`. A metadata read for another database is
//! served by fully qualifying the name (`` `db`.`table` `` for the `SHOW`
//! family, `WHERE TABLE_SCHEMA = ?` for `information_schema`), so the borrowed
//! pool connection is never re-pointed and no `USE` leaks back into the pool.
//!
//! Skips cleanly when MySQL is unavailable. Credentials come from process env
//! and/or the repo-root `.env` file (same `TEST_MYSQL_*` keys as workflow tests).
//!
//! Run (skip if no MySQL):
//!   cargo test -p datazen-driver-mysql --test mysql_cross_database -- --nocapture
//!
//! Force live run with env (example — use your own secrets, do not commit them):
//!   TEST_MYSQL_HOST=127.0.0.1 TEST_MYSQL_PORT=3306 TEST_MYSQL_USER=root \
//!   TEST_MYSQL_PASSWORD= TEST_MYSQL_DATABASE=datazen_test \
//!   TEST_MYSQL_DATABASE_B=datazen_sync_mysql_tgt \
//!   cargo test -p datazen-driver-mysql --test mysql_cross_database -- --nocapture

use std::collections::HashMap;
use std::path::PathBuf;

use datazen_driver_api::{ConnectionConfig, DatabaseDriver, DriverError};
use datazen_driver_mysql::MysqlDriver;

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
        id: "mysql-cross-database-it".into(),
        name: "mysql cross-database integration".into(),
        database_type: "mysql".into(),
        host: Some(cfg.host.clone()),
        port: Some(cfg.port),
        // Connect to database_b so database_a is always a foreign database.
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

#[tokio::test]
async fn cross_database_reads_use_qualified_names_not_use() {
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
            eprintln!(
                "⏭  Skipping: cannot connect to MySQL at {}:{}: {e}",
                cfg.host, cfg.port
            );
            return;
        }
    };

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
        .get_tables(&handle, &cfg.database_a, None)
        .await
        .unwrap_or_else(|e| panic!("get_tables({}): {e}", cfg.database_a));
    if !a_tables.iter().any(|t| t.name == "users") {
        let _ = driver.disconnect(handle).await;
        eprintln!(
            "⏭  Skipping: `{}.users` missing (needed to verify cross-database reads)",
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

    // ── BUG-003 regression: a foreign database's table structure must not be empty ──
    let schema = driver
        .get_table_schema(&handle, "users", &cfg.database_a, None)
        .await
        .unwrap_or_else(|e| panic!("get_table_schema({}.users): {e}", cfg.database_a));
    assert!(
        !schema.columns.is_empty(),
        "cross-database structure must not come back empty — that empty column \
         list is what the data grid turned into blank cells (BUG-003)"
    );
    let (cols, _pks) = driver
        .get_columns(&handle, "users", &cfg.database_a, None)
        .await
        .unwrap_or_else(|e| panic!("get_columns({}.users): {e}", cfg.database_a));
    assert_eq!(cols.len(), schema.columns.len());

    // ── the handle's own session is untouched: unqualified `users` is still
    //    absent from database_b ──
    let err = driver
        .query(&handle, "SELECT COUNT(*) FROM users")
        .await
        .expect_err("the handle's session must stay on database_b and not see database_a.users");
    assert!(
        matches!(err, DriverError::QueryFailed(_)),
        "expected QueryFailed, got {err:?}"
    );

    // ── repeated foreign reads stay stable (no per-acquire state to drift) ──
    for i in 0..5 {
        let schema = driver
            .get_table_schema(&handle, "users", &cfg.database_a, None)
            .await
            .unwrap_or_else(|e| panic!("cross-database read #{i}: {e}"));
        assert_eq!(schema.columns.len(), schema.columns.len(), "shape changed");
    }
    let err = driver
        .query(&handle, "SELECT COUNT(*) FROM users")
        .await
        .expect_err("session must still be on database_b after repeated foreign reads");
    assert!(matches!(err, DriverError::QueryFailed(_)), "got {err:?}");

    // ── a genuinely missing relation errors instead of reporting no columns ──
    let err = driver
        .get_table_schema(&handle, "users", &cfg.database_b, None)
        .await
        .expect_err("a relation absent from the requested database must not report success");
    assert!(
        err.to_string().contains("does not exist"),
        "error should name the missing relation, got: {err}"
    );

    // ── MySQL has no schema level, so a schema argument is rejected ──
    let err = driver
        .get_table_schema(&handle, "users", &cfg.database_a, Some("public"))
        .await
        .expect_err("MySQL must reject a schema argument");
    assert!(
        matches!(err, DriverError::InvalidConfig(_)),
        "expected InvalidConfig, got {err:?}"
    );

    // ── unknown database fails loudly ──
    let err = driver
        .get_tables(&handle, "nonexistent_db_xyz_f1_test", None)
        .await
        .expect_err("unknown database should error");
    assert!(
        matches!(err, DriverError::QueryFailed(_)),
        "expected QueryFailed for unknown database, got: {err:?}"
    );

    driver.disconnect(handle).await.expect("disconnect");
    println!("✅  MySQL cross-database live checks passed");
}
