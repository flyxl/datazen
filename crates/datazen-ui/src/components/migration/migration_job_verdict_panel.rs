//! Shared verdict surface for the P5 migration tools, ported 1:1 from
//! `packages/frontend/src/components/migration/MigrationJobVerdictPanel.tsx`.
//!
//! This is a pure, domain-free presentation component: Data Transfer, Schema
//! Diff and Data Sync all render "how did this run actually end" from the same
//! facts. It never invents a conclusion the backend did not reach. The three
//! load-bearing rules from the React source are preserved verbatim:
//!
//! - "cancel requested" is shown as an *intent* sentence next to where the run
//!   stopped, never as a completed rollback;
//! - a boundary with no evidence is shown as *unverified*, never as committed;
//! - an uncertain verdict is phrased as uncertain — there is no severity branch
//!   that beautifies "we don't know" into success.
//!
//! # Consumer / removal stage
//!
//! No production consumer yet. The Schema Diff / Data Sync "migration job view"
//! track will wire it (passing its `MigrationJobVerdict` snapshot plus raw
//! counters). Until then every reusable item carries a per-item
//! `#[allow(dead_code)]` with this removal stage noted; drop those attributes
//! when that view calls [`MigrationJobVerdictPanel::new`].
//!
//! # Key convention
//!
//! React `data-testid` becomes the element key (the convention used across this
//! crate). With the default prefix `migration-job`:
//!
//! | React testid / part        | key                                             |
//! |----------------------------|-------------------------------------------------|
//! | root (`data-*`)            | `migration-job-verdict`                         |
//! | status (`role=status`)     | `migration-job-verdict-status`                  |
//! | cancel sentence            | `migration-job-cancel`                          |
//! | unknown-job sentence       | `migration-job-cancel-unknown-job`              |
//! | progress counts            | `migration-job-progress`                        |
//! | committed rows             | `migration-job-rows`                            |
//! | replayed receipt           | `migration-job-replayed`                        |
//! | boundary list              | `migration-job-boundaries`                      |
//! | one boundary row           | `migration-job-boundary-<stageId>`              |
//! | empty-boundaries sentence  | `migration-job-boundaries-empty`                |
//! | artifact list / rows       | `migration-job-artifacts` / `-artifact`         |
//! | recovery verdict/reason    | `-recovery-verdict` / `-recovery-reason`        |
//! | resume-through             | `migration-job-recovery-resume-through`          |
//! | uncertainty badge          | `migration-job-uncertainty`                     |
//! | reconcile sentence         | `migration-job-reconcile`                       |
//! | error (CopyableError)      | `migration-job-error` (+ `-text` / `-copy`)     |
//!
//! React renders the *same* testid (`-artifact`) on every artifact row; the
//! Rune runtime suffixes later duplicate keys with `#dupN`, so probes count
//! artifact rows by key prefix. That is a faithful mirror of React's
//! `$$('[data-testid="…"]')`, not a key defect.
//!
//! # Documented deviations from React
//!
//! - **No `data-*` API**: rune cannot set arbitrary DOM attributes. The root's
//!   `data-severity`/`data-cancel-disposition`/`data-uncertainty`/
//!   `data-verified-boundaries`/`data-unverified-boundaries`/`data-plan-*` are
//!   mirrored by (a) pure [`VerdictSeverity::as_str`] /
//!   [`CancelDisposition::as_str`] / [`UncertaintyReason::as_str`] accessors and
//!   (b) a zero-size, `a11y_hidden` keyed marker child whose a11y label carries
//!   every attribute value. `data-evidence-gap` and `data-boundary-verified`
//!   get their own keyed markers. Values that are already visible text (artifact
//!   id, recovery verdict/reason) need no extra channel.
//! - **No action buttons / table / Banner in the React source**: the only
//!   interactive affordance is the copy control inside `CopyableError`; there
//!   is no `Button`, `<table>` or `ErrorBanner` in this component, so none are
//!   invented here.
//! - **Copied feedback**: React's `CopyableError` owns a 1500ms
//!   `useCopyFeedback` timer. The rune base schedules the same `wake_after`
//!   but leaves the deadline in host state, so the panel exposes
//!   [`MigrationJobVerdictPanel::copied`] and
//!   [`MigrationJobVerdictPanel::on_copy_tx`] for the future view to mirror
//!   `copied_until = tx.clock() + 1.5`.
//! - All colours come from `kit(dark)`; no `oklch` literal appears here.

