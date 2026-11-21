use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MigrationWindow {
    SchemaDiff,
    DataSync,
    DataTransfer,
}

pub fn job_kinds(window: MigrationWindow) -> &'static [&'static str] {
    match window {
        MigrationWindow::SchemaDiff => &["schemaDiffPrepare", "schemaDiffApply"],
        MigrationWindow::DataSync => &["dataSyncPrepare", "dataSyncApply"],
        MigrationWindow::DataTransfer => &["dataTransferPrepare", "dataTransferApply"],
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobState {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

impl JobState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn is_active(self) -> bool {
        matches!(self, Self::Queued | Self::Running)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobView {
    pub job_id: String,
    pub kind: String,
    pub state: JobState,
    pub created_at: i64,
    pub updated_at: i64,
    pub pending_verification_reason: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MigrationJobHydration {
    pub jobs: Vec<JobView>,
    pub active_jobs: Vec<JobView>,
    pub terminal_jobs: Vec<JobView>,
    pub verification_jobs: Vec<JobView>,
}

pub type ListJobsFn = dyn Fn(Option<&[JobState]>) -> Result<Vec<JobView>, String>;
pub type GetJobFn = dyn Fn(&str) -> Result<JobView, String>;

pub struct MigrationJobClient {
    pub list_jobs: Box<ListJobsFn>,
    pub get_job: Box<GetJobFn>,
}

fn buckets(jobs: Vec<JobView>) -> MigrationJobHydration {
    MigrationJobHydration {
        active_jobs: jobs
            .iter()
            .filter(|job| job.state.is_active())
            .cloned()
            .collect(),
        terminal_jobs: jobs
            .iter()
            .filter(|job| !job.state.is_active())
            .cloned()
            .collect(),
        verification_jobs: jobs
            .iter()
            .filter(|job| job.pending_verification_reason.is_some())
            .cloned()
            .collect(),
        jobs,
    }
}

pub fn hydrate_migration_jobs(
    client: &MigrationJobClient,
    window: MigrationWindow,
) -> Result<MigrationJobHydration, String> {
    let kinds = job_kinds(window);
    let is_our_kind = |job: &JobView| kinds.contains(&job.kind.as_str());

    let active_summaries: Vec<JobView> =
        (client.list_jobs)(Some(&[JobState::Queued, JobState::Running]))?
            .into_iter()
            .filter(is_our_kind)
            .collect();
    let mut refreshed_active = Vec::new();
    for summary in &active_summaries {
        if let Ok(job) = (client.get_job)(&summary.job_id) {
            refreshed_active.push(job);
        }
    }
    let summaries: Vec<JobView> = (client.list_jobs)(None)?
        .into_iter()
        .filter(is_our_kind)
        .collect();

    let mut jobs: Vec<JobView> = Vec::new();
    for job in summaries {
        if let Some(existing) = jobs.iter_mut().find(|entry| entry.job_id == job.job_id) {
            *existing = job;
        } else {
            jobs.push(job);
        }
    }
    for job in refreshed_active {
        let keep = match jobs.iter().find(|entry| entry.job_id == job.job_id) {
            None => true,
            Some(latest) => job.updated_at > latest.updated_at,
        };
        if !keep {
            continue;
        }
        if let Some(existing) = jobs.iter_mut().find(|entry| entry.job_id == job.job_id) {
            *existing = job;
        } else {
            jobs.push(job);
        }
    }
    Ok(buckets(jobs))
}

pub fn update_migration_job_projection(
    current: &MigrationJobHydration,
    updated: JobView,
) -> MigrationJobHydration {
    let mut jobs = current.jobs.clone();
    match jobs.iter().position(|job| job.job_id == updated.job_id) {
        None => jobs.push(updated),
        Some(index) => {
            let existing = &jobs[index];
            if !existing.state.is_active() || updated.updated_at < existing.updated_at {
                return current.clone();
            }
            jobs[index] = updated;
        }
    }
    buckets(jobs)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobClassification {
    Active,
    PendingVerification,
    Terminal,
}

impl JobClassification {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::PendingVerification => "pendingVerification",
            Self::Terminal => "terminal",
        }
    }
}

pub fn classify_job_view(job: &JobView) -> JobClassification {
    if job.pending_verification_reason.is_some() {
        return JobClassification::PendingVerification;
    }
    if job.state.is_active() {
        return JobClassification::Active;
    }
    JobClassification::Terminal
}

pub const STALE_PLAN_ERROR_CODES: [&str; 3] = ["PlanStale", "SourceChanged", "PermissionDenied"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ErrorLike {
    ApiError {
        code: String,
    },
    Error {
        code: Option<String>,
        message: String,
    },
    Object {
        code: Option<String>,
    },
    Other,
}

fn is_stale_code(code: Option<&str>) -> bool {
    code.is_some_and(|value| STALE_PLAN_ERROR_CODES.contains(&value))
}

pub fn is_stale_plan_error(error: &ErrorLike) -> bool {
    match error {
        ErrorLike::ApiError { code } => is_stale_code(Some(code)),
        ErrorLike::Error { code, message } => {
            is_stale_code(code.as_deref())
                || STALE_PLAN_ERROR_CODES
                    .iter()
                    .any(|candidate| message.contains(candidate))
        }
        ErrorLike::Object { code } => is_stale_code(code.as_deref()),
        ErrorLike::Other => false,
    }
}

pub fn latest_apply_job(jobs: &[JobView], window: MigrationWindow) -> Option<JobView> {
    let apply_kinds: Vec<&str> = job_kinds(window)
        .iter()
        .copied()
        .filter(|kind| kind.ends_with("Apply"))
        .collect();
    let mut latest: Option<JobView> = None;
    for job in jobs
        .iter()
        .filter(|job| apply_kinds.contains(&job.kind.as_str()))
    {
        if latest
            .as_ref()
            .is_none_or(|current| job.created_at > current.created_at)
        {
            latest = Some(job.clone());
        }
    }
    latest
}

static BOUND_CLIENT_FACTORY: OnceLock<fn() -> Option<MigrationJobClient>> = OnceLock::new();

pub fn bind_backend_client(
    factory: fn() -> Option<MigrationJobClient>,
) -> Result<(), fn() -> Option<MigrationJobClient>> {
    BOUND_CLIENT_FACTORY.set(factory)
}

pub fn bound_backend_client() -> Option<MigrationJobClient> {
    BOUND_CLIENT_FACTORY.get().and_then(|factory| factory())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(id: &str, kind: &str, state: JobState, created: i64, updated: i64) -> JobView {
        JobView {
            job_id: id.to_owned(),
            kind: kind.to_owned(),
            state,
            created_at: created,
            updated_at: updated,
            pending_verification_reason: None,
        }
    }

    fn client(
        active: Vec<JobView>,
        all: Vec<JobView>,
        refreshed: Vec<Result<JobView, String>>,
    ) -> MigrationJobClient {
        let refreshed = std::cell::RefCell::new(refreshed.into_iter());
        let active_for_filter = active.clone();
        let all_for_filter = all.clone();
        MigrationJobClient {
            list_jobs: Box::new(move |states| {
                let source = if states.is_some() {
                    &active_for_filter
                } else {
                    &all_for_filter
                };
                Ok(source.clone())
            }),
            get_job: Box::new(move |_| {
                refreshed
                    .borrow_mut()
                    .next()
                    .unwrap_or_else(|| Err("gone".to_owned()))
            }),
        }
    }

    #[test]
    fn job_kinds_and_state_literals_are_pinned() {
        assert_eq!(
            job_kinds(MigrationWindow::SchemaDiff),
            ["schemaDiffPrepare", "schemaDiffApply"]
        );
        assert_eq!(
            job_kinds(MigrationWindow::DataSync),
            ["dataSyncPrepare", "dataSyncApply"]
        );
        assert_eq!(
            job_kinds(MigrationWindow::DataTransfer),
            ["dataTransferPrepare", "dataTransferApply"]
        );
        assert_eq!(JobState::Queued.as_str(), "queued");
        assert_eq!(JobState::Cancelled.as_str(), "cancelled");
        assert!(JobState::Queued.is_active());
        assert!(!JobState::Failed.is_active());
    }

    #[test]
    fn hydration_filters_kinds_and_buckets_jobs() {
        let active = vec![
            job("a1", "schemaDiffPrepare", JobState::Running, 1, 3),
            job("x1", "dataSyncApply", JobState::Queued, 1, 1),
        ];
        let all = vec![
            job("a1", "schemaDiffPrepare", JobState::Running, 1, 2),
            job("a2", "schemaDiffApply", JobState::Succeeded, 2, 4),
            job("x1", "dataSyncApply", JobState::Queued, 1, 1),
        ];
        let mut verified = job("a3", "schemaDiffApply", JobState::Failed, 3, 5);
        verified.pending_verification_reason = Some("outcomeUnknown".to_owned());
        let mut all_with_verified = all.clone();
        all_with_verified.push(verified);
        let c = client(
            active,
            all_with_verified,
            vec![Ok(job("a1", "schemaDiffPrepare", JobState::Running, 1, 3))],
        );
        let result = hydrate_migration_jobs(&c, MigrationWindow::SchemaDiff).unwrap();
        assert_eq!(result.jobs.len(), 3);
        assert_eq!(
            result.jobs[0].updated_at, 3,
            "refreshed active wins on newer timestamp"
        );
        assert_eq!(result.active_jobs.len(), 1);
        assert_eq!(result.terminal_jobs.len(), 2);
        assert_eq!(result.verification_jobs.len(), 1);
        assert_eq!(result.verification_jobs[0].job_id, "a3");
    }

    #[test]
    fn hydration_ignores_vanished_jobs_and_stale_refreshes() {
        let active = vec![job("a1", "schemaDiffPrepare", JobState::Running, 1, 1)];
        let all = vec![job("a1", "schemaDiffPrepare", JobState::Running, 1, 5)];
        let c = client(active, all, vec![Err("gone".to_owned())]);
        let result = hydrate_migration_jobs(&c, MigrationWindow::SchemaDiff).unwrap();
        assert_eq!(result.jobs[0].updated_at, 5);

        let c = client(
            vec![job("a1", "schemaDiffPrepare", JobState::Running, 1, 1)],
            vec![job("a1", "schemaDiffPrepare", JobState::Running, 1, 5)],
            vec![Ok(job("a1", "schemaDiffPrepare", JobState::Running, 1, 3))],
        );
        let result = hydrate_migration_jobs(&c, MigrationWindow::SchemaDiff).unwrap();
        assert_eq!(result.jobs[0].updated_at, 5, "older refresh is discarded");
    }

    #[test]
    fn projection_adds_replaces_and_ignores_stale_updates() {
        let current = MigrationJobHydration {
            jobs: vec![job("a1", "schemaDiffPrepare", JobState::Running, 1, 5)],
            ..Default::default()
        };
        let added = update_migration_job_projection(
            &current,
            job("a2", "schemaDiffApply", JobState::Succeeded, 2, 6),
        );
        assert_eq!(added.jobs.len(), 2);
        assert_eq!(added.terminal_jobs.len(), 1);

        let replaced = update_migration_job_projection(
            &current,
            job("a1", "schemaDiffPrepare", JobState::Succeeded, 1, 6),
        );
        assert_eq!(replaced.jobs[0].state, JobState::Succeeded);
        assert_eq!(replaced.active_jobs.len(), 0);

        let stale = update_migration_job_projection(
            &current,
            job("a1", "schemaDiffPrepare", JobState::Running, 1, 4),
        );
        assert_eq!(stale, current);

        let terminal = MigrationJobHydration {
            jobs: vec![job("a1", "schemaDiffPrepare", JobState::Failed, 1, 5)],
            ..Default::default()
        };
        let updated = update_migration_job_projection(
            &terminal,
            job("a1", "schemaDiffPrepare", JobState::Running, 1, 9),
        );
        assert_eq!(updated, terminal, "a terminal job is never rewritten");
    }

    #[test]
    fn classification_prioritizes_verification_over_state() {
        let mut verified = job("a", "schemaDiffApply", JobState::Running, 1, 1);
        verified.pending_verification_reason = Some("outcomeUnknown".to_owned());
        assert_eq!(
            classify_job_view(&verified),
            JobClassification::PendingVerification
        );
        assert_eq!(
            classify_job_view(&job("a", "k", JobState::Queued, 1, 1)),
            JobClassification::Active
        );
        assert_eq!(
            classify_job_view(&job("a", "k", JobState::Succeeded, 1, 1)),
            JobClassification::Terminal
        );
        assert_eq!(
            JobClassification::PendingVerification.as_str(),
            "pendingVerification"
        );
    }

    #[test]
    fn stale_plan_errors_cover_api_error_code_and_message() {
        assert_eq!(
            STALE_PLAN_ERROR_CODES,
            ["PlanStale", "SourceChanged", "PermissionDenied"]
        );
        assert!(is_stale_plan_error(&ErrorLike::ApiError {
            code: "PlanStale".to_owned()
        }));
        assert!(!is_stale_plan_error(&ErrorLike::ApiError {
            code: "Other".to_owned()
        }));
        assert!(is_stale_plan_error(&ErrorLike::Error {
            code: Some("SourceChanged".to_owned()),
            message: String::new()
        }));
        assert!(is_stale_plan_error(&ErrorLike::Error {
            code: None,
            message: "apply failed: PermissionDenied".to_owned()
        }));
        assert!(!is_stale_plan_error(&ErrorLike::Error {
            code: None,
            message: "network timeout".to_owned()
        }));
        assert!(is_stale_plan_error(&ErrorLike::Object {
            code: Some("PlanStale".to_owned())
        }));
        assert!(!is_stale_plan_error(&ErrorLike::Object { code: None }));
        assert!(!is_stale_plan_error(&ErrorLike::Other));
    }

    #[test]
    fn latest_apply_job_picks_newest_apply_kind_only() {
        let jobs = vec![
            job("p", "schemaDiffPrepare", JobState::Succeeded, 9, 9),
            job("a1", "schemaDiffApply", JobState::Failed, 3, 3),
            job("a2", "schemaDiffApply", JobState::Succeeded, 7, 7),
            job("d", "dataSyncApply", JobState::Succeeded, 99, 99),
        ];
        let latest = latest_apply_job(&jobs, MigrationWindow::SchemaDiff).unwrap();
        assert_eq!(latest.job_id, "a2");
        assert_eq!(
            latest_apply_job(&jobs[..1], MigrationWindow::SchemaDiff),
            None
        );
    }
}
