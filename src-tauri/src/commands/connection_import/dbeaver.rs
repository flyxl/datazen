//! DBeaver `data-sources.json` (+ optional sibling `credentials-config.json`) import.

use super::super::error::CommandError;
use super::{ImportFormat, ParsedImport};
use crate::db::{ConnectionConfig, SslMode};
use aes::Aes128;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use cbc::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};
use regex::Regex;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::OnceLock;

type Aes128CbcDec = cbc::Decryptor<Aes128>;

/// Hardcoded DBeaver local credentials key (community editions).
const DBEAVER_KEY: [u8; 16] = [
    186, 187, 74, 159, 119, 74, 184, 83, 201, 108, 45, 101, 61, 254, 84, 74,
];

fn normalize_key(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

fn get_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.trim().to_string(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => String::new(),
    }
}

fn get_number(v: &Value) -> u16 {
    match v {
        Value::Number(n) => n.as_u64().and_then(|x| u16::try_from(x).ok()).unwrap_or(0),
        Value::String(s) => s.trim().parse().unwrap_or(0),
        _ => 0,
    }
}

fn map_driver(provider: &str, driver: &str, url: &str, name: &str) -> Option<&'static str> {
    let blob = normalize_key(&format!("{provider} {driver} {url} {name}"));
    let checks: &[(&[&str], &str)] = &[
        (&["mariadb"], "mariadb"),
        (&["mysql"], "mysql"),
        (&["cloudberry"], "cloudberry"),
        (&["questdb"], "questdb"),
        (
            &["postgresql", "postgres", "opengauss", "gaussdb"],
            "postgresql",
        ),
        (&["sqlite"], "sqlite"),
        (&["sqlserver", "mssql"], "sqlserver"),
        (&["clickhouse"], "clickhouse"),
        (&["duckdb"], "duckdb"),
        (&["mongodb", "mongo"], "mongodb"),
        (&["elasticsearch", "easysearch"], "elasticsearch"),
        (&["doris"], "doris"),
        (&["starrocks"], "starrocks"),
        (&["redis"], "redis"),
        (&["influx"], "influxdb"),
        (&["oboracle", "oceanbaseoracle"], "ob_oracle"),
    ];
    for (needles, db_type) in checks {
        if needles.iter().any(|n| blob.contains(n)) {
            return Some(db_type);
        }
    }
    None
}

