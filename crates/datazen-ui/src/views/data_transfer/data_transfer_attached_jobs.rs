//! T073: `windows/data-transfer/DataTransferAttachedJobs.tsx` ported to Rune.
//!
//! The strip above the Data Transfer wizard steps that re-attaches to
//! already-running `dataTransferApply` jobs (mounted by
//! `windows/data-transfer/DataTransferWindow.tsx:1095`, fed by
//! `migrationJobs.hydration.activeJobs` minus the current run's
//! `applyView.jobId`) and offers a cancel-request button per job. It renders
//! nothing when no job passes the filter.
//!
//! The cancel itself is async and host-owned: React awaits
//! `transferJobCommands.cancel(jobId)` (`cancel_data_transfer` IPC,
//! `commands/transferJobs.ts:205`), where `false` means "no such job" and a
//! rejection means the outcome is unknown — both are surfaced, never treated
//! as a successful cancel. Rune is immediate mode, so the `canceling` set and
//! the `cancelResult` map live in the app's state `S` as an
//! [`AttachedJobsState`], and the async call is the host's `on_cancel`
//! callback: the press handler first runs the host's `begin_cancel` (which
//! calls [`AttachedJobsState::begin_cancel`]) and only fires `on_cancel` when
//! the guard lets it through. The host applies the outcome later with
//! [`AttachedJobsState::finish_cancel`] on the same state slot.
//!
//! This component has no production consumer yet; it will be wired by the
//! Data Transfer window track (React mounts it in
//! `windows/data-transfer/DataTransferWindow.tsx:1095`). Reusable API items
//! carry per-item `#[allow(dead_code)]` (not a module-wide blanket) until
//! that caller lands.
//!
//! ## Keys
//!
//! React gives every job row the same `data-testid`
//! (`data-transfer-attached-job`) and tells rows apart with `data-job-id`;
//! Rune keys must be unique, so the job id moves into the key itself.
//!
//! | part | key |
//! |------|-----|
//! | root (absent when no job passes the filter) | `data-transfer-attached-jobs` |
//! | job row | `data-transfer-attached-job-<jobId>` |
//! | state line | `…-state` |
//! | progress line | `…-progress` |
//! | cancel-requested note | `…-cancel-requested` |
//! | cancel-unknown note | `…-cancel-unknown` |
//! | cancel button | `…-cancel` |
//!
//! ## Deliberate deviations
//!
//! * **No `data-state` / `data-cancel-requested` attributes.** Rune's `El`
//!   has no arbitrary data attributes; the job state is visible in the state
//!   line's text and the cancel disposition in the note plus the button's
//!   disabled state, which is what the tests assert.
//! * **One `gap` for the row.** React's row has `gap-x-4 gap-y-2`; Rune rows
//!   take a single gap, so both axes use 16pt (the horizontal value).
//! * **Kit ghost button.** React's shared `Button` defaults to `md` (36pt,
//!   13px); the Rune button is the kit ghost at `Small`, the same choice
//!   T024 made for its wizard cancel button.
//! * **Cancel-requested wins over cancel-unknown**, exactly as React's
//!   ternary order (`cancelSent ? … : cancelUnknown ? … : null`): a job
//!   whose backend flag already says cancel-requested never shows the
//!   unknown note, even if a local cancel returned `false`.
//! * **`begin_cancel` guards re-entry.** React's `requestCancel` would fire
//!   a second invoke if a press somehow arrived while one was in flight
//!   (its `Set.add` is idempotent but the invoke is not); the button is
//!   disabled while canceling, so the guard only covers a press that
//!   bypasses the button, and the rendered behaviour is identical.

use std::collections::{HashMap, HashSet};

use rune::{col, el, row, text, El};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::i18n::t;
use crate::ui::kit::kit;

/// JS `Number.MAX_SAFE_INTEGER` (`2^53 - 1`): the largest integer the
/// string branch of [`read_job_counter`] trusts, exactly as React's
/// `Number.isSafeInteger` on the string branch. The number branch has no
/// safe-integer gate.
const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

/// React `JobState` (`backend-client/types/jobs.ts:55`):
/// `'queued' | 'running' | 'succeeded' | 'failed' | 'cancelled'`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // reusable component; wired by the Data Transfer window track
pub enum JobState {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

#[allow(dead_code)] // reusable component; wired by the Data Transfer window track
impl JobState {
    /// The wire spelling, for the host that decodes the job projection.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    /// Parses the wire spelling; anything else is not a state React could
    /// have produced, so it is `None` rather than a silent default.
    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "queued" => Some(Self::Queued),
            "running" => Some(Self::Running),
            "succeeded" => Some(Self::Succeeded),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }
}

