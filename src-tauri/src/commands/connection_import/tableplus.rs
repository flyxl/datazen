//! TablePlus `.tableplusconnection` (RNCryptor v3 JSON array) import.

use super::super::error::CommandError;
use super::rncryptor;
use super::{ImportFormat, ParsedImport};
use crate::db::{ConnectionConfig, SshTunnelConfig, SslMode};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct TablePlusConnection {
    #[serde(default, alias = "ID", alias = "Id")]
    id: Option<String>,
    #[serde(default)]
    connection_name: Option<String>,
    #[serde(default)]
    driver: Option<String>,
    #[serde(default)]
    database_host: Option<String>,
    #[serde(default)]
    database_port: Option<serde_json::Value>,
    #[serde(default)]
    database_user: Option<String>,
    #[serde(default)]
    database_password: Option<String>,
    #[serde(default)]
    database_name: Option<String>,
    #[serde(default)]
    database_path: Option<String>,
    #[serde(default)]
    #[serde(alias = "isOverSSH")]
    is_over_ssh: Option<serde_json::Value>,
    #[serde(default)]
    server_address: Option<String>,
    #[serde(default)]
    server_port: Option<serde_json::Value>,
    #[serde(default)]
    server_user: Option<String>,
    #[serde(default)]
    server_password: Option<String>,
    #[serde(default)]
    #[serde(alias = "isUsePrivateKey")]
    is_use_private_key: Option<serde_json::Value>,
    #[serde(default)]
    private_key: Option<String>,
    #[serde(default, alias = "PrivateKeyPath")]
    private_key_path: Option<String>,
    /// DataZen round-trip; TablePlus ignores unknown keys.
    #[serde(default)]
    group: Option<String>,
}

fn truthy(v: &Option<serde_json::Value>) -> bool {
    match v {
        Some(serde_json::Value::Bool(b)) => *b,
        Some(serde_json::Value::Number(n)) => n.as_i64().unwrap_or(0) != 0,
        Some(serde_json::Value::String(s)) => {
            let s = s.trim().to_ascii_lowercase();
            s == "1" || s == "true" || s == "yes"
        }
        _ => false,
    }
}

fn as_port(v: &Option<serde_json::Value>) -> Option<u16> {
    match v {
        Some(serde_json::Value::Number(n)) => n.as_u64().and_then(|x| u16::try_from(x).ok()),
        Some(serde_json::Value::String(s)) => s.trim().parse().ok(),
        _ => None,
    }
}

fn map_driver(driver: &str) -> Option<String> {
    let original = driver.trim();
    let d = original.to_ascii_lowercase();
    if d.contains("maria") {
        Some("mariadb".into())
    } else if d.contains("mysql") {
        Some("mysql".into())
    } else if d.contains("postgres") || d.contains("cockroach") || d.contains("redshift") {
        Some("postgresql".into())
    } else if d.contains("sqlite") {
        Some("sqlite".into())
    } else if d.contains("sql server") || d.contains("sqlserver") || d.contains("mssql") {
        Some("sqlserver".into())
    } else if d.contains("mongo") {
        Some("mongodb".into())
    } else if d.contains("redis") {
        Some("redis".into())
    } else if d.contains("clickhouse") {
        Some("clickhouse".into())
    } else if d.contains("duckdb") {
        Some("duckdb".into())
    } else if d.contains("elastic") {
        Some("elasticsearch".into())
    } else if !original.is_empty()
        && original
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        // DataZen-only types we exported as lowercase ids (kiwi, superset, …).
        Some(original.to_string())
    } else {
        None
    }
}

fn export_driver(db_type: &str) -> String {
    match db_type {
        "postgresql" | "questdb" | "cloudberry" => "PostgreSQL".into(),
        "mysql" | "doris" | "starrocks" | "manticore" => "MySQL".into(),
        "mariadb" => "MariaDB".into(),
        "sqlite" => "SQLite".into(),
        "sqlserver" => "SQL Server".into(),
        "mongodb" => "MongoDB".into(),
        "redis" => "Redis".into(),
        "clickhouse" => "ClickHouse".into(),
        "duckdb" => "DuckDB".into(),
        "elasticsearch" => "Elasticsearch".into(),
        other => other.to_string(),
    }
}

fn is_file_type(db_type: &str) -> bool {
    db_type == "sqlite" || db_type == "duckdb"
}

