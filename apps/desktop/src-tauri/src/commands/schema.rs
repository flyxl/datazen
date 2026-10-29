use super::error::{CmdExt, CommandError};
use super::AppState;
use crate::services::{embedded_schema_of, metadata_schema, QueryExecutor};
use datazen_driver_api::schema_objects::PrivilegeGrant;
use std::time::Instant;
use tauri::State;

fn parse_grants_from_command(data: &serde_json::Value) -> Vec<PrivilegeGrant> {
    data.get("grants")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

async fn run_schema_object_command(
    state: &AppState,
    db_session_id: &str,
    command: &str,
    input: serde_json::Value,
) -> Result<serde_json::Value, CommandError> {
    let result = super::driver_command::execute_driver_command_impl(
        state,
        super::driver_command::ExecuteDriverCommandRequest {
            db_session_id: Some(db_session_id.to_string()),
            driver_type: None,
            command: command.to_string(),
            input,
            database: None,
            schema: None,
        },
    )
    .await?;
    Ok(result.data)
}

pub(crate) async fn get_privileges_impl(
    state: &AppState,
    db_session_id: String,
) -> Result<Vec<PrivilegeGrant>, CommandError> {
    let data = run_schema_object_command(
        state,
        &db_session_id,
        "list_privileges",
        serde_json::json!({}),
    )
    .await?;
    Ok(parse_grants_from_command(&data))
}

#[tauri::command]
pub async fn get_privileges(
    state: State<'_, AppState>,
    db_session_id: String,
) -> Result<Vec<PrivilegeGrant>, CommandError> {
    get_privileges_impl(&state, db_session_id).await
}

/// The estimated number of rows in a table.
///
/// `Ok(None)` means "this database cannot estimate this table" — an answer,
/// not a failure. The command never falls back to an exact count behind the
/// caller's back: deciding to spend that work is the caller's business.
pub(crate) async fn estimate_table_rows_impl(
    state: &AppState,
    db_session_id: String,
    table: String,
    database: Option<String>,
    schema: Option<String>,
) -> Result<Option<i64>, CommandError> {
    let start = Instant::now();
    tracing::info!(%db_session_id, %table, "estimate_table_rows");
    let resolved =
        super::driver_command::resolve_command_driver(state, Some(&db_session_id), None).await?;
    let driver = &resolved.driver;
    let handle = &resolved.handle;

    let config = resolved
        .session
        .as_ref()
        .ok_or_else(|| CommandError::NotFound("DB session is unavailable".into()))?;
    let config = state
        .store
        .get_connection(config.connection_id.as_str())
        .await
        .ok_or_else(|| CommandError::NotFound("connection profile is unavailable".into()))?;
    let database = database
        .as_deref()
        .map(str::trim)
        .filter(|db| !db.is_empty())
        .or(config.database.as_deref())
        .unwrap_or("default");
    let schema = metadata_schema(
        driver.as_ref(),
        schema.as_deref(),
        embedded_schema_of(&table),
        config.schema.as_deref(),
    );

    let executor = QueryExecutor::new(state.schema_cache.clone());
    let estimate = executor
        .estimate_row_count(&driver, &handle, database, schema.as_deref(), &table)
        .await
        .cmd_err("estimate_table_rows")?;
    tracing::info!(
        %db_session_id,
        %table,
        estimated = ?estimate,
        ms = start.elapsed().as_millis() as u64,
        "estimate_table_rows OK"
    );
    Ok(estimate)
}

#[tauri::command]
pub async fn estimate_table_rows(
    state: State<'_, AppState>,
    db_session_id: String,
    table: String,
    database: Option<String>,
    schema: Option<String>,
) -> Result<Option<i64>, CommandError> {
    estimate_table_rows_impl(&state, db_session_id, table, database, schema).await
}

#[cfg(test)]
mod count_tests;
#[cfg(test)]
mod tests;
