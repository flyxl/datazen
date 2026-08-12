//! Locate competitor app connection files at well-known data / install paths.

use super::super::error::CommandError;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportApp {
    Dbx,
    Navicat,
    DataGrip,
    DBeaver,
    TablePlus,
}

impl ImportApp {
    pub fn parse(raw: &str) -> Result<Self, CommandError> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "dbx" => Ok(Self::Dbx),
            "navicat" => Ok(Self::Navicat),
            "datagrip" => Ok(Self::DataGrip),
            "dbeaver" => Ok(Self::DBeaver),
            "tableplus" => Ok(Self::TablePlus),
            other => Err(CommandError::Validation(format!(
                "Unknown import source: {other}"
            ))),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Dbx => "DBX",
            Self::Navicat => "Navicat",
            Self::DataGrip => "DataGrip",
            Self::DBeaver => "DBeaver",
            Self::TablePlus => "TablePlus",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PathContext {
    pub home: PathBuf,
    pub data: PathBuf,
    pub data_local: PathBuf,
    pub config: PathBuf,
}

impl PathContext {
    pub fn from_env() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
        #[cfg(target_os = "macos")]
        {
            let data = home.join("Library").join("Application Support");
            Self {
                home: home.clone(),
                data: data.clone(),
                data_local: data,
                config: home.join("Library").join("Preferences"),
            }
        }
        #[cfg(target_os = "windows")]
        {
            let data = dirs::data_dir().unwrap_or_else(|| home.join("AppData").join("Roaming"));
            let data_local =
                dirs::data_local_dir().unwrap_or_else(|| home.join("AppData").join("Local"));
            Self {
                home,
                data: data.clone(),
                data_local,
                config: data,
            }
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            let data = dirs::data_dir().unwrap_or_else(|| home.join(".local").join("share"));
            let config = dirs::config_dir().unwrap_or_else(|| home.join(".config"));
            Self {
                home,
                data: data.clone(),
                data_local: data,
                config,
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedImportPath {
    pub path: String,
    pub found: bool,
}

const MAX_SCAN_DEPTH: u32 = 5;
const MAX_SCAN_FILES: usize = 400;

fn file_name_lower(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn ext_lower(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

pub fn is_source_file(app: ImportApp, path: &Path) -> bool {
    let name = file_name_lower(path);
    let ext = ext_lower(path);
    match app {
        ImportApp::Dbx => {
            name == "dbx.db"
                || ((name.contains("dbx") || name.contains("connection")) && ext == "json")
        }
        ImportApp::DBeaver => name == "data-sources.json",
        ImportApp::DataGrip => name == "datasources.xml",
        ImportApp::Navicat => ext == "ncx",
        ImportApp::TablePlus => {
            name == "connections.plist" || ext == "plist" || ext == "tableplusconnection"
        }
    }
}

fn is_explicit_source_file(app: ImportApp, path: &Path) -> bool {
    if is_source_file(app, path) {
        return true;
    }
    let ext = ext_lower(path);
    match app {
        ImportApp::Dbx => ext == "db" || ext == "json" || ext == "sqlite",
        ImportApp::Navicat => ext == "xml",
        ImportApp::DataGrip => ext == "xml",
        ImportApp::DBeaver => ext == "json",
        ImportApp::TablePlus => ext == "tableplusconnection" || ext == "plist",
    }
}

fn skip_dir_name(name: &str) -> bool {
    matches!(
        name,
        "node_modules"
            | ".git"
            | "Cache"
            | "Code Cache"
            | "GPUCache"
            | "logs"
            | "CachedData"
            | "Crashpad"
    )
}

fn scan_dir(app: ImportApp, dir: &Path, depth: u32, budget: &mut usize, out: &mut Vec<PathBuf>) {
    if depth == 0 || *budget == 0 || !dir.is_dir() {
        return;
    }
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    let mut dirs = Vec::new();
    for entry in entries.flatten() {
        if *budget == 0 {
            return;
        }
        *budget = budget.saturating_sub(1);
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if skip_dir_name(name.as_ref()) {
                continue;
            }
            dirs.push(path);
            continue;
        }
        if is_source_file(app, &path) {
            out.push(path);
        }
    }
    for child in dirs {
        scan_dir(app, &child, depth - 1, budget, out);
    }
}

fn known_relatives(app: ImportApp) -> &'static [&'static str] {
    match app {
        ImportApp::Dbx => &["dbx.db"],
        ImportApp::DBeaver => &[
            "workspace6/General/.dbeaver/data-sources.json",
            "workspace5/General/.dbeaver/data-sources.json",
            "General/.dbeaver/data-sources.json",
            ".dbeaver/data-sources.json",
            "data-sources.json",
        ],
        ImportApp::DataGrip => &["options/dataSources.xml", "dataSources.xml"],
        ImportApp::Navicat => &[],
        ImportApp::TablePlus => &["Data/Connections.plist", "Connections.plist"],
    }
}

fn collect_under(app: ImportApp, root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    if !root.exists() {
        return found;
    }
    if root.is_file() {
        if is_explicit_source_file(app, root) {
            found.push(root.to_path_buf());
        }
        return found;
    }
    for rel in known_relatives(app) {
        let candidate = root.join(rel);
        if candidate.is_file() {
            found.push(candidate);
        }
    }
    let mut budget = MAX_SCAN_FILES;
    scan_dir(app, root, MAX_SCAN_DEPTH, &mut budget, &mut found);
    dedupe_paths(found)
}

fn dedupe_paths(mut paths: Vec<PathBuf>) -> Vec<PathBuf> {
    paths.sort();
    paths.dedup();
    paths
}

fn newest_by_mtime(paths: &[PathBuf]) -> Option<PathBuf> {
    paths
        .iter()
        .max_by_key(|p| {
            fs::metadata(p)
                .and_then(|m| m.modified())
                .ok()
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
        })
        .cloned()
}

fn default_roots(app: ImportApp, ctx: &PathContext) -> Vec<PathBuf> {
    match app {
        ImportApp::Dbx => {
            let mut roots = vec![ctx.data.join("com.dbx.app")];
            if let Ok(dir) = std::env::var("DBX_DATA_DIR") {
                let trimmed = dir.trim();
                if !trimmed.is_empty() {
                    roots.insert(0, PathBuf::from(trimmed));
                }
            }
            roots
        }
        ImportApp::DBeaver => {
            let mut roots = Vec::new();
            #[cfg(target_os = "macos")]
            {
                roots.push(ctx.home.join("Library").join("DBeaverData"));
            }
            roots.push(ctx.data.join("DBeaverData"));
            roots.push(ctx.home.join(".local").join("share").join("DBeaverData"));
            roots
        }
        ImportApp::DataGrip => {
            vec![
                ctx.data.join("JetBrains"),
                ctx.config.join("JetBrains"),
                ctx.home.join(".config").join("JetBrains"),
            ]
        }
        ImportApp::TablePlus => vec![
            ctx.data.join("com.tinyapp.TablePlus"),
            ctx.data_local.join("com.tinyapp.TablePlus"),
            ctx.config.join("tableplus"),
            ctx.data.join("tableplus"),
        ],
        ImportApp::Navicat => {
            let mut roots = vec![
                ctx.data.join("PremiumSoft CyberTech"),
                ctx.data.join("PremiumSoft"),
                ctx.config.join("navicat"),
                ctx.data.join("navicat"),
            ];
            #[cfg(target_os = "macos")]
            {
                roots.push(
                    ctx.home
                        .join("Library")
                        .join("Containers")
                        .join("com.navicat.NavicatPremium")
                        .join("Data")
                        .join("Library")
                        .join("Application Support")
                        .join("PremiumSoft CyberTech"),
                );
                roots.push(
                    ctx.home
                        .join("Library")
                        .join("Containers")
                        .join("com.prect.NavicatPremium")
                        .join("Data")
                        .join("Library")
                        .join("Application Support")
                        .join("PremiumSoft CyberTech"),
                );
            }
            roots
        }
    }
}

fn datagrip_version_dirs(jetbrains: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(jetbrains) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.to_ascii_lowercase().starts_with("datagrip"))
        })
        .collect()
}

fn collect_defaults(app: ImportApp, ctx: &PathContext) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for root in default_roots(app, ctx) {
        if app == ImportApp::DataGrip && root.ends_with("JetBrains") {
            for version in datagrip_version_dirs(&root) {
                found.extend(collect_under(app, &version));
            }
        }
        found.extend(collect_under(app, &root));
    }
    if app == ImportApp::DataGrip {
        if let Some(newest) = newest_by_mtime(&found) {
            return vec![newest];
        }
    }
    dedupe_paths(found)
}

