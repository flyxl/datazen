//! Shared workspace-app URI grammar, access decisions and package asset routing.
use crate::workspace_apps::{is_valid_wapp_id, manager::WappManager};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteStatus {
    NotFound,
    Forbidden,
}
impl RouteStatus {
    pub const NOT_FOUND: Self = Self::NotFound;
    pub const FORBIDDEN: Self = Self::Forbidden;
}
/// Event emitted when a `open` deep link resolves to a contributed page.
/// Payload: `{ wappId, pageId, params }`.
pub const WAPPS_OPEN_PAGE_EVENT: &str = "wapps:open-page";

/// Reserved first path segment marking a deep-link command (not an asset).
pub const OPEN_COMMAND: &str = "open";

// Explicit scheme sources alongside `'self'`: WebKit (macOS) serves
// custom-scheme documents from an opaque origin and does not match `'self'`
// against `datazen://` subresources, which blocked the page's own scripts and
// images (BUG-F9-04) — mirrors VSCode webviews enumerating the resource scheme
// explicitly in CSP instead of trusting `'self'`. `'self'` is kept because
// Windows/WebView2 maps the scheme to `http(s)://datazen.<host>/…`, where only
// `'self'` matches. `connect-src 'none'` keeps plugins offline; `data:` stays
// image-only for inline SVG data URLs.
pub const ASSET_CSP: &str = "default-src 'self' datazen:; script-src 'self' datazen:; \
style-src 'self' datazen: 'unsafe-inline'; img-src 'self' datazen: data:; \
font-src 'self' datazen:; connect-src 'none'";

/// Decoded query parameters keyed by parameter name.
pub type QueryMap = BTreeMap<String, String>;

/// Result of routing a parsed request against the wapp registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DatazenOutcome {
    /// Serve package file bytes with a Content-Type from the safe table.
    Asset {
        content_type: &'static str,
        bytes: Vec<u8>,
    },
    /// Emit [`WAPPS_OPEN_PAGE_EVENT`] and answer 200 with an empty body.
    OpenPage {
        wapp_id: String,
        page_id: String,
        /// Remaining query entries (`page` excluded) forwarded verbatim.
        params: Map<String, Value>,
    },
}

// ---------------------------------------------------------------------------
// URI parsing (pure)
// ---------------------------------------------------------------------------

/// Parse a raw `datazen://...` / `http(s)://datazen./...` request target into
/// `(wapp id, relative path-or-command, decoded query map)`.
///
/// Percent-escapes are decoded *before* any component validation so encoded
/// traversal sequences cannot slip past later checks.
pub fn parse_datazen_uri(uri: &str) -> Result<(String, String, QueryMap), String> {
    let without_scheme = strip_scheme(uri).ok_or_else(|| format!("unsupported scheme: {uri}"))?;

    // Split off the query string before any decoding.
    let (host_and_path, raw_query) = match without_scheme.split_once('?') {
        Some((hp, q)) => (hp, Some(q)),
        None => (without_scheme, None),
    };

    let (host, raw_path) = match host_and_path.split_once('/') {
        Some((h, p)) => (h, p),
        None => (host_and_path, ""),
    };

    // Host must be a valid `<publisher>.<name>` wapp id; this rejects hosts
    // without the publisher dot, uppercase ids and overlong segments.
    if !is_valid_wapp_id(host) {
        return Err(format!("invalid wapp host: `{host}`"));
    }

    let path = percent_decode(raw_path);
    let query = parse_query(raw_query);

    Ok((host.to_string(), path, query))
}

/// Strip one of the accepted scheme prefixes, returning `<host>/<path>?<query>`.
fn strip_scheme(uri: &str) -> Option<&str> {
    let lower = uri.to_ascii_lowercase();
    if lower.starts_with("datazen://") {
        return uri.get("datazen://".len()..);
    }
    // Windows WebView2 maps custom schemes to http(s) with a dotted host:
    // `http://datazen./acme.bill-audit/index.html`. The `/` separator is
    // mandatory (BUG-F2-01): spec §2.4 only defines
    // `http://datazen./<host>/<path>`, so `datazen.` followed directly by the
    // wapp id is not an alias and must fail as an unsupported scheme.
    for prefix in ["https://datazen.", "http://datazen."] {
        if lower.starts_with(prefix) {
            return uri.get(prefix.len()..)?.strip_prefix('/');
        }
    }
    None
}

/// Parse a raw query string into a decoded map. Duplicate keys: last wins,
/// missing values decode to the empty string.
fn parse_query(raw: Option<&str>) -> QueryMap {
    let mut map = QueryMap::new();
    let Some(raw) = raw else {
        return map;
    };
    for pair in raw.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        map.insert(decode_query_component(key), decode_query_component(value));
    }
    map
}

