//! T148: `windows/data-sync/DataSyncJobStatusPanel.tsx` ported to Rune.
//!
//! The status strip the Data Sync window mounts above its wizard: the
//! pending-verification banner, the hydration-error banner, the
//! not-executed banner, and the durable job history list with the
//! verify-recovery and cancel buttons per job.
//!
//! ## React source facts preserved
//!
//! * The four blocks are **independent**: each renders on its own truthiness
//!   (count > 0, error present, jobs non-empty), so any subset can appear.
//! * The history list shows **at most the first 5 jobs** (`slice(0, 5)`),
//!   scrolls at `max-h-24` (96pt), and drops entirely when the vector is
//!   empty — React's `durableJobs.length ? … : null`.
//! * The per-row state label prefers the recovery verdict: `notExecuted`
//!   renders `migrationJob.notExecutedJob`, `pendingVerification` renders
//!   `migrationJob.pendingVerificationHint`, anything else (no recovery, or
//!   the other three verdicts) falls through to the raw wire job state.
//! * The committed count prefers the domain counters: the first
//!   `domainResults.flatMap(counters)` entry with `code === 'committed'`
//!   wins, and only a missing entry falls back to
//!   `Number(job.progress.committed)` — the `??` fallback fires on
//!   `undefined` only, never to rescue a `NaN` the counter itself produced.
//! * The verify-recovery button mounts only when the verdict is
//!   `pendingVerification`; the cancel button mounts only while the job is
//!   `queued` or `running`. Disabled states and label swaps mirror the
//!   React ternaries exactly.
//!
//! ## Keys
//!
//! React gives the history list a `data-testid` and each button a
//! `data-testid` with the job id embedded; the banners, the rows and the
//! three spans per row carry no test id, so they take internal names
//! (the convention of this crate, see T074):
//!
//! | part | key |
//! |---|---|
//! | root (renders nothing itself) | `data-sync-job-status-panel` |
//! | pending-verification banner | `data-sync-pending-hint` |
//! | hydration-error banner | `data-sync-hydration-error` |
//! | not-executed banner | `data-sync-not-executed-hint` |
//! | history list (React test id) | `data-sync-job-history` |
//! | job row | `data-sync-durable-job-<jobId>` |
//! | kind span | `…-kind` |
//! | state span | `…-state` |
//! | committed span | `…-committed` |
//! | verify-recovery button (React test id) | `data-sync-verify-recovery-<jobId>` |
//! | cancel button (React test id) | `data-sync-cancel-job-<jobId>` |
//!
//! ## Documented deviations from React
//!
//! * **`text-amber-400` renders as the kit warning foreground.** The crate
//!   maps amber hints onto `k.c.warning.fg` (same choice as T073's
//!   cancel-requested note); no new color literal is introduced.
//! * **`max-h-24 overflow-auto` becomes a 96pt max height with a vertical
//!   scroll viewport.** React's `overflow-auto` is two-axes, but the list's
//!   content can only grow vertically, so a y-only viewport is the same
//!   rendered behaviour.
//! * **`Number()` is mirrored, not `readJobCounter`.** React's inline
//!   `Number(...)` conversions (the two committed readings) are loose: empty
//!   strings become 0, hexadecimal and `Infinity` spellings parse, anything
//!   else becomes `NaN`, which then prints as `NaN`. That is what
//!   [`js_number`] implements. It is deliberately different from the strict
//!   [`read_job_counter`] gate the migration verdict panel uses — the two
//!   call sites have different React sources.
//! * **Out of model for `Number(string)`:** JS accepts spellings Rust's
//!   `str::parse::<f64>` rejects (`"5."`, `"0b101"`); the real wire counter
//!   is always a plain decimal string, so those branches stay unreachable
//!   and fall to `NaN`. `String(Number)` prints `Infinity` / `NaN`, which
//!   [`js_number_to_string`] reproduces (Rust would print `inf`).
//! * **A disabled button carries no press handler at all**, per the
//!   `El::disabled` contract (same as T073): while the press stays off, the
//!   guard is the disabled flag itself.
//! * **The hydration-error banner treats an empty string as falsy.** React's
//!   `hydrationError ? … : null` hides an empty string along with `null`;
//!   the Rust model filters both the same way (the count banners and the
//!   job list are numeric/length truthiness, which a `usize > 0` /
//!   `!is_empty()` check mirrors exactly).
//! * **The row is a no-wrap `justify_between` row.** React's
//!   `flex items-center justify-between gap-3 py-0.5` does not wrap, and
//!   neither does the Rune row; the banners wrap their text (CSS default)
//!   via `.wrap()`.

