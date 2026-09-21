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
