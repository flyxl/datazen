//! View functions for the DataZen Rune UI.
#![allow(unused_imports)]

use super::*;

use crate::constants::*;
use crate::i18n::t;
use crate::icons::{self, section_icon, ICON_DASHBOARDS, ICON_PLUS, ICON_REFRESH};
use crate::state_def::{AppState, StructureState, WidgetBarsState};
use crate::types::{Section, TableView, Tool};
use crate::ui::field::TextField;
use crate::{splitter, sql_editor};
use datazen_ui_api::{
    ColumnDraft, ConnectionProfile, DashboardWidget, MemoryWorkspace, ProgressSnapshot, QueryApi,
    Setting, SettingsApi, TabKind, Workflow, WorkflowStepResult, WorkspaceApi,
};
use rune::tok::{self, type_};
use rune::{col, row, text, Color, Cx, El};

use crate::ui::palette;
use rune::{icon, FileDialogOptions, ScrollSpec, TextEdit, Tx};
use rune_kit::{Button, ButtonVariant, ControlSize};

use crate::ui::kit::kit;

// The workflows section arms in `views/workspace.rs` route to the page
// port (`views/workflow/workflow_page.rs`, T145) — the lite inline editor
// this module carried until that swap is retired with it.
/// The dashboards page: the widget list. An empty list has its own state.
pub fn dashboards_page(state: &AppState, theme: tok::Theme, dark: bool) -> El {
    let k = kit(dark);
    let mut page = col().key("dashboards-page").w_full().grow(1.0).min_h(0.0).fill(k.c.page);
    let board = match state.desktop.native_dashboard_board() {
        Ok(None) => return page.child(first_board_view(state, k)),
        Ok(Some(board)) => Some(board),
        // The demo/older adapter keeps its already supported report UI.
        Err(error) if error == datazen_ui_api::DESKTOP_NOT_IMPLEMENTED => None,
        Err(error) => return page.child(dashboard_read_error("dashboard-board-error", error, k)),
    };
    let native = board.is_some();
    let widgets = if let Some(board) = board {
        page = page.child(selected_board_marker(board, k));
        // Count actual report rows, not the demo fallback or a fabricated zero.
        let widgets = match state.desktop.dashboard_widgets() {
            Ok(widgets) => widgets,
            Err(error) => return page.child(dashboard_read_error("dashboard-report-read-error", error, k)),
        };
        page = page.child(native_report_toolbar(widgets.len(), k));
        widgets
    } else {
        state.host_widgets()
    };
    if !native || state.dashboard_report_open {
        let active_session = !native || state.desktop.native_dashboard_report_session_open();
        page = page.child(widget_form(state, k, dark, native, active_session));
        if !active_session {
            page = page.child(text(t("dashboard.reportNeedsConnection"))
                .key("dashboard-report-needs-connection").role("status")
                .w_full().shrink().px(16.0).py(8.0)
                .font(k.ty.text_xsmall).color(k.c.text_muted).wrap());
        }
    }
    if let Some(error) = state.widget_error.as_ref().filter(|_| !native || state.dashboard_report_open) {
        page = page.child(
            text(error.clone())
                .key("widget-form-error")
                .w_full()
                .shrink()
                .px(16.0)
                .py(8.0)
                .font(k.ty.text_xsmall)
                .color(k.c.danger.fg)
                .fill(k.alpha(k.c.danger.fg, 0.06))
                .wrap()
                .selectable(true),
        );
    }
    // In the real board Add is the final grid cell, including when no
    // reports exist; opening an inline draft suppresses just that cell.
    if native {
        return page.child(native_report_grid(state, &widgets, theme, k));
    }
    if widgets.is_empty() {
        // The demo's original widget-empty treatment.
        return page.child(
            col()
                .key("dashboard-empty")
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .center()
                .gap(12.0)
                .px(16.0)
                .child(
                    icon(&ICON_DASHBOARDS, 40.0)
                        .key("dashboard-empty-icon")
                        .color(k.alpha(k.c.text_faint, 0.40)),
                )
                .child(
                    text(t("dashboard.empty"))
                        .key("dashboard-empty-message")
                        .font(k.ty.body)
                        .color(k.c.text_faint)
                        .align(rune::Align::Center)
                        .wrap(),
                ),
        );
    }
    let mut list = col()
        .key("dashboard-widget-list")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .gap(12.0)
        .padding(16.0)
        .scroll(ScrollSpec { stick_bottom: false });
    for widget in &widgets {
        list = list.child(report_tile(state, widget, theme, k, None));
    }
    page.child(list)
}

