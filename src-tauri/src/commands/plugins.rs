//! Runtime UI-plugin IPC: list / install / remove / enable, manifest lookup,
//! per-plugin KV storage, and sandbox-constrained file reads.

use std::fs;
use std::path::PathBuf;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, State};

use super::error::{CmdExt, CommandError};
use super::AppState;
use crate::plugins::{
    install::{install_from_dir, install_from_zip},
    storage_get, storage_remove, storage_set, LoadedPlugin, PluginManifest,
};

/// Emitted after any install/remove/enable change so the frontend can refresh.
pub const PLUGINS_CHANGED_EVENT: &str = "plugins:changed";

fn ensure_plugin_exists(state: &AppState, id: &str) -> Result<LoadedPlugin, CommandError> {
    state
        .plugins
        .get(id)
        .ok_or_else(|| CommandError::NotFound(format!("plugin not found: {id}")))
}

// ---------------------------------------------------------------------------
// Summaries
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginPageSummary {
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginThemeSummary {
    pub id: String,
    pub name: String,
    pub modes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginSummary {
    pub id: String,
    pub name: String,
    pub version: String,
    pub api_version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub enabled: bool,
    pub permissions: Vec<String>,
    pub pages: Vec<PluginPageSummary>,
    pub themes: Vec<PluginThemeSummary>,
}

impl From<&LoadedPlugin> for PluginSummary {
    fn from(plugin: &LoadedPlugin) -> Self {
        let manifest = &plugin.manifest;
        Self {
            id: manifest.id.clone(),
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            api_version: manifest.api_version,
            author: manifest.author.clone(),
            description: manifest.description.clone(),
            enabled: plugin.enabled,
            permissions: manifest
                .permissions
                .iter()
                .map(|permission| permission.as_str().to_string())
                .collect(),
            pages: manifest
                .contributes
                .pages
                .iter()
                .map(|page| PluginPageSummary {
                    id: page.id.clone(),
                    title: page.title.clone(),
                    icon: page.icon.clone(),
                })
                .collect(),
            themes: manifest
                .contributes
                .themes
                .iter()
                .map(|theme| PluginThemeSummary {
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

pub(crate) fn list_plugins_impl(state: &AppState) -> Vec<PluginSummary> {
    state
        .plugins
        .list()
        .iter()
        .map(PluginSummary::from)
        .collect()
}

pub(crate) fn get_plugin_manifest_impl(
    state: &AppState,
    id: &str,
) -> Result<PluginManifest, CommandError> {
    ensure_plugin_exists(state, id).map(|loaded| loaded.manifest)
}

pub(crate) async fn install_plugin_from_path_impl(
    state: &AppState,
    path: String,
) -> Result<PluginSummary, CommandError> {
    let source = PathBuf::from(&path);
    if !source.exists() {
        return Err(CommandError::NotFound(format!(
            "plugin package not found: {}",
            source.display()
        )));
    }

    let plugins_dir = state.plugins.plugins_dir().to_path_buf();
    let manifest = tokio::task::spawn_blocking(move || {
        let is_zip = source.is_file()
            && source
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"));
        if is_zip {
            install_from_zip(&source, &plugins_dir)
        } else {
            install_from_dir(&source, &plugins_dir)
        }
    })
    .await
    .map_err(|e| CommandError::Internal(format!("install_plugin_from_path task: {e}")))?
    .map_err(CommandError::Validation)?;

    state
        .plugins
        .register(manifest.clone(), true)
        .map_err(CommandError::Validation)?;
    tracing::info!(id = %manifest.id, version = %manifest.version, "install_plugin_from_path OK");

    Ok(PluginSummary::from(&LoadedPlugin {
        manifest,
        enabled: true,
    }))
}

pub(crate) async fn inspect_plugin_package_impl(
    path: String,
) -> Result<PluginManifest, CommandError> {
    let source = PathBuf::from(&path);
    if !source.exists() {
        return Err(CommandError::NotFound(format!(
            "plugin package not found: {}",
            source.display()
        )));
    }

    // Full rule-set validation in a throwaway temp dir; nothing touches
    // `{plugins_dir}` until `install_plugin_from_path` runs.
    tokio::task::spawn_blocking(move || crate::plugins::install::inspect_plugin_package(&source))
        .await
        .map_err(|e| CommandError::Internal(format!("inspect_plugin_package task: {e}")))?
        .map_err(CommandError::Validation)
}

pub(crate) async fn remove_plugin_impl(state: &AppState, id: String) -> Result<(), CommandError> {
    ensure_plugin_exists(state, &id)?;

    let manager = state.plugins.clone();
    let removed_id = id.clone();
    tokio::task::spawn_blocking(move || manager.remove(&removed_id))
        .await
        .map_err(|e| CommandError::Internal(format!("remove_plugin task: {e}")))?
        .map_err(CommandError::Validation)?;

    tracing::info!(%id, "remove_plugin OK");
    Ok(())
}

pub(crate) async fn set_plugin_enabled_impl(
    state: &AppState,
    id: String,
    enabled: bool,
) -> Result<(), CommandError> {
    ensure_plugin_exists(state, &id)?;

    let manager = state.plugins.clone();
    let toggled_id = id.clone();
    tokio::task::spawn_blocking(move || manager.set_enabled(&toggled_id, enabled))
        .await
        .map_err(|e| CommandError::Internal(format!("set_plugin_enabled task: {e}")))?
        .map_err(CommandError::Validation)?;

    tracing::info!(%id, %enabled, "set_plugin_enabled OK");
    Ok(())
}

pub(crate) async fn plugin_storage_get_impl(
    state: &AppState,
    plugin_id: String,
    key: String,
) -> Result<Option<Value>, CommandError> {
    ensure_plugin_exists(state, &plugin_id)?;

    let plugins_dir = state.plugins.plugins_dir().to_path_buf();
    run_storage_op(
        move || storage_get(&plugins_dir, &plugin_id, &key),
        "plugin_storage_get",
    )
    .await
}

pub(crate) async fn plugin_storage_set_impl(
    state: &AppState,
    plugin_id: String,
    key: String,
    value: Value,
) -> Result<(), CommandError> {
    ensure_plugin_exists(state, &plugin_id)?;

    let plugins_dir = state.plugins.plugins_dir().to_path_buf();
    run_storage_op(
        move || storage_set(&plugins_dir, &plugin_id, &key, value),
        "plugin_storage_set",
    )
    .await
}

pub(crate) async fn plugin_storage_remove_impl(
    state: &AppState,
    plugin_id: String,
    key: String,
) -> Result<(), CommandError> {
    ensure_plugin_exists(state, &plugin_id)?;

    let plugins_dir = state.plugins.plugins_dir().to_path_buf();
    run_storage_op(
        move || {
            storage_remove(&plugins_dir, &plugin_id, &key)?;
            Ok(())
        },
        "plugin_storage_remove",
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

pub(crate) async fn read_plugin_file_impl(
    state: &AppState,
    id: String,
    relative_path: String,
) -> Result<Vec<u8>, CommandError> {
    let loaded = ensure_plugin_exists(state, &id)?;
    if !loaded.enabled {
        return Err(CommandError::Validation(format!(
            "plugin is disabled: {id}"
        )));
    }

    // Path component checks: no absolute paths, no traversal, no hidden files
    // (`.storage.json` / `.enabled` are host-managed and never readable).
    let rel = crate::app_data_archive::validate_zip_entry_path(&relative_path).map_err(|e| {
        CommandError::Validation(format!("unsafe plugin file path `{relative_path}`: {e}"))
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
                    "unsafe plugin file path: {relative_path}"
                )));
            }
        }
    }

    let plugin_dir = state.plugins.plugin_dir(&id);
    let file_path = plugin_dir.join(&rel);
    if !file_path.is_file() {
        return Err(CommandError::NotFound(format!(
            "plugin file not found: {relative_path}"
        )));
    }

    let canonical_base = fs::canonicalize(&plugin_dir).cmd_err("read_plugin_file")?;
    let canonical_file = fs::canonicalize(&file_path).cmd_err("read_plugin_file")?;
    if !canonical_file.starts_with(&canonical_base) {
        return Err(CommandError::Validation(
            "path traversal not allowed".into(),
        ));
    }

    tokio::fs::read(&file_path)
        .await
        .cmd_err("read_plugin_file")
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_plugins(state: State<'_, AppState>) -> Result<Vec<PluginSummary>, CommandError> {
    Ok(list_plugins_impl(&state))
}

#[tauri::command]
pub async fn get_plugin_manifest(
    state: State<'_, AppState>,
    id: String,
) -> Result<PluginManifest, CommandError> {
    get_plugin_manifest_impl(&state, &id)
}

#[tauri::command]
pub async fn install_plugin_from_path(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<PluginSummary, CommandError> {
    let summary = install_plugin_from_path_impl(&state, path).await?;
    let _ = app.emit(PLUGINS_CHANGED_EVENT, ());
    Ok(summary)
}

#[tauri::command]
pub async fn inspect_plugin_package(path: String) -> Result<PluginManifest, CommandError> {
    inspect_plugin_package_impl(path).await
}

#[tauri::command]
pub async fn remove_plugin(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), CommandError> {
    remove_plugin_impl(&state, id).await?;
    let _ = app.emit(PLUGINS_CHANGED_EVENT, ());
    Ok(())
}

/// Append a plugin-initiated audit entry to the host log file
/// (`{dataDir}/logs/datazen.log` via the `tracing` rolling appender).
///
/// The frontend sends only the command name and target connection id — never
/// argument contents — and both sides cap field lengths so a misbehaving
/// plugin cannot flood the log.
#[tauri::command]
pub async fn plugin_audit_log(
    plugin_id: String,
    event: String,
    detail: String,
) -> Result<(), CommandError> {
    if plugin_id.is_empty() || plugin_id.chars().count() > 64 {
        return Err(CommandError::Validation("invalid plugin_id".into()));
    }
    let event = event.chars().take(64).collect::<String>();
    let detail = detail.chars().take(200).collect::<String>();
    tracing::info!(
        target: "ui_plugin_audit",
        plugin_id = %plugin_id,
        event = %event,
        detail = %detail,
        "ui-plugin audit"
    );
    Ok(())
}

#[tauri::command]
pub async fn set_plugin_enabled(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> Result<(), CommandError> {
    set_plugin_enabled_impl(&state, id, enabled).await?;
    let _ = app.emit(PLUGINS_CHANGED_EVENT, ());
    Ok(())
}

#[tauri::command]
pub async fn plugin_storage_get(
    state: State<'_, AppState>,
    plugin_id: String,
    key: String,
) -> Result<Option<Value>, CommandError> {
    plugin_storage_get_impl(&state, plugin_id, key).await
}

#[tauri::command]
pub async fn plugin_storage_set(
    state: State<'_, AppState>,
    plugin_id: String,
    key: String,
    value: Value,
) -> Result<(), CommandError> {
    plugin_storage_set_impl(&state, plugin_id, key, value).await
}

#[tauri::command]
pub async fn plugin_storage_remove(
    state: State<'_, AppState>,
    plugin_id: String,
    key: String,
) -> Result<(), CommandError> {
    plugin_storage_remove_impl(&state, plugin_id, key).await
}

#[tauri::command]
pub async fn read_plugin_file(
    state: State<'_, AppState>,
    id: String,
    relative_path: String,
) -> Result<Vec<u8>, CommandError> {
    read_plugin_file_impl(&state, id, relative_path).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::app_state::TestAppState;
    use serde_json::json;
    use std::io::Write as _;
    use std::path::Path;
    use tempfile::TempDir;
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    const DEMO_MANIFEST: &str = r#"{
      "id": "acme.demo",
      "name": "Demo Plugin",
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

    async fn install_zip(test: &TestAppState, path: &Path) -> PluginSummary {
        install_plugin_from_path_impl(&test.state, path.to_string_lossy().to_string())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn install_list_enable_disable_remove_flow() {
        let test = TestAppState::new().await;
        assert!(list_plugins_impl(&test.state).is_empty());

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
        let plugins = list_plugins_impl(&test.state);
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].id, "acme.demo");
        assert!(plugins[0].enabled);

        // -- marker file written
        assert!(test
            .state
            .store
            .data_dir()
            .join("plugins/acme.demo/.enabled")
            .is_file());

        // -- manifest lookup
        let manifest = get_plugin_manifest_impl(&test.state, "acme.demo").unwrap();
        assert_eq!(manifest.version, "1.0.0");

        // -- disable: still listed, enabled=false, marker removed
        set_plugin_enabled_impl(&test.state, "acme.demo".into(), false)
            .await
            .unwrap();
        let plugins = list_plugins_impl(&test.state);
        assert_eq!(plugins.len(), 1);
        assert!(!plugins[0].enabled);
        assert!(!test
            .state
            .store
            .data_dir()
            .join("plugins/acme.demo/.enabled")
            .exists());

        // -- reads are refused while disabled
        let err = read_plugin_file_impl(&test.state, "acme.demo".into(), "index.html".into())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("disabled"));

        // -- enable again
        set_plugin_enabled_impl(&test.state, "acme.demo".into(), true)
            .await
            .unwrap();
        assert!(list_plugins_impl(&test.state)[0].enabled);

        // -- remove deletes the directory and unregisters
        remove_plugin_impl(&test.state, "acme.demo".into())
            .await
            .unwrap();
        assert!(list_plugins_impl(&test.state).is_empty());
        assert!(!test
            .state
            .store
            .data_dir()
            .join("plugins/acme.demo")
            .exists());

        // -- unknown ids error cleanly
        assert!(remove_plugin_impl(&test.state, "acme.demo".into())
            .await
            .is_err());
        assert!(get_plugin_manifest_impl(&test.state, "acme.demo").is_err());
    }

    #[tokio::test]
    async fn install_from_directory_and_reinstall() {
        let test = TestAppState::new().await;
        let src = TempDir::new().unwrap();
        write_demo_dir(src.path());

        let summary =
            install_plugin_from_path_impl(&test.state, src.path().to_string_lossy().to_string())
                .await
                .unwrap();
        assert_eq!(summary.id, "acme.demo");

        // Reinstall over the same id keeps exactly one entry.
        install_plugin_from_path_impl(&test.state, src.path().to_string_lossy().to_string())
            .await
            .unwrap();
        assert_eq!(list_plugins_impl(&test.state).len(), 1);
    }

    #[tokio::test]
    async fn install_rejects_invalid_packages() {
        let test = TestAppState::new().await;

        // Missing path.
        let err = install_plugin_from_path_impl(&test.state, "/nonexistent/pkg.zip".into())
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
        let err = install_plugin_from_path_impl(&test.state, bad.to_string_lossy().to_string())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("apiVersion"), "{err}");
        assert!(list_plugins_impl(&test.state).is_empty());
    }

    #[tokio::test]
    async fn inspect_plugin_package_previews_manifest_without_writing() {
        let test = TestAppState::new().await;
        let plugins_dir = test.state.plugins.plugins_dir().to_path_buf();

        // Unknown path → NotFound.
        let err = inspect_plugin_package_impl("/nonexistent/pkg.zip".into())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("not found"), "{err}");

        let tmp = TempDir::new().unwrap();
        let zip_path = tmp.path().join("demo.zip");
        write_demo_zip(&zip_path);

        // Valid package: manifest returned, plugins dir untouched.
        assert!(!plugins_dir.exists() || plugins_dir.read_dir().unwrap().next().is_none());
        let manifest = inspect_plugin_package_impl(zip_path.to_string_lossy().to_string())
            .await
            .unwrap();
        assert_eq!(manifest.id, "acme.demo");
        assert_eq!(manifest.version, "1.0.0");
        assert!(list_plugins_impl(&test.state).is_empty());
        assert!(!plugins_dir.join("acme.demo").exists());

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
        let err = inspect_plugin_package_impl(bad.to_string_lossy().to_string())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("apiVersion"), "{err}");
    }

    #[tokio::test]
    async fn storage_requires_existing_plugin_and_namespaces_by_id() {
        let test = TestAppState::new().await;

        // Unknown plugin id is rejected before touching the disk.
        assert!(
            plugin_storage_get_impl(&test.state, "acme.ghost".into(), "k".into())
                .await
                .is_err()
        );

        let tmp = TempDir::new().unwrap();
        let zip_path = tmp.path().join("demo.zip");
        write_demo_zip(&zip_path);
        install_zip(&test, &zip_path).await;

        plugin_storage_set_impl(
            &test.state,
            "acme.demo".into(),
            "lastUid".into(),
            json!(58043285),
        )
        .await
        .unwrap();

        let value = plugin_storage_get_impl(&test.state, "acme.demo".into(), "lastUid".into())
            .await
            .unwrap();
        assert_eq!(value, Some(json!(58043285)));

        plugin_storage_remove_impl(&test.state, "acme.demo".into(), "lastUid".into())
            .await
            .unwrap();
        let value = plugin_storage_get_impl(&test.state, "acme.demo".into(), "lastUid".into())
            .await
            .unwrap();
        assert_eq!(value, None);
    }

    #[tokio::test]
    async fn read_plugin_file_enforces_sandbox_rules() {
        let test = TestAppState::new().await;

        let tmp = TempDir::new().unwrap();
        let zip_path = tmp.path().join("demo.zip");
        write_demo_zip(&zip_path);
        install_zip(&test, &zip_path).await;

        // Normal read.
        let html = read_plugin_file_impl(&test.state, "acme.demo".into(), "index.html".into())
            .await
            .unwrap();
        assert_eq!(html, b"<html>demo</html>");

        // Nested read inside assets/.
        read_plugin_file_impl(&test.state, "acme.demo".into(), "assets/icon.svg".into())
            .await
            .unwrap();

        // Host-managed hidden files are refused.
        for hidden in [".storage.json", ".enabled"] {
            let err = read_plugin_file_impl(&test.state, "acme.demo".into(), hidden.into())
                .await
                .unwrap_err();
            assert!(
                err.to_string().contains("hidden") || err.to_string().contains("unsafe"),
                "`{hidden}`: {err}"
            );
        }

        // Traversal is refused.
        let err = read_plugin_file_impl(&test.state, "acme.demo".into(), "../settings.json".into())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("unsafe"), "{err}");

        // Missing files are NotFound.
        let err = read_plugin_file_impl(&test.state, "acme.demo".into(), "nope.html".into())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("not found"), "{err}");

        // Unknown plugins are NotFound before any path handling.
        let err = read_plugin_file_impl(&test.state, "acme.ghost".into(), "index.html".into())
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
        set_plugin_enabled_impl(&test.state, "acme.demo".into(), false)
            .await
            .unwrap();

        // Simulate a restart: fresh manager over the same app-data dir.
        let reloaded =
            crate::plugins::PluginManager::new(test.state.store.data_dir().join("plugins"));
        reloaded.load_from_disk();
        let restored = reloaded.get("acme.demo").expect("plugin restored");
        assert!(!restored.enabled);
        assert_eq!(restored.manifest.name, "Demo Plugin");
    }

    #[test]
    fn summary_serializes_camel_case_payload() {
        let dir = TempDir::new().unwrap();
        let pack = dir.path().join("acme.demo");
        write_demo_dir(&pack);
        let manifest = crate::plugins::validate_plugin_dir(&pack).unwrap();
        let summary = PluginSummary::from(&LoadedPlugin {
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
}
