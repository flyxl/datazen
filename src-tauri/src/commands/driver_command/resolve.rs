use std::sync::Arc;

use super::super::error::CommandError;
use super::super::AppState;
use super::helpers::{nonempty, unbound_handle};
use datazen_driver_api::{ConnectionHandle, DatabaseDriver};
use datazen_platform_api::dto::session::SessionView;
use datazen_runtime::application::LiveConnectionLease;

pub(crate) struct ResolvedCommandDriver {
    pub(crate) driver: Arc<dyn DatabaseDriver>,
    pub(crate) handle: ConnectionHandle,
    pub(crate) _lease: Option<LiveConnectionLease>,
    pub(crate) session: Option<SessionView>,
    pub(crate) bound: bool,
}

pub(crate) async fn resolve_command_driver(
    state: &AppState,
    db_session_id: Option<&String>,
    driver_type: Option<&String>,
) -> Result<ResolvedCommandDriver, CommandError> {
    if let Some(id) = nonempty(db_session_id) {
        let adapter = state
            .platform_require()
            .map_err(crate::platform::into_command_error)?;
        let call = adapter.begin_call();
        let runtime = adapter
            .runtime()
            .map_err(crate::platform::into_command_error)?;
        let (session, lease) = runtime
            .borrow_live_connection_by_id(
                call.context(),
                id,
                datazen_runtime::application::LivePurpose::CommandExecution,
            )
            .await
            .map_err(|error| {
                if error.code == datazen_application::error::ApiErrorCode::SessionNotFound {
                    CommandError::NotFound(format!("DB session {id} is unavailable"))
                } else {
                    crate::platform::into_command_error(error)
                }
            })?;
        let connection = lease
            .connection()
            .map_err(|error| CommandError::Internal(error.to_string()))?;
        return Ok(ResolvedCommandDriver {
            driver: Arc::clone(connection.driver()),
            handle: connection.handle().clone(),
            _lease: Some(lease),
            session: Some(session),
            bound: true,
        });
    }
    let driver_type = nonempty(driver_type)
        .ok_or_else(|| CommandError::Validation("dbSessionId or driverType is required".into()))?;
    let driver = state
        .driver_registry
        .get(&driver_type.to_string())
        .await
        .ok_or_else(|| CommandError::NotFound(format!("Driver not found: {driver_type}")))?;
    Ok(ResolvedCommandDriver {
        driver,
        handle: unbound_handle(),
        _lease: None,
        session: None,
        bound: false,
    })
}
