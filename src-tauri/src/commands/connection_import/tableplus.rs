//! TablePlus `.tableplusconnection` (RNCryptor v3 JSON array) import.

use super::rncryptor;
use super::{ImportFormat, ParsedImport};
use super::super::error::CommandError;
use crate::db::{ConnectionConfig, SslMode, SshTunnelConfig};
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

fn map_driver(driver: &str) -> Option<&'static str> {
    let d = driver.to_ascii_lowercase();
    if d.contains("maria") {
        Some("mariadb")
    } else if d.contains("mysql") {
        Some("mysql")
    } else if d.contains("postgres") || d.contains("cockroach") || d.contains("redshift") {
        Some("postgresql")
    } else if d.contains("sqlite") {
        Some("sqlite")
    } else if d.contains("sql server") || d.contains("sqlserver") || d.contains("mssql") {
        Some("sqlserver")
    } else if d.contains("mongo") {
        Some("mongodb")
    } else if d.contains("redis") {
        Some("redis")
    } else if d.contains("clickhouse") {
        Some("clickhouse")
    } else if d.contains("duckdb") {
        Some("duckdb")
    } else if d.contains("elastic") {
        Some("elasticsearch")
    } else {
        None
    }
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
    let text = String::from_utf8(plain).map_err(|e| {
        CommandError::Validation(format!("TablePlus payload is not UTF-8: {e}"))
    })?;

    let items: Vec<TablePlusConnection> = serde_json::from_str(&text).map_err(|e| {
        CommandError::Validation(format!("Invalid TablePlus JSON after decrypt: {e}"))
    })?;

    let mut connections = Vec::new();
    let mut skipped = Vec::new();

    for item in items {
        let driver = item.driver.as_deref().unwrap_or("").trim();
        let Some(db_type) = map_driver(driver) else {
            skipped.push(format!("unsupported driver: {driver}"));
            continue;
        };

        let is_file = db_type == "sqlite" || db_type == "duckdb";
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
                    password: item
                        .server_password
                        .clone()
                        .filter(|s| !s.is_empty()),
                    private_key_path: key_path,
                    passphrase: None,
                })
            }
        } else {
            None
        };

        connections.push(ConnectionConfig {
            id,
            name,
            database_type: db_type.into(),
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
                as_port(&item.database_port).or_else(|| default_port(db_type))
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
            group: None,
            last_connected_at: None,
            server_version: None,
            options: None,
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
}