fn primary_default_path(app: ImportApp, ctx: &PathContext) -> PathBuf {
    default_roots(app, ctx)
        .into_iter()
        .next()
        .unwrap_or_else(|| ctx.data.clone())
}

fn is_app_bundle(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()) == Some("app") || path.join("Contents").is_dir()
}

fn is_too_broad_to_scan(path: &Path, ctx: &PathContext) -> bool {
    if path == ctx.home {
        return true;
    }
    matches!(
        file_name_lower(path).as_str(),
        "applications" | "program files" | "program files (x86)" | "windows"
    )
}

/// Files to parse for this source. `custom_path` is a file, data dir, or install dir.
pub fn resolve_import_files(
    app: ImportApp,
    custom_path: Option<&Path>,
    ctx: &PathContext,
) -> Result<Vec<PathBuf>, CommandError> {
    if let Some(raw) = custom_path.filter(|p| !p.as_os_str().is_empty()) {
        if raw.is_file() {
            let found = collect_under(app, raw);
            if !found.is_empty() {
                return Ok(found);
            }
        } else if is_app_bundle(raw) || looks_like_install_dir(raw) {
            // Portable folders may contain the data files; .app bundles do not.
            if !is_app_bundle(raw) && !is_too_broad_to_scan(raw, ctx) {
                let found = collect_under(app, raw);
                if !found.is_empty() {
                    return Ok(found);
                }
            }
            let defaults = collect_defaults(app, ctx);
            if !defaults.is_empty() {
                return Ok(defaults);
            }
        } else if !is_too_broad_to_scan(raw, ctx) {
            let found = collect_under(app, raw);
            if !found.is_empty() {
                return Ok(found);
            }
        }
        return Err(CommandError::Validation(format!(
            "No {} connection files found at {}. Specify the data directory, install path, or an exported file.",
            app.label(),
            raw.display()
        )));
    }
    let found = collect_defaults(app, ctx);
    if found.is_empty() {
        return Err(CommandError::Validation(format!(
            "Could not find {} connection files at the default location. Enter the data directory, install path, or an exported file.",
            app.label()
        )));
    }
    Ok(found)
}

