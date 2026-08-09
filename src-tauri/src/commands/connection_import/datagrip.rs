//! DataGrip `dataSources.xml` import (JDBC URL based).

use super::{ImportFormat, ParsedImport};
use super::super::error::CommandError;
use crate::db::{ConnectionConfig, SslMode};
use regex::Regex;
use std::sync::OnceLock;

struct DriverProfile {
    db_type: &'static str,
    default_port: u16,
    default_user: &'static str,
}

fn profile_for(driver_ref: &str, product: &str, driver_class: &str, subprotocol: &str) -> Option<DriverProfile> {
    let ref_key = driver_ref.split('.').next().unwrap_or("").to_ascii_lowercase();
    if let Some(p) = map_key(&ref_key) {
        return Some(p);
    }

    let product_l = product.to_ascii_lowercase();
    for needle in [
        "mariadb",
        "mysql",
        "postgresql",
        "postgres",
        "sqlite",
        "sql server",
        "sqlserver",
        "mongodb",
        "redis",
        "clickhouse",
        "duckdb",
        "elasticsearch",
        "cockroach",
        "redshift",
    ] {
        if product_l.contains(needle) {
            if let Some(p) = map_key(needle) {
                return Some(p);
            }
        }
    }

    let sub = subprotocol.to_ascii_lowercase();
    if let Some(p) = map_key(&sub) {
        return Some(p);
    }

    let class_l = driver_class.to_ascii_lowercase();
    if class_l.contains("mariadb") {
        return Some(map_key("mariadb").unwrap());
    }
    if class_l.contains("mysql") {
        return Some(map_key("mysql").unwrap());
    }
    if class_l.contains("postgres") {
        return Some(map_key("postgresql").unwrap());
    }
    if class_l.contains("sqlite") {
        return Some(map_key("sqlite").unwrap());
    }
    if class_l.contains("sqlserver") || class_l.contains("mssql") {
        return Some(map_key("sqlserver").unwrap());
    }
    if class_l.contains("mongo") {
        return Some(map_key("mongodb").unwrap());
    }
    if class_l.contains("redis") {
        return Some(map_key("redis").unwrap());
    }
    if class_l.contains("clickhouse") {
        return Some(map_key("clickhouse").unwrap());
    }
    if class_l.contains("duckdb") {
        return Some(map_key("duckdb").unwrap());
    }
    if class_l.contains("elasticsearch") {
        return Some(map_key("elasticsearch").unwrap());
    }

    None
}

fn map_key(key: &str) -> Option<DriverProfile> {
    match key {
        "mysql" => Some(DriverProfile {
            db_type: "mysql",
            default_port: 3306,
            default_user: "root",
        }),
        "mariadb" => Some(DriverProfile {
            db_type: "mariadb",
            default_port: 3306,
            default_user: "root",
        }),
        "postgresql" | "postgres" | "cockroach" | "cockroachdb" | "redshift" => Some(DriverProfile {
            db_type: "postgresql",
            default_port: if key.contains("redshift") {
                5439
            } else if key.contains("cockroach") {
                26257
            } else {
                5432
            },
            default_user: "postgres",
        }),
        "sqlite" => Some(DriverProfile {
            db_type: "sqlite",
            default_port: 0,
            default_user: "",
        }),
        "sqlserver" | "mssql" | "jtds" | "sql server" => Some(DriverProfile {
            db_type: "sqlserver",
            default_port: 1433,
            default_user: "sa",
        }),
        "mongodb" | "mongo" => Some(DriverProfile {
            db_type: "mongodb",
            default_port: 27017,
            default_user: "",
        }),
        "redis" => Some(DriverProfile {
            db_type: "redis",
            default_port: 6379,
            default_user: "",
        }),
        "clickhouse" => Some(DriverProfile {
            db_type: "clickhouse",
            default_port: 8123,
            default_user: "default",
        }),
        "duckdb" => Some(DriverProfile {
            db_type: "duckdb",
            default_port: 0,
            default_user: "",
        }),
        "elasticsearch" | "easysearch" => Some(DriverProfile {
            db_type: "elasticsearch",
            default_port: 9200,
            default_user: "",
        }),
        _ => None,
    }
}

struct JdbcParts {
    host: String,
    port: u16,
    database: String,
}

fn expand_path_macros(value: &str) -> String {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| "~".into());
    value.replace("$USER_HOME$", &home)
}

