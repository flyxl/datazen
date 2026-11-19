//! View functions for the DataZen Rune UI.
#![allow(unused_imports)]

use super::*;

use crate::constants::*;
use crate::icons::{self, section_icon};
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
use rune::{FileDialogOptions, TextEdit, Tx};

// The workflows section arms in `views/workspace.rs` route to the page
// port (`views/workflow/workflow_page.rs`, T145) — the lite inline editor
// this module carried until that swap is retired with it.
/// The dashboards page: the widget list. An empty list has its own state.
pub(crate) fn dashboards_page(state: &AppState, theme: tok::Theme, dark: bool) -> El {
    let widgets = state.host_widgets();
    let mut page = col()
        .key("dashboards-page")
        .w_full()
        .gap(8.0)
        .child(
            text("Dashboards".to_owned())
                .font(type_::TITLE)
                .color(theme.text),
        )
        .child(
            row()
                .key("widget-form")
                .w_full()
                .h(28.0)
                .gap(8.0)
                .center_y()
                .child(
                    TextField::new("widget-title-input", &state.widget_title)
                        .height(24.0)
                        .width(160.0)
                        .on_edit(|state: &mut AppState| &mut state.widget_title)
                        .build(dark),
                )
                .child(
                    TextField::new("widget-query-input", &state.widget_query)
                        .height(24.0)
                        .width(260.0)
                        .shrink()
                        .on_edit(|state: &mut AppState| &mut state.widget_query)
                        .build(dark),
                )
                .child(
                    row()
                        .key("widget-save")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text("Save widget").font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.save_widget_form()),
                )
                .child(
                    row()
                        .key("widget-refresh")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text("Refresh").font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.refresh_widget_bars()),
                ),
        );
    if let Some(error) = &state.widget_error {
        page = page.child(
            row()
                .key("widget-form-error")
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(
                    text(error.clone())
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                ),
        );
    }
    if widgets.is_empty() {
        return page.child(
            col()
                .key("dashboard-empty")
                .w_full()
                .h(120.0)
                .px(12.0)
                .py(8.0)
                .radius(8.0)
                .fill(theme.control_off)
                .child(
                    text("No widgets yet".to_owned())
                        .font(type_::TITLE)
                        .color(theme.text),
                )
                .child(
                    text("The widget form arrives with the next task")
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                ),
        );
    }
    for widget in &widgets {
        page = page.child(
            col()
                .key(format!("widget-{}", widget.id))
                .w_full()
                .gap(4.0)
                .px(8.0)
                .py(6.0)
                .radius(8.0)
                .fill(theme.control_off)
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
    page
}

/// A bar chart drawn with color blocks — the port's integers, no chart library.
/// The zero bar keeps a visible sliver and its label. E2: the bars come
/// from the widget's cache entry — the render never queries the widget's
/// data; a read in flight, its failure sentence or the not-loaded hint
/// each say themselves instead of a phantom empty chart.
pub(crate) fn bar_chart(state: &AppState, id: u64, theme: tok::Theme) -> El {
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
pub(crate) fn workflow_yaml(name: &str, steps: &[String]) -> String {
    let mut out = format!("name: {name}\nsteps:\n");
    for step in steps {
        out.push_str(&format!("  - {step}\n"));
    }
    out
}
pub(crate) fn next_connection(workspace: &MemoryWorkspace, current: &str) -> String {
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
