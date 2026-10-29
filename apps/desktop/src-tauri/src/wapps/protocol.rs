//! `datazen://` custom protocol: wapp asset service + deep-link commands.
//!
//! URL syntax: `datazen://<publisher>.<extension-name>/<path-or-command>?<query>`
//!
//! - **path form** (`datazen://acme.bill-audit/index.html`): host == `manifest.id`,
//!   remainder is a package-relative file path served with a fixed safe
//!   Content-Type table.
//! - **command form** (`datazen://acme.bill-audit/open?page=quota-check&uid=1`):
//!   first path segment `open` is a reserved deep link; the host emits
//!   [`WAPPS_OPEN_PAGE_EVENT`] to the frontend instead of serving bytes.
//!
//! On Windows the WebView exposes custom schemes as
//! `http://datazen./<host>/<path>` while macOS/Linux keep
//! `datazen://<host>/<path>`; [`parse_datazen_uri`] accepts both.
//!
//! Every response carries a strict CSP, `X-Content-Type-Options: nosniff`
//! and `Cache-Control: no-cache`. Error responses (403/404) have empty bodies.

use std::borrow::Cow;
#[cfg(test)]
use std::path::Path;

#[cfg(test)]
use serde_json::Value;
use tauri::{http, Emitter, Manager, Runtime, UriSchemeContext};

use crate::commands::AppState;
use datazen::workspace_apps::manager::WappManager;

pub use datazen::workspace_apps::protocol::{
    parse_datazen_uri, DatazenOutcome, QueryMap, WAPPS_OPEN_PAGE_EVENT,
};
pub(crate) fn route_datazen_request(
    manager: &WappManager,
    wapp_id: &str,
    path: &str,
    query: &QueryMap,
) -> Result<DatazenOutcome, http::StatusCode> {
    datazen::workspace_apps::protocol::route_datazen_request(manager, wapp_id, path, query).map_err(
        |error| match error {
            datazen::workspace_apps::protocol::RouteStatus::NotFound => http::StatusCode::NOT_FOUND,
            datazen::workspace_apps::protocol::RouteStatus::Forbidden => {
                http::StatusCode::FORBIDDEN
            }
        },
    )
}
const ASSET_CSP: &str = datazen::workspace_apps::protocol::ASSET_CSP;
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
        Ok((wapp_id, path, query)) => {
            match route_datazen_request(&state.wapps, &wapp_id, &path, &query) {
                Ok(DatazenOutcome::Asset {
                    content_type,
                    bytes,
                }) => datazen_response(StatusCode::OK, Some(content_type), bytes),
                Ok(outcome @ DatazenOutcome::OpenPage { .. }) => {
                    emit_open_page(app, &outcome);
                    datazen_response(StatusCode::OK, None, Vec::new())
                }
                Err(status) => {
                    tracing::warn!(wapp = %wapp_id, status = %status, "datazen request rejected");
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
        wapp_id,
        page_id,
        params,
    } = outcome
    else {
        return;
    };
    let payload = serde_json::json!({
        "wappId": wapp_id,
        "pageId": page_id,
        "params": params,
    });
    if let Err(error) = app.emit(WAPPS_OPEN_PAGE_EVENT, payload) {
        tracing::warn!(error = %error, "failed to emit wapps:open-page");
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
    use datazen::workspace_apps::protocol::{content_type_for, safe_relative_path};
    #[cfg(test)]
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

    fn manager_with_page_wapp(dir: &Path) -> WappManager {
        write_file(dir, "acme.bill-audit/manifest.json", PAGE_MANIFEST);
        write_file(dir, "acme.bill-audit/index.html", "<html>bill-audit</html>");
        write_file(dir, "acme.bill-audit/assets/icon.svg", "<svg/>");
        write_file(dir, "acme.bill-audit/data.json", "{\"k\":1}");
        write_file(dir, "acme.bill-audit/.storage.json", "{}");
        write_file(dir, "acme.bill-audit/.enabled", "1\n");

        let manager = WappManager::new(dir.to_path_buf());
        assert_eq!(manager.load_from_disk(), 1);
        manager
    }

    fn route(manager: &WappManager, uri: &str) -> Result<DatazenOutcome, http::StatusCode> {
        let (wapp_id, path, query) =
            parse_datazen_uri(uri).map_err(|_| http::StatusCode::NOT_FOUND)?;
        route_datazen_request(manager, &wapp_id, &path, &query)
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
        // BUG-F2-01: `datazen.` followed directly by the wapp id (no `/`
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
        let manager = manager_with_page_wapp(dir.path());
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
        let manager = manager_with_page_wapp(dir.path());
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
                parse_datazen_uri(&format!("datazen://acme.test/{bad}"))
                    .expect("valid test URI")
                    .1
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
        let manager = manager_with_page_wapp(dir.path());

        let outcome = route(
            &manager,
            "datazen://acme.bill-audit/open?page=quota-check&uid=123",
        )
        .expect("deep link should resolve");
        assert_eq!(
            outcome,
            DatazenOutcome::OpenPage {
                wapp_id: "acme.bill-audit".into(),
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
        let manager = manager_with_page_wapp(dir.path());

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
        let manager = manager_with_page_wapp(dir.path());

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
        let manager = manager_with_page_wapp(dir.path());

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
    fn disabled_wapp_is_forbidden_for_both_forms() {
        let dir = tempfile::TempDir::new().unwrap();
        let manager = manager_with_page_wapp(dir.path());
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
    fn unknown_wapps_are_not_found() {
        let dir = tempfile::TempDir::new().unwrap();
        let manager = manager_with_page_wapp(dir.path());
        assert_eq!(
            route(&manager, "datazen://acme.ghost/index.html"),
            Err(http::StatusCode::NOT_FOUND)
        );
    }

    #[test]
    fn hidden_host_files_are_never_served() {
        let dir = tempfile::TempDir::new().unwrap();
        let manager = manager_with_page_wapp(dir.path());
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

        let manager = WappManager::new(dir.path().to_path_buf());
        // Register directly: simulates a file dropped into a plugin dir after
        // install-time validation (which itself rejects `.txt` packages).
        let manifest = datazen::workspace_apps::parse_manifest(PAGE_MANIFEST).unwrap();
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
