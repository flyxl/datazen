//! Migration-job hydration helpers carried for the
//! `useMigrationJobHydration.ts` port (T376).
//!
//! Source of truth: `packages/frontend/src/lib/migrationJobHydration.ts`
//! (T559 — another segment's ledger entry). This module carries the
//! pieces the hook reads until that task lands and absorbs them — the
//! `grid_clipboard_helpers` / `lib_helpers` precedent:
//!
//! * `MIGRATION_JOB_KINDS` (the per-window kind prefixes) and
//!   `MigrationWindow`,
//! * the `JobView` mirror ([`MigrationJobViewVm`], the
//!   backend-client wire shape camelCase),
//! * `MigrationJobHydration` (the four-bucket projection) and
//!   `hydrateMigrationJobs` (the list→get→merge fold, over the
//!   [`MigrationJobCenter`] trait — the host supplies the client),
//! * `updateMigrationJobProjection` (the watch fold).
//!
//! Not carried (no caller in this port): `classifyJobView`,
//! `STALE_PLAN_ERROR_CODES`, `isStalePlanError`, `latestApplyJob`
//! (the windows' banner faces — T559's own) and
//! `boundBackendClient` (the client binding is the host's seam; the
//! hook's no-client branch is the state machine's).
//!
//! Known deltas: JS `Map` insertion order becomes the `Vec` order the
//! fold already walks (the React code never sorts); `updatedAt`
//! comparisons are the same integer epoch-millisecond comparisons.

use serde::{Deserialize, Serialize};

/// `JobState` (backend-client `types/jobs.ts`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum JobState {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

impl JobState {
    /// The hydrate filter's active set and the projection's active
    /// bucket share this predicate (`'queued' || 'running'`).
    pub(crate) fn is_active(self) -> bool {
        matches!(self, Self::Queued | Self::Running)
    }
}

/// `EffectOutcome` (backend-client `types/session.ts`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum EffectOutcome {
    NotStarted,
    Completed,
    RolledBack,
    PartiallyApplied,
    Unknown,
}

/// `JobProgress`: the P5 five-bucket counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JobProgress {
    pub(crate) read: i64,
    pub(crate) converted: i64,
    pub(crate) attempted: i64,
    pub(crate) committed: i64,
    pub(crate) unknown: i64,
}

/// The `JobView` mirror (backend-client `types/jobs.ts`), camelCase on
/// the wire exactly as the transport speaks.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MigrationJobViewVm {
    pub(crate) job_id: String,
    pub(crate) kind: String,
    pub(crate) state: JobState,
    pub(crate) stage: Option<String>,
    pub(crate) execution_ids: Vec<String>,
    pub(crate) artifact_ids: Vec<String>,
    /// Epoch milliseconds.
    pub(crate) created_at: i64,
    /// Epoch milliseconds.
    pub(crate) updated_at: i64,
    pub(crate) effect_outcome: Option<EffectOutcome>,
    pub(crate) cancel_requested: bool,
    pub(crate) pending_verification_reason: Option<String>,
    pub(crate) error: Option<String>,
    pub(crate) progress: JobProgress,
}

/// `MIGRATION_JOB_KINDS`: per-window job kind prefixes. Prepare and
/// apply are separate Job kinds.
pub(crate) const MIGRATION_JOB_KINDS: MigrationWindowKinds = MigrationWindowKinds {
    schema_diff: ["schemaDiffPrepare", "schemaDiffApply"],
    data_sync: ["dataSyncPrepare", "dataSyncApply"],
    data_transfer: ["dataTransferPrepare", "dataTransferApply"],
};

/// The kinds table (a const struct — Rust has no const maps).
pub(crate) struct MigrationWindowKinds {
    pub(crate) schema_diff: [&'static str; 2],
    pub(crate) data_sync: [&'static str; 2],
    pub(crate) data_transfer: [&'static str; 2],
}

/// `MigrationWindow`: one of the three migration windows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MigrationWindow {
    SchemaDiff,
    DataSync,
    DataTransfer,
}

impl MigrationWindow {
    /// This window's job kinds (`MIGRATION_JOB_KINDS[window]`).
    pub(crate) fn kinds(self) -> &'static [&'static str; 2] {
        match self {
            Self::SchemaDiff => &MIGRATION_JOB_KINDS.schema_diff,
            Self::DataSync => &MIGRATION_JOB_KINDS.data_sync,
            Self::DataTransfer => &MIGRATION_JOB_KINDS.data_transfer,
        }
    }

    /// `isOurKind(job)`: whether a job belongs to this window.
    pub(crate) fn owns(self, job: &MigrationJobViewVm) -> bool {
        self.kinds().contains(&job.kind.as_str())
    }
}

/// `MigrationJobHydration`: the window's projection.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct MigrationJobHydration {
    /// Latest visible projection for this window, including completed
    /// jobs.
    pub(crate) jobs: Vec<MigrationJobViewVm>,
    /// Jobs of this window's kinds that are still queued/running.
    pub(crate) active_jobs: Vec<MigrationJobViewVm>,
    /// Jobs of this window's kinds that reached a terminal state.
    pub(crate) terminal_jobs: Vec<MigrationJobViewVm>,
    /// Terminal or paused jobs whose effect outcome still needs a
    /// human decision.
    pub(crate) verification_jobs: Vec<MigrationJobViewVm>,
}