fn report_tile(
    state: &AppState, widget: &DashboardWidget, theme: tok::Theme, k: &rune_kit::Kit,
    width: Option<f32>,
) -> El {
    let tile = col()
        .key(format!("widget-{}", widget.id))
        .gap(4.0)
        .px(8.0)
        .py(6.0)
        .radius(8.0)
        .shrink()
        .fill(k.c.bg_base)
        .border(1.0, k.c.border_base)
        .child(
            row()
                .key(format!("widget-head-{}", widget.id))
                .w_full()
                .h(24.0)
                .center_y()
                .child(
                    text(if width.is_some() { widget.title.clone() } else {
                        format!("#{} · {} · {}", widget.id, widget.title, widget.kind)
                    })
                    .min_w(0.0).truncate()
                    .font(type_::LABEL)
                    .color(theme.text),
                ),
        )
        .child(bar_chart(state, widget.id, theme));
    match width {
        Some(width) => tile.w(width).min_w(width).h(320.0),
        None => tile.w_full(),
    }
}

fn native_report_grid(
    state: &AppState, widgets: &[DashboardWidget], theme: tok::Theme, k: &rune_kit::Kit,
) -> El {
    // CSS repeat(auto-fill, minmax(420px, 1fr)); reserve empty tracks on
    // the last row. Measure the mounted content box, never the OS window.
    const GAP: f32 = 12.0;
    const MIN: f32 = 420.0;
    let available = rune::memo::layout_with("dashboard-report-grid", |rect| rect[2])
        .filter(|w| w.is_finite()).unwrap_or(MIN).max(MIN);
    let columns = ((available + GAP) / (MIN + GAP)).floor() as usize;
    let width = (available - GAP * (columns - 1) as f32) / columns as f32;
    let count = widgets.len() + usize::from(!state.dashboard_report_open);
    let mut grid = col().key("dashboard-report-grid").w_full().gap(GAP);
    for from in (0..count).step_by(columns) {
        let mut line = row().key(format!("dashboard-report-row-{from}")).w_full().min_h(320.0).gap(GAP);
        for index in from..(from + columns).min(count) {
            line = line.child(match widgets.get(index) {
                Some(widget) => report_tile(state, widget, theme, k, Some(width)),
                None => add_report_tile(k, width),
            });
        }
        grid = grid.child(line);
    }
    col().key(if widgets.is_empty() { "dashboard-empty" } else { "dashboard-widget-list" })
        .w_full().grow(1.0).min_h(0.0).padding(16.0)
        .scroll(ScrollSpec { stick_bottom: false }).child(grid)
}

fn dashboard_read_error(key: &str, error: String, k: &rune_kit::Kit) -> El {
    col().w_full().grow(1.0).center().padding(16.0).child(
        text(error).key(key).color(k.c.danger.fg).wrap(),
    )
}

fn selected_board_marker(board: datazen_ui_api::NativeDashboardBoard, k: &rune_kit::Kit) -> El {
    // React's 32pt selected entry plus the strip's 1pt edge. This is an
    // identity marker for the single persisted board, never a tab button.
    row().key("dashboard-selected-board").w_full().h(33.0).shrink()
        .fill(k.c.layer_02).border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
        .child(
            col().key(format!("dashboard-selected-board-{}", board.id)).h(32.0).max_w(145.0).shrink()
                .fill(k.c.page).border_sides([0.0, 1.0, 0.0, 0.0], k.c.border_base)
                .child(
                    row().h(30.0).px(12.0).center_y().child(
                        text(board.name).key("dashboard-selected-board-title")
                            .font(k.ty.text_xsmall_medium).color(k.c.text_base)
                            .max_w(120.0).truncate(),
                    ),
                )
                .child(
                    row().key("dashboard-selected-board-indicator")
                        .pin(Some(0.0), None, Some(0.0), Some(0.0)).h(2.0).fill(k.c.bg_accent),
                ),
        )
}

