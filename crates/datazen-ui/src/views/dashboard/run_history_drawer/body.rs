//! The body half of the T175 `RunHistoryDrawer` port
//! (`packages/frontend/src/windows/dashboard/RunHistoryDrawer.tsx`, the
//! `flex min-h-0 flex-1` row and everything inside it): the left run-index
//! column with its three branches (loading / empty / the entry list), the
//! entry rows (started-at line, status line, alert glyph), and the right
//! content column — the chart↔table view switch over the loaded run's
//! mutually exclusive states. The shared state, view models, geometry
//! constants and React testid keys it reads are the parent module's
//! privates.

use std::rc::Rc;

use rune::{col, icon, row, text, Color, El, ScrollSpec, TextStyle};
use rune_kit::Kit;

use crate::components::chart::chart_canvas::ChartCanvas;
use crate::i18n::t;
use crate::icons::ICON_ALERT_TRIANGLE;
use crate::support::formatters::format_last_connected;
use crate::ui::palette;
use crate::ui::spinner::{Spinner, SpinnerSize};
use crate::views::dashboard::chart_widget_tile::{
    has_renderable_chart, table_grid, TileViewMode, WidgetRunStatus,
};

use super::{
    status_color, status_label, type_xs, RunHistoryDrawer, RunIndexEntryVm, ALERT_ICON, ALERT_MT,
    ENTRY_GAP, ENTRY_PX, ENTRY_PY, LIST_PY, LIST_W, NOTE_PX, ROOT_KEY, SPINNER_GAP, TOGGLE_PX,
    TOGGLE_PY, VIEW_PX, VIEW_PY, VIEW_TOGGLE_KEY,
};

