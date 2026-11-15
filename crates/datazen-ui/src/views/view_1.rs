//! View functions for the DataZen Rune UI.

use super::*;
use crate::constants::*;
use crate::icons::{self, ICON_CHEVRON_LEFT, ICON_CHEVRON_RIGHT, section_icon};
use crate::state_def::{StructureState, WidgetBarsState};
use datazen_ui_api::{ColumnDraft, MemoryWorkspace};

use crate::state_def::AppState;
use crate::types::{Section, TableView, Tool};
use crate::constants::*;
use crate::{splitter, sql_editor};

use datazen_ui_api::{ConnectionProfile, DashboardWidget, ProgressSnapshot, QueryApi, Setting, SettingsApi, TabKind, Workflow, WorkflowStepResult, WorkspaceApi};
use rune::{FileDialogOptions, TextEdit, Tx};
use rune::tok::{self, type_};
use rune::{Color, Cx, El, col, oklch, row, text, text_input};

pub(crate) fn rail_item(state: &AppState, section: Section, expanded: bool, dark: bool) -> El {
    let theme = tok::theme(dark);
    let selected = state.active == section;
    let bg = if selected { tok::accent(0, dark) } else { Color::TRANSPARENT };
    let fg = if selected { theme.text } else { theme.text_secondary };
    let mut item = row()
        .key(format!("rail-{}", section.key()))
        .w_full()
        .h(40.0)
        .px(if expanded { 12.0 } else { 0.0 })
        .center_y()
        .gap(if expanded { 10.0 } else { 0.0 })
        .radius(8.0)
        .fill(bg)
        .role("tab")
        .label(section.label())
        .on_press(move |state: &mut AppState| state.select(section));
    if expanded {
        item = item
            .child(rune::icon(section_icon(section), 18.0).color(fg))
            .child(text(section.label()).font(type_::LABEL).color(fg));
    } else {
        item = item.justify_center().child(rune::icon(section_icon(section), 18.0).color(fg));
    }
    item
}

pub(crate) fn rail_view(state: &AppState, dark: bool) -> El {
    let theme = tok::theme(dark);
    let expanded = state.sidebar_expanded();
    let width = state.sidebar_rail_width() as f32;
    let mut items = col().w_full().gap(4.0);
    for section in Section::ALL {
        items = items.child(rail_item(state, section, expanded, dark));
    }
    let toggle_icon = if expanded { &ICON_CHEVRON_LEFT } else { &ICON_CHEVRON_RIGHT };
    let toggle = row()
        .key("rail-toggle")
        .w_full()
        .h(32.0)
        .center()
        .radius(8.0)
        .fill(theme.control_off)
        .label(if expanded { "Collapse sidebar" } else { "Expand sidebar" })
        .on_press(|state: &mut AppState| state.toggle_sidebar_expanded())
        .child(rune::icon(toggle_icon, 14.0).color(theme.text_secondary));
    col()
        .key("datazen-rail")
        .w(width)
        .h_full()
        .px(4.0)
        .py(8.0)
        .gap(8.0)
        .fill(theme.bg)
        .child(items)
        .child(col().w_full().grow(1.0))
        .child(toggle)
}