/// RNCryptor v3 `.tableplusconnection` bytes (JSON array payload).
pub fn export_connections(
    connections: &[ConnectionConfig],
    password: &str,
) -> Result<Vec<u8>, CommandError> {
    if password.trim().is_empty() {
        return Err(CommandError::Validation(
            "Password is required for TablePlus export".into(),
        ));
    }
    let items: Vec<serde_json::Value> = connections
        .iter()
        .map(connection_to_tableplus_json)
        .collect();
    let json = serde_json::to_vec(&items)
        .map_err(|e| CommandError::Internal(format!("TablePlus export JSON failed: {e}")))?;
    rncryptor::encrypt_password(&json, password)
}

fn connection_to_tableplus_json(conn: &ConnectionConfig) -> serde_json::Value {
    let file = is_file_type(&conn.database_type);
    let ssh = conn.ssh_tunnel.as_ref().filter(|s| s.enabled);
    let mut obj = serde_json::json!({
        "ID": conn.id,
        "ConnectionName": conn.name,
        "Driver": export_driver(&conn.database_type),
        "DatabaseUser": conn.username.clone().unwrap_or_default(),
        "DatabasePassword": conn.password.clone().unwrap_or_default(),
        "isOverSSH": ssh.is_some(),
    });
    if file {
        obj["DatabasePath"] = serde_json::Value::String(conn.database.clone().unwrap_or_default());
    } else {
        obj["DatabaseHost"] =
            serde_json::Value::String(conn.host.clone().unwrap_or_else(|| "127.0.0.1".into()));
        obj["DatabasePort"] =
            serde_json::Value::String(conn.port.map(|p| p.to_string()).unwrap_or_default());
        obj["DatabaseName"] = serde_json::Value::String(conn.database.clone().unwrap_or_default());
    }
    if let Some(ssh) = ssh {
        obj["ServerAddress"] = serde_json::Value::String(ssh.host.clone());
        obj["ServerPort"] = serde_json::Value::String(ssh.port.to_string());
        obj["ServerUser"] = serde_json::Value::String(ssh.username.clone());
        obj["ServerPassword"] = serde_json::Value::String(ssh.password.clone().unwrap_or_default());
        let use_key = ssh.auth_method == "private_key";
        obj["isUsePrivateKey"] = serde_json::Value::Bool(use_key);
        if let Some(path) = ssh.private_key_path.as_ref() {
            obj["PrivateKeyPath"] = serde_json::Value::String(path.clone());
        }
    }
    if let Some(group) = conn.group.as_ref().filter(|g| !g.is_empty()) {
        obj["Group"] = serde_json::Value::String(group.clone());
    }
    obj
}

fn default_port(db_type: &str) -> Option<u16> {
    match db_type {
        "mysql" | "mariadb" => Some(3306),
        "postgresql" => Some(5432),
        "sqlserver" => Some(1433),
        "mongodb" => Some(27017),
        "redis" => Some(6379),
        "clickhouse" => Some(8123),
        "elasticsearch" => Some(9200),
        _ => None,
    }
}

pub fn parse(bytes: &[u8], password: &str) -> Result<ParsedImport, CommandError> {
    let plain = rncryptor::decrypt_password(bytes, password)?;
    let text = String::from_utf8(plain)
        .map_err(|e| CommandError::Validation(format!("TablePlus payload is not UTF-8: {e}")))?;

    let items: Vec<TablePlusConnection> = serde_json::from_str(&text).map_err(|e| {
        CommandError::Validation(format!("Invalid TablePlus JSON after decrypt: {e}"))
    })?;
    from_items(items)
}

fn plist_unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

struct XmlCursor<'a> {
    s: &'a str,
    i: usize,
}

impl<'a> XmlCursor<'a> {
    fn new(s: &'a str) -> Self {
        Self { s, i: 0 }
    }

    fn skip_misc(&mut self) {
        loop {
            self.skip_ws();
            if self.s[self.i..].starts_with("<?") {
                if let Some(end) = self.s[self.i..].find("?>") {
                    self.i += end + 2;
                    continue;
                }
            }
            if self.s[self.i..].starts_with("<!--") {
                if let Some(end) = self.s[self.i..].find("-->") {
                    self.i += end + 3;
                    continue;
                }
            }
            if self.s[self.i..].starts_with("<!DOCTYPE") {
                if let Some(end) = self.s[self.i..].find('>') {
                    self.i += end + 1;
                    continue;
                }
            }
            break;
        }
    }

    fn skip_ws(&mut self) {
        while let Some(c) = self.s[self.i..].chars().next() {
            if !c.is_whitespace() {
                break;
            }
            self.i += c.len_utf8();
        }
    }

