use crate::build_connection_url::{ConnectionMode, ConnectionUrlDriverMeta};
use crate::connection_types::SslMode;

use super::{ClipboardDriverMeta, ConnectionClipboardFill};

const POPULARITY_ORDER: [&str; 26] = [
    "mysql",
    "postgresql",
    "mariadb",
    "sqlite",
    "redis",
    "mongodb",
    "sqlserver",
    "clickhouse",
    "doris",
    "starrocks",
    "ob_oracle",
    "elasticsearch",
    "duckdb",
    "questdb",
    "cloudberry",
    "turso",
    "rqlite",
    "influxdb",
    "victoriametrics",
    "hbase",
    "manticore",
    "vector",
    "kiwi",
    "presto",
    "trino",
    "superset",
];

fn popularity_rank(id: &str) -> usize {
    POPULARITY_ORDER
        .iter()
        .position(|value| *value == id)
        .unwrap_or(usize::MAX)
}

pub(crate) fn pick_preferred_type(ids: &[String]) -> Option<String> {
    if ids.len() == 1 {
        return ids.first().cloned();
    }
    ids.iter()
        .min_by(|a, b| {
            popularity_rank(a)
                .cmp(&popularity_rank(b))
                .then_with(|| a.cmp(b))
        })
        .cloned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub(crate) fn decode_uri(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let Some(high) = bytes.get(index + 1).copied().and_then(hex_value) else {
                return value.to_owned();
            };
            let Some(low) = bytes.get(index + 2).copied().and_then(hex_value) else {
                return value.to_owned();
            };
            out.push(high * 16 + low);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| value.to_owned())
}

fn decode_form(value: &str) -> String {
    decode_uri(&value.replace('+', " "))
}

fn query_value(query: &str, key: &str) -> Option<String> {
    for part in query.split('&') {
        if part.is_empty() {
            continue;
        }
        let (raw_key, raw_value) = match part.split_once('=') {
            Some((name, value)) => (name, value),
            None => (part, ""),
        };
        if decode_form(raw_key) == key {
            return Some(decode_form(raw_value));
        }
    }
    None
}

fn strip_prefix_ci<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let len = prefix.len();
    let head = text.get(..len)?;
    if head.eq_ignore_ascii_case(prefix) {
        Some(&text[len..])
    } else {
        None
    }
}

pub(crate) fn strip_env_prefix(text: &str) -> String {
    let after_export = strip_prefix_ci(text, "export").and_then(|rest| {
        if rest.starts_with(char::is_whitespace) {
            Some(rest.trim_start_matches(char::is_whitespace))
        } else {
            None
        }
    });
    let candidate = after_export.unwrap_or(text);
    for key in ["DATABASE_URL", "DB_URL", "REDIS_URL", "URL"] {
        if let Some(rest) = strip_prefix_ci(candidate, key) {
            let rest = rest.trim_start_matches(char::is_whitespace);
            if let Some(value) = rest.strip_prefix('=') {
                let value = value.trim_start_matches(char::is_whitespace);
                if !value.is_empty() {
                    return value.trim().to_owned();
                }
            }
        }
    }
    text.to_owned()
}

pub(crate) fn normalize_clipboard(raw: &str) -> String {
    let no_bom = raw.strip_prefix('\u{feff}').unwrap_or(raw);
    let trimmed = no_bom.trim();
    trimmed
        .trim_matches(|ch| ch == '\'' || ch == '"')
        .trim()
        .to_owned()
}

pub(crate) fn extract_url_scheme(text: &str) -> Option<String> {
    let lower = text.to_ascii_lowercase();
    let candidate = lower.strip_prefix("jdbc:").unwrap_or(&lower);
    let index = candidate.find("://")?;
    let scheme = &candidate[..index];
    if scheme.is_empty() || !scheme.as_bytes()[0].is_ascii_alphabetic() {
        return None;
    }
    if !scheme
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'.' | b'-'))
    {
        return None;
    }
    Some(scheme.to_owned())
}

fn split_scheme(text: &str) -> Option<&str> {
    let lower = text.to_ascii_lowercase();
    let candidate = lower.strip_prefix("jdbc:").unwrap_or(&lower);
    let offset = lower.len() - candidate.len();
    let index = candidate.find("://")?;
    let scheme = &candidate[..index];
    if scheme.is_empty() || !scheme.as_bytes()[0].is_ascii_alphabetic() {
        return None;
    }
    if !scheme
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'.' | b'-'))
    {
        return None;
    }
    Some(&text[offset + index + 3..])
}

