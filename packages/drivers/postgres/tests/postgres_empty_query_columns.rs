//! Gated live test: empty SELECT result sets still expose column metadata.
//!
//! Skips when PostgreSQL is unavailable. Uses the same TEST_PG_* env / `.env` as
//! `postgres_use_database`.
//!
//! Run:
//!   cargo test -p datazen-driver-postgres --test postgres_empty_query_columns -- --nocapture

use std::collections::HashMap;
use std::path::PathBuf;

use datazen_driver_api::{ConnectionConfig, DatabaseDriver, Value};
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
            user: "goecoride".into(),
            password: String::new(),
            database: "goecoride".into(),
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

fn load_pg_config() -> Option<PgTestConfig> {
    let file = load_dotenv_file();
    let has_marker = std::env::vars().any(|(k, _)| k.starts_with("TEST_PG_"))
        || file.keys().any(|k| k.starts_with("TEST_PG_"));

    if !has_marker {
        eprintln!("⏭  Skipping postgres_empty_query_columns: no TEST_PG_* in env or .env");
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
        color_tag: None,
        group: None,
        last_connected_at: None,
        server_version: None,
        options: None,
        read_only: false,
        pinned: false,
    }
}

fn cell_as_string(value: &Option<Value>) -> Option<&str> {
    match value {
        Some(Value::String(s)) => Some(s.as_str()),
        _ => None,
    }
}

/// Same shape as `datazen_driver_api::schema_objects::list_objects_sql` for PostgreSQL functions, forced empty.
const EMPTY_FUNCTION_LIST_SQL: &str = "SELECT n.nspname AS schema, p.proname AS name \
     FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace \
     WHERE false \
     ORDER BY 1, 2";

#[tokio::test]
async fn empty_select_still_returns_column_metadata() {
    let Some(cfg) = load_pg_config() else {
        return;
    };

    let driver = PostgresDriver::new();
    let config = sample_config(&cfg, "pg-empty-cols");
    let handle = driver
        .connect(&config)
        .await
        .expect("connect to postgres for empty-query column test");

    let result = driver
        .query(&handle, EMPTY_FUNCTION_LIST_SQL)
        .await
        .expect("empty function list query should succeed");

    assert!(
        result.rows.is_empty(),
        "fixture SQL should return no rows, got {}",
        result.rows.len()
    );
    assert!(
        !result.columns.is_empty(),
        "empty result set should still expose column metadata"
    );
    let names: Vec<&str> = result.columns.iter().map(|c| c.name.as_str()).collect();
    assert!(
        names.iter().any(|n| n.eq_ignore_ascii_case("schema")),
        "expected schema column, got {names:?}"
    );
    assert!(
        names.iter().any(|n| n.eq_ignore_ascii_case("name")),
        "expected name column, got {names:?}"
    );

    driver.disconnect(handle).await.ok();
}

#[tokio::test]
async fn empty_function_list_rows_are_parsed_when_present() {
    let Some(cfg) = load_pg_config() else {
        return;
    };

    let driver = PostgresDriver::new();
    let config = sample_config(&cfg, "pg-fn-list");
    let handle = driver.connect(&config).await.expect("connect");

    let create_sql = "CREATE OR REPLACE FUNCTION public.dz_empty_cols_probe() \
        RETURNS int LANGUAGE sql AS $$ SELECT 1 $$";
    driver
        .execute(&handle, create_sql)
        .await
        .expect("create probe fn");

    let list_sql = "SELECT n.nspname AS schema, p.proname AS name \
        FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace \
        WHERE n.nspname = 'public' AND p.proname = 'dz_empty_cols_probe' \
        ORDER BY 1, 2";
    let result = driver
        .query(&handle, list_sql)
        .await
        .expect("list probe fn");

    assert_eq!(result.rows.len(), 1);
    assert!(!result.columns.is_empty());
    let schema_idx = result
        .columns
        .iter()
        .position(|c| c.name.eq_ignore_ascii_case("schema"))
        .expect("schema column");
    let name_idx = result
        .columns
        .iter()
        .position(|c| c.name.eq_ignore_ascii_case("name"))
        .expect("name column");
    assert_eq!(cell_as_string(&result.rows[0][schema_idx]), Some("public"));
    assert_eq!(
        cell_as_string(&result.rows[0][name_idx]),
        Some("dz_empty_cols_probe")
    );

    driver
        .execute(
            &handle,
            "DROP FUNCTION IF EXISTS public.dz_empty_cols_probe()",
        )
        .await
        .ok();
    driver.disconnect(handle).await.ok();
}
