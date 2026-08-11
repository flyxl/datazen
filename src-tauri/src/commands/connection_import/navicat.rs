//! Navicat `.ncx` connection export import.

use super::super::error::CommandError;
use super::{ImportFormat, ParsedImport};
use crate::db::{ConnectionConfig, SslMode, SshTunnelConfig};
use aes::Aes128;
use cbc::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};
use regex::Regex;
use std::collections::HashMap;
use std::sync::OnceLock;

type Aes128CbcDec = cbc::Decryptor<Aes128>;

const NAVICAT_KEY: &[u8; 16] = b"libcckeylibcckey";
const NAVICAT_IV: &[u8; 16] = b"libcciv libcciv ";

fn normalize_key(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

fn get_any(values: &HashMap<String, String>, keys: &[&str]) -> String {
    for key in keys {
        if let Some(v) = values.get(&normalize_key(key)) {
            let t = v.trim();
            if !t.is_empty() {
                return t.to_string();
            }
        }
    }
    String::new()
}

fn truthy(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "y" | "on" | "checked"
    )
}

fn hex_to_bytes(hex: &str) -> Option<Vec<u8>> {
    let clean = hex.trim();
    if clean.is_empty() || clean.len() % 2 != 0 || !clean.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    (0..clean.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&clean[i..i + 2], 16).ok())
        .collect()
}

pub fn decrypt_password(hex: &str) -> String {
    let Some(encrypted) = hex_to_bytes(hex) else {
        return String::new();
    };
    if encrypted.is_empty() {
        return String::new();
    }
    let mut buf = encrypted;
    match Aes128CbcDec::new_from_slices(NAVICAT_KEY, NAVICAT_IV)
        .ok()
        .and_then(|c| c.decrypt_padded_mut::<Pkcs7>(&mut buf).ok())
    {
        Some(plain) => String::from_utf8_lossy(plain).into_owned(),
        None => String::new(),
    }
}

#[cfg(test)]
pub fn encrypt_password(plain: &str) -> String {
    use cbc::cipher::BlockEncryptMut;
    type Aes128CbcEnc = cbc::Encryptor<Aes128>;

    let mut buf = plain.as_bytes().to_vec();
    let pad = 16 - (buf.len() % 16);
    buf.extend(std::iter::repeat(0u8).take(pad));
    let cipher = Aes128CbcEnc::new_from_slices(NAVICAT_KEY, NAVICAT_IV).unwrap();
    let out = cipher.encrypt_padded_mut::<Pkcs7>(&mut buf, plain.len()).unwrap();
    out.iter().map(|b| format!("{b:02X}")).collect()
}

fn map_conn_type(raw: &str, tag: &str, port: Option<u16>) -> Option<&'static str> {
    let key = normalize_key(if raw.is_empty() { tag } else { raw });
    let mapped = if key.contains("mariadb") || key == "5" {
        Some("mariadb")
    } else if key.contains("mysql") || key == "1" {
        Some("mysql")
    } else if key.contains("postgres") || key.contains("postgresql") || key == "2" {
        Some("postgresql")
    } else if key.contains("sqlite") || key == "3" {
        Some("sqlite")
    } else if key.contains("sqlserver") || key.contains("mssql") || key == "7" {
        Some("sqlserver")
    } else if key.contains("mongo") || key == "8" {
        Some("mongodb")
    } else if key.contains("redis") || key == "9" {
        Some("redis")
    } else if key.contains("clickhouse") {
        Some("clickhouse")
    } else if key.contains("duckdb") {
        Some("duckdb")
    } else if key.contains("elastic") {
        Some("elasticsearch")
    } else if key.contains("doris") {
        Some("doris")
    } else if key.contains("starrocks") {
        Some("starrocks")
    } else if key.contains("questdb") {
        Some("questdb")
    } else if key.contains("cloudberry") {
        Some("cloudberry")
    } else if key.contains("influx") {
        Some("influxdb")
    } else if key.contains("http") || key.contains("ftp") || key.contains("ssh") {
        None
    } else if key.contains("oracle") {
        None // no generic oracle driver; may remap via ServiceProvider
    } else {
        None
    };

    if mapped.is_some() {
        return mapped;
    }

    match port {
        Some(6379) => Some("redis"),
        Some(27017) => Some("mongodb"),
        Some(5432) => Some("postgresql"),
        Some(3306) => Some("mysql"),
        Some(1433) => Some("sqlserver"),
        Some(8123) => Some("clickhouse"),
        _ => None,
    }
}