    fn starts_with(&self, tag: &str) -> bool {
        self.s[self.i..].starts_with(tag)
    }

    fn take_until(&mut self, needle: &str) -> Result<&'a str, CommandError> {
        let rest = &self.s[self.i..];
        let Some(pos) = rest.find(needle) else {
            return Err(CommandError::Validation(
                "Invalid TablePlus plist: unterminated tag".into(),
            ));
        };
        let out = &rest[..pos];
        self.i += pos + needle.len();
        Ok(out)
    }

    fn parse_value(&mut self) -> Result<serde_json::Value, CommandError> {
        self.skip_misc();
        if self.starts_with("<true") {
            let _ = self.take_until(">")?;
            return Ok(serde_json::Value::Bool(true));
        }
        if self.starts_with("<false") {
            let _ = self.take_until(">")?;
            return Ok(serde_json::Value::Bool(false));
        }
        if self.starts_with("<string") {
            let open = self.take_until(">")?;
            if open.trim_end().ends_with('/') {
                return Ok(serde_json::Value::String(String::new()));
            }
            let inner = self.take_until("</string>")?;
            return Ok(serde_json::Value::String(plist_unescape(inner)));
        }
        if self.starts_with("<integer") || self.starts_with("<real") {
            let open = self.take_until(">")?.to_string();
            let close = if open.contains("integer") {
                "</integer>"
            } else {
                "</real>"
            };
            let inner = self.take_until(close)?.trim().to_string();
            if let Ok(n) = inner.parse::<i64>() {
                return Ok(serde_json::json!(n));
            }
            if let Ok(n) = inner.parse::<f64>() {
                return Ok(serde_json::json!(n));
            }
            return Ok(serde_json::Value::String(inner));
        }
        if self.starts_with("<data") || self.starts_with("<date") {
            let tag = if self.starts_with("<data") {
                "data"
            } else {
                "date"
            };
            let _ = self.take_until(">")?;
            let _ = self.take_until(&format!("</{tag}>"))?;
            return Ok(serde_json::Value::Null);
        }
        if self.starts_with("<array") {
            let _ = self.take_until(">")?;
            let mut items = Vec::new();
            loop {
                self.skip_misc();
                if self.starts_with("</array>") {
                    let _ = self.take_until(">")?;
                    break;
                }
                if self.i >= self.s.len() {
                    break;
                }
                items.push(self.parse_value()?);
            }
            return Ok(serde_json::Value::Array(items));
        }
        if self.starts_with("<dict") {
            let _ = self.take_until(">")?;
            let mut map = serde_json::Map::new();
            loop {
                self.skip_misc();
                if self.starts_with("</dict>") {
                    let _ = self.take_until(">")?;
                    break;
                }
                if !self.starts_with("<key") {
                    if self.i >= self.s.len() {
                        break;
                    }
                    let _ = self.parse_value()?;
                    continue;
                }
                let _ = self.take_until(">")?;
                let key = plist_unescape(self.take_until("</key>")?);
                let value = self.parse_value()?;
                map.insert(key, value);
            }
            return Ok(serde_json::Value::Object(map));
        }
        if self.starts_with("<plist") {
            let _ = self.take_until(">")?;
            let value = self.parse_value()?;
            self.skip_misc();
            if self.starts_with("</plist>") {
                let _ = self.take_until(">")?;
            }
            return Ok(value);
        }
        Err(CommandError::Validation(
            "Invalid TablePlus plist structure".into(),
        ))
    }
}

fn flatten_plist_connections(value: serde_json::Value) -> Vec<serde_json::Value> {
    match value {
        serde_json::Value::Array(items) => items
            .into_iter()
            .flat_map(flatten_plist_connections)
            .collect(),
        serde_json::Value::Object(map) => {
            if map.contains_key("ConnectionName")
                || map.contains_key("Driver")
                || map.contains_key("DatabaseHost")
            {
                return vec![serde_json::Value::Object(map)];
            }
            map.into_values()
                .flat_map(flatten_plist_connections)
                .collect()
        }
        _ => Vec::new(),
    }
}

