//! Port of `windows/connection/result-workspace/ResultWorkspace.tsx`.
//!
//! React is a controlled shell over one statement result: an error branch
//! (copyable error), an empty branch (`sqlFile.noResults`), or a toolbar that
//! switches between a table view and a chart view of the *same* result. View
//! resolution (`resultWorkspaceHelpers.ts`) decides which view can actually be
//! rendered without changing caller state. This port keeps that shape and
//! routes every interaction through an injected [`ResultWorkspaceAction`] host
//! port — no fake IPC, clipboard write or command.
//!
//! ## Keys
//!
//! React `tid(...)` values are kept verbatim: `result-workspace`,
//! `result-workspace-error`, `result-workspace-empty`,
//! `result-workspace-view-table` and `result-workspace-view-chart`. The table /
//! chart / summary / find-bar areas are honest keyed slots the host fills with
//! the already-ported child views.
//!
//! ## Local stand-ins (removal stage)
//!
//! The VM mirrors the props and `resolveResultWorkspaceView` / chartability are
//! pure ports of the React helpers. No production host mounts this shell yet,
//! so every reusable item carries a per-item `#[allow(dead_code)]` with removal
//! stage "drop when the query result host wires the workspace".

use std::rc::Rc;

use rune::{col, row, text, El};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::i18n::t;
use crate::icons::{ICON_TABLE, ICON_TERMINAL};
use crate::ui::copyable::CopyableError;
use crate::ui::kit::kit;

pub(crate) const ROOT_KEY: &str = "result-workspace";
pub(crate) const ERROR_KEY: &str = "result-workspace-error";
pub(crate) const EMPTY_KEY: &str = "result-workspace-empty";
pub(crate) const VIEW_TABLE_KEY: &str = "result-workspace-view-table";
pub(crate) const VIEW_CHART_KEY: &str = "result-workspace-view-chart";
pub(crate) const TABLE_SLOT_KEY: &str = "result-workspace-table-slot";
pub(crate) const CHART_SLOT_KEY: &str = "result-workspace-chart-slot";
pub(crate) const SUMMARY_SLOT_KEY: &str = "result-workspace-summary-slot";
pub(crate) const FIND_SLOT_KEY: &str = "result-workspace-find-slot";

/// React `ResultWorkspaceView`.
#[allow(dead_code)] // contract mirror; remove when the host owns the view.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ResultWorkspaceView {
    #[default]
    Table,
    Chart,
}

/// React `ResultWorkspaceFallbackReason`.
#[allow(dead_code)] // contract mirror; remove when the shared helper lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FallbackReason {
    EmptyResult,
    NotChartable,
}

/// React `ResultWorkspaceViewResolution`.
#[allow(dead_code)] // pure result; remove when the shared helper lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ViewResolution {
    pub(crate) view: ResultWorkspaceView,
    pub(crate) chart_available: bool,
    pub(crate) fallback_reason: Option<FallbackReason>,
}

/// The result fields the workspace reads (columns carry a data type; rows are
/// sample strings used for numeric inference).
#[allow(dead_code)] // VM mirror; remove when the host owns the result feed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct WorkspaceResultVm {
    pub(crate) columns: Vec<(String, String)>,
    pub(crate) rows: Vec<Vec<String>>,
}

/// React `ResultWorkspaceProps` reduced to the observable state.
#[allow(dead_code)] // VM surface; remove when the host wires the workspace.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ResultWorkspaceVm {
    pub(crate) result: Option<WorkspaceResultVm>,
    pub(crate) view: ResultWorkspaceView,
    /// Whether a saved chart config exists (ChartView can initialize without).
    pub(crate) has_chart_config: bool,
    pub(crate) error: String,
    pub(crate) row_detail_index: Option<usize>,
}

/// Field types React infers for chartability. Only numeric matters here.
#[allow(dead_code)] // pure helper; remove when fieldInference lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InferredType {
    Numeric,
    Other,
}