use std::rc::Rc;

use rune::{col, row, text, El, Tx};
use rune_kit::Kit;

use crate::i18n::t;
use crate::ui::badge::{Badge, BadgeTone};
use crate::ui::copyable::CopyableError;

/// React `VerdictSeverity = 'ok' | 'partial' | 'uncertain' | 'failed'`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // T106: consumed by the migration job view; remove when wired.
pub enum VerdictSeverity {
    Ok,
    #[default]
    Partial,
    Uncertain,
    Failed,
}

#[allow(dead_code)] // T106: consumed by the migration job view; remove when wired.
impl VerdictSeverity {
    /// Exact `data-severity` value from React.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Partial => "partial",
            Self::Uncertain => "uncertain",
            Self::Failed => "failed",
        }
    }

    /// Exact i18n key from React's `severityKey()`.
    pub fn key(self) -> &'static str {
        match self {
            Self::Ok => "migration.verdict.ok",
            Self::Partial => "migration.verdict.partial",
            Self::Uncertain => "migration.verdict.uncertain",
            Self::Failed => "migration.verdict.failed",
        }
    }
}

/// React `CancelDisposition` — a cancel *intent* contrasted with where the run
/// actually stopped. "Cancel requested" is never "cancelled".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // T106: consumed by the migration job view; remove when wired.
pub enum CancelDisposition {
    #[default]
    None,
    NotStarted,
    RequestedInFlight,
    SettledPartially,
    SettledRolledBack,
    SettledCompleted,
    SettledUnknown,
}

#[allow(dead_code)] // T106: consumed by the migration job view; remove when wired.
impl CancelDisposition {
    /// Exact `data-cancel-disposition` value / key segment from React.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::NotStarted => "notStarted",
            Self::RequestedInFlight => "requestedInFlight",
            Self::SettledPartially => "settledPartially",
            Self::SettledRolledBack => "settledRolledBack",
            Self::SettledCompleted => "settledCompleted",
            Self::SettledUnknown => "settledUnknown",
        }
    }

    /// Exact i18n key from React's `cancelDispositionKey()`.
    pub fn key(self) -> &'static str {
        match self {
            Self::None => "migration.cancel.none",
            Self::NotStarted => "migration.cancel.notStarted",
            Self::RequestedInFlight => "migration.cancel.requestedInFlight",
            Self::SettledPartially => "migration.cancel.settledPartially",
            Self::SettledRolledBack => "migration.cancel.settledRolledBack",
            Self::SettledCompleted => "migration.cancel.settledCompleted",
            Self::SettledUnknown => "migration.cancel.settledUnknown",
        }
    }
}

/// React `UncertaintyReason`. `None` returns no badge at all.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // T106: consumed by the migration job view; remove when wired.
pub enum UncertaintyReason {
    #[default]
    None,
    NoCheckpoint,
    RecoveryRejected,
    EffectOutcomeUnknown,
    MissingEvidence,
    ManualReviewRequired,
}

#[allow(dead_code)] // T106: consumed by the migration job view; remove when wired.
impl UncertaintyReason {
    /// Exact `data-uncertainty` value from React.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::NoCheckpoint => "noCheckpoint",
            Self::RecoveryRejected => "recoveryRejected",
            Self::EffectOutcomeUnknown => "effectOutcomeUnknown",
            Self::MissingEvidence => "missingEvidence",
            Self::ManualReviewRequired => "manualReviewRequired",
        }
    }

    /// Exact i18n key from React's `uncertaintyKey()`; `None` mounts no badge.
    pub fn key(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::NoCheckpoint => Some("migration.uncertainty.noCheckpoint"),
            Self::RecoveryRejected => Some("migration.uncertainty.recoveryRejected"),
            Self::EffectOutcomeUnknown => Some("migration.uncertainty.effectOutcomeUnknown"),
            Self::MissingEvidence => Some("migration.uncertainty.missingEvidence"),
            Self::ManualReviewRequired => Some("migration.uncertainty.manualReviewRequired"),
        }
    }
}

