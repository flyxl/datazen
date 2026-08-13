//! Multi-format connection import:
//! DataZen / DBX / DBeaver / Navicat / DataGrip / TablePlus.

mod app_source;
mod datagrip;
mod datazen;
mod dbeaver;
mod dbx;
mod map;
mod navicat;
mod rncryptor;
mod tableplus;

pub use app_source::{detect_import_path, resolve_import_files, ImportApp, PathContext};

use super::error::CommandError;
use crate::db::ConnectionConfig;
use std::path::{Path, PathBuf};

#[cfg(test)]
pub use datazen::{decrypt_datazen_fields, derive_argon2_key, encrypt_field};
pub use tableplus::export_connections as build_tableplus_export;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportFormat {
    DataZen,
    DbxEncrypted,
    DbxPlain,
    DBeaver,
    Navicat,
    DataGrip,
    TablePlus,
}

#[derive(Debug, Clone)]
pub struct ParsedImport {
    pub format: ImportFormat,
    pub connections: Vec<ConnectionConfig>,
    pub groups: Vec<String>,
    pub skipped: Vec<String>,
}

pub fn format_label(format: ImportFormat) -> &'static str {
    match format {
        ImportFormat::DataZen => "DataZen",
        ImportFormat::DbxEncrypted | ImportFormat::DbxPlain => "DBX",
        ImportFormat::DBeaver => "DBeaver",
        ImportFormat::Navicat => "Navicat",
        ImportFormat::DataGrip => "DataGrip",
        ImportFormat::TablePlus => "TablePlus",
    }
}

fn looks_like_rncryptor_v3(bytes: &[u8]) -> bool {
    bytes.len() >= 66 && bytes[0] == 0x03 && bytes[1] == 0x01
}

fn is_datazen_connection_ext(ext: &str) -> bool {
    ext == "datazenconnection" || ext == "datazenconnections"
}

fn is_encrypted_connection_ext(ext: &str) -> bool {
    is_datazen_connection_ext(ext) || ext == "tableplusconnection"
}

fn looks_like_datazen_json(value: &serde_json::Value) -> bool {
    if value.get("app").and_then(|v| v.as_str()) == Some("DataZen") {
        return true;
    }
    value.get("encrypted").and_then(|v| v.as_bool()) == Some(true)
        && value.get("connections").is_some()
        && value.get("salt").is_some()
}

fn looks_like_dbx_plain(value: &serde_json::Value) -> bool {
    if value.get("format").and_then(|v| v.as_str()) == Some("dbx-config") {
        return true;
    }
    let Some(arr) = value
        .get("connections")
        .and_then(|v| v.as_array())
        .or_else(|| value.as_array())
    else {
        return false;
    };
    arr.iter().any(|c| {
        c.get("db_type").is_some()
            || c.get("dbType").is_some()
            || c.get("transport_layers").is_some()
    })
}

/// Text-only entry (legacy / webdriver preview). Binary TablePlus needs [`parse_import_file`].
#[cfg(test)]
pub fn parse_connections_import(
    content: &str,
    password: Option<&str>,
) -> Result<ParsedImport, CommandError> {
    parse_import_file(Path::new("import.json"), content.as_bytes(), password)
}

