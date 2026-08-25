//! Theme pack IPC: list, install, remove, read files.

use std::fs;
use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;

use super::error::{CmdExt, CommandError};
use super::AppState;
use crate::theme::surface_bg::SurfaceBgCache;
use crate::theme::{install_theme_zip, validate_pack_dir, ThemeManifest};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemePackSummary {
    pub id: String,
    pub name: String,
    pub version: String,
    pub api_version: u32,
    pub modes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl From<ThemeManifest> for ThemePackSummary {
    fn from(m: ThemeManifest) -> Self {
        Self {
            id: m.id,
            name: m.name,
            version: m.version,
            api_version: m.api_version,
            modes: m.modes,
            author: m.author,
            description: m.description,
        }
    }
}

fn themes_root(state: &AppState) -> PathBuf {
    state.store.data_dir().join("themes")
}

fn validate_pack_id(id: &str) -> Result<(), CommandError> {
    if id.is_empty()
        || id.contains('/')
        || id.contains('\\')
        || id.contains("..")
        || id.contains('\0')
    {
        return Err(CommandError::Validation(format!("invalid theme id: {id}")));
    }
    Ok(())
}

/// Normalize a relative path under a pack root; reject `..` and absolute paths.
fn safe_pack_rel_path(relative: &str) -> Result<PathBuf, CommandError> {
    crate::app_data_archive::validate_zip_entry_path(relative)
        .map_err(|e| CommandError::Validation(e.to_string()))
}

fn pack_dir(state: &AppState, id: &str) -> Result<PathBuf, CommandError> {
    validate_pack_id(id)?;
    Ok(themes_root(state).join(id))
}

pub(crate) async fn list_theme_packs_impl(
    state: &AppState,
) -> Result<Vec<ThemePackSummary>, CommandError> {
    let root = themes_root(state);
    if !root.is_dir() {
        return Ok(Vec::new());
    }

    let mut packs = Vec::new();
    let entries = fs::read_dir(&root).cmd_err("list_theme_packs")?;
    for entry in entries {
        let entry = entry.cmd_err("list_theme_packs")?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        match validate_pack_dir(&path) {
            Ok(manifest) => packs.push(ThemePackSummary::from(manifest)),
            Err(e) => {
                tracing::warn!(dir = %path.display(), error = %e, "skipping invalid theme pack")
            }
        }
    }
    packs.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(packs)
}

pub(crate) async fn remove_theme_pack_impl(
    state: &AppState,
    id: String,
) -> Result<(), CommandError> {
    validate_pack_id(&id)?;
    let dir = pack_dir(state, &id)?;
    if !dir.is_dir() {
        return Err(CommandError::NotFound(format!(
            "theme pack not found: {id}"
        )));
    }

    let mut settings = state.store.get_settings().await;
    if settings.theme.pack_id.as_deref() == Some(id.as_str()) {
        settings.theme.pack_id = None;
        state.store.save_settings(settings).await?;
    }

    tokio::task::spawn_blocking(move || fs::remove_dir_all(&dir))
        .await
        .map_err(|e| CommandError::Internal(format!("remove_theme_pack task: {e}")))?
        .cmd_err("remove_theme_pack")?;

    tracing::info!(%id, "remove_theme_pack OK");
    Ok(())
}

pub(crate) async fn read_theme_pack_file_impl(
    state: &AppState,
    id: String,
    relative_path: String,
) -> Result<Vec<u8>, CommandError> {
    let rel = safe_pack_rel_path(&relative_path)?;
    let pack = pack_dir(state, &id)?;
    if !pack.is_dir() {
        return Err(CommandError::NotFound(format!(
            "theme pack not found: {id}"
        )));
    }

    let file_path = pack.join(&rel);
    if !file_path.is_file() {
        return Err(CommandError::NotFound(format!(
            "theme pack file not found: {relative_path}"
        )));
    }

    super::error::assert_under_dir(&pack, &file_path, "read_theme_pack_file")?;

    tokio::fs::read(&file_path)
        .await
        .cmd_err("read_theme_pack_file")
}

#[tauri::command]
pub async fn list_theme_packs(
    state: State<'_, AppState>,
) -> Result<Vec<ThemePackSummary>, CommandError> {
    list_theme_packs_impl(&state).await
}

#[tauri::command]
pub async fn install_theme_pack_with_dialog(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ThemePackSummary, CommandError> {
    let picked = app
        .dialog()
        .file()
        .add_filter("Theme pack", &["zip"])
        .blocking_pick_file();
    let Some(fp) = picked else {
        return Err(CommandError::Validation("cancelled".into()));
    };
    let zip_path = fp
        .into_path()
        .map_err(|e| CommandError::Validation(format!("Invalid dialog path: {e}")))?;

    let root = themes_root(&state);
    let manifest = tokio::task::spawn_blocking(move || install_theme_zip(&zip_path, &root))
        .await
        .map_err(|e| CommandError::Internal(format!("install_theme_pack_with_dialog task: {e}")))?
        .map_err(CommandError::Validation)?;

    tracing::info!(id = %manifest.id, "install_theme_pack_with_dialog OK");
    Ok(ThemePackSummary::from(manifest))
}

#[tauri::command]
pub async fn remove_theme_pack(state: State<'_, AppState>, id: String) -> Result<(), CommandError> {
    remove_theme_pack_impl(&state, id).await
}

#[tauri::command]
pub async fn read_theme_pack_file(
    state: State<'_, AppState>,
    id: String,
    relative_path: String,
) -> Result<Vec<u8>, CommandError> {
    read_theme_pack_file_impl(&state, id, relative_path).await
}

#[tauri::command]
pub fn set_surface_background(app: AppHandle, hex: String, dark: bool) -> Result<(), CommandError> {
    let cache = app
        .try_state::<SurfaceBgCache>()
        .ok_or_else(|| CommandError::Internal("surface background cache missing".into()))?;
    cache.set(&hex, dark).map_err(CommandError::Validation)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_path_rejects_escape() {
        assert!(safe_pack_rel_path("tokens.css").is_ok());
        assert!(safe_pack_rel_path("../settings.json").is_err());
        assert!(safe_pack_rel_path("icons/../../x").is_err());
    }

    #[test]
    fn validate_pack_id_rejects_traversal() {
        assert!(validate_pack_id("community.fixture-dark").is_ok());
        assert!(validate_pack_id("../evil").is_err());
        assert!(validate_pack_id("a/b").is_err());
    }

    #[tokio::test]
    async fn theme_pack_list_read_remove() {
        use crate::store::AppSettings;
        use crate::testing::app_state::TestAppState;

        let test = TestAppState::new().await;
        assert!(list_theme_packs_impl(&test.state).await.unwrap().is_empty());

        let pack_root = test
            .state
            .store
            .data_dir()
            .join("themes/community.fixture-dark");
        std::fs::create_dir_all(&pack_root).unwrap();
        std::fs::write(
            pack_root.join("manifest.json"),
            r#"{
  "id": "community.fixture-dark",
  "name": "Fixture Dark",
  "version": "1.0.0",
  "apiVersion": 1,
  "modes": ["dark"]
}"#,
        )
        .unwrap();
        std::fs::write(
            pack_root.join("tokens.css"),
            ":root { --color-accent: #6366f1; }\n",
        )
        .unwrap();

        let packs = list_theme_packs_impl(&test.state).await.unwrap();
        assert_eq!(packs.len(), 1);
        assert_eq!(packs[0].id, "community.fixture-dark");

        let css = read_theme_pack_file_impl(
            &test.state,
            "community.fixture-dark".into(),
            "tokens.css".into(),
        )
        .await
        .unwrap();
        assert!(!css.is_empty());

        let mut settings = AppSettings::default();
        settings.theme.pack_id = Some("community.fixture-dark".into());
        test.state.store.save_settings(settings).await.unwrap();

        remove_theme_pack_impl(&test.state, "community.fixture-dark".into())
            .await
            .unwrap();
        assert!(list_theme_packs_impl(&test.state).await.unwrap().is_empty());
        let settings = test.state.store.get_settings().await;
        assert!(settings.theme.pack_id.is_none());
    }
}
