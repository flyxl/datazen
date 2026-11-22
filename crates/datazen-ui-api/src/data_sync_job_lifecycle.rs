#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobRecoveryVerdict {
    NotExecuted,
    PendingVerification,
    ResumeAfterVerify,
    Reject,
    RequireManualReview,
}

impl JobRecoveryVerdict {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotExecuted => "notExecuted",
            Self::PendingVerification => "pendingVerification",
            Self::ResumeAfterVerify => "resumeAfterVerify",
            Self::Reject => "reject",
            Self::RequireManualReview => "requireManualReview",
        }
    }

    pub fn from_str(value: &str) -> Self {
        match value {
            "pendingVerification" => Self::PendingVerification,
            "resumeAfterVerify" => Self::ResumeAfterVerify,
            "reject" => Self::Reject,
            "requireManualReview" => Self::RequireManualReview,
            _ => Self::NotExecuted,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveryTarget {
    pub connection_id: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DataSyncJobDetails {
    pub job_id: String,
    pub recovery_verdict: Option<JobRecoveryVerdict>,
    pub recovery_targets: Vec<RecoveryTarget>,
    pub domain_results: Vec<serde_json::Value>,
}

impl DataSyncJobDetails {
    pub fn new(job_id: impl Into<String>, verdict: Option<JobRecoveryVerdict>) -> Self {
        Self {
            job_id: job_id.into(),
            recovery_verdict: verdict,
            recovery_targets: Vec::new(),
            domain_results: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Endpoints {
    pub source_id: String,
    pub target_id: String,
    pub source_database: String,
    pub target_database: String,
    pub source_schema: String,
    pub target_schema: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EndpointSession {
    pub db_session_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VerifyRequest {
    pub job_id: String,
    pub source_db_session_id: String,
    pub target_db_session_id: String,
    pub source_database: String,
    pub target_database: String,
    pub source_schema: Option<String>,
    pub target_schema: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VerifyResponse {
    pub recovery_verdict: Option<JobRecoveryVerdict>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerifyOutcome {
    Skipped,
    EndpointMismatch,
    EndpointsMissing,
    SessionsMissing,
    Verified { manual_review: bool },
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CancelOutcome {
    Cancelled,
    Failed,
}

pub trait FeedbackSink {
    fn set_error_msg(&mut self, message: String);
    fn set_error_open(&mut self, open: bool);
    fn set_status_msg(&mut self, message: String);
}

pub trait JobLifecycleHost {
    fn translate(&self, key: &str) -> String;

    fn list_jobs(&self) -> Option<Vec<DataSyncJobDetails>>;

    fn refresh_endpoint_sessions(&self) -> Result<(EndpointSession, EndpointSession), String>;

    fn verify_recovery(&self, request: &VerifyRequest) -> Result<VerifyResponse, String>;

    fn cancel_job(&self, job_id: &str) -> Result<(), String>;

    fn set_poll_timer(&self, interval_ms: u64);

    fn clear_poll_timer(&self);
}

pub const RECOVERY_ENDPOINT_MISMATCH_KEY: &str = "migrationJob.recoveryEndpointMismatch";
pub const RECOVERY_SELECT_ENDPOINTS_KEY: &str = "migrationJob.recoverySelectEndpoints";
pub const RECOVERY_MANUAL_REVIEW_KEY: &str = "migrationJob.recoveryManualReview";
pub const RECOVERY_RECORDED_KEY: &str = "migrationJob.recoveryRecorded";
pub const JOB_POLL_INTERVAL_MS: u64 = 2_000;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LifecycleState {
    pub durable_jobs: Vec<DataSyncJobDetails>,
    pub verifying_recovery_job: Option<String>,
    pub cancelling_durable_job: Option<String>,
}

pub struct DataSyncJobLifecycle {
    pub endpoints: Endpoints,
    pub state: LifecycleState,
}

impl DataSyncJobLifecycle {
    pub fn new(endpoints: Endpoints) -> Self {
        Self {
            endpoints,
            state: LifecycleState::default(),
        }
    }

    pub fn begin_polling(&self, host: &dyn JobLifecycleHost) {
        host.set_poll_timer(JOB_POLL_INTERVAL_MS);
    }

    pub fn end_polling(&self, host: &dyn JobLifecycleHost) {
        host.clear_poll_timer();
    }

    pub fn refresh_jobs(&mut self, host: &dyn JobLifecycleHost) -> Option<Vec<DataSyncJobDetails>> {
        match host.list_jobs() {
            Some(jobs) => {
                self.state.durable_jobs = jobs.clone();
                Some(jobs)
            }
            None => None,
        }
    }

    pub fn not_executed_jobs(&self) -> Vec<&DataSyncJobDetails> {
        self.state
            .durable_jobs
            .iter()
            .filter(|job| job.recovery_verdict == Some(JobRecoveryVerdict::NotExecuted))
            .collect()
    }

    pub fn verify_recovery(
        &mut self,
        host: &dyn JobLifecycleHost,
        feedback: &mut dyn FeedbackSink,
        candidate: &DataSyncJobDetails,
    ) -> VerifyOutcome {
        if candidate.recovery_verdict != Some(JobRecoveryVerdict::PendingVerification) {
            return VerifyOutcome::Skipped;
        }
        let source = candidate.recovery_targets.first();
        let target = candidate.recovery_targets.get(1);
        let endpoints_match = match (source, target) {
            (Some(source), Some(target)) => {
                source.connection_id == self.endpoints.source_id
                    && target.connection_id == self.endpoints.target_id
            }
            _ => false,
        };
        if !endpoints_match {
            feedback.set_error_msg(host.translate(RECOVERY_ENDPOINT_MISMATCH_KEY));
            feedback.set_error_open(true);
            return VerifyOutcome::EndpointMismatch;
        }
        if self.endpoints.source_database.is_empty() || self.endpoints.target_database.is_empty() {
            feedback.set_error_msg(host.translate(RECOVERY_SELECT_ENDPOINTS_KEY));
            feedback.set_error_open(true);
            return VerifyOutcome::EndpointsMissing;
        }

        let job_id = candidate.job_id.clone();
        self.state.verifying_recovery_job = Some(job_id.clone());
        let outcome: Result<VerifyOutcome, String> = (|| {
            let (source, target) = host.refresh_endpoint_sessions().map_err(|error| {
                feedback.set_error_msg(error.clone());
                feedback.set_error_open(true);
                error
            })?;
            let source_db_session_id = match source.db_session_id {
                Some(id) => id,
                None => {
                    let message = host.translate(RECOVERY_SELECT_ENDPOINTS_KEY);
                    feedback.set_error_msg(message.clone());
                    feedback.set_error_open(true);
                    return Err(message);
                }
            };
            let target_db_session_id = match target.db_session_id {
                Some(id) => id,
                None => {
                    let message = host.translate(RECOVERY_SELECT_ENDPOINTS_KEY);
                    feedback.set_error_msg(message.clone());
                    feedback.set_error_open(true);
                    return Err(message);
                }
            };
            let request = VerifyRequest {
                job_id: job_id.clone(),
                source_db_session_id,
                target_db_session_id,
                source_database: self.endpoints.source_database.clone(),
                target_database: self.endpoints.target_database.clone(),
                source_schema: non_empty(&self.endpoints.source_schema),
                target_schema: non_empty(&self.endpoints.target_schema),
            };
            let verified = host.verify_recovery(&request).map_err(|error| {
                feedback.set_error_msg(error.clone());
                feedback.set_error_open(true);
                error
            })?;
            self.refresh_jobs(host);
            let manual_review =
                verified.recovery_verdict == Some(JobRecoveryVerdict::RequireManualReview);
            let key = if manual_review {
                RECOVERY_MANUAL_REVIEW_KEY
            } else {
                RECOVERY_RECORDED_KEY
            };
            feedback.set_status_msg(host.translate(key));
            Ok(VerifyOutcome::Verified { manual_review })
        })();
        self.state.verifying_recovery_job = None;
        outcome.unwrap_or(VerifyOutcome::Failed)
    }

    pub fn cancel_job(
        &mut self,
        host: &dyn JobLifecycleHost,
        feedback: &mut dyn FeedbackSink,
        job_id: &str,
    ) -> CancelOutcome {
        self.state.cancelling_durable_job = Some(job_id.to_owned());
        let outcome: Result<CancelOutcome, String> = (|| {
            host.cancel_job(job_id).map_err(|error| {
                feedback.set_error_msg(error.clone());
                feedback.set_error_open(true);
                error
            })?;
            self.refresh_jobs(host);
            Ok(CancelOutcome::Cancelled)
        })();
        self.state.cancelling_durable_job = None;
        outcome.unwrap_or(CancelOutcome::Failed)
    }
}

fn non_empty(value: &str) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value.to_owned())
    }
}

pub const DATA_SYNC_JOB_LIFECYCLE_SURFACE: [&str; 10] = [
    "refreshDataSyncJobs",
    "verifyDataSyncJobRecovery",
    "cancelDurableDataSyncJob",
    "notExecutedJobs",
    "DataSyncJobDetails",
    "JobRecoveryVerdict",
    "FeedbackSink",
    "JobLifecycleHost",
    "LifecycleState",
    "DATA_SYNC_JOB_LIFECYCLE_GAP",
];

pub const DATA_SYNC_JOB_LIFECYCLE_GAP: [&str; 6] = [
    "React hook state, effects and the 2000ms window.setInterval timer",
    "syncCommands listDataSyncJobs / verifyDataSyncRecovery / cancelDataSync IPC",
    "refreshEndpointSessions from useDataSyncEndpoints is injected as a host seam",
    "useI18n t() is injected via JobLifecycleHost::translate",
    "useDataSyncFeedback is injected as FeedbackSink",
    "async Promise timing and error-to-message coercion are immediate-mode here",
];

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    #[derive(Default)]
    struct Feedback {
        errors: RefCell<Vec<String>>,
        opened: RefCell<usize>,
        statuses: RefCell<Vec<String>>,
    }

    impl FeedbackSink for Feedback {
        fn set_error_msg(&mut self, message: String) {
            self.errors.borrow_mut().push(message);
        }

        fn set_error_open(&mut self, open: bool) {
            if open {
                *self.opened.borrow_mut() += 1;
            }
        }

        fn set_status_msg(&mut self, message: String) {
            self.statuses.borrow_mut().push(message);
        }
    }

    struct Host {
        jobs: Option<Vec<DataSyncJobDetails>>,
        sessions: Result<(EndpointSession, EndpointSession), String>,
        verified: Result<VerifyResponse, String>,
        cancel_error: Option<String>,
        requests: RefCell<Vec<VerifyRequest>>,
        cancelled: RefCell<Vec<String>>,
        timer: RefCell<Vec<u64>>,
        cleared: RefCell<usize>,
        list_calls: RefCell<usize>,
    }

    impl Default for Host {
        fn default() -> Self {
            Self {
                jobs: None,
                sessions: Ok((EndpointSession::default(), EndpointSession::default())),
                verified: Ok(VerifyResponse {
                    recovery_verdict: None,
                }),
                cancel_error: None,
                requests: RefCell::new(Vec::new()),
                cancelled: RefCell::new(Vec::new()),
                timer: RefCell::new(Vec::new()),
                cleared: RefCell::new(0),
                list_calls: RefCell::new(0),
            }
        }
    }

    impl JobLifecycleHost for Host {
        fn translate(&self, key: &str) -> String {
            format!("i18n:{key}")
        }

        fn list_jobs(&self) -> Option<Vec<DataSyncJobDetails>> {
            *self.list_calls.borrow_mut() += 1;
            self.jobs.clone()
        }

        fn refresh_endpoint_sessions(&self) -> Result<(EndpointSession, EndpointSession), String> {
            self.sessions.clone()
        }

        fn verify_recovery(&self, request: &VerifyRequest) -> Result<VerifyResponse, String> {
            self.requests.borrow_mut().push(request.clone());
            self.verified.clone()
        }

        fn cancel_job(&self, job_id: &str) -> Result<(), String> {
            self.cancelled.borrow_mut().push(job_id.to_owned());
            match &self.cancel_error {
                Some(error) => Err(error.clone()),
                None => Ok(()),
            }
        }

        fn set_poll_timer(&self, interval_ms: u64) {
            self.timer.borrow_mut().push(interval_ms);
        }

        fn clear_poll_timer(&self) {
            *self.cleared.borrow_mut() += 1;
        }
    }

    fn endpoints() -> Endpoints {
        Endpoints {
            source_id: "c1".to_owned(),
            target_id: "c2".to_owned(),
            source_database: "app".to_owned(),
            target_database: "warehouse".to_owned(),
            source_schema: String::new(),
            target_schema: "public".to_owned(),
        }
    }

    fn candidate(verdict: JobRecoveryVerdict) -> DataSyncJobDetails {
        let mut job = DataSyncJobDetails::new("job-1", Some(verdict));
        job.recovery_targets = vec![
            RecoveryTarget {
                connection_id: "c1".to_owned(),
            },
            RecoveryTarget {
                connection_id: "c2".to_owned(),
            },
        ];
        job
    }

    fn host() -> Host {
        Host {
            jobs: Some(Vec::new()),
            sessions: Ok((
                EndpointSession {
                    db_session_id: Some("s1".to_owned()),
                },
                EndpointSession {
                    db_session_id: Some("s2".to_owned()),
                },
            )),
            verified: Ok(VerifyResponse {
                recovery_verdict: None,
            }),
            ..Host::default()
        }
    }

    #[test]
    fn verdict_literals_round_trip() {
        for (verdict, literal) in [
            (JobRecoveryVerdict::NotExecuted, "notExecuted"),
            (
                JobRecoveryVerdict::PendingVerification,
                "pendingVerification",
            ),
            (JobRecoveryVerdict::ResumeAfterVerify, "resumeAfterVerify"),
            (JobRecoveryVerdict::Reject, "reject"),
            (
                JobRecoveryVerdict::RequireManualReview,
                "requireManualReview",
            ),
        ] {
            assert_eq!(verdict.as_str(), literal);
            assert_eq!(JobRecoveryVerdict::from_str(literal), verdict);
        }
        assert_eq!(
            JobRecoveryVerdict::from_str("?"),
            JobRecoveryVerdict::NotExecuted
        );
    }

    #[test]
    fn polling_interval_and_refresh_failure_are_observable() {
        let mut h = host();
        h.jobs = None;
        let mut lifecycle = DataSyncJobLifecycle::new(endpoints());
        lifecycle.begin_polling(&h);
        assert_eq!(h.timer.borrow().as_slice(), &[2_000]);
        assert_eq!(lifecycle.refresh_jobs(&h), None);
        assert!(lifecycle.state.durable_jobs.is_empty());
        lifecycle.end_polling(&h);
        assert_eq!(*h.cleared.borrow(), 1);
    }

    #[test]
    fn only_not_executed_jobs_are_listed() {
        let h = host();
        let mut lifecycle = DataSyncJobLifecycle::new(endpoints());
        lifecycle.state.durable_jobs = vec![
            candidate(JobRecoveryVerdict::NotExecuted),
            candidate(JobRecoveryVerdict::Reject),
        ];
        assert_eq!(lifecycle.not_executed_jobs().len(), 1);
        assert_eq!(lifecycle.not_executed_jobs()[0].job_id, "job-1");
    }

    #[test]
    fn verification_skips_non_candidates() {
        let h = host();
        let mut feedback = Feedback::default();
        let mut lifecycle = DataSyncJobLifecycle::new(endpoints());
        assert_eq!(
            lifecycle.verify_recovery(
                &h,
                &mut feedback,
                &candidate(JobRecoveryVerdict::NotExecuted)
            ),
            VerifyOutcome::Skipped
        );
        assert!(h.requests.borrow().is_empty());
        assert!(feedback.errors.borrow().is_empty());
    }

    #[test]
    fn verification_rejects_a_target_mismatch_before_any_session_work() {
        let h = host();
        let mut feedback = Feedback::default();
        let mut lifecycle = DataSyncJobLifecycle::new(endpoints());
        let mut bad = candidate(JobRecoveryVerdict::PendingVerification);
        bad.recovery_targets[1].connection_id = "other".to_owned();
        assert_eq!(
            lifecycle.verify_recovery(&h, &mut feedback, &bad),
            VerifyOutcome::EndpointMismatch
        );
        assert_eq!(
            feedback.errors.borrow().as_slice(),
            &["i18n:migrationJob.recoveryEndpointMismatch".to_owned()]
        );
        assert_eq!(*feedback.opened.borrow(), 1);
        assert_eq!(*h.list_calls.borrow(), 0);
    }

    #[test]
    fn verification_requires_both_databases_and_both_sessions() {
        let h = host();
        let mut feedback = Feedback::default();
        let mut lifecycle = DataSyncJobLifecycle::new(endpoints());
        lifecycle.endpoints.source_database.clear();
        assert_eq!(
            lifecycle.verify_recovery(
                &h,
                &mut feedback,
                &candidate(JobRecoveryVerdict::PendingVerification)
            ),
            VerifyOutcome::EndpointsMissing
        );

        let h2 = host();
        let mut feedback2 = Feedback::default();
        let mut lifecycle2 = DataSyncJobLifecycle::new(endpoints());
        let mut no_session = h2;
        no_session.sessions = Ok((
            EndpointSession {
                db_session_id: None,
            },
            EndpointSession {
                db_session_id: Some("s2".to_owned()),
            },
        ));
        assert_eq!(
            lifecycle2.verify_recovery(
                &no_session,
                &mut feedback2,
                &candidate(JobRecoveryVerdict::PendingVerification)
            ),
            VerifyOutcome::Failed
        );
        assert_eq!(lifecycle2.state.verifying_recovery_job, None);
        assert_eq!(*feedback2.opened.borrow(), 1);
        assert_eq!(
            feedback2.errors.borrow().as_slice(),
            &["i18n:migrationJob.recoverySelectEndpoints".to_owned()]
        );
    }

    #[test]
    fn a_successful_verification_records_status_and_refreshes_jobs() {
        let h = host();
        let mut feedback = Feedback::default();
        let mut lifecycle = DataSyncJobLifecycle::new(endpoints());
        let outcome = lifecycle.verify_recovery(
            &h,
            &mut feedback,
            &candidate(JobRecoveryVerdict::PendingVerification),
        );
        assert_eq!(
            outcome,
            VerifyOutcome::Verified {
                manual_review: false
            }
        );
        assert_eq!(
            feedback.statuses.borrow().as_slice(),
            &["i18n:migrationJob.recoveryRecorded".to_owned()]
        );
        assert_eq!(*h.list_calls.borrow(), 1);
        assert_eq!(lifecycle.state.verifying_recovery_job, None);
        let request = &h.requests.borrow()[0];
        assert_eq!(request.job_id, "job-1");
        assert_eq!(request.source_db_session_id, "s1");
        assert_eq!(request.target_db_session_id, "s2");
        assert_eq!(request.source_schema, None);
        assert_eq!(request.target_schema.as_deref(), Some("public"));
    }

    #[test]
    fn manual_review_verdict_selects_its_own_message() {
        let h = host();
        let mut feedback = Feedback::default();
        let mut lifecycle = DataSyncJobLifecycle::new(endpoints());
        lifecycle.endpoints = Endpoints {
            source_schema: "app".to_owned(),
            ..endpoints()
        };
        let mut verified = Host {
            verified: Ok(VerifyResponse {
                recovery_verdict: Some(JobRecoveryVerdict::RequireManualReview),
            }),
            ..host()
        };
        let outcome = lifecycle.verify_recovery(
            &verified,
            &mut feedback,
            &candidate(JobRecoveryVerdict::PendingVerification),
        );
        assert_eq!(
            outcome,
            VerifyOutcome::Verified {
                manual_review: true
            }
        );
        assert_eq!(
            feedback.statuses.borrow().as_slice(),
            &["i18n:migrationJob.recoveryManualReview".to_owned()]
        );
        assert_eq!(
            verified.requests.borrow()[0].source_schema.as_deref(),
            Some("app")
        );
    }

    #[test]
    fn cancelling_a_job_is_gated_and_reports_errors() {
        let mut h = host();
        h.cancel_error = Some("busy".to_owned());
        let mut feedback = Feedback::default();
        let mut lifecycle = DataSyncJobLifecycle::new(endpoints());
        assert_eq!(
            lifecycle.cancel_job(&h, &mut feedback, "job-9"),
            CancelOutcome::Failed
        );
        assert_eq!(h.cancelled.borrow().as_slice(), &["job-9".to_owned()]);
        assert_eq!(feedback.errors.borrow().as_slice(), &["busy".to_owned()]);
        assert_eq!(lifecycle.state.cancelling_durable_job, None);
    }

    #[test]
    fn gap_and_surface_are_pinned_and_root_reexports_match() {
        assert_eq!(DATA_SYNC_JOB_LIFECYCLE_SURFACE.len(), 10);
        assert!(DATA_SYNC_JOB_LIFECYCLE_SURFACE.contains(&"verifyDataSyncJobRecovery"));
        assert_eq!(DATA_SYNC_JOB_LIFECYCLE_GAP.len(), 6);
        assert_eq!(crate::DATA_SYNC_JOB_LIFECYCLE_SURFACE.len(), 10);
        assert_eq!(crate::DATA_SYNC_JOB_LIFECYCLE_GAP.len(), 6);
        let _: Option<crate::DataSyncJobDetails> = Some(candidate(JobRecoveryVerdict::Reject));
    }
}