fn plist_xml_from_bytes(
    bytes: &[u8],
    path: Option<&std::path::Path>,
) -> Result<String, CommandError> {
    if bytes.starts_with(b"bplist") {
        #[cfg(target_os = "macos")]
        if let Some(path) = path {
            let out = std::process::Command::new("plutil")
                .args(["-convert", "xml1", "-o", "-", "--"])
                .arg(path)
                .output()
                .map_err(|e| CommandError::Internal(format!("plutil: {e}")))?;
            if out.status.success() {
                return String::from_utf8(out.stdout)
                    .map_err(|e| CommandError::Validation(format!("TablePlus plist UTF-8: {e}")));
            }
        }
        return Err(CommandError::Validation(
            "Binary TablePlus Connections.plist is not supported on this platform. Export .tableplusconnection or convert the plist to XML.".into(),
        ));
    }
    String::from_utf8(bytes.to_vec())
        .map_err(|e| CommandError::Validation(format!("TablePlus plist UTF-8: {e}")))
}

/// Live TablePlus `Connections.plist` (XML, or binary on macOS via plutil).
pub fn parse_plist(
    bytes: &[u8],
    path: Option<&std::path::Path>,
) -> Result<ParsedImport, CommandError> {
    let text = plist_xml_from_bytes(bytes, path)?;
    let mut cur = XmlCursor::new(&text);
    cur.skip_misc();
    let root = cur.parse_value()?;
    let values = flatten_plist_connections(root);
    let mut items = Vec::new();
    for value in values {
        match serde_json::from_value::<TablePlusConnection>(value) {
            Ok(item) => items.push(item),
            Err(_) => continue,
        }
    }
    from_items(items)
}

