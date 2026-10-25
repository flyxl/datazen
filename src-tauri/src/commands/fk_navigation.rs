//! `resolve_foreign_key_targets` — where does this grid cell point? (DB-07)
//!
//! The command shell does three things and nothing else: it lends the shared
//! resolver a live session, it validates the request envelope, and it maps the
//! shared error onto [`CommandError`]. Every rule — which constraint matches,
//! how a composite key pairs, what a NULL means, how a target is resolved —
//! lives in `datazen_feature_core::services::fk_navigation`.

use super::error::CommandError;
use super::AppState;
use datazen_feature_core::services::fk_navigation::{
    ForeignKeyTargetResolution, ResolveForeignKeyTargetsRequest,
};
use datazen_feature_core::services::ports::{
    FkSessionBinding, FkSessionError, ForeignKeyNavigationSource,
};
use datazen_feature_core::services::referenced_by::{
    InboundForeignKeyCatalog, ListInboundForeignKeysRequest,
};
use tauri::State;

/// Lends a live session to the shared foreign-key resolver.
///
/// The binding keeps the runtime's live-connection lease alive for as long as
/// the resolver works, so the driver handle stays usable while the target
/// table is probed.
struct HostNavigationSource<'a> {
    state: &'a AppState,
}

#[async_trait::async_trait]
impl ForeignKeyNavigationSource for HostNavigationSource<'_> {
    async fn bind_session(&self, db_session_id: &str) -> Result<FkSessionBinding, FkSessionError> {
        let resolved = super::driver_command::resolve_command_driver(
            self.state,
            Some(&db_session_id.to_string()),
            None,
        )
        .await
        .map_err(|error| match error {
            CommandError::NotFound(message) => FkSessionError::NotFound(message),
            other => FkSessionError::Unavailable(other.to_string()),
        })?;
        let session = resolved
            .session
            .as_ref()
            .ok_or_else(|| FkSessionError::Unavailable("DB session is unavailable".to_string()))?;
        let config = self
            .state
            .store
            .get_connection(session.connection_id.as_str())
            .await
            .ok_or_else(|| {
                FkSessionError::NotFound("connection profile is unavailable".to_string())
            })?;
        Ok(FkSessionBinding {
            driver: std::sync::Arc::clone(&resolved.driver),
            handle: resolved.handle.clone(),
            database: config.database.clone(),
            schema: config.schema.clone(),
            // The lease keeps the live connection borrowed while the resolver
            // reads the catalog and probes the target table.
            guard: Box::new(resolved._lease),
        })
    }
}

/// Resolve the target of one foreign-key group and probe it for existence.
pub(crate) async fn resolve_foreign_key_targets_impl(
    state: &AppState,
    request: ResolveForeignKeyTargetsRequest,
) -> Result<ForeignKeyTargetResolution, CommandError> {
    if request.db_session_id.trim().is_empty() {
        return Err(CommandError::Validation("dbSessionId is required".into()));
    }
    if request.table.trim().is_empty() {
        return Err(CommandError::Validation("table is required".into()));
    }
    let source = HostNavigationSource { state };
    datazen_feature_core::services::fk_navigation::resolve_foreign_key_targets(&source, &request)
        .await
        .map_err(CommandError::from)
}

/// Resolve the target of one foreign-key group and probe it for existence.
///
/// `exists = false` means the target table is readable but the referenced row
/// is gone — a successful answer, not an error.
#[tauri::command]
pub async fn resolve_foreign_key_targets(
    state: State<'_, AppState>,
    request: ResolveForeignKeyTargetsRequest,
) -> Result<ForeignKeyTargetResolution, CommandError> {
    resolve_foreign_key_targets_impl(&state, request).await
}

/// List the constraints of other tables that reference one table (DB-20).
pub(crate) async fn list_inbound_foreign_keys_impl(
    state: &AppState,
    request: ListInboundForeignKeysRequest,
) -> Result<InboundForeignKeyCatalog, CommandError> {
    if request.db_session_id.trim().is_empty() {
        return Err(CommandError::Validation("dbSessionId is required".into()));
    }
    if request.table.trim().is_empty() {
        return Err(CommandError::Validation("table is required".into()));
    }
    let source = HostNavigationSource { state };
    datazen_feature_core::services::referenced_by::list_inbound_foreign_keys(&source, &request)
        .await
        .map_err(CommandError::from)
}

/// List the constraints of other tables that reference one table (DB-20).
///
/// The scan is bounded and `truncated` says when it stopped early; the front
/// end renders the partial answer, never a silent one.
#[tauri::command]
pub async fn list_inbound_foreign_keys(
    state: State<'_, AppState>,
    request: ListInboundForeignKeysRequest,
) -> Result<InboundForeignKeyCatalog, CommandError> {
    list_inbound_foreign_keys_impl(&state, request).await
}

#[cfg(test)]
mod tests;