/// Decode one query component: `+` means space (form encoding), then
/// percent-escapes are resolved.
fn decode_query_component(input: &str) -> String {
    percent_decode(&input.replace('+', " "))
}

/// Lenient percent-decoding: invalid escapes are kept literally, non-UTF-8
/// sequences are replaced lossily. Paths/queries that survive validation only
/// ever contain benign characters anyway.
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                out.push(hi * 16 + lo);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_val(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Path safety + MIME (pure)
// ---------------------------------------------------------------------------

/// Validate a *decoded* relative path from a datazen URI: every `/`-separated
/// component must be non-empty, must not start with `.` (rejects `.storage.json`,
/// `.enabled` and any hidden component, including `.` / `..`), and the whole
/// string must not contain backslashes or NUL bytes.
pub fn safe_relative_path(path: &str) -> Result<PathBuf, String> {
    if path.is_empty() {
        return Err("empty path".into());
    }
    if path.contains('\\') {
        return Err(format!("backslash not allowed: {path}"));
    }
    if path.contains('\0') {
        return Err(format!("NUL byte not allowed: {path}"));
    }
    let mut rel = PathBuf::new();
    for component in path.split('/') {
        if component.is_empty() || component.starts_with('.') {
            return Err(format!("unsafe path component in `{path}`"));
        }
        rel.push(component);
    }
    Ok(rel)
}

/// Safe Content-Type table (§2.4): extensions outside the package whitelist
/// yield `None`, which callers must turn into 404 — never guess a type.
pub fn content_type_for(ext: &str) -> Option<&'static str> {
    match ext.to_ascii_lowercase().as_str() {
        "html" => Some("text/html"),
        "js" | "mjs" => Some("text/javascript"),
        "css" => Some("text/css"),
        "json" => Some("application/json"),
        "svg" => Some("image/svg+xml"),
        "png" => Some("image/png"),
        "webp" => Some("image/webp"),
        "woff2" => Some("font/woff2"),
        "woff" => Some("font/woff"),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Routing (pure against the registry; testable without a Tauri app)
// ---------------------------------------------------------------------------

/// Route `(wapp id, path, query)` through the registry: existence → enabled
/// → command/asset dispatch. Errors are bare status codes (404 / 403) so the
/// HTTP layer can respond without leaking details.
pub fn route_datazen_request(
    manager: &WappManager,
    wapp_id: &str,
    path: &str,
    query: &QueryMap,
) -> Result<DatazenOutcome, RouteStatus> {
    let Some(loaded) = manager.get(wapp_id) else {
        return Err(RouteStatus::NOT_FOUND);
    };
    if !loaded.enabled {
        return Err(RouteStatus::FORBIDDEN);
    }

    let mut segments = path.split('/');
    let first = segments.next().unwrap_or("");
    if first == OPEN_COMMAND {
        if segments.next().is_some() {
            return Err(RouteStatus::NOT_FOUND);
        }
        let page_id = query.get("page").ok_or(RouteStatus::NOT_FOUND)?;
        loaded
            .manifest
            .contributes
            .pages
            .iter()
            .find(|page| page.id == *page_id)
            .ok_or(RouteStatus::NOT_FOUND)?;

        let params: Map<String, Value> = query
            .iter()
            .filter(|(key, _)| key.as_str() != "page")
            .map(|(key, value)| (key.clone(), Value::String(value.clone())))
            .collect();

        return Ok(DatazenOutcome::OpenPage {
            wapp_id: wapp_id.to_string(),
            page_id: page_id.clone(),
            params,
        });
    }

    // Asset form: validate, whitelist the extension, then read the file.
    let rel = safe_relative_path(path).map_err(|_| RouteStatus::NOT_FOUND)?;
    let ext = rel
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default();
    let content_type = content_type_for(ext).ok_or(RouteStatus::NOT_FOUND)?;

    let wapp_dir = manager.wapp_dir(wapp_id);
    let file_path = wapp_dir.join(&rel);
    if !file_path.is_file() {
        return Err(RouteStatus::NOT_FOUND);
    }
    ensure_within_wapp_dir(&wapp_dir, &file_path)?;

    let bytes = fs::read(&file_path).map_err(|_| RouteStatus::NOT_FOUND)?;
    Ok(DatazenOutcome::Asset {
        content_type,
        bytes,
    })
}

/// Symlink-swap defense: resolve both directories and require containment.
fn ensure_within_wapp_dir(wapp_dir: &Path, file_path: &Path) -> Result<(), RouteStatus> {
    let canonical_dir = fs::canonicalize(wapp_dir).map_err(|_| RouteStatus::NOT_FOUND)?;
    let canonical_file = fs::canonicalize(file_path).map_err(|_| RouteStatus::NOT_FOUND)?;
    if canonical_file.starts_with(&canonical_dir) {
        Ok(())
    } else {
        Err(RouteStatus::NOT_FOUND)
    }
}
