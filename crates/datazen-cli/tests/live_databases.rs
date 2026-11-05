//! Opt-in live-driver contract tests.
//!
//! Skipped by default: a local PostgreSQL/MySQL instance is not guaranteed in
//! CI or on a contributor machine. Run with `--ignored` and the corresponding
//! `DATAZEN_TEST_*` variables when an instance is available.

use std::process::{Command, Output};
use std::time::Instant;

use serde_json::{json, Value};
use tempfile::TempDir;

fn run(dir: &str, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_datazen"))
        .arg("--data-dir")
        .arg(dir)
        .args(args)
        .output()
        .expect("run datazen")
}

fn write_connections(dir: &std::path::Path, connection: Value) {
    std::fs::write(
        dir.join("connections.json"),
        serde_json::to_vec(&[connection]).unwrap(),
    )
    .unwrap();
}

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

fn connection(
    kind: &str,
    host: &str,
    port: u16,
    database: &str,
    username: &str,
    password: &str,
) -> Value {
    json!({
        "id": "live",
        "name": format!("live-{kind}"),
        "databaseType": kind,
        "host": host,
        "port": port,
        "database": database,
        "username": username,
        "password": password,
        "sshTunnel": null,
        "httpProxyTunnel": null,
        "websocketTunnel": null
    })
}

#[test]
#[ignore = "requires a local PostgreSQL instance; set DATAZEN_TEST_PG_*"]
fn postgres_live_query_contract() {
    let Some(port) = std::env::var("DATAZEN_TEST_PG_PORT").ok() else {
        return;
    };
    let temp = TempDir::new().unwrap();
    write_connections(
        temp.path(),
        connection(
            "postgresql",
            &env_or("DATAZEN_TEST_PG_HOST", "127.0.0.1"),
            port.parse().unwrap(),
            &env_or("DATAZEN_TEST_PG_DATABASE", "postgres"),
            &env_or("DATAZEN_TEST_PG_USER", "postgres"),
            &env_or("DATAZEN_TEST_PG_PASSWORD", ""),
        ),
    );
    let output = run(
        temp.path().to_str().unwrap(),
        &["query", "live", "SELECT 1 AS n", "--json"],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["rows"], json!([[1]]));
}

#[test]
#[ignore = "requires a local PostgreSQL instance; set DATAZEN_TEST_PG_*"]
fn postgres_limit_is_pushed_into_execution() {
    let Some(port) = std::env::var("DATAZEN_TEST_PG_PORT").ok() else {
        return;
    };
    let temp = TempDir::new().unwrap();
    write_connections(
        temp.path(),
        connection(
            "postgresql",
            &env_or("DATAZEN_TEST_PG_HOST", "127.0.0.1"),
            port.parse().unwrap(),
            &env_or("DATAZEN_TEST_PG_DATABASE", "postgres"),
            &env_or("DATAZEN_TEST_PG_USER", "postgres"),
            &env_or("DATAZEN_TEST_PG_PASSWORD", ""),
        ),
    );
    let started = Instant::now();
    let output = run(
        temp.path().to_str().unwrap(),
        &[
            "query",
            "live",
            "/* lead */ SELECT i, pg_sleep(0.1) FROM generate_series(1, 10) AS i -- tail",
            "--limit",
            "1",
            "--timeout",
            "2s",
            "--json",
        ],
    );
    let elapsed = started.elapsed();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["rows"].as_array().unwrap().len(), 1);
    assert_eq!(value["truncated"], true);
    // Without a pushed-down LIMIT this takes ~1s; two rows take ~0.2s.
    assert!(elapsed.as_millis() < 800, "query took {elapsed:?}");
}

#[test]
#[ignore = "requires a local MySQL instance; set DATAZEN_TEST_MYSQL_*"]
fn mysql_live_query_contract() {
    let Some(port) = std::env::var("DATAZEN_TEST_MYSQL_PORT").ok() else {
        return;
    };
    let temp = TempDir::new().unwrap();
    write_connections(
        temp.path(),
        connection(
            "mysql",
            &env_or("DATAZEN_TEST_MYSQL_HOST", "127.0.0.1"),
            port.parse().unwrap(),
            &env_or("DATAZEN_TEST_MYSQL_DATABASE", "mysql"),
            &env_or("DATAZEN_TEST_MYSQL_USER", "root"),
            &env_or("DATAZEN_TEST_MYSQL_PASSWORD", ""),
        ),
    );
    let output = run(
        temp.path().to_str().unwrap(),
        &["query", "live", "SELECT 1 AS n", "--json"],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["rows"], json!([[1]]));
}
