//! Port of
//! `packages/frontend/src/components/migration/MigrationRunHistoryDialog.tsx`.
//!
//! The React component owns its fetch/loading state with hooks. Rune views are
//! pure functions, so this port is a presentation builder: the caller supplies
//! `open`, `loading`, `error`, the run list and the selected id, and receives
//! callbacks for the trigger, the close, a row selection and the Data Sync
//! reconciliation. The fetch itself belongs to the host/slice that owns the
//! history port; no IPC is invented here. (Removal stage: when that view lands,
//! these inputs are wired there and the per-item `#[allow(dead_code)]` below are
//! dropped one by one.)
//!
//! React has no pagination in this dialog: it renders every returned row in a
//! `max-h-[65vh] overflow-auto` column. The Rune port keeps the same scroll
//! surface (`migration-run-history` key) and caps the dialog body at 65% of the
//! 640pt minimum window; the content scrolls. No colour literal appears here:
//! all tones come from `kit(dark)`.
//!
//! # Key convention
//!
//! React `data-testid` becomes the element key:
//!
//! | React testid                       | key                                          |
//! |------------------------------------|----------------------------------------------|
//! | `migration-history-open`           | `migration-history-open`                     |
//! | `migration-run-history`            | `migration-run-history`                      |
//! | `migration-run-${id}`              | `migration-run-${id}`                        |
//! | `migration-run-detail`             | `migration-run-detail`                       |
//! | `migration-history-rollback-outcome` | `migration-history-rollback-outcome`       |
//! | `migration-run-reconcile`          | `migration-run-reconcile`                    |
//!
//! Rune has no `data-*` API, so `data-outcome` is mirrored as the element's
//! a11y `label` plus a hidden keyed marker for the warning class (the pattern
//! used by the neighbouring migration components).

use std::rc::Rc;

use rune::{col, row, text, El, ScrollSpec};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::i18n::t;
use crate::icons;
use crate::support::formatters::format_last_connected;
use crate::ui::banner::{Banner, BannerElement, BannerVariant};
use crate::ui::dialog::Dialog;
use crate::ui::kit::kit;
use crate::ui::spinner::{Spinner, SpinnerSize, SpinnerVariant};

/// 65vh of the 960x640 minimum window (React `max-h-[65vh]`). A view has no
/// viewport, so the fixed figure matches the pre-port minimum-window size.
const HISTORY_BODY_MAX_HEIGHT: f32 = 416.0;

/// React `MigrationOperation` (backend-client domain vocabulary).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // reusable VM; drop when the migration-history view wires it.
pub enum MigrationOperation {
    SchemaDiff,
    DataSync,
    DataTransfer,
}

#[allow(dead_code)] // reusable VM; drop when the migration-history view wires it.
impl MigrationOperation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SchemaDiff => "schemaDiff",
            Self::DataSync => "dataSync",
            Self::DataTransfer => "dataTransfer",
        }
    }
}

/// The subset of React `MigrationRunRecord` this dialog actually renders.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // reusable VM; drop when the migration-history view wires it.
pub struct MigrationRunRecord {
    pub id: String,
    pub operation: MigrationOperation,
    pub status: String,
    pub outcome: String,
    pub phase: String,
    pub profile_id: Option<String>,
    pub profile_revision: Option<String>,
    pub started_at: String,
    pub selected_count: u64,
    pub committed_count: u64,
    pub failed_count: u64,
    pub conflict_count: u64,
    pub rollback_outcome: String,
    pub error_summary: Option<String>,
}

/// React `rollbackDisplay(run)`: only a known Transfer outcome is localized
/// and only `unknown` / `partiallyApplied` are warned; every other case keeps
/// the raw historical string verbatim.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // reusable VM; drop when the migration-history view wires it.
pub struct RollbackDisplay {
    pub label: String,
    pub uncertain: bool,
    pub localized: bool,
}

