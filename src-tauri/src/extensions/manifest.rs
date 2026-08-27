//! Runtime extension manifest: schema types and validation rules.
//!
//! Mirrors [`crate::theme::validate`] conventions: strict serde parsing,
//! `<publisher>.<name>` id format, semver versions, path-traversal
//! protection, and package size/file-count quotas.

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use super::EXTENSION_API_VERSION;

/// Total uncompressed package size limit (rule 6).
pub const MAX_EXTENSION_UNCOMPRESSED: u64 = 50 * 1024 * 1024;
/// Maximum number of files in a package (rule 6).
pub const MAX_EXTENSION_FILES: usize = 2000;

const MAX_SVG_BYTES: usize = 256 * 1024;

static EXTENSION_ID_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[a-z0-9][a-z0-9-]{0,30}\.[a-z][a-z0-9-]{1,31}$")
        .expect("valid extension id regex")
});

static SEMVER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)",
        r"(?:-((?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*)",
        r"(?:\.(?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*))*))?",
        r"(?:\+([0-9a-zA-Z-]+(?:\.[0-9a-zA-Z-]+)*))?$"
    ))
    .expect("valid semver regex")
});

static PAGE_ID_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-z0-9-_]{1,64}$").expect("valid page id regex"));

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtensionManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub api_version: u32,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// Optional package-level icon (square brand image) shown in plugin lists.
    /// Rule-5 validated path; `png|webp|svg` allowed.
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub entry: Option<String>,
    pub contributes: Contributions,
    #[serde(default)]
    pub permissions: Vec<Permission>,
    /// Reserved for P2 backend plugins; must be null/absent in v1.
    #[serde(default)]
    pub backend: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Contributions {
    #[serde(default)]
    pub pages: Vec<PageContribution>,
    #[serde(default)]
    pub themes: Vec<ThemeContribution>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PageContribution {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default = "default_show_in")]
    pub show_in: String,
}

fn default_show_in() -> String {
    "workspace".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemeContribution {
    pub id: String,
    pub name: String,
    pub tokens_css: String,
    pub modes: Vec<String>,
    #[serde(default)]
    pub preview_image: Option<String>,
    /// Optional CodeMirror color overlay (legacy ThemePack `editor.json`),
    /// applied by the host after tokens.css. Rule-5 validated path.
    #[serde(default)]
    pub editor_json: Option<String>,
    /// Optional chart series palette (legacy `charts.json`). Rule-5 path.
    #[serde(default)]
    pub charts_json: Option<String>,
    /// Optional directory of semantic icon overrides named
    /// `<semanticId>.svg|.webp|.png` (legacy `icons/`). Rule-5 path; must be
    /// an existing directory. Contents are still subject to the package-wide
    /// file scan (rule 6).
    #[serde(default)]
    pub icons_dir: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Permission {
    #[serde(rename = "context:connections")]
    ContextConnections,
    #[serde(rename = "command:invoke")]
    CommandInvoke,
    #[serde(rename = "storage:local")]
    StorageLocal,
    #[serde(rename = "ui:notify")]
    UiNotify,
}

impl Permission {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ContextConnections => "context:connections",
            Self::CommandInvoke => "command:invoke",
            Self::StorageLocal => "storage:local",
            Self::UiNotify => "ui:notify",
        }
    }
}

impl std::fmt::Display for Permission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Whether `id` matches the `<publisher>.<name>` format enforced by rule 1.
pub fn is_valid_extension_id(id: &str) -> bool {
    EXTENSION_ID_RE.is_match(id)
}

/// Whether `ext` is in the package file-type whitelist.
pub fn allowed_extension_file_ext(ext: &str) -> bool {
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "html" | "js" | "mjs" | "css" | "json" | "svg" | "png" | "webp" | "woff2" | "woff"
    )
}

/// Parse a manifest.json payload; unknown fields are rejected.
pub fn parse_manifest(content: &str) -> Result<ExtensionManifest, String> {
    serde_json::from_str(content).map_err(|e| format!("invalid manifest.json: {e}"))
}