/// The job-center half the hydration fold reads: the host supplies the
/// bound backend client behind it (`listJobs` / `getJob`).
pub(crate) trait MigrationJobCenter {
    /// `client.listJobs({ states })` — `None` is the empty filter.
    /// The error is the message the hook surfaces.
    fn list_jobs(&self, states: Option<&[JobState]>) -> Result<Vec<MigrationJobViewVm>, String>;
    /// `client.getJob(jobId)`.
    fn get_job(&self, job_id: &str) -> Result<MigrationJobViewVm, String>;
}

/// Recompute the three derived buckets over `jobs` (the order the
/// React filters leave).
fn rebucket(jobs: Vec<MigrationJobViewVm>) -> MigrationJobHydration {
    let active_jobs = jobs.iter().filter(|j| j.state.is_active()).cloned().collect();
    let terminal_jobs = jobs.iter().filter(|j| !j.state.is_active()).cloned().collect();
    let verification_jobs = jobs
        .iter()
        .filter(|j| j.pending_verification_reason.is_some())
        .cloned()
        .collect();
    MigrationJobHydration { jobs, active_jobs, terminal_jobs, verification_jobs }
}

/// `hydrateMigrationJobs(client, window)`: query the job center
/// first. The callers read plans/results only after this has run, so
/// a window never hydrates from stale local state.
pub(crate) fn hydrate_migration_jobs(
    center: &dyn MigrationJobCenter,
    window: MigrationWindow,
) -> Result<MigrationJobHydration, String> {
    let active_summaries: Vec<MigrationJobViewVm> = center
        .list_jobs(Some(&[JobState::Queued, JobState::Running]))?
        .into_iter()
        .filter(|job| window.owns(job))
        .collect();
    let mut refreshed_active: Vec<MigrationJobViewVm> = Vec::new();
    for summary in &active_summaries {
        // A job that vanished between list/get is simply not reported.
        if let Ok(job) = center.get_job(&summary.job_id) {
            refreshed_active.push(job);
        }
    }
    let summaries: Vec<MigrationJobViewVm> =
        center.list_jobs(None)?.into_iter().filter(|job| window.owns(job)).collect();
    // The `Map` fold: summaries in insertion order, a fresher active
    // read replacing its summary in place, a job the second listing
    // no longer knows appended at the end.
    let mut merged: Vec<MigrationJobViewVm> = Vec::with_capacity(summaries.len());
    for summary in summaries {
        let replaced = refreshed_active.iter().find_map(|job| {
            (job.job_id == summary.job_id
                && job.updated_at > summary.updated_at)
                .then(|| job.clone())
        });
        merged.push(replaced.unwrap_or(summary));
    }
    for job in refreshed_active {
        if !merged.iter().any(|j| j.job_id == job.job_id) {
            merged.push(job);
        }
    }
    Ok(rebucket(merged))
}

