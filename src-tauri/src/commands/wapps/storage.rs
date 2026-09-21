//! Wapp sandbox storage (per-wapp key/value) and file reads.

use super::*;

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

    super::super::error::assert_under_dir(&wapp_dir, &file_path, "read_wapp_file")?;

    tokio::fs::read(&file_path).await.cmd_err("read_wapp_file")
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
