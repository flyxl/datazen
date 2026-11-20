//! Port of `packages/frontend/src/windows/schema-diff/SchemaDiffJobStatusPanel.tsx`
//! (T115).
//!
//! The wizard's live job status strip: the current job's kind/state line
//! with its stage, progress counters and the request-cancel button while
//! active; the cancel-requested / unavailable and plan-unavailable notes;
//! the restart-recovery notices (not dispatched, not executed, pending
//! verification with the verify button, manual review); and the last
//! apply's report line with its recovery code.
//!
//! ## What maps where
//!
//! - `currentJob` is [`SchemaDiffTrackedJobVm`] (T391's hook), `latestApply`
//!   / the recovery fields ride the T377 wire's details projection
//!   (including `pendingVerificationReason`, the recovery verdict and the
//!   recovery targets this panel reads).
//! - React's `STATE_KEYS` record + `?? 'schemaDiff.jobUnknown'` fallback is
//!   [`state_key`]; `readJobCounter(bucket) ?? 0` is the shared
//!   `data_transfer_attached_jobs` port's [`job_counter_or_zero`].
//! - The recovery-details fallback chain (`view?.pendingVerificationReason
//!   != null ? details : latestApply… ? latestApply : details`) is
//!   [`recovery_details`] verbatim; the four notices below it map 1:1.
//! - `text-amber-400` → the kit's warning ink; `text-fg-secondary` /
//!   `text-fg-muted` → `text_muted` (the workspace-navigator convention);
//!   the cancel / verify buttons are kit buttons (default variant, Small).
//! - `aria-live="polite"` → the root's live region.
//!
//! ## Keys
//!
//! React `data-testid` stays verbatim on the root
//! (`schema-diff-job-status`). Derived (no React counterpart, for
//! observation): `-kind-state` (the kind/state line), `-stage`,
//! `-progress`, `-cancel` (the button), `-cancel-requested`,
//! `-cancel-unavailable`, `-plan-unavailable`, `-not-dispatched`,
//! `-not-executed`, `-recovery-pending`, `-verify`, `-recovery-identity`,
//! `-manual-review`, `-last-report`, `-last-counts`, `-recovery-code`.
//!
//! Deltas: pixel classes → points (×0.75); `mt-2` notes are the column's
//! 6pt gap; the section's bottom rule is `border_sides`.

use std::rc::Rc;

use rune::{col, row, text, El, LiveRegion, TextStyle};

use crate::commands::schema_diff::wire::{JobRecoveryVerdictVm, SchemaDiffJobDetailsVm};
use crate::i18n::t;
use crate::ui::kit::kit;
use crate::views::data_transfer::data_transfer_attached_jobs::job_counter_or_zero;
use crate::views::schema_diff::use_schema_diff_job_lifecycle::{
    SchemaDiffCancelOutcome, SchemaDiffTrackedJobVm,
};
use rune_kit::{Button, ButtonVariant, ControlSize};

type CancelFn<S> = Rc<dyn Fn(&mut S) + 'static>;
type VerifyFn<S> = Rc<dyn Fn(&mut S, String) + 'static>;

/// React `STATE_KEYS` + the unknown fallback.
fn state_key(state: &crate::views::data_transfer::data_transfer_attached_jobs::JobState) -> &'static str {
    use crate::views::data_transfer::data_transfer_attached_jobs::JobState;
    match state {
        JobState::Queued => "schemaDiff.jobQueued",
        JobState::Running => "schemaDiff.jobRunning",
        JobState::Succeeded => "schemaDiff.jobSucceeded",
        JobState::Failed => "schemaDiff.jobFailed",
        JobState::Cancelled => "schemaDiff.jobCancelled",
    }
}

