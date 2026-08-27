//! `datazen://` custom protocol: plugin asset service + deep-link commands.
//!
//! URL syntax: `datazen://<publisher>.<extension-name>/<path-or-command>?<query>`
//!
//! - **path form** (`datazen://acme.bill-audit/index.html`): host == `manifest.id`,
//!   remainder is a package-relative file path served with a fixed safe
//!   Content-Type table.
//! - **command form** (`datazen://acme.bill-audit/open?page=quota-check&uid=1`):
//!   first path segment `open` is a reserved deep link; the host emits
//!   [`EXTENSIONS_OPEN_PAGE_EVENT`] to the frontend instead of serving bytes.
//!
//! On Windows the WebView exposes custom schemes as
//! `http://datazen./<host>/<path>` while macOS/Linux keep
//! `datazen://<host>/<path>`; [`parse_datazen_uri`] accepts both.
//!
//! Every response carries a strict CSP, `X-Content-Type-Options: nosniff`
//! and `Cache-Control: no-cache`. Error responses (403/404) have empty bodies.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};
use tauri::{http, Emitter, Manager, Runtime, UriSchemeContext};

use super::{is_valid_extension_id, ExtensionManager};
use crate::commands::AppState;

/// Event emitted when a `open` deep link resolves to a contributed page.
/// Payload: `{ pluginId, pageId, params }`.
pub const EXTENSIONS_OPEN_PAGE_EVENT: &str = "plugins:open-page";

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
const ASSET_CSP: &str = "default-src 'self' datazen:; script-src 'self' datazen:; \
style-src 'self' datazen: 'unsafe-inline'; img-src 'self' datazen: data:; \
font-src 'self' datazen:; connect-src 'none'";

/// Decoded query parameters keyed by parameter name.
pub type QueryMap = BTreeMap<String, String>;

/// Result of routing a parsed request against the plugin registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DatazenOutcome {
    /// Serve package file bytes with a Content-Type from the safe table.
    Asset {
        content_type: &'static str,
        bytes: Vec<u8>,
    },
    /// Emit [`EXTENSIONS_OPEN_PAGE_EVENT`] and answer 200 with an empty body.
    OpenPage {
        plugin_id: String,
        page_id: String,
        /// Remaining query entries (`page` excluded) forwarded verbatim.
        params: Map<String, Value>,
    },
}

// ---------------------------------------------------------------------------
// URI parsing (pure)
// ---------------------------------------------------------------------------

