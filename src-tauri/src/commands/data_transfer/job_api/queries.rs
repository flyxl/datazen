//! The read side of a transfer Job's handle.
//!
//! Apply now returns as soon as its Job is admitted, which moves the moment a
//! caller can first *name* the Job to before the Job has done any work. An id
//! nobody can ask about is a log line, not a handle, so these two commands are
//! the other half of that split: `apply_data_transfer_job` and
//! `prepare_data_transfer_job` hand out an id, and `get_job` / `list_jobs` are
//! how the same caller turns that id back into a state, a progress count and a
//! cancel flag. Together they are what makes a Job addressable while it is
//! still running, which is the only reason a mid-run cancel is expressible.
//!
//! The names follow the platform Job surface the frontend client already calls
//! (`getJob` / `listJobs`): the desktop transport rewrites a camelCase method
//! name into this snake_case command name before invoking it, so a caller that
//! knows nothing about Data Transfer reaches the same two commands and gets the
//! same `JobView` back.

use datazen_platform_api::dto::job::{JobFilter, JobRecord, JobState, JobView};
use datazen_platform_api::id::{ArtifactId, JobId};
use datazen_platform_api::{JobRepository, PortError};

use crate::commands::error::{CmdExt, CommandError};

use super::runtime::{admit_error, host, host_artifacts, host_progress, request_context};

/// The filter `list_jobs` narrows by, after the repository has had its say.
///
/// `kind` is here rather than in `JobFilter` because that filter has no kind
/// field at all: the repository stores the kind on the record and cannot narrow
/// on it, so a caller that asks for one kind would otherwise be handed every
/// Job in the host and left to filter a raw view it does not own.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct JobQuery {
    pub(crate) kind: Option<String>,
    pub(crate) limit: Option<u32>,
}

impl JobQuery {
    fn matches(&self, record: &JobRecord) -> bool {
        match &self.kind {
            Some(kind) => record.view.kind == *kind,
            None => true,
        }
    }
}

/// Read one Job by id.
///
/// A job id this host never accepted is a refusal, not an empty view: the
/// caller passed something that cannot name a Job, and answering with a
/// plausible-looking view would let it treat a typo as a queued run.
#[tauri::command]
pub async fn get_job(job_id: String) -> Result<JobView, CommandError> {
    read_job(&job_id).await.cmd_err("get_job")
}

/// Read every Job the caller may see, oldest first, narrowed by the caller's
/// filter.
///
/// The filter arrives as a flat argument bag, because the client hands its
/// filter object straight to the invoke call instead of wrapping it. That is why
/// each field is its own optional argument: a caller that sends `{}` reaches
/// this command with every argument absent, and a caller that later sends a
/// field this host does not know about is not refused for it.
#[tauri::command]
pub async fn list_jobs(
    states: Option<Vec<JobState>>,
    kind: Option<String>,
    limit: Option<u32>,
) -> Result<Vec<JobView>, CommandError> {
    read_jobs(states, kind, limit).await.cmd_err("list_jobs")
}

pub(crate) async fn read_job(job_id: &str) -> Result<JobView, CommandError> {
    let ctx = request_context();
    let record = match host().repo.get(&ctx, JobId::new(job_id.to_string())).await {
        Ok(record) => record,
        Err(PortError::NotFound(_)) => {
            return Err(CommandError::NotFound(format!(
                "job '{job_id}' was not found"
            )));
        }
        Err(error) => return Err(admit_error(error)),
    };
    Ok(view(record.view))
}

pub(crate) async fn read_jobs(
    states: Option<Vec<JobState>>,
    kind: Option<String>,
    limit: Option<u32>,
) -> Result<Vec<JobView>, CommandError> {
    let ctx = request_context();
    // The repository applies the state filter and orders by creation time; the
    // cursor and the limit are left to it unset on purpose, because this
    // repository honours neither, and a filter field that is accepted and then
    // ignored reads to a caller as a result it can trust.
    let filter = JobFilter {
        states: states.unwrap_or_default(),
        owner: None,
        after: None,
        limit: None,
    };
    let records = host().repo.list(&ctx, filter).await.map_err(admit_error)?;
    Ok(select(&records, &JobQuery { kind, limit }))
}

/// Narrow already-ordered records and project them into what a caller reads.
pub(crate) fn select(records: &[JobRecord], query: &JobQuery) -> Vec<JobView> {
    let mut views: Vec<JobView> = records
        .iter()
        .filter(|record| query.matches(record))
        .map(|record| view(record.view.clone()))
        .collect();
    if let Some(limit) = query.limit {
        views.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
    }
    views
}

/// The view a caller reads back, with what the host has to add merged in.
///
/// Two things a caller is entitled to see do not live in the Job record. The ids
/// the host hashes on its own — the emitted SQL file is one — are kept beside the
/// Job because an apply that returned at admission has no return value left to
/// publish them through. And the progress counters are kept for the same reason:
/// the repository writes them once as zero and its contract has no port to change
/// them, while the run accumulates the real ones. Without these two merges a
/// caller polling a finished migration learns that it produced no artifact and
/// moved no rows.
fn view(mut view: JobView) -> JobView {
    let mut extra = host_artifacts(view.job_id.as_str());
    extra.retain(|id| !view.artifact_ids.iter().any(|known| known.as_str() == id));
    extra.dedup();
    view.artifact_ids
        .extend(extra.into_iter().map(ArtifactId::from));
    if let Some(progress) = host_progress(view.job_id.as_str()) {
        view.progress = progress;
    }
    view
}