/// Read and parse `{dir}/manifest.json`.
fn read_manifest_from_dir(dir: &Path) -> Result<ExtensionManifest, String> {
    let manifest_path = dir.join("manifest.json");
    if !manifest_path.is_file() {
        return Err("missing manifest.json".into());
    }
    let content =
        fs::read_to_string(&manifest_path).map_err(|e| format!("read manifest.json: {e}"))?;
    parse_manifest(&content)
}

/// Validate an installed extension directory; folder name must equal `manifest.id`.
pub fn validate_extension_dir(dir: &Path) -> Result<ExtensionManifest, String> {
    let manifest = read_manifest_from_dir(dir)?;

    let folder_name = dir
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "invalid extension directory path".to_string())?;
    if folder_name != manifest.id {
        return Err(format!(
            "extension directory name `{folder_name}` does not match manifest id `{}`",
            manifest.id
        ));
    }

    validate_manifest(&manifest, dir)?;
    Ok(manifest)
}

/// Validate a manifest against its on-disk package directory (rules 1–7).
pub fn validate_manifest(manifest: &ExtensionManifest, plugin_dir: &Path) -> Result<(), String> {
    // Rule 1: `<publisher>.<name>` id format.
    if !EXTENSION_ID_RE.is_match(&manifest.id) {
        return Err(format!("invalid extension id: {}", manifest.id));
    }

    // Rule 2: API version handshake; mismatch means a newer host is required.
    if manifest.api_version != EXTENSION_API_VERSION {
        return Err(format!(
            "unsupported apiVersion: {} (expected {EXTENSION_API_VERSION}); \
             需要更新版本的 DataZen >= {}",
            manifest.api_version,
            env!("CARGO_PKG_VERSION")
        ));
    }

    // Rule 3a: semver version.
    if !SEMVER_RE.is_match(&manifest.version) {
        return Err(format!("invalid semantic version: {}", manifest.version));
    }

    // Rule 3b: backend plugins are reserved for P2.
    if manifest.backend.is_some() {
        return Err(
            "manifest.backend is not supported by this host; 需要更新版本的 DataZen".into(),
        );
    }

    // Rule 4: page ids / showIn sanity; pages require an entry that exists.
    for page in &manifest.contributes.pages {
        if !PAGE_ID_RE.is_match(&page.id) {
            return Err(format!("invalid page id: {}", page.id));
        }
        if page.show_in != "workspace" {
            return Err(format!("unsupported showIn value: {}", page.show_in));
        }
    }
    if !manifest.contributes.pages.is_empty() && manifest.entry.is_none() {
        return Err("pages contribution requires an `entry` field".into());
    }
    if let Some(entry) = &manifest.entry {
        let entry_path = safe_declared_path(plugin_dir, entry)?;
        ensure_allowed_extension(entry)?;
        if !entry_path.is_file() {
            return Err(format!("entry file not found: {entry}"));
        }
    }

    // Rule 5: package-level icon (when declared) must be a safe, whitelisted,
    // present image file.
    if let Some(icon) = &manifest.icon {
        let icon_path = safe_declared_path(plugin_dir, icon)?;
        ensure_allowed_extension(icon)?;
        if !icon_path.is_file() {
            return Err(format!("plugin icon not found: {icon}"));
        }
    }

    // Rules 4/5: contribution-declared paths are safe, whitelisted, and present.
    for page in &manifest.contributes.pages {
        if let Some(icon) = &page.icon {
            let icon_path = safe_declared_path(plugin_dir, icon)?;
            ensure_allowed_extension(icon)?;
            if !icon_path.is_file() {
                return Err(format!("page icon not found: {icon}"));
            }
        }
    }
    for theme in &manifest.contributes.themes {
        if theme.modes.is_empty() {
            return Err(format!("theme `{}` modes must not be empty", theme.id));
        }
        for mode in &theme.modes {
            if mode != "light" && mode != "dark" {
                return Err(format!("invalid theme mode: {mode}"));
            }
        }

        let tokens_path = safe_declared_path(plugin_dir, &theme.tokens_css)?;
        ensure_allowed_extension(&theme.tokens_css)?;
        if !tokens_path.is_file() {
            return Err(format!("theme tokens.css not found: {}", theme.tokens_css));
        }
        if let Some(preview) = &theme.preview_image {
            let preview_path = safe_declared_path(plugin_dir, preview)?;
            ensure_allowed_extension(preview)?;
            if !preview_path.is_file() {
                return Err(format!("theme preview image not found: {preview}"));
            }
        }
        // Optional legacy-parity assets (editor overlay / chart palette /
        // icon overrides). Declared ⇒ must exist; json files are whitelisted
        // by extension, icons_dir must be a real directory (its contents go
        // through the rule-6 package scan).
        for (label, declared) in [
            ("theme editor.json", &theme.editor_json),
            ("theme charts.json", &theme.charts_json),
        ] {
            if let Some(path) = declared {
                let resolved = safe_declared_path(plugin_dir, path)?;
                ensure_allowed_extension(path)?;
                if !resolved.is_file() {
                    return Err(format!("{label} not found: {path}"));
                }
            }
        }
        if let Some(icons_dir) = &theme.icons_dir {
            let resolved = safe_declared_path(plugin_dir, icons_dir)?;
            if !resolved.is_dir() {
                return Err(format!(
                    "theme icons dir not found or not a directory: {icons_dir}"
                ));
            }
        }
    }

    // Rule 6: package-wide whitelist / quota scan.
    scan_package_files(plugin_dir, PackageLimits::default())?;

    // Rule 7: no mandatory permission combination — pure-theme plugins may
    // declare none; undeclared runtime calls are rejected at the bridge.
    Ok(())
}

