//! React `lib/buildConnectionUrl.ts` — the clipboard URL a connection's
//! context menu can copy, ported verbatim for T578. Removal stage: shard 2's
//! T340 ports the file formally; this moves beside it.
//!
//! The driver meta the React function reads off `DB_REGISTRY` arrives as a
//! snapshot (`clipboard_schemes`, `connection_mode`, `default_port`), the
//! same caller-supplies-the-registry contract the flat-row builder chose.

/// The read set `buildConnectionUrl` has off a connection config.
#[derive(Clone, Debug, Default)]
pub struct ConnectionUrlConfig<'a> {
    pub database_type: &'a str,
    pub host: Option<&'a str>,
    pub port: Option<u16>,
    pub username: Option<&'a str>,
    pub password: Option<&'a str>,
    pub database: Option<&'a str>,
    pub ssl_mode: Option<&'a str>,
}

/// The read set off `DB_REGISTRY[type]`.
#[derive(Clone, Debug, Default)]
pub struct ConnectionUrlMeta {
    /// `clipboardSchemes?.[0]` — empty means "no URL to copy".
    pub scheme: String,
    /// `'file'` drivers URL-ify as `scheme://path`.
    pub file_mode: bool,
    pub default_port: u16,
}

/// React `encodeURIComponent` for the two places the URL uses it: userinfo
/// and the path segment. The JS function escapes everything outside
/// `A-Za-z0-9-_.!~*'()`; that is the set the copied URLs keep.
fn encode_uri_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        let keep = byte.is_ascii_alphanumeric()
            || matches!(byte, b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')');
        if keep {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// React `buildConnectionUrl(config)` — `None` is React's `null`: no
/// registry entry, no clipboard scheme, or a file driver with no path.
pub fn build_connection_url(
    config: &ConnectionUrlConfig<'_>,
    meta: Option<&ConnectionUrlMeta>,
) -> Option<String> {
    let meta = meta?;
    if meta.scheme.is_empty() {
        return None;
    }

    if meta.file_mode {
        let path = config.database.unwrap_or("");
        if path.is_empty() {
            return None;
        }
        return Some(format!("{}://{path}", meta.scheme));
    }

    let host = config.host.unwrap_or("localhost");
    let port = config.port.unwrap_or(meta.default_port);
    let host_port = if port != 0 && port != meta.default_port {
        format!("{host}:{port}")
    } else {
        host.to_owned()
    };

    let mut user_info = String::new();
    match (config.username, config.password) {
        (Some(user), pass) if !user.is_empty() => {
            user_info = encode_uri_component(user);
            if let Some(pass) = pass.filter(|p| !p.is_empty()) {
                user_info.push(':');
                user_info.push_str(&encode_uri_component(pass));
            }
            user_info.push('@');
        }
        (None, Some(pass)) if !pass.is_empty() => {
            user_info = format!(":{}@", encode_uri_component(pass));
        }
        _ => {}
    }

    let mut path = String::new();
    if let Some(database) = config.database.filter(|d| !d.is_empty()) {
        path = format!("/{}", encode_uri_component(database));
    }

    let mut query = String::new();
    if let Some(ssl) = config.ssl_mode.filter(|s| !s.is_empty() && *s != "disable") {
        query = format!("sslmode={ssl}");
    }

    let query = if query.is_empty() { String::new() } else { format!("?{query}") };
    Some(format!("{}://{user_info}{host_port}{path}{query}", meta.scheme))
}
