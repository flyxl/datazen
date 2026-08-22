//! Security-focused tests for the `datazen://` protocol attack surface (F2).
//!
//! Complements the functional unit tests in [`super::protocol`] with
//! adversarial inputs: encoded/double-encoded traversal bypasses, Windows
//! host forms, host obfuscation, deep-link parameter handling, response
//! error contracts and MIME whitelist edges.

use std::fs;
use std::path::Path;

use serde_json::{Map, Value};
use tauri::http;

use super::protocol::{
    content_type_for, parse_datazen_uri, route_datazen_request, safe_relative_path, DatazenOutcome,
    PLUGINS_OPEN_PAGE_EVENT,
};
use super::{parse_manifest, PluginManager};

const MANIFEST: &str = r#"{
  "id": "acme.bill-audit",
  "name": "Bill Audit",
  "version": "1.0.0",
  "apiVersion": 2,
  "entry": "index.html",
  "contributes": {
    "pages": [{ "id": "quota-check", "title": "Quota Check", "icon": "assets/icon.svg" }]
  }
}"#;

fn write_text(dir: &Path, rel: &str, content: &str) {
    write_bytes(dir, rel, content.as_bytes());
}

fn write_bytes(dir: &Path, rel: &str, content: &[u8]) {
    let path = dir.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

/// Fixture whose package passed install-time validation (`load_from_disk`),
/// containing one file per whitelisted extension plus host-managed state.
fn rich_manager(dir: &Path) -> PluginManager {
    write_text(dir, "acme.bill-audit/manifest.json", MANIFEST);
    write_text(dir, "acme.bill-audit/index.html", "<html>bill-audit</html>");
    write_text(dir, "acme.bill-audit/app.js", "console.log(1)");
    write_text(dir, "acme.bill-audit/module.mjs", "export const x = 1;");
    write_text(dir, "acme.bill-audit/style.css", "body {}");
    write_text(dir, "acme.bill-audit/data.json", "{\"k\":1}");
    write_text(dir, "acme.bill-audit/assets/icon.svg", "<svg/>");
    write_bytes(dir, "acme.bill-audit/pic.png", b"\x89PNG\r\n\x1a\n");
    write_bytes(dir, "acme.bill-audit/pic.webp", b"RIFF....WEBP");
    write_bytes(dir, "acme.bill-audit/font.woff2", b"wOF2");
    write_bytes(dir, "acme.bill-audit/font.woff", b"wOFF");
    // Host-managed state files that must never be servable.
    write_text(dir, "acme.bill-audit/.storage.json", "{}");
    write_text(dir, "acme.bill-audit/.enabled", "1\n");

    let manager = PluginManager::new(dir.to_path_buf());
    assert_eq!(manager.load_from_disk(), 1);
    manager
}

/// Fixture registered without a package rescan, simulating files dropped
/// into an installed plugin directory after install-time validation.
fn registered_manager(dir: &Path) -> PluginManager {
    write_text(dir, "acme.bill-audit/manifest.json", MANIFEST);
    write_text(dir, "acme.bill-audit/index.html", "<html>bill-audit</html>");
    write_text(dir, "acme.bill-audit/data.json", "{\"k\":1}");
    write_text(dir, "acme.bill-audit/assets/icon.svg", "<svg/>");

    let manager = PluginManager::new(dir.to_path_buf());
    manager
        .register(parse_manifest(MANIFEST).unwrap(), true)
        .unwrap();
    manager
}

fn route(manager: &PluginManager, uri: &str) -> Result<DatazenOutcome, http::StatusCode> {
    let (plugin_id, path, query) =
        parse_datazen_uri(uri).map_err(|_| http::StatusCode::NOT_FOUND)?;
    route_datazen_request(manager, &plugin_id, &path, &query)
}

// ---------------------------------------------------------------------------
// Encoded traversal bypass attempts
// ---------------------------------------------------------------------------

#[test]
fn fully_encoded_traversal_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let manager = rich_manager(dir.path());
    for uri in [
        "datazen://acme.bill-audit/%2e%2e%2f%2e%2e%2fsettings.json",
        // Uppercase hex escapes must behave identically.
        "datazen://acme.bill-audit/%2E%2E%2Fsecret.json",
        // Trailing-dot-dot only, fully encoded.
        "datazen://acme.bill-audit/%2e%2e",
        // Mixed: one real separator, encoded dots.
        "datazen://acme.bill-audit/%2e%2e/settings.json",
    ] {
        assert_eq!(
            route(&manager, uri),
            Err(http::StatusCode::NOT_FOUND),
            "{uri}"
        );
    }
}