/// The presentation model React receives as `MigrationJobVerdict`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // T106: consumed by the migration job view; remove when wired.
pub struct MigrationJobVerdict {
    pub severity: VerdictSeverity,
    pub cancel_disposition: CancelDisposition,
    pub completed: bool,
    pub verified_boundaries: u64,
    pub unverified_boundaries: u64,
    pub uncertainty: UncertaintyReason,
    pub requires_reconcile: bool,
}

/// Raw five-bucket progress. Rust's `Counter` serializes as a decimal string,
/// so each bucket is `Option<String>`; a malformed/absent bucket is *not* read
/// as zero (see [`read_job_counter`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // T106: consumed by the migration job view; remove when wired.
pub struct JobProgress {
    pub read: Option<String>,
    pub converted: Option<String>,
    pub attempted: Option<String>,
    pub committed: Option<String>,
    pub unknown: Option<String>,
}

/// Largest integer JavaScript's `Number.isSafeInteger` accepts.
const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

/// Mirror of React `readJobCounter`: accepts only a non-negative decimal string
/// that is a safe integer, otherwise `None` (React returns `null`).
#[allow(dead_code)] // T106: consumed by the migration job view; remove when wired.
pub fn read_job_counter(value: &Option<String>) -> Option<u64> {
    let raw = value.as_ref()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() || !trimmed.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let parsed = trimmed.parse::<u64>().ok()?;
    (parsed <= MAX_SAFE_INTEGER).then_some(parsed)
}

/// Mirror of React `sumJobCounters`: `None` when *any* bucket is unreadable.
#[allow(dead_code)] // T106: consumed by the migration job view; remove when wired.
pub fn sum_job_counters(values: &[Option<String>]) -> Option<u64> {
    let mut total = 0u64;
    for value in values {
        total = total.checked_add(read_job_counter(value)?)?;
    }
    Some(total)
}

/// One React `CommitBoundary`. `evidence` is the proof that makes the boundary
/// "verified"; an empty list is shown as unverified, never as committed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // T106: consumed by the migration job view; remove when wired.
pub struct CommitBoundary {
    pub stage_id: String,
    pub committed_at: String,
    pub evidence: Vec<String>,
    pub operation_id: Option<String>,
}

/// React `plan` identity. Surfaced as `data-*` (not as copy) so a spec and a
/// support ticket can name the exact plan this verdict belongs to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // T106: consumed by the migration job view; remove when wired.
pub struct PlanInfo {
    pub plan_id: Option<String>,
    pub plan_digest: Option<String>,
    pub selection_revision: Option<u64>,
}

/// The whole controlled panel. Fields mirror `MigrationJobVerdictPanelProps`;
/// every optional string is passed through unchanged (all UI copy goes through
/// `t()` with the exact React keys).
#[derive(Clone, Debug)]
#[allow(dead_code)] // T106: consumed by the migration job view; remove when wired.
pub struct VerdictPanelVm {
    pub verdict: MigrationJobVerdict,
    pub progress: Option<JobProgress>,
    pub commit_boundaries: Vec<CommitBoundary>,
    pub artifact_ids: Vec<String>,
    pub cancel_requested: bool,
    /// `false` means the backend had no such Job — a cancel that did nothing.
    pub cancel_acknowledged: bool,
    pub replayed: bool,
    pub recovery_verdict: Option<String>,
    pub recovery_reason: Option<String>,
    /// `None` means "nothing safe to resume", which is not the same as `0`.
    pub recovery_resume_through: Option<u64>,
    pub error: Option<String>,
    pub plan: Option<PlanInfo>,
    pub test_id_prefix: String,
}

impl Default for VerdictPanelVm {
    fn default() -> Self {
        Self {
            verdict: MigrationJobVerdict::default(),
            progress: None,
            commit_boundaries: Vec::new(),
            artifact_ids: Vec::new(),
            cancel_requested: false,
            cancel_acknowledged: false,
            replayed: false,
            recovery_verdict: None,
            recovery_reason: None,
            recovery_resume_through: None,
            error: None,
            plan: None,
            test_id_prefix: "migration-job".to_owned(),
        }
    }
}

type CopyCallback<S> = Rc<dyn Fn(&mut S, &mut Tx)>;

