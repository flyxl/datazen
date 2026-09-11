use super::manifest::*;
use super::WAPP_API_VERSION;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

const PAGE_MANIFEST: &str = r#"{
  "id": "acme.bill-audit",
  "name": "Bill Audit",
  "version": "1.0.0",
  "apiVersion": 2,
  "entry": "index.html",
  "contributes": {
    "pages": [{ "id": "quota-check", "title": "Quota Check", "icon": "assets/icon.svg", "showIn": "workspace" }]
  },
  "permissions": ["context:connections", "storage:local"]
}"#;

const THEME_MANIFEST: &str = r#"{
  "id": "acme.midnight",
  "name": "Midnight",
  "version": "0.2.1",
  "apiVersion": 2,
  "contributes": {
    "themes": [{
      "id": "midnight-blue",
      "name": "Midnight Blue",
      "tokensCss": "themes/midnight-blue/tokens.css",
      "modes": ["dark"],
      "previewImage": "themes/midnight-blue/preview.png"
    }]
  }
}"#;

fn write_file(dir: &Path, rel: &str, content: &str) {
    let path = dir.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

/// Write a complete, valid pages-wapp package into `dir`.
fn write_page_wapp(dir: &Path) {
    write_file(dir, "manifest.json", PAGE_MANIFEST);
    write_file(dir, "index.html", "<html><body></body></html>");
    write_file(
        dir,
        "assets/icon.svg",
        "<svg xmlns='http://www.w3.org/2000/svg'/>",
    );
}

/// Write a complete, valid pure-theme wapp package into `dir`.
fn write_theme_wapp(dir: &Path) {
    write_file(dir, "manifest.json", THEME_MANIFEST);
    write_file(
        dir,
        "themes/midnight-blue/tokens.css",
        ":root { --c-accent: red; }",
    );
    write_file(dir, "themes/midnight-blue/preview.png", "pretend-png");
}

fn parsed(json: &str) -> WappManifest {
    parse_manifest(json).unwrap()
}

#[test]
fn validates_pages_wapp_package() {
    let dir = TempDir::new().unwrap();
    // Folder name must equal manifest.id, so install into a named subdir.
    let pack = dir.path().join("acme.bill-audit");
    write_page_wapp(&pack);

    let manifest = validate_wapp_dir(&pack).unwrap();
    assert_eq!(manifest.id, "acme.bill-audit");
    assert_eq!(manifest.api_version, WAPP_API_VERSION);
    assert_eq!(
        manifest.permissions,
        vec![Permission::ContextConnections, Permission::StorageLocal]
    );
}

#[test]
fn validates_pure_theme_wapp_without_entry_or_permissions() {
    let dir = TempDir::new().unwrap();
    let pack = dir.path().join("acme.midnight");
    write_theme_wapp(&pack);

    let manifest = validate_wapp_dir(&pack).unwrap();
    assert!(manifest.entry.is_none());
    assert!(manifest.permissions.is_empty());
    assert_eq!(manifest.contributes.themes.len(), 1);
}

#[test]
fn rejects_id_without_publisher() {
    let dir = TempDir::new().unwrap();
    write_page_wapp(dir.path());
    let mut m = parsed(PAGE_MANIFEST);
    m.id = "bill-audit".into();
    let err = validate_manifest(&m, dir.path()).unwrap_err();
    assert!(err.contains("invalid wapp id"), "unexpected: {err}");
}

#[test]
fn rejects_uppercase_id() {
    assert!(!is_valid_wapp_id("Acme.bill-audit"));
    assert!(!is_valid_wapp_id("acme.Bill-Audit"));
}

#[test]
fn rejects_overlong_id_segments() {
    // publisher segment: leading char + up to 30 more (31 total)
    assert!(is_valid_wapp_id(&format!("{}x.bill", "a".repeat(30))));
    assert!(!is_valid_wapp_id(&format!("{}x.bill", "a".repeat(31))));
    // name segment: leading char + up to 31 more (32 total)
    assert!(is_valid_wapp_id(&format!("acme.{}", "b".repeat(31))));
    assert!(!is_valid_wapp_id(&format!("acme.{}", "b".repeat(33))));
    assert!(is_valid_wapp_id("acme.bill-audit"));
}

#[test]
fn rejects_api_version_mismatch() {
    let dir = TempDir::new().unwrap();
    write_page_wapp(dir.path());
    let json = PAGE_MANIFEST.replace("\"apiVersion\": 2", "\"apiVersion\": 3");
    let m = parsed(&json);
    let err = validate_manifest(&m, dir.path()).unwrap_err();
    assert!(err.contains("apiVersion"), "unexpected: {err}");
    assert!(err.contains("需要更新版本的 DataZen"), "unexpected: {err}");
}

#[test]
fn rejects_backend_not_null() {
    let dir = TempDir::new().unwrap();
    write_page_wapp(dir.path());
    let m = parsed(
        &r#"{
      "id": "acme.bill-audit",
      "name": "Bill Audit",
      "version": "1.0.0",
      "apiVersion": 2,
      "entry": "index.html",
      "contributes": { "pages": [{ "id": "quota-check", "title": "Q" }] },
      "backend": { "kind": "sidecar", "entry": "plugin.wasm" }
    }"#,
    );
    let err = validate_manifest(&m, dir.path()).unwrap_err();
    assert!(err.contains("backend"), "unexpected: {err}");
    assert!(err.contains("需要更新版本的 DataZen"), "unexpected: {err}");
}

