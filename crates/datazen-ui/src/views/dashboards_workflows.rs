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
use rune::{col, oklch, row, text, Color, Cx, El};
use rune::{FileDialogOptions, TextEdit, Tx};

pub(crate) fn wapps_page(theme: tok::Theme) -> El {
    let entry = |key: &'static str, title: &'static str, body: &'static str| {
        col()
            .key(key)
            .w_full()
            .gap(6.0)
            .px(12.0)
            .py(10.0)
            .radius(8.0)
            .fill(theme.control_off)
            .child(text(title.to_owned()).font(type_::TITLE).color(theme.text))
            .child(
                text(body.to_owned())
                    .font(type_::LABEL)
                    .color(theme.text_secondary)
                    .w_full(),
            )
    };
    col()
        .key("wapps-page")
        .w_full()
        .gap(8.0)
        .child(
            text("Not yet migrated".to_owned())
                .font(type_::TITLE)
                .color(theme.text),
        )
        .child(
            text("These areas are not yet migrated to Rune. The React pages are still there — use the React interface for them.")
                .font(type_::LABEL)
                .color(theme.text_secondary)
                .w_full(),
        )
        .child(entry(
            "wapps-pending",
            "WAPPS management",
            "Not yet migrated. The React page is still there: install and manage WAPPS in the React interface. WAPP iframes do not go into Rune.",
        ))
        .child(entry(
            "onboarding-pending",
            "Onboarding wizard",
            "Not yet migrated. The React wizard is still there: first-run onboarding runs in the React interface.",
        ))
}

/// The workflows page: the list, sorted by name, and the step editor.
pub(crate) fn workflows_page(state: &AppState, theme: tok::Theme, dark: bool) -> El {
    let workflows = state.host_workflows();
    let mut page = col().key("workflows-page").w_full().gap(6.0).child(
        text("Workflows".to_owned())
            .font(type_::TITLE)
            .color(theme.text),
    );
    for workflow in &workflows {
        let name = workflow.name.clone();
        page = page.child(
            row()
                .key(format!("workflow-{}", workflow.name))
                .w_full()
                .h(28.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(
                    text(format!(
                        "{} · {} steps",
                        workflow.name,
                        workflow.steps.len()
                    ))
                    .font(type_::LABEL)
                    .color(theme.text),
                )
                .on_press(move |state: &mut AppState| state.edit_workflow(&name)),
        );
    }
    if let Some((name, steps)) = &state.workflow_editing {
        let mut editor = col()
            .key("workflow-form")
            .w_full()
            .gap(4.0)
            .px(8.0)
            .py(6.0)
            .radius(8.0)
            .fill(theme.control_off)
            .child(
                row()
                    .key("workflow-form-head")
                    .w_full()
                    .h(24.0)
                    .center_y()
                    .child(text(name.clone()).font(type_::LABEL).color(theme.text))
                    .child(
                        row()
                            .key("workflow-save")
                            .h(20.0)
                            .px(6.0)
                            .center_y()
                            .radius(4.0)
                            .fill(theme.control_off)
                            .child(text("Save").font(type_::LABEL).color(theme.text))
                            .on_press(|state: &mut AppState| state.save_workflow_form()),
                    )
                    .child(
                        row()
                            .key("workflow-run")
                            .h(20.0)
                            .px(6.0)
                            .center_y()
                            .radius(4.0)
                            .fill(theme.control_off)
                            .child(text("Run").font(type_::LABEL).color(theme.text))
                            .on_press(|state: &mut AppState| state.run_edited_workflow()),
                    )
                    .child(
                        row()
                            .key("workflow-cancel")
                            .h(20.0)
                            .px(6.0)
                            .center_y()
                            .radius(4.0)
                            .fill(theme.control_off)
                            .child(
                                text("Close".to_owned())
                                    .font(type_::LABEL)
                                    .color(theme.text_secondary),
                            )
                            .on_press(|state: &mut AppState| state.close_workflow_editor()),
                    ),
            );
        for (index, step) in steps.iter().enumerate() {
            editor = editor.child(
                row()
                    .key(format!("step-row-{index}"))
                    .w_full()
                    .h(24.0)
                    .gap(6.0)
                    .center_y()
                    .child(
                        text(step.clone())
                            .font(type_::LABEL)
                            .color(theme.text_secondary),
                    )
                    .child(
                        row()
                            .key(format!("step-up-{index}"))
                            .h(18.0)
                            .px(6.0)
                            .center_y()
                            .radius(4.0)
                            .fill(theme.control_off)
                            .child(text("up".to_owned()).font(type_::LABEL).color(theme.text))
                            .on_press(move |state: &mut AppState| {
                                state.move_workflow_step(index, true)
                            }),
                    )
                    .child(
                        row()
                            .key(format!("step-down-{index}"))
                            .h(18.0)
                            .px(6.0)
                            .center_y()
                            .radius(4.0)
                            .fill(theme.control_off)
                            .child(text("down".to_owned()).font(type_::LABEL).color(theme.text))
                            .on_press(move |state: &mut AppState| {
                                state.move_workflow_step(index, false)
                            }),
                    ),
            );
        }
        page = page.child(editor);
        page = page.child(crate::sql_editor::plain_panel(
            &state.workflow_yaml,
            theme.text,
            theme.text_secondary,
            dark,
        ));
        if !state.workflow_results.is_empty() {
            let mut results = col()
                .key("run-results")
                .w_full()
                .gap(4.0)
                .px(8.0)
                .py(6.0)
                .radius(8.0)
                .fill(theme.control_off);
            for (index, result) in state.workflow_results.iter().enumerate() {
                results = results.child(
                    row()
                        .key(format!("run-result-{index}"))
                        .w_full()
                        .h(24.0)
                        .gap(8.0)
                        .center_y()
                        .child(
                            row()
                                .key(format!(
                                    "run-{}-{index}",
                                    if result.ok { "ok" } else { "fail" }
                                ))
                                .w(10.0)
                                .h(10.0)
                                .radius(5.0)
                                .fill(if result.ok {
                                    oklch(0.75, 0.12, 145.0)
                                } else {
                                    oklch(0.7, 0.14, 25.0)
                                }),
                        )
                        .child(
                            text(format!(
                                "{} · {} — {}",
                                result.step,
                                if result.ok { "ok" } else { "failed" },
                                result.summary
                            ))
                            .font(type_::LABEL)
                            .color(theme.text),
                        ),
                );
            }
            page = page.child(results);
        }
    }
    page
}

/// The dashboards page: the widget list. An empty list has its own state.
pub(crate) fn dashboards_page(state: &AppState, theme: tok::Theme) -> El {
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
                        .build(),
                )
                .child(
                    TextField::new("widget-query-input", &state.widget_query)
                        .height(24.0)
                        .width(260.0)
                        .shrink()
                        .on_edit(|state: &mut AppState| &mut state.widget_query)
                        .build(),
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
                        .fill(oklch(0.72, 0.13, 250.0)),
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