/// The two runtime forms a progress bucket can arrive in. React types the
/// bucket `Counter` (a decimal string on the wire — the Rust counter
/// serializes with `collect_str`) but `readJobCounter` also accepts a JSON
/// number, so the value keeps the forms distinct instead of stringifying up
/// front.
#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)] // reusable component; wired by the Data Transfer window track
pub enum JobCounter {
    /// A JSON number.
    Number(f64),
    /// A decimal string — the encoding Rust actually emits.
    Text(String),
}

/// React `JobProgress` (`backend-client/types/jobs.ts:62-68`): the P5
/// five-bucket progress counters. Each bucket is `Option` because the wire
/// may leave it absent (`null` / `undefined` on the React side), which
/// [`read_job_counter`] reports as unreadable and the counts line renders
/// as `0`.
#[derive(Clone, Debug, Default, PartialEq)]
#[allow(dead_code)] // reusable component; wired by the Data Transfer window track
pub struct JobProgress {
    pub read: Option<JobCounter>,
    pub converted: Option<JobCounter>,
    pub attempted: Option<JobCounter>,
    pub committed: Option<JobCounter>,
    pub unknown: Option<JobCounter>,
}

/// `readJobCounter` (`lib/migrationJobVerdict.ts:124-141`): read a progress
/// counter, or `None` when it cannot be trusted. Numbers must be finite and
/// non-negative (fractional parts truncate, as `Math.trunc` does); there is
/// no safe-integer gate on the number branch — React only checks
/// `Number.isFinite(value) && value >= 0`. Strings are trimmed first and must
/// then be one or more ASCII digits that parse to a safe integer.
/// Out of model: a JSON number above `u64::MAX` saturates to `u64::MAX` on
/// the `as u64` cast, where JS would print it in exponent form (e.g. `1e+30`)
/// in the counts line; `u64` cannot express it. The real wire form is a
/// decimal string, so this only bites on a JSON number input, never on a
/// real backend counter.
#[allow(dead_code)] // reusable component; wired by the Data Transfer window track
pub fn read_job_counter(value: Option<&JobCounter>) -> Option<u64> {
    match value {
        None => None,
        Some(JobCounter::Number(value)) => {
            if !value.is_finite() || *value < 0.0 {
                return None;
            }
            let truncated = value.trunc();
            // No safe-integer gate here: React's number branch is only
            // `Number.isFinite(value) && value >= 0 ? Math.trunc(value) : null`
            // (`lib/migrationJobVerdict.ts:124-131`). The safe-integer gate
            // lives only on the string branch below.
            Some(truncated as u64)
        }
        Some(JobCounter::Text(value)) => {
            // JS `String.trim` also strips U+FEFF, which Rust's
            // `char::is_whitespace` does not cover.
            let trimmed = value.trim_matches(|c: char| c.is_whitespace() || c == '\u{FEFF}');
            if trimmed.is_empty() || !trimmed.bytes().all(|byte| byte.is_ascii_digit()) {
                return None;
            }
            let parsed: u64 = trimmed.parse().ok()?;
            if parsed > MAX_SAFE_INTEGER {
                return None;
            }
            Some(parsed)
        }
    }
}

/// `readJobCounter(value) ?? 0`: an unreadable bucket counts as zero in the
/// counts line, exactly as the React `.map((value) => readJobCounter(value)
/// ?? 0)` does.
#[allow(dead_code)] // reusable component; wired by the Data Transfer window track
pub fn job_counter_or_zero(value: Option<&JobCounter>) -> u64 {
    read_job_counter(value).unwrap_or(0)
}

/// The counts line: `read / converted / attempted / committed / unknown`,
/// in that order — React's
/// `[read, converted, attempted, committed, unknown].map(… ?? 0).join(' / ')`.
#[allow(dead_code)] // reusable component; wired by the Data Transfer window track
pub fn progress_counts_text(progress: &JobProgress) -> String {
    [
        job_counter_or_zero(progress.read.as_ref()),
        job_counter_or_zero(progress.converted.as_ref()),
        job_counter_or_zero(progress.attempted.as_ref()),
        job_counter_or_zero(progress.committed.as_ref()),
        job_counter_or_zero(progress.unknown.as_ref()),
    ]
    .iter()
    .map(u64::to_string)
    .collect::<Vec<_>>()
    .join(" / ")
}

/// The fields of `JobView` (`backend-client/types/jobs.ts:92-118`) this
/// component reads. The rest of the projection (execution/artifact ids,
/// timestamps, effect outcome, pending-verification reason, error) never
/// reaches this strip, so it is not modelled here.
#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)] // reusable component; wired by the Data Transfer window track
pub struct AttachedJob {
    pub job_id: String,
    pub kind: String,
    pub state: JobState,
    pub stage: Option<String>,
    pub cancel_requested: bool,
    pub progress: JobProgress,
}

