//! `resolve_result_origin` / `preview_result_cell_change` — the write-back gate
//! of the query result grid (DB-14).
//!
//! The command shell does three things and nothing else: it lends the shared
//! rules a live session and the catalog's answer about the source table, it
//! validates the request envelope, and it maps the shared error onto
//! [`CommandError`]. Every rule — what makes a result writable, which request
//! is refused before any SQL exists, what a write-back plan looks like — lives
//! in `datazen::services::result_edit`.
//!
//! Two facts are read here rather than computed here, and both are read from
//! the same place the rest of the grid reads them: the **primary key** comes
//! from the catalog cache (the normalised list, never a re-derivation from the
//! column flags), and the **read-only flag** comes from the connection
//! profile. Neither is guessed.

use datazen::data_browsing::RowChangePlan;
use datazen::schema::cache::CachedColumns;
use datazen::services::result_edit::{
    build_result_cell_change_plan, evaluate_result_editability, read_only_error_code,
    GridReadOnlyReason, ResultCellChangeRequest, ResultEditability,
};
use datazen_driver_api::{ConnectionConfig, LegacyCatalogReader, ResultOrigin, SqlTarget};
use serde::{Deserialize, Serialize};
use tauri::State;

use super::driver_command::{resolve_command_driver, ResolvedCommandDriver};
use super::error::CommandError;
use super::AppState;

/// Ask where a result came from, and whether it may be written back.
///
/// The front end calls this once per result, for rendering. The answer is a
/// verdict plus a reason — never a bare "no".
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveResultOriginRequest {
    /// The session the result was read from.
    ///
    /// The connection is **not** part of the request: it is resolved from the
    /// session, so every surface of a result can ask the same question
    /// without knowing which connection the session belongs to.
    pub db_session_id: String,
    /// The database the result was read from.
    pub database: Option<String>,
    /// The schema the result was read from.
    pub schema: Option<String>,
    /// The statement the result came from.
    pub sql: String,
    /// The column names the executed result reports.
    pub result_columns: Vec<String>,
    /// The real name of the relation the result was read from, as the narrow
    /// host-side mapping check named it.
    ///
    /// It is what lets the catalog be read on the narrow channel: when the
    /// driver cannot describe the result, this is the only handle on the
    /// source table — and the catalog is where the locating key columns come
    /// from.
    pub table: Option<String>,
}

/// Ask where a result came from, and whether it may be written back.
///
/// The front end calls this once per result, for rendering. The answer is a
/// verdict plus a reason — never a bare "no".
#[tauri::command]
pub async fn resolve_result_origin(
    state: State<'_, AppState>,
    request: ResolveResultOriginRequest,
) -> Result<ResultEditability, CommandError> {
    resolve_result_origin_impl(&state, request).await
}

pub(crate) async fn resolve_result_origin_impl(
    state: &AppState,
    request: ResolveResultOriginRequest,
) -> Result<ResultEditability, CommandError> {
    validate_envelope(&request.db_session_id, &request.sql)?;
    let resolved = resolve_command_driver(state, Some(&request.db_session_id), None).await?;
    let profile = session_profile(state, &resolved).await?;
    let (database, schema) = sql_target(
        &resolved,
        &profile,
        request.database.as_deref(),
        request.schema.as_deref(),
    );
    let target = SqlTarget::new(database.as_deref(), schema.as_deref());
    let origin = describe_origin(&resolved, target, &request.result_columns, &request.sql).await?;
    let columns = source_table_columns(
        state,
        &resolved,
        &profile,
        request.database.as_deref(),
        request.schema.as_deref(),
        &origin,
        request.table.as_deref(),
    )
    .await;
    let primary_keys = columns
        .as_ref()
        .map(|columns| columns.primary_keys.clone())
        .unwrap_or_default();
    Ok(evaluate_result_editability(
        &origin,
        &primary_keys,
        &request.result_columns,
        !profile.read_only,
    ))
}

/// Build the change plan for one edited cell of a query result.
///
/// The origin is read **again** here: a verdict that was computed for the
/// rendering may be stale, and a write must never be based on a cached "yes".
#[tauri::command]
pub async fn preview_result_cell_change(
    state: State<'_, AppState>,
    request: ResultCellChangeRequest,
) -> Result<RowChangePlan, CommandError> {
    preview_result_cell_change_impl(&state, request).await
}

pub(crate) async fn preview_result_cell_change_impl(
    state: &AppState,
    request: ResultCellChangeRequest,
) -> Result<RowChangePlan, CommandError> {
    validate_envelope(&request.db_session_id, &request.sql)?;
    if request.table.trim().is_empty() {
        return Err(CommandError::Validation(
            "the change request names no table".into(),
        ));
    }
    if request.column.trim().is_empty() {
        return Err(CommandError::Validation(
            "the change request names no column".into(),
        ));
    }
    let resolved = resolve_command_driver(state, Some(&request.db_session_id), None).await?;
    let profile = connection_profile(state, &request.connection_id).await?;
    let (database, schema) = sql_target(
        &resolved,
        &profile,
        request.database.as_deref(),
        request.schema.as_deref(),
    );
    let target = SqlTarget::new(database.as_deref(), schema.as_deref());
    let origin = describe_origin(&resolved, target, &request.result_columns, &request.sql).await?;
    let columns = source_table_columns(
        state,
        &resolved,
        &profile,
        request.database.as_deref(),
        request.schema.as_deref(),
        &origin,
        Some(request.table.as_str()),
    )
    .await
    .ok_or_else(|| {
        CommandError::Validation(format!(
            "{}: the source table's columns cannot be read, so the write cannot be verified",
            read_only_error_code(GridReadOnlyReason::UnknownOrigin)
        ))
    })?;
    let verdict = evaluate_result_editability(
        &origin,
        &columns.primary_keys,
        &request.result_columns,
        !profile.read_only,
    );
    if !verdict.writable {
        let reason = verdict.reason.unwrap_or(GridReadOnlyReason::UnknownOrigin);
        let code = read_only_error_code(reason);
        return Err(CommandError::Validation(format!(
            "{code}: this result cannot be written back ({reason:?})"
        )));
    }
    let mut request = request;
    // The effective target is resolved here, so the plan names the database
    // and schema the result was really read from.
    request.database = database.clone().or_else(|| profile.database.clone());
    request.schema = schema.clone().or_else(|| profile.schema.clone());
    request.driver_type = profile.database_type.clone();
    build_result_cell_change_plan(resolved.driver.as_ref(), &request, &verdict, &columns)
        .map_err(CommandError::from)
}

