//! Runtime UI-extension IPC: list / install / remove / enable, manifest lookup,
//! per-extension KV storage, and sandbox-constrained file reads.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, State};

use super::error::{resolve_override_path, CmdExt, CommandError, OVERRIDE_DISABLED_MSG};
use super::AppState;
use crate::wapps::{
    install::{install_from_dir, install_from_zip},
    storage_get, storage_remove, storage_set, LoadedWapp, WappManifest,
};

/// Emitted after any install/remove/enable change so the frontend can refresh.
pub const WAPPS_CHANGED_EVENT: &str = "wapps:changed";

const MAX_PICK_SESSIONS: usize = 8;

struct WappPickSession {
    path: PathBuf,
}

static WAPP_PICK_SESSIONS: LazyLock<tokio::sync::Mutex<HashMap<String, WappPickSession>>> =
    LazyLock::new(|| tokio::sync::Mutex::new(HashMap::new()));

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WappPackageKind {
    Zip,
    Folder,
}

impl WappPackageKind {
    fn parse(raw: &str) -> Result<Self, CommandError> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "zip" | "file" => Ok(Self::Zip),
            "folder" | "dir" | "directory" => Ok(Self::Folder),
            other => Err(CommandError::Validation(format!(
                "Invalid wapp package kind: {other}"
            ))),
        }
    }
}

fn package_label(path: &Path) -> String {
    path.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "package".into())
}

async fn insert_pick_session(path: PathBuf) -> Result<String, CommandError> {
    let token = uuid::Uuid::new_v4().to_string();
    let mut sessions = WAPP_PICK_SESSIONS.lock().await;
    if sessions.len() >= MAX_PICK_SESSIONS {
        return Err(CommandError::Validation(
            "Too many pending wapp picks".into(),
        ));
    }
    sessions.insert(token.clone(), WappPickSession { path });
    Ok(token)
}

async fn take_pick_session(token: &str) -> Result<PathBuf, CommandError> {
    let mut sessions = WAPP_PICK_SESSIONS.lock().await;
    sessions
        .remove(token)
        .map(|session| session.path)
        .ok_or_else(|| CommandError::NotFound("Wapp pick session not found or expired".into()))
}

/// Native open dialog for a plugin `.zip` or unpacked directory. Path stays
/// on the host; callers use an opaque pick token for the install step.
pub(crate) async fn pick_wapp_package_with_dialog(
    app: &AppHandle,
    kind: WappPackageKind,
) -> Result<Option<PathBuf>, CommandError> {
    match kind {
        WappPackageKind::Zip => {
            super::dialog::open_file(app, vec![("Wapp package".into(), vec!["zip".into()])]).await
        }
        WappPackageKind::Folder => super::dialog::pick_folder(app).await,
    }
}

async fn resolve_wapp_package_path(
    app: &AppHandle,
    kind: WappPackageKind,
    override_path: Option<String>,
) -> Result<Option<PathBuf>, CommandError> {
    match resolve_override_path(override_path, OVERRIDE_DISABLED_MSG)? {
        Some(path) => Ok(Some(path)),
        None => pick_wapp_package_with_dialog(app, kind).await,
    }
}

fn ensure_wapp_exists(state: &AppState, id: &str) -> Result<LoadedWapp, CommandError> {
    state
        .wapps
        .get(id)
        .ok_or_else(|| CommandError::NotFound(format!("wapp not found: {id}")))
}

