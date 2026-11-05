use std::path::Path;
use std::process::{Command, Output};

use serde_json::{json, Value};
use tempfile::TempDir;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_datazen"))
}

fn write_connections(dir: &Path, records: Value) {
    std::fs::write(
        dir.join("connections.json"),
        serde_json::to_vec(&records).unwrap(),
    )
    .unwrap();
}

fn sqlite_connection(dir: &Path, name: &str) -> Value {
    let db = dir.join("test.db");
    std::fs::write(&db, b"").unwrap();
    json!({
        "id": "local",
        "name": name,
        "databaseType": "sqlite",
        "database": db,
        "host": null,
        "port": null,
        "schema": null,
        "username": null,
        "password": null,
        "sshTunnel": null,
        "httpProxyTunnel": null,
        "websocketTunnel": null
    })
}

fn run(dir: &Path, args: &[&str]) -> Output {
    run_env(dir, args, &[])
}

fn run_env(dir: &Path, args: &[&str], envs: &[(&str, &str)]) -> Output {
    let mut command = bin();
    command.arg("--data-dir").arg(dir).args(args);
    for (key, value) in envs {
        command.env(key, value);
    }
    command.output().expect("run datazen")
}

#[test]
fn doctor_and_capabilities_report_real_linked_drivers() {
    let temp = TempDir::new().unwrap();
    let doctor = run(temp.path(), &["doctor", "--json"]);
    assert!(doctor.status.success());
    let report: Value = serde_json::from_slice(&doctor.stdout).unwrap();
    assert_eq!(report["ok"], true);
    assert_eq!(report["connectionCount"], 0);
    assert!(report["drivers"]
        .as_array()
        .unwrap()
        .iter()
        .any(|driver| driver["id"] == "sqlite"));

    let capabilities = run(temp.path(), &["capabilities", "--json"]);
    assert!(capabilities.status.success());
    let report: Value = serde_json::from_slice(&capabilities.stdout).unwrap();
    assert_eq!(report["safety"]["defaultReadOnly"], true);
    assert!(report["drivers"]
        .as_array()
        .unwrap()
        .iter()
        .any(|driver| driver["id"] == "postgresql"));
}

#[test]
fn connections_json_and_csv_are_parseable_and_never_print_secrets() {
    let temp = TempDir::new().unwrap();
    let mut connection = sqlite_connection(temp.path(), "local");
    connection["password"] = json!("super-secret-value");
    write_connections(temp.path(), json!([connection]));

    let json_output = run(temp.path(), &["connections", "--json"]);
    assert!(json_output.status.success());
    let rendered = String::from_utf8(json_output.stdout).unwrap();
    assert!(!rendered.contains("super-secret-value"));
    assert!(!rendered.contains("password"));
    let rows: Value = serde_json::from_str(&rendered).unwrap();
    assert_eq!(rows[0]["id"], "local");

    let csv = run(temp.path(), &["connections", "--csv"]);
    assert!(csv.status.success());
    let rendered = String::from_utf8(csv.stdout).unwrap();
    let mut lines = rendered.lines();
    assert!(lines.next().unwrap().starts_with("id,name,databaseType"));
    assert!(lines.next().unwrap().contains("local"));
    assert!(!rendered.contains("super-secret-value"));
}

#[test]
fn file_output_is_written_and_parseable() {
    let temp = TempDir::new().unwrap();
    let connection = sqlite_connection(temp.path(), "local");
    write_connections(temp.path(), json!([connection]));
    let out = temp.path().join("connections.json.out");

    let output = bin()
        .arg("--data-dir")
        .arg(temp.path())
        .args(["connections", "--json", "--file"])
        .arg(&out)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    let parsed: Value = serde_json::from_slice(&std::fs::read(out).unwrap()).unwrap();
    assert_eq!(parsed[0]["name"], "local");
}

