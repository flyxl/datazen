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

pub(crate) fn list_wapps_impl(state: &AppState) -> Vec<WappSummary> {
    state.wapps.list().iter().map(WappSummary::from).collect()
}

pub(crate) fn get_wapp_manifest_impl(
    state: &AppState,
    id: &str,
) -> Result<WappManifest, CommandError> {
    ensure_wapp_exists(state, id).map(|loaded| loaded.manifest)
}

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

#[cfg(test)]
#[path = "wapps_tests.rs"]
mod tests;
