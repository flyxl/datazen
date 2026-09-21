//! Wapp registry and lifecycle: list, manifest, inspect, install, remove,
//! enable. The commands sit next to the impls they wrap so the whole feature
//! reads as one module.

use super::*;

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
    pub pick_token: String,
    pub package_label: String,
    pub manifest: WappManifest,
}

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
