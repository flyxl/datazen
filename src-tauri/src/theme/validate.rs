//! Theme pack directory and content validation.

use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;
use serde::Deserialize;
use std::sync::LazyLock;

pub const THEME_API_VERSION: u32 = 1;
pub const MAX_THEME_UNCOMPRESSED: u64 = 16 * 1024 * 1024;
pub const MAX_THEME_FILES: usize = 500;
pub const MAX_THEME_FONT_BYTES: u64 = 4 * 1024 * 1024;
const MAX_SVG_BYTES: usize = 256 * 1024;

const FORBIDDEN_MANIFEST_KEYS: &[&str] = &["appIcon", "trayIcon", "bundleIcon"];

static THEME_ID_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-z0-9]+([.-][a-z0-9]+)*$").expect("valid theme id regex"));

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub api_version: u32,
    pub modes: Vec<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

pub fn allowed_theme_extension(ext: &str) -> bool {
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "css" | "svg" | "png" | "webp" | "json" | "woff2" | "woff"
    )
}

/// Validate a zip entry path; reject absolute paths, `..`, and symlink-like names.
pub fn validate_theme_zip_path(name: &str) -> Result<(), String> {
    if name.is_empty() || name.contains('\0') {
        return Err("invalid zip entry name".into());
    }
    if name.contains(" -> ") {
        return Err("symlink entry name not allowed".into());
    }

    crate::app_data_archive::validate_zip_entry_path(name).map(|_| ()).map_err(|e| e.to_string())
}

/// Validate pack contents (manifest, tokens, file whitelist) without checking folder name.
pub fn validate_pack_contents(dir: &Path) -> Result<ThemeManifest, String> {
    let manifest_path = dir.join("manifest.json");
    let tokens_path = dir.join("tokens.css");

    if !manifest_path.is_file() {
        return Err("missing manifest.json".into());
    }
    if !tokens_path.is_file() {
        return Err("missing tokens.css".into());
    }

    let manifest_content =
        fs::read_to_string(&manifest_path).map_err(|e| format!("read manifest.json: {e}"))?;
    let manifest = parse_manifest(&manifest_content)?;

    let mut file_count = 0usize;
    let mut total_bytes = 0u64;
    let mut font_bytes = 0u64;
    scan_pack_files(dir, dir, &mut file_count, &mut total_bytes, &mut font_bytes)?;

    Ok(manifest)
}

/// Validate an installed pack directory; folder name must equal `manifest.id`.
pub fn validate_pack_dir(dir: &Path) -> Result<ThemeManifest, String> {
    let manifest = validate_pack_contents(dir)?;

    let folder_name = dir
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "invalid pack directory path".to_string())?;

    if folder_name != manifest.id {
        return Err(format!(
            "pack directory name `{folder_name}` does not match manifest id `{}`",
            manifest.id
        ));
    }

    Ok(manifest)
}

fn parse_manifest(content: &str) -> Result<ThemeManifest, String> {
    let value: serde_json::Value =
        serde_json::from_str(content).map_err(|e| format!("invalid manifest.json: {e}"))?;

    if let Some(obj) = value.as_object() {
        for key in FORBIDDEN_MANIFEST_KEYS {
            if obj.contains_key(*key) {
                return Err(format!("forbidden manifest key: {key}"));
            }
        }
    }

    let manifest: ThemeManifest =
        serde_json::from_value(value).map_err(|e| format!("invalid manifest.json: {e}"))?;

    if !THEME_ID_RE.is_match(&manifest.id) {
        return Err(format!("invalid theme id: {}", manifest.id));
    }
    if manifest.api_version != THEME_API_VERSION {
        return Err(format!(
            "unsupported apiVersion: {} (expected {THEME_API_VERSION})",
            manifest.api_version
        ));
    }
    if manifest.modes.is_empty() {
        return Err("modes must not be empty".into());
    }
    for mode in &manifest.modes {
        if mode != "light" && mode != "dark" {
            return Err(format!("invalid mode: {mode}"));
        }
    }

    Ok(manifest)
}