pub(crate) fn parse_authority(token: &str, default_port: u16) -> Option<(String, u16)> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(rest) = trimmed.strip_prefix('[') {
        let end = rest.find(']')?;
        if end < 1 {
            return None;
        }
        let host = trimmed[..end + 2].to_owned();
        let tail = &rest[end + 1..];
        if tail.is_empty() {
            return (default_port > 0).then_some((host, default_port));
        }
        let port = tail.strip_prefix(':')?.parse::<u16>().ok()?;
        return (port > 0).then_some((host, port));
    }
    match trimmed.rfind(':') {
        None | Some(0) => (default_port > 0).then_some((trimmed.to_owned(), default_port)),
        Some(index) => {
            let host = &trimmed[..index];
            if host.is_empty() {
                return None;
            }
            let port = trimmed[index + 1..].parse::<u16>().ok()?;
            (port > 0).then_some((host.to_owned(), port))
        }
    }
}

pub(crate) fn parse_bare_host_port(text: &str) -> Option<(String, u16)> {
    if text.contains("://")
        || text
            .chars()
            .any(|ch| ch.is_whitespace() || matches!(ch, ',' | ';'))
    {
        return None;
    }
    parse_authority(text, 0)
}

pub(crate) fn read_ssl_mode(value: Option<&str>) -> Option<SslMode> {
    match value?.to_ascii_lowercase().as_str() {
        "disable" | "false" | "0" => Some(SslMode::Disable),
        "prefer" => Some(SslMode::Prefer),
        "require" | "true" | "1" | "verify-ca" | "verify-full" => Some(SslMode::Require),
        _ => None,
    }
}

pub(crate) fn url_meta(entry: &ClipboardDriverMeta) -> ConnectionUrlDriverMeta {
    ConnectionUrlDriverMeta {
        clipboard_schemes: entry.clipboard_schemes.clone(),
        connection_mode: entry.connection_mode,
        default_port: entry.default_port,
    }
}

pub fn parse_generic_connection_url(
    raw: &str,
    meta: &ConnectionUrlDriverMeta,
) -> Option<ConnectionClipboardFill> {
    let text = strip_env_prefix(&normalize_clipboard(raw));
    let mut rest = split_scheme(&text)?.to_owned();
    let mut query = String::new();
    if let Some(index) = rest.find('?') {
        query = rest[index + 1..].to_owned();
        rest.truncate(index);
    }

    if meta.connection_mode == ConnectionMode::File {
        let path = if rest.starts_with("//") {
            &rest[1..]
        } else {
            &rest
        };
        let file_path = decode_uri(path);
        if file_path.is_empty() {
            return None;
        }
        let name = file_path
            .split(|ch| ch == '\\' || ch == '/')
            .filter(|part| !part.is_empty())
            .next_back()
            .map(str::to_owned);
        return Some(ConnectionClipboardFill {
            database: Some(file_path),
            name,
            ..Default::default()
        });
    }

    let mut path = String::new();
    if let Some(index) = rest.find('/') {
        path = rest[index + 1..].to_owned();
        rest.truncate(index);
    }

    let mut username = String::new();
    let mut password = None;
    if let Some(index) = rest.rfind('@') {
        let auth = rest[..index].to_owned();
        rest = rest[index + 1..].to_owned();
        if let Some(colon) = auth.find(':') {
            username = decode_uri(&auth[..colon]);
            password = Some(decode_uri(&auth[colon + 1..]));
        } else {
            username = decode_uri(&auth);
        }
    }

    let (host, port) = parse_authority(&rest, meta.default_port.unwrap_or(0))?;
    let database = path
        .split('/')
        .next()
        .filter(|part| !part.is_empty())
        .map(decode_uri);
    let ssl_mode = read_ssl_mode(
        query_value(&query, "sslmode")
            .or_else(|| query_value(&query, "ssl"))
            .as_deref(),
    );
    let mut fill = ConnectionClipboardFill {
        host: Some(host.clone()),
        port: (port > 0).then(|| port.to_string()),
        database,
        username: (!username.is_empty()).then_some(username),
        password,
        name: Some(if port > 0 {
            format!("{host}:{port}")
        } else {
            host
        }),
        ..Default::default()
    };
    if let Some(mode) = ssl_mode {
        fill.expand_advanced = mode != SslMode::Prefer;
        fill.ssl_mode = Some(mode);
    }
    Some(fill)
}