fn contains_ignore_ascii(haystack: &str, needles: &[&str]) -> bool {
    let lower = haystack.to_ascii_lowercase();
    needles.iter().any(|needle| lower.contains(needle))
}

fn numeric_like(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty() && trimmed.parse::<f64>().is_ok()
}

/// React `inferFieldType` reduced to the numeric decision.
fn infer_numeric(data_type: &str, samples: &[String]) -> InferredType {
    if contains_ignore_ascii(data_type, &["bool"]) {
        return InferredType::Other;
    }
    if contains_ignore_ascii(
        data_type,
        &["int", "serial", "double", "numeric", "decimal", "real", "float", "money", "bigint", "smallint"],
    ) {
        return InferredType::Numeric;
    }
    if contains_ignore_ascii(data_type, &["timestamp", "date", "time"]) {
        return InferredType::Other;
    }
    if !samples.is_empty() && samples.iter().all(|value| numeric_like(value)) {
        return InferredType::Numeric;
    }
    InferredType::Other
}

/// React `isChartableResult`.
#[allow(dead_code)] // pure helper; remove when fieldInference lands.
pub(crate) fn is_chartable_result(result: Option<&WorkspaceResultVm>) -> bool {
    let Some(result) = result else {
        return false;
    };
    if result.rows.is_empty() || result.columns.is_empty() {
        return false;
    }
    result.columns.iter().enumerate().any(|(index, (_name, data_type))| {
        let sample_size = result.rows.len().min(50);
        let samples: Vec<String> = result
            .rows
            .iter()
            .take(sample_size)
            .filter_map(|row| row.get(index).cloned())
            .collect();
        infer_numeric(data_type, &samples) == InferredType::Numeric
    })
}

/// React `canRenderResultChart`.
#[allow(dead_code)] // pure helper; remove when the shared helper lands.
pub(crate) fn can_render_result_chart(result: Option<&WorkspaceResultVm>) -> bool {
    is_chartable_result(result)
}

/// React `resolveResultWorkspaceView`, byte-for-byte logic.
#[allow(dead_code)] // pure helper; remove when the shared helper lands.
pub(crate) fn resolve_view(vm: &ResultWorkspaceVm) -> ViewResolution {
    let chart_available = can_render_result_chart(vm.result.as_ref());
    if vm.view == ResultWorkspaceView::Table {
        return ViewResolution {
            view: ResultWorkspaceView::Table,
            chart_available,
            fallback_reason: None,
        };
    }
    let empty = vm
        .result
        .as_ref()
        .map_or(true, |result| result.rows.is_empty() || result.columns.is_empty());
    if empty {
        return ViewResolution {
            view: ResultWorkspaceView::Table,
            chart_available,
            fallback_reason: Some(FallbackReason::EmptyResult),
        };
    }
    if !chart_available {
        return ViewResolution {
            view: ResultWorkspaceView::Table,
            chart_available,
            fallback_reason: Some(FallbackReason::NotChartable),
        };
    }
    ViewResolution {
        view: ResultWorkspaceView::Chart,
        chart_available,
        fallback_reason: None,
    }
}

/// React's chart data-point handler: switch to table *before* forwarding the
/// row. The host applies these two actions in this exact order.
#[allow(dead_code)] // pure helper; remove when the host wires the chart slot.
pub(crate) fn chart_data_point_actions(row: usize) -> [ResultWorkspaceAction; 2] {
    [
        ResultWorkspaceAction::SetView(ResultWorkspaceView::Table),
        ResultWorkspaceAction::RowDetail(row),
    ]
}

/// The interactions the shell emits.
#[allow(dead_code)] // host port; remove when the host wires the workspace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ResultWorkspaceAction {
    SetView(ResultWorkspaceView),
    RowDetail(usize),
    ChartConfigChange,
    /// Switch to table then forward the row (React's chart data-point order).
    ChartDataPoint(usize),
}

#[allow(dead_code)] // host port; remove when the host wires the workspace.
pub(crate) type ResultWorkspaceActionFn<S> = Rc<dyn Fn(&mut S, ResultWorkspaceAction) + 'static>;

