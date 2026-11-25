//! T247: `windows/data-sync/DataSyncResultStep.tsx` ported to Rune.
//!
//! Last step of the Data Sync wizard: the outcome of the execution and
//! the comparison that follows it. The outcome derivation is React's own
//! chain — `lastExecutionResult?.outcome ?? (rolledBack ? 'rolled_back' :
//! 'committed')` — with the amber/green verdict line (the reason text or
//! its pack word, the skipped-conflicts aside, and the re-compare button)
//! above the compare summary strip.
//!
//! ## What maps where
//!
//! | React | here |
//! |---|---|
//! | `DataSyncExecutionResponse` (aliased `DataSyncExecutionResult` in `commands/sync.ts`) | the [`DataSyncExecutionResult`] mirror below |
//! | `outcome ?? (rolledBack ? … : …)` | [`DataSyncExecutionResult::outcome_or`] |
//! | the amber/green class split | the verdict color: warning fg vs the kit success/accent |
//!
//! ## Keys
//!
//! | part | key |
//! |---|---|
//! | root (React test id) | `data-sync-result` |
//! | verdict row (React test id) | `data-sync-execute-done` |
//! | verdict span (internal) | `…-verdict` |
//! | skipped span (internal) | `…-skipped` |
//! | re-compare button (React test id) | `data-sync-re-compare` |
//! | the compare summary | `CompareSummary`'s own keys |
//!
//! ## Deviations
//!
//! * `text-amber-700 dark:text-amber-400` vs `text-green-700
//!   dark:text-green-400` — the kit warning vs the crate's green mapping
//!   for success text (the job-status panel's committed green
//!   precedent) — no new color literals.
//! * `flex-wrap` — the verdict row is a wrapping row with the gaps React
//!   names; Rune rows do not wrap, so the gaps carry the layout (the
//!   960pt minimum still fits the row).

// The production consumer is the Data Sync window track (T268); until it
// wires the step in, the component tests are the only callers.
#![allow(dead_code)] // reusable component; wired by the Data Sync wizard (T268)

use std::rc::Rc;

use rune::{col, row, text, El};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::i18n::t;
use crate::ui::kit::kit;
use crate::views::data_sync::compare_summary::CompareSummary;
use crate::views::data_sync::mapping_view::CompareSummaryStats;

/// React `DataSyncExecutionOutcome` (`backend-client/types/dataSync.ts:51`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataSyncExecutionOutcome {
    NotStarted,
    Committed,
    PartiallyApplied,
    RolledBack,
    Unknown,
}

/// React `DataSyncExecutionResponse`, aliased `DataSyncExecutionResult` in
/// `commands/sync.ts:62` — the evidence-based outcome of one execution.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DataSyncExecutionResult {
    pub applied: usize,
    /// A confirmed rollback.
    pub rolled_back: bool,
    /// Explains a confirmed rollback; absent means committed or
    /// unknown-outcome.
    pub rollback_reason: Option<String>,
    /// Total database-reported affected rows; optional on older responses.
    pub affected_rows: Option<usize>,
    /// Conflicts skipped by the conflict policy.
    pub skipped: Option<usize>,
    /// The evidence-based outcome; `None` mirrors the wire's absent field.
    pub outcome: Option<DataSyncExecutionOutcome>,
}

impl DataSyncExecutionResult {
    /// React's derivation: `outcome ?? (rolledBack ? 'rolled_back' :
    /// 'committed')` — a missing outcome reads the rollback flag instead.
    pub fn outcome_or(&self) -> DataSyncExecutionOutcome {
        self.outcome.unwrap_or(if self.rolled_back {
            DataSyncExecutionOutcome::RolledBack
        } else {
            DataSyncExecutionOutcome::Committed
        })
    }
}

/// The step builder. `S` is the host state that owns the report and the
/// explain/re-compare actions.
pub struct DataSyncResultStep<S: 'static> {
    last_execution_result: Option<DataSyncExecutionResult>,
    stats: CompareSummaryStats,
    explain_loading: bool,
    on_copy_report: Option<Rc<dyn Fn(&mut S)>>,
    on_explain_diff: Option<Rc<dyn Fn(&mut S)>>,
    on_re_compare: Option<Rc<dyn Fn(&mut S)>>,
}