pub(crate) fn navigator_connections_tree(state: &AppState, theme: tok::Theme) -> El {
    let filter = state.navigator_filter.text.trim().to_lowercase();
    let visible = state.visible_connections();
    let matches = |c: &ConnectionProfile| {
        filter.is_empty()
            || c.name.to_lowercase().contains(&filter)
            || c.engine.to_lowercase().contains(&filter)
            || c.id.to_lowercase().contains(&filter)
    };

    let mut recent: Vec<ConnectionProfile> = Vec::new();
    for id in &state.recent_connection_ids {
        if let Some(c) = visible.iter().find(|c| &c.id == id) {
            if matches(c) {
                recent.push(c.clone());
            }
        }
    }
    let all: Vec<ConnectionProfile> = visible.into_iter().filter(matches).collect();

    let mut tree = col().key("navigator-connections").w_full().gap(4.0);
    tree = tree.child(
        text("Connections")
            .key("navigator-connections-title")
            .font(type_::LABEL)
            .color(theme.text_secondary),
    );

    tree = tree.child(
        text("Recent")
            .key("nav-group-recent")
            .font(type_::LABEL)
            .color(theme.text_secondary),
    );
    if recent.is_empty() {
        tree = tree.child(
            row()
                .key("nav-group-recent-empty")
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .child(
                    text(if filter.is_empty() { "No recent connections" } else { "No matches" })
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                ),
        );
    } else {
        for c in recent {
            let id = c.id.clone();
            let label = format!("{} · {}", c.name, c.engine);
            tree = tree.child(
                row()
                    .key(format!("nav-conn-recent-{}", c.id))
                    .w_full()
                    .h(28.0)
                    .px(16.0)
                    .center_y()
                    .radius(6.0)
                    .fill(theme.control_off)
                    .label(label.clone())
                    .child(text(label).font(type_::LABEL).color(theme.text))
                    .on_press(move |state: &mut AppState| state.open_connection(&id)),
            );
        }
    }

    tree = tree.child(
        text("All connections")
            .key("nav-group-all")
            .font(type_::LABEL)
            .color(theme.text_secondary),
    );
    if all.is_empty() {
        tree = tree.child(
            row()
                .key("nav-group-all-empty")
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .child(
                    text(if filter.is_empty() { "No connections" } else { "No matches" })
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                ),
        );
    } else {
        for c in all {
            let id = c.id.clone();
            let label = format!("{} · {}", c.name, c.engine);
            tree = tree.child(
                row()
                    .key(format!("nav-conn-{}", c.id))
                    .w_full()
                    .h(28.0)
                    .px(16.0)
                    .center_y()
                    .radius(6.0)
                    .fill(theme.control_off)
                    .label(label.clone())
                    .child(text(label).font(type_::LABEL).color(theme.text))
                    .on_press(move |state: &mut AppState| state.open_connection(&id)),
            );
        }
    }

    tree
}

pub(crate) fn navigator_panel(state: &AppState, theme: tok::Theme) -> El {
    let width = state.navigator_width() as f32;
    col()
        .key("datazen-navigator")
        .w(width)
        .h_full()
        .px(12.0)
        .py(12.0)
        .gap(8.0)
        .fill(theme.bg)
        .child(
            text("Navigator")
                .key("navigator-title")
                .font(type_::LABEL)
                .color(theme.text_secondary),
        )
        .child(
            text_input(&state.navigator_filter, type_::LABEL)
                .key("navigator-filter-input")
                .w_full()
                .h(28.0)
                .on_edit(|state: &mut AppState| &mut state.navigator_filter),
        )
        .child(navigator_connections_tree(state, theme))
        .child(object_tree(state, theme))
        .child(refresh_button(state, theme))
        .child(er_button(state, theme))
        .child(open_query_button(theme))
        .child(disconnect_button(state, theme))
}
/// The connection after `current` in the port's list, wrapping around.
/// A workflow as YAML text.
pub(crate) fn workflow_yaml(name: &str, steps: &[String]) -> String {
    let mut out = format!("name: {name}\nsteps:\n");
    for step in steps {
        out.push_str(&format!("  - {step}\n"));
    }
    out
}

pub(crate) fn next_connection(workspace: &MemoryWorkspace, current: &str) -> String {
    let ids: Vec<String> = workspace.connections().iter().map(|profile| profile.id.clone()).collect();
    match ids.iter().position(|id| id == current) {
        Some(index) => ids[(index + 1) % ids.len()].clone(),
        None => ids.first().cloned().unwrap_or_else(|| current.to_owned()),
    }
}

pub(crate) fn query_selected(state: &AppState) -> bool {
    state.tabs.selected().is_some_and(|tab| tab.kind == TabKind::Query)
}

pub(crate) fn page_body(state: &AppState, theme: tok::Theme, dark: bool) -> El {
    match state.active {
        Section::Connections => connection_list(state, theme),
        Section::Workspace => workspace_body(state, theme, dark),
        Section::Settings => settings_page(state, theme),
        Section::Tools => tools_page(state, theme, dark),
        Section::Dashboards => dashboards_page(state, theme),
        Section::Workflows => workflows_page(state, theme, dark),
        Section::Wapps => wapps_page(theme),
    }
}