fn native_report_toolbar(count: usize, k: &rune_kit::Kit) -> El {
    // React's existing report toolbar is 28pt controls + 6pt vertical insets
    // + 1pt bottom edge. Keep real count/actions in that space, not filler.
    let actions = row().key("dashboard-report-actions").w_full().h(41.0).shrink()
        .px(12.0).gap(4.0).center_y()
        .fill(k.c.page).border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
        .child(
            row().grow(1.0).min_w(0.0).child(
                text(t("dashboard.widgetCount").replace("{count}", &count.to_string()))
                    .key("dashboard-report-count").role("status")
                    .font(k.ty.text_xsmall_medium).color(k.c.text_muted).truncate(),
            ),
        );
    let mut refresh = Button::new(t("dashboard.refreshAll"))
        .variant(ButtonVariant::Ghost).control_size(ControlSize::Large).icon(&ICON_REFRESH);
    if count == 0 {
        // No fake action when there is nothing to refresh.
        refresh = refresh.disabled(true);
    } else {
        refresh = refresh.on_press(|state: &mut AppState| state.refresh_widget_bars());
    }
    actions.child(refresh.build(k).key("widget-refresh"))
}

fn widget_refresh(k: &rune_kit::Kit) -> El {
    Button::new(t("dashboard.refreshWidget"))
        .variant(ButtonVariant::Ghost)
        .control_size(ControlSize::Large)
        .icon(&ICON_REFRESH)
        .on_press(|state: &mut AppState| state.refresh_widget_bars())
        .build(k)
        .key("widget-refresh")
}

fn add_report_tile(k: &rune_kit::Kit, width: f32) -> El {
    let label = t("dashboard.addWidget");
    col().key("dashboard-add-widget").w(width).min_w(width).h(320.0)
        .gap(12.0).center().radius(k.radius.lg)
        .border(2.0, k.alpha(k.c.border_base, 0.6))
        .hover(k.alpha(k.c.bg_accent, 0.05))
        .focus_ring(2.0, 2.5, k.c.border_focus)
        .role("button").label(label).focusable(true)
        .child(icon(&ICON_PLUS, 32.0).color(k.alpha(k.c.text_muted, 0.6)).a11y_hidden())
        .child(text(label).key("dashboard-add-widget-text").font(k.ty.body).color(k.c.text_muted).a11y_hidden())
        .on_press(|state: &mut AppState| state.begin_native_dashboard_report())
}

fn widget_form(
    state: &AppState,
    k: &rune_kit::Kit,
    dark: bool,
    native: bool,
    active_session: bool,
) -> El {
    let title = TextField::new("widget-title-input", &state.widget_title)
        .height(28.0).width(164.0).shrink().pad_x(8.0).radius(k.radius.sm)
        .fill(k.c.bg_base).border(1.0, k.c.border_base)
        .placeholder(t("dashboard.widgetTitlePlaceholder")).label(t("dashboard.widgetTitle"))
        .on_edit(|state: &mut AppState| &mut state.widget_title).build(dark);
    let query = TextField::new("widget-query-input", &state.widget_query)
        .height(28.0).width(280.0).shrink().pad_x(8.0).radius(k.radius.sm)
        .fill(k.c.bg_base).border(1.0, k.c.border_base)
        .placeholder(t("dashboard.sql")).label(t("dashboard.sql"))
        .on_edit(|state: &mut AppState| &mut state.widget_query).build(dark);
    let label = t("common.save");
    let mut save = row().key("widget-save").h(28.0).px(12.0).gap(4.0).center()
        .radius(k.radius.md)
        .fill(if active_session { k.c.bg_accent } else { k.c.surface_weak })
        .role("button").label(label)
        .child(icon(&ICON_PLUS, 14.0)
            .color(if active_session { k.c.submit_icon } else { k.c.text_faint }).a11y_hidden())
        .child(text(label).font(k.ty.text_xsmall_medium)
            .color(if active_session { k.c.submit_fg } else { k.c.text_faint }).a11y_hidden());
    if active_session {
        save = save.hover(k.alpha(k.c.submit_hover, 0.8))
            .focus_ring(2.0, 2.5, k.c.border_focus).focusable(true)
            .on_press(move |state: &mut AppState| {
                if native {
                    state.save_native_dashboard_report();
                } else {
                    state.save_widget_form();
                }
            });
    } else {
        save = save.disabled(true);
    }
    let form = row().key("widget-form").w_full().shrink().gap(8.0)
        .px(16.0).py(8.0).items_center().flex_wrap()
        .fill(k.c.surface_weak).border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
        .child(title).child(query).child(save);
    if native {
        form.child(
            Button::new(t("common.cancel")).variant(ButtonVariant::Ghost)
                .control_size(ControlSize::Large)
                .on_press(|state: &mut AppState| state.cancel_native_dashboard_report())
                .build(k).key("widget-cancel"),
        )
    } else {
        form.child(widget_refresh(k))
    }
}