#[test]
fn sqlite_query_is_read_only_by_default_and_writes_need_both_flags() {
    let temp = TempDir::new().unwrap();
    let connection = sqlite_connection(temp.path(), "local");
    write_connections(temp.path(), json!([connection]));

    let query = run(temp.path(), &["query", "local", "SELECT 1 AS n", "--json"]);
    assert!(query.status.success());
    let result: Value = serde_json::from_slice(&query.stdout).unwrap();
    assert_eq!(result["rows"], json!([[1]]));
    assert_eq!(result["columns"], json!(["n"]));

    let write = run(
        temp.path(),
        &["query", "local", "CREATE TABLE t (id INTEGER)", "--json"],
    );
    assert_eq!(write.status.code(), Some(6));
    let error: Value = serde_json::from_slice(&write.stderr).unwrap();
    assert_eq!(error["code"], "E_READ_ONLY");

    let dangerous_only = run(
        temp.path(),
        &[
            "query",
            "local",
            "CREATE TABLE t (id INTEGER)",
            "--allow-writes",
            "--json",
        ],
    );
    assert_eq!(dangerous_only.status.code(), Some(8));
    let error: Value = serde_json::from_slice(&dangerous_only.stderr).unwrap();
    assert_eq!(error["code"], "E_DANGEROUS_SQL");
}

#[test]
fn query_limit_and_csv_output_are_parseable() {
    let temp = TempDir::new().unwrap();
    let connection = sqlite_connection(temp.path(), "local");
    write_connections(temp.path(), json!([connection]));

    let output = run(
        temp.path(),
        &[
            "query",
            "local",
            "SELECT 1 AS n UNION ALL SELECT 2",
            "--limit",
            "1",
            "--csv",
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "n\n1\n");
}

#[test]
fn production_refuses_mutating_pragma_with_both_flags() {
    let temp = TempDir::new().unwrap();
    let connection = sqlite_connection(temp.path(), "prod-db");
    write_connections(temp.path(), json!([connection]));

    let denied = run(
        temp.path(),
        &[
            "query",
            "local",
            "PRAGMA journal_mode=WAL",
            "--allow-writes",
            "--allow-dangerous-sql",
            "--json",
        ],
    );
    assert_eq!(
        denied.status.code(),
        Some(9),
        "{}",
        String::from_utf8_lossy(&denied.stderr)
    );
    let error: Value = serde_json::from_slice(&denied.stderr).unwrap();
    assert_eq!(error["code"], "E_PRODUCTION_WRITE");

    let version_denied = run(
        temp.path(),
        &[
            "query",
            "local",
            "PRAGMA user_version=7",
            "--allow-writes",
            "--allow-dangerous-sql",
            "--json",
        ],
    );
    assert_eq!(version_denied.status.code(), Some(9));

    // The rejected statements must not have changed persisted SQLite state.
    let read = run(
        temp.path(),
        &["query", "local", "PRAGMA user_version", "--json"],
    );
    assert!(
        read.status.success(),
        "{}",
        String::from_utf8_lossy(&read.stderr)
    );
    let result: Value = serde_json::from_slice(&read.stdout).unwrap();
    assert_eq!(result["rows"][0][0], 0);

    // Read-only PRAGMA introspection stays available on production.
    for pragma in ["PRAGMA table_info(items)", "PRAGMA main.table_info(items)"] {
        let introspection = run(temp.path(), &["query", "local", pragma, "--json"]);
        assert!(
            introspection.status.success(),
            "{pragma}: {}",
            String::from_utf8_lossy(&introspection.stderr)
        );
    }
}

#[test]
fn tunnel_config_fails_closed_instead_of_direct_connect() {
    let temp = TempDir::new().unwrap();
    let mut connection = sqlite_connection(temp.path(), "tunneled");
    connection["sshTunnel"] = json!({
        "enabled": true,
        "host": "127.0.0.1",
        "port": 1,
        "username": "nobody"
    });
    write_connections(temp.path(), json!([connection]));

    let output = run(temp.path(), &["query", "local", "SELECT 1", "--json"]);
    assert_eq!(output.status.code(), Some(14));
    let error: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error["code"], "E_UNSUPPORTED");
    assert!(error["message"].as_str().unwrap().contains("tunnel"));
}