/// The areas the migration has not reached. Each says so, and says where
/// the product still serves it: the React interface, which stays.
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
    let mut page = col()
        .key("workflows-page")
        .w_full()
        .gap(6.0)
        .child(text("Workflows".to_owned()).font(type_::TITLE).color(theme.text));
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
                    text(format!("{} · {} steps", workflow.name, workflow.steps.len()))
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
                            .child(text("Close".to_owned()).font(type_::LABEL).color(theme.text_secondary))
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
                    .child(text(step.clone()).font(type_::LABEL).color(theme.text_secondary))
                    .child(
                        row()
                            .key(format!("step-up-{index}"))
                            .h(18.0)
                            .px(6.0)
                            .center_y()
                            .radius(4.0)
                            .fill(theme.control_off)
                            .child(text("up".to_owned()).font(type_::LABEL).color(theme.text))
                            .on_press(move |state: &mut AppState| state.move_workflow_step(index, true)),
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
                            .on_press(move |state: &mut AppState| state.move_workflow_step(index, false)),
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
                                .key(format!("run-{}-{index}", if result.ok { "ok" } else { "fail" }))
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
        .child(text("Dashboards".to_owned()).font(type_::TITLE).color(theme.text))
        .child(
            row()
                .key("widget-form")
                .w_full()
                .h(28.0)
                .gap(8.0)
                .center_y()
                .child(
                    text_input(&state.widget_title, type_::LABEL)
                        .key("widget-title-input")
                        .w(160.0)
                        .h(24.0)
                        .on_edit(|state: &mut AppState| &mut state.widget_title),
                )
                .child(
                    text_input(&state.widget_query, type_::LABEL)
                        .key("widget-query-input")
                        .w(260.0)
                        .h(24.0)
                        .on_edit(|state: &mut AppState| &mut state.widget_query),
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
                .child(text(error.clone()).font(type_::LABEL).color(theme.text_secondary)),
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
                .child(text("No widgets yet".to_owned()).font(type_::TITLE).color(theme.text))
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
                            text(format!("#{} · {} · {}", widget.id, widget.title, widget.kind))
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
        Some(WidgetBarsState { error: Some(error), .. }) => Some(error.clone()),
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
            .child(text(sentence).font(type_::LABEL).color(theme.text_secondary));
    }
    let bars = state
        .widget_bars
        .get(&id)
        .and_then(|entry| entry.bars.clone())
        .unwrap_or_default();
    let max = bars.iter().map(|(_, value)| *value).max().unwrap_or(1).max(1);
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
pub(crate) fn tools_page(state: &AppState, theme: tok::Theme, dark: bool) -> El {
    let entry = |key: &str, label: &str, tool: Tool, active: bool| {
        row()
            .key(key)
            .h(24.0)
            .px(10.0)
            .center_y()
            .radius(6.0)
            .fill(if active { tok::accent(0, true) } else { theme.control_off })
            .child(text(label).font(type_::LABEL).color(theme.text))
            .on_press(move |state: &mut AppState| state.set_tool(tool))
    };
    col()
        .key("tools-page")
        .w_full()
        .gap(8.0)
        .child(
            row()
                .key("tool-switcher")
                .w_full()
                .h(28.0)
                .gap(6.0)
                .center_y()
                .child(text("Data tools".to_owned()).font(type_::TITLE).color(theme.text))
                .child(entry("tool-sync", "Sync", Tool::Sync, state.tool == Tool::Sync))
                .child(entry("tool-compare", "Compare", Tool::Compare, state.tool == Tool::Compare))
                .child(entry("tool-transfer", "Transfer", Tool::Transfer, state.tool == Tool::Transfer))
                .child(entry("tool-search", "Search values", Tool::ValueSearch, state.tool == Tool::ValueSearch)),
        )
        .child(match state.tool {
            Tool::Sync => sync_tool(state, theme, dark),
            Tool::Compare => compare_tool(state, theme),
            Tool::Transfer => transfer_tool(state, theme),
            Tool::ValueSearch => value_search_tool(state, theme),
        })
}
