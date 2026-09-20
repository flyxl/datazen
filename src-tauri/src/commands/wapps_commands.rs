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
