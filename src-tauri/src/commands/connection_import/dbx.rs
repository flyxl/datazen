use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use pbkdf2::pbkdf2_hmac;
use sha2::Sha256;

use super::super::error::CommandError;
use super::map::{base_connection, map_database_type};
use super::{ImportFormat, ParsedImport};
use crate::db::SshTunnelConfig;

const PBKDF2_ITERS: u32 = 100_000;

fn decrypt_dbx_envelope(content: &str, passphrase: &str) -> Result<String, CommandError> {
    let envelope: serde_json::Value =
        serde_json::from_str(content).map_err(|e| CommandError::Validation(e.to_string()))?;
    let salt = BASE64
        .decode(envelope.get("salt").and_then(|v| v.as_str()).unwrap_or(""))
        .map_err(|e| CommandError::Internal(format!("DBX salt: {e}")))?;
    let iv = BASE64
        .decode(envelope.get("iv").and_then(|v| v.as_str()).unwrap_or(""))
        .map_err(|e| CommandError::Internal(format!("DBX iv: {e}")))?;
    let data = BASE64
        .decode(envelope.get("data").and_then(|v| v.as_str()).unwrap_or(""))
        .map_err(|e| CommandError::Internal(format!("DBX data: {e}")))?;
    if iv.len() != 12 {
        return Err(CommandError::Validation("Invalid DBX iv length".into()));
    }
    let mut key = [0u8; 32];
    pbkdf2_hmac::<Sha256>(passphrase.as_bytes(), &salt, PBKDF2_ITERS, &mut key);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));
    let plaintext = cipher
        .decrypt(Nonce::from_slice(&iv), data.as_ref())
        .map_err(|_| CommandError::Internal("DBX decryption failed: wrong passphrase".into()))?;
    String::from_utf8(plaintext).map_err(|e| CommandError::Internal(format!("DBX UTF-8: {e}")))
}