/// The recovery-details fallback chain, verbatim: the current view's
/// pending-verification reason wins, else a latest-apply that is itself
/// pending verification or carries a not-executed / manual-review verdict,
/// else back to the current details.
fn recovery_details<'a>(
    tracked: Option<&'a SchemaDiffTrackedJobVm>,
    latest_apply: Option<&'a SchemaDiffJobDetailsVm>,
) -> Option<&'a SchemaDiffJobDetailsVm> {
    let details = tracked.and_then(|job| job.details.as_ref());
    let view = details.map(|details| &details.details);
    let latest_apply_job = latest_apply.map(|details| &details.details);
    let latest_apply_recovery = latest_apply.and_then(|details| details.recovery.as_ref());
    if view
        .map(|view| view.pending_verification_reason.is_some())
        .unwrap_or(false)
    {
        return details;
    }
    let latest_pending = latest_apply_job
        .map(|job| job.pending_verification_reason.is_some())
        .unwrap_or(false);
    let latest_not_executed =
        latest_apply_recovery.map(|r| r.verdict == JobRecoveryVerdictVm::NotExecuted).unwrap_or(false);
    let latest_manual = latest_apply_recovery
        .map(|r| r.verdict == JobRecoveryVerdictVm::RequireManualReview)
        .unwrap_or(false);
    if latest_pending || latest_not_executed || latest_manual {
        return latest_apply;
    }
    details
}

/// The status strip builder, generic over the owning state `S`.
pub(crate) struct SchemaDiffJobStatusPanel<'a, S: 'static> {
    current_job: Option<&'a SchemaDiffTrackedJobVm>,
    latest_apply: Option<&'a SchemaDiffJobDetailsVm>,
    cancel_outcome: SchemaDiffCancelOutcome,
    verifying: bool,
    target_ready: bool,
    on_cancel: Option<CancelFn<S>>,
    on_verify: Option<VerifyFn<S>>,
}