#[test]
fn rejects_unknown_permission_string() {
    let json = PAGE_MANIFEST.replace(
        r#""permissions": ["context:connections", "storage:local"]"#,
        r#""permissions": ["storage:local", "fs:write-anything"]"#,
    );
    let err = parse_manifest(&json).unwrap_err();
    assert!(err.contains("unknown variant"), "unexpected: {err}");
}

#[test]
fn rejects_unknown_top_level_field() {
    let json = PAGE_MANIFEST.replace(
        "\"entry\": \"index.html\",",
        "\"entry\": \"index.html\", \"appIcon\": \"x.png\",",
    );
    let err = parse_manifest(&json).unwrap_err();
    assert!(err.contains("appIcon"), "unexpected: {err}");
}

#[test]
fn rejects_unknown_contribution_field() {
    let json = PAGE_MANIFEST.replace(
        "\"showIn\": \"workspace\" }]",
        "\"showIn\": \"workspace\", \"shell\": true }]",
    );
    assert!(parse_manifest(&json).is_err());
}

#[test]
fn rejects_entry_path_traversal() {
    let dir = TempDir::new().unwrap();
    write_page_wapp(dir.path());
    let mut m = parsed(PAGE_MANIFEST);
    m.entry = Some("../evil.html".into());
    let err = validate_manifest(&m, dir.path()).unwrap_err();
    assert!(
        err.contains("unsafe declared path") || err.contains("escapes"),
        "unexpected: {err}"
    );
}

#[test]
fn rejects_hidden_component_in_declared_path() {
    let dir = TempDir::new().unwrap();
    write_page_wapp(dir.path());
    write_file(dir.path(), ".storage.json", "{}");
    let mut m = parsed(PAGE_MANIFEST);
    m.entry = Some(".storage.json".into());
    let err = validate_manifest(&m, dir.path()).unwrap_err();
    assert!(err.contains("hidden path component"), "unexpected: {err}");
}

#[test]
fn rejects_pages_without_entry() {
    let dir = TempDir::new().unwrap();
    write_file(dir.path(), "index.html", "<html></html>");
    let json = PAGE_MANIFEST
        .replace("\"entry\": \"index.html\",", "")
        .replace("\"icon\": \"assets/icon.svg\", ", "");
    let m = parsed(&json);
    // icon file missing would fail first only if entry check ran; entry is absent here.
    let err = validate_manifest(&m, dir.path()).unwrap_err();
    assert!(err.contains("entry"), "unexpected: {err}");
}

#[test]
fn rejects_missing_entry_file() {
    let dir = TempDir::new().unwrap();
    write_file(dir.path(), "manifest.json", PAGE_MANIFEST);
    write_file(dir.path(), "assets/icon.svg", "<svg/>");
    let m = parsed(PAGE_MANIFEST);
    let err = validate_manifest(&m, dir.path()).unwrap_err();
    assert!(err.contains("entry file not found"), "unexpected: {err}");
}