/// Builder for the verdict panel. `S` is the host state that owns the copied
/// feedback deadline (mirroring React's `useCopyFeedback` hook).
#[allow(dead_code)] // T106: consumed by the migration job view; remove when wired.
pub struct MigrationJobVerdictPanel<S: 'static> {
    vm: VerdictPanelVm,
    copied: bool,
    on_copy: Option<CopyCallback<S>>,
}

#[allow(dead_code)] // T106: consumed by the migration job view; remove when wired.
impl<S: 'static> MigrationJobVerdictPanel<S> {
    pub fn new(vm: VerdictPanelVm) -> Self {
        Self {
            vm,
            copied: false,
            on_copy: None,
        }
    }

    /// Whether the transient "已复制" feedback is visible this frame. The host
    /// computes this as `now < copied_until`.
    pub fn copied(mut self, copied: bool) -> Self {
        self.copied = copied;
        self
    }

    /// React `useCopyFeedback`'s deadline write: record
    /// `copied_until = tx.clock() + 1.5` in host state. The control itself
    /// copies the message and schedules the real 1500ms runtime wake.
    pub fn on_copy_tx(mut self, f: impl Fn(&mut S, &mut Tx) + 'static) -> Self {
        self.on_copy = Some(Rc::new(f));
        self
    }

    pub fn build(self, dark: bool) -> El {
        let k: &'static Kit = crate::ui::kit::kit(dark);
        let Self {
            vm,
            copied,
            on_copy,
        } = self;
        let prefix = vm.test_id_prefix.clone();
        let verdict = vm.verdict;

        let mut root = col()
            .key(format!("{prefix}-verdict"))
            .w_full()
            .gap(12.0)
            .radius(8.0)
            .border(1.0, k.c.border_base)
            .fill(k.c.layer_02)
            .padding(24.0);

        // data-* mirror: a hidden keyed node carries every root attribute in an
        // a11y label (rune has no arbitrary data attribute API).
        root = root.child(data_attrs(&prefix, &vm));

        // Status line: the exact severity key, coloured by the React class map.
        let tone = match verdict.severity {
            VerdictSeverity::Ok => k.c.success.fg,
            VerdictSeverity::Partial | VerdictSeverity::Uncertain => k.c.warning.fg,
            VerdictSeverity::Failed => k.c.danger.fg,
        };
        root = root.child(
            text(t(verdict.severity.key()))
                .key(format!("{prefix}-verdict-status"))
                .role("status")
                .font(k.ty.ui_medium)
                .color(tone),
        );

        // Intent sentence (cancel) is deliberately separate from the outcome.
        root = root.child(
            text(t(verdict.cancel_disposition.key()))
                .key(format!("{prefix}-cancel"))
                .font(k.ty.text_small)
                .color(k.c.text_muted)
                .wrap(),
        );
        if vm.cancel_requested && !vm.cancel_acknowledged {
            root = root.child(
                text(t("migration.cancel.unknownJob"))
                    .key(format!("{prefix}-cancel-unknown-job"))
                    .font(k.ty.text_small)
                    .color(k.c.warning.fg)
                    .wrap(),
            );
        }

        if let Some(progress) = vm.progress.as_ref() {
            let counts = format!(
                "{}: {} / {} / {} / {} / {}",
                t("migration.progress.counts"),
                read_job_counter(&progress.read).unwrap_or(0),
                read_job_counter(&progress.converted).unwrap_or(0),
                read_job_counter(&progress.attempted).unwrap_or(0),
                read_job_counter(&progress.committed).unwrap_or(0),
                read_job_counter(&progress.unknown).unwrap_or(0),
            );
            root = root.child(
                text(counts)
                    .key(format!("{prefix}-progress"))
                    .font(k.ty.text_small)
                    .color(k.c.text_base)
                    .wrap(),
            );
        }

        // Unknown rows are counted *beside* committed ones, never folded in.
        let committed_rows = vm
            .progress
            .as_ref()
            .map(|p| sum_job_counters(&[p.committed.clone(), p.unknown.clone()]))
            .unwrap_or_else(|| sum_job_counters(&[None, None]));
        let rows_line = match committed_rows {
            Some(count) => format!("{}: {}", t("migration.verdict.committedRows"), count),
            None => format!("{}: ", t("migration.verdict.committedRows")),
        };
        root = root.child(
            text(rows_line)
                .key(format!("{prefix}-rows"))
                .font(k.ty.text_small)
                .color(k.c.text_base),
        );

        if vm.replayed {
            root = root.child(
                text(t("migration.verdict.replayedReceipt"))
                    .key(format!("{prefix}-replayed"))
                    .font(k.ty.text_small)
                    .color(k.c.text_muted)
                    .wrap(),
            );
        }

        root = build_boundaries(root, k, &prefix, &vm);

        if !vm.artifact_ids.is_empty() {
            let mut list = col()
                .key(format!("{prefix}-artifacts"))
                .gap(4.0)
                .min_w(0.0);
            for artifact_id in &vm.artifact_ids {
                // React uses the same testid on every row; the runtime's #dupN
                // suffix is the faithful equivalent of React's $$() query.
                list = list.child(
                    text(artifact_id.clone())
                        .key(format!("{prefix}-artifact"))
                        .font(k.ty.text_xsmall)
                        .color(k.c.text_base)
                        .wrap_anywhere(),
                );
            }
            root = root.child(list);
        }

        root = build_recovery(root, k, &prefix, &vm);

        if let Some(key) = verdict.uncertainty.key() {
            root = root.child(
                Badge::new(format!("{prefix}-uncertainty"), t(key))
                    .tone(BadgeTone::Warning)
                    .build(dark),
            );
        }

        if verdict.requires_reconcile {
            root = root.child(
                text(t("migration.verdict.requiresReconcile"))
                    .key(format!("{prefix}-reconcile"))
                    .font(k.ty.text_small)
                    .color(k.c.warning.fg)
                    .wrap(),
            );
        }

        if let Some(message) = vm.error.as_ref() {
            let error_key = format!("{prefix}-error");
            let mut error = CopyableError::new(error_key, message.clone()).copy_button(true);
            error = error.copied(copied);
            if let Some(on_copy) = on_copy.clone() {
                error = error.on_copy_tx(move |s: &mut S, tx: &mut Tx| on_copy(s, tx));
            }
            root = root.child(error.build::<S>(k));
        }

        root
    }
}