// The production consumer is the Data Sync window (T268); until then the
// component tests are the only callers, so the dead-code lint is silenced
// per item rather than crate-wide.
#![allow(dead_code)] // reusable component; wired by the Data Sync window track (T268)

use std::rc::Rc;

use rune::{col, row, text, El, ScrollSpec};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::i18n::t;
use crate::ui::kit::kit;
use crate::views::{JobCounter, JobProgress, JobState};

/// React `JobRecoveryVerdict` (`backend-client/types/jobs.ts:135-140`):
/// the five stable verdicts a recovery receipt can carry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum JobRecoveryVerdict {
    NotExecuted,
    PendingVerification,
    ResumeAfterVerify,
    Reject,
    RequireManualReview,
}

impl JobRecoveryVerdict {
    /// The wire spelling, for the host that decodes the recovery receipt.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::NotExecuted => "notExecuted",
            Self::PendingVerification => "pendingVerification",
            Self::ResumeAfterVerify => "resumeAfterVerify",
            Self::Reject => "reject",
            Self::RequireManualReview => "requireManualReview",
        }
    }

    /// Parses the wire spelling. An unknown verdict is `None`: this panel's
    /// branches treat every non-`notExecuted` / non-`pendingVerification`
    /// verdict (known or not) exactly like React's chained `===` does —
    /// it falls through to the raw job state.
    pub(crate) fn from_str(value: &str) -> Option<Self> {
        match value {
            "notExecuted" => Some(Self::NotExecuted),
            "pendingVerification" => Some(Self::PendingVerification),
            "resumeAfterVerify" => Some(Self::ResumeAfterVerify),
            "reject" => Some(Self::Reject),
            "requireManualReview" => Some(Self::RequireManualReview),
            _ => None,
        }
    }
}

/// One entry of `domainResults.flatMap((result) => result.counters)` —
/// the `(code, value)` pairs this panel flattens the domain results into.
/// The other fields of `JobDomainResult` never reach this strip.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DomainCounter {
    pub code: String,
    pub value: JobCounter,
}

/// The fields of `JobView` this panel reads, plus the recovery verdict and
/// the flattened domain counters — the `Pick<JobDetails, 'job' |
/// 'recoveryTargets' | 'domainResults' | 'recovery'>` projection React
/// types as `DataSyncJobDetails`. `recoveryTargets` is part of the React
/// type but never read by this component (the verify callback receives the
/// whole details object and the window re-queries by itself), so it is not
/// modelled here.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DataSyncJobDetailsVm {
    pub job: DurableJobView,
    /// `recovery?.verdict`, `None` when there is no receipt or the verdict
    /// does not parse.
    pub recovery_verdict: Option<JobRecoveryVerdict>,
    /// `domainResults.flatMap((result) => result.counters)`.
    pub domain_counters: Vec<DomainCounter>,
}

/// The `JobView` fields the strip renders.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DurableJobView {
    pub job_id: String,
    pub kind: String,
    pub state: JobState,
    pub cancel_requested: bool,
    pub progress: JobProgress,
}

/// JS `Number(value)` on the two runtime counter forms: the number branch
/// is the value itself, the string branch trims JS whitespace, maps `''`
/// to 0, accepts hexadecimal and `Infinity` spellings, and is `NaN` for
/// anything else. This is the loose inline conversion of the React source,
/// **not** the strict `readJobCounter` gate of `lib/migrationJobVerdict.ts`.
pub(crate) fn js_number(value: &JobCounter) -> f64 {
    match value {
        JobCounter::Number(value) => *value,
        JobCounter::Text(text) => js_number_of_text(text),
    }
}

/// The string branch of [`js_number`].
fn js_number_of_text(s: &str) -> f64 {
    // JS `String.prototype.trim` (U+FEFF included, same set as
    // `read_job_counter`).
    let trimmed = s.trim_matches(|c: char| c.is_whitespace() || c == '\u{FEFF}');
    if trimmed.is_empty() {
        return 0.0;
    }
    // Hexadecimal literals: `0x…` and `+0x…` parse, `-0x…` does not.
    let hex = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .or_else(|| trimmed.strip_prefix("+0x"))
        .or_else(|| trimmed.strip_prefix("+0X"));
    if let Some(hex) = hex {
        if !hex.is_empty() && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return u64::from_str_radix(hex, 16).map(|v| v as f64).unwrap_or(f64::NAN);
        }
        return f64::NAN;
    }
    match trimmed {
        "Infinity" | "+Infinity" => f64::INFINITY,
        "-Infinity" => f64::NEG_INFINITY,
        _ => trimmed.parse::<f64>().unwrap_or(f64::NAN),
    }
}