#[test]
fn encoded_backslash_traversal_is_rejected() {
    assert_eq!(
        safe_relative_path(
            &super::protocol::parse_datazen_uri("datazen://acme.bill-audit/%2E%2E\\secret.json")
                .unwrap()
                .1
        ),
        Err("backslash not allowed: ..\\secret.json".to_string())
    );

    let dir = tempfile::tempdir().unwrap();
    let manager = rich_manager(dir.path());
    for uri in [
        // Fully encoded backslash (%5C) between encoded dot-dots.
        "datazen://acme.bill-audit/%2e%2e%5csettings.json",
        // Encoded dots + raw backslash.
        "datazen://acme.bill-audit/%2E%2E\\secret.json",
        // Plain name with embedded backslash.
        "datazen://acme.bill-audit/a\\b.html",
    ] {
        assert_eq!(
            route(&manager, uri),
            Err(http::StatusCode::NOT_FOUND),
            "{uri}"
        );
    }
}

#[test]
fn double_encoding_is_decoded_exactly_once() {
    // `%252e%252e%252f` decodes to the LITERAL string `%2e%2e%2f`, which is a
    // plain (odd-looking) file name — never re-decoded into `../`.
    let (_, path, _) =
        parse_datazen_uri("datazen://acme.bill-audit/%252e%252e%252fsecret.html").unwrap();
    assert_eq!(path, "%2e%2e%2fsecret.html");
    assert!(
        safe_relative_path(&path).is_ok(),
        "literal %2e.. is a benign name"
    );

    let dir = tempfile::tempdir().unwrap();
    let manager = rich_manager(dir.path());
    for uri in [
        "datazen://acme.bill-audit/%252e%252e%252fsecret.html",
        "datazen://acme.bill-audit/assets%252f..%252f..%252fsecret.json",
        // Double-encoded NUL: decodes once to literal `%00`.
        "datazen://acme.bill-audit/index.html%2500",
    ] {
        assert_eq!(
            route(&manager, uri),
            Err(http::StatusCode::NOT_FOUND),
            "{uri}"
        );
    }
}

#[test]
fn utf8_overlong_sequences_cannot_smuggle_dot_slash_or_hidden_files() {
    let dir = tempfile::tempdir().unwrap();
    let manager = rich_manager(dir.path());
    for uri in [
        // Overlong `.` (C0 AE) lossily decodes to U+FFFD, never to ASCII '.'.
        "datazen://acme.bill-audit/%c0%ae%c0%ae/settings.json",
        // Overlong `/`: stays U+FFFD, so no separator appears.
        "datazen://acme.bill-audit/assets%c0%af..%c0%afsettings.json",
        // Overlong prefix before `.storage.json` cannot revive the hidden name.
        "datazen://acme.bill-audit/%c0%ae.storage.json",
        // Invalid UTF-8 continuation bytes.
        "datazen://acme.bill-audit/%ff%fe.html",
    ] {
        assert_eq!(
            route(&manager, uri),
            Err(http::StatusCode::NOT_FOUND),
            "{uri}"
        );
    }
}

#[test]
fn nul_truncation_variants_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let manager = rich_manager(dir.path());
    for uri in [
        // Classic truncation: index.html%00 → anything-after is ignored by
        // naive C string handling; here the decoded NUL must abort outright.
        "datazen://acme.bill-audit/index.html%00",
        "datazen://acme.bill-audit/assets/icon.svg%00.exe",
        "datazen://acme.bill-audit/assets%2Ficon.svg%00.exe",
        "datazen://acme.bill-audit/%00.html",
        "datazen://acme.bill-audit/a/%00",
    ] {
        assert_eq!(
            route(&manager, uri),
            Err(http::StatusCode::NOT_FOUND),
            "{uri}"
        );
    }

    // Unit-level: the decoded forms are rejected by the path validator too.
    for decoded in ["index.html\0", "assets/icon.svg\0.exe", "\0.html"] {
        assert!(
            safe_relative_path(decoded).is_err(),
            "`{decoded:?}` should be rejected"
        );
    }
}