/// Validate a declared relative path (rule 5): relative, no `..`, no hidden
/// components, resolves inside `plugin_dir`.
pub(crate) fn safe_declared_path(plugin_dir: &Path, declared: &str) -> Result<PathBuf, String> {
    if declared.is_empty() {
        return Err("empty declared path".into());
    }

    let rel = crate::app_data_archive::validate_zip_entry_path(declared)
        .map_err(|e| format!("unsafe declared path `{declared}`: {e}"))?;

    for component in rel.components() {
        if let Component::Normal(name) = component {
            let name = name.to_string_lossy();
            if name.starts_with('.') {
                return Err(format!("hidden path component not allowed: {declared}"));
            }
        }
    }

    let joined = plugin_dir.join(&rel);
    if !joined.starts_with(plugin_dir) {
        return Err(format!(
            "declared path escapes plugin directory: {declared}"
        ));
    }

    if joined.exists() {
        let canonical_dir = fs::canonicalize(plugin_dir).map_err(|e| e.to_string())?;
        let canonical_file = fs::canonicalize(&joined).map_err(|e| e.to_string())?;
        if !canonical_file.starts_with(&canonical_dir) {
            return Err(format!(
                "declared path escapes plugin directory: {declared}"
            ));
        }
    }

    Ok(joined)
}

