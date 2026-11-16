//! View functions for the DataZen Rune UI.
#![allow(unused_imports)]

use super::*;

use crate::constants::*;
use crate::icons::{self, section_icon, ICON_PANEL_LEFT_CLOSE, ICON_PANEL_LEFT_OPEN};
use crate::state_def::{AppState, StructureState, WidgetBarsState};
use crate::types::{Section, TableView, Tool};
use crate::{splitter, sql_editor};
use datazen_ui_api::{
    ColumnDraft, ConnectionProfile, DashboardWidget, MemoryWorkspace, ProgressSnapshot, QueryApi,
    Setting, SettingsApi, TabKind, Workflow, WorkflowStepResult, WorkspaceApi,
};
use rune::tok::{self, type_};
use rune::{col, oklch, row, text, text_input, Color, Cx, El};
use rune::{FileDialogOptions, TextEdit, Tx};

pub(crate) fn rail_item(state: &AppState, section: Section, expanded: bool, dark: bool) -> El {
    let theme = tok::theme(dark);
    let selected = state.active == section;
    let bg = if selected {
        tok::accent(0, dark).alpha(0.2)
    } else {
        Color::TRANSPARENT
    };
    let fg = if selected {
        tok::accent(0, dark)
    } else {
        theme.text_secondary
    };
    let mut item = row()
        .key(format!("rail-{}", section.key()))
        .w_full()
        .h(40.0)
        .px(if expanded { 12.0 } else { 0.0 })
        .center_y()
        .gap(if expanded { 10.0 } else { 0.0 })
        .radius(0.0)
        .fill(bg)
        .role("tab")
        .label(section.label())
        .on_press(move |state: &mut AppState| state.select(section));
    if !selected {
        item = item.hover(theme.hover);
    }
    if expanded {
        item = item
            .child(rune::icon(section_icon(section), 16.0).color(fg))
            .child(
                text(section.label())
                    .font(type_::LABEL)
                    .color(fg)
                    .grow(1.0)
                    .min_w(0.0)
                    .truncate(),
            );
    } else {
        item = item
            .justify_center()
            .child(rune::icon(section_icon(section), 16.0).color(fg));
    }
    item
}

pub(crate) fn rail_view(state: &AppState, dark: bool) -> El {
    let theme = tok::theme(dark);
    let expanded = state.sidebar_expanded();
    let width = state.sidebar_rail_width() as f32;
    // React WorkspaceModeSidebar top group: Databases, Workflow, Dashboard,
    // Workspace, Extensions.
    let mut items = col().w_full().gap(2.0);
    for section in [
        Section::Connections,
        Section::Workflows,
        Section::Dashboards,
        Section::Workspace,
        Section::Wapps,
    ] {
        items = items.child(rail_item(state, section, expanded, dark));
    }
    let toggle_icon = if expanded {
        &ICON_PANEL_LEFT_CLOSE
    } else {
        &ICON_PANEL_LEFT_OPEN
    };
    let toggle = row()
        .key("rail-toggle")
        .w_full()
        .h(40.0)
        .radius(0.0)
        .hover(theme.hover)
        .label(if expanded {
            "Collapse sidebar"
        } else {
            "Expand sidebar"
        })
        .on_press(|state: &mut AppState| state.toggle_sidebar_expanded());
    let toggle = if expanded {
        toggle
            .px(12.0)
            .gap(10.0)
            .center_y()
            .child(rune::icon(toggle_icon, 16.0).color(theme.text_secondary))
            .child(
                text("Collapse sidebar")
                    .font(type_::LABEL)
                    .color(theme.text_secondary)
                    .grow(1.0)
                    .min_w(0.0)
                    .truncate(),
            )
    } else {
        toggle
            .justify_center()
            .center_y()
            .child(rune::icon(toggle_icon, 16.0).color(theme.text_secondary))
    };
    let bottom = col()
        .key("rail-bottom")
        .w_full()
        .gap(2.0)
        .child(rail_item(state, Section::Settings, expanded, dark))
        .child(toggle);
    col()
        .key("datazen-rail")
        .w(width)
        .h_full()
        .py(0.0)
        .gap(0.0)
        .fill(theme.card)
        .border_sides([0.0, 1.0, 0.0, 0.0], theme.card_border)
        .child(items)
        .child(col().w_full().grow(1.0))
        .child(bottom)
}
pub(crate) fn content_toolbar(state: &AppState, theme: tok::Theme) -> El {
    let mut bar = row()
        .key("content-toolbar")
        .w_full()
        .h(48.0)
        .px(12.0)
        .gap(8.0)
        .center_y()
        .fill(theme.bg);

    // Section title/context on the left.
    bar = bar.child(
        text(state.active.label())
            .key("content-toolbar-title")
            .font(type_::LABEL)
            .color(theme.text_secondary),
    );
    bar = bar.child(col().grow(1.0));

    // Section-specific actions (wired).
    match state.active {
        Section::Workspace => {
            bar = bar.child(
                row()
                    .key("toolbar-new-query")
                    .h(32.0)
                    .px(10.0)
                    .gap(6.0)
                    .center_y()
                    .radius(6.0)
                    .fill(theme.control_off)
                    .label("New query")
                    .on_press(|state: &mut AppState| {
                        state.open_query_tab("Query");
                    })
                    .child(text("New query").font(type_::LABEL).color(theme.text)),
            );
        }
        Section::Connections => {}
        Section::Settings => {}
        Section::Tools => {}
        Section::Dashboards => {}
        Section::Workflows => {}
        Section::Wapps => {}
    }
    bar
}