#[test]
fn malformed_escapes_stay_literal_without_panicking() {
    let dir = tempfile::tempdir().unwrap();
    let manager = rich_manager(dir.path());
    for uri in [
        "datazen://acme.bill-audit/a%.html",
        "datazen://acme.bill-audit/%zz.html",
        "datazen://acme.bill-audit/index.htm%2",
        "datazen://acme.bill-audit/%",
    ] {
        assert_eq!(
            route(&manager, uri),
            Err(http::StatusCode::NOT_FOUND),
            "{uri}"
        );
    }
}

// ---------------------------------------------------------------------------
// Windows host forms
// ---------------------------------------------------------------------------

#[test]
fn windows_forms_serve_identical_asset_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let manager = rich_manager(dir.path());

    let native = parse_datazen_uri("datazen://acme.bill-audit/index.html?v=1.0").unwrap();
    for windows in [
        "http://datazen./acme.bill-audit/index.html?v=1.0",
        "https://datazen./acme.bill-audit/index.html?v=1.0",
        "HTTP://DATAZEN./acme.bill-audit/index.html?v=1.0",
    ] {
        assert_eq!(native, parse_datazen_uri(windows).unwrap(), "{windows}");
        assert_eq!(
            route(&manager, windows).unwrap(),
            DatazenOutcome::Asset {
                content_type: "text/html",
                bytes: b"<html>bill-audit</html>".to_vec(),
            },
            "{windows}"
        );
    }
}

#[test]
fn windows_form_backslash_traversal_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let manager = rich_manager(dir.path());
    for uri in [
        "http://datazen./acme.bill-audit/assets%5C..%5C..%5Csettings.json",
        "https://datazen./acme.bill-audit/assets\\..\\..\\settings.json",
        "http://datazen./acme.bill-audit/..\\..\\settings.json",
    ] {
        assert_eq!(
            route(&manager, uri),
            Err(http::StatusCode::NOT_FOUND),
            "{uri}"
        );
    }
}

#[test]
fn windows_dotted_host_without_separator_parses_leniently() {
    // `http://datazen.acme.bill-audit/...` (no `/` right after `datazen.`)
    // is accepted as an alias of the canonical Windows form. Lenient superset:
    // the same plugin-id/host/path/MIME checks still gate every request.
    let canonical = parse_datazen_uri("datazen://acme.bill-audit/index.html").unwrap();
    assert_eq!(
        parse_datazen_uri("http://datazen.acme.bill-audit/index.html"),
        Ok(canonical.clone())
    );
    assert_eq!(
        parse_datazen_uri("https://datazen.acme.bill-audit/index.html"),
        Ok(canonical)
    );
    assert_eq!(
        parse_datazen_uri("https://datazenacme.bill-audit/index.html"),
        Err("unsupported scheme: https://datazenacme.bill-audit/index.html".to_string())
    );
}

#[test]
fn drive_letter_component_cannot_escape_containment() {
    let dir = tempfile::tempdir().unwrap();
    let manager = rich_manager(dir.path());
    // On Unix `C:` is just a directory name (nothing there → 404). On Windows
    // `Path::join` would replace the base for prefixed paths, so this exercises
    // the `ensure_within_plugin_dir` containment backstop instead.
    for uri in [
        "datazen://acme.bill-audit/C:/evil.json",
        "http://datazen./acme.bill-audit/C%3A/evil.json",
    ] {
        assert_eq!(
            route(&manager, uri),
            Err(http::StatusCode::NOT_FOUND),
            "{uri}"
        );
    }
}

#[test]
fn empty_windows_host_segment_is_rejected() {
    for uri in ["http://datazen.", "http://datazen./", "https://datazen."] {
        assert!(parse_datazen_uri(uri).is_err(), "`{uri}` should not parse");
    }
}

// ---------------------------------------------------------------------------
// Host obfuscation
// ---------------------------------------------------------------------------