fn scan_pack_files(
    dir: &Path,
    root: &Path,
    file_count: &mut usize,
    total_bytes: &mut u64,
    font_bytes: &mut u64,
) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|e| format!("read dir {}: {e}", dir.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let meta = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;

        if meta.file_type().is_symlink() {
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            return Err(format!("symlink not allowed: {rel}"));
        }

        if path.is_dir() {
            scan_pack_files(&path, root, file_count, total_bytes, font_bytes)?;
            continue;
        }

        *file_count += 1;
        if *file_count > MAX_THEME_FILES {
            return Err(format!("too many files (max {MAX_THEME_FILES})"));
        }

        let size = meta.len();
        *total_bytes = total_bytes
            .checked_add(size)
            .ok_or_else(|| "total size overflow".to_string())?;
        if *total_bytes > MAX_THEME_UNCOMPRESSED {
            return Err(format!(
                "total size exceeds limit ({MAX_THEME_UNCOMPRESSED} bytes)"
            ));
        }

        let rel = path
            .strip_prefix(root)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();

        if ext.is_empty() {
            return Err(format!("file without extension: {rel}"));
        }
        if !allowed_theme_extension(&ext) {
            return Err(format!("forbidden extension .{ext}: {rel}"));
        }

        if ext == "svg" {
            if size as usize > MAX_SVG_BYTES {
                return Err(format!("svg too large: {rel}"));
            }
            let content = fs::read_to_string(&path).map_err(|e| format!("read {rel}: {e}"))?;
            validate_svg_content(&content)?;
        }

        if ext == "woff" || ext == "woff2" {
            *font_bytes = font_bytes
                .checked_add(size)
                .ok_or_else(|| "font size overflow".to_string())?;
            if *font_bytes > MAX_THEME_FONT_BYTES {
                return Err(format!(
                    "font size exceeds limit ({MAX_THEME_FONT_BYTES} bytes)"
                ));
            }
        }
    }

    Ok(())
}

