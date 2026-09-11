//! Runtime wapp manifest: schema types and validation rules.
//!
//! Mirrors shared zip path validation conventions: strict serde parsing,
//! `<publisher>.<name>` id format, semver versions, path-traversal
//! protection, and package size/file-count quotas.

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use super::WAPP_API_VERSION;

/// Total uncompressed package size limit (rule 6).
pub const MAX_WAPP_UNCOMPRESSED: u64 = 50 * 1024 * 1024;
/// Maximum number of files in a package (rule 6).
pub const MAX_WAPP_FILES: usize = 2000;

const MAX_SVG_BYTES: usize = 256 * 1024;

static WAPP_ID_RE: LazyLock<Regex> = LazyLock::new(|| {
    // panic-policy exception: compile-time constant regex; a failure here means
    // a programming error in the literal pattern, not a runtime condition.
    Regex::new(r"^[a-z0-9][a-z0-9-]{0,30}\.[a-z][a-z0-9-]{1,31}$").expect("valid wapp id regex")
});

static SEMVER_RE: LazyLock<Regex> = LazyLock::new(|| {
    // panic-policy exception: compile-time constant regex; a failure here means
    // a programming error in the literal pattern, not a runtime condition.
    Regex::new(concat!(
        r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)",
        r"(?:-((?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*)",
        r"(?:\.(?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*))*))?",
        r"(?:\+([0-9a-zA-Z-]+(?:\.[0-9a-zA-Z-]+)*))?$"
    ))
    .expect("valid semver regex")
});

// panic-policy exception: compile-time constant regex; a failure here means
// a programming error in the literal pattern, not a runtime condition.
static PAGE_ID_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-z0-9-_]{1,64}$").expect("valid page id regex"));

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WappManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub api_version: u32,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// Optional package-level icon (square brand image) shown in wapp lists.
    /// Rule-5 validated path; `png|webp|svg` allowed.
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub entry: Option<String>,
    pub contributes: Contributions,
    #[serde(default)]
    pub permissions: Vec<Permission>,
    /// Reserved for P2 backend wapps; must be null/absent in v1.
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
pub fn is_valid_wapp_id(id: &str) -> bool {
    WAPP_ID_RE.is_match(id)
}

/// Whether `ext` is in the package file-type whitelist.
pub fn allowed_wapp_file_ext(ext: &str) -> bool {
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "html" | "js" | "mjs" | "css" | "json" | "svg" | "png" | "webp" | "woff2" | "woff"
    )
}

/// Parse a manifest.json payload; unknown fields are rejected.
pub fn parse_manifest(content: &str) -> Result<WappManifest, String> {
    serde_json::from_str(content).map_err(|e| format!("invalid manifest.json: {e}"))
}

/// Read and parse `{dir}/manifest.json`.
fn read_manifest_from_dir(dir: &Path) -> Result<WappManifest, String> {
    let manifest_path = dir.join("manifest.json");
    if !manifest_path.is_file() {
        return Err("missing manifest.json".into());
    }
    let content =
        fs::read_to_string(&manifest_path).map_err(|e| format!("read manifest.json: {e}"))?;
    parse_manifest(&content)
}

/// Validate an installed wapp directory; folder name must equal `manifest.id`.
pub fn validate_wapp_dir(dir: &Path) -> Result<WappManifest, String> {
    let manifest = read_manifest_from_dir(dir)?;

    let folder_name = dir
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "invalid wapp directory path".to_string())?;
    if folder_name != manifest.id {
        return Err(format!(
            "wapp directory name `{folder_name}` does not match manifest id `{}`",
            manifest.id
        ));
    }

    validate_manifest(&manifest, dir)?;
    Ok(manifest)
}

/// Validate a manifest against its on-disk package directory (rules 1–7).
pub fn validate_manifest(manifest: &WappManifest, wapp_dir: &Path) -> Result<(), String> {
    // Rule 1: `<publisher>.<name>` id format.
    if !WAPP_ID_RE.is_match(&manifest.id) {
        return Err(format!("invalid wapp id: {}", manifest.id));
    }

    // Rule 2: API version handshake; mismatch means a newer host is required.
    if manifest.api_version != WAPP_API_VERSION {
        return Err(format!(
            "unsupported apiVersion: {} (expected {WAPP_API_VERSION}); \
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
        let entry_path = safe_declared_path(wapp_dir, entry)?;
        ensure_allowed_extension(entry)?;
        if !entry_path.is_file() {
            return Err(format!("entry file not found: {entry}"));
        }
    }

    // Rule 5: package-level icon (when declared) must be a safe, whitelisted,
    // present image file.
    if let Some(icon) = &manifest.icon {
        let icon_path = safe_declared_path(wapp_dir, icon)?;
        ensure_allowed_extension(icon)?;
        if !icon_path.is_file() {
            return Err(format!("wapp icon not found: {icon}"));
        }
    }

    // Rules 4/5: contribution-declared paths are safe, whitelisted, and present.
    for page in &manifest.contributes.pages {
        if let Some(icon) = &page.icon {
            let icon_path = safe_declared_path(wapp_dir, icon)?;
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

        let tokens_path = safe_declared_path(wapp_dir, &theme.tokens_css)?;
        ensure_allowed_extension(&theme.tokens_css)?;
        if !tokens_path.is_file() {
            return Err(format!("theme tokens.css not found: {}", theme.tokens_css));
        }
        if let Some(preview) = &theme.preview_image {
            let preview_path = safe_declared_path(wapp_dir, preview)?;
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
                let resolved = safe_declared_path(wapp_dir, path)?;
                ensure_allowed_extension(path)?;
                if !resolved.is_file() {
                    return Err(format!("{label} not found: {path}"));
                }
            }
        }
        if let Some(icons_dir) = &theme.icons_dir {
            let resolved = safe_declared_path(wapp_dir, icons_dir)?;
            if !resolved.is_dir() {
                return Err(format!(
                    "theme icons dir not found or not a directory: {icons_dir}"
                ));
            }
        }
    }

    // Rule 6: package-wide whitelist / quota scan.
    scan_package_files(wapp_dir, PackageLimits::default())?;

    // Rule 7: no mandatory permission combination — pure-theme plugins may
    // declare none; undeclared runtime calls are rejected at the bridge.
    Ok(())
}

/// Validate a declared relative path (rule 5): relative, no `..`, no hidden
/// components, resolves inside `wapp_dir`.
pub(crate) fn safe_declared_path(wapp_dir: &Path, declared: &str) -> Result<PathBuf, String> {
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

    let joined = wapp_dir.join(&rel);
    if !joined.starts_with(wapp_dir) {
        return Err(format!("declared path escapes wapp directory: {declared}"));
    }

    if joined.exists() {
        let canonical_dir = fs::canonicalize(wapp_dir).map_err(|e| e.to_string())?;
        let canonical_file = fs::canonicalize(&joined).map_err(|e| e.to_string())?;
        if !canonical_file.starts_with(&canonical_dir) {
            return Err(format!("declared path escapes wapp directory: {declared}"));
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
    if ext.is_empty() || !allowed_wapp_file_ext(&ext) {
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
            max_total_bytes: MAX_WAPP_UNCOMPRESSED,
            max_files: MAX_WAPP_FILES,
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
        if !allowed_wapp_file_ext(&ext) {
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
