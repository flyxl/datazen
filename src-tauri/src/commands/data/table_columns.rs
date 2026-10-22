//! Host adapter for the shared catalog port.
//!
//! `datazen_feature_core::data_browsing` decides that a plan must be checked
//! against the real primary key and column list; it must not know how a given
//! host reads a catalog. This adapter answers that question with the same
//! schema cache and driver resolution the grid read uses, so a preview after a
//! page load is normally a pure cache hit.

use super::*;
use async_trait::async_trait;
use datazen_feature_core::data_browsing::TableColumnSource;
use datazen_feature_core::error::FeatureError;
use datazen_runtime::schema_cache::CachedColumns;

pub(super) struct HostTableColumns<'a> {
    state: &'a AppState,
}

impl<'a> HostTableColumns<'a> {
    pub(super) fn new(state: &'a AppState) -> Self {
        Self { state }
    }
}

fn into_feature_error(error: CommandError) -> FeatureError {
    match error {
        CommandError::Validation(message) => FeatureError::Validation(message),
        CommandError::NotFound(message) => FeatureError::NotFound(message),
        CommandError::NotConfigured(message) => FeatureError::NotConfigured(message),
        CommandError::Internal(message) => FeatureError::Internal(message),
        CommandError::Driver(error) => FeatureError::Driver(error),
        CommandError::Json(error) => FeatureError::Json(error),
        CommandError::Io(error) => FeatureError::Io(error),
        CommandError::Ai(error) => FeatureError::Ai(error),
        other => FeatureError::Internal(other.to_string()),
    }
}

#[async_trait]
impl TableColumnSource for HostTableColumns<'_> {
    async fn table_columns(
        &self,
        table: &RowChangeTableContext,
    ) -> Result<CachedColumns, FeatureError> {
        load_table_columns(self.state, table)
            .await
            .map_err(into_feature_error)
    }
}

/// Load the table's columns and primary key through the shared schema cache.
///
/// The cache is keyed like the grid's data read (`dbSessionId` + database +
/// schema), so a preview after a page load is usually a pure cache hit and
/// opening the ChangeSet editor does not add a catalog round trip. A cache miss
/// reads through the driver's catalog seam with the session that owns the shown
/// page.
///
/// Every failure mode is reported as [`CommandError::Validation`] with the
/// table name and the reason: the plan asked for a guarded catalog, and the
/// guard cannot answer "matches the primary key" from an unreadable schema.
async fn load_table_columns(
    state: &AppState,
    table: &RowChangeTableContext,
) -> Result<CachedColumns, CommandError> {
    let database = table
        .database
        .as_deref()
        .map(str::trim)
        .filter(|database| !database.is_empty())
        .ok_or_else(|| {
            CommandError::Validation(format!(
                "Table schema unavailable for '{}': the change context carries no database, so the primary key cannot be verified",
                table.table
            ))
        })?;
    // A driver without a schema level rejects any schema argument instead of
    // ignoring it (`public.users` is not a thing on MySQL), so resolve the
    // effective schema from the driver's own declaration first.
    let driver = state
        .driver_registry
        .get_sql_driver_by_name(&table.driver_type)
        .await
        .ok_or_else(|| CommandError::NotConfigured("Driver unavailable".into()))?;
    let schema = table
        .schema
        .as_deref()
        .map(str::trim)
        .filter(|schema| !schema.is_empty() && driver.has_schema_level());
    if let Some(cached) = state
        .schema_cache
        .try_get_cached_columns(&table.db_session_id, database, schema, &table.table)
        .await
    {
        return Ok(cached);
    }
    let resolved = crate::commands::driver_command::resolve_command_driver(
        state,
        Some(&table.db_session_id),
        Some(&table.driver_type),
    )
    .await?;
    let catalog = datazen_driver_api::LegacyCatalogReader::new(resolved.driver.as_ref());
    let columns = state
        .schema_cache
        .get_columns(
            &table.db_session_id,
            database,
            schema,
            &table.table,
            &catalog,
            &resolved.handle,
        )
        .await
        .map_err(|error| {
            CommandError::Validation(format!(
                "Table schema unavailable for '{}': {error}; the primary key cannot be verified",
                table.table
            ))
        })?;
    if columns.columns.is_empty() {
        return Err(CommandError::Validation(format!(
            "Table schema unavailable for '{}': the driver reported no columns, so the primary key cannot be verified",
            table.table
        )));
    }
    Ok(columns)
}
