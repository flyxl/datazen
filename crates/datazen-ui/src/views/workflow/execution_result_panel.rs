//! Port of `packages/frontend/src/windows/workflow/WorkflowExecutionResultPanel.tsx`.
//!
//! React renders one bordered, rounded container for a finished workflow
//! run: a tinted head row (`✓ t('workflows.result')` green or
//! `✗ t('common.executionFailed')` red, `totalTimeMs` on the right), one
//! expandable row per step, an optional workflow-level error line and an
//! optional final-output block.
//!
//! A step row is a full-width toggle: chevron, status mark, the step id
//! (truncated), `[{stepType}]`, the connection name in accent small text and
//! the step's `executionTimeMs` at the right edge. The expanded body stacks
//! the executed SQL, the step error, a data preview and the AI step's result
//! text.
//!
//! The data preview shows the row count, a table/chart segmented toggle
//! (present only when the result is chartable) and either the chart frame or
//! a table preview (column names, the first 20 rows, a `... N more` row).
//!
//! # Immediate-mode state
//!
//! React keeps three `useState`s per row (expanded, viewMode, chartConfig).
//! Rune stores the two boolean ones in the host's state `S` as sets keyed by
//! step id ([`StepUiSlots`]); the chart config belongs to `ChartView` and
//! waits for T104 with it.
//!
//! # Deliberate boundaries
//!
//! - **Chart mode**: React embeds `ChartView` (T104, not ported). The ported
//!   facilities (T008's `ChartCanvas`) cannot faithfully replicate it —
//!   `ChartView`'s field inference and default chart config are exactly the
//!   unported part — so the chart frame keeps React's geometry (260pt tall,
//!   bordered, with the >1000-row sampling warning) and says the chart view
//!   is not ported instead of drawing a phantom chart. T104 落地后接通（届时
//!   以 `step_to_statement_preview` 的产物喂 `ChartView`）.
//! - **⏭ / ⏱ status marks**: React draws them as text glyphs; the app font
//!   pool (Inter, Noto Sans SC, IBM Plex Mono) has no cmap entry for either,
//!   so they render as their lucide counterparts (`skip-forward`, `clock`)
//!   from the icon registry. ✓/✗ stay text (every pool font shapes them).
//! - **Table/chart toggle**: React renders the segmented control only when
//!   the result is chartable, so its `disabled={!chartable}` branch and
//!   `notChartableTooltip` title are unreachable in the React source itself;
//!   the port reproduces the reachable behaviour (no toggle when not
//!   chartable) and keeps the tooltip key unused, as in React.
//!
//! ## Keys (prefix `workflow-execution-result`)
//!
//! | part | key |
//! |------|-----|
//! | root / head / status / total time | `<prefix>`, `<prefix>-head`, `<prefix>-status`, `<prefix>-total-time` |
//! | workflow error | `<prefix>-error`, `<prefix>-error-text` |
//! | final output | `<prefix>-final-output`, `<prefix>-final-output-label`, `<prefix>-final-output-text` |
//! | step wrapper / row | `<prefix>-step-<i>`, `<prefix>-step-<i>-row` |
//! | chevron lane / glyph | `<prefix>-step-<i>-chevron`, `<prefix>-step-<i>-glyph-down` / `-glyph-right` |
//! | status mark | `<prefix>-step-<i>-status` (text) / `-status-skip` / `-status-clock` (icons) |
//! | id / type / connection / time | `<prefix>-step-<i>-id`, `-type`, `-connection`, `-time` |
//! | expanded body | `<prefix>-step-<i>-body` |
//! | SQL | `<prefix>-step-<i>-sql`, `-sql-label`, `-sql-text` |
//! | step error | `<prefix>-step-<i>-error` |
//! | data / row count | `<prefix>-step-<i>-data`, `-rows-count`, `-data-body` |
//! | view toggle | `<prefix>-step-<i>-view-toggle`, `-view-table`, `-view-chart` |
//! | chart frame | `<prefix>-step-<i>-chart`, `-sampled-warning`, `-chart-pending` |
//! | table preview | `<prefix>-step-<i>-table`, `-table-head`, `-table-col-<j>`, `-table-row-<r>`, `-table-cell-<r>-<j>`, `-table-more` |
//! | AI result | `<prefix>-step-<i>-ai-result` |

use std::collections::BTreeSet;

use rune::{col, el, icon, row, text, El, ScrollSpec, TextStyle};
use rune_kit::Kit;