fn first_board_view(state: &AppState, k: &rune_kit::Kit) -> El {
    let label = t("dashboard.createFirstBoard");
    let board_name = t("dashboard.defaultName").to_owned();
    let mut hero = col().key("dashboard-empty-boards").w_full().grow(1.0).min_h(0.0)
        .center().gap(12.0).px(16.0)
        .child(icon(&ICON_DASHBOARDS, 40.0).color(k.alpha(k.c.text_faint, 0.4)))
        .child(text(t("dashboard.emptyBoards")).font(k.ty.body).color(k.c.text_faint).align(rune::Align::Center).wrap())
        .child(
            row().key("dashboard-create-first").h(28.0).px(8.0).gap(4.0).center()
                .radius(k.radius.md).fill(k.c.bg_accent).hover(k.alpha(k.c.submit_hover, 0.8))
                .focus_ring(2.0, 2.5, k.c.border_focus).role("button").label(label).focusable(true)
                .child(icon(&ICON_PLUS, 14.0).color(k.c.submit_icon).a11y_hidden())
                .child(text(label).font(k.ty.text_xsmall_medium).color(k.c.submit_fg).a11y_hidden())
                .on_press(move |state: &mut AppState| state.create_first_dashboard(&board_name)),
        );
    if let Some(error) = state.dashboard_create_error.as_ref() {
        hero = hero.child(text(error.clone()).key("dashboard-create-error").color(k.c.danger.fg).wrap());
    }
    hero
}

/// A bar chart drawn with color blocks — the port's integers, no chart library.
/// The zero bar keeps a visible sliver and its label. E2: the bars come
/// from the widget's cache entry — the render never queries the widget's
/// data; a read in flight, its failure sentence or the not-loaded hint
/// each say themselves instead of a phantom empty chart.
pub fn bar_chart(state: &AppState, id: u64, theme: tok::Theme) -> El {
    let notice = match state.widget_bars.get(&id) {
        None => Some("the widget's data is not loaded — refresh the page".to_owned()),
        Some(WidgetBarsState { loading: true, .. }) => {
            Some("reading the widget's data…".to_owned())
        }
        Some(WidgetBarsState {
            error: Some(error), ..
        }) => Some(error.clone()),
        Some(WidgetBarsState { bars: Some(_), .. }) => None,
        Some(WidgetBarsState { bars: None, .. }) => {
            Some("the widget's data is not loaded — refresh the page".to_owned())
        }
    };
    if let Some(sentence) = notice {
        return row()
            .key(format!("widget-chart-notice-{id}"))
            .w_full()
            .h(120.0)
            .px(8.0)
            .center_y()
            .child(
                text(sentence)
                    .font(type_::LABEL)
                    .color(theme.text_secondary),
            );
    }
    let bars = state
        .widget_bars
        .get(&id)
        .and_then(|entry| entry.bars.clone())
        .unwrap_or_default();
    let max = bars
        .iter()
        .map(|(_, value)| *value)
        .max()
        .unwrap_or(1)
        .max(1);
    let mut chart = row()
        .key(format!("widget-chart-{id}"))
        .w_full()
        .h(120.0)
        .gap(10.0);
    for (label, value) in &bars {
        let height = if *value == 0 {
            2.0
        } else {
            12.0 + (*value as f32 / max as f32) * 84.0
        };
        chart = chart.child(
            col()
                .key(format!("widget-bar-{id}-{label}"))
                .w(46.0)
                .gap(4.0)
                .child(
                    row()
                        .key(format!("widget-block-{id}-{label}"))
                        .w(30.0)
                        .h(height)
                        .radius(3.0)
                        .fill(palette::chart_bar_accent(false)),
                )
                .child(
                    text(format!("{label} {value}"))
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                ),
        );
    }
    chart
}

/// The tools page: one switcher, three tools. Sync, compare, transfer.
pub fn workflow_yaml(name: &str, steps: &[String]) -> String {
    let mut out = format!("name: {name}\nsteps:\n");
    for step in steps {
        out.push_str(&format!("  - {step}\n"));
    }
    out
}
pub fn next_connection(workspace: &MemoryWorkspace, current: &str) -> String {
    let ids: Vec<String> = workspace
        .connections()
        .iter()
        .map(|profile| profile.id.clone())
        .collect();
    match ids.iter().position(|id| id == current) {
        Some(index) => ids[(index + 1) % ids.len()].clone(),
        None => ids.first().cloned().unwrap_or_else(|| current.to_owned()),
    }
}