fn map_ssh(conn: &serde_json::Value) -> Option<SshTunnelConfig> {
    let layers = conn.get("transport_layers")?.as_array()?;
    for layer in layers {
        if layer.get("type").and_then(|v| v.as_str()) != Some("ssh") {
            continue;
        }
        let enabled = layer
            .get("enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        if !enabled {
            continue;
        }
        let host = layer.get("host")?.as_str()?.to_string();
        let port = layer.get("port").and_then(|v| v.as_u64()).unwrap_or(22) as u16;
        let username = layer
            .get("user")
            .or_else(|| layer.get("username"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let password = layer
            .get("password")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let private_key_path = layer
            .get("key_path")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let passphrase = layer
            .get("key_passphrase")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let auth_method = if private_key_path.is_some() {
            "private_key".to_string()
        } else {
            "password".to_string()
        };
        return Some(SshTunnelConfig {
            enabled: true,
            host,
            port,
            username,
            auth_method,
            password,
            private_key_path,
            passphrase,
            jump: None,
        });
    }
    None
}

fn map_connections(list: &[serde_json::Value]) -> (Vec<crate::db::ConnectionConfig>, Vec<String>) {
    let mut connections = Vec::new();
    let mut skipped = Vec::new();
    for raw in list {
        let name = raw
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("imported")
            .to_string();
        let db_type_raw = raw
            .get("db_type")
            .or_else(|| raw.get("dbType"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let Some(mapped) = map_database_type(db_type_raw) else {
            skipped.push(format!("{name} ({db_type_raw})"));
            continue;
        };
        let host = raw
            .get("host")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let port = raw.get("port").and_then(|v| v.as_u64()).map(|p| p as u16);
        let database = raw
            .get("database")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let username = raw
            .get("username")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let password = raw
            .get("password")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let color = raw
            .get("color")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let id = raw
            .get("id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        connections.push(base_connection(
            id,
            name,
            mapped,
            host,
            port,
            database,
            username,
            password,
            color,
            None,
            map_ssh(raw),
        ));
    }
    (connections, skipped)
}

fn extract_connection_array(
    value: &serde_json::Value,
) -> Result<Vec<serde_json::Value>, CommandError> {
    if let Some(arr) = value.as_array() {
        return Ok(arr.clone());
    }
    if let Some(arr) = value.get("connections").and_then(|v| v.as_array()) {
        return Ok(arr.clone());
    }
    Err(CommandError::Validation(
        "DBX import: missing connections array".into(),
    ))
}

pub fn parse_encrypted(content: &str, passphrase: &str) -> Result<ParsedImport, CommandError> {
    let plain = decrypt_dbx_envelope(content, passphrase)?;
    let value: serde_json::Value =
        serde_json::from_str(&plain).map_err(|e| CommandError::Validation(e.to_string()))?;
    let list = extract_connection_array(&value)?;
    let (connections, skipped) = map_connections(&list);
    let groups = value
        .get("layout")
        .and_then(|l| l.get("groups"))
        .and_then(|g| serde_json::from_value::<Vec<String>>(g.clone()).ok())
        .unwrap_or_default();
    Ok(ParsedImport {
        format: ImportFormat::DbxEncrypted,
        connections,
        groups,
        skipped,
    })
}

pub fn parse_plain(content: &str) -> Result<ParsedImport, CommandError> {
    let value: serde_json::Value =
        serde_json::from_str(content).map_err(|e| CommandError::Validation(e.to_string()))?;
    parse_plain_value(value)
}

pub fn parse_plain_value(value: serde_json::Value) -> Result<ParsedImport, CommandError> {
    let list = extract_connection_array(&value)?;
    let (connections, skipped) = map_connections(&list);
    let groups = value
        .get("layout")
        .and_then(|l| l.get("groups"))
        .and_then(|g| serde_json::from_value::<Vec<String>>(g.clone()).ok())
        .unwrap_or_default();
    Ok(ParsedImport {
        format: ImportFormat::DbxPlain,
        connections,
        groups,
        skipped,
    })
}

fn apply_secret(obj: &mut serde_json::Value, key: &str, secret: &str) {
    if secret.is_empty() {
        return;
    }
    if key == "password" {
        obj["password"] = serde_json::Value::String(secret.to_string());
        return;
    }
    let Some(rest) = key.strip_prefix("transport_layers.") else {
        return;
    };
    let Some((seg, field)) = rest.rsplit_once('.') else {
        return;
    };
    let json_field = match field {
        "ssh_password" => "password",
        "ssh_key_passphrase" => "key_passphrase",
        "proxy_password" => "password",
        "http_tunnel_token" => "token",
        _ => return,
    };
    let Some(layers) = obj
        .get_mut("transport_layers")
        .and_then(|v| v.as_array_mut())
    else {
        return;
    };
    let idx = if let Ok(i) = seg.parse::<usize>() {
        i
    } else {
        match layers
            .iter()
            .position(|l| l.get("id").and_then(|v| v.as_str()) == Some(seg))
        {
            Some(i) => i,
            None => return,
        }
    };
    if let Some(layer) = layers.get_mut(idx) {
        layer[json_field] = serde_json::Value::String(secret.to_string());
    }
}

fn snapshot_sqlite_path(src: &std::path::Path) -> Result<std::path::PathBuf, CommandError> {
    let dir = std::env::temp_dir().join(format!("datazen-dbx-import-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir)
        .map_err(|e| CommandError::Internal(format!("DBX snapshot dir: {e}")))?;
    let dest = dir.join("dbx.db");
    std::fs::copy(src, &dest).map_err(|e| CommandError::Internal(format!("DBX copy: {e}")))?;
    for suffix in ["-wal", "-shm"] {
        let mut sidecar = src.as_os_str().to_os_string();
        sidecar.push(suffix);
        let sidecar = std::path::PathBuf::from(sidecar);
        if sidecar.is_file() {
            let mut dest_side = dest.as_os_str().to_os_string();
            dest_side.push(suffix);
            let _ = std::fs::copy(&sidecar, dest_side);
        }
    }
    Ok(dest)
}

/// Live DBX `dbx.db` (SQLite): `connections.config_json` + `connection_secrets`.
pub fn parse_sqlite(path: &std::path::Path) -> Result<ParsedImport, CommandError> {
    let snapshot = snapshot_sqlite_path(path)?;
    let result = parse_sqlite_file(&snapshot);
    if let Some(parent) = snapshot.parent() {
        let _ = std::fs::remove_dir_all(parent);
    }
    result
}

fn parse_sqlite_file(path: &std::path::Path) -> Result<ParsedImport, CommandError> {
    let conn =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| CommandError::Validation(format!("Cannot open DBX database: {e}")))?;

    let mut secrets: std::collections::HashMap<String, Vec<(String, String)>> =
        std::collections::HashMap::new();
    if let Ok(mut stmt) = conn.prepare("SELECT connection_id, key, secret FROM connection_secrets")
    {
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        });
        if let Ok(rows) = rows {
            for row in rows.flatten() {
                secrets.entry(row.0).or_default().push((row.1, row.2));
            }
        }
    }

    let mut stmt = conn
        .prepare("SELECT id, config_json FROM connections")
        .map_err(|e| CommandError::Validation(format!("DBX connections table: {e}")))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| CommandError::Validation(format!("DBX query: {e}")))?;

    let mut list = Vec::new();
    for row in rows {
        let (id, json) = row.map_err(|e| CommandError::Validation(format!("DBX row: {e}")))?;
        let mut value: serde_json::Value = serde_json::from_str(&json)
            .map_err(|e| CommandError::Validation(format!("DBX config_json: {e}")))?;
        if let Some(obj) = value.as_object_mut() {
            obj.entry("id".to_string())
                .or_insert_with(|| serde_json::Value::String(id.clone()));
        }
        if let Some(pairs) = secrets.get(&id) {
            for (key, secret) in pairs {
                apply_secret(&mut value, key, secret);
            }
        }
        list.push(value);
    }

    parse_plain_value(serde_json::json!({ "connections": list }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_plain_maps_postgres_with_ssh() {
        let json = serde_json::json!({
            "connections": [{
                "id": "pg1",
                "name": "Remote PG",
                "db_type": "postgresql",
                "host": "127.0.0.1",
                "port": 5432,
                "username": "alice",
                "password": "secret",
                "database": "app",
                "transport_layers": [{
                    "type": "ssh",
                    "enabled": true,
                    "host": "bastion.example",
                    "port": 22,
                    "user": "deploy",
                    "password": "jump"
                }]
            }],
            "layout": { "groups": ["Prod"] }
        })
        .to_string();

        let parsed = parse_plain(&json).unwrap();
        assert_eq!(parsed.connections.len(), 1);
        assert_eq!(parsed.groups, vec!["Prod".to_string()]);
        let ssh = parsed.connections[0].ssh_tunnel.as_ref().unwrap();
        assert_eq!(ssh.host, "bastion.example");
        assert_eq!(ssh.port, 22);
        assert_eq!(ssh.username, "deploy");
        assert_eq!(ssh.auth_method, "password");
    }

    #[test]
    fn parse_plain_skips_unknown_db_type() {
        let json = r#"{"connections":[{"name":"x","db_type":"oracle","host":"h"}]}"#;
        let parsed = parse_plain(json).unwrap();
        assert!(parsed.connections.is_empty());
        assert_eq!(parsed.skipped.len(), 1);
    }

    #[test]
    fn extract_connection_array_accepts_root_array() {
        let value = serde_json::json!([{"name": "a", "db_type": "mysql", "host": "h"}]);
        let list = extract_connection_array(&value).unwrap();
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn apply_secret_merges_password_and_ssh() {
        let mut obj = serde_json::json!({
            "transport_layers": [{ "type": "ssh", "id": "hop1", "host": "b" }]
        });
        apply_secret(&mut obj, "password", "secret");
        apply_secret(&mut obj, "transport_layers.hop1.ssh_password", "jump");
        assert_eq!(obj["password"], "secret");
        assert_eq!(obj["transport_layers"][0]["password"], "jump");
    }

    #[test]
    fn parse_sqlite_reads_connections_and_secrets() {
        let dir = std::env::temp_dir().join(format!("datazen-dbx-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("dbx.db");
        {
            let conn = rusqlite::Connection::open(&path).unwrap();
            conn.execute(
                "CREATE TABLE connections (id TEXT PRIMARY KEY, config_json TEXT NOT NULL)",
                [],
            )
            .unwrap();
            conn.execute(
                "CREATE TABLE connection_secrets (
                    connection_id TEXT NOT NULL,
                    key TEXT NOT NULL,
                    secret TEXT NOT NULL,
                    PRIMARY KEY (connection_id, key)
                )",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO connections (id, config_json) VALUES (?1, ?2)",
                rusqlite::params![
                    "c1",
                    r#"{"name":"demo","db_type":"mysql","host":"127.0.0.1","port":3306,"username":"root"}"#
                ],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO connection_secrets (connection_id, key, secret) VALUES (?1, ?2, ?3)",
                rusqlite::params!["c1", "password", "s3cret"],
            )
            .unwrap();
        }
        let parsed = parse_sqlite(&path).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(parsed.connections.len(), 1);
        assert_eq!(parsed.connections[0].name, "demo");
        assert_eq!(parsed.connections[0].password.as_deref(), Some("s3cret"));
    }
}