#[test]
fn rejects_folder_name_mismatch() {
    let dir = TempDir::new().unwrap();
    let pack = dir.path().join("wrong-folder");
    fs::create_dir_all(&pack).unwrap();
    write_page_wapp(&pack);
    let err = validate_wapp_dir(&pack).unwrap_err();
    assert!(
        err.contains("does not match manifest id"),
        "unexpected: {err}"
    );
}

#[test]
fn rejects_disallowed_declared_extension() {
    let dir = TempDir::new().unwrap();
    write_page_wapp(dir.path());
    let mut m = parsed(PAGE_MANIFEST);
    if let Some(page) = m.contributes.pages.first_mut() {
        page.icon = Some("assets/icon.gif".into());
    }
    let err = validate_manifest(&m, dir.path()).unwrap_err();
    assert!(
        err.contains("forbidden extension .gif"),
        "unexpected: {err}"
    );
}

#[test]
fn rejects_invalid_page_id() {
    for bad in ["Quota Check", "quota/check", "", "a".repeat(65).as_str()] {
        let mut m = parsed(PAGE_MANIFEST);
        m.contributes.pages[0].id = bad.to_string();
        let dir = TempDir::new().unwrap();
        write_page_wapp(dir.path());
        let err = validate_manifest(&m, dir.path()).unwrap_err();
        assert!(err.contains("invalid page id"), "id `{bad}`: {err}");
    }
}

/// PAGE_MANIFEST plus a valid top-level package icon.
fn icon_manifest(icon: &str) -> String {
    PAGE_MANIFEST.replacen("\"entry\"", &format!("\"icon\": \"{icon}\", \"entry\""), 1)
}

#[test]
fn accepts_valid_package_icon() {
    let dir = TempDir::new().unwrap();
    write_file(
        dir.path(),
        "manifest.json",
        &icon_manifest("assets/icon.png"),
    );
    write_file(dir.path(), "index.html", "<html/>");
    write_file(dir.path(), "assets/icon.svg", "<svg/>");
    write_file(dir.path(), "assets/icon.png", "fake-png");
    let m = parsed(&icon_manifest("assets/icon.png"));
    validate_manifest(&m, dir.path()).unwrap();
}

#[test]
fn accepts_wapp_without_package_icon() {
    let dir = TempDir::new().unwrap();
    write_page_wapp(dir.path());
    validate_manifest(&parsed(PAGE_MANIFEST), dir.path()).unwrap();
}

#[test]
fn rejects_missing_package_icon_file() {
    let dir = TempDir::new().unwrap();
    write_file(
        dir.path(),
        "manifest.json",
        &icon_manifest("assets/theme.png"),
    );
    write_file(dir.path(), "index.html", "<html/>");
    write_file(dir.path(), "assets/icon.svg", "<svg/>");
    let m = parsed(&icon_manifest("assets/theme.png"));
    let err = validate_manifest(&m, dir.path()).unwrap_err();
    assert!(err.contains("wapp icon not found"), "unexpected: {err}");
}

#[test]
fn rejects_disallowed_package_icon_extension() {
    let dir = TempDir::new().unwrap();
    write_file(
        dir.path(),
        "manifest.json",
        &icon_manifest("assets/icon.ico"),
    );
    write_file(dir.path(), "index.html", "<html/>");
    write_file(dir.path(), "assets/icon.svg", "<svg/>");
    write_file(dir.path(), "assets/icon.ico", "fake");
    let m = parsed(&icon_manifest("assets/icon.ico"));
    let err = validate_manifest(&m, dir.path()).unwrap_err();
    assert!(err.contains("forbidden extension"), "unexpected: {err}");
}

#[test]
fn rejects_unsupported_show_in_value() {
    let dir = TempDir::new().unwrap();
    write_page_wapp(dir.path());
    let mut m = parsed(PAGE_MANIFEST);
    m.contributes.pages[0].show_in = "sidebar".into();
    let err = validate_manifest(&m, dir.path()).unwrap_err();
    assert!(err.contains("unsupported showIn"), "unexpected: {err}");
}

