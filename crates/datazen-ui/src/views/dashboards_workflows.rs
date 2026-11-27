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
    let widgets = state.host_widgets();
    let k = kit(dark);
    let mut page = col()
        .key("dashboards-page")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .fill(k.c.page)
        .child(
            row()
                .key("widget-form")
                .w_full()
                .shrink()
                .gap(8.0)
                .px(16.0)
                .py(8.0)
                .items_center()
                .flex_wrap()
                .fill(k.c.surface_weak)
                .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
                .child(
                    TextField::new("widget-title-input", &state.widget_title)
                        .height(28.0)
                        .width(164.0)
                        .shrink()
                        .pad_x(8.0)
                        .radius(k.radius.sm)
                        .fill(k.c.bg_base)
                        .border(1.0, k.c.border_base)
                        .placeholder(t("dashboard.widgetTitlePlaceholder"))
                        .label(t("dashboard.widgetTitle"))
                        .on_edit(|state: &mut AppState| &mut state.widget_title)
                        .build(dark),
                )
                .child(
                    TextField::new("widget-query-input", &state.widget_query)
                        .height(28.0)
                        .width(280.0)
                        .shrink()
                        .pad_x(8.0)
                        .radius(k.radius.sm)
                        .fill(k.c.bg_base)
                        .border(1.0, k.c.border_base)
                        .placeholder(t("dashboard.sql"))
                        .label(t("dashboard.sql"))
                        .on_edit(|state: &mut AppState| &mut state.widget_query)
                        .build(dark),
                )
                .child(
                    row()
                        .key("widget-save")
                        .h(28.0)
                        .px(12.0)
                        .gap(4.0)
                        .center()
                        .radius(k.radius.md)
                        .fill(k.c.bg_accent)
                        .hover(k.alpha(k.c.submit_hover, 0.8))
                        .focus_ring(2.0, 2.5, k.c.border_focus)
                        .role("button")
                        .label(t("common.save"))
                        .focusable(true)
                        .child(icon(&ICON_PLUS, 14.0).color(k.c.submit_icon).a11y_hidden())
                        .child(
                            text(t("common.save"))
                                .font(k.ty.text_xsmall_medium)
                                .color(k.c.submit_fg)
                                .a11y_hidden(),
                        )
                        .on_press(|state: &mut AppState| state.save_widget_form()),
                )
                .child(
                    Button::new(t("dashboard.refreshWidget"))
                        .variant(ButtonVariant::Ghost)
                        .control_size(ControlSize::Large)
                        .icon(&ICON_REFRESH)
                        .on_press(|state: &mut AppState| state.refresh_widget_bars())
                        .build(k)
                        .key("widget-refresh"),
                ),
        );
    if let Some(error) = &state.widget_error {
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
    if widgets.is_empty() {
        // Match the embedded DashboardPanel's centered empty treatment. This
        // host manages widgets, not boards; the form is the real entry point.
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
        list = list.child(
            col()
                .key(format!("widget-{}", widget.id))
                .w_full()
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
                            text(format!(
                                "#{} · {} · {}",
                                widget.id, widget.title, widget.kind
                            ))
                            .font(type_::LABEL)
                            .color(theme.text),
                        ),
                )
                .child(bar_chart(state, widget.id, theme)),
        );
    }
    page.child(list)
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
