//! R2-BUG-001 regression (live): an **empty schema** in the catalog used to
//! leak into `dump_database` as a blank identifier — the driver lists one
//! navigation-only marker row per schema without relations (`get_tables`,
//! typed `SystemTable`, `name = ""`), and the dump pipeline turned it into
//! `-- Table:` + `CREATE TABLE IF NOT EXISTS "" (...)`. Restoring such an
//! artifact fails with `zero-length delimited identifier at or near ""`.
//!
//! Skips when PostgreSQL is unavailable. Credentials come from process env
//! and/or `packages/drivers/.env` (`TEST_PG_*` keys, same as the other live
//! tests).
//!
//! Run (skip if no Postgres):
//!   cargo test -p datazen-driver-postgres --test postgres_dump_empty_schema -- --nocapture
//!
//! Fixture: any writable database; the test creates and drops its own throwaway
//! schema so no pre-existing state is required.

use std::collections::HashMap;
use std::path::PathBuf;

use datazen_driver_api::{
    BackupDumpOptions, ConnectionConfig, ConnectionHandle, DatabaseDriver, DriverError,
};
use datazen_driver_postgres::PostgresDriver;

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

/// Gate on `TEST_PG_*` in process env or `packages/drivers/.env`.
fn load_pg_config() -> Option<(String, u16, String, String, String)> {
    let file = load_dotenv_file();
    let has_marker = std::env::vars().any(|(k, _)| k.starts_with("TEST_PG_"))
        || file.keys().any(|k| k.starts_with("TEST_PG_"));
    if !has_marker {
        eprintln!("⏭  Skipping postgres_dump_empty_schema: no TEST_PG_* in env or .env");
        return None;
    }

    let host = env_or_file(&file, "TEST_PG_HOST").unwrap_or_else(|| "127.0.0.1".into());
    let port = env_or_file(&file, "TEST_PG_PORT")
        .and_then(|v| v.parse().ok())
        .unwrap_or(5432);
    let user = env_or_file(&file, "TEST_PG_USER").unwrap_or_else(|| "postgres".into());
    let password = env_or_file(&file, "TEST_PG_PASSWORD").unwrap_or_default();
    let database = env_or_file(&file, "TEST_PG_DATABASE").unwrap_or_else(|| "postgres".into());
    Some((host, port, user, password, database))
}

#[tokio::test]
async fn dump_database_omits_empty_schema_marker_rows() {
    let Some((host, port, user, password, database)) = load_pg_config() else {
        return;
    };

    let config = ConnectionConfig {
        id: "pg-r2-bug001-it".into(),
        name: "postgres r2 bug001 empty-schema marker".into(),
        database_type: "postgresql".into(),
        host: Some(host.clone()),
        port: Some(port),
        database: Some(database.clone()),
        schema: None,
        username: Some(user.clone()),
        password: Some(password.clone()),
        ssl_mode: Default::default(),
        connection_timeout: 5,
        max_pool_size: 5,
        ssh_tunnel: None,
        color_tag: None,
        group: None,
        last_connected_at: None,
        server_version: None,
        options: None,
        read_only: false,
        pinned: false,
    };

    let driver = PostgresDriver::new();
    let handle: ConnectionHandle = match driver.connect(&config).await {
        Ok(h) => h,
        Err(e) => {
            eprintln!("⏭  Skipping: cannot connect to PostgreSQL at {host}:{port}: {e}");
            return;
        }
    };

    // Arrange the defect precondition: one schema with zero relations.
    let schema_name = format!(
        "datazen_r2_bug001_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or_default()
    );
    if let Err(e) = driver
        .execute(&handle, &format!("CREATE SCHEMA \"{schema_name}\""))
        .await
    {
        let _ = driver.disconnect(handle).await;
        eprintln!("⏭  Skipping: cannot create fixture schema (no CREATE privilege?): {e}");
        return;
    }

    let result = run_assertions(&driver, &handle, &database, &schema_name).await;

    let _ = driver
        .execute(
            &handle,
            &format!("DROP SCHEMA IF EXISTS \"{schema_name}\" CASCADE"),
        )
        .await;
    let _ = driver.disconnect(handle).await;

    if let Err(err) = result {
        panic!("{err}");
    }
}

async fn run_assertions(
    driver: &PostgresDriver,
    handle: &ConnectionHandle,
    database: &str,
    schema_name: &str,
) -> Result<(), String> {
    // Precondition proof: the listing carries exactly the navigation marker the
    // old pipeline choked on — SystemTable type + blank name for our schema.
    let tables = driver
        .get_tables(handle, database)
        .await
        .map_err(|e: DriverError| format!("get_tables failed: {e}"))?;
    let marker = tables
        .iter()
        .find(|t| t.schema.as_deref() == Some(schema_name))
        .ok_or_else(|| format!("marker row for `{schema_name}` missing from get_tables"))?;
    assert_eq!(
        marker.name, "",
        "empty schema must be listed as blank-name marker"
    );
    assert!(
        matches!(
            marker.table_type,
            datazen_driver_api::TableType::SystemTable
        ),
        "marker row must be typed SystemTable, got {:?}",
        marker.table_type
    );

    // Act: full backup of this database (clean exercises the DROP path too).
    let opts = BackupDumpOptions {
        clean: true,
        ..Default::default()
    };
    let sql = driver
        .dump_database(handle, database, &opts)
        .await
        .map_err(|e| format!("dump_database failed: {e}"))?;

    // Assert: no zero-length identifier may appear anywhere in the artifact.
    assert!(
        !sql.contains("\"\""),
        "R2-BUG-001 regression: dump contains a zero-length identifier\n{sql}"
    );
    assert!(
        !sql.contains("-- Table: \n"),
        "R2-BUG-001 regression: dump contains a blank object comment\n{sql}"
    );
    Ok(())
}