impl<'a, S: 'static> SchemaDiffJobStatusPanel<'a, S> {
    /// React's `currentJob` / `latestApply` / `cancelOutcome` /
    /// `verifying` / `targetReady` props.
    pub(crate) fn new(
        current_job: Option<&'a SchemaDiffTrackedJobVm>,
        latest_apply: Option<&'a SchemaDiffJobDetailsVm>,
        cancel_outcome: SchemaDiffCancelOutcome,
        verifying: bool,
        target_ready: bool,
    ) -> Self {
        Self {
            current_job,
            latest_apply,
            cancel_outcome,
            verifying,
            target_ready,
            on_cancel: None,
            on_verify: None,
        }
    }

    /// React's `onCancel`.
    pub(crate) fn on_cancel(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_cancel = Some(Rc::new(f));
        self
    }

    /// React's `onVerify(jobId)`.
    pub(crate) fn on_verify(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_verify = Some(Rc::new(f));
        self
    }

    /// Paint the strip (`None` when nothing is tracked, like React's
    /// `null`).
    pub(crate) fn build(self, dark: bool) -> Option<El> {
        if self.current_job.is_none() && self.latest_apply.is_none() {
            return None;
        }
        let k = kit(dark);
        let mut section = col()
            .key("schema-diff-job-status")
            .w_full()
            .gap(6.0) // the notes' mt-2 spacing
            .pl(18.0)
            .pr(18.0)
            .pt(9.0)
            .pb(9.0) // React px-6 py-3
            .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
            .fill(k.c.layer_01) // bg-surface-alt
            .font(type_xs())
            .live_region(LiveRegion::Polite); // aria-live="polite"

        // --- The current job block. ---
        let tracked = self.current_job;
        let details = tracked.and_then(|job| job.details.as_ref());
        let view = details.map(|details| &details.details);
        let state = view.map(|view| view.state).or(tracked.map(|job| job.state));
        let kind = view
            .map(|view| view.kind.as_str())
            .or(tracked.map(|job| job.kind.as_str()));
        if let (Some(state), Some(kind)) = (state, kind) {
            let active = tracked.is_some_and(|job| job.is_active());
            let mut line = row()
                .key("schema-diff-job-status-line")
                .w_full()
                .flex_wrap()
                .center_y()
                .gap(9.0); // React gap-x-3 gap-y-1 (the x axis dominates)
            line = line.child(
                text(format!(
                    "{}: {}",
                    if kind == "schemaDiffApply" {
                        t("schemaDiff.applyJob")
                    } else {
                        t("schemaDiff.prepareJob")
                    },
                    t(state_key(&state)),
                ))
                .key("schema-diff-job-status-kind-state")
                .font(type_xs_medium())
                .color(k.c.text_base),
            );
            if let Some(stage) = view.and_then(|view| view.stage.as_deref()).filter(|s| !s.is_empty())
            {
                line = line.child(
                    text(stage.to_owned())
                        .key("schema-diff-job-status-stage")
                        .color(k.c.text_muted),
                );
            }
            if active {
                if let Some(progress) = view.map(|view| &view.progress) {
                    line = line.child(
                        text(
                            t("schemaDiff.jobProgress")
                                .replace(
                                    "{attempted}",
                                    &job_counter_or_zero(progress.attempted.as_ref()).to_string(),
                                )
                                .replace(
                                    "{committed}",
                                    &job_counter_or_zero(progress.committed.as_ref()).to_string(),
                                )
                                .replace(
                                    "{unknown}",
                                    &job_counter_or_zero(progress.unknown.as_ref()).to_string(),
                                ),
                        )
                        .key("schema-diff-job-status-progress")
                        .color(k.c.text_muted),
                    );
                }
                let mut cancel = Button::new(t("schemaDiff.requestCancel").to_owned())
                    .variant(ButtonVariant::Neutral) // React variant="secondary"
                    .control_size(ControlSize::Small);
                if let Some(on_cancel) = &self.on_cancel {
                    let on_cancel = Rc::clone(on_cancel);
                    cancel = cancel.on_press(move |s: &mut S| on_cancel(s));
                }
                line = line.child(cancel.build(&k).key("schema-diff-job-status-cancel"));
            }
            section = section.child(line);

            let cancel_requested = view.map(|view| view.cancel_requested).unwrap_or(false);
            if (cancel_requested || self.cancel_outcome == SchemaDiffCancelOutcome::Requested)
                && active
            {
                section = section.child(amber_note(
                    "schema-diff-job-status-cancel-requested",
                    "schemaDiff.cancelRequested",
                    dark,
                ));
            }
            if self.cancel_outcome == SchemaDiffCancelOutcome::Unavailable {
                section = section.child(amber_note(
                    "schema-diff-job-status-cancel-unavailable",
                    "schemaDiff.cancelUnavailable",
                    dark,
                ));
            }
            if details.is_some_and(|details| details.plan_unavailable_after_restart) {
                section = section.child(amber_note(
                    "schema-diff-job-status-plan-unavailable",
                    "schemaDiff.planUnavailableAfterRestart",
                    dark,
                ));
            }
        }

        // --- The restart-recovery notices. ---
        let recovery_details = recovery_details(tracked, self.latest_apply);
        let recovery_view = recovery_details.map(|details| &details.details);
        let recovery = recovery_details.and_then(|details| details.recovery.as_ref());
        let not_executed = recovery
            .map(|r| r.verdict == JobRecoveryVerdictVm::NotExecuted)
            .unwrap_or(false);
        let not_dispatched = not_executed
            && recovery
                .and_then(|r| r.reason_code.as_deref())
                .map(|code| code == "notDispatchedAfterRestart")
                .unwrap_or(false);
        if not_dispatched {
            section = section.child(amber_note(
                "schema-diff-job-status-not-dispatched",
                "schemaDiff.notDispatchedAfterRestart",
                dark,
            ));
        } else if not_executed {
            section = section.child(amber_note(
                "schema-diff-job-status-not-executed",
                "schemaDiff.notExecuted",
                dark,
            ));
        }

        let pending_verification = recovery_view
            .map(|view| view.pending_verification_reason.is_some())
            .unwrap_or(false);
        if !not_executed && pending_verification {
            let has_recovery_targets =
                recovery_details.map(|details| !details.recovery_targets.is_empty()).unwrap_or(false);
            let mut pending = row()
                .key("schema-diff-job-status-recovery-pending")
                .w_full()
                .flex_wrap()
                .center_y()
                .gap(6.0)
                .child(
                    text(t("schemaDiff.recoveryPending").to_owned())
                        .key("schema-diff-job-status-recovery-pending-text")
                        .color(k.c.warning.fg),
                );
            if has_recovery_targets && recovery_view.is_some() {
                let job_id = recovery_view.map(|view| view.job_id.clone()).unwrap_or_default();
                let disabled = !self.target_ready || self.verifying;
                let mut verify = Button::new(
                    if self.verifying {
                        t("schemaDiff.verifyingRecovery").to_owned()
                    } else {
                        t("schemaDiff.verifyRecovery").to_owned()
                    },
                )
                .variant(ButtonVariant::Neutral)
                .control_size(ControlSize::Small)
                .disabled(disabled);
                if !disabled {
                    if let Some(on_verify) = &self.on_verify {
                        let on_verify = Rc::clone(on_verify);
                        let job_id = job_id.clone();
                        verify = verify.on_press(move |s: &mut S| on_verify(s, job_id.clone()));
                    }
                }
                pending = pending.child(verify.build(&k).key("schema-diff-job-status-verify"));
            } else {
                pending = pending.child(
                    text(t("schemaDiff.recoveryIdentityUnavailable").to_owned())
                        .key("schema-diff-job-status-recovery-identity")
                        .color(k.c.warning.fg),
                );
            }
            section = section.child(pending);
        }

        if recovery.is_some_and(|r| r.verdict == JobRecoveryVerdictVm::RequireManualReview) {
            let reason = recovery
                .and_then(|r| r.reason_code.as_deref())
                .unwrap_or_default();
            let key = match reason {
                "targetStateUnchangedReprepareRequired" => "schemaDiff.recoveryUnchanged",
                "targetStateChangedManualReview" => "schemaDiff.recoveryChanged",
                _ => "schemaDiff.recoveryManualReview",
            };
            section = section.child(amber_note("schema-diff-job-status-manual-review", key, dark));
        }

        // --- The last apply's report. ---
        if let Some(latest) = self.latest_apply {
            if let Some(last_result) = &latest.deploy_result {
                let mut report = row()
                    .key("schema-diff-job-status-last-report-row")
                    .w_full()
                    .flex_wrap()
                    .center_y()
                    .gap(9.0)
                    .pt(6.0) // React pt-2
                    .border_sides([1.0, 0.0, 0.0, 0.0], k.c.border_base);
                report = report
                    .child(
                        text(format!(
                            "{}: {}",
                            t("schemaDiff.lastApplyReport"),
                            last_result.status.wire_str()
                        ))
                        .key("schema-diff-job-status-last-report")
                        .font(type_xs_medium())
                        .color(k.c.text_base),
                    )
                    .child(
                        text(format!(
                            "{}/{} {}",
                            last_result.executed_count,
                            last_result.statement_count,
                            t("schemaDiff.executed")
                        ))
                        .key("schema-diff-job-status-last-counts")
                        .color(k.c.text_muted),
                    );
                if let Some(reason_code) =
                    latest.recovery.as_ref().and_then(|r| r.reason_code.as_deref())
                {
                    report = report.child(
                        text(
                            t("schemaDiff.recoveryCode")
                                .replace("{code}", reason_code),
                        )
                        .key("schema-diff-job-status-recovery-code")
                        .color(k.c.warning.fg),
                    );
                }
                section = section.child(report);
            }
        }
        Some(section)
    }
}

/// One `mt-2 text-amber-400` note line.
fn amber_note(key: &str, i18n_key: &str, dark: bool) -> El {
    let k = kit(dark);
    text(t(i18n_key).to_owned())
        .key(key.to_owned())
        .color(k.c.warning.fg)
}

/// React `text-xs` (12px → 9pt).
fn type_xs() -> TextStyle {
    TextStyle::new(9.0, 400.0)
}

/// React `text-xs font-medium` (9pt, weight 500).
fn type_xs_medium() -> TextStyle {
    TextStyle::new(9.0, 500.0)
}