#[test]
fn show_in_defaults_to_workspace() {
    let json = PAGE_MANIFEST.replace(", \"showIn\": \"workspace\"", "");
    let m = parsed(&json);
    assert_eq!(m.contributes.pages[0].show_in, "workspace");
}

#[test]
fn rejects_theme_modes_problems() {
    let base = THEME_MANIFEST.replace("\"modes\": [\"dark\"]", "\"modes\": []");
    let dir = TempDir::new().unwrap();
    write_theme_wapp(dir.path());
    let err = validate_manifest(&parsed(&base), dir.path()).unwrap_err();
    assert!(err.contains("must not be empty"), "unexpected: {err}");

    let sepia = THEME_MANIFEST.replace("\"modes\": [\"dark\"]", "\"modes\": [\"sepia\"]");
    let err = validate_manifest(&parsed(&sepia), dir.path()).unwrap_err();
    assert!(err.contains("invalid theme mode"), "unexpected: {err}");
}

#[test]
fn accepts_optional_editor_charts_icons_parity_assets() {
    let json = THEME_MANIFEST.replace(
        "\"previewImage\": \"themes/midnight-blue/preview.png\"",
        "\"editorJson\": \"themes/midnight-blue/editor.json\", \
         \"chartsJson\": \"themes/midnight-blue/charts.json\", \
         \"iconsDir\": \"themes/midnight-blue/icons\"",
    );
    let mut m = parsed(&json);
    if let Some(theme) = m.contributes.themes.first_mut() {
        theme.editor_json = Some("themes/midnight-blue/editor.json".into());
        theme.charts_json = Some("themes/midnight-blue/charts.json".into());
        theme.icons_dir = Some("themes/midnight-blue/icons".into());
    }
    let dir = TempDir::new().unwrap();
    write_theme_wapp(dir.path());
    write_file(dir.path(), "themes/midnight-blue/editor.json", "{}");
    write_file(
        dir.path(),
        "themes/midnight-blue/charts.json",
        "{\"series\":[\"#fff\"]}",
    );
    write_file(
        dir.path(),
        "themes/midnight-blue/icons/nav.settings.svg",
        "<svg/>",
    );
    assert!(
        validate_manifest(&m, dir.path()).is_ok(),
        "declared parity assets should validate"
    );
}

#[test]
fn rejects_missing_declared_parity_assets() {
    for field in ["editorJson", "chartsJson", "iconsDir"] {
        let declared = format!("themes/midnight-blue/{field}.json");
        let json = THEME_MANIFEST.replace(
            "\"previewImage\": \"themes/midnight-blue/preview.png\"",
            &format!(
                "\"previewImage\": \"themes/midnight-blue/preview.png\", \"{field}\": \"{declared}\""
            ),
        );
        let mut m = parsed(&json);
        if let Some(theme) = m.contributes.themes.first_mut() {
            match field {
                "editorJson" => theme.editor_json = Some(declared.clone()),
                "chartsJson" => theme.charts_json = Some(declared.clone()),
                _ => theme.icons_dir = Some(declared.clone()),
            }
        }
        let dir = TempDir::new().unwrap();
        write_theme_wapp(dir.path());
        // Nothing written at the declared paths → all three must fail.
        let err = validate_manifest(&m, dir.path()).unwrap_err();
        assert!(
            err.contains("not found"),
            "{field} missing should be rejected: {err}"
        );
    }
}

#[test]
fn rejects_traversal_in_declared_parity_assets() {
    let json = THEME_MANIFEST.replace(
        "\"previewImage\": \"themes/midnight-blue/preview.png\"",
        "\"previewImage\": \"themes/midnight-blue/preview.png\", \"chartsJson\": \"../evil.json\"",
    );
    let mut m = parsed(&json);
    if let Some(theme) = m.contributes.themes.first_mut() {
        theme.charts_json = Some("../evil.json".into());
    }
    let dir = TempDir::new().unwrap();
    write_theme_wapp(dir.path());
    // Rejected by the escape check alone — no target file needs to exist.
    let err = validate_manifest(&m, dir.path()).unwrap_err();
    assert!(
        err.contains("unsafe declared path") || err.contains("escapes"),
        "unexpected: {err}"
    );
}