#[cfg(unix)]
#[test]
fn open_launches_desktop_with_argv_and_never_emits_a_uri() {
    use std::os::unix::fs::PermissionsExt;

    let temp = TempDir::new().unwrap();
    let connection = sqlite_connection(temp.path(), "local");
    write_connections(temp.path(), json!([connection]));
    let args_file = temp.path().join("launch-args.txt");
    let script = temp.path().join("fake-datazen");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n",
            args_file.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

    let output = run_env(
        temp.path(),
        &["open", "local", "--query", "SELECT 1", "--json"],
        &[("DATAZEN_DESKTOP_BIN", script.to_str().unwrap())],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("datazen://"));
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["launched"], true);
    assert_eq!(report["queryProvided"], true);

    let mut args = String::new();
    for _ in 0..200 {
        if let Ok(value) = std::fs::read_to_string(&args_file) {
            args = value;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(args.contains("--datazen-open-connection"), "{args}");
    assert!(args.contains("--datazen-open-query"), "{args}");
    assert!(args.contains("SELECT 1"), "{args}");
    assert!(args.contains("--datazen-data-dir"), "{args}");
    assert!(
        args.contains(temp.path().to_str().unwrap()),
        "data dir missing from {args}"
    );
}

#[test]
fn open_help_describes_desktop_dispatch_and_query() {
    let temp = TempDir::new().unwrap();
    let output = run(temp.path(), &["open", "--help"]);
    assert!(output.status.success());
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(
        help.contains("Open a saved connection in the DataZen desktop app"),
        "{help}"
    );
    assert!(help.contains("--query"), "{help}");
    assert!(help.contains("SQL to preload"), "{help}");
}

#[test]
fn open_missing_desktop_is_not_found_without_emitting_a_uri() {
    let temp = TempDir::new().unwrap();
    let connection = sqlite_connection(temp.path(), "local");
    write_connections(temp.path(), json!([connection]));

    let output = run_env(
        temp.path(),
        &["open", "local", "--json"],
        &[("DATAZEN_DESKTOP_BIN", "/definitely/missing/datazen-desktop")],
    );
    assert_eq!(output.status.code(), Some(4));
    assert!(output.stdout.is_empty());
    let error: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error["code"], "E_NOT_FOUND");
    assert!(!String::from_utf8_lossy(&output.stderr).contains("datazen://"));
}

#[test]
fn production_writes_are_refused_with_both_flags() {
    let temp = TempDir::new().unwrap();
    let connection = sqlite_connection(temp.path(), "prod-db");
    write_connections(temp.path(), json!([connection]));

    let output = run(
        temp.path(),
        &[
            "query",
            "local",
            "CREATE TABLE t (id INTEGER)",
            "--allow-writes",
            "--allow-dangerous-sql",
            "--json",
        ],
    );
    assert_eq!(output.status.code(), Some(9));
    let error: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error["code"], "E_PRODUCTION_WRITE");
    assert!(error["message"].as_str().unwrap().contains("production"));
}

#[test]
fn unknown_connection_and_encrypted_secret_have_stable_codes() {
    let temp = TempDir::new().unwrap();
    write_connections(temp.path(), json!([]));
    let missing = run(temp.path(), &["query", "missing", "SELECT 1", "--json"]);
    assert_eq!(missing.status.code(), Some(4));
    let error: Value = serde_json::from_slice(&missing.stderr).unwrap();
    assert_eq!(error["code"], "E_NOT_FOUND");

    let mut connection = sqlite_connection(temp.path(), "local");
    // Valid base64 and >= 12 bytes: shaped like AES-GCM ciphertext.
    connection["password"] = json!("c3VwZXItc2VjcmV0LXZhbHVl");
    write_connections(temp.path(), json!([connection]));
    let denied = run(temp.path(), &["query", "local", "SELECT 1", "--json"]);
    assert_eq!(denied.status.code(), Some(5));
    let error: Value = serde_json::from_slice(&denied.stderr).unwrap();
    assert_eq!(error["code"], "E_CREDENTIAL");
    assert!(!String::from_utf8_lossy(&denied.stderr).contains("super-secret-value"));
}

#[test]
fn schema_uses_the_real_driver_catalog() {
    let temp = TempDir::new().unwrap();
    let connection = sqlite_connection(temp.path(), "local");
    write_connections(temp.path(), json!([connection]));
    let output = run(
        temp.path(),
        &["schema", "local", "--database", "main", "--json"],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let schema: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(schema["connection"]["id"], "local");
    assert_eq!(schema["tables"], json!([]));
}

#[test]
fn agent_setup_installs_without_overwriting_the_user_skill() {
    let home = TempDir::new().unwrap();
    let first = bin()
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .args(["agent", "setup", "--json"])
        .output()
        .unwrap();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let first: Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(first["status"], "installed");
    let path = home.path().join(".agents/skills/datazen/SKILL.md");
    assert!(path.is_file());

    std::fs::write(&path, "user-owned").unwrap();
    let second = bin()
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .args(["agent", "setup", "--json"])
        .output()
        .unwrap();
    let second: Value = serde_json::from_slice(&second.stdout).unwrap();
    assert_eq!(second["status"], "alreadyInstalled");
    assert_eq!(std::fs::read_to_string(path).unwrap(), "user-owned");
}
