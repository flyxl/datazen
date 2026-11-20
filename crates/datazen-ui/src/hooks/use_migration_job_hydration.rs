//! Port of `packages/frontend/src/hooks/useMigrationJobHydration.ts`
//! (T376).
//!
//! Per-window job hydration for the three migration windows. Mounts
//! with a job-center-first read: listJobs → getJob. Re-subscribing to
//! an active job happens through `watchJob` if the window opts in.
//!
//! ## What maps where
//!
//! React's effect becomes the embeddable state machine
//! [`UseMigrationJobHydration`]; the async client reads happen on the
//! host, which feeds the answers back:
//!
//! - the mount's **no-client branch** (`boundBackendClient()` null)
//!   is [`mount_without_client`]: the empty four-bucket hydration, no
//!   error;
//! - the async block's success arm is [`on_hydrated`]: install the
//!   projection and **start a watch per active job** (the
//!   [`HydrationEffect::WatchJob`] effects the host drains, with the
//!   `intervalMs: 2000` the React test pins —
//!   [`WATCH_INTERVAL_MS`]);
//! - the failure arm is [`on_hydration_error`] (the message is the
//!   host's `error.message` translation);
//! - one `watchJob` update is [`on_watch_update`]: the projection
//!   fold, then the **terminal stop** (a job that left
//!   queued/running stops its own watch — exactly once);
//! - the unmount rule is [`unmount`]: the cleanup only flags the loop
//!   as cancelled and stops the watch. No `cancelJob`, `removeJob` or
//!   session `release` call may be made here — a window closing is an
//!   unsubscribe, never a resource release (the effect vocabulary has
//!   no cancel arm, by construction).
//!
//! The `unsubscribed`/`cancelledRef` gates become one rule: after
//! [`unmount`] no callback mutates state (every feed returns `None`).
//!
//! The hydration fold itself is carried in
//! [`super::migration_job_hydration_helpers`] (its source file is
//! another segment's ledger entry, T559).
//!
//! Known deltas: a window **change** re-runs the React effect
//! (cleanup + re-mount); here it is a fresh
//! [`UseMigrationJobHydration::new`] — the instance's window is its
//! identity, and the cleanup-before-remount order is the host's
//! drain.

use crate::hooks::migration_job_hydration_helpers::{
    MigrationJobHydration, MigrationJobViewVm, MigrationWindow,
};

/// The `watchJob` subscription interval the React source pins
/// (`intervalMs: 2000`).
pub(crate) const WATCH_INTERVAL_MS: u32 = 2000;

/// What the host must do: start/stop one job watch. There is
/// deliberately no cancel arm — the unmount rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum HydrationEffect {
    /// `client.watchJob(jobId, …, { intervalMs: 2000 })`.
    WatchJob { job_id: String },
    /// `watch.stop()` — emitted at most once per watch.
    StopWatch { job_id: String },
}

/// One active job's subscription state (the React `handleState`).
#[derive(Clone, Debug, PartialEq, Eq)]
struct Watch {
    job_id: String,
    stopped: bool,
}

/// `useMigrationJobHydration(window)`'s state machine.
pub(crate) struct UseMigrationJobHydration {
    window: MigrationWindow,
    hydration: Option<MigrationJobHydration>,
    hydration_error: Option<String>,
    /// `cancelledRef` + the cleanup's `unsubscribed`.
    cancelled: bool,
    watches: Vec<Watch>,
}

impl UseMigrationJobHydration {
    /// `useMigrationJobHydration(window)` — the hook's initial state
    /// (null hydration, null error, nothing subscribed).
    pub(crate) fn new(window: MigrationWindow) -> Self {
        Self {
            window,
            hydration: None,
            hydration_error: None,
            cancelled: false,
            watches: Vec::new(),
        }
    }

    /// The window this instance hydrates.
    pub(crate) fn window(&self) -> MigrationWindow {
        self.window
    }

    /// The mount's no-client branch: an empty hydration — all four
    /// buckets empty — and no error (nothing is bound, e.g. in
    /// tests).
    pub(crate) fn mount_without_client(&mut self) {
        self.hydration = Some(MigrationJobHydration::default());
        self.hydration_error = None;
    }

