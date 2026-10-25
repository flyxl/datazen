//! `search_table_values` — the statement side of the full-database value
//! search (DB-19).
//!
//! The command shell does three things and nothing else: it lends the shared
//! search a live session (resolving the driver, the handle and the
//! database/schema fallback the grid's own reads use), it validates the
//! request envelope, and it maps the shared error onto [`CommandError`]. The
//! statement rules — how the value is bound, how the wildcards are escaped —
//! live in `datazen_feature_core::services::value_search`.
//!
//! The **scan** is the front end's: one command per table, because that is
//! what makes pause and resume real (pausing is not issuing the next
//! command). This host command never iterates on its own.

use super::error::CommandError;
use super::AppState;
use datazen_feature_core::services::schema_scope::{embedded_schema_of, metadata_schema};
use datazen_feature_core::services::value_search::{
    search_table_values, SearchTableValuesRequest, SearchTableValuesResult,
};
use tauri::State;

pub(crate) async fn search_table_values_impl(
    state: &AppState,
    request: SearchTableValuesRequest,
) -> Result<SearchTableValuesResult, CommandError> {
    if request.table.trim().is_empty() {
        return Err(CommandError::Validation("table is required".into()));
    }
    if request.value.trim().is_empty() {
        return Err(CommandError::Validation("value is required".into()));
    }

    let resolved = super::driver_command::resolve_command_driver(
        state,
        Some(&request.db_session_id),
        None,
    )
    .await?;
    let session = resolved
        .session
        .as_ref()
        .ok_or_else(|| CommandError::NotFound("DB session is unavailable".into()))?;
    let config = state
        .store
        .get_connection(session.connection_id.as_str())
        .await
        .ok_or_else(|| {
            CommandError::NotFound("connection profile is unavailable".to_string())
        })?;

    // The same fallback ladder the grid's own reads climb, so a searched
    // table and an opened one can never land on two scopes.
    let database = request
        .database
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| config.database.as_deref().map(str::to_string))
        .unwrap_or_else(|| "default".to_string());
    let schema = metadata_schema(
        resolved.driver.as_ref(),
        request.schema.as_deref(),
        embedded_schema_of(&request.table),
        config.schema.as_deref(),
    );

    let result = search_table_values(
        resolved.driver.as_ref(),
        &resolved.handle,
        &database,
        schema.as_deref(),
        &request,
    )
    .await
    .map_err(|error| CommandError::Validation(error.to_string()))?;
    Ok(result)
}

/// Count the rows of one table whose searchable columns match a value (DB-19).
///
/// One command per table; the front end drives the scan and owns pause and
/// resume. The value is bound wherever the dialect owns a bound-parameter
/// vocabulary — it never reaches the SQL text on such a driver.
#[tauri::command]
pub async fn search_table_values(
    state: State<'_, AppState>,
    request: SearchTableValuesRequest,
) -> Result<SearchTableValuesResult, CommandError> {
    search_table_values_impl(&state, request).await
}
