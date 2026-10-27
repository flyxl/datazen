use super::{AppState, CommandError, State};
use datazen::workspace_apps::Permission;
use serde::Deserialize;

const WAPP_COMMAND_DENYLIST: &[&str] = &[
    "execute",
    "create_database",
    "drop_database",
    "create_schema",
    "drop_schema",
    "create_user",
    "drop_user",
    "grant_privileges",
    "revoke_privileges",
];

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WappDriverCommandRequest {
    pub wapp_id: String,
    pub connection_id: String,
    pub command: String,
    #[serde(default)]
    pub args: serde_json::Value,
}

pub(crate) async fn execute_wapp_driver_command_impl(
    state: &AppState,
    request: WappDriverCommandRequest,
) -> Result<serde_json::Value, CommandError> {
    let wapp = state
        .wapps
        .get(&request.wapp_id)
        .ok_or_else(|| CommandError::NotFound("Wapp is unavailable".into()))?;
    if !wapp.enabled {
        return Err(CommandError::Validation("Wapp is disabled".into()));
    }
    if !wapp
        .manifest
        .permissions
        .contains(&Permission::CommandInvoke)
    {
        return Err(CommandError::Validation(
            "Wapp is missing command:invoke permission".into(),
        ));
    }
    if WAPP_COMMAND_DENYLIST.contains(&request.command.as_str()) {
        return Err(CommandError::Validation(
            "Command is not permitted for Wapps".into(),
        ));
    }
    if !request.args.is_object() {
        return Err(CommandError::Validation(
            "Wapp command args must be an object".into(),
        ));
    }

    tracing::info!(
        wapp_id = %request.wapp_id,
        command = %request.command,
        connection_id = %request.connection_id,
        "Wapp runtime command invoked"
    );
    let source = crate::services::runtime_command_source::RuntimeCommandSource::new(
        state.driver_registry.clone(),
        state.store.clone(),
        state.platform.clone(),
    );
    source
        .read_target_command(&request.connection_id, &request.command, request.args)
        .await
        .map_err(CommandError::Validation)
}

#[tauri::command]
pub async fn execute_wapp_driver_command(
    state: State<'_, AppState>,
    request: WappDriverCommandRequest,
) -> Result<serde_json::Value, CommandError> {
    execute_wapp_driver_command_impl(&state, request).await
}