    /// The async block's success arm: install the hydration and
    /// start a watch per active job. Empty when the loop was already
    /// cancelled (the `unsubscribed || cancelledRef.current` gate).
    pub(crate) fn on_hydrated(&mut self, result: MigrationJobHydration) -> Vec<HydrationEffect> {
        if self.cancelled {
            return Vec::new();
        }
        self.hydration = Some(result.clone());
        self.hydration_error = None;
        self.watches = result
            .active_jobs
            .iter()
            .map(|job| Watch { job_id: job.job_id.clone(), stopped: false })
            .collect();
        self.watches
            .iter()
            .map(|w| HydrationEffect::WatchJob { job_id: w.job_id.clone() })
            .collect()
    }

    /// The async block's failure arm: surface the message. A no-op
    /// once cancelled.
    pub(crate) fn on_hydration_error(&mut self, message: String) {
        if self.cancelled {
            return;
        }
        self.hydration_error = Some(message);
    }

    /// One `watchJob` update: fold the job into the projection, then
    /// stop the watch when the job left queued/running. `None` when
    /// the loop was already cancelled — no state may change after
    /// unmount.
    pub(crate) fn on_watch_update(
        &mut self,
        latest: MigrationJobViewVm,
    ) -> Option<Vec<HydrationEffect>> {
        if self.cancelled {
            return None;
        }
        // `setHydration(current => current ? update… : current)`.
        if let Some(current) = self.hydration.take() {
            self.hydration = Some(crate::hooks::migration_job_hydration_helpers::
                update_migration_job_projection(&current, latest.clone()));
        }
        // The terminal stop: once, and only for a watched job.
        let mut effects = Vec::new();
        if !latest.state.is_active() {
            if let Some(watch) = self.watches.iter_mut().find(|w| w.job_id == latest.job_id) {
                if !watch.stopped {
                    watch.stopped = true;
                    effects.push(HydrationEffect::StopWatch { job_id: latest.job_id.clone() });
                }
            }
        }
        Some(effects)
    }

    /// The unmount rule: unsubscribe only — stop local polling, never
    /// cancel the job itself. Idempotent; a watch already stopped by
    /// its terminal update is not stopped twice.
    pub(crate) fn unmount(&mut self) -> Vec<HydrationEffect> {
        self.cancelled = true;
        self.watches
            .iter_mut()
            .filter(|w| !w.stopped)
            .map(|w| {
                w.stopped = true;
                HydrationEffect::StopWatch { job_id: w.job_id.clone() }
            })
            .collect()
    }

    /// `result.hydration`.
    pub(crate) fn hydration(&self) -> Option<&MigrationJobHydration> {
        self.hydration.as_ref()
    }