// ---------------------------------------------------------------------------
// Summaries
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WappPageSummary {
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WappThemeSummary {
    pub id: String,
    pub name: String,
    pub modes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WappSummary {
    pub id: String,
    pub name: String,
    pub version: String,
    pub api_version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Optional package-level icon path (mirrors `WappManifest.icon`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub enabled: bool,
    pub permissions: Vec<String>,
    pub pages: Vec<WappPageSummary>,
    pub themes: Vec<WappThemeSummary>,
}

impl From<&LoadedWapp> for WappSummary {
    fn from(wapp: &LoadedWapp) -> Self {
        let manifest = &wapp.manifest;
        Self {
            id: manifest.id.clone(),
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            api_version: manifest.api_version,
            author: manifest.author.clone(),
            description: manifest.description.clone(),
            icon: manifest.icon.clone(),
            enabled: wapp.enabled,
            permissions: manifest
                .permissions
                .iter()
                .map(|permission| permission.as_str().to_string())
                .collect(),
            pages: manifest
                .contributes
                .pages
                .iter()
                .map(|page| WappPageSummary {
                    id: page.id.clone(),
                    title: page.title.clone(),
                    icon: page.icon.clone(),
                })
                .collect(),
            themes: manifest
                .contributes
                .themes
                .iter()
                .map(|theme| WappThemeSummary {
                    id: theme.id.clone(),
                    name: theme.name.clone(),
                    modes: theme.modes.clone(),
                })
                .collect(),
        }
    }
}

// ---------------------------------------------------------------------------
// Implementations (shared by commands and unit tests)
// ---------------------------------------------------------------------------

pub(crate) fn list_wapps_impl(state: &AppState) -> Vec<WappSummary> {
    state.wapps.list().iter().map(WappSummary::from).collect()
}

pub(crate) fn get_wapp_manifest_impl(
    state: &AppState,
    id: &str,
) -> Result<WappManifest, CommandError> {
    ensure_wapp_exists(state, id).map(|loaded| loaded.manifest)
}

pub(crate) async fn install_wapp_from_path_impl(
    state: &AppState,
    path: String,
) -> Result<WappSummary, CommandError> {
    let source = PathBuf::from(&path);
    if !source.exists() {
        return Err(CommandError::NotFound(format!(
            "wapp package not found: {}",
            source.display()
        )));
    }

    let wapps_dir = state.wapps.wapps_dir().to_path_buf();
    let manifest = tokio::task::spawn_blocking(move || {
        let is_zip = source.is_file()
            && source
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"));
        if is_zip {
            install_from_zip(&source, &wapps_dir)
        } else {
            install_from_dir(&source, &wapps_dir)
        }
    })
    .await
    .map_err(|e| CommandError::Internal(format!("install_wapp_from_path task: {e}")))?
    .map_err(CommandError::Validation)?;

    state
        .wapps
        .register(manifest.clone(), true)
        .map_err(CommandError::Validation)?;
    tracing::info!(id = %manifest.id, version = %manifest.version, "install_wapp_from_path OK");

    Ok(WappSummary::from(&LoadedWapp {
        manifest,
        enabled: true,
    }))
}

pub(crate) async fn inspect_wapp_package_impl(path: String) -> Result<WappManifest, CommandError> {
    let source = PathBuf::from(&path);
    if !source.exists() {
        return Err(CommandError::NotFound(format!(
            "wapp package not found: {}",
            source.display()
        )));
    }

    // Full rule-set validation in a throwaway temp dir; nothing touches
    // `{wapps_dir}` until `install_wapp_from_path` runs.
    tokio::task::spawn_blocking(move || crate::wapps::install::inspect_wapp_package(&source))
        .await
        .map_err(|e| CommandError::Internal(format!("inspect_wapp_package task: {e}")))?
        .map_err(CommandError::Validation)
}

pub(crate) async fn remove_wapp_impl(state: &AppState, id: String) -> Result<(), CommandError> {
    ensure_wapp_exists(state, &id)?;

    let manager = state.wapps.clone();
    let removed_id = id.clone();
    tokio::task::spawn_blocking(move || manager.remove(&removed_id))
        .await
        .map_err(|e| CommandError::Internal(format!("remove_wapp task: {e}")))?
        .map_err(CommandError::Validation)?;

    tracing::info!(%id, "remove_wapp OK");
    Ok(())
}

pub(crate) async fn set_wapp_enabled_impl(
    state: &AppState,
    id: String,
    enabled: bool,
) -> Result<(), CommandError> {
    ensure_wapp_exists(state, &id)?;

    let manager = state.wapps.clone();
    let toggled_id = id.clone();
    tokio::task::spawn_blocking(move || manager.set_enabled(&toggled_id, enabled))
        .await
        .map_err(|e| CommandError::Internal(format!("set_wapp_enabled task: {e}")))?
        .map_err(CommandError::Validation)?;

    tracing::info!(%id, %enabled, "set_wapp_enabled OK");
    Ok(())
}

pub(crate) async fn wapp_storage_get_impl(
    state: &AppState,
    wapp_id: String,
    key: String,
) -> Result<Option<Value>, CommandError> {
    ensure_wapp_exists(state, &wapp_id)?;

    let wapps_dir = state.wapps.wapps_dir().to_path_buf();
    run_storage_op(
        move || storage_get(&wapps_dir, &wapp_id, &key),
        "wapp_storage_get",
    )
    .await
}

pub(crate) async fn wapp_storage_set_impl(
    state: &AppState,
    wapp_id: String,
    key: String,
    value: Value,
) -> Result<(), CommandError> {
    ensure_wapp_exists(state, &wapp_id)?;

    let wapps_dir = state.wapps.wapps_dir().to_path_buf();
    run_storage_op(
        move || storage_set(&wapps_dir, &wapp_id, &key, value),
        "wapp_storage_set",
    )
    .await
}

pub(crate) async fn wapp_storage_remove_impl(
    state: &AppState,
    wapp_id: String,
    key: String,
) -> Result<(), CommandError> {
    ensure_wapp_exists(state, &wapp_id)?;

    let wapps_dir = state.wapps.wapps_dir().to_path_buf();
    run_storage_op(
        move || {
            storage_remove(&wapps_dir, &wapp_id, &key)?;
            Ok(())
        },
        "wapp_storage_remove",
    )
    .await
}

async fn run_storage_op<T>(
    op: impl FnOnce() -> Result<T, String> + Send + 'static,
    cmd: &'static str,
) -> Result<T, CommandError>
where
    T: Send + 'static,
{
    tokio::task::spawn_blocking(op)
        .await
        .map_err(|e| CommandError::Internal(format!("{cmd} task: {e}")))?
        .map_err(CommandError::Validation)
}

pub(crate) async fn read_wapp_file_impl(
    state: &AppState,
    id: String,
    relative_path: String,
) -> Result<Vec<u8>, CommandError> {
    let loaded = ensure_wapp_exists(state, &id)?;
    if !loaded.enabled {
        return Err(CommandError::Validation(format!("wapp is disabled: {id}")));
    }

    // Path component checks: no absolute paths, no traversal, no hidden files
    // (`.storage.json` / `.enabled` are host-managed and never readable).
    let rel = crate::app_data_archive::validate_zip_entry_path(&relative_path).map_err(|e| {
        CommandError::Validation(format!("unsafe wapp file path `{relative_path}`: {e}"))
    })?;
    for component in rel.components() {
        match component {
            std::path::Component::Normal(name) => {
                if name.to_string_lossy().starts_with('.') {
                    return Err(CommandError::Validation(format!(
                        "hidden file not allowed: {relative_path}"
                    )));
                }
            }
            _ => {
                return Err(CommandError::Validation(format!(
                    "unsafe wapp file path: {relative_path}"
                )));
            }
        }
    }

    let wapp_dir = state.wapps.wapp_dir(&id);
    let file_path = wapp_dir.join(&rel);
    if !file_path.is_file() {
        return Err(CommandError::NotFound(format!(
            "wapp file not found: {relative_path}"
        )));
    }

    super::error::assert_under_dir(&wapp_dir, &file_path, "read_wapp_file")?;

    tokio::fs::read(&file_path).await.cmd_err("read_wapp_file")
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_wapps(state: State<'_, AppState>) -> Result<Vec<WappSummary>, CommandError> {
    Ok(list_wapps_impl(&state))
}

#[tauri::command]
pub async fn get_wapp_manifest(
    state: State<'_, AppState>,
    id: String,
) -> Result<WappManifest, CommandError> {
    get_wapp_manifest_impl(&state, &id)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WappPackagePreview {
    /// Opaque host-side handle for the picked package; required for install.
    pub pick_token: String,
    /// Basename of the picked zip or folder (no directory path).
    pub package_label: String,
    pub manifest: WappManifest,
}

/// Open the native file/folder picker, validate the package, and return a
/// preview plus an opaque pick token. The filesystem path never crosses the
/// webview. E2E may pass `override_path` (webdriver builds only).
#[tauri::command]
pub async fn inspect_wapp_package_with_dialog(
    app: AppHandle,
    package_kind: String,
    override_path: Option<String>,
) -> Result<Option<WappPackagePreview>, CommandError> {
    let kind = WappPackageKind::parse(&package_kind)?;
    let Some(source) = resolve_wapp_package_path(&app, kind, override_path).await? else {
        return Ok(None);
    };
    if !source.exists() {
        return Err(CommandError::NotFound(format!(
            "wapp package not found: {}",
            source.display()
        )));
    }

    let manifest = inspect_wapp_package_impl(source.to_string_lossy().into_owned()).await?;
    let pick_token = insert_pick_session(source.clone()).await?;
    Ok(Some(WappPackagePreview {
        pick_token,
        package_label: package_label(&source),
        manifest,
    }))
}

/// Install a package previously picked via [`inspect_wapp_package_with_dialog`].
/// Production callers pass `pick_token` only; E2E may pass `override_path`
/// (webdriver builds only) to bypass the opaque session.
#[tauri::command]
pub async fn install_wapp(
    app: AppHandle,
    state: State<'_, AppState>,
    pick_token: Option<String>,
    override_path: Option<String>,
) -> Result<WappSummary, CommandError> {
    let path = match resolve_override_path(override_path, OVERRIDE_DISABLED_MSG)? {
        Some(path) => path.to_string_lossy().into_owned(),
        None => {
            let Some(token) = pick_token else {
                return Err(CommandError::Validation("No wapp package selected".into()));
            };
            take_pick_session(&token)
                .await?
                .to_string_lossy()
                .into_owned()
        }
    };

    let summary = install_wapp_from_path_impl(&state, path).await?;
    let _ = app.emit(WAPPS_CHANGED_EVENT, ());
    Ok(summary)
}

#[tauri::command]
pub async fn remove_wapp(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), CommandError> {
    remove_wapp_impl(&state, id).await?;
    let _ = app.emit(WAPPS_CHANGED_EVENT, ());
    Ok(())
}

/// Append a wapp-initiated audit entry to the host log file
/// (`{dataDir}/logs/datazen.log` via the `tracing` rolling appender).
///
/// The frontend sends only the command name and target connection id — never
/// argument contents — and both sides cap field lengths so a misbehaving
/// wapp cannot flood the log.
#[tauri::command]
pub async fn wapp_audit_log(
    wapp_id: String,
    event: String,
    detail: String,
) -> Result<(), CommandError> {
    let id = wapp_id;
    if id.is_empty() || id.chars().count() > 64 {
        return Err(CommandError::Validation("invalid wapp_id".into()));
    }
    let event = event.chars().take(64).collect::<String>();
    let detail = detail.chars().take(200).collect::<String>();
    tracing::info!(
        target: "wapp_audit",
        wapp_id = %id,
        event = %event,
        detail = %detail,
        "ui-wapp audit"
    );
    Ok(())
}

#[tauri::command]
pub async fn set_wapp_enabled(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> Result<(), CommandError> {
    set_wapp_enabled_impl(&state, id, enabled).await?;
    let _ = app.emit(WAPPS_CHANGED_EVENT, ());
    Ok(())
}

#[tauri::command]
pub async fn wapp_storage_get(
    state: State<'_, AppState>,
    wapp_id: String,
    key: String,
) -> Result<Option<Value>, CommandError> {
    wapp_storage_get_impl(&state, wapp_id, key).await
}

#[tauri::command]
pub async fn wapp_storage_set(
    state: State<'_, AppState>,
    wapp_id: String,
    key: String,
    value: Value,
) -> Result<(), CommandError> {
    wapp_storage_set_impl(&state, wapp_id, key, value).await
}

#[tauri::command]
pub async fn wapp_storage_remove(
    state: State<'_, AppState>,
    wapp_id: String,
    key: String,
) -> Result<(), CommandError> {
    wapp_storage_remove_impl(&state, wapp_id, key).await
}

#[tauri::command]
pub async fn read_wapp_file(
    state: State<'_, AppState>,
    id: String,
    relative_path: String,
) -> Result<Vec<u8>, CommandError> {
    read_wapp_file_impl(&state, id, relative_path).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::app_state::TestAppState;
    use serde_json::json;
    use std::fs;
    use std::io::Write as _;
    use std::path::Path;
    use tempfile::TempDir;
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    const DEMO_MANIFEST: &str = r#"{
      "id": "acme.demo",
      "name": "Demo Wapp",
      "version": "1.0.0",
      "apiVersion": 2,
      "author": "Acme",
      "entry": "index.html",
      "contributes": {
        "pages": [{ "id": "main", "title": "Main", "icon": "assets/icon.svg" }],
        "themes": [{
          "id": "demo-dark",
          "name": "Demo Dark",
          "tokensCss": "themes/demo-dark/tokens.css",
          "modes": ["dark"]
        }]
      },
      "permissions": ["storage:local", "command:invoke"]
    }"#;

    fn write_demo_zip(path: &Path) {
        let file = fs::File::create(path).unwrap();
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (name, content) in [
            ("manifest.json", DEMO_MANIFEST),
            ("index.html", "<html>demo</html>"),
            (
                "assets/icon.svg",
                "<svg xmlns='http://www.w3.org/2000/svg'/>",
            ),
            ("themes/demo-dark/tokens.css", ":root { --c-accent: red; }"),
        ] {
            zip.start_file(name, options).unwrap();
            zip.write_all(content.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }

    fn write_demo_dir(dir: &Path) {
        for (name, content) in [
            ("manifest.json", DEMO_MANIFEST),
            ("index.html", "<html>demo</html>"),
            (
                "assets/icon.svg",
                "<svg xmlns='http://www.w3.org/2000/svg'/>",
            ),
            ("themes/demo-dark/tokens.css", ":root { --c-accent: red; }"),
        ] {
            let path = dir.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        }
    }

    async fn install_zip(test: &TestAppState, path: &Path) -> WappSummary {
        install_wapp_from_path_impl(&test.state, path.to_string_lossy().to_string())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn install_list_enable_disable_remove_flow() {
        let test = TestAppState::new().await;
        assert!(list_wapps_impl(&test.state).is_empty());

        let tmp = TempDir::new().unwrap();
        let zip_path = tmp.path().join("demo.zip");
        write_demo_zip(&zip_path);

        // -- install (zip)
        let summary = install_zip(&test, &zip_path).await;
        assert_eq!(summary.id, "acme.demo");
        assert!(summary.enabled);
        assert_eq!(
            summary.permissions,
            vec!["storage:local".to_string(), "command:invoke".to_string()]
        );
        assert_eq!(summary.pages.len(), 1);
        assert_eq!(summary.pages[0].id, "main");
        assert_eq!(summary.themes.len(), 1);
        assert_eq!(summary.themes[0].id, "demo-dark");

        // -- listed with enabled=true
        let wapps = list_wapps_impl(&test.state);
        assert_eq!(wapps.len(), 1);
        assert_eq!(wapps[0].id, "acme.demo");
        assert!(wapps[0].enabled);

        // -- marker file written
        assert!(test
            .state
            .store
            .data_dir()
            .join("wapps/acme.demo/.enabled")
            .is_file());

        // -- manifest lookup
        let manifest = get_wapp_manifest_impl(&test.state, "acme.demo").unwrap();
        assert_eq!(manifest.version, "1.0.0");

        // -- disable: still listed, enabled=false, marker removed
        set_wapp_enabled_impl(&test.state, "acme.demo".into(), false)
            .await
            .unwrap();
        let wapps = list_wapps_impl(&test.state);
        assert_eq!(wapps.len(), 1);
        assert!(!wapps[0].enabled);
        assert!(!test
            .state
            .store
            .data_dir()
            .join("wapps/acme.demo/.enabled")
            .exists());

        // -- reads are refused while disabled
        let err = read_wapp_file_impl(&test.state, "acme.demo".into(), "index.html".into())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("disabled"));

        // -- enable again
        set_wapp_enabled_impl(&test.state, "acme.demo".into(), true)
            .await
            .unwrap();
        assert!(list_wapps_impl(&test.state)[0].enabled);

        // -- remove deletes the directory and unregisters
        remove_wapp_impl(&test.state, "acme.demo".into())
            .await
            .unwrap();
        assert!(list_wapps_impl(&test.state).is_empty());
        assert!(!test.state.store.data_dir().join("wapps/acme.demo").exists());

        // -- unknown ids error cleanly
        assert!(remove_wapp_impl(&test.state, "acme.demo".into())
            .await
            .is_err());
        assert!(get_wapp_manifest_impl(&test.state, "acme.demo").is_err());
    }

    #[tokio::test]
    async fn install_from_directory_and_reinstall() {
        let test = TestAppState::new().await;
        let src = TempDir::new().unwrap();
        write_demo_dir(src.path());

        let summary =
            install_wapp_from_path_impl(&test.state, src.path().to_string_lossy().to_string())
                .await
                .unwrap();
        assert_eq!(summary.id, "acme.demo");

        // Reinstall over the same id keeps exactly one entry.
        install_wapp_from_path_impl(&test.state, src.path().to_string_lossy().to_string())
            .await
            .unwrap();
        assert_eq!(list_wapps_impl(&test.state).len(), 1);
    }

    #[tokio::test]
    async fn install_rejects_invalid_packages() {
        let test = TestAppState::new().await;

        // Missing path.
        let err = install_wapp_from_path_impl(&test.state, "/nonexistent/pkg.zip".into())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("not found"));

        // Zip whose manifest fails validation (apiVersion mismatch).
        let tmp = TempDir::new().unwrap();
        let bad = tmp.path().join("bad.zip");
        {
            let file = fs::File::create(&bad).unwrap();
            let mut zip = ZipWriter::new(file);
            let options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            let manifest = DEMO_MANIFEST.replace("\"apiVersion\": 2", "\"apiVersion\": 99");
            zip.start_file("manifest.json", options).unwrap();
            zip.write_all(manifest.as_bytes()).unwrap();
            zip.finish().unwrap();
        }
        let err = install_wapp_from_path_impl(&test.state, bad.to_string_lossy().to_string())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("apiVersion"), "{err}");
        assert!(list_wapps_impl(&test.state).is_empty());
    }

    #[tokio::test]
    async fn inspect_wapp_package_previews_manifest_without_writing() {
        let test = TestAppState::new().await;
        let wapps_dir = test.state.wapps.wapps_dir().to_path_buf();

        // Unknown path → NotFound.
        let err = inspect_wapp_package_impl("/nonexistent/pkg.zip".into())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("not found"), "{err}");

        let tmp = TempDir::new().unwrap();
        let zip_path = tmp.path().join("demo.zip");
        write_demo_zip(&zip_path);

        // Valid package: manifest returned, plugins dir untouched.
        assert!(!wapps_dir.exists() || wapps_dir.read_dir().unwrap().next().is_none());
        let manifest = inspect_wapp_package_impl(zip_path.to_string_lossy().to_string())
            .await
            .unwrap();
        assert_eq!(manifest.id, "acme.demo");
        assert_eq!(manifest.version, "1.0.0");
        assert!(list_wapps_impl(&test.state).is_empty());
        assert!(!wapps_dir.join("acme.demo").exists());

        // Invalid package surfaces the validation error.
        let bad = tmp.path().join("bad.zip");
        {
            let file = fs::File::create(&bad).unwrap();
            let mut zip = ZipWriter::new(file);
            let options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            let manifest = DEMO_MANIFEST.replace("\"apiVersion\": 2", "\"apiVersion\": 99");
            zip.start_file("manifest.json", options).unwrap();
            zip.write_all(manifest.as_bytes()).unwrap();
            zip.finish().unwrap();
        }
        let err = inspect_wapp_package_impl(bad.to_string_lossy().to_string())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("apiVersion"), "{err}");
    }

    #[tokio::test]
    async fn storage_requires_existing_wapp_and_namespaces_by_id() {
        let test = TestAppState::new().await;

        // Unknown wapp id is rejected before touching the disk.
        assert!(
            wapp_storage_get_impl(&test.state, "acme.ghost".into(), "k".into())
                .await
                .is_err()
        );

        let tmp = TempDir::new().unwrap();
        let zip_path = tmp.path().join("demo.zip");
        write_demo_zip(&zip_path);
        install_zip(&test, &zip_path).await;

        wapp_storage_set_impl(
            &test.state,
            "acme.demo".into(),
            "lastUid".into(),
            json!(58043285),
        )
        .await
        .unwrap();

        let value = wapp_storage_get_impl(&test.state, "acme.demo".into(), "lastUid".into())
            .await
            .unwrap();
        assert_eq!(value, Some(json!(58043285)));

        wapp_storage_remove_impl(&test.state, "acme.demo".into(), "lastUid".into())
            .await
            .unwrap();
        let value = wapp_storage_get_impl(&test.state, "acme.demo".into(), "lastUid".into())
            .await
            .unwrap();
        assert_eq!(value, None);
    }

    #[tokio::test]
    async fn read_wapp_file_enforces_sandbox_rules() {
        let test = TestAppState::new().await;

        let tmp = TempDir::new().unwrap();
        let zip_path = tmp.path().join("demo.zip");
        write_demo_zip(&zip_path);
        install_zip(&test, &zip_path).await;

        // Normal read.
        let html = read_wapp_file_impl(&test.state, "acme.demo".into(), "index.html".into())
            .await
            .unwrap();
        assert_eq!(html, b"<html>demo</html>");

        // Nested read inside assets/.
        read_wapp_file_impl(&test.state, "acme.demo".into(), "assets/icon.svg".into())
            .await
            .unwrap();

        // Host-managed hidden files are refused.
        for hidden in [".storage.json", ".enabled"] {
            let err = read_wapp_file_impl(&test.state, "acme.demo".into(), hidden.into())
                .await
                .unwrap_err();
            assert!(
                err.to_string().contains("hidden") || err.to_string().contains("unsafe"),
                "`{hidden}`: {err}"
            );
        }

        // Traversal is refused.
        let err = read_wapp_file_impl(&test.state, "acme.demo".into(), "../settings.json".into())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("unsafe"), "{err}");

        // Missing files are NotFound.
        let err = read_wapp_file_impl(&test.state, "acme.demo".into(), "nope.html".into())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("not found"), "{err}");

        // Unknown wapps are NotFound before any path handling.
        let err = read_wapp_file_impl(&test.state, "acme.ghost".into(), "index.html".into())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("not found"), "{err}");
    }

    #[tokio::test]
    async fn load_from_disk_restores_registry_with_persisted_enabled_state() {
        let test = TestAppState::new().await;
        let tmp = TempDir::new().unwrap();
        let zip_path = tmp.path().join("demo.zip");
        write_demo_zip(&zip_path);
        install_zip(&test, &zip_path).await;
        set_wapp_enabled_impl(&test.state, "acme.demo".into(), false)
            .await
            .unwrap();

        // Simulate a restart: fresh manager over the same app-data dir.
        let reloaded = crate::wapps::WappManager::new(test.state.store.data_dir().join("wapps"));
        reloaded.load_from_disk();
        let restored = reloaded.get("acme.demo").expect("wapp restored");
        assert!(!restored.enabled);
        assert_eq!(restored.manifest.name, "Demo Wapp");
    }

    #[test]
    fn summary_serializes_camel_case_payload() {
        let dir = TempDir::new().unwrap();
        let pack = dir.path().join("acme.demo");
        write_demo_dir(&pack);
        let manifest = crate::wapps::validate_wapp_dir(&pack).unwrap();
        let summary = WappSummary::from(&LoadedWapp {
            manifest,
            enabled: true,
        });
        let json = serde_json::to_value(&summary).unwrap();
        assert_eq!(json["apiVersion"], 2);
        assert_eq!(json["enabled"], true);
        assert_eq!(json["author"], "Acme");
        assert_eq!(json["permissions"][0], "storage:local");
        assert_eq!(json["pages"][0]["id"], "main");
        assert_eq!(json["pages"][0]["icon"], "assets/icon.svg");
        assert_eq!(json["themes"][0]["id"], "demo-dark");
        assert_eq!(json["themes"][0]["modes"][0], "dark");
        // description is None in the manifest → omitted from the payload.
        assert!(json.get("description").is_none());
    }

    #[test]
    fn wapp_package_kind_parses_zip_and_folder_aliases() {
        assert_eq!(WappPackageKind::parse("zip").unwrap(), WappPackageKind::Zip);
        assert_eq!(
            WappPackageKind::parse("folder").unwrap(),
            WappPackageKind::Folder
        );
        assert!(WappPackageKind::parse("bogus").is_err());
    }

    #[tokio::test]
    async fn pick_session_is_consumed_once_on_install_path_resolution() {
        let tmp = TempDir::new().unwrap();
        let token = insert_pick_session(tmp.path().to_path_buf()).await.unwrap();
        let resolved = take_pick_session(&token).await.unwrap();
        assert_eq!(resolved, tmp.path());
        assert!(take_pick_session(&token).await.is_err());
    }

    #[test]
    fn merged_wapp_commands_gate_override_path_in_production() {
        const SOURCE: &str = include_str!("wapps.rs");
        const BOOTSTRAP_RS: &str = include_str!("../bootstrap.rs");

        for gone in [
            "commands::list_extensions,",
            "commands::install_extension,",
            "commands::remove_extension,",
            "commands::set_extension_enabled,",
            "commands::get_extension_manifest,",
            "commands::inspect_extension_package_with_dialog,",
            "commands::extension_storage_get,",
            "commands::extension_storage_set,",
            "commands::extension_storage_remove,",
            "commands::read_extension_file,",
            "commands::extension_audit_log,",
        ] {
            assert!(
                !BOOTSTRAP_RS.contains(gone),
                "`{gone}` must no longer be registered"
            );
        }
        for kept in [
            "commands::list_wapps,",
            "commands::install_wapp,",
            "commands::remove_wapp,",
            "commands::set_wapp_enabled,",
            "commands::get_wapp_manifest,",
            "commands::inspect_wapp_package_with_dialog,",
            "commands::wapp_storage_get,",
            "commands::wapp_storage_set,",
            "commands::wapp_storage_remove,",
            "commands::read_wapp_file,",
            "commands::wapp_audit_log,",
        ] {
            assert!(BOOTSTRAP_RS.contains(kept), "`{kept}` must stay registered");
        }

        let call = "resolve_override_path(override_path";
        let gated = SOURCE.matches(call).count();
        assert_eq!(
            gated, 3,
            "inspect resolver + inspect + install must gate override_path through the shared helper"
        );
    }
}
