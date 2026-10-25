use super::error::{CmdExt, CommandError};
use super::AppState;
use crate::services::{embedded_schema_of, metadata_schema, QueryExecutor};
use datazen_driver_api::schema_objects::{DatabaseObject, ObjectKind, PrivilegeGrant};
use datazen_driver_api::TableSchema;
use std::time::Instant;
use tauri::State;

pub(crate) async fn get_er_data_impl(
    state: &AppState,
    db_session_id: String,
    database: String,
    schema: Option<String>,
) -> Result<Vec<TableSchema>, CommandError> {
    let start = Instant::now();
    tracing::info!(%db_session_id, %database, schema = ?schema, "get_er_data");
    let resolved =
        super::driver_command::resolve_command_driver(state, Some(&db_session_id), None).await?;
    let driver = &resolved.driver;
    let handle = &resolved.handle;

    // No session pin and no `USE`: every read below carries its own explicit
    // target, and each table's own schema (from `TableInfo`) is authoritative
    // over the caller's filter — a filter selects *which* tables to draw, it
    // does not relocate the ones that came back.
    let tables = driver
        .get_tables(&handle, &database, schema.as_deref())
        .await
        .cmd_err("get_er_data")?;

    let mut schemas = Vec::with_capacity(tables.len());
    for table in &tables {
        let table_schema = metadata_schema(
            driver.as_ref(),
            schema.as_deref(),
            table.schema.as_deref(),
            None,
        );
        let catalog = datazen_driver_api::LegacyCatalogReader::new(driver.as_ref());
        match state
            .schema_cache
            .get_table_schema(
                &db_session_id,
                &database,
                table_schema.as_deref(),
                &table.name,
                &catalog,
                &handle,
            )
            .await
        {
            Ok(schema) => schemas.push(schema),
            Err(e) => {
                tracing::warn!(table = %table.name, error = %e, "get_er_data: skipping table");
            }
        }
    }

    if !tables.is_empty() && schemas.is_empty() {
        return Err(CommandError::Internal(format!(
            "get_er_data: failed to read column metadata for all {} tables in {}",
            tables.len(),
            database
        )));
    }

    tracing::info!(
        %db_session_id, %database,
        tables = schemas.len(),
        ms = start.elapsed().as_millis() as u64,
        "get_er_data OK"
    );
    Ok(schemas)
}

fn parse_objects_from_command(data: &serde_json::Value) -> Vec<DatabaseObject> {
    data.get("objects")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

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

pub(crate) async fn get_database_objects_impl(
    state: &AppState,
    db_session_id: String,
    kind: String,
    database: Option<String>,
) -> Result<Vec<DatabaseObject>, CommandError> {
    if ObjectKind::parse(&kind).is_none() {
        return Err(CommandError::Validation(format!(
            "Unknown object kind: {kind}"
        )));
    }
    let data = run_schema_object_command(
        state,
        &db_session_id,
        "list_objects",
        serde_json::json!({ "kind": kind, "database": database }),
    )
    .await?;
    Ok(parse_objects_from_command(&data))
}

pub(crate) async fn get_object_ddl_impl(
    state: &AppState,
    db_session_id: String,
    kind: String,
    name: String,
    schema: Option<String>,
) -> Result<String, CommandError> {
    get_object_ddl_with_metadata_impl(
        state,
        db_session_id,
        kind,
        name,
        schema,
        None,
        None,
        None,
        None,
    )
    .await
}

pub(crate) async fn get_object_ddl_with_metadata_impl(
    state: &AppState,
    db_session_id: String,
    kind: String,
    name: String,
    schema: Option<String>,
    signature: Option<String>,
    target_schema: Option<String>,
    target_name: Option<String>,
    database: Option<String>,
) -> Result<String, CommandError> {
    if ObjectKind::parse(&kind).is_none() {
        return Err(CommandError::Validation(format!(
            "Unknown object kind: {kind}"
        )));
    }
    let data = run_schema_object_command(
        state,
        &db_session_id,
        "get_object_ddl",
        serde_json::json!({
            "kind": kind,
            "name": name,
            "schema": schema,
            "signature": signature,
            "targetSchema": target_schema,
            "targetName": target_name,
            "database": database,
        }),
    )
    .await?;
    Ok(data
        .get("ddl")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string())
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
pub async fn get_database_objects(
    state: State<'_, AppState>,
    db_session_id: String,
    kind: String,
    database: Option<String>,
) -> Result<Vec<DatabaseObject>, CommandError> {
    get_database_objects_impl(&state, db_session_id, kind, database).await
}

#[tauri::command]
pub async fn get_object_ddl(
    state: State<'_, AppState>,
    db_session_id: String,
    kind: String,
    name: String,
    schema: Option<String>,
    signature: Option<String>,
    target_schema: Option<String>,
    target_name: Option<String>,
    database: Option<String>,
) -> Result<String, CommandError> {
    get_object_ddl_with_metadata_impl(
        &state,
        db_session_id,
        kind,
        name,
        schema,
        signature,
        target_schema,
        target_name,
        database,
    )
    .await
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

#[tauri::command]
pub async fn get_er_data(
    state: State<'_, AppState>,
    db_session_id: String,
    database: String,
    schema: Option<String>,
) -> Result<Vec<TableSchema>, CommandError> {
    get_er_data_impl(&state, db_session_id, database, schema).await
}

#[cfg(test)]
mod count_tests;
#[cfg(test)]
mod tests;