/// Parse a raw `datazen://...` / `http(s)://datazen./...` request target into
/// `(plugin id, relative path-or-command, decoded query map)`.
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

    // Host must be a valid `<publisher>.<name>` plugin id; this rejects hosts
    // without the publisher dot, uppercase ids and overlong segments.
    if !is_valid_extension_id(host) {
        return Err(format!("invalid extension host: `{host}`"));
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
    // plugin id is not an alias and must fail as an unsupported scheme.
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
pub(crate) fn safe_relative_path(path: &str) -> Result<PathBuf, String> {
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
pub(crate) fn content_type_for(ext: &str) -> Option<&'static str> {
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

/// Route `(plugin id, path, query)` through the registry: existence → enabled
/// → command/asset dispatch. Errors are bare status codes (404 / 403) so the
/// HTTP layer can respond without leaking details.
pub(crate) fn route_datazen_request(
    manager: &ExtensionManager,
    plugin_id: &str,
    path: &str,
    query: &QueryMap,
) -> Result<DatazenOutcome, http::StatusCode> {
    use http::StatusCode;

    let Some(loaded) = manager.get(plugin_id) else {
        return Err(StatusCode::NOT_FOUND);
    };
    if !loaded.enabled {
        return Err(StatusCode::FORBIDDEN);
    }

    let mut segments = path.split('/');
    let first = segments.next().unwrap_or("");
    if first == OPEN_COMMAND {
        if segments.next().is_some() {
            return Err(StatusCode::NOT_FOUND);
        }
        let page_id = query.get("page").ok_or(StatusCode::NOT_FOUND)?;
        loaded
            .manifest
            .contributes
            .pages
            .iter()
            .find(|page| page.id == *page_id)
            .ok_or(StatusCode::NOT_FOUND)?;

        let params: Map<String, Value> = query
            .iter()
            .filter(|(key, _)| key.as_str() != "page")
            .map(|(key, value)| (key.clone(), Value::String(value.clone())))
            .collect();

        return Ok(DatazenOutcome::OpenPage {
            plugin_id: plugin_id.to_string(),
            page_id: page_id.clone(),
            params,
        });
    }

    // Asset form: validate, whitelist the extension, then read the file.
    let rel = safe_relative_path(path).map_err(|_| StatusCode::NOT_FOUND)?;
    let ext = rel
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default();
    let content_type = content_type_for(ext).ok_or(StatusCode::NOT_FOUND)?;

    let plugin_dir = manager.plugin_dir(plugin_id);
    let file_path = plugin_dir.join(&rel);
    if !file_path.is_file() {
        return Err(StatusCode::NOT_FOUND);
    }
    ensure_within_plugin_dir(&plugin_dir, &file_path)?;

    let bytes = fs::read(&file_path).map_err(|_| StatusCode::NOT_FOUND)?;
    Ok(DatazenOutcome::Asset {
        content_type,
        bytes,
    })
}

/// Symlink-swap defense: resolve both directories and require containment.
fn ensure_within_plugin_dir(plugin_dir: &Path, file_path: &Path) -> Result<(), http::StatusCode> {
    use http::StatusCode;

    let canonical_dir = fs::canonicalize(plugin_dir).map_err(|_| StatusCode::NOT_FOUND)?;
    let canonical_file = fs::canonicalize(file_path).map_err(|_| StatusCode::NOT_FOUND)?;
    if canonical_file.starts_with(&canonical_dir) {
        Ok(())
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

// ---------------------------------------------------------------------------
// Tauri protocol handler
// ---------------------------------------------------------------------------

/// Entry point registered via
/// `tauri::Builder::register_uri_scheme_protocol("datazen", ...)`.
pub fn handle_datazen_request<R: Runtime>(
    ctx: UriSchemeContext<'_, R>,
    request: http::Request<Vec<u8>>,
) -> http::Response<Cow<'static, [u8]>> {
    use http::StatusCode;

    let app = ctx.app_handle();
    let state = app.state::<AppState>();
    let uri = request.uri().to_string();

    let response = match parse_datazen_uri(&uri) {
        Err(error) => {
            tracing::warn!(error = %error, "rejected malformed datazen URI");
            datazen_response(StatusCode::NOT_FOUND, None, Vec::new())
        }
        Ok((plugin_id, path, query)) => {
            match route_datazen_request(&state.extensions, &plugin_id, &path, &query) {
                Ok(DatazenOutcome::Asset {
                    content_type,
                    bytes,
                }) => datazen_response(StatusCode::OK, Some(content_type), bytes),
                Ok(outcome @ DatazenOutcome::OpenPage { .. }) => {
                    emit_open_page(app, &outcome);
                    datazen_response(StatusCode::OK, None, Vec::new())
                }
                Err(status) => {
                    tracing::warn!(plugin = %plugin_id, status = %status, "datazen request rejected");
                    datazen_response(status, None, Vec::new())
                }
            }
        }
    };
    response
}

/// Forward the deep link to the frontend; the workspace decides how to open
/// the plugin page tab (F3/F4 consumption).
fn emit_open_page<R: Runtime>(app: &tauri::AppHandle<R>, outcome: &DatazenOutcome) {
    let DatazenOutcome::OpenPage {
        plugin_id,
        page_id,
        params,
    } = outcome
    else {
        return;
    };
    let payload = serde_json::json!({
        "pluginId": plugin_id,
        "pageId": page_id,
        "params": params,
    });
    if let Err(error) = app.emit(EXTENSIONS_OPEN_PAGE_EVENT, payload) {
        tracing::warn!(error = %error, "failed to emit plugins:open-page");
    }
}

/// Build a response with the mandatory security headers on every reply.
fn datazen_response(
    status: http::StatusCode,
    content_type: Option<&'static str>,
    body: Vec<u8>,
) -> http::Response<Cow<'static, [u8]>> {
    let mut builder = http::Response::builder()
        .status(status)
        .header(http::header::CONTENT_SECURITY_POLICY, ASSET_CSP)
        .header(http::header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .header(http::header::CACHE_CONTROL, "no-cache");
    if let Some(content_type) = content_type {
        builder = builder.header(http::header::CONTENT_TYPE, content_type);
    }
    builder
        .body(Cow::Owned(body))
        .expect("static response parts always build")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const PAGE_MANIFEST: &str = r#"{
      "id": "acme.bill-audit",
      "name": "Bill Audit",
      "version": "1.0.0",
      "apiVersion": 2,
      "entry": "index.html",
      "contributes": {
        "pages": [{ "id": "quota-check", "title": "Quota Check", "icon": "assets/icon.svg" }]
      }
    }"#;

    fn write_file(dir: &Path, rel: &str, content: &str) {
        let path = dir.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    fn manager_with_page_plugin(dir: &Path) -> ExtensionManager {
        write_file(dir, "acme.bill-audit/manifest.json", PAGE_MANIFEST);
        write_file(dir, "acme.bill-audit/index.html", "<html>bill-audit</html>");
        write_file(dir, "acme.bill-audit/assets/icon.svg", "<svg/>");
        write_file(dir, "acme.bill-audit/data.json", "{\"k\":1}");
        write_file(dir, "acme.bill-audit/.storage.json", "{}");
        write_file(dir, "acme.bill-audit/.enabled", "1\n");

        let manager = ExtensionManager::new(dir.to_path_buf());
        assert_eq!(manager.load_from_disk(), 1);
        manager
    }

    fn route(manager: &ExtensionManager, uri: &str) -> Result<DatazenOutcome, http::StatusCode> {
        let (plugin_id, path, query) =
            parse_datazen_uri(uri).map_err(|_| http::StatusCode::NOT_FOUND)?;
        route_datazen_request(manager, &plugin_id, &path, &query)
    }

    // -- parse ---------------------------------------------------------------

    #[test]
    fn parses_native_scheme_form() {
        let (id, path, query) = parse_datazen_uri("datazen://acme.bill-audit/index.html").unwrap();
        assert_eq!(id, "acme.bill-audit");
        assert_eq!(path, "index.html");
        assert!(query.is_empty());
    }

    #[test]
    fn windows_http_form_is_equivalent_to_native_scheme() {
        let native = parse_datazen_uri("datazen://acme.bill-audit/assets/icon.svg?x=1");
        for windows in [
            "http://datazen./acme.bill-audit/assets/icon.svg?x=1",
            "https://datazen./acme.bill-audit/assets/icon.svg?x=1",
        ] {
            assert_eq!(native, parse_datazen_uri(windows), "{windows}");
        }
    }

    #[test]
    fn windows_form_without_separator_is_rejected() {
        // BUG-F2-01: `datazen.` followed directly by the plugin id (no `/`
        // separator) used to be accepted as a lenient alias. Spec §2.4 only
        // defines `http://datazen./<host>/<path>`, so these must fail with
        // "unsupported scheme" while the canonical form keeps parsing.
        for uri in [
            "http://datazen.acme.bill-audit/index.html",
            "https://datazen.acme.bill-audit/index.html",
            "HTTP://DATAZEN.ACME.BILL-AUDIT/index.html",
        ] {
            assert!(
                parse_datazen_uri(uri)
                    .unwrap_err()
                    .starts_with("unsupported scheme"),
                "`{uri}` should be rejected as an unsupported scheme"
            );
        }
        let native = parse_datazen_uri("datazen://acme.bill-audit/index.html").unwrap();
        assert_eq!(
            parse_datazen_uri("http://datazen./acme.bill-audit/index.html"),
            Ok(native),
            "canonical Windows form still parses and matches native"
        );
    }

    #[test]
    fn rejects_host_without_publisher_dot() {
        for uri in [
            "datazen://bill-audit/index.html",
            "datazen://Acme.bill-audit/index.html",
            "datazen:///index.html",
            "ftp://acme.bill-audit/index.html",
        ] {
            assert!(
                parse_datazen_uri(uri).is_err(),
                "`{uri}` should be rejected"
            );
        }
    }

    #[test]
    fn rejects_missing_path_but_keeps_host_check_in_route() {
        // Parse succeeds structurally; routing turns the empty path into 404.
        let (id, path, _) = parse_datazen_uri("datazen://acme.bill-audit").unwrap();
        assert_eq!(id, "acme.bill-audit");
        assert_eq!(path, "");
        let dir = tempfile::TempDir::new().unwrap();
        let manager = manager_with_page_plugin(dir.path());
        assert_eq!(
            route_datazen_request(&manager, &id, "", &QueryMap::new()),
            Err(http::StatusCode::NOT_FOUND)
        );
    }

    #[test]
    fn decodes_percent_escapes_and_query_params() {
        let (_, _, query) = parse_datazen_uri(
            "datazen://acme.bill-audit/open?page=quota-check&uid=123&q=a%20b&flag&plus=z+y",
        )
        .unwrap();
        assert_eq!(query.get("page").map(String::as_str), Some("quota-check"));
        assert_eq!(query.get("uid").map(String::as_str), Some("123"));
        assert_eq!(query.get("q").map(String::as_str), Some("a b"));
        assert_eq!(query.get("flag").map(String::as_str), Some(""));
        assert_eq!(query.get("plus").map(String::as_str), Some("z y"));

        let (_, path, _) =
            parse_datazen_uri("datazen://acme.bill-audit/assets%2Ficon.svg").unwrap();
        assert_eq!(path, "assets/icon.svg");
    }

    #[test]
    fn encoded_traversal_is_caught_after_decoding() {
        let dir = tempfile::TempDir::new().unwrap();
        let manager = manager_with_page_plugin(dir.path());
        assert_eq!(
            route(
                &manager,
                "datazen://acme.bill-audit/%2e%2e/%2e%2e/settings.json"
            ),
            Err(http::StatusCode::NOT_FOUND)
        );
    }

    // -- path safety ---------------------------------------------------------

    #[test]
    fn unsafe_paths_are_rejected() {
        for bad in [
            "..",
            "../secret.txt",
            "assets/../..",
            "a/../../b.png",
            "a\\b.html",
            "dir\\..\\..\\x",
            ".storage.json",
            ".enabled",
            ".hidden/file.html",
            "dir/.secret.png",
            "/etc/passwd",
            "//etc/passwd",
            "a/\0b",
            "%00.html",
            "",
        ] {
            let candidate = if bad.starts_with('%') || bad.contains('\0') {
                percent_decode(bad)
            } else {
                bad.to_string()
            };
            assert!(
                safe_relative_path(&candidate).is_err(),
                "`{bad}` should be rejected"
            );
        }
    }

    #[test]
    fn safe_paths_are_accepted() {
        for good in ["index.html", "assets/icon.svg", "a/b/c/d.json"] {
            assert!(safe_relative_path(good).is_ok(), "`{good}` should pass");
        }
    }

    // -- command vs asset dispatch -------------------------------------------

    #[test]
    fn open_command_resolves_contributed_page_and_splits_params() {
        let dir = tempfile::TempDir::new().unwrap();
        let manager = manager_with_page_plugin(dir.path());

        let outcome = route(
            &manager,
            "datazen://acme.bill-audit/open?page=quota-check&uid=123",
        )
        .expect("deep link should resolve");
        assert_eq!(
            outcome,
            DatazenOutcome::OpenPage {
                plugin_id: "acme.bill-audit".into(),
                page_id: "quota-check".into(),
                params: [("uid".to_string(), Value::String("123".into()))]
                    .into_iter()
                    .collect(),
            }
        );
    }

    #[test]
    fn unknown_commands_are_not_found() {
        let dir = tempfile::TempDir::new().unwrap();
        let manager = manager_with_page_plugin(dir.path());

        // Not a real file either → plain 404.
        assert_eq!(
            route(&manager, "datazen://acme.bill-audit/close?page=x"),
            Err(http::StatusCode::NOT_FOUND)
        );
        // Extra segments behind the reserved word are rejected outright.
        assert_eq!(
            route(&manager, "datazen://acme.bill-audit/open/sub"),
            Err(http::StatusCode::NOT_FOUND)
        );
    }

    #[test]
    fn open_without_known_page_is_not_found() {
        let dir = tempfile::TempDir::new().unwrap();
        let manager = manager_with_page_plugin(dir.path());

        for uri in [
            "datazen://acme.bill-audit/open",
            "datazen://acme.bill-audit/open?other=1",
            "datazen://acme.bill-audit/open?page=nope",
        ] {
            assert_eq!(
                route(&manager, uri),
                Err(http::StatusCode::NOT_FOUND),
                "{uri}"
            );
        }
    }

    // -- asset service -------------------------------------------------------

    #[test]
    fn serves_asset_bytes_with_content_type() {
        let dir = tempfile::TempDir::new().unwrap();
        let manager = manager_with_page_plugin(dir.path());

        let outcome = route(&manager, "datazen://acme.bill-audit/index.html").unwrap();
        assert_eq!(
            outcome,
            DatazenOutcome::Asset {
                content_type: "text/html",
                bytes: b"<html>bill-audit</html>".to_vec(),
            }
        );

        let outcome = route(&manager, "datazen://acme.bill-audit/data.json").unwrap();
        assert_eq!(
            outcome,
            DatazenOutcome::Asset {
                content_type: "application/json",
                bytes: b"{\"k\":1}".to_vec(),
            }
        );
    }

    #[test]
    fn disabled_plugin_is_forbidden_for_both_forms() {
        let dir = tempfile::TempDir::new().unwrap();
        let manager = manager_with_page_plugin(dir.path());
        manager.set_enabled("acme.bill-audit", false).unwrap();

        for uri in [
            "datazen://acme.bill-audit/index.html",
            "datazen://acme.bill-audit/open?page=quota-check",
        ] {
            assert_eq!(
                route(&manager, uri),
                Err(http::StatusCode::FORBIDDEN),
                "{uri}"
            );
        }

        manager.set_enabled("acme.bill-audit", true).unwrap();
        assert!(route(&manager, "datazen://acme.bill-audit/index.html").is_ok());
    }

    #[test]
    fn unknown_plugins_are_not_found() {
        let dir = tempfile::TempDir::new().unwrap();
        let manager = manager_with_page_plugin(dir.path());
        assert_eq!(
            route(&manager, "datazen://acme.ghost/index.html"),
            Err(http::StatusCode::NOT_FOUND)
        );
    }

    #[test]
    fn hidden_host_files_are_never_served() {
        let dir = tempfile::TempDir::new().unwrap();
        let manager = manager_with_page_plugin(dir.path());
        for uri in [
            "datazen://acme.bill-audit/.storage.json",
            "datazen://acme.bill-audit/.enabled",
        ] {
            assert_eq!(
                route(&manager, uri),
                Err(http::StatusCode::NOT_FOUND),
                "{uri}"
            );
        }
    }

    #[test]
    fn unknown_extension_yields_not_found_even_when_file_exists() {
        let dir = tempfile::TempDir::new().unwrap();
        write_file(dir.path(), "acme.bill-audit/manifest.json", PAGE_MANIFEST);
        write_file(dir.path(), "acme.bill-audit/blob.txt", "nope");

        let manager = ExtensionManager::new(dir.path().to_path_buf());
        // Register directly: simulates a file dropped into a plugin dir after
        // install-time validation (which itself rejects `.txt` packages).
        let manifest = super::super::parse_manifest(PAGE_MANIFEST).unwrap();
        manager.register(manifest, true).unwrap();

        assert_eq!(
            route(&manager, "datazen://acme.bill-audit/blob.txt"),
            Err(http::StatusCode::NOT_FOUND)
        );
    }

    // -- MIME table ----------------------------------------------------------

    #[test]
    fn mime_table_covers_whitelist_and_rejects_the_rest() {
        assert_eq!(content_type_for("html"), Some("text/html"));
        assert_eq!(content_type_for("js"), Some("text/javascript"));
        assert_eq!(content_type_for("mjs"), Some("text/javascript"));
        assert_eq!(content_type_for("css"), Some("text/css"));
        assert_eq!(content_type_for("json"), Some("application/json"));
        assert_eq!(content_type_for("svg"), Some("image/svg+xml"));
        assert_eq!(content_type_for("png"), Some("image/png"));
        assert_eq!(content_type_for("webp"), Some("image/webp"));
        assert_eq!(content_type_for("woff2"), Some("font/woff2"));
        assert_eq!(content_type_for("woff"), Some("font/woff"));

        // Case-insensitive.
        assert_eq!(content_type_for("SVG"), Some("image/svg+xml"));
        assert_eq!(content_type_for("HTML"), Some("text/html"));

        for unknown in ["exe", "txt", "gif", "sh", "html5", ""] {
            assert_eq!(
                content_type_for(unknown),
                None,
                ".{unknown} must not guess a type"
            );
        }
    }

    // -- response headers ----------------------------------------------------

    #[test]
    fn security_headers_are_always_injected() {
        for (status, body_len) in [
            (http::StatusCode::OK, 4usize),
            (http::StatusCode::FORBIDDEN, 0),
            (http::StatusCode::NOT_FOUND, 0),
        ] {
            let response = datazen_response(status, Some("text/html"), vec![0; body_len]);
            assert_eq!(response.status(), status);
            assert_eq!(
                response
                    .headers()
                    .get(http::header::CONTENT_SECURITY_POLICY)
                    .unwrap(),
                "default-src 'self' datazen:; script-src 'self' datazen:; \
                 style-src 'self' datazen: 'unsafe-inline'; img-src 'self' datazen: data:; \
                 font-src 'self' datazen:; connect-src 'none'"
            );
            assert_eq!(
                response
                    .headers()
                    .get(http::header::X_CONTENT_TYPE_OPTIONS)
                    .unwrap(),
                "nosniff"
            );
            assert_eq!(
                response.headers().get(http::header::CACHE_CONTROL).unwrap(),
                "no-cache"
            );
            assert_eq!(response.body().len(), body_len);
            if let Some(ct) = response.headers().get(http::header::CONTENT_TYPE) {
                assert_eq!(ct, "text/html");
            }
        }
    }
}