fn apply_service_provider(db_type: &str, service_provider: &str, raw_type: &str) -> Option<&'static str> {
    let sp = service_provider.to_ascii_lowercase();
    if sp.contains("oceanbase") {
        if normalize_key(raw_type).contains("oracle") {
            return Some("ob_oracle");
        }
        return Some("mysql");
    }
    if sp.contains("gauss") {
        return Some("postgresql");
    }
    Some(match db_type {
        "mariadb" => "mariadb",
        "mysql" => "mysql",
        "postgresql" => "postgresql",
        "sqlite" => "sqlite",
        "sqlserver" => "sqlserver",
        "mongodb" => "mongodb",
        "redis" => "redis",
        "clickhouse" => "clickhouse",
        "duckdb" => "duckdb",
        "elasticsearch" => "elasticsearch",
        "doris" => "doris",
        "starrocks" => "starrocks",
        "questdb" => "questdb",
        "cloudberry" => "cloudberry",
        "influxdb" => "influxdb",
        "ob_oracle" => "ob_oracle",
        _ => return None,
    })
}

fn default_port(db_type: &str) -> Option<u16> {
    match db_type {
        "mysql" | "mariadb" => Some(3306),
        "postgresql" | "cloudberry" | "questdb" => Some(if db_type == "questdb" { 8812 } else { 5432 }),
        "sqlserver" => Some(1433),
        "mongodb" => Some(27017),
        "redis" => Some(6379),
        "clickhouse" => Some(8123),
        "elasticsearch" => Some(9200),
        "doris" | "starrocks" => Some(9030),
        "influxdb" => Some(8086),
        "ob_oracle" => Some(2883),
        _ => None,
    }
}