fn default_port(db_type: &str) -> Option<u16> {
    match db_type {
        "mysql" | "mariadb" => Some(3306),
        "postgresql" | "cloudberry" => Some(5432),
        "questdb" => Some(8812),
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

fn parse_jdbc_bits(url: &str) -> (String, u16, String) {
    let source = url.trim();
    let without = source
        .strip_prefix("jdbc:")
        .or_else(|| source.strip_prefix("JDBC:"))
        .unwrap_or(source);

    static SQLSERVER_RE: OnceLock<Regex> = OnceLock::new();
    let sqlserver_re = SQLSERVER_RE
        .get_or_init(|| Regex::new(r"(?i)^sqlserver://([^;:/]+)(?::(\d+))?(?:;(.*))?").unwrap());
    if let Some(caps) = sqlserver_re.captures(without) {
        let host = caps
            .get(1)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        let port = caps
            .get(2)
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(0);
        let mut database = String::new();
        if let Some(params) = caps.get(3) {
            for part in params.as_str().split(';') {
                let mut kv = part.splitn(2, '=');
                let key = kv.next().unwrap_or("").to_ascii_lowercase();
                let val = kv.next().unwrap_or("").to_string();
                if key == "databasename" || key == "database" {
                    database = val;
                }
            }
        }
        return (host, port, database);
    }

    static SQLITE_RE: OnceLock<Regex> = OnceLock::new();
    let sqlite_re = SQLITE_RE.get_or_init(|| Regex::new(r"(?i)^sqlite:(.+)$").unwrap());
    if let Some(caps) = sqlite_re.captures(without) {
        let path = caps
            .get(1)
            .map(|m| m.as_str().split('?').next().unwrap_or(""))
            .unwrap_or("")
            .to_string();
        return (path.clone(), 0, path);
    }

    if let Some(scheme_end) = without.find("://") {
        let mut remainder = &without[scheme_end + 3..];
        remainder = remainder.split('?').next().unwrap_or(remainder);
        let (authority, database) = match remainder.find('/') {
            Some(i) => (&remainder[..i], &remainder[i + 1..]),
            None => (remainder, ""),
        };
        let authority = authority.rsplit('@').next().unwrap_or(authority);
        let first = authority.split(',').next().unwrap_or(authority);
        let (host, port) = if let Some(colon) = first.rfind(':') {
            (
                first[..colon].to_string(),
                first[colon + 1..].parse().unwrap_or(0),
            )
        } else {
            (first.to_string(), 0)
        };
        return (host, port, database.to_string());
    }

    (String::new(), 0, String::new())
}

pub fn looks_like_dbeaver_json(text: &str) -> bool {
    let Ok(v) = serde_json::from_str::<Value>(text) else {
        return false;
    };
    if v.get("format").and_then(|x| x.as_str()) == Some("dbeaver-import") {
        return true;
    }
    if let Some(obj) = v.get("connections").and_then(|c| c.as_object()) {
        return obj.values().any(|e| {
            e.get("provider").is_some()
                || e.get("driver").is_some()
                || e.get("configuration").is_some()
        });
    }
    false
}

pub fn looks_like_dbeaver_xml(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("<connections")
        && lower.contains("<connection")
        && !lower.contains("<data-source")
        && (lower.contains(" host=") || lower.contains(" url="))
}

fn decrypt_credentials_bytes(bytes: &[u8]) -> HashMap<String, (String, String)> {
    let mut out = HashMap::new();
    if bytes.len() <= 16 {
        return out;
    }
    let iv = &bytes[..16];
    let mut buf = bytes[16..].to_vec();
    let Ok(cipher) = Aes128CbcDec::new_from_slices(&DBEAVER_KEY, iv) else {
        return out;
    };
    let Ok(plain) = cipher.decrypt_padded_mut::<Pkcs7>(&mut buf) else {
        return out;
    };
    let Ok(json) = serde_json::from_slice::<Value>(plain) else {
        return out;
    };
    // Shape: { "<conn-id>": { "#connection": { "user": "...", "password": "..." } } }
    if let Some(obj) = json.as_object() {
        for (id, entry) in obj {
            let secure = entry.get("#connection").cloned().unwrap_or(Value::Null);
            let user = get_string(
                secure
                    .get("user")
                    .or_else(|| secure.get("username"))
                    .unwrap_or(&Value::Null),
            );
            let password = get_string(secure.get("password").unwrap_or(&Value::Null));
            if !user.is_empty() || !password.is_empty() {
                out.insert(id.clone(), (user, password));
            }
        }
    }
    out
}

fn load_sibling_credentials(path: &Path) -> HashMap<String, (String, String)> {
    let sibling = path.with_file_name("credentials-config.json");
    if !sibling.is_file() {
        return HashMap::new();
    }
    match std::fs::read(&sibling) {
        Ok(bytes) => decrypt_credentials_bytes(&bytes),
        Err(_) => HashMap::new(),
    }
}

fn extract_connections_root(parsed: &Value) -> Option<&Value> {
    parsed
        .get("connections")
        .or_else(|| parsed.get("dataSources"))
        .or_else(|| parsed.get("datasources"))
}

fn build_from_entry(
    id: &str,
    entry: &Value,
    credentials: &HashMap<String, (String, String)>,
) -> Option<(ConnectionConfig, Option<String>)> {
    let name = get_string(entry.get("name").unwrap_or(&Value::Null));
    let folder = {
        let f = get_string(entry.get("folder").unwrap_or(&Value::Null));
        if f.is_empty() {
            None
        } else {
            Some(f)
        }
    };
    let provider = get_string(entry.get("provider").unwrap_or(&Value::Null));
    let driver = get_string(entry.get("driver").unwrap_or(&Value::Null));
    let config = entry.get("configuration").cloned().unwrap_or(Value::Null);
    let url = get_string(config.get("url").unwrap_or(&Value::Null));
    let db_type = map_driver(&provider, &driver, &url, &name)?;
    let (url_host, url_port, url_db) = parse_jdbc_bits(&url);

    let configured_db = {
        let d = get_string(
            config
                .get("database")
                .or_else(|| config.get("database-name"))
                .or_else(|| config.get("schema"))
                .unwrap_or(&Value::Null),
        );
        if d.is_empty() {
            url_db
        } else {
            d
        }
    };

    let is_file = db_type == "sqlite" || db_type == "duckdb";
    let (host, database) = if is_file {
        let path = if !configured_db.is_empty() {
            configured_db
        } else {
            url_host.clone()
        };
        (None, if path.is_empty() { None } else { Some(path) })
    } else {
        let h = get_string(
            config
                .get("host")
                .or_else(|| config.get("host-name"))
                .unwrap_or(&Value::Null),
        );
        let host = Some(if !h.is_empty() {
            h
        } else if !url_host.is_empty() {
            url_host
        } else {
            "127.0.0.1".into()
        });
        let database = if configured_db.is_empty() {
            None
        } else {
            Some(configured_db)
        };
        (host, database)
    };

    let port = if is_file {
        None
    } else {
        let p = get_number(
            config
                .get("port")
                .or_else(|| config.get("host-port"))
                .unwrap_or(&Value::Null),
        );
        if p > 0 {
            Some(p)
        } else if url_port > 0 {
            Some(url_port)
        } else {
            default_port(db_type)
        }
    };

    let display_name = if !name.is_empty() {
        name
    } else if let Some(ref db) = database {
        db.clone()
    } else {
        host.clone().unwrap_or_else(|| db_type.to_string())
    };

    let (cred_user, cred_pass) = credentials.get(id).cloned().unwrap_or_default();
    let inline_user = get_string(
        config
            .get("user")
            .or_else(|| config.get("user-name"))
            .or_else(|| {
                config
                    .get("auth-properties")
                    .and_then(|a| a.get("name").or_else(|| a.get("user")))
            })
            .unwrap_or(&Value::Null),
    );
    let inline_pass = get_string(
        config
            .get("password")
            .or_else(|| {
                config
                    .get("auth-properties")
                    .and_then(|a| a.get("password"))
            })
            .or_else(|| config.get("credentials").and_then(|c| c.get("password")))
            .unwrap_or(&Value::Null),
    );

    let username = {
        let u = if !cred_user.is_empty() {
            cred_user
        } else {
            inline_user
        };
        if u.is_empty() {
            None
        } else {
            Some(u)
        }
    };
    let password = {
        let p = if !cred_pass.is_empty() {
            cred_pass
        } else {
            inline_pass
        };
        if p.is_empty() {
            None
        } else {
            Some(p)
        }
    };

    Some((
        ConnectionConfig {
            id: if id.is_empty() {
                uuid::Uuid::new_v4().to_string()
            } else {
                id.to_string()
            },
            name: display_name,
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
            ssh_tunnel: None,
            color_tag: None,
            group: folder.clone(),
            last_connected_at: None,
            server_version: None,
            options: None,
            read_only: false,
        },
        folder,
    ))
}

fn parse_data_sources_value(
    data_sources: &Value,
    credentials: &HashMap<String, (String, String)>,
) -> Result<ParsedImport, CommandError> {
    let root = extract_connections_root(data_sources).unwrap_or(data_sources);
    let mut connections = Vec::new();
    let mut groups = Vec::new();
    let mut skipped = Vec::new();
    let mut seen = HashSet::new();

    let entries: Vec<(String, Value)> = if let Some(arr) = root.as_array() {
        arr.iter()
            .filter_map(|e| {
                let id = get_string(
                    e.get("id")
                        .or_else(|| e.get("uuid"))
                        .or_else(|| e.get("name"))
                        .unwrap_or(&Value::Null),
                );
                Some((id, e.clone()))
            })
            .collect()
    } else if let Some(obj) = root.as_object() {
        obj.iter()
            .map(|(id, e)| {
                let eid = get_string(e.get("id").unwrap_or(&Value::Null));
                (if eid.is_empty() { id.clone() } else { eid }, e.clone())
            })
            .collect()
    } else {
        return Err(CommandError::Validation(
            "Invalid DBeaver data-sources.json: missing connections".into(),
        ));
    };

    for (id, entry) in entries {
        let Some((conn, folder)) = build_from_entry(&id, &entry, credentials) else {
            skipped.push("unsupported or incomplete connection".into());
            continue;
        };
        let key = format!(
            "{}\0{}\0{}\0{}\0{}",
            conn.name,
            conn.database_type,
            conn.host.as_deref().unwrap_or(""),
            conn.port.unwrap_or(0),
            conn.database.as_deref().unwrap_or("")
        );
        if !seen.insert(key) {
            continue;
        }
        if let Some(f) = folder {
            if !groups.iter().any(|g| g == &f) {
                groups.push(f);
            }
        }
        connections.push(conn);
    }

    if connections.is_empty() {
        return Err(CommandError::Validation(
            "No supported DBeaver connections found".into(),
        ));
    }

    Ok(ParsedImport {
        connections,
        groups,
        format: ImportFormat::DBeaver,
        skipped,
    })
}

pub fn parse_json(path: &Path, text: &str) -> Result<ParsedImport, CommandError> {
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if file_name == "credentials-config.json" {
        return Err(CommandError::Validation(
            "Select DBeaver data-sources.json (credentials-config.json is loaded automatically when present in the same folder)".into(),
        ));
    }

    let v: Value = serde_json::from_str(text).map_err(CommandError::Json)?;

    let (data_sources, mut credentials) =
        if v.get("format").and_then(|x| x.as_str()) == Some("dbeaver-import") {
            let ds_text = v
                .get("dataSources")
                .and_then(|x| x.as_str())
                .ok_or_else(|| {
                    CommandError::Validation(
                        "Invalid dbeaver-import payload: missing dataSources".into(),
                    )
                })?;
            let ds: Value = serde_json::from_str(ds_text).map_err(CommandError::Json)?;
            let mut creds = HashMap::new();
            if let Some(b64) = v.get("credentialsBase64").and_then(|x| x.as_str()) {
                if let Ok(bytes) = BASE64.decode(b64) {
                    creds = decrypt_credentials_bytes(&bytes);
                }
            }
            (ds, creds)
        } else {
            (v, HashMap::new())
        };

    if credentials.is_empty() {
        credentials = load_sibling_credentials(path);
    }

    parse_data_sources_value(&data_sources, &credentials)
}

pub fn parse_xml(xml: &str) -> Result<ParsedImport, CommandError> {
    static CONN_RE: OnceLock<Regex> = OnceLock::new();
    let conn_re = CONN_RE.get_or_init(|| Regex::new(r#"(?is)<connection\b([^>]*?)/?>"#).unwrap());
    static ATTR_RE: OnceLock<Regex> = OnceLock::new();
    let attr_re = ATTR_RE.get_or_init(|| Regex::new(r#"([^\s=]+)="([^"]*)""#).unwrap());

    let mut connections = Vec::new();
    let mut skipped = Vec::new();

    for caps in conn_re.captures_iter(xml) {
        let attrs = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        let mut map = HashMap::new();
        for a in attr_re.captures_iter(attrs) {
            map.insert(
                a.get(1)
                    .map(|m| m.as_str().to_ascii_lowercase())
                    .unwrap_or_default(),
                a.get(2).map(|m| m.as_str().to_string()).unwrap_or_default(),
            );
        }
        let name = map.get("name").cloned().unwrap_or_default();
        let host = map.get("host").cloned().unwrap_or_default();
        let url = map.get("url").cloned().unwrap_or_default();
        let database = map.get("database").cloned().unwrap_or_default();
        let user = map.get("user").cloned().unwrap_or_default();
        let password = map.get("password").cloned().unwrap_or_default();
        let port = map
            .get("port")
            .and_then(|p| p.parse::<u16>().ok())
            .filter(|p| *p > 0);
        let driver_hint = map.get("driver").cloned().unwrap_or_default();

        let Some(db_type) = map_driver(&driver_hint, &driver_hint, &url, &name) else {
            // Infer from JDBC URL / port
            let inferred = if url.to_ascii_lowercase().contains("postgres") || port == Some(5432) {
                Some("postgresql")
            } else if url.to_ascii_lowercase().contains("mysql") || port == Some(3306) {
                Some("mysql")
            } else if url.to_ascii_lowercase().contains("sqlite") {
                Some("sqlite")
            } else {
                None
            };
            let Some(db_type) = inferred else {
                skipped.push("unsupported or incomplete connection".into());
                continue;
            };
            let (url_host, url_port, url_db) = parse_jdbc_bits(&url);
            let is_file = db_type == "sqlite";
            connections.push(ConnectionConfig {
                id: uuid::Uuid::new_v4().to_string(),
                name: if name.is_empty() {
                    "DBeaver".into()
                } else {
                    name
                },
                database_type: db_type.into(),
                host: if is_file {
                    None
                } else if !host.is_empty() {
                    Some(host)
                } else if !url_host.is_empty() {
                    Some(url_host)
                } else {
                    Some("127.0.0.1".into())
                },
                port: if is_file {
                    None
                } else {
                    port.or(if url_port > 0 {
                        Some(url_port)
                    } else {
                        default_port(db_type)
                    })
                },
                database: if is_file {
                    Some(if !database.is_empty() {
                        database
                    } else {
                        url_db
                    })
                } else if !database.is_empty() {
                    Some(database)
                } else if url_db.is_empty() {
                    None
                } else {
                    Some(url_db)
                },
                schema: None,
                username: if user.is_empty() { None } else { Some(user) },
                password: if password.is_empty() {
                    None
                } else {
                    Some(password)
                },
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
            });
            continue;
        };

        let (url_host, url_port, url_db) = parse_jdbc_bits(&url);
        let is_file = db_type == "sqlite" || db_type == "duckdb";
        let (mapped_host, mapped_database) = if is_file {
            let path = if !database.is_empty() {
                database
            } else if !url_db.is_empty() {
                url_db
            } else {
                url_host.clone()
            };
            (None, if path.is_empty() { None } else { Some(path) })
        } else {
            (
                Some(if !host.is_empty() {
                    host
                } else if !url_host.is_empty() {
                    url_host
                } else {
                    "127.0.0.1".into()
                }),
                if !database.is_empty() {
                    Some(database)
                } else if url_db.is_empty() {
                    None
                } else {
                    Some(url_db)
                },
            )
        };
        connections.push(ConnectionConfig {
            id: uuid::Uuid::new_v4().to_string(),
            name: if name.is_empty() {
                "DBeaver".into()
            } else {
                name
            },
            database_type: db_type.into(),
            host: mapped_host,
            port: if is_file {
                None
            } else {
                port.or(if url_port > 0 {
                    Some(url_port)
                } else {
                    default_port(db_type)
                })
            },
            database: mapped_database,
            schema: None,
            username: if user.is_empty() { None } else { Some(user) },
            password: if password.is_empty() {
                None
            } else {
                Some(password)
            },
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
        });
    }

    if connections.is_empty() {
        return Err(CommandError::Validation(
            "No supported DBeaver connections found in XML".into(),
        ));
    }

    Ok(ParsedImport {
        connections,
        groups: Vec::new(),
        format: ImportFormat::DBeaver,
        skipped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cbc::cipher::BlockEncryptMut;
    type Aes128CbcEnc = cbc::Encryptor<Aes128>;

    fn encrypt_credentials_json(json: &str) -> Vec<u8> {
        let iv = [7u8; 16];
        let mut buf = json.as_bytes().to_vec();
        let pad = 16 - (buf.len() % 16);
        buf.extend(std::iter::repeat(0u8).take(pad));
        let cipher = Aes128CbcEnc::new_from_slices(&DBEAVER_KEY, &iv).unwrap();
        let enc = cipher
            .encrypt_padded_mut::<Pkcs7>(&mut buf, json.len())
            .unwrap()
            .to_vec();
        let mut out = iv.to_vec();
        out.extend(enc);
        out
    }

    #[test]
    fn parse_data_sources_json() {
        let json = r#"{
          "folders": {},
          "connections": {
            "pg1": {
              "provider": "postgresql",
              "driver": "postgres-jdbc",
              "name": "PG Prod",
              "folder": "Prod",
              "configuration": {
                "host": "db.example",
                "port": "5432",
                "database": "app",
                "user": "alice"
              }
            },
            "sqlite1": {
              "provider": "sqlite",
              "driver": "sqlite_jdbc",
              "name": "Local",
              "configuration": {
                "url": "jdbc:sqlite:/tmp/app.sqlite",
                "database": "/tmp/app.sqlite"
              }
            },
            "oracle1": {
              "provider": "oracle",
              "driver": "oracle_thin",
              "name": "SkipMe",
              "configuration": { "host": "x", "port": "1521" }
            }
          }
        }"#;
        let parsed = parse_json(Path::new("/tmp/data-sources.json"), json).unwrap();
        assert_eq!(parsed.format, ImportFormat::DBeaver);
        assert_eq!(parsed.connections.len(), 2);
        assert_eq!(parsed.skipped.len(), 1);
        assert_eq!(parsed.groups, vec!["Prod".to_string()]);
        assert_eq!(parsed.connections[0].database_type, "postgresql");
        assert_eq!(parsed.connections[0].username.as_deref(), Some("alice"));
        assert_eq!(parsed.connections[1].database_type, "sqlite");
        assert_eq!(
            parsed.connections[1].database.as_deref(),
            Some("/tmp/app.sqlite")
        );
    }

    #[test]
    fn decrypt_credentials_and_wrapper_payload() {
        let cred_json = r##"{"pg1":{"#connection":{"user":"from-cred","password":"secret"}}}"##;
        let enc = encrypt_credentials_json(cred_json);
        let b64 = BASE64.encode(&enc);
        let ds = r#"{"connections":{"pg1":{"provider":"postgresql","driver":"postgres-jdbc","name":"PG","configuration":{"host":"h","port":5432,"database":"d"}}}}"#;
        let payload = serde_json::json!({
            "format": "dbeaver-import",
            "dataSources": ds,
            "credentialsBase64": b64,
        });
        let parsed = parse_json(
            Path::new("wrapped.json"),
            &serde_json::to_string(&payload).unwrap(),
        )
        .unwrap();
        assert_eq!(parsed.connections[0].username.as_deref(), Some("from-cred"));
        assert_eq!(parsed.connections[0].password.as_deref(), Some("secret"));
    }

    #[test]
    fn parse_custom_xml() {
        let xml = r#"
<connections>
  <connection name="Postgre Import" host="localhost" port="5432" database="postgres" url="jdbc:postgresql://localhost:5432/postgres" user="postgres" password="postgres"/>
</connections>"#;
        let parsed = parse_xml(xml).unwrap();
        assert_eq!(parsed.connections.len(), 1);
        assert_eq!(parsed.connections[0].database_type, "postgresql");
        assert_eq!(parsed.connections[0].password.as_deref(), Some("postgres"));
    }

    #[test]
    fn map_driver_recognizes_common_providers() {
        assert_eq!(
            map_driver("postgresql", "postgres-jdbc", "", ""),
            Some("postgresql")
        );
        assert_eq!(map_driver("mysql", "mysql8", "", ""), Some("mysql"));
        assert_eq!(map_driver("oracle", "thin", "", ""), None);
    }

    #[test]
    fn parse_jdbc_bits_postgres_and_sqlite() {
        let (host, port, db) = parse_jdbc_bits("jdbc:postgresql://db.example:5433/myapp?ssl=true");
        assert_eq!(host, "db.example");
        assert_eq!(port, 5433);
        assert_eq!(db, "myapp");

        let (path, port, db) = parse_jdbc_bits("jdbc:sqlite:/tmp/local.db");
        assert_eq!(path, "/tmp/local.db");
        assert_eq!(port, 0);
        assert_eq!(db, "/tmp/local.db");
    }

    #[test]
    fn default_port_known_types() {
        assert_eq!(default_port("postgresql"), Some(5432));
        assert_eq!(default_port("redis"), Some(6379));
        assert_eq!(default_port("duckdb"), None);
    }
}
