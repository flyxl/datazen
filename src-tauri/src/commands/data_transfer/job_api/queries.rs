//! Durable query commands for the shared desktop Job host.

use datazen_platform_api::dto::job::{JobDetails, JobFilter, JobState, JobView};
use datazen_platform_api::id::JobId;
use datazen_platform_api::PortError;
use tauri::State;

use crate::commands::error::{CmdExt, CommandError};
use crate::commands::AppState;

use super::runtime::{admit_error, request_context, APPLY_KIND, PREPARE_KIND};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct JobQuery {
    pub(crate) kind: Option<String>,
    pub(crate) limit: Option<u32>,
}

impl JobQuery {
    fn matches(&self, kind: &str) -> bool {
        self.kind
            .as_deref()
            .map_or(true, |requested| requested == kind)
    }
}

#[tauri::command]
pub async fn get_job(state: State<'_, AppState>, job_id: String) -> Result<JobView, CommandError> {
    read_job(&state, &job_id).await.cmd_err("get_job")
}

#[tauri::command]
pub async fn list_jobs(
    state: State<'_, AppState>,
    states: Option<Vec<JobState>>,
    kind: Option<String>,
    limit: Option<u32>,
) -> Result<Vec<JobView>, CommandError> {
    read_jobs(&state, states, kind, limit)
        .await
        .cmd_err("list_jobs")
}

/// Read the durable Transfer result, including exact commit boundaries and the
/// persisted recovery decision needed after the window or process has reopened.
#[tauri::command]
pub async fn get_transfer_job_details(
    state: State<'_, AppState>,
    job_id: String,
) -> Result<JobDetails, CommandError> {
    read_transfer_job_details(&state, &job_id)
        .await
        .cmd_err("get_transfer_job_details")
}

pub(crate) async fn read_job(state: &AppState, job_id: &str) -> Result<JobView, CommandError> {
    let ctx = request_context();
    let record = match state
        .desktop_job_host
        .get(&ctx, JobId::new(job_id.to_string()))
        .await
    {
        Ok(record) => record,
        Err(PortError::NotFound(_)) => {
            return Err(CommandError::NotFound(format!(
                "job '{job_id}' was not found"
            )));
        }
        Err(error) => return Err(admit_error(error)),
    };
    Ok(record.view)
}

pub(crate) async fn read_jobs(
    state: &AppState,
    states: Option<Vec<JobState>>,
    kind: Option<String>,
    limit: Option<u32>,
) -> Result<Vec<JobView>, CommandError> {
    let ctx = request_context();
    let filter = JobFilter {
        states: states.unwrap_or_default(),
        owner: None,
        after: None,
        limit: None,
    };
    let records = state
        .desktop_job_host
        .list(&ctx, filter)
        .await
        .map_err(admit_error)?;
    let query = JobQuery { kind, limit };
    Ok(select(&records, &query))
}

pub(crate) async fn read_transfer_job_details(
    state: &AppState,
    job_id: &str,
) -> Result<JobDetails, CommandError> {
    let ctx = request_context();
    let details = state
        .desktop_job_host
        .details(&ctx, JobId::new(job_id.to_string()))
        .await
        .map_err(admit_error)?;
    if details.job.kind != PREPARE_KIND && details.job.kind != APPLY_KIND {
        return Err(CommandError::NotFound(format!(
            "transfer job '{job_id}' was not found"
        )));
    }
    Ok(details)
}

pub(crate) fn select(
    records: &[datazen_platform_api::dto::job::JobRecord],
    query: &JobQuery,
) -> Vec<JobView> {
    let mut views = records
        .iter()
        .filter(|record| query.matches(&record.view.kind))
        .map(|record| record.view.clone())
        .collect::<Vec<_>>();
    if let Some(limit) = query.limit {
        views.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
    }
    views
}
