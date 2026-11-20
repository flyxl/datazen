use std::collections::HashSet;
use std::fmt;

use serde_json::{Map, Value};

use crate::build_connection_url::ConnectionMode;
use crate::connection_types::SslMode;

mod parsing;
mod tests;

pub use parsing::parse_generic_connection_url;
use parsing::{
    extract_url_scheme, normalize_clipboard, parse_bare_host_port, pick_preferred_type,
    strip_env_prefix, url_meta,
};

#[derive(Clone, Default, PartialEq, Eq)]
pub struct ConnectionClipboardFill {
    pub host: Option<String>,
    pub port: Option<String>,
    pub database: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub schema: Option<String>,
    pub name: Option<String>,
    pub ssl_mode: Option<SslMode>,
    pub options: Option<Value>,
    pub expand_advanced: bool,
}

impl fmt::Debug for ConnectionClipboardFill {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConnectionClipboardFill")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("database", &self.database)
            .field("username", &self.username)
            .field("password", &self.password.as_ref().map(|_| "<redacted>"))
            .field("schema", &self.schema)
            .field("name", &self.name)
            .field("ssl_mode", &self.ssl_mode)
            .field("options", &self.options)
            .field("expand_advanced", &self.expand_advanced)
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatchedConnectionClipboard {
    pub database_type: String,
    pub fill: ConnectionClipboardFill,
}

pub type ConnectionClipboardParser = fn(&str) -> Option<ConnectionClipboardFill>;

#[derive(Clone)]
pub struct ClipboardDriverMeta {
    pub id: String,
    pub clipboard_schemes: Vec<String>,
    pub default_port: Option<u16>,
    pub connection_mode: ConnectionMode,
    pub parser: Option<ConnectionClipboardParser>,
}

pub trait ConnectionFormOps {
    fn database_type(&self) -> &str;
    fn set_database_type(&mut self, value: &str);
    fn set_host(&mut self, value: &str);
    fn set_port(&mut self, value: &str);
    fn set_database(&mut self, value: &str);
    fn set_username(&mut self, value: &str);
    fn set_password(&mut self, value: &str);
    fn set_schema(&mut self, value: &str);
    fn set_ssl_mode(&mut self, value: SslMode);
    fn set_options(&mut self, value: Value);
    fn options(&self) -> Option<&Value>;
    fn set_show_advanced(&mut self, value: bool);
    fn set_name(&mut self, value: &str);
    fn name(&self) -> &str;
}

pub fn match_connection_clipboard(
    raw: &str,
    available_types: Option<&[String]>,
    registry: &[ClipboardDriverMeta],
) -> Option<MatchedConnectionClipboard> {
    let text = strip_env_prefix(&normalize_clipboard(raw));
    if text.is_empty() {
        return None;
    }
    let allowed: HashSet<&str> = match available_types {
        Some(types) if !types.is_empty() => types.iter().map(String::as_str).collect(),
        _ => registry.iter().map(|entry| entry.id.as_str()).collect(),
    };

    for entry in registry {
        if !allowed.contains(entry.id.as_str()) {
            continue;
        }
        if let Some(parser) = entry.parser {
            if let Some(fill) = parser(&text) {
                return Some(MatchedConnectionClipboard {
                    database_type: entry.id.clone(),
                    fill,
                });
            }
        }
    }

    if let Some(scheme) = extract_url_scheme(&text) {
        let matches: Vec<&ClipboardDriverMeta> = registry
            .iter()
            .filter(|entry| allowed.contains(entry.id.as_str()))
            .filter(|entry| {
                entry
                    .clipboard_schemes
                    .iter()
                    .any(|item| item.to_ascii_lowercase() == scheme)
            })
            .collect();
        if let Some(picked) = pick_preferred_type(
            &matches
                .iter()
                .map(|entry| entry.id.clone())
                .collect::<Vec<_>>(),
        ) {
            if let Some(entry) = matches.iter().find(|entry| entry.id == picked) {
                if let Some(fill) = parse_generic_connection_url(&text, &url_meta(entry)) {
                    return Some(MatchedConnectionClipboard {
                        database_type: picked,
                        fill,
                    });
                }
            }
        }
        return None;
    }

    let (host, port) = parse_bare_host_port(&text)?;
    let port_matches: Vec<&ClipboardDriverMeta> = registry
        .iter()
        .filter(|entry| allowed.contains(entry.id.as_str()))
        .filter(|entry| entry.default_port == Some(port))
        .collect();
    if port_matches.len() != 1 {
        return None;
    }
    Some(MatchedConnectionClipboard {
        database_type: port_matches[0].id.clone(),
        fill: ConnectionClipboardFill {
            host: Some(host.clone()),
            port: Some(port.to_string()),
            name: Some(format!("{host}:{port}")),
            ..Default::default()
        },
    })
}

pub fn apply_connection_clipboard_fill<F: ConnectionFormOps>(
    form: &mut F,
    fill: &ConnectionClipboardFill,
) {
    if let Some(host) = &fill.host {
        form.set_host(host);
    }
    if let Some(port) = &fill.port {
        form.set_port(port);
    }
    if let Some(database) = &fill.database {
        form.set_database(database);
    }
    if let Some(username) = &fill.username {
        form.set_username(username);
    }
    if let Some(password) = &fill.password {
        form.set_password(password);
    }
    if let Some(schema) = &fill.schema {
        form.set_schema(schema);
    }
    if let Some(ssl_mode) = fill.ssl_mode {
        form.set_ssl_mode(ssl_mode);
    }
    if let Some(options) = &fill.options {
        let mut merged = form
            .options()
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_else(Map::new);
        if let Some(fill_map) = options.as_object() {
            for (key, value) in fill_map {
                merged.insert(key.clone(), value.clone());
            }
        }
        form.set_options(Value::Object(merged));
    }
    if fill.expand_advanced || matches!(fill.ssl_mode, Some(SslMode::Require | SslMode::Disable)) {
        form.set_show_advanced(true);
    }
    if let Some(name) = &fill.name {
        if form.name().trim().is_empty() {
            form.set_name(name);
        }
    }
}

pub fn apply_matched_clipboard<F: ConnectionFormOps>(
    form: &mut F,
    matched: &MatchedConnectionClipboard,
) {
    if form.database_type() != matched.database_type {
        form.set_database_type(&matched.database_type);
    }
    apply_connection_clipboard_fill(form, &matched.fill);
}