fn parse_attrs(source: &str) -> HashMap<String, String> {
    static ATTR_RE: OnceLock<Regex> = OnceLock::new();
    let re = ATTR_RE.get_or_init(|| Regex::new(r#"([^\s=]+)="([^"]*)""#).unwrap());
    let mut values = HashMap::new();
    for caps in re.captures_iter(source) {
        let name = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        let value = caps.get(2).map(|m| m.as_str()).unwrap_or("");
        values.insert(normalize_key(name), value.to_string());
    }
    values
}

fn parse_port(value: &str, fallback: Option<u16>) -> Option<u16> {
    value
        .trim()
        .parse::<u16>()
        .ok()
        .filter(|p| *p > 0)
        .or(fallback)
}

fn sqlite_path(values: &HashMap<String, String>) -> String {
    get_any(
        values,
        &[
            "databaseFile",
            "databaseFileName",
            "databaseFilename",
            "filename",
            "fileName",
            "path",
            "databasePath",
            "dbPath",
            "dbFile",
            "sqliteFile",
            "sqlitePath",
            "database",
            "databaseName",
        ],
    )
}

fn build_ssh(values: &HashMap<String, String>) -> Option<SshTunnelConfig> {
    let enabled = get_any(
        values,
        &[
            "ssh",
            "useSsh",
            "sshEnabled",
            "enableSsh",
            "useSshTunnel",
            "sshTunnelEnabled",
        ],
    );
    if !truthy(&enabled) {
        return None;
    }
    let host = get_any(values, &["sshHost", "sshTunnelHost", "tunnelHost"]);
    let user = get_any(
        values,
        &[
            "sshUserName",
            "sshUsername",
            "sshUser",
            "sshTunnelUserName",
            "sshTunnelUsername",
            "tunnelUserName",
        ],
    );
    if host.is_empty() || user.is_empty() {
        return None;
    }

    let auth_value = normalize_key(&get_any(
        values,
        &[
            "sshAuthenMethod",
            "sshAuthMethod",
            "sshAuthenticationMethod",
            "sshAuthentication",
            "sshAuthType",
        ],
    ));
    let key_path = get_any(
        values,
        &[
            "sshPrivateKey",
            "sshKeyFile",
            "sshKeyPath",
            "sshIdentityFile",
            "sshTunnelPrivateKey",
        ],
    );
    let uses_key = auth_value.contains("key") || (!auth_value.contains("password") && !key_path.is_empty());

    Some(SshTunnelConfig {
        enabled: true,
        host,
        port: parse_port(
            &get_any(values, &["sshPort", "sshTunnelPort", "tunnelPort"]),
            Some(22),
        )
        .unwrap_or(22),
        username: user,
        auth_method: if uses_key {
            "private_key".into()
        } else {
            "password".into()
        },
        password: if uses_key {
            None
        } else {
            let pw = decrypt_password(&get_any(values, &["sshPassword", "sshTunnelPassword"]));
            if pw.is_empty() {
                None
            } else {
                Some(pw)
            }
        },
        private_key_path: if uses_key && !key_path.is_empty() {
            Some(key_path)
        } else {
            None
        },
        passphrase: if uses_key {
            let pp = decrypt_password(&get_any(
                values,
                &["sshPassphrase", "sshKeyPassphrase", "sshPrivateKeyPassphrase"],
            ));
            if pp.is_empty() {
                None
            } else {
                Some(pp)
            }
        } else {
            None
        },
    })
}

fn is_connection_candidate(tag: &str, values: &HashMap<String, String>) -> bool {
    let tag_n = normalize_key(tag);
    if tag_n == "member" || tag_n == "advance" {
        return false;
    }
    let type_v = get_any(
        values,
        &["connType", "databaseType", "driver", "connectionType", "type"],
    );
    let name = get_any(
        values,
        &["name", "connectionName", "connName", "caption", "title"],
    );
    let host = get_any(
        values,
        &["host", "server", "hostname", "serverHost", "address"],
    );
    let file = sqlite_path(values);
    (!name.is_empty() || !host.is_empty() || !file.is_empty())
        && (!type_v.is_empty() || !host.is_empty() || !file.is_empty())
}

pub fn parse(xml: &str) -> Result<ParsedImport, CommandError> {
    static CONN_RE: OnceLock<Regex> = OnceLock::new();
    let conn_re = CONN_RE.get_or_init(|| {
        Regex::new(r#"(?is)<(Connection|connection)\b([^>]*?)(?:/>|>(.*?)</(?:Connection|connection)>)"#)
            .unwrap()
    });

    let mut connections = Vec::new();
    let mut skipped = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for caps in conn_re.captures_iter(xml) {
        let tag = caps.get(1).map(|m| m.as_str()).unwrap_or("Connection");
        let attrs = caps.get(2).map(|m| m.as_str()).unwrap_or("");
        let inner = caps.get(3).map(|m| m.as_str()).unwrap_or("");
        let mut values = parse_attrs(attrs);

        // Flatten simple child attribute bags (Advance / Member ignored for mapping beyond SSH).
        static CHILD_RE: OnceLock<Regex> = OnceLock::new();
        let child_re = CHILD_RE.get_or_init(|| {
            Regex::new(r#"(?is)<(Advance|advance)\b([^>]*?)(?:/>|></(?:Advance|advance)>)"#).unwrap()
        });
        for child in child_re.captures_iter(inner) {
            for (k, v) in parse_attrs(child.get(2).map(|m| m.as_str()).unwrap_or("")) {
                values.entry(k).or_insert(v);
            }
        }

        if !is_connection_candidate(tag, &values) {
            continue;
        }

        let raw_type = get_any(
            &values,
            &[
                "connType",
                "databaseType",
                "driver",
                "dbType",
                "connectionType",
                "type",
            ],
        );
        // Prefer ConnType over Redis deployment Type when both exist.
        let conn_type = get_any(&values, &["connType"]);
        let type_for_map = if !conn_type.is_empty() {
            conn_type.clone()
        } else {
            raw_type.clone()
        };

        let port_raw = get_any(&values, &["port", "serverPort"]);
        let port_hint = port_raw.trim().parse::<u16>().ok().filter(|p| *p > 0);
        let service_provider = get_any(&values, &["serviceProvider"]);
        let mapped = map_conn_type(&type_for_map, tag, port_hint)
            .and_then(|db| apply_service_provider(db, &service_provider, &type_for_map))
            .or_else(|| {
                // OceanBase Oracle reports ConnType=ORACLE (no generic oracle driver).
                let sp = service_provider.to_ascii_lowercase();
                if sp.contains("oceanbase") {
                    if normalize_key(&type_for_map).contains("oracle") {
                        Some("ob_oracle")
                    } else {
                        Some("mysql")
                    }
                } else {
                    None
                }
            });
        let Some(db_type) = mapped else {
            skipped.push("unsupported or incomplete connection".into());
            continue;
        };

        let is_file = db_type == "sqlite" || db_type == "duckdb";
        let file = if is_file {
            sqlite_path(&values)
        } else {
            String::new()
        };
        let name = {
            let n = get_any(
                &values,
                &["name", "connectionName", "connName", "caption", "title"],
            );
            if !n.is_empty() {
                n
            } else if !file.is_empty() {
                file.clone()
            } else {
                get_any(&values, &["host", "server", "hostname"])
            }
        };
        if name.is_empty() {
            skipped.push("unsupported or incomplete connection".into());
            continue;
        }

        let host = if is_file {
            None
        } else {
            let h = get_any(
                &values,
                &["host", "server", "hostname", "serverHost", "address"],
            );
            Some(if h.is_empty() {
                "127.0.0.1".into()
            } else {
                h
            })
        };

        let port = if is_file {
            None
        } else {
            parse_port(&port_raw, default_port(db_type))
        };

        let database = if is_file {
            if file.is_empty() {
                None
            } else {
                Some(file)
            }
        } else {
            let db = get_any(
                &values,
                &[
                    "database",
                    "databaseName",
                    "initialDatabase",
                    "serviceName",
                    "sid",
                    "schema",
                ],
            );
            if db.is_empty() {
                None
            } else {
                Some(db)
            }
        };

        let username = {
            let u = get_any(&values, &["user", "username", "userName", "uid"]);
            if u.is_empty() {
                None
            } else {
                Some(u)
            }
        };

        let password = {
            let pw = decrypt_password(&get_any(&values, &["password"]));
            if pw.is_empty() {
                None
            } else {
                Some(pw)
            }
        };

        let ssh_tunnel = build_ssh(&values);
        let dedupe = format!(
            "{name}\0{db_type}\0{}\0{}\0{}",
            host.as_deref().unwrap_or(""),
            port.unwrap_or(0),
            database.as_deref().unwrap_or("")
        );
        if !seen.insert(dedupe) {
            continue;
        }

        connections.push(ConnectionConfig {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            database_type: db_type.into(),
            host,
            port,
            database,
            schema: None,
            username,
            password,
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
            "No supported Navicat connections found in .ncx".into(),
        ));
    }

    Ok(ParsedImport {
        connections,
        groups: Vec::new(),
        format: ImportFormat::Navicat,
        skipped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decrypt_roundtrip() {
        let enc = encrypt_password("database-secret");
        assert_eq!(decrypt_password(&enc), "database-secret");
    }

    #[test]
    fn parses_postgres_and_sqlite_and_ssh() {
        let db_pw = encrypt_password("db-secret");
        let ssh_pw = encrypt_password("ssh-secret");
        let xml = format!(
            r#"<?xml version="1.0"?>
<Connections>
  <Connection ConnectionName="PG" ConnType="POSTGRESQL" Host="db.example.test" Port="15432" UserName="alice" Password="{db_pw}" Database="appdb" SSH="true" SSH_Host="bastion.example.test" SSH_Port="2202" SSH_UserName="sshuser" SSH_Password="{ssh_pw}" />
  <Connection ConnectionName="Lite" ConnType="SQLITE" DatabaseFile="C:\Users\demo.db" />
  <Connection ConnectionName="Oracle Skip" ConnType="ORACLE" Host="x" Port="1521" />
</Connections>"#
        );
        let parsed = parse(&xml).unwrap();
        assert_eq!(parsed.connections.len(), 2);
        assert_eq!(parsed.skipped.len(), 1);
        let pg = &parsed.connections[0];
        assert_eq!(pg.database_type, "postgresql");
        assert_eq!(pg.host.as_deref(), Some("db.example.test"));
        assert_eq!(pg.port, Some(15432));
        assert_eq!(pg.password.as_deref(), Some("db-secret"));
        let ssh = pg.ssh_tunnel.as_ref().unwrap();
        assert_eq!(ssh.host, "bastion.example.test");
        assert_eq!(ssh.port, 2202);
        assert_eq!(ssh.password.as_deref(), Some("ssh-secret"));
        let lite = &parsed.connections[1];
        assert_eq!(lite.database_type, "sqlite");
        assert_eq!(lite.database.as_deref(), Some(r"C:\Users\demo.db"));
    }

    #[test]
    fn prefers_conntype_over_redis_type() {
        let xml = r#"
<Connections>
  <Connection ConnectionName="redis-standalone" ConnType="REDIS" Type="Standalone" Host="redis.example.test" Port="16379" UserName="default" />
</Connections>"#;
        let parsed = parse(xml).unwrap();
        assert_eq!(parsed.connections[0].database_type, "redis");
        assert_eq!(parsed.connections[0].port, Some(16379));
    }
}
