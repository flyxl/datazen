//! T246: `windows/data-transfer/TransferJobResultPanel.tsx` ported to
//! Rune (118 lines).
//!
//! The result surface for Data Transfer: the rendering is shared, only
//! the plumbing is transfer-specific. Everything displayed comes from
//! [`MigrationJobVerdictPanel`] and the verdict engine, so Schema Diff
//! and Data Sync reuse this component by supplying their own run
//! object with the same field names — no verdict logic is re-implemented
//! here.
//!
//! ## The three blocks (React 51–114)
//!
//! | React source | Rune item |
//! |---|---|
//! | the in-flight card (62–81) | [`TransferJobResultPanel::in_flight_card`] |
//! | the failure notice mount (84–90) | [`TransferJobResultPanel::failure_notice`] |
//! | the verdict panel mount (97–113) | [`TransferJobResultPanel::verdict_panel`] |
//! | the `activeProgress` derivation (39–49) | [`active_progress`] |
//! | the plan identity (34–38) | [`plan_info`] |
//! | the six root `data-*` attributes (54–59) | the hidden attrs mirror |
//!
//! ## Key convention
//!
//! React `data-testid` becomes the element key verbatim
//! (`data-transfer-result`, `data-transfer-job-active-state`,
//! `data-transfer-job-active-progress`,
//! `data-transfer-job-active-cancel`); the six root `data-*`
//! attributes (rune's `El` cannot set them natively) ride a hidden
//! keyed node as a `|`-separated label, the same mirror the terminal
//! results surface (T113) established under this very React testid.
//! Internal keys (no React testid) are documented here for the window
//! wiring task: `data-transfer-result-attrs`,
//! `data-transfer-result-active-card`.
//!
//! ## Deliberate boundaries
//!
//! * **`useLocaleDomains(['sync'])`** has no Rune counterpart: the
//!   generated zh-CN table already covers every pack (T113 documented
//!   the same call away; the parity tests assert the gated keys
//!   exist).
//! * **The two `TransferRunFailure` twins.** The refusal notice's
//!   port (T106) carries its own `TransferRunFailure` (the React
//!   `components/migration` import), and the run's port (T244)
//!   carries the `useTransferJobRun` one — two mirrors of the same
//!   TypeScript type, kept by their owning tasks. This panel bridges
//!   them with a plain conversion ([`notice_failure`]) rather than
//!   unifying them (that is the un-port task's business, not a window
//!   panel's).
//! * **Two progress dialects.** The run's buckets arrive as the
//!   attached-jobs `JobCounter` (a JSON number or a decimal string —
//!   `readJobCounter` reads both), while the verdict panel's port
//!   holds its buckets as decimal strings. [`counter_text`] bridges
//!   the dialects for the verdict panel mount, so both surfaces read
//!   the counters through their own `readJobCounter` mirrors, exactly
//!   as React passes the same object through both.
//! * **`onReReview` is the refusal notice's `onReReview`**: the plan
//!   was spent, so the only legal way forward is a fresh review — the
//!   notice mounts the button only for `planConsumed` / `stalePlan`
//!   and only when the callback is present.

#![allow(dead_code)] // the Data Transfer window wiring task is the consumer

use std::rc::Rc;

use rune::{col, row, text, El};
use rune_kit::Kit;

use crate::components::migration::migration_job_failure_notice::{
    MigrationJobFailureNotice, TransferRunFailure as NoticeFailure,
    TransferRunFailureKind as NoticeFailureKind,
};
use crate::components::migration::migration_job_verdict_panel::{
    JobProgress as VerdictJobProgress, MigrationJobVerdictPanel, PlanInfo, VerdictPanelVm,
};
use crate::i18n::t;
use crate::views::data_transfer::data_transfer_attached_jobs::{
    read_job_counter, JobCounter, JobState,
};
use crate::views::data_transfer::transfer_job_run::{
    TransferApplyJobViewVm, TransferJobRunVm, TransferRunFailure, TransferRunFailureKind,
};

/// React `activeProgress` (39–49): the five progress buckets read as
/// counters (an unreadable bucket is *zero*, the in-flight line's own
/// rule — the verdict engine is stricter) joined ` / `, or `None` when
/// there is no apply view yet.
pub(crate) fn active_progress(apply: &TransferApplyJobViewVm) -> Option<String> {
    let progress = &apply.progress;
    let bucket = |value: &Option<JobCounter>| read_job_counter(value.as_ref()).unwrap_or(0);
    Some(format!(
        "{} / {} / {} / {} / {}",
        bucket(&progress.read),
        bucket(&progress.converted),
        bucket(&progress.attempted),
        bucket(&progress.committed),
        bucket(&progress.unknown),
    ))
}

/// One bucket as the verdict panel's decimal-string dialect (see the
/// module docs): a JSON number truncates as `readJobCounter` itself
/// does (Math.trunc), a decimal string passes through byte for byte,
/// an absent bucket stays absent.
fn counter_text(value: &Option<JobCounter>) -> Option<String> {
    match value {
        None => None,
        Some(JobCounter::Number(value)) => Some(format!("{}", value.trunc() as i64)),
        Some(JobCounter::Text(value)) => Some(value.clone()),
    }
}