fn ensure_allowed_extension(declared: &str) -> Result<(), String> {
    let ext = Path::new(declared)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    if ext.is_empty() || !allowed_extension_file_ext(&ext) {
        return Err(format!("forbidden extension .{ext}: {declared}"));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PackageLimits {
    pub max_total_bytes: u64,
    pub max_files: usize,
}

impl Default for PackageLimits {
    fn default() -> Self {
        Self {
            max_total_bytes: MAX_EXTENSION_UNCOMPRESSED,
            max_files: MAX_EXTENSION_FILES,
        }
    }
}

/// Scan every file under `dir` (rule 6): symlink rejection, extension
/// whitelist, size/count quotas. Dot-prefixed entries are skipped because they
/// are host-managed state (`.enabled`, `.storage.json`) rather than package
/// content.
pub(crate) fn scan_package_files(dir: &Path, limits: PackageLimits) -> Result<(), String> {
    scan_dir(dir, dir, &mut PackageStats::default(), limits)
}

#[derive(Default)]
struct PackageStats {
    files: usize,
    total_bytes: u64,
}

fn scan_dir(
    dir: &Path,
    root: &Path,
    stats: &mut PackageStats,
    limits: PackageLimits,
) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|e| format!("read dir {}: {e}", dir.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();

        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            return Err(format!("unreadable file name in {}", dir.display()));
        };
        if name.starts_with('.') {
            continue;
        }

        let meta = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if meta.file_type().is_symlink() {
            let rel = rel_display(&path, root);
            return Err(format!("symlink not allowed: {rel}"));
        }

        if path.is_dir() {
            scan_dir(&path, root, stats, limits)?;
            continue;
        }

        stats.files += 1;
        if stats.files > limits.max_files {
            return Err(format!("too many files (max {})", limits.max_files));
        }

        stats.total_bytes = stats
            .total_bytes
            .checked_add(meta.len())
            .ok_or_else(|| "package size overflow".to_string())?;
        if stats.total_bytes > limits.max_total_bytes {
            return Err(format!(
                "package size exceeds limit ({} bytes)",
                limits.max_total_bytes
            ));
        }

        let rel = rel_display(&path, root);
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();
        if ext.is_empty() {
            return Err(format!("file without extension: {rel}"));
        }
        if !allowed_extension_file_ext(&ext) {
            return Err(format!("forbidden extension .{ext}: {rel}"));
        }

        if ext == "svg" {
            if meta.len() as usize > MAX_SVG_BYTES {
                return Err(format!("svg too large: {rel}"));
            }
            let content = fs::read_to_string(&path).map_err(|e| format!("read {rel}: {e}"))?;
            validate_svg_content(&content)?;
        }
    }

    Ok(())
}