use super::step_result_utils::{
    is_chartable_preview, js_string, step_to_statement_preview, StatementPreview,
    StepExecutionResult, WorkflowExecutionResult, WorkflowStepStatus,
};
use crate::i18n::t;
use crate::icons::{
    ICON_ALERT_TRIANGLE, ICON_BAR_CHART_3, ICON_CHEVRON_DOWN, ICON_CHEVRON_RIGHT, ICON_CLOCK,
    ICON_SKIP_FORWARD, ICON_TABLE,
};
use crate::ui::kit::kit;

/// The chart frame's fixed height (React `style={{ height: 260 }}`).
const CHART_HEIGHT: f32 = 260.0;
/// The chart's sampling threshold (React `rows.length > 1000`).
const SAMPLE_LIMIT: usize = 1000;
/// How many preview rows the table shows (React `rows.slice(0, 20)`).
const PREVIEW_ROWS: usize = 20;

/// Where the panel's per-step interaction state lives in `S`: the step ids
/// whose rows are expanded, and the step ids currently showing the chart
/// view (React's `viewMode === 'chart'`). Function pointers, so the host
/// keeps the state and the panel stays stateless.
pub(crate) struct StepUiSlots<S: 'static> {
    pub(crate) expanded: fn(&S) -> &BTreeSet<String>,
    pub(crate) toggle: fn(&mut S, String),
    pub(crate) chart: fn(&S) -> &BTreeSet<String>,
    pub(crate) set_chart: fn(&mut S, String, bool),
}

/// The execution-result panel. `result` is the run to show; `slots` locates
/// the interaction state in the app's `S`.
pub(crate) struct WorkflowExecutionResultPanel<'a, S: 'static> {
    key: String,
    result: &'a WorkflowExecutionResult,
    slots: StepUiSlots<S>,
}

impl<'a, S: 'static> WorkflowExecutionResultPanel<'a, S> {
    pub(crate) fn new(result: &'a WorkflowExecutionResult, slots: StepUiSlots<S>) -> Self {
        Self {
            key: "workflow-execution-result".to_owned(),
            result,
            slots,
        }
    }

    /// Builds the panel for one appearance.
    pub(crate) fn build(self, s: &S, dark: bool) -> El {
        let k = kit(dark);
        let ctx = RenderCtx {
            k,
            prefix: &self.key,
            f10: TextStyle {
                size: 10.0,
                line_height: 13.0,
                ..k.ty.text_xsmall
            },
            f11: TextStyle {
                size: 11.0,
                line_height: 14.0,
                ..k.ty.text_xsmall
            },
            mono11: TextStyle {
                size: 11.0,
                line_height: 14.0,
                ..k.ty.mono
            },
        };
        let result = self.result;
        let last = result.steps.len().saturating_sub(1);
        let mut panel = col()
            .key(self.key.clone())
            .w_full()
            .shrink()
            .radius(k.radius.md)
            .border(1.0, k.c.border_base)
            .fill(k.c.bg_base)
            .clip_soft(0.0)
            .child(head(result, &ctx));
        for (index, step) in result.steps.iter().enumerate() {
            panel = panel.child(step_row(step, index, index == last, s, &self.slots, &ctx));
        }
        if let Some(error) = &result.error {
            panel = panel.child(workflow_error(error, &ctx));
        }
        // React's `result.finalOutput && …`: an empty final output renders
        // nothing (the lite projection never fills it).
        if !result.final_output.is_empty() {
            panel = panel.child(final_output(&result.final_output, &ctx));
        }
        panel
    }
}

/// Immutable render inputs shared by the row builders.
struct RenderCtx<'a> {
    k: &'a Kit,
    prefix: &'a str,
    f10: TextStyle,
    f11: TextStyle,
    mono11: TextStyle,
}

impl RenderCtx<'_> {
    fn base(&self, suffix: &str) -> String {
        format!("{}{suffix}", self.prefix)
    }
    fn step(&self, index: usize, suffix: &str) -> String {
        format!("{}-step-{index}{suffix}", self.prefix)
    }
}

