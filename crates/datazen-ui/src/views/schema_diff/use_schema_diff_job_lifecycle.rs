//! Port of `packages/frontend/src/windows/schema-diff/useSchemaDiffJobLifecycle.ts`
//! (T391).
//!
//! The wizard's job-tracking hook: the tracked current job (an accepted
//! receipt plus its freshest details), the latest apply's details, the
//! cancel outcome, the recovery-verification state and the error slot —
//! fed by the commands shell's observer bus (`T377`), a mount-time
//! hydration over `listJobs` (latest-first ordering, the
//! active/pending/apply/prepare selection, per-job detail reads, a 1000ms
//! poll loop while the active job stays queued/running) and the cancel /
//! verify-recovery actions.
//!
//! ## What maps where
//!
//! - `useState` becomes the fields of [`SchemaDiffJobLifecycle`]; the host
//!   owns one instance and renders the status panel (T115) from it.
//! - The `useEffect` subscription over `subscribeSchemaDiffJobUpdates`
//!   becomes [`SchemaDiffJobLifecycle::on_bus_update`]: the host routes
//!   every [`SchemaDiffJobUpdateVm`] there (accepted → [`Self::accept`],
//!   details → [`Self::observe`]).
//! - The mount-time hydration (`useEffect` + async IIFE + `setTimeout`
//!   chain) becomes the [`Self::hydrate_begin`] → `on_list_jobs` →
//!   `on_details` effect walk: [`LifecycleEffect`] steps carry the IPC,
//!   the host runs them and delivers receipts back; the 1000ms poll wait
//!   becomes [`LifecycleEffect::Wait`] (the host's `wake_at`, then
//!   [`Self::tick`]). `stopped` (the cleanup flag) is the host dropping
//!   the machine or ignoring further effects.
//! - `requestCancel` / `verifyRecovery` become
//!   [`Self::request_cancel`] / [`Self::verify_recovery_begin`] with the
//!   same outcome transitions (`requested` / `unavailable`, `verifying`
//!   on/off, the error slot) and their receipt methods.
//! - `latestFirst` (`[...jobs].sort((l, r) => r.updatedAt - l.updatedAt)`)
//!   is a stable sort by `updated_at` descending (`sort_by_key` on the
//!   negated stamp keeps JS's stable order for equal stamps).
//!
//! ## Keys
//!
//! No DOM of its own (a hook); the status panel that renders it (T115)
//! owns the keys.
//!
//! Known deltas (documented, not silent): the sequential `for … await
//! read(jobId)` hydration loop becomes an ordered read queue the host
//! drains in order (the reads still happen one at a time, the order is
//! the set's iteration order — React's `Set` insertion order, preserved
//! by the `Vec` here); a failed `getJobDetails` read sets the error slot
//! and stops that job's hydration exactly like the `catch`.

use crate::commands::schema_diff::wire::{
    JobRecoveryVerdictVm, SchemaDiffJobAcceptedVm, SchemaDiffJobDetailsVm, SchemaDiffJobViewVm,
    parse_job_views,
};
use crate::commands::schema_diff::SchemaDiffJobUpdateVm;
use crate::views::data_transfer::data_transfer_attached_jobs::JobState;

/// React `SchemaDiffTrackedJob`: the accepted receipt plus its freshest
/// details and the local-submission marker.
#[derive(Clone, Debug, PartialEq)]
pub struct SchemaDiffTrackedJobVm {
    pub job_id: String,
    pub kind: String,
    pub state: JobState,
    pub progress: crate::views::data_transfer::data_transfer_attached_jobs::JobProgress,
    pub details: Option<SchemaDiffJobDetailsVm>,
    /// Only jobs submitted in this wizard may populate its execution
    /// result.
    pub submitted_locally: bool,
}

impl SchemaDiffTrackedJobVm {
    /// `isActive(job)`: queued or running.
    pub fn is_active(&self) -> bool {
        matches!(self.state, JobState::Queued | JobState::Running)
    }
}

/// React's `cancelOutcome` union.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SchemaDiffCancelOutcome {
    #[default]
    Idle,
    Requested,
    Unavailable,
}