impl<S: 'static> DataSyncResultStep<S> {
    pub fn new(
        last_execution_result: Option<DataSyncExecutionResult>,
        stats: CompareSummaryStats,
    ) -> Self {
        Self {
            last_execution_result,
            stats,
            explain_loading: false,
            on_copy_report: None,
            on_explain_diff: None,
            on_re_compare: None,
        }
    }

    /// `onCopyReport` — passed to the compare summary.
    pub fn on_copy_report(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_copy_report = Some(Rc::new(f));
        self
    }

    /// `onExplainDiff` — passed to the compare summary.
    pub fn on_explain_diff(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_explain_diff = Some(Rc::new(f));
        self
    }

    /// `explainLoading` — the explain button's loading gate.
    pub fn explain_loading(mut self, loading: bool) -> Self {
        self.explain_loading = loading;
        self
    }

    /// `onReCompare`.
    pub fn on_re_compare(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_re_compare = Some(Rc::new(f));
        self
    }

    pub fn build(self, dark: bool) -> El {
        let k = kit(dark);
        let outcome = self
            .last_execution_result
            .as_ref()
            .map(DataSyncExecutionResult::outcome_or)
            .unwrap_or(DataSyncExecutionOutcome::Committed);
        let is_unknown = outcome == DataSyncExecutionOutcome::Unknown;
        let is_partial = outcome == DataSyncExecutionOutcome::PartiallyApplied;
        let was_rolled_back = outcome == DataSyncExecutionOutcome::RolledBack;
        // The amber verdict for the non-clean outcomes, green otherwise.
        let verdict_color = if is_unknown || was_rolled_back || is_partial {
            k.c.warning.fg
        } else {
            k.c.text_accent
        };

        let mut verdict = row()
            .key("data-sync-execute-done")
            .w_full()
            .items_center()
            .gap(12.0)
            .role("status");
        let verdict_text = if is_unknown {
            t("sync.executionUnknown").to_owned()
        } else if is_partial {
            t("migration.verdict.partial").to_owned()
        } else if was_rolled_back {
            self.last_execution_result
                .as_ref()
                .and_then(|result| result.rollback_reason.as_deref())
                .filter(|reason| !reason.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| t("sync.rolledBack").to_owned())
        } else {
            t("sync.executeDone").to_owned()
        };
        verdict = verdict
            .child(
                text(verdict_text)
                    .key("data-sync-execute-done-verdict")
                    .font(k.ty.text_small)
                    .color(verdict_color),
            )
            .wrap();
        if let Some(skipped) = self
            .last_execution_result
            .as_ref()
            .and_then(|result| result.skipped)
            .filter(|skipped| *skipped != 0)
        {
            verdict = verdict.child(
                text(t("sync.conflictsSkipped").replace("{count}", &skipped.to_string()))
                    .key("data-sync-execute-done-skipped")
                    .font(k.ty.text_small)
                    .color(k.c.warning.fg),
            );
        }
        if let Some(callback) = self.on_re_compare.clone() {
            verdict = verdict.child(
                Button::new(t("sync.reCompare"))
                    .variant(ButtonVariant::Outline)
                    .control_size(ControlSize::Small)
                    .on_press(move |s: &mut S| callback(s))
                    .build(&k)
                    .key("data-sync-re-compare"),
            );
        }

        let mut summary = CompareSummary::<S>::new(self.stats);
        if let Some(callback) = self.on_copy_report.clone() {
            summary = summary.on_copy_report(move |s| callback(s));
        }
        if let Some(callback) = self.on_explain_diff.clone() {
            summary = summary.on_explain_diff(move |s| callback(s));
        }
        summary = summary.explain_loading(self.explain_loading);

        col()
            .key("data-sync-result")
            .w_full()
            .gap(16.0)
            .radius(4.0)
            .border(1.0, k.c.border_base)
            .fill(k.c.surface_weak)
            .px(24.0)
            .py(24.0)
            .child(verdict)
            .child(summary.build(dark))
    }
}