fn parse_jdbc_url(jdbc_url: &str) -> JdbcParts {
    let url = jdbc_url
        .trim()
        .strip_prefix("jdbc:")
        .or_else(|| jdbc_url.trim().strip_prefix("JDBC:"))
        .unwrap_or(jdbc_url.trim());
    let mut result = JdbcParts {
        host: String::new(),
        port: 0,
        database: String::new(),
    };

    static SQLSERVER_RE: OnceLock<Regex> = OnceLock::new();
    let sqlserver_re = SQLSERVER_RE.get_or_init(|| {
        Regex::new(r"(?i)^sqlserver://([^;:/]+)(?::(\d+))?(?:;(.*))?").unwrap()
    });
    if let Some(caps) = sqlserver_re.captures(url) {
        result.host = caps.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
        result.port = caps
            .get(2)
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(0);
        if let Some(params) = caps.get(3) {
            for part in params.as_str().split(';') {
                let mut kv = part.splitn(2, '=');
                let key = kv.next().unwrap_or("").to_ascii_lowercase();
                let val = kv.next().unwrap_or("").to_string();
                if key == "databasename" || key == "database" {
                    result.database = val;
                }
            }
        }
        return result;
    }

    static FILE_RE: OnceLock<Regex> = OnceLock::new();
    let file_re = FILE_RE.get_or_init(|| Regex::new(r"(?i)^(sqlite|duckdb):(.+)$").unwrap());
    if let Some(caps) = file_re.captures(url) {
        let path = caps.get(2).map(|m| m.as_str()).unwrap_or("");
        let path = expand_path_macros(path.split('?').next().unwrap_or(path));
        result.host = path.clone();
        result.database = path;
        return result;
    }

    let Some(scheme_end) = url.find("://") else {
        return result;
    };
    let mut remainder = &url[scheme_end + 3..];
    remainder = remainder.split('?').next().unwrap_or(remainder);

    let (authority, database) = match remainder.find('/') {
        Some(i) => (&remainder[..i], &remainder[i + 1..]),
        None => (remainder, ""),
    };
    let authority = authority.rsplit('@').next().unwrap_or(authority);
    let first_host = authority.split(',').next().unwrap_or(authority);

    if let Some(rest) = first_host.strip_prefix('[') {
        if let Some(close) = rest.find(']') {
            result.host = rest[..close].to_string();
            if rest[close + 1..].starts_with(':') {
                result.port = rest[close + 2..].parse().unwrap_or(0);
            }
        }
    } else if let Some(colon) = first_host.rfind(':') {
        result.host = first_host[..colon].to_string();
        result.port = first_host[colon + 1..].parse().unwrap_or(0);
    } else {
        result.host = first_host.to_string();
    }
    result.database = database.to_string();
    result
}

fn extract_subprotocol(jdbc_url: &str) -> String {
    let url = jdbc_url.trim();
    let rest = match url.get(..5).map(|s| s.eq_ignore_ascii_case("jdbc:")) {
        Some(true) => &url[5..],
        _ => return String::new(),
    };
    let mut out = String::new();
    for ch in rest.chars() {
        if ch == ':' || ch == '/' {
            break;
        }
        out.push(ch);
    }
    out
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let pat = format!(r#"{name}\s*=\s*"([^"]*)""#);
    let re = Regex::new(&pat).ok()?;
    re.captures(tag)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}

fn child_text(block: &str, tag: &str) -> String {
    let pat = format!(r"(?is)<{tag}[^>]*>(.*?)</{tag}>");
    Regex::new(&pat)
        .ok()
        .and_then(|re| re.captures(block))
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim().to_string())
        .unwrap_or_default()
}

