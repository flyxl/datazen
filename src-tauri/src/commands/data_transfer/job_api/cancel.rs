//! Cancellation against the shared durable Job host.

use datazen_platform_api::id::JobId;
use datazen_platform_api::PortError;

use crate::commands::error::CommandError;
use crate::commands::AppState;

use super::runtime::{admit_error, request_context};

pub(crate) async fn cancel_data_transfer_job(
    state: &AppState,
    job_id: &str,
) -> Result<bool, CommandError> {
    let ctx = request_context();
    let id = JobId::new(job_id.to_string());
    match state.desktop_job_host.get(&ctx, id.clone()).await {
        Err(PortError::NotFound(_)) => Ok(false),
        Err(error) => Err(admit_error(error)),
        Ok(record) if record.view.state.is_terminal() => Ok(true),
        Ok(_) => state
            .desktop_job_host
            .cancel(&ctx, &id)
            .await
            .map(|_| true)
            .map_err(admit_error),
    }
}

pub(crate) async fn job_cancel_requested(
    state: &AppState,
    job_id: &str,
) -> Result<bool, CommandError> {
    let ctx = request_context();
    match state
        .desktop_job_host
        .get(&ctx, JobId::new(job_id.to_string()))
        .await
    {
        Ok(record) => Ok(record.view.cancel_requested),
        Err(PortError::NotFound(_)) => Ok(false),
        Err(error) => Err(admit_error(error)),
    }
}