/// The tinted head row: status sentence left, total time right.
fn head(result: &WorkflowExecutionResult, ctx: &RenderCtx) -> El {
    let k = ctx.k;
    let (mark, label, ink, tint) = if result.success {
        (
            "✓",
            t("workflows.result"),
            k.c.success.fg,
            k.alpha(k.c.success.fg, 0.05),
        )
    } else {
        (
            "✗",
            t("common.executionFailed"),
            k.c.danger.fg,
            k.alpha(k.c.danger.fg, 0.05),
        )
    };
    row()
        .key(ctx.base("-head"))
        .w_full()
        .shrink()
        .px(12.0)
        .py(8.0)
        .items_center()
        .justify_between()
        .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
        .fill(tint)
        .child(
            text(format!("{mark} {label}"))
                .key(ctx.base("-status"))
                .font(k.ty.text_xsmall_medium)
                .color(ink)
                .shrink(),
        )
        .child(
            text(format!("{}ms", result.total_time_ms))
                .key(ctx.base("-total-time"))
                .font(k.ty.text_xsmall)
                .color(k.c.text_faint)
                .shrink(),
        )
}

/// The workflow-level error line (React `select-text … bg-red-500/5 border-t`).
fn workflow_error(error: &str, ctx: &RenderCtx) -> El {
    let k = ctx.k;
    row()
        .key(ctx.base("-error"))
        .w_full()
        .shrink()
        .px(12.0)
        .py(8.0)
        .border_sides([1.0, 0.0, 0.0, 0.0], k.c.border_base)
        .fill(k.alpha(k.c.danger.fg, 0.05))
        .child(
            text(error.to_owned())
                .key(ctx.base("-error-text"))
                .font(k.ty.text_xsmall)
                .color(k.c.danger.fg)
                .w_full()
                .min_w(0.0)
                .wrap_anywhere()
                .selectable(true),
        )
}

/// The final-output block (React `px-3 py-2 border-t` + label + pre).
fn final_output(value: &str, ctx: &RenderCtx) -> El {
    let k = ctx.k;
    col()
        .key(ctx.base("-final-output"))
        .w_full()
        .shrink()
        .px(12.0)
        .py(8.0)
        .gap(4.0)
        .border_sides([1.0, 0.0, 0.0, 0.0], k.c.border_base)
        .child(
            text(t("workflows.finalOutput").to_owned())
                .key(ctx.base("-final-output-label"))
                .font(ctx.f10)
                .color(k.c.text_faint),
        )
        .child(
            text(value.to_owned())
                .key(ctx.base("-final-output-text"))
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted)
                .w_full()
                .min_w(0.0)
                .max_h(160.0)
                .wrap_anywhere()
                .selectable(true),
        )
}

/// One step: the always-toggleable row plus its expanded body.
fn step_row<S: 'static>(
    step: &StepExecutionResult,
    index: usize,
    last: bool,
    s: &S,
    slots: &StepUiSlots<S>,
    ctx: &RenderCtx,
) -> El {
    let k = ctx.k;
    let expanded = (slots.expanded)(s).contains(&step.step_id);
    let (glyph, glyph_key) = if expanded {
        (&ICON_CHEVRON_DOWN, ctx.step(index, "-glyph-down"))
    } else {
        (&ICON_CHEVRON_RIGHT, ctx.step(index, "-glyph-right"))
    };
    // The status mark: text ✓/✗ where the font pool shapes it, lucide
    // skip-forward / clock where it cannot (see the module docs). The text
    // marks inherit the row's `text-xs` (the kit's text_xsmall), matching
    // the 12pt icons beside them.
    let status = match step.status {
        WorkflowStepStatus::Success => text("✓".to_owned())
            .key(ctx.step(index, "-status"))
            .font(k.ty.text_xsmall)
            .color(k.c.success.fg)
            .shrink(),
        WorkflowStepStatus::Failed => text("✗".to_owned())
            .key(ctx.step(index, "-status"))
            .font(k.ty.text_xsmall)
            .color(k.c.danger.fg)
            .shrink(),
        WorkflowStepStatus::Skipped => icon(&ICON_SKIP_FORWARD, 12.0)
            .key(ctx.step(index, "-status-skip"))
            .color(k.c.warning.fg)
            .shrink(),
        WorkflowStepStatus::TimedOut => icon(&ICON_CLOCK, 12.0)
            .key(ctx.step(index, "-status-clock"))
            .color(k.c.danger.fg)
            .shrink(),
    };

    let mut row_el = row()
        .key(ctx.step(index, "-row"))
        .w_full()
        .shrink()
        .px(12.0)
        .py(6.0)
        .items_center()
        .gap(8.0)
        .hover(k.alpha(k.c.layer_02, 0.5))
        .focusable(true)
        .expanded(expanded)
        .child(
            el()
                .key(ctx.step(index, "-chevron"))
                .size(12.0, 12.0)
                .shrink()
                .child(icon(glyph, 12.0).key(glyph_key).color(k.c.text_faint)),
        )
        .child(status)
        .child(
            text(step.step_id.clone())
                .key(ctx.step(index, "-id"))
                .font(k.ty.text_xsmall_medium)
                .color(k.c.text_base)
                .shrink()
                .min_w(0.0)
                .truncate(),
        );
    if let Some(step_type) = &step.step_type {
        row_el = row_el.child(
            text(format!("[{step_type}]"))
                .key(ctx.step(index, "-type"))
                .font(k.ty.text_xsmall)
                .color(k.c.text_faint)
                .shrink(),
        );
    }
    if let Some(connection) = &step.connection_name {
        row_el = row_el.child(
            text(connection.clone())
                .key(ctx.step(index, "-connection"))
                .font(ctx.f10)
                .color(k.c.text_accent)
                .shrink(),
        );
    }
    // `ml-auto shrink-0`: a growing spacer pushes the time to the edge.
    let step_id = step.step_id.clone();
    let toggle = slots.toggle;
    row_el = row_el
        .child(el().key(ctx.step(index, "-spacer")).grow(1.0))
        .child(
            text(format!("{}ms", step.execution_time_ms))
                .key(ctx.step(index, "-time"))
                .font(k.ty.text_xsmall)
                .color(k.c.text_faint)
                .shrink(),
        )
        .on_press(move |s: &mut S| toggle(s, step_id.clone()));

    let mut wrapper = col()
        .key(ctx.step(index, ""))
        .w_full()
        .shrink()
        .border_sides(
            if last {
                [0.0, 0.0, 0.0, 0.0]
            } else {
                [0.0, 0.0, 1.0, 0.0]
            },
            k.c.border_base,
        )
        .child(row_el);
    if expanded {
        wrapper = wrapper.child(step_body(step, index, s, slots, ctx));
    }
    wrapper
}