/// The session object tree: databases, schemas and tables from the port's
/// catalog, with the tree's collapsed nodes kept in UI state.
/// L5: workspace panel tab strip (48pt). A tablist with roving keyboard
/// focus (Arrow/Home/End via focus_group), active underline, and a close
/// control per tab. Keys `tab-strip` / `tab-{id}` / `tab-close-{id}` are
/// kept for existing interactions and tests.
pub(crate) fn tab_strip(state: &AppState, theme: tok::Theme) -> El {
    let mut strip = row()
        .key("tab-strip")
        .role("tablist")
        .focus_group()
        .w_full()
        .h(48.0)
        .center_y()
        .fill(theme.bg);
    for tab in state.tabs.tabs() {
        let id = tab.id;
        let selected = state.tabs.selected().is_some_and(|open| open.id == id);
        let underline = if selected {
            tok::accent(0, true)
        } else {
            Color::TRANSPARENT
        };
        let tab_item = row()
            .key(format!("tab-{id}"))
            .role("tab")
            .label(tab.title.clone())
            .focusable(true)
            .h(48.0)
            .px(12.0)
            .gap(8.0)
            .center_y()
            .fill(if selected {
                theme.control_off
            } else {
                theme.bg
            })
            .on_press(move |state: &mut AppState| {
                state.select_tab(id);
            })
            .child(text(tab.title.clone()).font(type_::LABEL).color(theme.text))
            .child(
                row()
                    .key(format!("tab-close-{id}"))
                    .w(20.0)
                    .h(20.0)
                    .center()
                    .radius(4.0)
                    .label(format!("Close {}", tab.title))
                    .focusable(true)
                    .child(text("×").font(type_::LABEL).color(theme.text_secondary))
                    .on_press(move |state: &mut AppState| {
                        state.close_tab(id);
                    }),
            )
            .child(
                col()
                    .key(format!("tab-underline-{id}"))
                    .absolute(0.0, 46.0)
                    .w_full()
                    .h(2.0)
                    .fill(underline),
            );
        strip = strip.child(tab_item);
    }
    strip
}

/// L5: 48pt content toolbar per section (action row). Actions are those
/// already wired in the Rune UI for the active section.
/// L6: 40pt content status bar — replaces the migration preview banner.
/// Shows connection state, context counts (table/columns/rows) when available,
/// and shortcut hints.
pub(crate) fn content_status_bar(state: &AppState, theme: tok::Theme) -> El {
    let connected = state.session_open();
    let dot = if connected {
        tok::accent(2, true)
    } else {
        theme.text_secondary
    };

    // Resolve connection label when a desktop session is known.
    let connection_label = state.desktop_connection.as_ref().and_then(|id| {
        state
            .desktop_connections
            .as_ref()
            .and_then(|list| list.iter().find(|c| &c.id == id))
            .map(|c| c.name.clone())
    });

    // Center context: database/table/columns/rows derived from current state.
    let mut center_parts: Vec<String> = Vec::new();
    if let Some(name) = connection_label {
        if !name.is_empty() {
            center_parts.push(name);
        }
    }
    if let Some(tab) = state.tabs.selected() {
        if tab.kind == datazen_ui_api::TabKind::Table {
            if !tab.title.is_empty() {
                center_parts.push(tab.title.clone());
            }
            if state.grid.has_page() {
                if state.grid.cols > 0 {
                    center_parts.push(format!("{} cols", state.grid.cols));
                }
                if state.grid.total_rows() > 0 {
                    center_parts.push(format!("{} rows", state.grid.total_rows()));
                }
            }
        } else if tab.kind == datazen_ui_api::TabKind::Query {
            center_parts.push("Query".to_owned());
            if let Some(result) = &state.result_grid {
                if result.has_page() {
                    if result.cols > 0 {
                        center_parts.push(format!("{} cols", result.cols));
                    }
                    if result.total_rows() > 0 {
                        center_parts.push(format!("{} rows", result.total_rows()));
                    }
                }
            }
        }
    }
    let center_text = if center_parts.is_empty() {
        state.active.label().to_owned()
    } else {
        center_parts.join(" · ")
    };

    row()
        .key("content-status-bar")
        .w_full()
        .h(40.0)
        .px(12.0)
        .center_y()
        .fill(theme.control_off)
        .child(
            row()
                .gap(8.0)
                .center_y()
                .child(
                    col()
                        .key("status-connection-dot")
                        .w(8.0)
                        .h(8.0)
                        .radius(4.0)
                        .fill(dot),
                )
                .child(
                    text(if connected {
                        "Connected"
                    } else {
                        "Disconnected"
                    })
                    .key("status-connection-label")
                    .font(type_::LABEL)
                    .color(theme.text),
                ),
        )
        .child(
            row().grow(1.0).center_y().justify_center().child(
                text(center_text)
                    .key("status-context")
                    .font(type_::LABEL)
                    .color(theme.text_secondary),
            ),
        )
        .child(
            row().gap(8.0).center_y().child(
                text("⌘N New query · ⌘W Close")
                    .key("status-shortcuts")
                    .font(type_::LABEL)
                    .color(theme.text_secondary),
            ),
        )
}