/// Detect format from path + bytes, then parse.
pub fn parse_import_file(
    path: &Path,
    bytes: &[u8],
    password: Option<&str>,
) -> Result<ParsedImport, CommandError> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let pw = password.unwrap_or("").trim();

    if is_encrypted_connection_ext(&ext) || looks_like_rncryptor_v3(bytes) {
        if pw.is_empty() {
            return Err(CommandError::Validation(
                "Password is required for encrypted connection import".into(),
            ));
        }
        let mut parsed = tableplus::parse(bytes, pw)?;
        // Same RNCryptor payload as TablePlus; label by our export extension.
        if is_datazen_connection_ext(&ext) {
            parsed.format = ImportFormat::DataZen;
        }
        return Ok(parsed);
    }

    if bytes.starts_with(b"SQLite format 3") || file_name == "dbx.db" || ext == "db" {
        return dbx::parse_sqlite(path);
    }

    if ext == "plist"
        || file_name == "connections.plist"
        || bytes.starts_with(b"bplist")
        || (std::str::from_utf8(bytes).is_ok_and(|t| t.contains("<plist")))
    {
        return tableplus::parse_plist(bytes, Some(path));
    }

    let text = std::str::from_utf8(bytes).map_err(|_| {
        CommandError::Validation(
            "Import file is not valid UTF-8 text (for DataZen use .datazenconnection)".into(),
        )
    })?;

    if ext == "ncx"
        || text.contains("ConnType=")
        || (text.contains("<Connection") && text.contains("ConnectionName="))
    {
        return navicat::parse(text);
    }

    if text.contains("<data-source")
        || text.contains("jdbc-url")
        || (file_name.contains("datasources") && ext == "xml")
    {
        if text.contains("<data-source") || text.contains("jdbc-url") {
            return datagrip::parse(text);
        }
    }

    if dbeaver::looks_like_dbeaver_xml(text) {
        return dbeaver::parse_xml(text);
    }

    // JSON formats
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(text) {
        if value.get("format").and_then(|v| v.as_str()) == Some("dbx-encrypted") {
            if pw.is_empty() {
                return Err(CommandError::Validation(
                    "Password is required for DBX encrypted import".into(),
                ));
            }
            return dbx::parse_encrypted(text, pw);
        }
        if value.get("format").and_then(|v| v.as_str()) == Some("dbx-config") {
            return dbx::parse_plain(text);
        }
        if looks_like_datazen_json(&value)
            || (value.get("version").is_some()
                && value.get("salt").is_some()
                && value.get("connections").is_some())
        {
            if pw.is_empty() {
                return Err(CommandError::Validation(
                    "Password is required for DataZen encrypted import".into(),
                ));
            }
            let (connections, groups) = datazen::parse(text, pw)?;
            return Ok(ParsedImport {
                format: ImportFormat::DataZen,
                connections,
                groups,
                skipped: vec![],
            });
        }
        if file_name.contains("data-sources")
            || file_name == "credentials-config.json"
            || dbeaver::looks_like_dbeaver_json(text)
        {
            return dbeaver::parse_json(path, text);
        }
        if looks_like_dbx_plain(&value) {
            return dbx::parse_plain(text);
        }
        // DataZen plain (encrypted:false) still requires password in legacy API.
        if value.get("connections").is_some() && value.get("app").is_none() {
            // Could be DataZen without app field
            if value.get("encrypted").is_some() || value.get("groups").is_some() {
                if pw.is_empty() {
                    return Err(CommandError::Validation(
                        "Password is required for DataZen import".into(),
                    ));
                }
                let (connections, groups) = datazen::parse(text, pw)?;
                return Ok(ParsedImport {
                    format: ImportFormat::DataZen,
                    connections,
                    groups,
                    skipped: vec![],
                });
            }
        }
    }

    // Fallbacks by extension / heuristics
    if ext == "xml" && text.contains("<Connections") {
        return navicat::parse(text);
    }

    Err(CommandError::Validation(
        "Unrecognized connection import format".into(),
    ))
}

pub fn parse_import_files(
    paths: &[PathBuf],
    password: Option<&str>,
) -> Result<ParsedImport, CommandError> {
    if paths.is_empty() {
        return Err(CommandError::Validation(
            "No connection files to import".into(),
        ));
    }
    let mut connections = Vec::new();
    let mut groups = Vec::new();
    let mut skipped = Vec::new();
    let mut format = None;
    for path in paths {
        let bytes = std::fs::read(path).map_err(|e| {
            CommandError::Validation(format!("Failed to read {}: {e}", path.display()))
        })?;
        let parsed = parse_import_file(path, &bytes, password)?;
        format = Some(parsed.format);
        connections.extend(parsed.connections);
        groups.extend(parsed.groups);
        skipped.extend(parsed.skipped);
    }
    groups.sort();
    groups.dedup();
    Ok(ParsedImport {
        format: format.unwrap_or(ImportFormat::DataZen),
        connections,
        groups,
        skipped,
    })
}