/// The run's progress as the verdict panel's dialect, bucket for
/// bucket.
fn verdict_progress(apply: &TransferApplyJobViewVm) -> VerdictJobProgress {
    let progress = &apply.progress;
    VerdictJobProgress {
        read: counter_text(&progress.read),
        converted: counter_text(&progress.converted),
        attempted: counter_text(&progress.attempted),
        committed: counter_text(&progress.committed),
        unknown: counter_text(&progress.unknown),
    }
}

/// React `plan` (34–38): the plan identity travels with the verdict so
/// the panel can name the exact plan that was spent. Same three fields
/// for all three tools.
pub(crate) fn plan_info(apply: Option<&TransferApplyJobViewVm>) -> PlanInfo {
    match apply {
        Some(apply) => PlanInfo {
            plan_id: Some(apply.plan_id.clone()),
            plan_digest: Some(apply.plan_digest.clone()),
            selection_revision: Some(apply.selection_revision.max(0) as u64),
        },
        None => PlanInfo::default(),
    }
}

/// The run's failure → the notice's failure: the two `TransferRunFailure`
/// twins (see the module docs) are the same five kinds, so the bridge is
/// a plain kind-by-kind conversion.
fn notice_failure(failure: &TransferRunFailure) -> NoticeFailure {
    let kind = match failure.kind {
        TransferRunFailureKind::BackendScope => NoticeFailureKind::BackendScope,
        TransferRunFailureKind::PipelineBudget => NoticeFailureKind::PipelineBudget,
        TransferRunFailureKind::PlanConsumed => NoticeFailureKind::PlanConsumed,
        TransferRunFailureKind::StalePlan => NoticeFailureKind::StalePlan,
        TransferRunFailureKind::Other => NoticeFailureKind::Other,
    };
    NoticeFailure { kind, message: failure.message.clone() }
}

/// The state line's job label (`role="status"`'s a11y name: the job's
/// id and state, React's own `data-job-id` / `data-state` pair).
fn state_label(apply: &TransferApplyJobViewVm) -> String {
    format!("job_id={}|state={}", apply.job_id, apply.state.as_str())
}

/// The result surface. Build it with the run, attach the re-review
/// action, and the three blocks mount themselves (the in-flight card
/// while the apply is active, the refusal notice when the run failed
/// before an apply verdict, the verdict panel once a verdict exists and
/// the run has settled).
pub(crate) struct TransferJobResultPanel<'a, S: 'static> {
    run: &'a TransferJobRunVm,
    on_re_review: Option<Rc<dyn Fn(&mut S)>>,
    test_id_prefix: String,
}