/// What the host must run next — the hook's await points, as effects.
#[derive(Clone, Debug, PartialEq)]
pub enum LifecycleEffect {
    /// `schemaDiffCommands.listJobs()`.
    ListJobs,
    /// `schemaDiffCommands.getJobDetails(jobId)`.
    GetJobDetails(String),
    /// `schemaDiffCommands.cancelDeploy(jobId)`.
    CancelDeploy(String),
    /// `schemaDiffCommands.verifyRecovery(jobId, targetDbSessionId)`.
    VerifyRecovery(String, String),
    /// A poll wait: the host arms `wake_at(until_ms / 1000)` then calls
    /// [`SchemaDiffJobLifecycle::tick`].
    Wait { until_ms: i64 },
}

/// React's `useSchemaDiffJobLifecycle()` state machine.
#[derive(Default)]
pub struct SchemaDiffJobLifecycle {
    current_job: Option<SchemaDiffTrackedJobVm>,
    latest_apply: Option<SchemaDiffJobDetailsVm>,
    cancel_outcome: SchemaDiffCancelOutcome,
    verifying: bool,
    error: Option<String>,
    /// The hydration's phase (the mount `useEffect`).
    hydrate: HydratePhase,
}

#[derive(Default, Debug, PartialEq)]
enum HydratePhase {
    /// Not started / finished.
    #[default]
    Idle,
    /// Awaiting the `listJobs` answer.
    ListingJobs,
    /// Draining the ordered detail reads (hydration + poll share the
    /// read channel).
    Reading { queue: Vec<String>, active_job: Option<String> },
    /// Polling the active job until it leaves queued/running.
    Polling { job_id: String, next_poll_at_ms: i64 },
}

/// React `wait`-free 1s poll cadence (the `setTimeout(…, 1000)`).
pub const SCHEMA_DIFF_JOB_POLL_MS: i64 = 1_000;

impl SchemaDiffJobLifecycle {
    /// `accept(accepted)` — the bus's accepted branch: a fresh local
    /// submission resets the cancel outcome and replaces the tracked job
    /// (no details yet).
    pub fn accept(&mut self, accepted: SchemaDiffJobAcceptedVm) {
        self.cancel_outcome = SchemaDiffCancelOutcome::Idle;
        self.current_job = Some(SchemaDiffTrackedJobVm {
            job_id: accepted.job_id,
            kind: accepted.kind,
            state: accepted.state,
            progress: accepted.progress,
            details: None,
            submitted_locally: true,
        });
    }

    /// `observe(details)` — the bus's details branch: the tracked job
    /// refreshes only when it is the same job (a different job's details
    /// never replace it), keeping the local-submission marker; an apply
    /// job's details also become `latestApply`.
    pub fn observe(&mut self, details: SchemaDiffJobDetailsVm) {
        let view = &details.details;
        let same_job = self
            .current_job
            .as_ref()
            .map_or(true, |current| current.job_id == view.job_id);
        if same_job {
            let submitted_locally = self
                .current_job
                .as_ref()
                .map_or(false, |current| current.submitted_locally);
            self.current_job = Some(SchemaDiffTrackedJobVm {
                job_id: view.job_id.clone(),
                kind: view.kind.clone(),
                state: view.state,
                progress: view.progress.clone(),
                details: Some(details.clone()),
                submitted_locally,
            });
        }
        if view.kind == "schemaDiffApply" {
            self.latest_apply = Some(details.clone());
        }
    }

    /// The subscription: route one bus update (accepted or details).
    pub fn on_bus_update(&mut self, update: SchemaDiffJobUpdateVm) {
        match update {
            SchemaDiffJobUpdateVm::Accepted(accepted) => self.accept(accepted),
            SchemaDiffJobUpdateVm::Details(details) => self.observe(details),
        }
    }

    /// The mount effect's start: `listJobs`, latest first, then the
    /// selection and the ordered reads.
    pub fn hydrate_begin(&mut self) -> Vec<LifecycleEffect> {
        self.hydrate = HydratePhase::ListingJobs;
        vec![LifecycleEffect::ListJobs]
    }

