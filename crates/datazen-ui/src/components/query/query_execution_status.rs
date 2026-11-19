//! T134: query execution status line — port of
//! `packages/frontend/src/components/query/QueryExecutionStatus.tsx`.
//!
//! The small polite-status strip under the SQL editor: what the running
//! statement is doing, for how long, and how many rows it touched, plus the
//! stop button and the honest note when the driver cannot stop a query.
//!
//! The component renders a [`QueryExecutionStatusVm`] — the view-model slice
//! React's `lib/queryExecutionViewModel.ts` derives. That reducer belongs to
//! the `stores/queryExecActions` area (its own later task); this port carries
//! only the fields the strip paints, so the state machine can land without
//! touching the view.
//!
//! ## Keys
//!
//! | part | key |
//! |------|-----|
//! | root | `query-execution-status` (React's `tid`) |
//! | spinner | `query-execution-status-spinner` |
//! | phase label | `query-execution-status-phase` |
//! | elapsed | `query-execution-status-elapsed` |
//! | row count | `query-execution-status-rows` |
//! | affected rows | `query-execution-status-affected` |
//! | stop button | `editor-stop-button` (React's `tid`) |
//! | stop icon | `editor-stop-button-icon` |
//! | stop label | `label` |
//! | capability note | `query-execution-status-cancel-note` |
//!
//! ## Deliberate deviations from the React source
//!
//! * `aria-live="polite"` has no rune equivalent yet (`el/a11y.rs` has no
//!   live-region API); the root keeps React's `role="status"`, which the
//!   access tree does expose. Tracked as an upstream gap, like `aria-controls`.
//! * The stop button is hand-built from kit tokens in a `role="button"` row,
//!   the same way `DataTableSelectionBar`'s delete button is: kit
//!   `ButtonVariant::Danger` paints a filled surface while React wants plain
//!   `text-danger` ink with a danger-tinted hover wash, focus ring, tooltip
//!   (`title`) and keyboard activation.
//! * `lucide-react`'s `Square` maps to the registry's `ICON_SQUARE`
//!   (12pt, React's `h-3 w-3`).
//! * React interpolates JS numbers with no grouping (`{rowCount} 行`), so the
//!   counts render as plain numbers.

use std::rc::Rc;

use rune::{icon, row, text, El};

use crate::i18n::t;
use crate::icons::ICON_SQUARE;
use crate::ui::kit::kit;
use crate::ui::spinner::{Spinner, SpinnerSize};

/// React's `QueryPhase` union (the render slice; the transitions live with
/// the `queryExecActions` state machine).
#[allow(dead_code)] // wired with the editor host task that owns executions
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum QueryPhase {
    #[default]
    Idle,
    Running,
    CancelRequested,
    Succeeded,
    Failed,
    Cancelled,
    OutcomeUnknown,
}

#[allow(dead_code)] // the full state machine lands with queryExecActions; the strip follows
impl QueryPhase {
    /// React's `active`: a live run, winding down or not.
    fn is_active(self) -> bool {
        matches!(self, Self::Running | Self::CancelRequested)
    }

    /// The i18n key of the phase label (React's nested ternary, in order).
    fn label_key(self) -> &'static str {
        match self {
            Self::CancelRequested => "query.cancelling",
            Self::Running => "query.running",
            Self::Cancelled => "query.cancelled",
            Self::Failed | Self::OutcomeUnknown => "common.failed",
            Self::Succeeded | Self::Idle => "common.success",
        }
    }
}

/// React's `CancelCapability` union.
#[allow(dead_code)] // wired with the editor host task
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum CancelCapability {
    Supported,
    Unsupported,
    #[default]
    Unknown,
}

/// React's `CancelActionState` union (the user-action state of the Cancel
/// control).
#[allow(dead_code)] // wired with the editor host task
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum CancelActionState {
    Available,
    Requested,
    #[default]
    Unavailable,
    Failed,
}

/// The view-model slice the strip paints (React's
/// `QueryExecutionViewModel`, render fields only: `error` and `executionId`
/// never appear here).
#[allow(dead_code)] // wired with the editor host task
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct QueryExecutionStatusVm {
    pub phase: QueryPhase,
    pub cancel_capability: CancelCapability,
    pub cancel_state: CancelActionState,
    pub elapsed_ms: Option<u64>,
    pub row_count: Option<u64>,
    pub affected_rows: Option<u64>,
}

/// The status strip. Stateless: the host derives the VM and owns the cancel
/// path; `on_cancel` fires only while the button is enabled.
pub(crate) struct QueryExecutionStatus<S: 'static> {
    vm: QueryExecutionStatusVm,
    show_cancel: bool,
    on_cancel: Option<Rc<dyn Fn(&mut S) + 'static>>,
}