fn from_items(items: Vec<TablePlusConnection>) -> Result<ParsedImport, CommandError> {
    let mut connections = Vec::new();
    let mut skipped = Vec::new();

    for item in items {
        let driver = item.driver.as_deref().unwrap_or("").trim();
        let Some(db_type) = map_driver(driver) else {
            skipped.push(format!("unsupported driver: {driver}"));
            continue;
        };

        let is_file = is_file_type(&db_type);
        let path = item
            .database_path
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        let name = item
            .connection_name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                if is_file {
                    path.clone().unwrap_or_else(|| driver.to_string())
                } else {
                    format!(
                        "{}@{}",
                        item.database_user.as_deref().unwrap_or("user"),
                        item.database_host.as_deref().unwrap_or("localhost")
                    )
                }
            });

        let id = item
            .id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        let ssh_tunnel = if truthy(&item.is_over_ssh) {
            let host = item
                .server_address
                .as_deref()
                .unwrap_or("")
                .trim()
                .to_string();
            if host.is_empty() {
                None
            } else {
                let use_key = truthy(&item.is_use_private_key);
                let key_path = item
                    .private_key_path
                    .clone()
                    .or(item.private_key.clone())
                    .filter(|s| !s.trim().is_empty());
                Some(SshTunnelConfig {
                    enabled: true,
                    host,
                    port: as_port(&item.server_port).unwrap_or(22),
                    username: item
                        .server_user
                        .clone()
                        .unwrap_or_default()
                        .trim()
                        .to_string(),
                    auth_method: if use_key {
                        "private_key".into()
                    } else {
                        "password".into()
                    },
                    password: item.server_password.clone().filter(|s| !s.is_empty()),
                    private_key_path: key_path,
                    passphrase: None,
                    jump: None,
                })
            }
        } else {
            None
        };

        connections.push(ConnectionConfig {
            id,
            name,
            database_type: db_type.clone(),
            host: if is_file {
                None
            } else {
                Some(
                    item.database_host
                        .as_deref()
                        .unwrap_or("127.0.0.1")
                        .trim()
                        .to_string(),
                )
            },
            port: if is_file {
                None
            } else {
                as_port(&item.database_port).or_else(|| default_port(&db_type))
            },
            database: if is_file {
                path
            } else {
                item.database_name.filter(|s| !s.trim().is_empty())
            },
            schema: None,
            username: item.database_user.filter(|s| !s.trim().is_empty()),
            password: item.database_password.filter(|s| !s.is_empty()),
            ssl_mode: SslMode::default(),
            connection_timeout: 30,
            max_pool_size: 10,
            ssh_tunnel,
            color_tag: None,
            group: item.group.filter(|s| !s.trim().is_empty()),
            last_connected_at: None,
            server_version: None,
            options: None,
            read_only: false,
        });
    }

    if connections.is_empty() {
        return Err(CommandError::Validation(
            "No supported TablePlus connections found".into(),
        ));
    }

    Ok(ParsedImport {
        connections,
        groups: Vec::new(),
        format: ImportFormat::TablePlus,
        skipped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::connection_import::rncryptor;

    #[test]
    fn decrypt_and_map() {
        let json = r#"[
          {
            "ConnectionName": "Prod PG",
            "Driver": "PostgreSQL",
            "DatabaseHost": "db.local",
            "DatabasePort": "5432",
            "DatabaseUser": "alice",
            "DatabasePassword": "s3cret",
            "DatabaseName": "app",
            "isOverSSH": 1,
            "ServerAddress": "bastion",
            "ServerPort": "22",
            "ServerUser": "ubuntu",
            "ServerPassword": "sshpass"
          },
          {
            "ConnectionName": "Oracle Skip",
            "Driver": "Oracle",
            "DatabaseHost": "x",
            "DatabasePort": "1521"
          }
        ]"#;
        let enc = rncryptor::encrypt_password(json.as_bytes(), "pass").unwrap();
        let parsed = parse(&enc, "pass").unwrap();
        assert_eq!(parsed.format, ImportFormat::TablePlus);
        assert_eq!(parsed.connections.len(), 1);
        assert_eq!(parsed.skipped.len(), 1);
        let c = &parsed.connections[0];
        assert_eq!(c.name, "Prod PG");
        assert_eq!(c.database_type, "postgresql");
        assert_eq!(c.password.as_deref(), Some("s3cret"));
        let ssh = c.ssh_tunnel.as_ref().unwrap();
        assert!(ssh.enabled);
        assert_eq!(ssh.host, "bastion");
        assert_eq!(ssh.username, "ubuntu");
    }

    #[test]
    fn export_roundtrip_preserves_password_and_group() {
        let conn = ConnectionConfig {
            id: "c1".into(),
            name: "Demo".into(),
            database_type: "postgresql".into(),
            host: Some("localhost".into()),
            port: Some(5432),
            database: Some("app".into()),
            schema: None,
            username: Some("alice".into()),
            password: Some("pw".into()),
            ssl_mode: SslMode::default(),
            connection_timeout: 30,
            max_pool_size: 10,
            ssh_tunnel: None,
            color_tag: None,
            group: Some("Prod".into()),
            last_connected_at: None,
            server_version: None,
            options: None,
            read_only: false,
        };
        let bytes = export_connections(&[conn], "share-secret").unwrap();
        assert_eq!(&bytes[0..2], &[0x03, 0x01]);
        let parsed = parse(&bytes, "share-secret").unwrap();
        assert_eq!(parsed.connections.len(), 1);
        let c = &parsed.connections[0];
        assert_eq!(c.name, "Demo");
        assert_eq!(c.password.as_deref(), Some("pw"));
        assert_eq!(c.group.as_deref(), Some("Prod"));
        assert_eq!(c.host.as_deref(), Some("localhost"));
    }

    #[test]
    fn export_roundtrip_keeps_datazen_only_driver() {
        let conn = ConnectionConfig {
            id: "k1".into(),
            name: "Kiwi".into(),
            database_type: "kiwi".into(),
            host: Some("https://kiwi.example".into()),
            port: Some(443),
            database: None,
            schema: None,
            username: Some("u".into()),
            password: Some("p".into()),
            ssl_mode: SslMode::default(),
            connection_timeout: 30,
            max_pool_size: 10,
            ssh_tunnel: None,
            color_tag: None,
            group: None,
            last_connected_at: None,
            server_version: None,
            options: None,
            read_only: false,
        };
        let bytes = export_connections(&[conn], "pw").unwrap();
        let parsed = parse(&bytes, "pw").unwrap();
        assert_eq!(parsed.connections[0].database_type, "kiwi");
    }

    #[test]
    fn map_driver_skips_oracle() {
        assert!(map_driver("Oracle").is_none());
        assert_eq!(map_driver("kiwi").as_deref(), Some("kiwi"));
    }

    #[test]
    fn parse_xml_plist_array() {
        let xml = r#"<?xml version="1.0"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<array>
  <dict>
    <key>ConnectionName</key>
    <string>Local MySQL</string>
    <key>Driver</key>
    <string>MySQL</string>
    <key>DatabaseHost</key>
    <string>127.0.0.1</string>
    <key>DatabasePort</key>
    <string>3306</string>
    <key>DatabaseUser</key>
    <string>root</string>
    <key>DatabaseName</key>
    <string>app</string>
  </dict>
</array>
</plist>"#;
        let parsed = parse_plist(xml.as_bytes(), None).unwrap();
        assert_eq!(parsed.format, ImportFormat::TablePlus);
        assert_eq!(parsed.connections.len(), 1);
        assert_eq!(parsed.connections[0].name, "Local MySQL");
        assert_eq!(parsed.connections[0].database_type, "mysql");
        assert_eq!(parsed.connections[0].host.as_deref(), Some("127.0.0.1"));
    }
}