pub fn parse(xml: &str) -> Result<ParsedImport, CommandError> {
    static BLOCK_RE: OnceLock<Regex> = OnceLock::new();
    let block_re = BLOCK_RE.get_or_init(|| {
        Regex::new(r"(?is)<data-source\b([^>]*)>(.*?)</data-source>").unwrap()
    });

    let mut connections = Vec::new();
    let mut groups = Vec::new();
    let mut skipped = Vec::new();

    for caps in block_re.captures_iter(xml) {
        let open_attrs = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        let body = caps.get(2).map(|m| m.as_str()).unwrap_or("");

        let uuid = attr(open_attrs, "uuid").unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let name = attr(open_attrs, "name").unwrap_or_else(|| uuid.clone());
        let group = attr(open_attrs, "group")
            .or_else(|| attr(open_attrs, "group-name"))
            .filter(|g| !g.is_empty());

        let driver_ref = child_text(body, "driver-ref");
        let jdbc_url = child_text(body, "jdbc-url");
        let driver_class = child_text(body, "jdbc-driver");
        let username = child_text(body, "user-name");
        let product = {
            static PROD_RE: OnceLock<Regex> = OnceLock::new();
            let prod_re = PROD_RE.get_or_init(|| {
                Regex::new(r#"(?is)<database-info\b[^>]*\bproduct\s*=\s*"([^"]*)""#).unwrap()
            });
            prod_re
                .captures(body)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string())
                .unwrap_or_default()
        };

        if jdbc_url.is_empty() {
            skipped.push("unsupported or incomplete data-source".into());
            continue;
        }

        let subprotocol = extract_subprotocol(&jdbc_url);
        let Some(profile) = profile_for(&driver_ref, &product, &driver_class, &subprotocol) else {
            skipped.push("unsupported or incomplete data-source".into());
            continue;
        };

        let parsed = parse_jdbc_url(&jdbc_url);
        let is_file = profile.db_type == "sqlite" || profile.db_type == "duckdb";

        let port = if is_file {
            None
        } else if parsed.port > 0 {
            Some(parsed.port)
        } else if profile.default_port > 0 {
            Some(profile.default_port)
        } else {
            None
        };

        let (host, database) = if is_file {
            let path = if !parsed.database.is_empty() {
                parsed.database
            } else {
                expand_path_macros(&parsed.host)
            };
            (None, Some(path))
        } else {
            let host = if parsed.host.is_empty() {
                Some("127.0.0.1".into())
            } else {
                Some(parsed.host)
            };
            let database = if parsed.database.is_empty() {
                None
            } else {
                Some(parsed.database)
            };
            (host, database)
        };

        let username = if username.is_empty() {
            if profile.default_user.is_empty() {
                None
            } else {
                Some(profile.default_user.to_string())
            }
        } else {
            Some(username)
        };

        if let Some(ref g) = group {
            if !groups.iter().any(|x| x == g) {
                groups.push(g.clone());
            }
        }

        connections.push(ConnectionConfig {
            id: uuid,
            name,
            database_type: profile.db_type.into(),
            host,
            port,
            database,
            schema: None,
            username,
            password: None,
            ssl_mode: SslMode::default(),
            connection_timeout: 30,
            ssh_tunnel: None,
            color_tag: None,
            group,
            last_connected_at: None,
            server_version: None,
        });
    }

    if connections.is_empty() && skipped.is_empty() {
        return Err(CommandError::Validation(
            "No DataGrip data-source entries found in XML".into(),
        ));
    }
    if connections.is_empty() {
        return Err(CommandError::Validation(
            "No supported DataGrip connections found (unsupported JDBC drivers were skipped)".into(),
        ));
    }

    Ok(ParsedImport {
        connections,
        groups,
        format: ImportFormat::DataGrip,
        skipped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_postgres_and_sqlite() {
        let xml = r#"
<data-sources>
  <data-source name="PG" uuid="11111111-1111-1111-1111-111111111111" group="Prod">
    <driver-ref>postgresql</driver-ref>
    <jdbc-driver>org.postgresql.Driver</jdbc-driver>
    <jdbc-url>jdbc:postgresql://db.example:5433/appdb</jdbc-url>
    <user-name>alice</user-name>
  </data-source>
  <data-source name="Lite" uuid="22222222-2222-2222-2222-222222222222">
    <driver-ref>sqlite.xerial</driver-ref>
    <jdbc-url>jdbc:sqlite:$USER_HOME$/data/test.db</jdbc-url>
  </data-source>
  <data-source name="Oracle" uuid="33333333-3333-3333-3333-333333333333">
    <driver-ref>oracle</driver-ref>
    <jdbc-url>jdbc:oracle:thin:@host:1521:ORCL</jdbc-url>
  </data-source>
</data-sources>"#;
        let parsed = parse(xml).unwrap();
        assert_eq!(parsed.connections.len(), 2);
        assert_eq!(parsed.skipped.len(), 1);
        assert_eq!(parsed.groups, vec!["Prod".to_string()]);
        let pg = &parsed.connections[0];
        assert_eq!(pg.database_type, "postgresql");
        assert_eq!(pg.host.as_deref(), Some("db.example"));
        assert_eq!(pg.port, Some(5433));
        assert_eq!(pg.database.as_deref(), Some("appdb"));
        assert_eq!(pg.username.as_deref(), Some("alice"));
        let lite = &parsed.connections[1];
        assert_eq!(lite.database_type, "sqlite");
        assert!(lite.database.as_ref().unwrap().contains("test.db"));
    }
}