/// JS `String(number)`: `NaN` / `Infinity` / `-Infinity` print their JS
/// spellings (Rust's `Display` for `f64` would print `NaN` / `inf` / `-inf`),
/// everything else prints the same compact decimal.
pub(crate) fn js_number_to_string(value: f64) -> String {
    if value.is_nan() {
        "NaN".to_owned()
    } else if value == f64::INFINITY {
        "Infinity".to_owned()
    } else if value == f64::NEG_INFINITY {
        "-Infinity".to_owned()
    } else {
        format!("{value}")
    }
}

/// The per-row state label: the recovery verdict wins over the raw job
/// state for the two verdicts React names, everything else (including no
/// receipt) falls through to the wire state spelling.
pub(crate) fn durable_job_state_label(details: &DataSyncJobDetailsVm) -> String {
    match details.recovery_verdict {
        Some(JobRecoveryVerdict::NotExecuted) => t("migrationJob.notExecutedJob").to_owned(),
        Some(JobRecoveryVerdict::PendingVerification) => {
            t("migrationJob.pendingVerificationHint").to_owned()
        }
        _ => details.job.state.as_str().to_owned(),
    }
}

/// The committed reading of one durable job: the first flattened domain
/// counter with `code === 'committed'` wins; only a missing entry falls back
/// to `Number(job.progress.committed)` — the `??` fires on `undefined`
/// (no counter found) only, never to rescue a `NaN` the counter itself
/// produced. A missing progress bucket is `Number(undefined)` = `NaN`,
/// mirroring the Option this model carries.
pub(crate) fn committed_result(details: &DataSyncJobDetailsVm) -> f64 {
    let fallback = details
        .job
        .progress
        .committed
        .as_ref()
        .map(js_number)
        .unwrap_or(f64::NAN);
    details
        .domain_counters
        .iter()
        .find(|counter| counter.code == "committed")
        .map(|counter| js_number(&counter.value))
        .unwrap_or(fallback)
}

/// The controlled state of the panel. Fields mirror the React props one to
/// one; the two in-flight ids are props (controlled by the window), not
/// local state, exactly as React receives them.
#[derive(Clone, Debug, Default)]
pub(crate) struct DataSyncJobStatusPanelVm {
    pub pending_verification_count: usize,
    pub hydration_error: Option<String>,
    pub not_executed_count: usize,
    pub durable_jobs: Vec<DataSyncJobDetailsVm>,
    pub verifying_recovery_job: Option<String>,
    pub cancelling_durable_job: Option<String>,
}

/// The status strip builder. `S` is the host state that owns the
/// verify-recovery and cancel actions.
pub(crate) struct DataSyncJobStatusPanel<S: 'static> {
    vm: DataSyncJobStatusPanelVm,
    on_verify_recovery: Option<Rc<dyn Fn(&mut S, DataSyncJobDetailsVm)>>,
    on_cancel_job: Option<Rc<dyn Fn(&mut S, String)>>,
}