/// Port of React's `rollbackDisplay` closure.
#[allow(dead_code)] // reusable helper; drop when the migration-history view wires it.
pub fn rollback_display(run: &MigrationRunRecord) -> RollbackDisplay {
    if run.operation != MigrationOperation::DataTransfer {
        return RollbackDisplay {
            label: run.rollback_outcome.clone(),
            uncertain: false,
            localized: false,
        };
    }
    let known = [
        "notRequired",
        "notStarted",
        "rolledBack",
        "unknown",
        "partiallyApplied",
    ];
    if !known.contains(&run.rollback_outcome.as_str()) {
        return RollbackDisplay {
            label: run.rollback_outcome.clone(),
            uncertain: false,
            localized: false,
        };
    }
    let uncertain = matches!(
        run.rollback_outcome.as_str(),
        "unknown" | "partiallyApplied"
    );
    RollbackDisplay {
        label: t(&format!("transfer.historyOutcome.{}", run.rollback_outcome)).to_owned(),
        uncertain,
        localized: true,
    }
}

type Action<S> = Rc<dyn Fn(&mut S)>;
type SelectAction<S> = Rc<dyn Fn(&mut S, String)>;

/// Builder for the run-history trigger plus its dialog layer.
#[allow(dead_code)] // reusable component; drop when the migration-history view wires it.
pub struct MigrationRunHistoryDialog<S: 'static> {
    operation: MigrationOperation,
    open: bool,
    open_epoch: u64,
    loading: bool,
    error: Option<String>,
    items: Vec<MigrationRunRecord>,
    selected_id: Option<String>,
    on_open: Option<Action<S>>,
    on_close: Option<Action<S>>,
    on_select: Option<SelectAction<S>>,
    on_reconcile: Option<SelectAction<S>>,
}