/// The expanded body: SQL, error, data preview, AI result (`space-y-1`).
fn step_body<S: 'static>(
    step: &StepExecutionResult,
    index: usize,
    s: &S,
    slots: &StepUiSlots<S>,
    ctx: &RenderCtx,
) -> El {
    let k = ctx.k;
    let mut body = col()
        .key(ctx.step(index, "-body"))
        .w_full()
        .shrink()
        .px(12.0)
        .pb(8.0)
        .gap(4.0);

    if let Some(sql) = &step.sql_executed {
        body = body.child(
            col()
                .key(ctx.step(index, "-sql"))
                .w_full()
                .shrink()
                .gap(4.0)
                .child(
                    text("SQL".to_owned())
                        .key(ctx.step(index, "-sql-label"))
                        .font(ctx.f10)
                        .color(k.c.text_faint),
                )
                .child(
                    text(sql.clone())
                        .key(ctx.step(index, "-sql-text"))
                        .font(ctx.mono11)
                        .color(k.c.text_muted)
                        .w_full()
                        .min_w(0.0)
                        .max_h(96.0)
                        .wrap_anywhere()
                        .radius(k.radius.sm)
                        .fill(k.alpha(k.c.layer_01, 0.5))
                        .padding(6.0)
                        .selectable(true),
                ),
        );
    }

    if let Some(error) = &step.error {
        body = body.child(
            text(error.clone())
                .key(ctx.step(index, "-error"))
                .font(ctx.f11)
                .color(k.c.danger.fg)
                .w_full()
                .min_w(0.0)
                .wrap_anywhere()
                .selectable(true),
        );
    }

    if let Some(preview) = step_to_statement_preview(step) {
        if !preview.columns.is_empty() && !preview.rows.is_empty() {
            body = body.child(data_section(step, index, &preview, s, slots, ctx));
        }
    }

    // React: `step.stepType === 'ai' && step.result?.result != null`.
    if step.step_type.as_deref() == Some("ai") {
        if let Some(ai) = step
            .result
            .as_ref()
            .and_then(|result| result.get("result"))
            .filter(|value| !value.is_null())
        {
            body = body.child(
                text(js_string(ai))
                    .key(ctx.step(index, "-ai-result"))
                    .font(ctx.f11)
                    .color(k.c.text_muted)
                    .w_full()
                    .min_w(0.0)
                    .max_h(160.0)
                    .wrap_anywhere()
                    .selectable(true),
            );
        }
    }

    body
}

