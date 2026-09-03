use std::sync::Arc;

use super::helpers::{nonempty, unbound_handle};
use super::super::error::{CmdExt, CommandError};
use super::super::AppState;
use datazen_driver_api::{ConnectionHandle, DatabaseDriver};

pub(crate) async fn resolve_command_driver(
    state: &AppState,
    db_session_id: Option<&String>,
    driver_type: Option<&String>,
) -> Result<(Arc<dyn DatabaseDriver>, ConnectionHandle, bool), CommandError> {
    if let Some(id) = nonempty(db_session_id) {
        let (driver, handle) = state
            .connection_manager
            .get_session(id)
            .await
            .cmd_err("execute_driver_command")?;
        return Ok((driver, handle, true));
    }
    let driver_type = nonempty(driver_type)
        .ok_or_else(|| CommandError::Validation("dbSessionId or driverType is required".into()))?;
    let driver = state
        .driver_registry
        .get(&driver_type.to_string())
        .await
        .ok_or_else(|| CommandError::NotFound(format!("Driver not found: {driver_type}")))?;
    Ok((driver, unbound_handle(), false))
}