/// Build the boundaries list (or the "no boundaries" sentence with its
/// `data-evidence-gap` marker).
fn build_boundaries(mut root: El, k: &'static Kit, prefix: &str, vm: &VerdictPanelVm) -> El {
    if !vm.commit_boundaries.is_empty() {
        let mut list = col()
            .key(format!("{prefix}-boundaries"))
            .gap(4.0)
            .min_w(0.0);
        for boundary in &vm.commit_boundaries {
            let verified = !boundary.evidence.is_empty();
            let verdict_key = if verified {
                "migration.boundary.verified"
            } else {
                "migration.boundary.unverified"
            };
            let verdict_tone = if verified {
                k.c.success.fg
            } else {
                k.c.warning.fg
            };

            let mut row_el = row()
                .key(format!("{prefix}-boundary-{}", boundary.stage_id))
                .items_center()
                .gap(2.0)
                .radius(4.0)
                .border(1.0, k.c.border_base)
                .fill(k.c.layer_02)
                .px(8.0)
                .py(8.0)
                .min_w(0.0)
                .child(
                    text(boundary.stage_id.clone())
                        .font(k.ty.small_medium)
                        .color(k.c.text_base),
                )
                .child(text("·").font(k.ty.text_xsmall).color(k.c.text_muted))
                .child(
                    text(t(verdict_key))
                        .font(k.ty.text_xsmall)
                        .color(verdict_tone)
                        .wrap(),
                );
            // data-boundary-verified mirror (a rune keyed marker).
            row_el = row_el.child(
                row()
                    .key(format!(
                        "{prefix}-boundary-{}-verified-{}",
                        boundary.stage_id, verified
                    ))
                    .size(0.0, 0.0)
                    .absolute(0.0, 0.0)
                    .a11y_hidden(),
            );
            // React's internal key was `${stageId}:${committedAt}`; keep the
            // committed-at observable without polluting the testid key.
            row_el = row_el.child(
                row()
                    .key(format!(
                        "{prefix}-boundary-{}-committed-at",
                        boundary.stage_id
                    ))
                    .size(0.0, 0.0)
                    .absolute(0.0, 0.0)
                    .a11y_hidden()
                    .label(boundary.committed_at.clone()),
            );
            if let Some(operation_id) = boundary.operation_id.as_ref() {
                row_el = row_el
                    .child(text("·").font(k.ty.text_xsmall).color(k.c.text_muted))
                    .child(
                        text(operation_id.clone())
                            .font(k.ty.text_xsmall)
                            .color(k.c.text_muted)
                            .wrap_anywhere(),
                    );
            }
            list = list.child(row_el);
        }
        root = root.child(list);
    } else {
        let evidence_gap = vm.verdict.uncertainty == UncertaintyReason::MissingEvidence;
        root = root.child(
            text(t("migration.verdict.noBoundaries"))
                .key(format!("{prefix}-boundaries-empty"))
                .font(k.ty.text_small)
                .color(if evidence_gap {
                    k.c.warning.fg
                } else {
                    k.c.text_muted
                })
                .wrap(),
        );
        if evidence_gap {
            // data-evidence-gap mirror (a rune keyed marker sibling).
            root = root.child(
                row()
                    .key(format!("{prefix}-boundaries-empty-attrs-true"))
                    .size(0.0, 0.0)
                    .absolute(0.0, 0.0)
                    .a11y_hidden(),
            );
        }
    }
    root
}

