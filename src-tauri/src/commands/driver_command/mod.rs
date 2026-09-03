//! Driver Command IPC — discovery, validation, execute, and streaming.

mod access;
mod discovery;
mod execute;
mod helpers;
mod resolve;
mod streaming;
mod types;

#[cfg(test)]
mod tests;

pub(crate) use access::access_level_for_mode;
pub(crate) use discovery::{get_connection_commands_impl, get_driver_type_commands_impl};
pub(crate) use execute::{execute_driver_command_impl, execute_driver_command_with_mode};
pub(crate) use streaming::execute_driver_command_stream_impl;
pub use types::{
    ExecuteDriverCommandRequest, ExecuteDriverCommandStreamOpts, ExecuteDriverCommandStreamRequest,
};

use std::sync::Arc;

use super::error::CommandError;
use super::AppState;
use datazen_driver_api::{CommandResult, DriverCommandDefinition, QueryStreamCallback, QueryStreamEvent};
use tauri::ipc::Channel;
use tauri::State;

/// Discover commands from a Driver type. No live Connection is required.
#[tauri::command]
pub async fn get_driver_commands(
    state: State<'_, AppState>,
    driver_type: String,
) -> Result<Vec<DriverCommandDefinition>, CommandError> {
    discovery::get_driver_type_commands_impl(&state, driver_type).await
}

/// Discover commands from a concrete live DB session.
#[tauri::command]
pub async fn get_connection_commands(
    state: State<'_, AppState>,
    db_session_id: String,
) -> Result<Vec<DriverCommandDefinition>, CommandError> {
    discovery::get_connection_commands_impl(&state, db_session_id).await
}

#[tauri::command]
pub async fn execute_driver_command(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: ExecuteDriverCommandRequest,
) -> Result<CommandResult, CommandError> {
    // GUI IPC passes the AppHandle so metadata-declared save dialogs can run.
    execute::execute_driver_command_with_mode(&state, request, None, Some(&app)).await
}

#[tauri::command]
pub async fn execute_driver_command_stream(
    state: State<'_, AppState>,
    request: ExecuteDriverCommandStreamRequest,
    on_event: Channel<QueryStreamEvent>,
    apply_result_limit: Option<bool>,
    record_history: Option<bool>,
) -> Result<(), CommandError> {
    let callback: QueryStreamCallback = Arc::new(move |event| {
        let _ = on_event.send(event);
    });
    streaming::execute_driver_command_stream_impl(
        &state,
        request,
        callback,
        ExecuteDriverCommandStreamOpts {
            apply_result_limit: apply_result_limit.unwrap_or(true),
            record_history: record_history.unwrap_or(true),
        },
    )
    .await
}