    /// The `listJobs` answer: `latestFirst`, then the
    /// active/pending/apply/prepare selection and the read set (the
    /// selected job first, the apply job after — React's `Set` insertion
    /// order). An active job also arms the 1000ms poll after the reads.
    pub fn on_list_jobs(&mut self, jobs: Vec<SchemaDiffJobViewVm>) -> Vec<LifecycleEffect> {
        if !matches!(self.hydrate, HydratePhase::ListingJobs) {
            return Vec::new();
        }
        if jobs.is_empty() {
            self.hydrate = HydratePhase::Idle;
            return Vec::new();
        }
        // latestFirst: stable sort by updatedAt descending.
        let mut jobs = jobs;
        jobs.sort_by_key(|job| -job.updated_at);

        let is_active =
            |job: &SchemaDiffJobViewVm| matches!(job.state, JobState::Queued | JobState::Running);
        let active = jobs.iter().find(|job| is_active(job));
        let pending = jobs
            .iter()
            .find(|job| job.pending_verification_reason.is_some());
        let apply = jobs.iter().find(|job| job.kind == "schemaDiffApply");
        let prepare = jobs.iter().find(|job| job.kind == "schemaDiffPrepare");
        let selected = active.or(pending).or(apply).or(prepare);

        let mut queue: Vec<String> = Vec::new();
        if let Some(selected) = selected {
            if !queue.iter().any(|id| id == &selected.job_id) {
                queue.push(selected.job_id.clone());
            }
        }
        if let Some(apply) = apply {
            if !queue.iter().any(|id| id == &apply.job_id) {
                queue.push(apply.job_id.clone());
            }
        }
        if queue.is_empty() {
            self.hydrate = HydratePhase::Idle;
            return Vec::new();
        }
        let effects = queue
            .iter()
            .map(|job_id| LifecycleEffect::GetJobDetails(job_id.clone()))
            .collect();
        self.hydrate = HydratePhase::Reading {
            queue,
            active_job: active.map(|job| job.job_id.clone()),
        };
        effects
    }

    /// A `getJobDetails` answer (hydration or poll): observe it, advance
    /// the read queue; when the queue drains, an active job arms the
    /// 1000ms poll, else the hydration finishes.
    pub fn on_details(&mut self, now_ms: i64, details: SchemaDiffJobDetailsVm) -> Vec<LifecycleEffect> {
        self.observe(details);
        if let HydratePhase::Reading { queue, active_job } = &mut self.hydrate {
            if !queue.is_empty() {
                queue.remove(0);
            }
            if queue.is_empty() {
                if let Some(job_id) = active_job.take() {
                    let until = now_ms + SCHEMA_DIFF_JOB_POLL_MS;
                    self.hydrate = HydratePhase::Polling { job_id, next_poll_at_ms: until };
                    return vec![LifecycleEffect::Wait { until_ms: until }];
                }
                self.hydrate = HydratePhase::Idle;
            }
        }
        Vec::new()
    }

    /// A failed `getJobDetails` read: the error slot, and the queue moves
    /// on (React's `catch` returns null and the loop continues).
    pub fn on_read_error(&mut self, message: String) -> Vec<LifecycleEffect> {
        self.error = Some(message);
        if let HydratePhase::Reading { queue, active_job } = &mut self.hydrate {
            if !queue.is_empty() {
                queue.remove(0);
            }
            if queue.is_empty() {
                if let Some(job_id) = active_job.take() {
                    self.hydrate = HydratePhase::Polling { job_id: job_id.clone(), next_poll_at_ms: 0 };
                    return vec![LifecycleEffect::GetJobDetails(job_id)];
                }
                self.hydrate = HydratePhase::Idle;
            }
        }
        Vec::new()
    }