    /// `result.hydrationError`.
    pub(crate) fn hydration_error(&self) -> Option<&str> {
        self.hydration_error.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hooks::migration_job_hydration_helpers::{
        EffectOutcome, JobProgress, JobState, MigrationJobCenter,
    };

    /// The React suite's `jobView` fixture, verbatim shape.
    fn job_view(overrides: impl FnOnce(&mut MigrationJobViewVm)) -> MigrationJobViewVm {
        let mut job = MigrationJobViewVm {
            job_id: "transfer-apply-1".to_owned(),
            kind: "dataTransferApply".to_owned(),
            state: JobState::Running,
            stage: Some("data".to_owned()),
            execution_ids: Vec::new(),
            artifact_ids: Vec::new(),
            created_at: 1,
            updated_at: 1,
            effect_outcome: None,
            cancel_requested: false,
            pending_verification_reason: None,
            error: None,
            progress: JobProgress {
                read: 1,
                converted: 1,
                attempted: 1,
                committed: 0,
                unknown: 0,
            },
        };
        overrides(&mut job);
        job
    }

    fn hydrated_with_active() -> MigrationJobHydration {
        MigrationJobHydration {
            jobs: vec![job_view(|_| {})],
            active_jobs: vec![job_view(|_| {})],
            terminal_jobs: Vec::new(),
            verification_jobs: Vec::new(),
        }
    }

    #[test]
    fn the_no_client_branch_is_the_empty_hydration() {
        let mut hook = UseMigrationJobHydration::new(MigrationWindow::DataTransfer);
        assert!(hook.hydration().is_none());
        hook.mount_without_client();
        let hydration = hook.hydration().expect("installed");
        assert!(hydration.jobs.is_empty());
        assert!(hydration.active_jobs.is_empty());
        assert!(hydration.terminal_jobs.is_empty());
        assert!(hydration.verification_jobs.is_empty());
        assert_eq!(hook.hydration_error(), None);
    }

    #[test]
    fn hydration_installs_the_projection_and_watches_every_active_job() {
        let mut hook = UseMigrationJobHydration::new(MigrationWindow::DataTransfer);
        let effects = hook.on_hydrated(hydrated_with_active());
        assert_eq!(
            effects,
            vec![HydrationEffect::WatchJob { job_id: "transfer-apply-1".to_owned() }]
        );
        // The interval the watch must use (the React test's
        // `expect.objectContaining({ intervalMs: 2000 })`).
        assert_eq!(WATCH_INTERVAL_MS, 2000);
        assert_eq!(hook.hydration().unwrap().active_jobs.len(), 1);
        assert_eq!(hook.hydration().unwrap().jobs[0].progress.read, 1);
        assert_eq!(hook.hydration_error(), None);
    }

    #[test]
    fn a_terminal_update_moves_the_job_and_stops_its_watch_once() {
        let mut hook = UseMigrationJobHydration::new(MigrationWindow::DataTransfer);
        hook.on_hydrated(hydrated_with_active());
        let terminal = job_view(|job| {
            job.state = JobState::Cancelled;
            job.cancel_requested = true;
            job.effect_outcome = Some(EffectOutcome::RolledBack);
            job.updated_at = 2;
        });
        let effects = hook.on_watch_update(terminal.clone()).expect("applied");
        assert_eq!(
            effects,
            vec![HydrationEffect::StopWatch { job_id: "transfer-apply-1".to_owned() }]
        );
        let hydration = hook.hydration().expect("projection");
        assert!(hydration.active_jobs.is_empty());
        assert_eq!(hydration.terminal_jobs, vec![terminal.clone()]);
        assert_eq!(hydration.jobs, vec![terminal]);
    }

    #[test]
    fn a_stale_or_late_update_cannot_resurrect_a_terminal_job() {
        let mut hook = UseMigrationJobHydration::new(MigrationWindow::DataTransfer);
        hook.on_hydrated(hydrated_with_active());
        let settled = job_view(|job| {
            job.state = JobState::Cancelled;
            job.updated_at = 2;
        });
        hook.on_watch_update(settled.clone()).expect("applied");
        let before = hook.hydration().expect("projection").clone();
        // Older than the terminal view: refused wholesale.
        let stale = job_view(|job| {
            job.state = JobState::Running;
            job.updated_at = 1;
        });
        hook.on_watch_update(stale).expect("gate is on the fold, not the callback");
        assert_eq!(hook.hydration().unwrap(), &before);
    }

    #[test]
    fn an_unmounted_hook_answers_nothing_and_stops_only_once() {
        let mut hook = UseMigrationJobHydration::new(MigrationWindow::DataTransfer);
        hook.on_hydrated(hydrated_with_active());
        // The terminal update stops the watch…
        let terminal = job_view(|job| {
            job.state = JobState::Cancelled;
            job.updated_at = 2;
        });
        hook.on_watch_update(terminal).expect("applied");
        // …so the unmount has nothing left to stop — and never
        // cancels (no cancel arm exists to call).
        let effects = hook.unmount();
        assert!(effects.is_empty());
        // After unmount no callback mutates state.
        assert!(hook.on_watch_update(job_view(|_| {})).is_none());
        let frozen = hook.hydration().expect("projection").clone();
        hook.on_hydrated(hydrated_with_active());
        assert_eq!(hook.hydration().unwrap(), &frozen);
        hook.on_hydration_error("late".to_owned());
        assert_eq!(hook.hydration_error(), None);
    }

    #[test]
    fn unmount_stops_a_still_running_watch_and_only_that() {
        let mut hook = UseMigrationJobHydration::new(MigrationWindow::DataTransfer);
        hook.on_hydrated(hydrated_with_active());
        // A running update does not stop anything.
        let effects = hook.on_watch_update(job_view(|_| {})).expect("applied");
        assert!(effects.is_empty());
        // Unmount unsubscribes — exactly once.
        let effects = hook.unmount();
        assert_eq!(
            effects,
            vec![HydrationEffect::StopWatch { job_id: "transfer-apply-1".to_owned() }]
        );
        assert!(hook.unmount().is_empty(), "idempotent");
    }

    #[test]
    fn the_error_arm_surfaces_the_message() {
        let mut hook = UseMigrationJobHydration::new(MigrationWindow::SchemaDiff);
        hook.on_hydration_error("job center unreachable".to_owned());
        assert_eq!(hook.hydration_error(), Some("job center unreachable"));
        assert!(hook.hydration().is_none());
    }
}