pub fn parse_from_app(
    app: ImportApp,
    custom_path: Option<&Path>,
    password: Option<&str>,
    ctx: &PathContext,
) -> Result<ParsedImport, CommandError> {
    let files = resolve_import_files(app, custom_path, ctx)?;
    parse_import_files(&files, password)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_datagrip() {
        let xml = r#"<?xml version="1.0"?>
<data-sources>
  <data-source name="local" uuid="aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee">
    <driver-ref>postgresql</driver-ref>
    <jdbc-url>jdbc:postgresql://localhost:5432/demo</jdbc-url>
    <user-name>postgres</user-name>
  </data-source>
</data-sources>"#;
        let parsed = parse_import_file(Path::new("dataSources.xml"), xml.as_bytes(), None).unwrap();
        assert_eq!(parsed.format, ImportFormat::DataGrip);
        assert_eq!(parsed.connections[0].database_type, "postgresql");
    }

    #[test]
    fn detect_navicat() {
        let xml = r#"<?xml version="1.0"?>
<Connections>
  <Connection ConnectionName="PG" ConnType="POSTGRESQL" Host="h" Port="5432" UserName="u" Database="d" />
</Connections>"#;
        let parsed = parse_import_file(Path::new("c.ncx"), xml.as_bytes(), None).unwrap();
        assert_eq!(parsed.format, ImportFormat::Navicat);
    }

    #[test]
    fn detect_dbeaver() {
        let json = r#"{"connections":{"pg1":{"provider":"postgresql","driver":"postgres-jdbc","name":"PG","configuration":{"host":"h","port":5432,"database":"d"}}}}"#;
        let parsed =
            parse_import_file(Path::new("data-sources.json"), json.as_bytes(), None).unwrap();
        assert_eq!(parsed.format, ImportFormat::DBeaver);
    }

    #[test]
    fn detect_dbx_encrypted_label() {
        let json = r#"{"format":"dbx-encrypted","version":1,"salt":"a","iv":"b","data":"c"}"#;
        let err = parse_connections_import(json, None).unwrap_err();
        assert!(err.to_string().contains("Password"));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(json)
                .ok()
                .and_then(|v| v
                    .get("format")
                    .and_then(|x| x.as_str().map(|s| s.to_string())))
                .as_deref(),
            Some("dbx-encrypted")
        );
    }

    #[test]
    fn dbx_plain_maps_mysql() {
        let json = serde_json::json!({
            "connections": [{
                "id": "c1",
                "name": "demo",
                "db_type": "mysql",
                "host": "127.0.0.1",
                "port": 3306,
                "username": "root",
                "password": "secret",
                "database": "app",
                "color": "#ff0000"
            }]
        })
        .to_string();
        let parsed = parse_connections_import(&json, None).unwrap();
        assert_eq!(parsed.format, ImportFormat::DbxPlain);
        assert_eq!(parsed.connections[0].database_type, "mysql");
        assert_eq!(parsed.connections[0].color_tag.as_deref(), Some("#ff0000"));
    }

    #[test]
    fn dbx_skips_unknown_types() {
        let json = r#"{"connections":[{"id":"1","name":"x","db_type":"oracle","host":"h","port":1521,"username":"u","password":""}]}"#;
        let parsed = parse_connections_import(json, None).unwrap();
        assert!(parsed.connections.is_empty());
        assert_eq!(parsed.skipped.len(), 1);
    }

    #[test]
    fn detect_sqlite_header_as_dbx() {
        let dir =
            std::env::temp_dir().join(format!("datazen-import-sqlite-{}", uuid::Uuid::new_v4()));
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
                "INSERT INTO connections (id, config_json) VALUES (?1, ?2)",
                rusqlite::params![
                    "c1",
                    r#"{"name":"pg","db_type":"postgresql","host":"h","port":5432}"#
                ],
            )
            .unwrap();
        }
        let bytes = std::fs::read(&path).unwrap();
        let parsed = parse_import_file(&path, &bytes, None).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(parsed.format, ImportFormat::DbxPlain);
        assert_eq!(parsed.connections[0].database_type, "postgresql");
    }

    #[test]
    fn parse_from_app_uses_custom_ncx() {
        let dir = tempfile::tempdir().unwrap();
        let ncx = dir.path().join("export.ncx");
        std::fs::write(
            &ncx,
            r#"<?xml version="1.0"?>
<Connections>
  <Connection ConnectionName="PG" ConnType="POSTGRESQL" Host="h" Port="5432" UserName="u" Database="d" />
</Connections>"#,
        )
        .unwrap();
        let ctx = PathContext {
            home: dir.path().to_path_buf(),
            data: dir.path().join("data"),
            data_local: dir.path().join("local"),
            config: dir.path().join("config"),
        };
        let parsed = parse_from_app(ImportApp::Navicat, Some(&ncx), None, &ctx).unwrap();
        assert_eq!(parsed.format, ImportFormat::Navicat);
        assert_eq!(parsed.connections[0].name, "PG");
    }
}