    /// The clock tick: a due poll re-fires the read.
    pub fn tick(&mut self, now_ms: i64) -> Vec<LifecycleEffect> {
        if let HydratePhase::Polling { job_id, next_poll_at_ms } = &self.hydrate {
            if now_ms >= *next_poll_at_ms {
                let job_id = job_id.clone();
                return vec![LifecycleEffect::GetJobDetails(job_id)];
            }
        }
        Vec::new()
    }

    /// A poll read's answer while polling: still active re-arms the 1000ms
    /// wait; terminal stops the loop. (`on_details` handles the observe +
    /// reading-queue side; this is the polling transition.)
    pub fn on_poll_details(&mut self, now_ms: i64, details: &SchemaDiffJobDetailsVm) -> Vec<LifecycleEffect> {
        let view = &details.details;
        let active = matches!(view.state, JobState::Queued | JobState::Running);
        if let HydratePhase::Polling { job_id, next_poll_at_ms } = &mut self.hydrate {
            if *job_id != view.job_id {
                return Vec::new();
            }
            if active {
                let until = now_ms + SCHEMA_DIFF_JOB_POLL_MS;
                *next_poll_at_ms = until;
                return vec![LifecycleEffect::Wait { until_ms: until }];
            }
            self.hydrate = HydratePhase::Idle;
        }
        Vec::new()
    }

    /// `requestCancel`: only an active tracked job may request; the
    /// outcome starts `requested` and the host's answer lands in
    /// [`Self::on_cancel_acknowledged`] / [`Self::on_cancel_error`].
    pub fn request_cancel(&mut self) -> Option<LifecycleEffect> {
        let active = self
            .current_job
            .as_ref()
            .is_some_and(|job| job.is_active());
        if !active {
            return None;
        }
        self.cancel_outcome = SchemaDiffCancelOutcome::Requested;
        let job_id = self
            .current_job
            .as_ref()
            .map(|job| job.job_id.clone())
            .unwrap_or_default();
        Some(LifecycleEffect::CancelDeploy(job_id))
    }

    /// The cancel answer: `false` (not acknowledged) is `unavailable`.
    pub fn on_cancel_acknowledged(&mut self, acknowledged: bool) {
        if !acknowledged {
            self.cancel_outcome = SchemaDiffCancelOutcome::Unavailable;
        }
    }

    /// A failed cancel: `unavailable` plus the error slot.
    pub fn on_cancel_error(&mut self, message: String) {
        self.cancel_outcome = SchemaDiffCancelOutcome::Unavailable;
        self.error = Some(message);
    }

    /// `verifyRecovery(jobId, targetDbSessionId)`: `verifying` on, the
    /// error slot cleared; the host runs the IPC and delivers the details
    /// ([`Self::observe`], `verifying` off) or the failure.
    pub fn verify_recovery_begin(
        &mut self,
        job_id: &str,
        target_db_session_id: &str,
    ) -> LifecycleEffect {
        self.verifying = true;
        self.error = None;
        LifecycleEffect::VerifyRecovery(job_id.to_owned(), target_db_session_id.to_owned())
    }

    /// A successful verification: observe the details, `verifying` off.
    pub fn on_verify_details(&mut self, details: SchemaDiffJobDetailsVm) {
        self.observe(details);
        self.verifying = false;
    }

    /// A failed verification: the error slot, `verifying` off.
    pub fn on_verify_error(&mut self, message: String) {
        self.error = Some(message);
        self.verifying = false;
    }

    // ---- the render-side reads (the hook's return value) -------------------

    pub fn current_job(&self) -> Option<&SchemaDiffTrackedJobVm> {
        self.current_job.as_ref()
    }

    pub fn latest_apply(&self) -> Option<&SchemaDiffJobDetailsVm> {
        self.latest_apply.as_ref()
    }

    pub fn cancel_outcome(&self) -> SchemaDiffCancelOutcome {
        self.cancel_outcome
    }

    pub fn verifying(&self) -> bool {
        self.verifying
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Parse a `listJobs` wire answer for [`Self::on_list_jobs`].
    pub fn parse_list_jobs(
        &self,
        value: serde_json::Value,
    ) -> Result<Vec<SchemaDiffJobViewVm>, String> {
        parse_job_views(value)
    }
}