/// The data preview: row count, the table/chart toggle (chartable only) and
/// the active view.
fn data_section<S: 'static>(
    step: &StepExecutionResult,
    index: usize,
    preview: &StatementPreview,
    s: &S,
    slots: &StepUiSlots<S>,
    ctx: &RenderCtx,
) -> El {
    let k = ctx.k;
    let chartable = is_chartable_preview(preview);
    let chart_mode = (slots.chart)(s).contains(&step.step_id);

    let mut head = row()
        .key(ctx.step(index, "-data"))
        .w_full()
        .shrink()
        .items_center()
        .gap(8.0)
        .child(
            text(format!("{} row(s)", preview.rows_count))
                .key(ctx.step(index, "-rows-count"))
                .font(ctx.f10)
                .color(k.c.text_faint)
                .shrink(),
        );
    if chartable {
        head = head
            .child(el().key(ctx.step(index, "-data-spacer")).grow(1.0))
            .child(view_toggle(step, index, !chart_mode, slots, ctx));
    }
    let view = if chart_mode {
        chart_frame(index, preview, ctx)
    } else {
        table_preview(index, preview, ctx)
    };
    col()
        .key(ctx.step(index, "-data-body"))
        .w_full()
        .shrink()
        .gap(4.0)
        .child(head)
        .child(view)
}

/// The segmented table/chart control. React renders it only when the result
/// is chartable, so the chart button's `disabled` branch never runs in the
/// React source either (see the module docs). The pressed entry sets the
/// chart flag through the host's slot.
fn view_toggle<S: 'static>(
    step: &StepExecutionResult,
    index: usize,
    table_active: bool,
    slots: &StepUiSlots<S>,
    ctx: &RenderCtx,
) -> El {
    let k = ctx.k;
    let set_chart = slots.set_chart;
    let entry = |key: String,
                 label: &str,
                 glyph: &'static rune::Icon,
                 active: bool,
                 chart: bool|
     -> El {
        let ink = if active {
            k.c.text_accent
        } else {
            k.c.text_faint
        };
        let mut entry = row()
            .key(key.clone())
            .shrink()
            .px(6.0)
            .py(2.0)
            .gap(2.0)
            .center_y()
            .radius(k.radius.sm)
            .focusable(true)
            .child(icon(glyph, 10.0).key(format!("{key}-icon")).color(ink))
            .child(
                text(label.to_owned())
                    .key(format!("{key}-label"))
                    .font(ctx.f10)
                    .color(ink),
            );
        if active {
            entry = entry.fill(k.alpha(k.c.bg_accent, 0.2));
        } else {
            entry = entry.hover(k.c.hover);
        }
        let step_id = step.step_id.clone();
        entry.on_press(move |s: &mut S| set_chart(s, step_id.clone(), chart))
    };
    row()
        .key(ctx.step(index, "-view-toggle"))
        .shrink()
        // React `rounded-md` (6pt), outside the 4pt `rounded` entries.
        .radius(k.radius.md)
        .fill(k.c.bg_base)
        .padding(2.0)
        .gap(2.0)
        .child(entry(
            ctx.step(index, "-view-table"),
            t("workflows.viewTable"),
            &ICON_TABLE,
            table_active,
            false,
        ))
        .child(entry(
            ctx.step(index, "-view-chart"),
            t("workflows.viewChart"),
            &ICON_BAR_CHART_3,
            !table_active,
            true,
        ))
}