#[test]
fn host_obfuscation_variants_are_rejected() {
    for uri in [
        "datazen://ACME.bill-audit/index.html",
        "datazen://Acme.BILL-audit/index.html",
        "datazen://acme.bill-audit./index.html",
        "datazen://acme.bill-audit:8080/index.html",
        // Hosts are never percent-decoded: encoded publisher stays invalid.
        "datazen://%61cme.bill-audit/index.html",
        "datazen:// acme.bill-audit/index.html",
        "datazen://acme.bill-audit /index.html",
        "datazen://sub.acme.bill-audit/index.html",
    ] {
        assert!(
            parse_datazen_uri(uri).is_err(),
            "`{uri}` should be rejected"
        );
    }
}

#[test]
fn scheme_match_is_case_insensitive_but_prefix_exact() {
    // Scheme casing is irrelevant…
    assert!(parse_datazen_uri("DataZen://acme.bill-audit/index.html").is_ok());

    // …but look-alikes must not slip through:
    assert!(parse_datazen_uri("datazen:/acme.bill-audit/index.html").is_err());
    assert!(parse_datazen_uri("xdatazen://acme.bill-audit/index.html").is_err());
    assert!(parse_datazen_uri("datazen:\\\\acme.bill-audit\\index.html").is_err());
    assert!(
        parse_datazen_uri("http://evil.com/datazen./acme.bill-audit/index.html").is_err(),
        "foreign origin must not ride the windows-form prefix"
    );
}

// ---------------------------------------------------------------------------
// `open` deep link
// ---------------------------------------------------------------------------

#[test]
fn open_params_special_characters_forwarded_verbatim() {
    let dir = tempfile::tempdir().unwrap();
    let manager = rich_manager(dir.path());

    let outcome = route(
        &manager,
        "datazen://acme.bill-audit/open?page=quota-check&note=a%26b%3Dc&cn=%E4%B8%AD%E6%96%87&s=hello+world&pct=100%25&eq=a%3Db",
    )
    .expect("deep link should resolve");

    let mut expected: Map<String, Value> = Map::new();
    expected.insert("note".into(), Value::String("a&b=c".into()));
    expected.insert("cn".into(), Value::String("中文".into()));
    expected.insert("s".into(), Value::String("hello world".into()));
    expected.insert("pct".into(), Value::String("100%".into()));
    expected.insert("eq".into(), Value::String("a=b".into()));

    assert_eq!(
        outcome,
        DatazenOutcome::OpenPage {
            plugin_id: "acme.bill-audit".into(),
            page_id: "quota-check".into(),
            params: expected,
        }
    );
}

#[test]
fn open_missing_or_empty_page_param_is_404() {
    let dir = tempfile::tempdir().unwrap();
    let manager = rich_manager(dir.path());
    for uri in [
        "datazen://acme.bill-audit/open?",
        "datazen://acme.bill-audit/open?other=1",
        "datazen://acme.bill-audit/open?page=",
        "datazen://acme.bill-audit/open?page=%20",
        "http://datazen./acme.bill-audit/open",
    ] {
        assert_eq!(
            route(&manager, uri),
            Err(http::StatusCode::NOT_FOUND),
            "{uri}"
        );
    }
}

#[test]
fn open_extra_segments_are_404_in_both_url_forms() {
    let dir = tempfile::tempdir().unwrap();
    let manager = rich_manager(dir.path());
    for uri in [
        "datazen://acme.bill-audit/open/sub?page=quota-check",
        "datazen://acme.bill-audit/open/",
        "http://datazen./acme.bill-audit/open/sub?page=quota-check",
    ] {
        assert_eq!(
            route(&manager, uri),
            Err(http::StatusCode::NOT_FOUND),
            "{uri}"
        );
    }
}

#[test]
fn open_page_id_must_match_contribution_exactly() {
    let dir = tempfile::tempdir().unwrap();
    let manager = rich_manager(dir.path());
    for uri in [
        "datazen://acme.bill-audit/open?page=Quota-Check",
        "datazen://acme.bill-audit/open?page=quota-check%2Fx",
        "datazen://acme.bill-audit/open?page=../quota-check",
        "datazen://acme.bill-audit/open?page=nope",
    ] {
        assert_eq!(
            route(&manager, uri),
            Err(http::StatusCode::NOT_FOUND),
            "{uri}"
        );
    }
}