/// Ask the driver where the rows came from.
///
/// A driver that cannot answer is not an error of this command: the verdict
/// says "read-only" and the reason says why.
async fn describe_origin(
    resolved: &ResolvedCommandDriver,
    target: SqlTarget<'_>,
    result_columns: &[String],
    sql: &str,
) -> Result<ResultOrigin, CommandError> {
    resolved
        .driver
        .describe_result_origin(&resolved.handle, target, result_columns, sql)
        .await
        .map_err(|error| {
            CommandError::Validation(format!(
                "{}: the driver cannot describe where this result came from ({error})",
                read_only_error_code(GridReadOnlyReason::UnknownOrigin)
            ))
        })
}

/// Read the source table's columns — and with them its key — from the catalog.
///
/// `None` means the catalog could not answer. The caller treats that as
/// "nothing is provable", never as "the table has no key".
async fn source_table_columns(
    state: &AppState,
    resolved: &ResolvedCommandDriver,
    profile: &ConnectionConfig,
    database: Option<&str>,
    schema: Option<&str>,
    origin: &ResultOrigin,
    narrow_table: Option<&str>,
) -> Option<CachedColumns> {
    // The driver's origin names the relation when it can; the narrow mapping
    // check names it when it cannot. Either way the catalog is the authority
    // for the key columns.
    let table = origin_table_name(origin)
        .or_else(|| narrow_table.map(str::to_string))
        .filter(|name| !name.trim().is_empty())?;
    let database = nonblank(database).or_else(|| nonblank(profile.database.as_deref()))?;
    // A driver without a schema level rejects any schema argument instead of
    // ignoring it, so the effective schema comes from the driver's own
    // declaration first.
    let schema = nonblank(schema)
        .filter(|_| resolved.driver.has_schema_level())
        .or_else(|| {
            nonblank(profile.schema.as_deref()).filter(|_| resolved.driver.has_schema_level())
        });
    let catalog = LegacyCatalogReader::new(resolved.driver.as_ref());
    state
        .schema_cache
        .get_columns(
            &profile.id,
            database,
            schema,
            &table,
            &catalog,
            &resolved.handle,
        )
        .await
        .ok()
}

/// The relation the result came from, when the driver named one.
fn origin_table_name(origin: &ResultOrigin) -> Option<String> {
    match origin {
        ResultOrigin::Determined(detail) => detail
            .table
            .as_ref()
            .map(|table| table.name.clone())
            .filter(|name| !name.trim().is_empty()),
        ResultOrigin::Undeterminable(_) => None,
    }
}

/// The target the statement was read from.
///
/// The request wins over the profile: a query panel may read from a database
/// other than the connection's default. The parts are returned owned because
/// the target borrows them, and the caller owns them for the whole call.
fn sql_target(
    resolved: &ResolvedCommandDriver,
    profile: &ConnectionConfig,
    database: Option<&str>,
    schema: Option<&str>,
) -> (Option<String>, Option<String>) {
    let database = nonblank(database)
        .or_else(|| nonblank(profile.database.as_deref()))
        .map(str::to_string);
    let schema = nonblank(schema)
        .filter(|_| resolved.driver.has_schema_level())
        .or_else(|| {
            nonblank(profile.schema.as_deref()).filter(|_| resolved.driver.has_schema_level())
        })
        .map(str::to_string);
    (database, schema)
}

fn nonblank(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

/// The connection the session belongs to.
///
/// The session is the one that knows: the request names the session, and the
/// session names its connection.
async fn session_profile(
    state: &AppState,
    resolved: &ResolvedCommandDriver,
) -> Result<ConnectionConfig, CommandError> {
    let connection_id = resolved
        .session
        .as_ref()
        .map(|session| session.connection_id.as_str().to_string())
        .ok_or_else(|| {
            CommandError::NotFound("the DB session does not name a connection".into())
        })?;
    connection_profile(state, &connection_id).await
}

/// The connection the result was read from.
async fn connection_profile(
    state: &AppState,
    connection_id: &str,
) -> Result<ConnectionConfig, CommandError> {
    state
        .store
        .get_connection(connection_id)
        .await
        .ok_or_else(|| {
            CommandError::NotFound(format!(
                "connection profile '{connection_id}' is unavailable"
            ))
        })
}

/// The request envelope every result-origin command shares.
fn validate_envelope(db_session_id: &str, sql: &str) -> Result<(), CommandError> {
    if db_session_id.trim().is_empty() {
        return Err(CommandError::Validation("dbSessionId is required".into()));
    }
    if sql.trim().is_empty() {
        return Err(CommandError::Validation("sql is required".into()));
    }
    Ok(())
}
