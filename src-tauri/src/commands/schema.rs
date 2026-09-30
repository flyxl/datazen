use super::error::{CmdExt, CommandError};
use super::AppState;
use crate::db::{TableDataResult, TableSchema};
use crate::services::{metadata_schema, FilterCondition, OrderBy, QueryExecutor, SortCondition};
use std::time::Instant;
use tauri::State;

/// The schema embedded in a qualified table reference (`sales.orders` → `sales`).
///
/// Metadata callers may pass either an explicit `schema` argument or a
/// qualified `table`; the driver resolves the embedded form itself, so the host
/// must not shadow it with a fallback default.
fn embedded_schema_of(table: &str) -> Option<&str> {
    let (prefix, _) = table.rsplit_once('.')?;
    let prefix = prefix.trim().trim_matches('"');
    (!prefix.is_empty()).then_some(prefix)
}

pub(crate) async fn get_table_data_impl(
    state: &AppState,
    db_session_id: String,
    table: String,
    page: u32,
    page_size: u32,
    filters: Option<Vec<FilterCondition>>,
    sorts: Option<Vec<SortCondition>>,
    skip_count: Option<bool>,
    filter_logic: Option<String>,
    database: Option<String>,
    schema: Option<String>,
) -> Result<TableDataResult, CommandError> {
    let start = Instant::now();
    tracing::info!(%db_session_id, %table, page, page_size, "get_table_data");
    let (driver, handle) = state
        .connection_manager
        .get_session(&db_session_id)
        .await
        .cmd_err("get_table_data")?;

    // The caller's explicit target wins; the session's configured database is
    // only a fallback for callers that pass none. Reading `config.database`
    // unconditionally (the old behavior) silently ignored the request and
    // resolved columns against whatever database the session sat on.
    let config = state
        .connection_manager
        .get_session_config(&db_session_id)
        .await
        .cmd_err("get_table_data")?;
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
    // Mirrors the target the driver will read, so a "table does not exist"
    // report can be told apart from a mis-resolved target.
    tracing::info!(
        %db_session_id,
        %table,
        %database,
        resolved_schema = ?schema,
        "get_table_data target"
    );

    let order = sorts
        .and_then(|list| list.into_iter().next())
        .map(|s| OrderBy {
            column: s.column,
            descending: s.descending,
        });

    let effective_skip_count = skip_count.unwrap_or(false) || driver.skip_count_query();

    let executor = QueryExecutor::new(state.schema_cache.clone());
    let result = executor
        .get_table_data(
            &driver,
            &handle,
            &db_session_id,
            database,
            schema.as_deref(),
            &table,
            page,
            page_size,
            filters,
            order,
            effective_skip_count,
            filter_logic.as_deref(),
        )
        .await
        .cmd_err("get_table_data")?;
    tracing::info!(%db_session_id, %table, rows = result.rows.len(), ms = start.elapsed().as_millis() as u64, "get_table_data OK");
    Ok(result)
}

pub(crate) async fn get_er_data_impl(
    state: &AppState,
    db_session_id: String,
    database: String,
    schema: Option<String>,
) -> Result<Vec<TableSchema>, CommandError> {
    let start = Instant::now();
    tracing::info!(%db_session_id, %database, schema = ?schema, "get_er_data");
    let (driver, handle) = state
        .connection_manager
        .get_session(&db_session_id)
        .await
        .cmd_err("get_er_data")?;

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
        match state
            .schema_cache
            .get_table_schema(
                &db_session_id,
                &database,
                table_schema.as_deref(),
                &table.name,
                &driver,
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

fn parse_objects_from_command(
    data: &serde_json::Value,
) -> Vec<crate::schema_objects::DatabaseObject> {
    data.get("objects")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

fn parse_grants_from_command(
    data: &serde_json::Value,
) -> Vec<crate::schema_objects::PrivilegeGrant> {
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
) -> Result<Vec<crate::schema_objects::DatabaseObject>, CommandError> {
    if crate::schema_objects::ObjectKind::parse(&kind).is_none() {
        return Err(CommandError::Validation(format!(
            "Unknown object kind: {kind}"
        )));
    }
    let data = run_schema_object_command(
        state,
        &db_session_id,
        "list_objects",
        serde_json::json!({ "kind": kind }),
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
    get_object_ddl_with_metadata_impl(state, db_session_id, kind, name, schema, None, None, None)
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
) -> Result<String, CommandError> {
    if crate::schema_objects::ObjectKind::parse(&kind).is_none() {
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
) -> Result<Vec<crate::schema_objects::PrivilegeGrant>, CommandError> {
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
) -> Result<Vec<crate::schema_objects::DatabaseObject>, CommandError> {
    get_database_objects_impl(&state, db_session_id, kind).await
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
    )
    .await
}

#[tauri::command]
pub async fn get_privileges(
    state: State<'_, AppState>,
    db_session_id: String,
) -> Result<Vec<crate::schema_objects::PrivilegeGrant>, CommandError> {
    get_privileges_impl(&state, db_session_id).await
}

#[tauri::command]
pub async fn get_table_data(
    state: State<'_, AppState>,
    db_session_id: String,
    table: String,
    page: u32,
    page_size: u32,
    filters: Option<Vec<FilterCondition>>,
    sorts: Option<Vec<SortCondition>>,
    skip_count: Option<bool>,
    filter_logic: Option<String>,
    database: Option<String>,
    schema: Option<String>,
) -> Result<TableDataResult, CommandError> {
    get_table_data_impl(
        &state,
        db_session_id,
        table,
        page,
        page_size,
        filters,
        sorts,
        skip_count,
        filter_logic,
        database,
        schema,
    )
    .await
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
mod tests;