#[test]
fn reserved_open_word_is_case_sensitive() {
    let dir = tempfile::tempdir().unwrap();
    let manager = rich_manager(dir.path());
    // `OPEN` is not the reserved word → treated as an asset name → 404.
    assert_eq!(
        route(&manager, "datazen://acme.bill-audit/OPEN?page=quota-check"),
        Err(http::StatusCode::NOT_FOUND)
    );
}

#[test]
fn duplicate_page_param_last_wins() {
    let dir = tempfile::tempdir().unwrap();
    let manager = rich_manager(dir.path());
    let outcome = route(
        &manager,
        "datazen://acme.bill-audit/open?page=wrong&page=quota-check",
    )
    .expect("last duplicate wins");
    assert_eq!(
        outcome,
        DatazenOutcome::OpenPage {
            plugin_id: "acme.bill-audit".into(),
            page_id: "quota-check".into(),
            params: Map::new(),
        }
    );
}

#[test]
fn open_page_event_constant_matches_spec() {
    // Payload contract `{pluginId, pageId, params}` is built in
    // `emit_open_page`; the channel name is part of the frontend contract.
    assert_eq!(PLUGINS_OPEN_PAGE_EVENT, "plugins:open-page");
}

// ---------------------------------------------------------------------------
// Error contract: bare status codes, no detail leakage
// ---------------------------------------------------------------------------

#[test]
fn attack_surface_errors_are_bare_status_codes() {
    let dir = tempfile::tempdir().unwrap();
    let manager = rich_manager(dir.path());

    let cases: &[(&str, http::StatusCode)] = &[
        (
            "datazen://acme.bill-audit/%2e%2e%2fsettings.json",
            http::StatusCode::NOT_FOUND,
        ),
        (
            "datazen://acme.bill-audit/.storage.json",
            http::StatusCode::NOT_FOUND,
        ),
        (
            "datazen://acme.bill-audit/evil.exe",
            http::StatusCode::NOT_FOUND,
        ),
        (
            "datazen://acme.bill-audit/open?page=nope",
            http::StatusCode::NOT_FOUND,
        ),
        (
            "datazen://acme.ghost/index.html",
            http::StatusCode::NOT_FOUND,
        ),
    ];
    for (uri, expected) in cases {
        // Errors are bare `StatusCode`s by type — no detail payload exists to leak.
        assert_eq!(&route(&manager, uri), &Err(*expected), "{uri}");
    }

    // Disabled plugin → 403, equally detail-free.
    manager.set_enabled("acme.bill-audit", false).unwrap();
    assert_eq!(
        route(&manager, "datazen://acme.bill-audit/index.html"),
        Err(http::StatusCode::FORBIDDEN)
    );
    assert_eq!(
        route(&manager, "datazen://acme.bill-audit/open?page=quota-check"),
        Err(http::StatusCode::FORBIDDEN)
    );
}

// ---------------------------------------------------------------------------
// MIME whitelist
// ---------------------------------------------------------------------------

#[test]
fn mime_table_full_case_insensitive_assertion() {
    const TABLE: &[(&str, &str)] = &[
        ("html", "text/html"),
        ("js", "text/javascript"),
        ("mjs", "text/javascript"),
        ("css", "text/css"),
        ("json", "application/json"),
        ("svg", "image/svg+xml"),
        ("png", "image/png"),
        ("webp", "image/webp"),
        ("woff2", "font/woff2"),
        ("woff", "font/woff"),
    ];
    for (ext, mime) in TABLE {
        assert_eq!(content_type_for(ext), Some(*mime), ".{ext}");
        assert_eq!(
            content_type_for(&ext.to_ascii_uppercase()),
            Some(*mime),
            ".{ext} upper"
        );
        let mut mixed = ext.to_ascii_uppercase();
        mixed.replace_range(1..2, ext.get(1..2).unwrap());
        assert_eq!(content_type_for(&mixed), Some(*mime), ".{mixed} mixed");
    }

    for unknown in [
        "exe", "sh", "txt", "zip", "gif", "jpg", "htm", "html5", "wasm", "dll", "bat", "cmd",
        "ps1", "",
    ] {
        assert_eq!(
            content_type_for(unknown),
            None,
            ".{unknown} must never guess a type"
        );
    }
}