#[allow(dead_code)] // reusable component; drop when the migration-history view wires it.
impl<S: 'static> MigrationRunHistoryDialog<S> {
    pub fn new(operation: MigrationOperation) -> Self {
        Self {
            operation,
            open: false,
            open_epoch: 0,
            loading: false,
            error: None,
            items: Vec::new(),
            selected_id: None,
            on_open: None,
            on_close: None,
            on_select: None,
            on_reconcile: None,
        }
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    pub fn open_epoch(mut self, epoch: u64) -> Self {
        self.open_epoch = epoch;
        self
    }

    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }

    pub fn error(mut self, error: Option<String>) -> Self {
        self.error = error;
        self
    }

    pub fn items(mut self, items: Vec<MigrationRunRecord>) -> Self {
        self.items = items;
        self
    }

    pub fn selected(mut self, id: Option<String>) -> Self {
        self.selected_id = id;
        self
    }

    pub fn on_open(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_open = Some(Rc::new(f));
        self
    }

    pub fn on_close(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close = Some(Rc::new(f));
        self
    }

    pub fn on_select(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_select = Some(Rc::new(f));
        self
    }

    /// Only meaningful for a Data Sync dialog; the host decides whether to
    /// accept. A refused reconciliation keeps the dialog open (React closes it
    /// only when `onReconcile` resolves true).
    pub fn on_reconcile(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_reconcile = Some(Rc::new(f));
        self
    }

    /// Return the trigger followed by the dialog's layer elements. Callers push
    /// the layer to the root view as siblings (the same shape as `modal_layer`).
    pub fn build(self, dark: bool) -> Vec<El> {
        let k: &'static Kit = kit(dark);
        let mut out = Vec::new();

        // React: secondary, h-6, px-2 with a Clock3 glyph. Kit has no
        // "secondary" variant; Neutral is its neutral secondary action.
        let mut trigger = Button::new(t("migrationHistory.open"))
            .variant(ButtonVariant::Neutral)
            .control_size(ControlSize::Small)
            .icon(&icons::ICON_CLOCK)
            .build(k)
            .key("migration-history-open");
        if let Some(on_open) = &self.on_open {
            let action = on_open.clone();
            trigger = trigger.on_press(move |state: &mut S| action(state));
        }
        out.push(trigger);

        if !self.open {
            return out;
        }
        // Dialog::build mounts nothing without a dismissal policy; install a
        // no-op when the caller did not wire one.
        let close: Action<S> = self
            .on_close
            .clone()
            .unwrap_or_else(|| Rc::new(|_state: &mut S| {}));
        let close_action = close;
        let mut dialog = Dialog::new("migration-run-history-dialog", t("migrationHistory.title"))
            .open(true)
            .open_epoch(self.open_epoch)
            .width(640.0)
            .fit()
            .body_max_height(HISTORY_BODY_MAX_HEIGHT)
            .body_pad((0.0, 0.0))
            .on_close(move |state: &mut S| close_action(state))
            .child(self.body(k, dark));
        dialog.build(k, &mut out);
        out
    }

    /// The scrollable list, mutual loading/error/empty states and the selected
    /// run's detail panel.
    fn body(&self, k: &'static Kit, dark: bool) -> El {
        let mut body = col()
            .key("migration-run-history")
            .w_full()
            .min_w(0.0)
            .gap(0.0)
            .scroll(ScrollSpec {
                stick_bottom: false,
            });

        if self.loading {
            body = body.child(
                row()
                    .key("migration-run-loading")
                    .w_full()
                    .gap(8.0)
                    .center_y()
                    .padding(16.0)
                    .child(Spinner::new("migration-run-loading-spinner")
                        .size(SpinnerSize::Lg)
                        .variant(SpinnerVariant::Ring)
                        .build(dark))
                    .child(
                        text(t("common.loading"))
                            .key("migration-run-loading-label")
                            .font(k.ty.text_small)
                            .color(k.c.text_muted),
                    ),
            );
        }

        if let Some(error) = &self.error {
            if let Some(banner) = Banner::new("migration-run-error", error.clone())
                .variant(BannerVariant::Boxed)
                .as_element(BannerElement::P)
                .build(k)
            {
                body = body.child(banner);
            }
        }

        if !self.loading && self.error.is_none() && self.items.is_empty() {
            body = body.child(
                text(t("migrationHistory.empty"))
                    .key("migration-run-empty")
                    .font(k.ty.text_small)
                    .color(k.c.text_muted)
                    .padding(16.0)
                    .wrap(),
            );
        }

        let selected = self.items.iter().find(|run| {
            self.selected_id
                .as_deref()
                .is_some_and(|id| id == run.id.as_str())
        });
        for run in &self.items {
            body = body.child(self.row(run, k));
        }
        if let Some(run) = selected {
            body = body.child(self.detail(run, k));
        }
        body
    }

    /// One `migration-run-${id}` button: status/phase, locale timestamp and the
    /// committed/selected counts.
    fn row(&self, run: &MigrationRunRecord, k: &'static Kit) -> El {
        let mut row_el = row()
            .key(format!("migration-run-{}", run.id))
            .w_full()
            .min_w(0.0)
            .gap(12.0)
            .padding(12.0)
            .center_y()
            .fill(if self.selected_id.as_deref() == Some(run.id.as_str()) {
                k.c.layer_02
            } else {
                k.c.layer_01
            });
        if let Some(on_select) = &self.on_select {
            let id = run.id.clone();
            let action = on_select.clone();
            row_el = row_el.on_press(move |state: &mut S| action(state, id.clone()));
        }
        row_el = row_el.child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(4.0)
                .child(
                    text(format!("{} · {}", run.status, run.phase))
                        .key(format!("migration-run-status-{}", run.id))
                        .font(k.ty.ui_medium)
                        .color(k.c.text_base)
                        .wrap(),
                )
                .child(
                    text(format_last_connected(Some(&run.started_at)))
                        .key(format!("migration-run-time-{}", run.id))
                        .font(k.ty.text_small)
                        .color(k.c.text_muted),
                ),
        );
        row_el.child(
            text(format!("{}/{}", run.committed_count, run.selected_count))
                .key(format!("migration-run-count-{}", run.id))
                .font(k.ty.text_small)
                .color(k.c.text_base),
        )
    }

    /// The selected run's detail card, including the Transfer rollback display
    /// and the Data Sync "unknown" reconciliation affordance.
    fn detail(&self, run: &MigrationRunRecord, k: &'static Kit) -> El {
        let display = rollback_display(run);
        let mut detail = col()
            .key("migration-run-detail")
            .w_full()
            .min_w(0.0)
            .gap(6.0)
            .radius(8.0)
            .border(1.0, k.c.border_base)
            .fill(k.c.layer_02)
            .padding(12.0)
            .child(
                text(format!("{}: {}", t("migrationHistory.outcome"), run.outcome))
                    .key("migration-run-detail-outcome")
                    .font(k.ty.text_small)
                    .color(k.c.text_base)
                    .wrap(),
            )
            .child(
                text(format!(
                    "{}: {} / {} / {}",
                    t("migrationHistory.counts"),
                    run.committed_count,
                    run.failed_count,
                    run.conflict_count
                ))
                .key("migration-run-detail-counts")
                .font(k.ty.text_small)
                .color(k.c.text_base)
                .wrap(),
            );

        detail = detail.child(self.rollback(k, run, &display));

        if let Some(profile) = &run.profile_id {
            detail = detail.child(
                text(format!(
                    "{}: {} @ {}",
                    t("migrationHistory.profile"),
                    profile,
                    run.profile_revision.as_deref().unwrap_or("")
                ))
                .key("migration-run-detail-profile")
                .font(k.ty.text_small)
                .color(k.c.text_base)
                .wrap(),
            );
        }
        if let Some(summary) = &run.error_summary {
            if let Some(banner) = Banner::new("migration-run-detail-error", summary.clone())
                .variant(BannerVariant::Plain)
                .build(k)
            {
                detail = detail.child(banner);
            }
        }

        let can_reconcile = self.operation == MigrationOperation::DataSync
            && run.outcome == "unknown"
            && self.on_reconcile.is_some();
        if can_reconcile {
            detail = detail.child(
                col()
                    .gap(8.0)
                    .pt(12.0)
                    .border_sides([1.0, 0.0, 0.0, 0.0], k.c.border_muted)
                    .child(
                        text(t("migrationHistory.syncUnknownHint"))
                            .key("migration-run-detail-sync-unknown-hint")
                            .font(k.ty.text_small)
                            .color(k.c.text_muted)
                            .wrap(),
                    )
                    .child(self.reconcile_button(k, run)),
            );
        }
        detail
    }

    fn rollback(&self, k: &'static Kit, run: &MigrationRunRecord, display: &RollbackDisplay) -> El {
        let mut wrap = row()
            .key("migration-history-rollback-outcome")
            .w_full()
            .min_w(0.0)
            .gap(6.0)
            .center_y()
            // Rune has no data-* API; `label` mirrors data-outcome.
            .label(run.rollback_outcome.clone());
        wrap = wrap.child(
            text(format!("{}: ", t("migrationHistory.rollback")))
                .font(k.ty.text_small)
                .color(k.c.text_muted),
        );
        wrap = wrap.child(
            text(display.label.clone())
                .key("migration-history-rollback-label")
                .font(if display.localized {
                    k.ty.ui
                } else {
                    k.ty.text_small
                })
                .color(if display.uncertain {
                    k.c.warning.fg
                } else {
                    k.c.text_base
                })
                .wrap(),
        );
        // Hidden marker carries the React warning-class branch observably.
        wrap.child(
            row()
                .key(format!(
                    "migration-history-rollback-attrs-{}-{}",
                    run.rollback_outcome, display.uncertain
                ))
                .size(0.0, 0.0)
                .absolute(0.0, 0.0)
                .a11y_hidden(),
        )
    }

    fn reconcile_button(&self, k: &'static Kit, run: &MigrationRunRecord) -> El {
        let mut button = Button::new(t("migrationHistory.syncReconcile"))
            .variant(ButtonVariant::Contrast)
            .control_size(ControlSize::Small)
            .build(k)
            .key("migration-run-reconcile");
        if let Some(on_reconcile) = &self.on_reconcile {
            let id = run.id.clone();
            let action = on_reconcile.clone();
            button = button.on_press(move |state: &mut S| action(state, id.clone()));
        }
        button
    }
}