/// React's `activeJobs` filter (`DataTransferAttachedJobs.tsx:23-28`):
/// only `dataTransferApply` jobs that are still queued or running, minus the
/// current run's job id. `None` excludes nothing, as React's
/// `jobId !== undefined` never excludes.
#[allow(dead_code)] // reusable component; wired by the Data Transfer window track
pub fn active_attached_jobs<'a>(
    jobs: &'a [AttachedJob],
    exclude_job_id: Option<&str>,
) -> Vec<&'a AttachedJob> {
    jobs.iter()
        .filter(|job| {
            job.kind == "dataTransferApply"
                && matches!(job.state, JobState::Queued | JobState::Running)
                && exclude_job_id.map_or(true, |excluded| job.job_id != excluded)
        })
        .collect()
}

/// The state line: `t('transfer.job.<state>')`, plus ` · <stage>` when the
/// stage is a non-empty string — React's `job.stage ? ` · ${job.stage}` :
/// ''` (an empty stage is falsy there too).
#[allow(dead_code)] // reusable component; wired by the Data Transfer window track
pub fn job_state_label(job: &AttachedJob) -> String {
    let key = format!("transfer.job.{}", job.state.as_str());
    let mut label = t(&key).to_owned();
    if let Some(stage) = job.stage.as_deref().filter(|stage| !stage.is_empty()) {
        label.push_str(" · ");
        label.push_str(stage);
    }
    label
}

/// React's `cancelSent`: the backend flag, or a local cancel that was
/// accepted.
#[allow(dead_code)] // reusable component; wired by the Data Transfer window track
pub fn cancel_sent(job: &AttachedJob, state: &AttachedJobsState) -> bool {
    job.cancel_requested || state.cancel_result.get(&job.job_id) == Some(&true)
}

/// React's `cancelUnknown`: a local cancel that came back `false` ("no such
/// job"). Checked only after [`cancel_sent`], as React's ternary does.
#[allow(dead_code)] // reusable component; wired by the Data Transfer window track
pub fn cancel_unknown(job: &AttachedJob, state: &AttachedJobsState) -> bool {
    state.cancel_result.get(&job.job_id) == Some(&false)
}

/// React's `canceling` set and `cancelResult` map (`useState` in
/// `DataTransferAttachedJobs.tsx:14-17`), kept in the app's state `S`
/// because Rune has no component-local `useState`.
#[derive(Clone, Debug, Default)]
#[allow(dead_code)] // reusable component; wired by the Data Transfer window track
pub struct AttachedJobsState {
    /// Jobs with a cancel request still in flight.
    pub canceling: HashSet<String>,
    /// The last cancel outcome per job: `true` accepted, `false` "no such
    /// job" or rejected.
    pub cancel_result: HashMap<String, bool>,
}

#[allow(dead_code)] // reusable component; wired by the Data Transfer window track
impl AttachedJobsState {
    /// React's `requestCancel` entry: marks the job canceling. Returns
    /// `false` (and changes nothing) when a cancel is already in flight
    /// for the job — the button is disabled while canceling, so this only
    /// guards a press that bypasses the button.
    pub fn begin_cancel(&mut self, job_id: &str) -> bool {
        if self.canceling.contains(job_id) {
            return false;
        }
        self.canceling.insert(job_id.to_owned());
        true
    }

    /// React's `try` / `catch` / `finally`: the in-flight mark always
    /// clears, and the outcome is recorded (`false` on rejection, exactly
    /// as React's `catch` records `false`).
    pub fn finish_cancel(&mut self, job_id: &str, accepted: bool) {
        self.canceling.remove(job_id);
        self.cancel_result.insert(job_id.to_owned(), accepted);
    }
}

/// Reads the attached-jobs state out of the app's `S`.
#[allow(dead_code)] // reusable component; wired by the Data Transfer window track
pub type ReadAttachedJobsStateFn<S> = fn(&S) -> &AttachedJobsState;

/// Runs the in-flight guard on the app's `S`: the host's one-line call into
/// [`AttachedJobsState::begin_cancel`]. Returns whether the cancel may start.
#[allow(dead_code)] // reusable component; wired by the Data Transfer window track
pub type BeginCancelFn<S> = fn(&mut S, &str) -> bool;

/// Starts the host-owned async cancel of one job id (the
/// `cancel_data_transfer` IPC). Only called after [`BeginCancelFn`] allowed
/// it.
#[allow(dead_code)] // reusable component; wired by the Data Transfer window track
pub type CancelJobFn<S> = fn(&mut S, String);