fn validate_svg_content(content: &str) -> Result<(), String> {
    let lower = content.to_ascii_lowercase();
    if lower.contains("<script") {
        return Err("svg contains forbidden <script".into());
    }
    if lower.contains("javascript:") {
        return Err("svg contains javascript: URL".into());
    }
    if lower.contains("onload=") || lower.contains("onerror=") {
        return Err("svg contains forbidden event handler".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_file(dir: &Path, rel: &str, content: &str) {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    const MINIMAL_MANIFEST: &str = r#"{
  "id": "test.theme",
  "name": "Test Theme",
  "version": "1.0.0",
  "apiVersion": 1,
  "modes": ["dark"]
}"#;

    #[test]
    fn rejects_js_extension() {
        assert!(!allowed_theme_extension("js"));
        assert!(!allowed_theme_extension("wasm"));
        assert!(!allowed_theme_extension("ico"));
        assert!(allowed_theme_extension("svg"));
        assert!(allowed_theme_extension("woff2"));
    }

    #[test]
    fn rejects_path_traversal_entry() {
        assert!(validate_theme_zip_path("../evil.css").is_err());
        assert!(validate_theme_zip_path("icons/../../x.css").is_err());
        assert!(validate_theme_zip_path("tokens.css").is_ok());
    }

    #[test]
    fn validate_pack_contents_requires_manifest_and_tokens() {
        let dir = TempDir::new().unwrap();
        assert!(validate_pack_contents(dir.path()).is_err());

        write_file(dir.path(), "manifest.json", MINIMAL_MANIFEST);
        assert!(validate_pack_contents(dir.path()).is_err());

        write_file(dir.path(), "tokens.css", ":root { --c-accent: red; }");
        let manifest = validate_pack_contents(dir.path()).unwrap();
        assert_eq!(manifest.id, "test.theme");
        assert_eq!(manifest.api_version, THEME_API_VERSION);
    }

    #[test]
    fn validate_pack_dir_rejects_folder_name_mismatch() {
        let dir = TempDir::new().unwrap();
        write_file(dir.path(), "manifest.json", MINIMAL_MANIFEST);
        write_file(dir.path(), "tokens.css", ":root { --c-accent: red; }");
        let err = validate_pack_dir(dir.path()).unwrap_err();
        assert!(
            err.contains("does not match manifest id"),
            "unexpected: {err}"
        );
    }

    #[test]
    fn rejects_forbidden_manifest_keys() {
        let dir = TempDir::new().unwrap();
        write_file(
            dir.path(),
            "manifest.json",
            r#"{
  "id": "test.theme",
  "name": "Test",
  "version": "1.0.0",
  "apiVersion": 1,
  "modes": ["dark"],
  "appIcon": "evil.png"
}"#,
        );
        write_file(dir.path(), "tokens.css", ":root {}");
        let err = validate_pack_contents(dir.path()).unwrap_err();
        assert!(err.contains("appIcon"), "unexpected: {err}");
    }

    #[test]
    fn rejects_invalid_manifest_id() {
        let dir = TempDir::new().unwrap();
        write_file(
            dir.path(),
            "manifest.json",
            r#"{
  "id": "INVALID_ID",
  "name": "Test",
  "version": "1.0.0",
  "apiVersion": 1,
  "modes": ["dark"]
}"#,
        );
        write_file(dir.path(), "tokens.css", ":root {}");
        assert!(validate_pack_contents(dir.path()).is_err());
    }

    #[test]
    fn rejects_malicious_svg() {
        let dir = TempDir::new().unwrap();
        write_file(dir.path(), "manifest.json", MINIMAL_MANIFEST);
        write_file(dir.path(), "tokens.css", ":root {}");
        write_file(
            dir.path(),
            "icons/evil.svg",
            r#"<svg><script>alert(1)</script></svg>"#,
        );
        assert!(validate_pack_contents(dir.path()).is_err());
    }

    #[test]
    fn validates_community_fixture_pack() {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../fixtures/themes/community.fixture-dark");
        let manifest = validate_pack_dir(&fixture).unwrap();
        assert_eq!(manifest.id, "community.fixture-dark");
        assert!(manifest.modes.contains(&"dark".to_string()));
    }

    #[test]
    fn rejects_unsupported_api_version() {
        let dir = TempDir::new().unwrap();
        write_file(
            dir.path(),
            "manifest.json",
            r#"{
  "id": "test.theme",
  "name": "Test",
  "version": "1.0.0",
  "apiVersion": 99,
  "modes": ["dark"]
}"#,
        );
        write_file(dir.path(), "tokens.css", ":root {}");
        let err = validate_pack_contents(dir.path()).unwrap_err();
        assert!(err.contains("unsupported apiVersion"), "unexpected: {err}");
    }

    #[test]
    fn rejects_empty_modes() {
        let dir = TempDir::new().unwrap();
        write_file(
            dir.path(),
            "manifest.json",
            r#"{
  "id": "test.theme",
  "name": "Test",
  "version": "1.0.0",
  "apiVersion": 1,
  "modes": []
}"#,
        );
        write_file(dir.path(), "tokens.css", ":root {}");
        let err = validate_pack_contents(dir.path()).unwrap_err();
        assert!(err.contains("modes must not be empty"), "unexpected: {err}");
    }

    #[test]
    fn rejects_invalid_mode_value() {
        let dir = TempDir::new().unwrap();
        write_file(
            dir.path(),
            "manifest.json",
            r#"{
  "id": "test.theme",
  "name": "Test",
  "version": "1.0.0",
  "apiVersion": 1,
  "modes": ["sepia"]
}"#,
        );
        write_file(dir.path(), "tokens.css", ":root {}");
        let err = validate_pack_contents(dir.path()).unwrap_err();
        assert!(err.contains("invalid mode"), "unexpected: {err}");
    }

    #[test]
    fn rejects_file_without_extension() {
        let dir = TempDir::new().unwrap();
        write_file(dir.path(), "manifest.json", MINIMAL_MANIFEST);
        write_file(dir.path(), "tokens.css", ":root {}");
        write_file(dir.path(), "noext", "bad");
        let err = validate_pack_contents(dir.path()).unwrap_err();
        assert!(err.contains("without extension"), "unexpected: {err}");
    }

    #[test]
    fn rejects_svg_javascript_url_and_event_handlers() {
        let dir = TempDir::new().unwrap();
        write_file(dir.path(), "manifest.json", MINIMAL_MANIFEST);
        write_file(dir.path(), "tokens.css", ":root {}");

        write_file(
            dir.path(),
            "js.svg",
            r#"<svg><a href="javascript:alert(1)"/></svg>"#,
        );
        assert!(
            validate_pack_contents(dir.path())
                .unwrap_err()
                .contains("javascript:"),
            "expected javascript: rejection"
        );

        let dir2 = TempDir::new().unwrap();
        write_file(dir2.path(), "manifest.json", MINIMAL_MANIFEST);
        write_file(dir2.path(), "tokens.css", ":root {}");
        write_file(dir2.path(), "onload.svg", r#"<svg onload="x()"></svg>"#);
        let err = validate_pack_contents(dir2.path()).unwrap_err();
        assert!(err.contains("event handler"), "unexpected: {err}");
    }

    #[test]
    fn rejects_symlink_in_pack() {
        let dir = TempDir::new().unwrap();
        write_file(dir.path(), "manifest.json", MINIMAL_MANIFEST);
        write_file(dir.path(), "tokens.css", ":root {}");
        let target = dir.path().join("real.css");
        fs::write(&target, ":root {}").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            symlink(&target, dir.path().join("link.css")).unwrap();
            let err = validate_pack_contents(dir.path()).unwrap_err();
            assert!(err.contains("symlink not allowed"), "unexpected: {err}");
        }
    }

    #[test]
    fn validate_theme_zip_path_rejects_empty_and_null() {
        assert!(validate_theme_zip_path("").is_err());
        assert!(validate_theme_zip_path("icons\0evil.css").is_err());
        assert!(validate_theme_zip_path("a -> b").is_err());
    }
}