impl<'a, S: 'static> RunHistoryDrawer<'a, S> {
    // ---- the body -------------------------------------------------------------

    /// React's `flex min-h-0 flex-1` row: the index list and the content.
    pub(super) fn body_row(&self, k: &Kit, dark: bool) -> El {
        row()
            .key(format!("{ROOT_KEY}-body"))
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .child(self.list_column(k, dark))
            .child(self.content_column(k, dark))
    }

    /// The left column: `flex w-56 shrink-0 flex-col border-r border-edge`,
    /// painting exactly one of its three branches (loading / empty / list).
    fn list_column(&self, k: &Kit, dark: bool) -> El {
        let column = col()
            .key(format!("{ROOT_KEY}-list"))
            .w(LIST_W) // React w-56 (no shrink: rune children don't)
            .border_sides([0.0, 1.0, 0.0, 0.0], k.c.border_base); // border-r
        if self.state.loading_index {
            // React's `flex flex-1 items-center justify-center text-xs
            // text-fg-muted` with the large spinner.
            return column.child(
                row()
                    .key(format!("{ROOT_KEY}-list-loading"))
                    .grow(1.0)
                    .center()
                    .gap(SPINNER_GAP) // React mr-1.5
                    .child(
                        Spinner::new(format!("{ROOT_KEY}-list-loading-spinner"))
                            .size(SpinnerSize::Lg)
                            .build(dark),
                    )
                    .child(
                        text(t("common.loading"))
                            .key(format!("{ROOT_KEY}-list-loading-text"))
                            .font(type_xs())
                            .color(k.c.text_muted),
                    ),
            );
        }
        if self.state.index.is_empty() {
            // React's `flex flex-1 items-center justify-center px-3
            // text-center text-xs text-fg-muted`.
            return column.child(
                col()
                    .key(format!("{ROOT_KEY}-list-empty"))
                    .grow(1.0)
                    .center()
                    .px(ENTRY_PX) // React px-3
                    .child(
                        text(t("dashboard.noHistory"))
                            .key(format!("{ROOT_KEY}-list-empty-text"))
                            .font(type_xs())
                            .color(k.c.text_muted)
                            .wrap(),
                    ),
            );
        }
        // React's `<ul className="overflow-auto py-1">`.
        let mut list = col()
            .key(format!("{ROOT_KEY}-list-scroll"))
            .grow(1.0)
            .min_h(0.0)
            .scroll(ScrollSpec { stick_bottom: false })
            .py(LIST_PY);
        for entry in &self.state.index {
            list = list.child(self.entry_button(k, dark, entry));
        }
        column.child(list)
    }

    /// One index entry: React's `<li key={entry.id}>` button — `flex w-full
    /// items-start gap-1.5 px-3 py-2 text-left text-xs
    /// hover:bg-surface-raised` (plus `bg-surface-raised` when selected) —
    /// with the started-at line, the status line and the alert glyph.
    fn entry_button(&self, k: &Kit, dark: bool, entry: &RunIndexEntryVm) -> El {
        let id = entry.id.as_str();
        let selected = self.state.selected_id.as_deref() == Some(id);
        let time = format_last_connected(Some(&entry.started_at));
        let status = status_label(entry.status);
        // React's accessible name is the button's content.
        let name = format!("{time} {status}");
        let mut button = row()
            .key(format!("{ROOT_KEY}-entry-{id}"))
            .role("button")
            .label(name)
            .w_full()
            .items_start()
            .gap(ENTRY_GAP)
            .px(ENTRY_PX)
            .py(ENTRY_PY)
            .focusable(true)
            .focus_ring(2.0, 2.0, k.c.border_focus);
        if selected {
            button = button.fill(k.c.surface_raised); // bg-surface-raised
        } else {
            // React `hover:bg-surface-raised` (no transition counterpart).
            button = button.hover(k.c.surface_raised);
        }
        button = button.child(
            col()
                .key(format!("{ROOT_KEY}-entry-{id}-lines"))
                .grow(1.0) // React flex-1
                .min_w(0.0) // React min-w-0
                .child(
                    text(time)
                        .key(format!("{ROOT_KEY}-entry-{id}-time"))
                        .font(type_xs())
                        .color(k.c.text_base) // text-fg
                        .truncate(),
                )
                .child(
                    text(status)
                        .key(format!("{ROOT_KEY}-entry-{id}-status"))
                        .font(type_xs())
                        .color(status_color(entry.status, k, dark)),
                ),
        );
        if entry.alert_fired {
            button = button.child(
                icon(&ICON_ALERT_TRIANGLE, ALERT_ICON)
                    .key(format!("{ROOT_KEY}-entry-{id}-alert"))
                    .color(palette::alert_amber(dark)) // text-amber-500
                    .mt(ALERT_MT), // React mt-0.5
            );
        }
        if let Some(on_select) = self.on_select.as_ref() {
            let on_select = Rc::clone(on_select);
            let id = entry.id.clone();
            button = button.on_press(move |s: &mut S| on_select(s, id.clone()));
        }
        button
    }

    // ---- the right column ------------------------------------------------------

    /// The right column: `relative min-h-0 min-w-0 flex-1 flex flex-col` —
    /// the view switch (ok runs only) over the state body.
    fn content_column(&self, k: &Kit, dark: bool) -> El {
        let mut column = col()
            .key(format!("{ROOT_KEY}-content"))
            .grow(1.0)
            .min_w(0.0)
            .min_h(0.0);
        // React's gate is `loadedRun?.status === 'ok'` alone — the switch
        // stays mounted while the next run loads.
        if matches!(self.state.loaded_run.as_ref().map(|run| run.status), Some(WidgetRunStatus::Ok))
        {
            column = column.child(self.view_toggle_row(k));
        }
        column.child(self.content_body(k, dark))
    }

    /// The view switch row: `flex shrink-0 justify-end border-b border-edge
    /// px-3 py-1.5` (`data-testid="run-history-view-toggle"`).
    fn view_toggle_row(&self, k: &Kit) -> El {
        row()
            .key(VIEW_TOGGLE_KEY)
            .w_full()
            .justify_end()
            .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base) // border-b
            .px(TOGGLE_PX)
            .py(TOGGLE_PY)
            .child(
                row()
                    .key(format!("{VIEW_TOGGLE_KEY}-group"))
                    .items_center()
                    .radius(k.radius.sm) // React rounded
                    .border(1.0, k.c.border_base)
                    .child(self.view_button(
                        k,
                        TileViewMode::Chart,
                        "run-history-view-chart",
                        t("dashboard.viewChart"),
                    ))
                    .child(self.view_button(
                        k,
                        TileViewMode::Table,
                        "run-history-view-table",
                        t("dashboard.viewTable"),
                    )),
            )
    }

    /// One view-switch button (the T069 header switch's twin: `px-1.5 py-0.5
    /// text-[10px]`, selected `bg-accent/20 text-accent`, else
    /// `text-fg-muted hover:text-fg`).
    fn view_button(&self, k: &Kit, mode: TileViewMode, key: &str, label: &str) -> El {
        let selected = self.state.view_mode == mode;
        let ink = if selected {
            k.c.text_accent // React text-accent
        } else {
            k.c.text_muted // React text-fg-muted
        };
        let mut label_el = text(label)
            .key(format!("{key}-label"))
            .font(TextStyle::new(10.0, 400.0)) // React text-[10px]
            .color(ink);
        let mut button = row()
            .key(key)
            .role("button")
            .label(label)
            .help(label)
            .items_center()
            .px(VIEW_PX)
            .py(VIEW_PY)
            .focusable(true)
            .focus_ring(2.0, 2.0, k.c.border_focus);
        if selected {
            button = button.fill(k.alpha(k.c.bg_accent, 0.2)); // bg-accent/20
        } else {
            // React `hover:text-fg`: the hover-group tint.
            button = button.hover_group();
            label_el = label_el.group_color(k.c.text_weak);
        }
        if let Some(on_change) = self.on_view_mode_change.as_ref() {
            let on_change = Rc::clone(on_change);
            button = button.on_press(move |s: &mut S| on_change(s, mode));
        }
        button.child(label_el)
    }

    /// The state body: React's `relative flex min-h-0 flex-1 flex-col` with
    /// its `absolute inset-0` overlays, painted exclusively (every content
    /// branch is `!loadingRun`-gated in React too, so the observable tree is
    /// the same).
    fn content_body(&self, k: &Kit, dark: bool) -> El {
        let body = col()
            .key(format!("{ROOT_KEY}-states"))
            .w_full()
            .grow(1.0)
            .min_h(0.0);
        // React's `runError` overlay (red, `px-4 text-center text-xs`).
        if let Some(error) = self.state.run_error.clone() {
            return body.child(centered_note(
                k,
                "run-history-load-error",
                &error,
                k.c.danger.fg, // React text-red-400
            ));
        }
        // React's `!runError && !selectedId && !loadingIndex` hint.
        if self.state.selected_id.is_none() && !self.state.loading_index {
            return body.child(centered_note(
                k,
                "run-history-select-run",
                t("dashboard.selectRun"),
                k.c.text_muted,
            ));
        }
        // React's `loadingRun` overlay (the previous run stays loaded in the
        // state; its content branches are all gated off).
        if self.state.loading_run {
            return body.child(
                row()
                    .key("run-history-loading")
                    .grow(1.0)
                    .center()
                    .gap(SPINNER_GAP) // React mr-1.5
                    .child(
                        Spinner::new("run-history-loading-spinner")
                            .size(SpinnerSize::Lg)
                            .build(dark),
                    )
                    .child(
                        text(t("common.loading"))
                            .key("run-history-loading-text")
                            .font(type_xs())
                            .color(k.c.text_muted),
                    ),
            );
        }
        let Some(run) = self.state.loaded_run.as_ref() else {
            // Unreachable through the state machine (a cleared run with a
            // live selection is either loading or errored, both above);
            // React's branches all skip, painting nothing.
            return body;
        };
        match run.status {
            WidgetRunStatus::Error => body.child(centered_note(
                k,
                "run-history-error-state",
                // React `loadedRun.error ?? t('dashboard.runError')`.
                run.error.as_deref().unwrap_or(t("dashboard.runError")),
                k.c.danger.fg, // React text-red-400
            )),
            WidgetRunStatus::Timeout => body.child(centered_note(
                k,
                "run-history-timeout-state",
                t("dashboard.runTimeout"),
                palette::timeout_amber(dark), // React text-amber-400
            )),
            WidgetRunStatus::Ok => match self.state.view_mode {
                TileViewMode::Chart => {
                    // React's `hasChart && chartData && chartConfig`:
                    // the config, the seam data and the predicate together.
                    let chart = match (self.widget.chart_config.as_ref(), self.chart_data) {
                        (Some(config), Some(data))
                            if has_renderable_chart(Some(data), config) =>
                        {
                            Some((config, data))
                        }
                        _ => None,
                    };
                    match chart {
                        Some((config, data)) => body.child(
                            col()
                                .key("run-history-chart")
                                .w_full()
                                .grow(1.0)
                                .min_h(0.0)
                                .child(
                                    ChartCanvas::<S>::new(config, data).dark(dark).build(),
                                ),
                        ),
                        None => body.child(centered_note(
                            k,
                            "run-history-empty-chart",
                            // React `rowCount === 0 ? emptyResult
                            // : chartNotConfigured`.
                            if run.row_count == 0 {
                                t("dashboard.emptyResult")
                            } else {
                                t("dashboard.chartNotConfigured")
                            },
                            k.c.text_muted,
                        )),
                    }
                }
                TileViewMode::Table => {
                    if run.rows.is_empty() {
                        body.child(centered_note(
                            k,
                            "run-history-empty-table",
                            t("dashboard.emptyResult"),
                            k.c.text_muted,
                        ))
                    } else {
                        body.child(
                            col()
                                .key("run-history-table")
                                .w_full()
                                .grow(1.0)
                                .min_h(0.0)
                                // The shared keyed grid (28pt rows, the JS
                                // String(value) cells — T069's mapping).
                                .child(table_grid(run, "run-history-table", dark)),
                        )
                    }
                }
            },
        }
    }
}

/// React's `absolute inset-0 flex items-center justify-center px-4
/// text-center text-xs` states (the body paints exactly one, so it fills
/// instead). The text child carries `{key}-text` so audits can read the
/// message — the T069 `centered_note` convention.
fn centered_note(k: &Kit, key: &str, message: &str, color: Color) -> El {
    col()
        .key(key)
        .w_full()
        .grow(1.0)
        .center()
        .px(NOTE_PX)
        .child(
            text(message)
                .key(format!("{key}-text"))
                .font(k.ty.text_xsmall)
                .color(color)
                .wrap(),
        )
}