#[test]
fn rejects_non_semver_version() {
    let dir = TempDir::new().unwrap();
    write_page_wapp(dir.path());
    for bad in ["1.0", "v1.0.0", "not-a-version", "1.0.0.1"] {
        let mut m = parsed(PAGE_MANIFEST);
        m.version = bad.to_string();
        let err = validate_manifest(&m, dir.path()).unwrap_err();
        assert!(err.contains("invalid semantic version"), "`{bad}`: {err}");
    }
    for good in ["1.0.0", "0.2.1-beta.1", "10.20.30+build.7"] {
        let mut m = parsed(PAGE_MANIFEST);
        m.version = good.to_string();
        let dir_ok = TempDir::new().unwrap();
        write_page_wapp(dir_ok.path());
        assert!(
            validate_manifest(&m, dir_ok.path()).is_ok(),
            "semver `{good}` should pass"
        );
    }
}

#[test]
fn rejects_forbidden_extension_in_package_scan() {
    let dir = TempDir::new().unwrap();
    write_page_wapp(dir.path());
    write_file(dir.path(), "assets/evil.sh", "#!/bin/sh");
    let m = parsed(PAGE_MANIFEST);
    let err = validate_manifest(&m, dir.path()).unwrap_err();
    assert!(err.contains("forbidden extension .sh"), "unexpected: {err}");
}

#[test]
fn scan_enforces_small_limits() {
    let dir = TempDir::new().unwrap();
    write_page_wapp(dir.path());

    let tiny_files = PackageLimits {
        max_total_bytes: MAX_WAPP_UNCOMPRESSED,
        max_files: 2,
    };
    let err = scan_package_files(dir.path(), tiny_files).unwrap_err();
    assert!(err.contains("too many files"), "unexpected: {err}");

    let tiny_bytes = PackageLimits {
        max_total_bytes: 8,
        max_files: MAX_WAPP_FILES,
    };
    let err = scan_package_files(dir.path(), tiny_bytes).unwrap_err();
    assert!(err.contains("size exceeds limit"), "unexpected: {err}");
}

#[test]
fn scan_skips_host_marker_files() {
    let dir = TempDir::new().unwrap();
    write_theme_wapp(dir.path());
    write_file(dir.path(), ".enabled", "1\n");
    write_file(dir.path(), ".storage.json", "{\"k\":1}");
    assert!(scan_package_files(dir.path(), PackageLimits::default()).is_ok());
}

#[test]
fn scan_rejects_symlink_inside_package() {
    let dir = TempDir::new().unwrap();
    write_page_wapp(dir.path());
    let target = dir.path().join("index.html");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&target, dir.path().join("link.html")).unwrap();
        let err = scan_package_files(dir.path(), PackageLimits::default()).unwrap_err();
        assert!(err.contains("symlink not allowed"), "unexpected: {err}");
    }
}

#[test]
fn scan_rejects_malicious_svg() {
    let dir = TempDir::new().unwrap();
    write_page_wapp(dir.path());
    write_file(
        dir.path(),
        "assets/icon.svg",
        "<svg xmlns='http://www.w3.org/2000/svg' onload='evil()'/>",
    );
    let err = scan_package_files(dir.path(), PackageLimits::default()).unwrap_err();
    assert!(err.contains("event handler"), "unexpected: {err}");
}

#[test]
fn safe_declared_path_rejects_absolute_and_escape() {
    let dir = TempDir::new().unwrap();
    assert!(safe_declared_path(dir.path(), "/etc/passwd").is_err());
    assert!(safe_declared_path(dir.path(), "../../etc/passwd").is_err());
    assert!(safe_declared_path(dir.path(), "assets/../..").is_err());
    assert!(safe_declared_path(dir.path(), "").is_err());
    assert!(safe_declared_path(dir.path(), "assets/icon.svg").is_ok());
}
