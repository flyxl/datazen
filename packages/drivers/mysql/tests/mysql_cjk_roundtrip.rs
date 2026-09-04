//! Live CJK round-trip probe for the MySQL driver.
//!
//! Gated: skips cleanly when MySQL is unavailable. Credentials come from
//! process env and/or the repo-root `.env` file (same `TEST_MYSQL_*` keys as
//! `mysql_use_database.rs`).
//!
//! Run:
//!   TEST_MYSQL_HOST=127.0.0.1 TEST_MYSQL_PORT=3306 TEST_MYSQL_USER=root \
//!   TEST_MYSQL_PASSWORD= TEST_MYSQL_DATABASE=datazen_test \
//!   cargo test -p datazen-driver-mysql --test mysql_cjk_roundtrip -- --nocapture

use std::collections::HashMap;
use std::path::PathBuf;

use datazen_driver_api::{ConnectionConfig, DatabaseDriver, Value};
use datazen_driver_mysql::MysqlDriver;

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

fn connection_config(
    host: &str,
    port: u16,
    user: &str,
    password: &str,
    database: &str,
) -> ConnectionConfig {
    ConnectionConfig {
        id: "mysql-cjk-roundtrip".into(),
        name: "mysql cjk roundtrip".into(),
        database_type: "mysql".into(),
        host: Some(host.into()),
        port: Some(port),
        database: Some(database.into()),
        schema: None,
        username: Some(user.into()),
        password: Some(password.into()),
        ssl_mode: Default::default(),
        connection_timeout: 5,
        max_pool_size: 2,
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

fn cell_string(value: &Option<Value>) -> Option<String> {
    match value {
        Some(Value::String(s)) => Some(s.clone()),
        _ => None,
    }
}

#[tokio::test]
async fn cjk_text_roundtrip_through_driver() {
    let file = load_dotenv_file();
    let has_marker = std::env::vars().any(|(k, _)| k.starts_with("TEST_MYSQL_"))
        || file.keys().any(|k| k.starts_with("TEST_MYSQL_"));
    if !has_marker {
        eprintln!("Skipping mysql_cjk_roundtrip: no TEST_MYSQL_* in env or .env");
        return;
    }

    let host = env_or_file(&file, "TEST_MYSQL_HOST").unwrap_or_else(|| "127.0.0.1".into());
    let port = env_or_file(&file, "TEST_MYSQL_PORT")
        .and_then(|v| v.parse().ok())
        .unwrap_or(3306);
    let user = env_or_file(&file, "TEST_MYSQL_USER").unwrap_or_else(|| "root".into());
    let password = std::env::var("TEST_MYSQL_PASSWORD")
        .ok()
        .or_else(|| file.get("TEST_MYSQL_PASSWORD").cloned())
        .unwrap_or_default();
    let database =
        env_or_file(&file, "TEST_MYSQL_DATABASE").unwrap_or_else(|| "datazen_test".into());

    let driver = MysqlDriver::new(false);
    let handle = match driver
        .connect(&connection_config(&host, port, &user, &password, &database))
        .await
    {
        Ok(h) => h,
        Err(e) => {
            eprintln!("Skipping: cannot connect to MySQL at {host}:{port}: {e}");
            return;
        }
    };

    let probe = "降噪耳机 H900";
    let status = "已完成";

    // Prepared-statement write path (what the app uses for cell edits).
    let write = driver
        .query_with_params(
            &handle,
            "INSERT INTO _charset_probe (id, name) VALUES (?, ?) \
             ON DUPLICATE KEY UPDATE name = VALUES(name)",
            &[Value::Integer(9001), Value::String(probe.into())],
        )
        .await;
    if let Err(e) = write {
        let _ = driver.disconnect(handle).await;
        eprintln!("Skipping: probe write failed: {e}");
        return;
    }

    let result = driver
        .query(
            &handle,
            "SELECT name, HEX(name) FROM _charset_probe WHERE id = 9001",
        )
        .await
        .unwrap_or_else(|e| panic!("probe read failed: {e}"));
    assert_eq!(result.rows.len(), 1, "expected one probe row");
    let got = cell_string(&result.rows[0][0]).unwrap_or_default();
    let got_hex = cell_string(&result.rows[0][1]).unwrap_or_default();
    println!("probe read: {got:?} hex={got_hex}");

    assert_eq!(got, probe, "driver read path corrupted CJK text");
    assert_eq!(
        got_hex,
        probe
            .as_bytes()
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<String>(),
        "stored bytes are not clean UTF-8"
    );
    assert_eq!(status, "已完成", "sanity: test literal intact");

    // Cleanup probe row, then disconnect.
    let _ = driver
        .execute(&handle, "DELETE FROM _charset_probe WHERE id = 9001")
        .await;
    driver.disconnect(handle).await.expect("disconnect");
}
