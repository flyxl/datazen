//! Gated live test: schema reads resolve against the **requested** database,
//! and a genuinely missing relation still fails loudly.
//!
//! Regression (BUG-003): `get_table_schema` / `get_columns` had no database
//! dimension — they resolved `information_schema` against the session's active
//! database. A read for a table that lives in another catalog therefore found
//! no rows and was reported as `Ok(TableSchema { columns: [] })`, i.e. a table
//! that exists with zero columns. Callers cached that answer, blanking the
//! structure view, the ER diagram and the data grid for the whole cache TTL.
//!
//! Both halves are asserted here: the foreign-database read now *succeeds*
//! (the driver picks that database's pool), while a relation that really is
//! absent keeps failing instead of degrading to a column-less table.
//!
//! Skips when PostgreSQL is unavailable. Uses `TEST_PG_*` from the **process
//! environment only** (shell export or CI secret) — see
//! `postgres_cross_database.rs` for why no `.env` file is read.
//!
//! Run:
//!   cargo test -p datazen-driver-postgres --test schema_missing_table -- --nocapture

use datazen_driver_api::{ConnectionConfig, DatabaseDriver, DriverError};
use datazen_driver_postgres::PostgresDriver;

#[derive(Clone, Debug)]
struct PgTestConfig {
    host: String,
    port: u16,
    user: String,
    password: String,
    database: String,
}

impl Default for PgTestConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 5432,
            user: "postgres".into(),
            password: String::new(),
            database: "postgres".into(),
        }
    }
}

/// Resolve a `TEST_PG_*` key from the **process environment only**.
///
/// No file fallback: a test must never parse `packages/drivers/.env`
/// (AGENTS.md 「本地环境变量文件保护」). Inject the keys from the shell or CI.
fn env_var(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

fn load_pg_config() -> Option<PgTestConfig> {
    let has_marker = std::env::vars().any(|(k, _)| k.starts_with("TEST_PG_"));

    if !has_marker {
        eprintln!("⏭  Skipping schema_missing_table: no TEST_PG_* in process env");
        return None;
    }

    let mut cfg = PgTestConfig::default();
    if let Some(v) = env_var("TEST_PG_HOST") {
        cfg.host = v;
    }
    if let Some(v) = env_var("TEST_PG_PORT") {
        cfg.port = v.parse().unwrap_or(cfg.port);
    }
    if let Some(v) = env_var("TEST_PG_USER") {
        cfg.user = v;
    }
    if let Some(v) = env_var("TEST_PG_PASSWORD") {
        cfg.password = v;
    }
    if let Some(v) = env_var("TEST_PG_DATABASE") {
        cfg.database = v;
    }
    Some(cfg)
}

fn sample_config(cfg: &PgTestConfig, id: &str) -> ConnectionConfig {
    ConnectionConfig {
        id: id.into(),
        name: id.into(),
        database_type: "postgresql".into(),
        host: Some(cfg.host.clone()),
        port: Some(cfg.port),
        database: Some(cfg.database.clone()),
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

#[tokio::test]
async fn cross_database_read_resolves_and_missing_relation_still_fails() {
    let Some(cfg) = load_pg_config() else {
        return;
    };

    let driver = PostgresDriver::new();
    let handle = driver
        .connect(&sample_config(&cfg, "pg-missing-table"))
        .await
        .expect("connect to postgres");

    // A probe table that exists only in the connected database.
    let probe = format!("dz_missing_probe_{}", std::process::id());
    driver
        .execute(
            &handle,
            &format!("CREATE TABLE public.{probe} (id int PRIMARY KEY, label text)"),
        )
        .await
        .expect("create probe table");

    // Control: in its own database the table reads normally.
    let schema = driver
        .get_table_schema(&handle, &probe, &cfg.database, Some("public"))
        .await
        .expect("schema of the probe table in its own database");
    assert_eq!(
        schema.columns.len(),
        2,
        "probe table must expose its columns"
    );
    let (columns, primary_keys) = driver
        .get_columns(&handle, &probe, &cfg.database, Some("public"))
        .await
        .expect("columns of the probe table in its own database");
    assert_eq!(columns.len(), 2);
    assert_eq!(primary_keys, vec!["id".to_string()]);

    // Reading the *same* table name from a database that does not hold it must
    // fail. Before the fix this returned an empty column list that callers then
    // cached, which is what blanked the data grid.
    let other = "postgres";
    if other != cfg.database {
        let err = driver
            .get_table_schema(&handle, &probe, other, Some("public"))
            .await
            .expect_err("a relation absent from the requested database must not report success");
        assert!(
            err.to_string().contains("does not exist"),
            "error should name the missing relation, got: {err}"
        );

        let err = driver
            .get_columns(&handle, &probe, other, Some("public"))
            .await
            .expect_err("column read for an absent relation must fail");
        assert!(err.to_string().contains("does not exist"), "got: {err}");

        // The failed read must not have poisoned the correct one: no session was
        // re-pointed, so the original database still reads normally.
        let schema = driver
            .get_table_schema(&handle, &probe, &cfg.database, Some("public"))
            .await
            .expect("schema after a failed cross-database read");
        assert_eq!(schema.columns.len(), 2);
    }

    // A schema is mandatory when resolving a single table on PostgreSQL.
    let err = driver
        .get_table_schema(&handle, &probe, &cfg.database, None)
        .await
        .expect_err("schema-aware resolution must require an explicit schema");
    assert!(
        matches!(err, DriverError::InvalidConfig(_)),
        "expected InvalidConfig, got: {err:?}"
    );

    driver
        .execute(&handle, &format!("DROP TABLE IF EXISTS public.{probe}"))
        .await
        .ok();
    driver.disconnect(handle).await.ok();
}
