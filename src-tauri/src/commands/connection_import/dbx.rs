use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use pbkdf2::pbkdf2_hmac;
use sha2::Sha256;

use super::map::{base_connection, map_database_type};
use super::{ImportFormat, ParsedImport};
use super::super::error::CommandError;
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
    String::from_utf8(plaintext)
        .map_err(|e| CommandError::Internal(format!("DBX UTF-8: {e}")))
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

fn extract_connection_array(value: &serde_json::Value) -> Result<Vec<serde_json::Value>, CommandError> {
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
}
