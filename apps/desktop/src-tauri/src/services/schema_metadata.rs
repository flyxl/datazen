//! Desktop compatibility adapter for portable metadata reads.
use crate::commands::{AppState, CommandError};
pub use datazen::services::schema_metadata::{command_definitions, is_metadata_command};
use datazen_driver_api::{CommandResult, ConnectionHandle, DatabaseDriver};
use serde_json::Value;
use std::sync::Arc;
pub async fn execute(
    state: &AppState,
    session: &str,
    driver: &Arc<dyn DatabaseDriver>,
    handle: &ConnectionHandle,
    command: &str,
    input: Value,
) -> Result<CommandResult, CommandError> {
    datazen::services::schema_metadata::execute(
        &state.schema_cache,
        session,
        driver,
        handle,
        command,
        input,
    )
    .await
    .map_err(Into::into)
}
pub async fn execute_on_runtime_resource(
    driver: Arc<dyn DatabaseDriver>,
    handle: &ConnectionHandle,
    command: &str,
    input: Value,
) -> Result<CommandResult, CommandError> {
    datazen::services::schema_metadata::execute_on_runtime_resource(driver, handle, command, input)
        .await
        .map_err(Into::into)
}
#[cfg(test)]
use datazen_driver_api::schema_metadata::*;
#[cfg(test)]
use serde_json::json;
#[cfg(test)]
mod tests;