impl<S: 'static> DataSyncJobStatusPanel<S> {
    pub(crate) fn new(vm: DataSyncJobStatusPanelVm) -> Self {
        Self {
            vm,
            on_verify_recovery: None,
            on_cancel_job: None,
        }
    }

    /// React `onVerifyRecovery(details)`: the whole details projection
    /// travels back to the host (the window re-queries the durable job by
    /// itself).
    pub(crate) fn on_verify_recovery(
        mut self,
        f: impl Fn(&mut S, DataSyncJobDetailsVm) + 'static,
    ) -> Self {
        self.on_verify_recovery = Some(Rc::new(f));
        self
    }

    /// React `onCancelJob(jobId)`.
    pub(crate) fn on_cancel_job(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_cancel_job = Some(Rc::new(f));
        self
    }

    /// The strip: a column root (React returns a fragment; the column
    /// mounts nothing visible itself) with the three banners and the
    /// history list as children.
    pub(crate) fn build(self, dark: bool) -> El {
        let k = kit(dark);
        let vm = &self.vm;
        let mut root = col().key("data-sync-job-status-panel");
        if vm.pending_verification_count > 0 {
            root = root.child(banner(
                &k,
                "data-sync-pending-hint",
                t("migrationJob.pendingVerificationHint"),
            ));
        }
        // React's truthiness: a present-but-empty error string is falsy
        // and renders nothing, exactly like `null`.
        if let Some(error) = vm.hydration_error.as_deref().filter(|e| !e.is_empty()) {
            root = root.child(banner(&k, "data-sync-hydration-error", error));
        }
        if vm.not_executed_count > 0 {
            root = root.child(banner(
                &k,
                "data-sync-not-executed-hint",
                t("migrationJob.notExecutedHint"),
            ));
        }
        if !vm.durable_jobs.is_empty() {
            root = root.child(self.job_history(&k));
        }
        root
    }

    /// The history list: `max-h-24 overflow-auto border-b border-edge px-6
    /// py-2 text-xs text-fg-muted`, at most the first five jobs.
    fn job_history(&self, k: &Kit) -> El {
        let mut history = col()
            .key("data-sync-job-history")
            .w_full()
            .max_h(96.0)
            .scroll(ScrollSpec { stick_bottom: false })
            .px(24.0)
            .py(8.0)
            .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base);
        for details in self.vm.durable_jobs.iter().take(5) {
            history = history.child(self.job_row(k, details));
        }
        history
    }

    /// One history row: `flex items-center justify-between gap-3 py-0.5`
    /// with the kind, the state label, the committed count, and at most
    /// the verify-recovery and cancel buttons.
    fn job_row(&self, k: &Kit, details: &DataSyncJobDetailsVm) -> El {
        let base = format!("data-sync-durable-job-{}", details.job.job_id);
        let mut row_el = row()
            .key(base.clone())
            .w_full()
            .items_center()
            .justify_between()
            .gap(12.0)
            .py(2.0)
            .child(
                text(details.job.kind.clone())
                    .key(format!("{base}-kind"))
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            )
            .child(
                text(durable_job_state_label(details))
                    .key(format!("{base}-state"))
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            )
            .child(
                text(format!(
                    "{}: {}",
                    t("migration.verdict.committedRows"),
                    js_number_to_string(committed_result(details))
                ))
                .key(format!("{base}-committed"))
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted),
            );
        if let Some(button) = self.verify_recovery_button(k, details) {
            row_el = row_el.child(button);
        }
        if let Some(button) = self.cancel_button(k, details) {
            row_el = row_el.child(button);
        }
        row_el
    }

    /// The verify-recovery button: only a `pendingVerification` verdict
    /// mounts it; disabled while this job's verify is in flight, with the
    /// label swapping to `migrationJob.verifyingRecovery`.
    fn verify_recovery_button(&self, k: &Kit, details: &DataSyncJobDetailsVm) -> Option<El> {
        if details.recovery_verdict != Some(JobRecoveryVerdict::PendingVerification) {
            return None;
        }
        let verifying =
            self.vm.verifying_recovery_job.as_deref() == Some(details.job.job_id.as_str());
        let label = if verifying {
            t("migrationJob.verifyingRecovery")
        } else {
            t("migrationJob.verifyRecovery")
        };
        let mut button = Button::new(label)
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Small)
            .disabled(verifying);
        if !verifying {
            if let Some(on_verify) = &self.on_verify_recovery {
                let payload = details.clone();
                let on_verify = on_verify.clone();
                button = button.on_press(move |s: &mut S| on_verify(s, payload.clone()));
            }
        }
        Some(
            button
                .build(k)
                .key(format!("data-sync-verify-recovery-{}", details.job.job_id)),
        )
    }

    /// The cancel button: only a `queued` or `running` job mounts it;
    /// disabled while a cancel was requested (backend flag or in-flight
    /// local cancel), with the label swapping to
    /// `migrationJob.cancellingJob`.
    fn cancel_button(&self, k: &Kit, details: &DataSyncJobDetailsVm) -> Option<El> {
        if !matches!(
            details.job.state,
            JobState::Queued | JobState::Running
        ) {
            return None;
        }
        let cancelling = details.job.cancel_requested
            || self.vm.cancelling_durable_job.as_deref() == Some(details.job.job_id.as_str());
        let label = if cancelling {
            t("migrationJob.cancellingJob")
        } else {
            t("migrationJob.cancelJob")
        };
        let mut button = Button::new(label)
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Small)
            .disabled(cancelling);
        if !cancelling {
            if let Some(on_cancel) = &self.on_cancel_job {
                let job_id = details.job.job_id.clone();
                let on_cancel = on_cancel.clone();
                button = button.on_press(move |s: &mut S| on_cancel(s, job_id.clone()));
            }
        }
        Some(
            button
                .build(k)
                .key(format!("data-sync-cancel-job-{}", details.job.job_id)),
        )
    }
}

/// One banner: `border-b border-edge px-6 py-2 text-xs text-amber-400`,
/// wrapping like a CSS block.
fn banner(k: &Kit, key: &str, message: &str) -> El {
    col()
        .key(key)
        .w_full()
        .px(24.0)
        .py(8.0)
        .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
        .child(
            text(message.to_owned())
                .key(format!("{key}-text"))
                .font(k.ty.text_xsmall)
                .color(k.c.warning.fg)
                .wrap(),
        )
}