impl<'a, S: 'static> TransferJobResultPanel<'a, S> {
    pub(crate) fn new(run: &'a TransferJobRunVm) -> Self {
        Self {
            run,
            on_re_review: None,
            test_id_prefix: "migration-job".to_owned(),
        }
    }

    /// React `onReReview`: the plan was spent, so the only legal way
    /// forward is a fresh review. Mounts the refusal notice's review
    /// button.
    pub(crate) fn on_re_review(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_re_review = Some(Rc::new(f));
        self
    }

    /// React `testIdPrefix` (default `migration-job`): the refusal
    /// notice and the verdict panel share the prefix, so a spec can
    /// query one namespace for both the refusal and the settled
    /// verdict.
    pub(crate) fn test_id_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.test_id_prefix = prefix.into();
        self
    }

    /// The section. React's `flex flex-col gap-4` root with the hidden
    /// `data-*` mirror and whichever of the three blocks hold.
    pub(crate) fn build(self, dark: bool) -> El {
        let k: &'static Kit = crate::ui::kit::kit(dark);
        let run = self.run;
        let verdict = run.verdict();
        let apply = run.apply_view.as_ref();

        let mut section = col()
            .key("data-transfer-result")
            .w_full()
            .gap(16.0)
            .child(self.attrs_mirror(verdict.as_ref(), apply));

        // The in-flight card.
        if run.is_in_flight() && apply.is_some() {
            section = section.child(self.in_flight_card(k, apply.unwrap()));
        }

        // The refusal notice.
        if let Some(failure) = &run.failure {
            section = section.child(self.failure_notice(dark, failure));
        }

        // The verdict panel — no verdict means the backend never
        // admitted a Job to report on; drawing the panel anyway would
        // imply a run happened, so the refusal is shown on its own.
        if let (Some(verdict), false) = (&verdict, run.is_in_flight()) {
            section = section.child(self.verdict_panel(dark, verdict, apply));
        }
        section
    }

    /// The six root `data-*` attributes (54–59): the hidden mirror the
    /// terminal results surface (T113) established — severity turns
    /// `active` while in flight, the verdict's own spelling otherwise
    /// (`failed` with no verdict at all).
    fn attrs_mirror(
        &self,
        verdict: Option<&crate::components::migration::migration_job_verdict_panel::MigrationJobVerdict>,
        apply: Option<&TransferApplyJobViewVm>,
    ) -> El {
        let run = self.run;
        let severity = if run.is_in_flight() {
            "active".to_owned()
        } else {
            verdict
                .map(|verdict| verdict.severity.as_str().to_owned())
                .unwrap_or_else(|| "failed".to_owned())
        };
        let completed = verdict.map(|verdict| verdict.completed).unwrap_or(false);
        let replayed = apply.map(|apply| apply.replayed).unwrap_or(false);
        let cancel_disposition = verdict
            .map(|verdict| verdict.cancel_disposition.as_str().to_owned())
            .unwrap_or_else(|| "none".to_owned());
        let requires_reconcile = verdict.map(|verdict| verdict.requires_reconcile).unwrap_or(false);
        let plan = plan_info(apply);
        let label = format!(
            "verdict_severity={severity}|completed={completed}|replayed={replayed}|cancel_disposition={cancel_disposition}|requires_reconcile={requires_reconcile}|plan_id={}",
            plan.plan_id.as_deref().unwrap_or("undefined")
        );
        row()
            .key("data-transfer-result-attrs")
            .size(0.0, 0.0)
            .absolute(0.0, 0.0)
            .a11y_hidden()
            .label(label)
    }

    /// The in-flight card (62–81): `rounded border border-accent/30
    /// bg-accent/5 px-3 py-2 text-sm text-fg` — the `role="status"`
    /// state line (the job's id and state ride its label), the five
    /// progress counters, and the cancel-requested notice.
    fn in_flight_card(&self, k: &'static Kit, apply: &TransferApplyJobViewVm) -> El {
        let state_line = text(t(&format!("transfer.job.{}", apply.state.as_str())))
            .key("data-transfer-job-active-state")
            .role("status")
            .label(state_label(apply))
            .font(k.ty.body)
            .color(k.c.text_base);

        let counts = t("migration.progress.counts");
        let progress = active_progress(apply).unwrap_or_default();
        let progress_line = text(format!("{counts}: {progress}"))
            .key("data-transfer-job-active-progress")
            .font(k.ty.text_xsmall)
            .color(k.c.text_muted);

        let mut card = col()
            .key("data-transfer-result-active-card")
            .w_full()
            .radius(4.0)
            .border(1.0, k.alpha(k.c.bg_accent, 0.3))
            .fill(k.alpha(k.c.bg_accent, 0.05))
            .px(12.0)
            .py(8.0)
            .gap(4.0)
            .child(state_line)
            .child(progress_line);
        if self.run.cancel_requested {
            card = card.child(
                text(t("migration.cancel.requestedInFlight"))
                    .key("data-transfer-job-active-cancel")
                    .font(k.ty.text_xsmall)
                    .color(k.c.warning.fg),
            );
        }
        card
    }

    /// The refusal notice (84–90): the shared fail-closed notice with
    /// the panel's own re-review action.
    fn failure_notice(&self, dark: bool, failure: &TransferRunFailure) -> El {
        let mut notice = MigrationJobFailureNotice::<S>::new(notice_failure(failure))
            .test_id_prefix(self.test_id_prefix.clone());
        if let Some(on_re_review) = &self.on_re_review {
            let on_re_review = Rc::clone(on_re_review);
            notice = notice.on_review(move |s: &mut S| on_re_review(s));
        }
        notice.build(dark)
    }

    /// The verdict panel (97–113): the shared panel with the run's
    /// props — the verdict, the apply view's progress / boundaries /
    /// artifacts, the cancel pair, the replay and recovery triple,
    /// the error, and the plan identity.
    fn verdict_panel(
        &self,
        dark: bool,
        verdict: &crate::components::migration::migration_job_verdict_panel::MigrationJobVerdict,
        apply: Option<&TransferApplyJobViewVm>,
    ) -> El {
        let run = self.run;
        let vm = VerdictPanelVm {
            verdict: verdict.clone(),
            progress: apply.map(verdict_progress),
            commit_boundaries: apply.map(|apply| apply.commit_boundaries.clone()).unwrap_or_default(),
            artifact_ids: apply.map(|apply| apply.artifact_ids.clone()).unwrap_or_default(),
            cancel_requested: run.cancel_requested,
            cancel_acknowledged: run.cancel_acknowledged,
            replayed: apply.map(|apply| apply.replayed).unwrap_or(false),
            recovery_verdict: apply.and_then(|apply| apply.recovery_verdict.clone()),
            recovery_reason: apply.and_then(|apply| apply.recovery_reason.clone()),
            recovery_resume_through: apply
                .and_then(|apply| apply.recovery_resume_through)
                .map(|value| value.max(0) as u64),
            error: apply.and_then(|apply| apply.error.clone()),
            plan: Some(plan_info(apply)),
            test_id_prefix: self.test_id_prefix.clone(),
        };
        MigrationJobVerdictPanel::<S>::new(vm).build(dark)
    }
}

/// Keeps the `JobState` import honest: the in-flight state line's key
/// is the run view's own state spelling (`transfer.job.<state>`), the
/// same enum the run's verdict input reads.
#[allow(dead_code)]
fn state_key_prefix(state: JobState) -> String {
    format!("transfer.job.{}", state.as_str())
}