/// Build the recovery verdict / reason / resume-through sentences.
fn build_recovery<'a>(
    mut root: El,
    k: &'static Kit,
    prefix: &str,
    vm: &VerdictPanelVm,
) -> El {
    if let Some(recovery_verdict) = vm.recovery_verdict.as_ref() {
        let shown = if recovery_verdict == "notExecuted" {
            t("migration.verdict.notExecuted")
        } else {
            recovery_verdict.as_str()
        };
        root = root.child(
            text(format!(
                "{}: {}",
                t("migration.verdict.recoveryLabel"),
                shown
            ))
            .key(format!("{prefix}-recovery-verdict"))
            .font(k.ty.text_small)
            .color(k.c.text_base)
            .wrap(),
        );
    }

    if let Some(recovery_reason) = vm.recovery_reason.as_ref() {
        let restart_before_dispatch = vm.recovery_verdict.as_deref() == Some("notExecuted")
            && recovery_reason == "notDispatchedAfterRestart";
        let shown = if restart_before_dispatch {
            t("migration.verdict.notDispatchedAfterRestart")
        } else {
            recovery_reason.as_str()
        };
        root = root.child(
            text(shown)
                .key(format!("{prefix}-recovery-reason"))
                .font(k.ty.text_small)
                .color(k.c.text_muted)
                .wrap(),
        );
    }

    if vm.recovery_verdict.is_some() {
        if let Some(through) = vm.recovery_resume_through {
            root = root.child(
                text(format!(
                    "{}: {}",
                    t("migration.verdict.resumeThrough"),
                    through
                ))
                .key(format!("{prefix}-recovery-resume-through"))
                .font(k.ty.text_small)
                .color(k.c.text_muted),
            );
        }
    }
    root
}

/// Hidden marker carrying all root `data-*` attributes in one a11y label.
fn data_attrs(prefix: &str, vm: &VerdictPanelVm) -> El {
    let plan_id = vm
        .plan
        .as_ref()
        .and_then(|p| p.plan_id.clone())
        .unwrap_or_default();
    let plan_digest = vm
        .plan
        .as_ref()
        .and_then(|p| p.plan_digest.clone())
        .unwrap_or_default();
    let revision = vm
        .plan
        .as_ref()
        .and_then(|p| p.selection_revision)
        .map(|n| n.to_string())
        .unwrap_or_default();
    let label = format!(
        "severity={}|cancel={}|uncertainty={}|verified={}|unverified={}|cancel_requested={}|cancel_acknowledged={}|plan_id={}|plan_digest={}|selection_revision={}",
        vm.verdict.severity.as_str(),
        vm.verdict.cancel_disposition.as_str(),
        vm.verdict.uncertainty.as_str(),
        vm.verdict.verified_boundaries,
        vm.verdict.unverified_boundaries,
        vm.cancel_requested,
        vm.cancel_acknowledged,
        plan_id,
        plan_digest,
        revision,
    );
    row()
        .key(format!("{prefix}-verdict-attrs"))
        .size(0.0, 0.0)
        .absolute(0.0, 0.0)
        .a11y_hidden()
        .label(label)
}