#[test]
fn every_whitelisted_extension_is_served_with_declared_type() {
    let dir = tempfile::tempdir().unwrap();
    let manager = rich_manager(dir.path());

    let cases: &[(&str, &str, &[u8])] = &[
        ("index.html", "text/html", b"<html>bill-audit</html>"),
        ("app.js", "text/javascript", b"console.log(1)"),
        ("module.mjs", "text/javascript", b"export const x = 1;"),
        ("style.css", "text/css", b"body {}"),
        ("data.json", "application/json", b"{\"k\":1}"),
        ("assets/icon.svg", "image/svg+xml", b"<svg/>"),
        ("pic.png", "image/png", b"\x89PNG\r\n\x1a\n"),
        ("pic.webp", "image/webp", b"RIFF....WEBP"),
        ("font.woff2", "font/woff2", b"wOF2"),
        ("font.woff", "font/woff", b"wOFF"),
    ];
    for (rel, mime, bytes) in cases {
        let uri = format!("datazen://acme.bill-audit/{rel}");
        assert_eq!(
            route(&manager, &uri).unwrap(),
            DatazenOutcome::Asset {
                content_type: mime,
                bytes: bytes.to_vec(),
            },
            "{uri}"
        );
    }
}

#[test]
fn final_extension_wins_and_trailing_dot_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let manager = registered_manager(dir.path());
    write_text(
        dir.path(),
        "acme.bill-audit/logo.svg.html",
        "<html>not-svg</html>",
    );

    // Serving policy keys off the FINAL extension, not sniffing.
    assert_eq!(
        route(&manager, "datazen://acme.bill-audit/logo.svg.html").unwrap(),
        DatazenOutcome::Asset {
            content_type: "text/html",
            bytes: b"<html>not-svg</html>".to_vec(),
        }
    );

    // Trailing dot yields an empty extension → whitelist miss → 404.
    assert_eq!(
        route(&manager, "datazen://acme.bill-audit/index.html."),
        Err(http::StatusCode::NOT_FOUND)
    );
}

#[test]
fn dropped_non_whitelisted_files_stay_unreachable() {
    let dir = tempfile::tempdir().unwrap();
    let manager = registered_manager(dir.path());

    // Simulate post-install drops (install-time scan would have refused these).
    write_bytes(dir.path(), "acme.bill-audit/evil.exe", b"MZ");
    write_bytes(
        dir.path(),
        "acme.bill-audit/run.sh",
        b"#!/bin/sh\nrm -rf /\n",
    );
    write_text(dir.path(), "acme.bill-audit/notes.txt", "secret notes");
    write_bytes(dir.path(), "acme.bill-audit/archive.zip", b"PK\x03\x04");

    for rel in ["evil.exe", "run.sh", "notes.txt", "archive.zip"] {
        let uri = format!("datazen://acme.bill-audit/{rel}");
        assert_eq!(
            route(&manager, &uri),
            Err(http::StatusCode::NOT_FOUND),
            "{uri}"
        );
    }
}

// ---------------------------------------------------------------------------
// Symlink swap defense (runtime containment backstop)
// ---------------------------------------------------------------------------

#[test]
#[cfg(unix)]
fn symlink_inside_serves_and_outside_is_contained() {
    use std::os::unix::fs::symlink;

    let dir = tempfile::tempdir().unwrap();
    let manager = registered_manager(dir.path());
    let plugin_assets = dir.path().join("acme.bill-audit/assets");

    // Internal alias: resolves back into the plugin dir → served.
    symlink(Path::new("../data.json"), plugin_assets.join("alias.json")).unwrap();
    assert_eq!(
        route(&manager, "datazen://acme.bill-audit/assets/alias.json").unwrap(),
        DatazenOutcome::Asset {
            content_type: "application/json",
            bytes: b"{\"k\":1}".to_vec(),
        }
    );

    // External swap: symlink pointing out of the package → contained → 404.
    let outside = tempfile::tempdir().unwrap();
    write_text(outside.path(), "settings.json", "{\"secret\":true}");
    symlink(
        outside.path().join("settings.json"),
        plugin_assets.join("out.json"),
    )
    .unwrap();
    assert_eq!(
        route(&manager, "datazen://acme.bill-audit/assets/out.json"),
        Err(http::StatusCode::NOT_FOUND)
    );
}
