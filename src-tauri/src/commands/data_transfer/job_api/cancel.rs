//! Cancellation for a Data Transfer Job.
//!
//! The Job repository owns cancellation: `request_cancel` records the request
//! without touching the state, the runtime turns it into a terminal
//! `Cancelled`, and a request that lands before dispatch is confirmed as
//! not-started. The legacy `services::job_registry::cancel_job` path knows
//! nothing about these Jobs, so this module is the one the IPC command calls
//! first and falls back only for a job id it has never heard of.

use datazen_platform_api::id::JobId;
use datazen_platform_api::{JobRepository, PortError};

use crate::commands::error::CommandError;

use super::runtime::{admit_error, host, request_context};

/// Ask a running Job to stop. `Ok(false)` means this client never accepted
/// that job id, so the caller may fall back to the legacy registry. A Job that
/// already finished is answered here too: it is still a P5 Job, and a state the
/// runtime has already settled has nothing left to interrupt.
pub(crate) async fn cancel_data_transfer_job(job_id: &str) -> Result<bool, CommandError> {
    let host = host();
    let ctx = request_context();
    let id = JobId::new(job_id.to_string());
    match host.repo.get(&ctx, id.clone()).await {
        // Not a P5 Job: the caller may fall back to the legacy registry.
        Err(PortError::NotFound(_)) => return Ok(false),
        Err(error) => return Err(admit_error(error)),
        Ok(record) if record.view.state.is_terminal() => return Ok(true),
        Ok(_) => {}
    }
    host.repo
        .request_cancel(&ctx, &id)
        .map(|_| true)
        .map_err(admit_error)
}

/// Whether a cancel is already recorded for this Job. A recorded cancel is what
/// the run watch turns into the handler's flag, so this is also the honest
/// answer to "did my cancel reach the Job at all".
pub(crate) async fn job_cancel_requested(job_id: &str) -> Result<bool, CommandError> {
    let host = host();
    let ctx = request_context();
    match host.repo.get(&ctx, JobId::new(job_id.to_string())).await {
        Ok(record) => Ok(record.view.cancel_requested),
        Err(PortError::NotFound(_)) => Ok(false),
        Err(error) => Err(admit_error(error)),
    }
}
