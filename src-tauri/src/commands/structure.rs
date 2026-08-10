//! Table structure editor IPC — delegates to driver `structure_capabilities` /
//! `plan_structure_changes` (no host-side capability registry).

use super::error::{CmdExt, CommandError};
use super::AppState;
use crate::db::{StructureCapabilities, StructureChangePlan, StructureChangeRequest};
use std::time::Instant;
use tauri::State;

#[tauri::command]
pub async fn get_structure_capabilities(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<StructureCapabilities, CommandError> {
    let start = Instant::now();
    tracing::info!(%connection_id, "get_structure_capabilities");
    let (driver, handle) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .cmd_err("get_structure_capabilities")?;

    let caps = driver
        .structure_capabilities(&handle)
        .await
        .cmd_err("get_structure_capabilities")?;
    tracing::info!(
        %connection_id,
        dialect = %caps.dialect_id,
        ms = start.elapsed().as_millis() as u64,
        "get_structure_capabilities OK"
    );
    Ok(caps)
}

#[tauri::command]
pub async fn plan_table_structure_changes(
    state: State<'_, AppState>,
    connection_id: String,
    request: StructureChangeRequest,
) -> Result<StructureChangePlan, CommandError> {
    let start = Instant::now();
    tracing::info!(
        %connection_id,
        mode = ?request.mode,
        schema = ?request.schema,
        table = %request.table,
        "plan_table_structure_changes"
    );
    let (driver, handle) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .cmd_err("plan_table_structure_changes")?;

    let plan = driver
        .plan_structure_changes(&handle, &request)
        .await
        .cmd_err("plan_table_structure_changes")?;
    tracing::info!(
        %connection_id,
        statements = plan.statements.len(),
        ms = start.elapsed().as_millis() as u64,
        "plan_table_structure_changes OK"
    );
    Ok(plan)
}