impl<S: 'static> Default for QueryExecutionStatus<S> {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(dead_code)] // reachable only through `build`
impl<S: 'static> QueryExecutionStatus<S> {
    #[allow(dead_code)] // wired with the editor host task
    pub(crate) fn new() -> Self {
        Self {
            vm: QueryExecutionStatusVm::default(),
            show_cancel: true,
            on_cancel: None,
        }
    }

    /// The view-model slice to render.
    #[allow(dead_code)] // wired with the editor host task
    pub(crate) fn vm(mut self, vm: QueryExecutionStatusVm) -> Self {
        self.vm = vm;
        self
    }

    /// React's `showCancel` prop (default true).
    #[allow(dead_code)] // wired with the editor host task
    pub(crate) fn show_cancel(mut self, show: bool) -> Self {
        self.show_cancel = show;
        self
    }

    /// React's `onCancel` prop.
    #[allow(dead_code)] // wired with the editor host task
    pub(crate) fn on_cancel(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_cancel = Some(Rc::new(f));
        self
    }

    /// The strip, or `None` when React renders `null` (the `idle` phase).
    pub(crate) fn build(self, dark: bool) -> Option<El> {
        if self.vm.phase == QueryPhase::Idle {
            return None;
        }
        let k = kit(dark);
        let c = k.c;
        let vm = &self.vm;
        let active = vm.phase.is_active();
        // React's `canCancel`: an active run whose cancel is still available.
        let can_cancel = active && vm.cancel_state == CancelActionState::Available;

        let mut strip = row()
            .key("query-execution-status")
            .role("status")
            .gap(8.0)
            .items_center()
            // React's `shrink-0`: the strip never gives way to its siblings.
            .shrink();

        if active {
            strip = strip.child(
                Spinner::new("query-execution-status-spinner")
                    .size(SpinnerSize::Sm)
                    .build(dark),
            );
        }

        strip = strip.child(
            text(t(vm.phase.label_key()))
                .key("query-execution-status-phase")
                .font(k.ty.small)
                .color(c.text_muted),
        );

        if let Some(elapsed) = vm.elapsed_ms {
            strip = strip.child(
                text(format!("{elapsed} ms"))
                    .key("query-execution-status-elapsed")
                    .font(k.ty.small)
                    .color(c.text_muted),
            );
        }

        if let Some(rows) = vm.row_count {
            strip = strip.child(
                text(format!("{rows} {}", t("common.rows")))
                    .key("query-execution-status-rows")
                    .font(k.ty.small)
                    .color(c.text_muted),
            );
        }

        if let Some(affected) = vm.affected_rows {
            strip = strip.child(
                text(format!("{affected} {}", t("query.affectedRows")))
                    .key("query-execution-status-affected")
                    .font(k.ty.small)
                    .color(c.text_muted),
            );
        }

        if active && self.show_cancel {
            strip = strip.child(self.stop_button(&k, can_cancel));
        }

        // The honest note while a run is going and the driver cannot stop it.
        if active && vm.cancel_capability != CancelCapability::Supported {
            let note = match vm.cancel_capability {
                CancelCapability::Unknown => t("query.cancelUnknown"),
                _ => t("query.cancelUnavailable"),
            };
            strip = strip.child(
                text(note)
                    .key("query-execution-status-cancel-note")
                    .font(k.ty.small)
                    .color(c.text_muted)
                    .shrink(),
            );
        }

        Some(strip)
    }

    /// React's stop button: a small danger-ink action that cancels the run.
    fn stop_button(&self, k: &rune_kit::Kit, can_cancel: bool) -> El {
        let c = k.c;
        let vm = &self.vm;
        // React's `title`: the reason the button is inert, when the driver
        // spells one out (the label/help pair is the rune tooltip).
        let tooltip: Option<&'static str> = match vm.cancel_capability {
            CancelCapability::Unsupported => Some(t("query.cancelUnavailable")),
            CancelCapability::Unknown => Some(t("query.cancelUnknown")),
            CancelCapability::Supported => None,
        };
        let label = if vm.phase == QueryPhase::CancelRequested {
            t("query.cancelling")
        } else {
            t("query.stop")
        };

        let mut button = row()
            .key("editor-stop-button")
            .role("button")
            .gap(4.0)
            .px(6.0)
            .py(2.0)
            .radius(k.radius.sm)
            .items_center()
            // React's `disabled:opacity-50` on an inert control.
            .opacity(if can_cancel { 1.0 } else { 0.5 })
            // React's `hover:bg-danger/10`.
            .hover(k.alpha(c.danger.bg, 0.1));
        if let Some(tooltip) = tooltip {
            button = button.label(tooltip).help(tooltip);
        }
        if can_cancel {
            // A disabled control is not focusable and has no press path.
            button = button
                .focusable(true)
                .focus_ring(2.0, 2.0, c.border_focus);
            if let Some(on_cancel) = &self.on_cancel {
                let on_cancel = Rc::clone(on_cancel);
                button = button.on_press(move |s: &mut S| on_cancel(s));
            }
        }

        button
            .child(
                icon(&ICON_SQUARE, 12.0)
                    .key("editor-stop-button-icon")
                    .color(c.danger.fg),
            )
            .child(
                text(label)
                    .key("label")
                    .font(k.ty.small)
                    .color(c.danger.fg),
            )
    }
}