fn rel_display(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
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

    /// Write a complete, valid pages-plugin package into `dir`.
    fn write_page_plugin(dir: &Path) {
        write_file(dir, "manifest.json", PAGE_MANIFEST);
        write_file(dir, "index.html", "<html><body></body></html>");
        write_file(
            dir,
            "assets/icon.svg",
            "<svg xmlns='http://www.w3.org/2000/svg'/>",
        );
    }

    /// Write a complete, valid pure-theme package into `dir`.
    fn write_theme_plugin(dir: &Path) {
        write_file(dir, "manifest.json", THEME_MANIFEST);
        write_file(
            dir,
            "themes/midnight-blue/tokens.css",
            ":root { --c-accent: red; }",
        );
        write_file(dir, "themes/midnight-blue/preview.png", "pretend-png");
    }

    fn parsed(json: &str) -> ExtensionManifest {
        parse_manifest(json).unwrap()
    }

    #[test]
    fn validates_pages_plugin_package() {
        let dir = TempDir::new().unwrap();
        // Folder name must equal manifest.id, so install into a named subdir.
        let pack = dir.path().join("acme.bill-audit");
        write_page_plugin(&pack);

        let manifest = validate_extension_dir(&pack).unwrap();
        assert_eq!(manifest.id, "acme.bill-audit");
        assert_eq!(manifest.api_version, EXTENSION_API_VERSION);
        assert_eq!(
            manifest.permissions,
            vec![Permission::ContextConnections, Permission::StorageLocal]
        );
    }

    #[test]
    fn validates_pure_theme_plugin_without_entry_or_permissions() {
        let dir = TempDir::new().unwrap();
        let pack = dir.path().join("acme.midnight");
        write_theme_plugin(&pack);

        let manifest = validate_extension_dir(&pack).unwrap();
        assert!(manifest.entry.is_none());
        assert!(manifest.permissions.is_empty());
        assert_eq!(manifest.contributes.themes.len(), 1);
    }

    #[test]
    fn rejects_id_without_publisher() {
        let dir = TempDir::new().unwrap();
        write_page_plugin(dir.path());
        let mut m = parsed(PAGE_MANIFEST);
        m.id = "bill-audit".into();
        let err = validate_manifest(&m, dir.path()).unwrap_err();
        assert!(err.contains("invalid extension id"), "unexpected: {err}");
    }

    #[test]
    fn rejects_uppercase_id() {
        assert!(!is_valid_extension_id("Acme.bill-audit"));
        assert!(!is_valid_extension_id("acme.Bill-Audit"));
    }

    #[test]
    fn rejects_overlong_id_segments() {
        // publisher segment: leading char + up to 30 more (31 total)
        assert!(is_valid_extension_id(&format!("{}x.bill", "a".repeat(30))));
        assert!(!is_valid_extension_id(&format!("{}x.bill", "a".repeat(31))));
        // name segment: leading char + up to 31 more (32 total)
        assert!(is_valid_extension_id(&format!("acme.{}", "b".repeat(31))));
        assert!(!is_valid_extension_id(&format!("acme.{}", "b".repeat(33))));
        assert!(is_valid_extension_id("acme.bill-audit"));
    }

    #[test]
    fn rejects_api_version_mismatch() {
        let dir = TempDir::new().unwrap();
        write_page_plugin(dir.path());
        let json = PAGE_MANIFEST.replace("\"apiVersion\": 2", "\"apiVersion\": 3");
        let m = parsed(&json);
        let err = validate_manifest(&m, dir.path()).unwrap_err();
        assert!(err.contains("apiVersion"), "unexpected: {err}");
        assert!(err.contains("需要更新版本的 DataZen"), "unexpected: {err}");
    }

    #[test]
    fn rejects_backend_not_null() {
        let dir = TempDir::new().unwrap();
        write_page_plugin(dir.path());
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
        write_page_plugin(dir.path());
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
        write_page_plugin(dir.path());
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
        write_page_plugin(&pack);
        let err = validate_extension_dir(&pack).unwrap_err();
        assert!(
            err.contains("does not match manifest id"),
            "unexpected: {err}"
        );
    }

    #[test]
    fn rejects_disallowed_declared_extension() {
        let dir = TempDir::new().unwrap();
        write_page_plugin(dir.path());
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
            write_page_plugin(dir.path());
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
    fn accepts_plugin_without_package_icon() {
        let dir = TempDir::new().unwrap();
        write_page_plugin(dir.path());
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
        assert!(err.contains("plugin icon not found"), "unexpected: {err}");
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
        write_page_plugin(dir.path());
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
        write_theme_plugin(dir.path());
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
        write_theme_plugin(dir.path());
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
            write_theme_plugin(dir.path());
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
        write_theme_plugin(dir.path());
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
        write_page_plugin(dir.path());
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
            write_page_plugin(dir_ok.path());
            assert!(
                validate_manifest(&m, dir_ok.path()).is_ok(),
                "semver `{good}` should pass"
            );
        }
    }

    #[test]
    fn rejects_forbidden_extension_in_package_scan() {
        let dir = TempDir::new().unwrap();
        write_page_plugin(dir.path());
        write_file(dir.path(), "assets/evil.sh", "#!/bin/sh");
        let m = parsed(PAGE_MANIFEST);
        let err = validate_manifest(&m, dir.path()).unwrap_err();
        assert!(err.contains("forbidden extension .sh"), "unexpected: {err}");
    }

    #[test]
    fn scan_enforces_small_limits() {
        let dir = TempDir::new().unwrap();
        write_page_plugin(dir.path());

        let tiny_files = PackageLimits {
            max_total_bytes: MAX_EXTENSION_UNCOMPRESSED,
            max_files: 2,
        };
        let err = scan_package_files(dir.path(), tiny_files).unwrap_err();
        assert!(err.contains("too many files"), "unexpected: {err}");

        let tiny_bytes = PackageLimits {
            max_total_bytes: 8,
            max_files: MAX_EXTENSION_FILES,
        };
        let err = scan_package_files(dir.path(), tiny_bytes).unwrap_err();
        assert!(err.contains("size exceeds limit"), "unexpected: {err}");
    }

    #[test]
    fn scan_skips_host_marker_files() {
        let dir = TempDir::new().unwrap();
        write_theme_plugin(dir.path());
        write_file(dir.path(), ".enabled", "1\n");
        write_file(dir.path(), ".storage.json", "{\"k\":1}");
        assert!(scan_package_files(dir.path(), PackageLimits::default()).is_ok());
    }

    #[test]
    fn scan_rejects_symlink_inside_package() {
        let dir = TempDir::new().unwrap();
        write_page_plugin(dir.path());
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
        write_page_plugin(dir.path());
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
}