fn view_button<S: 'static>(
    key: &str,
    label: &str,
    active: bool,
    disabled: bool,
    help: Option<&str>,
    on_action: &Option<ResultWorkspaceActionFn<S>>,
    view: ResultWorkspaceView,
    dark: bool,
) -> El {
    let mut button = Button::new(label.to_owned())
        .variant(if active {
            ButtonVariant::Neutral
        } else {
            ButtonVariant::Ghost
        })
        .control_size(ControlSize::Small)
        .icon(if view == ResultWorkspaceView::Table {
            &ICON_TABLE
        } else {
            &ICON_TERMINAL
        })
        .disabled(disabled)
        .build(kit(dark))
        .key(key.to_owned())
        .label(label.to_owned())
        .checked(active);
    if let Some(help) = help {
        button = button.help(help.to_owned());
    }
    if !disabled {
        if let Some(callback) = on_action.clone() {
            button = button.on_press(move |s: &mut S| callback(s, ResultWorkspaceAction::SetView(view)));
        }
    }
    button
}

/// Build the workspace into `layer`.
#[allow(dead_code)] // view entry; remove when the host wires the workspace.
pub(crate) fn build<S: 'static>(
    vm: &ResultWorkspaceVm,
    on_action: &Option<ResultWorkspaceActionFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) {
    let k = kit(dark);
    if !vm.error.is_empty() {
        layer.push(
            CopyableError::new(ERROR_KEY, vm.error.clone())
                .copy_button(true)
                .build::<S>(k)
                .w_full()
                .h_full()
                .center()
                .padding(16.0),
        );
        return;
    }
    if vm.result.is_none() {
        layer.push(
            col()
                .key(EMPTY_KEY)
                .role("status")
                .w_full()
                .h_full()
                .center()
                .child(
                    text(t("sqlFile.noResults"))
                        .key("result-workspace-empty-text")
                        .font(k.ty.text_small)
                        .color(k.c.text_muted),
                ),
        );
        return;
    }

    let resolution = resolve_view(vm);
    // React: no result or zero rows -> emptyResultTooltip; otherwise the
    // result is non-chartable -> notChartableTooltip.
    let chart_help = if !resolution.chart_available {
        let rows_empty = vm.result.as_ref().map_or(true, |result| result.rows.is_empty());
        Some(if rows_empty {
            t("chart.emptyResultTooltip")
        } else {
            t("chart.notChartableTooltip")
        })
    } else {
        None
    };
    let toolbar = row()
        .key("result-workspace-toolbar")
        .w_full()
        .h(36.0)
        .min_w(0.0)
        .center_y()
        .px(8.0)
        .gap(4.0)
        .child(view_button::<S>(
            VIEW_TABLE_KEY,
            t("chart.viewTable"),
            resolution.view == ResultWorkspaceView::Table,
            false,
            None,
            on_action,
            ResultWorkspaceView::Table,
            dark,
        ))
        .child(view_button::<S>(
            VIEW_CHART_KEY,
            t("chart.viewChart"),
            resolution.view == ResultWorkspaceView::Chart,
            !resolution.chart_available,
            chart_help,
            on_action,
            ResultWorkspaceView::Chart,
            dark,
        ));

    let content = if resolution.view == ResultWorkspaceView::Table {
        col()
            .key(TABLE_SLOT_KEY)
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .min_w(0.0)
    } else {
        col()
            .key(CHART_SLOT_KEY)
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .min_w(0.0)
    };
    let root = col()
        .key(ROOT_KEY)
        .w_full()
        .h_full()
        .min_h(0.0)
        .min_w(0.0)
        .child(toolbar)
        .child(content)
        .child(row().key(SUMMARY_SLOT_KEY).h(0.0).a11y_hidden())
        .child(row().key(FIND_SLOT_KEY).h(0.0).a11y_hidden());
    layer.push(root);
}