/// `updateMigrationJobProjection(current, updated)`: replace one
/// watched job in the window projection and recalculate its buckets.
/// A stale update (older `updatedAt`) or one that would resurrect a
/// terminal job leaves the projection untouched.
pub(crate) fn update_migration_job_projection(
    current: &MigrationJobHydration,
    updated: MigrationJobViewVm,
) -> MigrationJobHydration {
    let Some(index) = current.jobs.iter().position(|job| job.job_id == updated.job_id) else {
        let mut jobs = current.jobs.clone();
        jobs.push(updated);
        return rebucket(jobs);
    };
    let existing = &current.jobs[index];
    if !existing.state.is_active() || updated.updated_at < existing.updated_at {
        // The React fold returns `current` itself; the port returns an
        // equal projection (Rust has no reference identity to keep).
        return current.clone();
    }
    let mut jobs = current.jobs.clone();
    jobs[index] = updated;
    rebucket(jobs)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The React suite's `jobView` fixture shape.
    fn job_view(job_id: &str, overrides: impl FnOnce(&mut MigrationJobViewVm)) -> MigrationJobViewVm {
        let mut job = MigrationJobViewVm {
            job_id: job_id.to_owned(),
            kind: "dataSyncApply".to_owned(),
            state: JobState::Running,
            stage: None,
            execution_ids: Vec::new(),
            artifact_ids: Vec::new(),
            created_at: 1_700_000_000_000,
            updated_at: 1_700_000_000_000,
            effect_outcome: None,
            cancel_requested: false,
            pending_verification_reason: None,
            error: None,
            progress: JobProgress::default(),
        };
        overrides(&mut job);
        job
    }

    #[test]
    fn kinds_cover_all_three_windows_with_prepare_and_apply() {
        for kinds in [
            &MIGRATION_JOB_KINDS.schema_diff,
            &MIGRATION_JOB_KINDS.data_sync,
            &MIGRATION_JOB_KINDS.data_transfer,
        ] {
            assert!(kinds.iter().any(|k| k.ends_with("Prepare")));
            assert!(kinds.iter().any(|k| k.ends_with("Apply")));
        }
        assert_eq!(MigrationWindow::SchemaDiff.kinds(), &["schemaDiffPrepare", "schemaDiffApply"]);
        assert_eq!(MigrationWindow::DataSync.kinds(), &["dataSyncPrepare", "dataSyncApply"]);
        assert_eq!(MigrationWindow::DataTransfer.kinds(), &["dataTransferPrepare", "dataTransferApply"]);
    }

    /// The recording client: the call order and answers the React
    /// mock pins.
    struct RecordingCenter {
        order: std::cell::RefCell<Vec<String>>,
    }

    impl MigrationJobCenter for RecordingCenter {
        fn list_jobs(&self, states: Option<&[JobState]>) -> Result<Vec<MigrationJobViewVm>, String> {
            self.order.borrow_mut().push("listJobs".to_owned());
            if states.is_some() {
                return Ok(vec![job_view("j1", |job| {
                    job.kind = "dataSyncApply".to_owned();
                    job.state = JobState::Running;
                })]);
            }
            Ok(vec![
                job_view("j1", |job| {
                    job.state = JobState::Cancelled;
                    job.cancel_requested = true;
                    job.updated_at = 1_700_000_002_000;
                }),
                job_view("j3", |job| {
                    job.state = JobState::Failed;
                    job.pending_verification_reason = Some("outcomeUnknown".to_owned());
                }),
                job_view("j2", |job| {
                    job.kind = "workflow".to_owned();
                    job.pending_verification_reason = Some("outcomeUnknown".to_owned());
                }),
            ])
        }

        fn get_job(&self, job_id: &str) -> Result<MigrationJobViewVm, String> {
            self.order.borrow_mut().push(format!("getJob:{job_id}"));
            Ok(job_view(job_id, |_| {}))
        }
    }

    #[test]
    fn hydrate_queries_the_center_first_and_filters_by_window() {
        let center = RecordingCenter { order: std::cell::RefCell::new(Vec::new()) };
        let result = hydrate_migration_jobs(&center, MigrationWindow::DataSync).expect("hydrates");
        assert_eq!(
            *center.order.borrow(),
            vec!["listJobs".to_owned(), "getJob:j1".to_owned(), "listJobs".to_owned()]
        );
        assert!(result.active_jobs.is_empty());
        assert_eq!(result.verification_jobs.len(), 1);
        assert_eq!(result.verification_jobs[0].kind, "dataSyncApply");
        let terminal_ids: Vec<&str> = result.terminal_jobs.iter().map(|j| j.job_id.as_str()).collect();
        assert_eq!(terminal_ids, ["j1", "j3"]);
        let ids: Vec<&str> = result.jobs.iter().map(|j| j.job_id.as_str()).collect();
        assert_eq!(ids, ["j1", "j3"], "the workflow-kind j2 is not this window's");
    }

    #[test]
    fn a_vanished_get_job_is_simply_not_reported() {
        struct VanishingCenter;
        impl MigrationJobCenter for VanishingCenter {
            fn list_jobs(&self, states: Option<&[JobState]>) -> Result<Vec<MigrationJobViewVm>, String> {
                if states.is_some() {
                    Ok(vec![job_view("j1", |job| job.state = JobState::Running)])
                } else {
                    // The second listing no longer knows the job either:
                    // the fold must still answer (empty), not error.
                    Ok(Vec::new())
                }
            }
            fn get_job(&self, _job_id: &str) -> Result<MigrationJobViewVm, String> {
                Err("job vanished".to_owned())
            }
        }
        let result = hydrate_migration_jobs(&VanishingCenter, MigrationWindow::DataSync).expect("hydrates");
        assert!(result.jobs.is_empty());
        assert!(result.active_jobs.is_empty());
    }

    #[test]
    fn a_fresher_active_read_replaces_its_summary_in_place() {
        struct FresherCenter;
        impl MigrationJobCenter for FresherCenter {
            fn list_jobs(&self, states: Option<&[JobState]>) -> Result<Vec<MigrationJobViewVm>, String> {
                if states.is_some() {
                    Ok(vec![job_view("j1", |job| job.state = JobState::Running)])
                } else {
                    Ok(vec![
                        job_view("j0", |job| {
                            job.kind = "dataSyncPrepare".to_owned();
                            job.state = JobState::Succeeded;
                        }),
                        job_view("j1", |job| {
                            job.state = JobState::Running;
                            job.updated_at = 1_700_000_001_000;
                        }),
                    ])
                }
            }
            fn get_job(&self, job_id: &str) -> Result<MigrationJobViewVm, String> {
                // The fresh read is newer than the summary's snapshot.
                Ok(job_view(job_id, |job| {
                    job.updated_at = 1_700_000_003_000;
                    job.progress.read = 42;
                }))
            }
        }
        let result = hydrate_migration_jobs(&FresherCenter, MigrationWindow::DataSync).expect("hydrates");
        // Order preserved (j0 first, j1 in place), and the fresh read won.
        let ids: Vec<&str> = result.jobs.iter().map(|j| j.job_id.as_str()).collect();
        assert_eq!(ids, ["j0", "j1"]);
        assert_eq!(result.jobs[1].updated_at, 1_700_000_003_000);
        assert_eq!(result.jobs[1].progress.read, 42);
        assert_eq!(result.active_jobs.len(), 1);
    }

    #[test]
    fn a_watch_update_moves_a_job_from_active_to_terminal() {
        let active = job_view("j1", |job| {
            job.kind = "dataTransferApply".to_owned();
        });
        let terminal = MigrationJobViewVm { state: JobState::Cancelled, cancel_requested: true, ..active.clone() };
        let initial = MigrationJobHydration {
            jobs: vec![active.clone()],
            active_jobs: vec![active],
            terminal_jobs: Vec::new(),
            verification_jobs: Vec::new(),
        };
        let updated = update_migration_job_projection(&initial, terminal.clone());
        assert_eq!(updated.jobs, vec![terminal.clone()]);
        assert!(updated.active_jobs.is_empty());
        assert_eq!(updated.terminal_jobs, vec![terminal]);
        assert!(updated.verification_jobs.is_empty());
    }

    #[test]
    fn a_stale_watch_update_cannot_move_a_terminal_job_back_to_active() {
        let terminal = job_view("j1", |job| {
            job.kind = "dataTransferApply".to_owned();
            job.state = JobState::Cancelled;
            job.updated_at = 1_700_000_002_000;
        });
        let stale_active = job_view("j1", |job| {
            job.kind = "dataTransferApply".to_owned();
            job.state = JobState::Running;
            job.updated_at = 1_700_000_001_000;
        });
        let current = MigrationJobHydration {
            jobs: vec![terminal.clone()],
            active_jobs: Vec::new(),
            terminal_jobs: vec![terminal.clone()],
            verification_jobs: Vec::new(),
        };
        let updated = update_migration_job_projection(&current, stale_active);
        assert_eq!(updated.jobs, vec![terminal.clone()]);
        assert!(updated.active_jobs.is_empty());
        assert_eq!(updated.terminal_jobs, vec![terminal]);
    }

    #[test]
    fn an_unknown_job_is_appended_with_its_buckets_recomputed() {
        let current = MigrationJobHydration::default();
        let fresh = job_view("j9", |job| {
            job.pending_verification_reason = Some("cleanupNotConfirmed".to_owned());
        });
        let updated = update_migration_job_projection(&current, fresh.clone());
        assert_eq!(updated.jobs, vec![fresh.clone()]);
        // Still active AND pending verification (the buckets are not
        // mutually exclusive).
        assert_eq!(updated.active_jobs, vec![fresh.clone()]);
        assert_eq!(updated.verification_jobs, vec![fresh]);
        assert!(updated.terminal_jobs.is_empty());
    }
}