/// The chart frame: React's fixed 260pt bordered box with the sampling
/// warning over `ChartView`. T104 落地后接通（the placeholder says so until
/// the real chart view exists）.
fn chart_frame(index: usize, preview: &StatementPreview, ctx: &RenderCtx) -> El {
    let k = ctx.k;
    let mut frame = col()
        .key(ctx.step(index, "-chart"))
        .w_full()
        .shrink()
        .mt(4.0)
        .border(1.0, k.c.border_base)
        .radius(k.radius.sm)
        .h(CHART_HEIGHT)
        .clip_soft(0.0);
    if preview.rows.len() > SAMPLE_LIMIT {
        let warning = t("workflows.sampledWarning").replace("{limit}", "1000");
        frame = frame.child(
            row()
                .key(ctx.step(index, "-sampled-warning"))
                .w_full()
                .shrink()
                .px(8.0)
                .py(2.0)
                .gap(4.0)
                .items_center()
                .fill(k.c.layer_01)
                .child(
                    icon(&ICON_ALERT_TRIANGLE, 10.0)
                        .key(ctx.step(index, "-sampled-warning-icon"))
                        .color(k.c.warning.fg),
                )
                .child(
                    text(warning)
                        .key(ctx.step(index, "-sampled-warning-text"))
                        .font(ctx.f10)
                        .color(k.c.warning.fg)
                        .wrap()
                        .min_w(0.0)
                        .shrink(),
                ),
        );
    }
    // ChartView (T104) is not ported and T008's canvas cannot stand in for
    // it (see the module docs): the frame says what is missing.
    frame.child(
        col()
            .key(ctx.step(index, "-chart-pending"))
            .w_full()
            .min_h(0.0)
            .grow(1.0)
            .center()
            .child(
                text(
                    "chart view is not ported yet — T104 will render the statement result here"
                        .to_owned(),
                )
                .key(ctx.step(index, "-chart-pending-text"))
                .font(ctx.f10)
                .color(k.c.text_faint)
                .wrap(),
            ),
    )
}

/// The table preview: header row, the first 20 rows and the `... N more`
/// row. Columns share the row's width equally (the kit table's auto-column
/// behaviour) with truncating cells, so no preview overflows horizontally.
fn table_preview(index: usize, preview: &StatementPreview, ctx: &RenderCtx) -> El {
    let k = ctx.k;
    let cell_width = |e: El| e.w(0.0).grow(1.0).shrink().min_w(0.0);

    let mut head = row()
        .key(ctx.step(index, "-table-head"))
        .w_full()
        .shrink()
        .fill(k.c.layer_01)
        .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base);
    for (j, column) in preview.columns.iter().enumerate() {
        let title = text(column.name.clone())
            .key(ctx.step(index, &format!("-table-col-{j}-title")))
            .font(k.ty.text_xsmall_medium)
            .color(k.c.text_base)
            .shrink()
            .min_w(0.0)
            .truncate();
        head = head.child(cell_width(
            row()
                .key(ctx.step(index, &format!("-table-col-{j}")))
                .role("columnheader")
                .label(column.name.clone())
                .shrink()
                .px(8.0)
                .py(4.0)
                .child(title),
        ));
    }

    let mut table = col()
        .key(ctx.step(index, "-table"))
        .w_full()
        .shrink()
        .mt(2.0)
        .border(1.0, k.c.border_base)
        .radius(k.radius.sm)
        .max_h(160.0)
        .scroll(ScrollSpec { stick_bottom: false })
        .child(head);

    let body_rows = preview.rows.len().min(PREVIEW_ROWS);
    for (r, cells) in preview.rows.iter().take(PREVIEW_ROWS).enumerate() {
        let last_body_row = r + 1 == body_rows;
        let mut row_el = row()
            .key(ctx.step(index, &format!("-table-row-{r}")))
            .w_full()
            .shrink()
            .hover(k.alpha(k.c.layer_02, 0.3))
            .border_sides(
                if last_body_row {
                    [0.0, 0.0, 0.0, 0.0]
                } else {
                    [0.0, 0.0, 1.0, 0.0]
                },
                k.c.border_base,
            );
        for (j, cell) in cells.iter().enumerate() {
            // React: `val == null` paints an italic muted null; anything else
            // is `String(val)`.
            let value = match cell {
                None => {
                    text("null".to_owned())
                        .font(ctx.f11.italic())
                        .color(k.c.text_faint)
                }
                Some(value) => text(js_string(value)).font(ctx.f11).color(k.c.text_muted),
            };
            row_el = row_el.child(cell_width(
                row()
                    .key(ctx.step(index, &format!("-table-cell-{r}-{j}")))
                    .shrink()
                    .px(8.0)
                    .py(2.0)
                    .child(value.truncate().shrink().min_w(0.0)),
            ));
        }
        table = table.child(row_el);
    }

    if preview.rows.len() > PREVIEW_ROWS {
        table = table.child(
            row()
                .key(ctx.step(index, "-table-more"))
                .w_full()
                .shrink()
                .py(2.0)
                .child(
                    text(format!("... {} more", preview.rows.len() - PREVIEW_ROWS))
                        .key(ctx.step(index, "-table-more-text"))
                        .font(ctx.f11)
                        .color(k.c.text_faint)
                        .w_full()
                        .align(rune::Align::Center),
                ),
        );
    }
    table
}