/// The attached-jobs strip. `read_state` and `begin_cancel` locate the state
/// slot in the app's `S`; `on_cancel` is the host's async cancel starter.
#[allow(dead_code)] // reusable component; wired by the Data Transfer window track
pub struct DataTransferAttachedJobs<S: 'static> {
    jobs: Vec<AttachedJob>,
    exclude_job_id: Option<String>,
    read_state: ReadAttachedJobsStateFn<S>,
    begin_cancel: BeginCancelFn<S>,
    on_cancel: Option<CancelJobFn<S>>,
}

#[allow(dead_code)] // reusable component; wired by the Data Transfer window track
impl<S: 'static> DataTransferAttachedJobs<S> {
    pub fn new(
        jobs: Vec<AttachedJob>,
        exclude_job_id: Option<String>,
        read_state: ReadAttachedJobsStateFn<S>,
        begin_cancel: BeginCancelFn<S>,
    ) -> Self {
        Self {
            jobs,
            exclude_job_id,
            read_state,
            begin_cancel,
            on_cancel: None,
        }
    }

    /// The host's async cancel starter; without it the buttons render but
    /// do nothing, like a React strip whose command never resolves.
    pub fn on_cancel(mut self, on_cancel: CancelJobFn<S>) -> Self {
        self.on_cancel = Some(on_cancel);
        self
    }

    /// Builds the strip, or `el()` (nothing) when no job passes the filter
    /// — React's early `return null`.
    pub fn build(self, s: &S, dark: bool) -> El {
        let active = active_attached_jobs(&self.jobs, self.exclude_job_id.as_deref());
        if active.is_empty() {
            return el();
        }
        let k = kit(dark);
        let state = (self.read_state)(s);
        // React: `section.space-y-2 border-b border-edge px-6 py-3`.
        let mut root = col()
            .key("data-transfer-attached-jobs")
            .label(t("transfer.job.attachedTitle"))
            .w_full()
            .gap(8.0)
            .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
            .px(24.0)
            .py(12.0);
        for job in active {
            root = root.child(self.job_row(&k, state, job));
        }
        root
    }

    /// One job row: `flex flex-wrap items-center gap-x-4 gap-y-2 rounded
    /// border border-edge bg-surface-alt px-3 py-2 text-xs`.
    fn job_row(&self, k: &Kit, state: &AttachedJobsState, job: &AttachedJob) -> El {
        let base = format!("data-transfer-attached-job-{}", job.job_id);
        let sent = cancel_sent(job, state);
        let unknown = cancel_unknown(job, state);
        let disabled = state.canceling.contains(&job.job_id) || sent;
        let mut row_el = row()
            .key(base.clone())
            .flex_wrap()
            .items_center()
            .gap(16.0)
            .border(1.0, k.c.border_base)
            .radius(4.0)
            .fill(k.c.layer_01)
            .px(12.0)
            .py(8.0);
        // React: `div.min-w-0 flex-1` with the state line, the progress
        // line, and at most one cancel note.
        let mut info = col()
            .grow(1.0)
            .min_w(0.0)
            .child(
                text(job_state_label(job))
                    .key(format!("{base}-state"))
                    .font(k.ty.text_xsmall_medium)
                    .color(k.c.text_base)
                    .role("status"),
            )
            .child(
                text(format!(
                    "{}: {}",
                    t("migration.progress.counts"),
                    progress_counts_text(&job.progress)
                ))
                .key(format!("{base}-progress"))
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted),
            );
        if sent {
            info = info.child(
                text(t("migration.cancel.requestedInFlight"))
                    .key(format!("{base}-cancel-requested"))
                    .font(k.ty.text_xsmall)
                    .color(k.c.warning.fg),
            );
        } else if unknown {
            info = info.child(
                text(t("migration.cancel.unknownJob"))
                    .key(format!("{base}-cancel-unknown"))
                    .font(k.ty.text_xsmall)
                    .color(k.c.warning.fg),
            );
        }
        row_el = row_el.child(info);
        row_el = row_el.child(self.cancel_button(k, &base, job, disabled));
        row_el
    }

    /// React's ghost `Button`: `disabled={canceling || cancelSent}`. While
    /// disabled the press handler stays off entirely, per the `El::disabled`
    /// contract; the `begin_cancel` guard is the second layer, so a press
    /// that somehow arrives while canceling is still a no-op.
    fn cancel_button(&self, k: &Kit, base: &str, job: &AttachedJob, disabled: bool) -> El {
        let mut button = Button::new(t("transfer.cancel"))
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Small)
            .disabled(disabled);
        if !disabled {
            if let Some(on_cancel) = self.on_cancel {
                let job_id = job.job_id.clone();
                let begin = self.begin_cancel;
                button = button.on_press(move |s: &mut S| {
                    if begin(s, &job_id) {
                        on_cancel(s, job_id.clone());
                    }
                });
            }
        }
        button.build(k).key(format!("{base}-cancel"))
    }
}