fn looks_like_install_dir(path: &Path) -> bool {
    let name = file_name_lower(path);
    name.contains("navicat")
        || name.contains("dbeaver")
        || name.contains("datagrip")
        || name.contains("tableplus")
        || name.contains("dbx")
}

pub fn detect_import_path(app: ImportApp, ctx: &PathContext) -> DetectedImportPath {
    let found = collect_defaults(app, ctx);
    if let Some(path) = found.first() {
        let display = if app == ImportApp::DataGrip {
            newest_by_mtime(&found).unwrap_or_else(|| path.clone())
        } else {
            path.clone()
        };
        return DetectedImportPath {
            path: display.to_string_lossy().into_owned(),
            found: true,
        };
    }
    DetectedImportPath {
        path: primary_default_path(app, ctx)
            .to_string_lossy()
            .into_owned(),
        found: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    fn ctx_at(root: &Path) -> PathContext {
        PathContext {
            home: root.to_path_buf(),
            data: root.join("data"),
            data_local: root.join("local"),
            config: root.join("config"),
        }
    }

    fn write_file(path: &Path, body: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut f = fs::File::create(path).unwrap();
        f.write_all(body.as_bytes()).unwrap();
    }

    #[test]
    fn parse_source_ids() {
        assert_eq!(ImportApp::parse("DBX").unwrap(), ImportApp::Dbx);
        assert_eq!(ImportApp::parse("dbeaver").unwrap(), ImportApp::DBeaver);
        assert!(ImportApp::parse("oracle").is_err());
    }

    #[test]
    fn detects_dbx_db_under_data_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = ctx_at(tmp.path());
        let db = ctx.data.join("com.dbx.app").join("dbx.db");
        write_file(&db, "sqlite");
        let detected = detect_import_path(ImportApp::Dbx, &ctx);
        assert!(detected.found);
        assert_eq!(detected.path, db.to_string_lossy());
        let files = resolve_import_files(ImportApp::Dbx, None, &ctx).unwrap();
        assert_eq!(files, vec![db]);
    }

    #[test]
    fn custom_file_wins() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = ctx_at(tmp.path());
        let custom = tmp.path().join("export.ncx");
        write_file(&custom, "<Connections/>");
        let files = resolve_import_files(ImportApp::Navicat, Some(&custom), &ctx).unwrap();
        assert_eq!(files, vec![custom]);
    }

    #[test]
    fn custom_dir_scans_relatives() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = ctx_at(tmp.path());
        let portable = tmp.path().join("DBeaverPortable");
        let json = portable
            .join("workspace6")
            .join("General")
            .join(".dbeaver")
            .join("data-sources.json");
        write_file(&json, "{}");
        let files = resolve_import_files(ImportApp::DBeaver, Some(&portable), &ctx).unwrap();
        assert_eq!(files, vec![json]);
    }

    #[test]
    fn missing_custom_path_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = ctx_at(tmp.path());
        let err = resolve_import_files(ImportApp::TablePlus, Some(&tmp.path().join("nope")), &ctx)
            .unwrap_err();
        assert!(err.to_string().contains("TablePlus"));
    }

    #[test]
    fn datagrip_picks_newest_version_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = ctx_at(tmp.path());
        let old = ctx
            .data
            .join("JetBrains")
            .join("DataGrip2023.1")
            .join("options")
            .join("dataSources.xml");
        let new = ctx
            .data
            .join("JetBrains")
            .join("DataGrip2024.3")
            .join("options")
            .join("dataSources.xml");
        write_file(&old, "<old/>");
        std::thread::sleep(std::time::Duration::from_millis(20));
        write_file(&new, "<new/>");
        let files = resolve_import_files(ImportApp::DataGrip, None, &ctx).unwrap();
        assert_eq!(files, vec![new]);
    }

    #[test]
    fn empty_defaults_error_asks_for_path() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = ctx_at(tmp.path());
        let err = resolve_import_files(ImportApp::Dbx, None, &ctx).unwrap_err();
        assert!(err.to_string().contains("default location"));
    }

    #[test]
    fn install_app_bundle_falls_back_to_default_data_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = ctx_at(tmp.path());
        let db = ctx.data.join("com.dbx.app").join("dbx.db");
        write_file(&db, "sqlite");
        let bundle = tmp.path().join("Applications").join("DBX.app");
        fs::create_dir_all(bundle.join("Contents")).unwrap();
        let files = resolve_import_files(ImportApp::Dbx, Some(&bundle), &ctx).unwrap();
        assert_eq!(files, vec![db]);
    }
}
