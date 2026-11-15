//! View functions for the DataZen Rune UI.
#![allow(unused_imports)]

use super::*;

use crate::constants::*;
use crate::icons::{self, ICON_CHEVRON_LEFT, ICON_CHEVRON_RIGHT, section_icon};
use crate::state_def::{AppState, StructureState, WidgetBarsState};
use crate::types::{Section, TableView, Tool};
use crate::{splitter, sql_editor};
use datazen_ui_api::{ColumnDraft, ConnectionProfile, DashboardWidget, MemoryWorkspace, ProgressSnapshot, QueryApi, Setting, SettingsApi, TabKind, Workflow, WorkflowStepResult, WorkspaceApi};
use rune::{FileDialogOptions, TextEdit, Tx};
use rune::tok::{self, type_};
use rune::{Color, Cx, El, col, oklch, row, text, text_input};

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
/// The workspace is driven by the tabs: a query tab shows the SQL editor above the
/// results, a loaded table page shows the grid, and anything else is an empty state.
/// The old standalone editor and demo-grid sections are no longer product pages.
pub(crate) fn workspace_body(state: &AppState, theme: tok::Theme, dark: bool) -> El {
    if query_selected(state) {
        return query_workspace(state, theme, dark);
    }
    if let Some((database, schema)) = &state.er_schema {
        return er_view(state, database, schema, theme, dark);
    }
    if state.grid.has_page() {
        return table_workspace(state, theme, dark);
    }
    workspace_empty(theme)
}

/// The ER page: one box per table of the open schema. Wide tables (more than 12
/// columns) draw collapsed, showing the column count. Boxes drag; their
/// positions live in UI state.
pub(crate) fn workspace_empty(theme: tok::Theme) -> El {
    col()
        .key("workspace-empty")
        .w_full()
        .h(220.0)
        .px(20.0)
        .py(20.0)
        .gap(12.0)
        .radius(12.0)
        .fill(theme.control_off)
        .child(text("No tab open").font(type_::TITLE).color(theme.text))
        .child(
            text("Open a table from the session object tree, or start a new query from the sidebar.")
                .font(type_::LABEL)
                .color(theme.text_secondary)
                .w_full(),
        )
}
pub(crate) fn table_workspace(state: &AppState, theme: tok::Theme, dark: bool) -> El {
    let title = state.tabs.selected().map(|tab| tab.title.clone()).unwrap_or_default();
    let _ = &title;
    let body = match state.table_view {
        TableView::Data => crate::data_grid::panel(state, theme.text, theme.text_secondary, theme.control_off, dark),
        TableView::Ddl => ddl_view(state, &title, theme),
        TableView::Columns => columns_view(state, &title, theme),
        TableView::Indexes => indexes_view(state, &title, theme),
        TableView::ForeignKeys => foreign_keys_view(state, &title, theme),
    };
    col()
        .key("table-workspace")
        .w_full()
        .gap(8.0)
        .child(view_switcher(state, theme))
        .child(body)
}
pub(crate) fn view_switcher(state: &AppState, theme: tok::Theme) -> El {
    let entry = |key: &str, label: &str, view: TableView, active: bool| {
        row()
            .key(key)
            .h(24.0)
            .px(10.0)
            .center_y()
            .radius(6.0)
            .fill(if active { tok::accent(0, true) } else { theme.control_off })
            .child(text(label).font(type_::LABEL).color(theme.text))
            .on_press(move |state: &mut AppState| state.set_table_view(view))
    };
    row()
        .key("table-view-switcher")
        .w_full()
        .h(28.0)
        .gap(6.0)
        .center_y()
        .child(entry("view-data", "Data", TableView::Data, state.table_view == TableView::Data))
        .child(entry("view-columns", "Columns", TableView::Columns, state.table_view == TableView::Columns))
        .child(entry("view-indexes", "Indexes", TableView::Indexes, state.table_view == TableView::Indexes))
        .child(entry("view-fks", "Foreign keys", TableView::ForeignKeys, state.table_view == TableView::ForeignKeys))
        .child(entry("view-ddl", "DDL", TableView::Ddl, state.table_view == TableView::Ddl))
}

/// E1: the honest state of one table's structure read — a read in flight,
/// its failure sentence, or the "not loaded" hint for an entry that never
/// landed. None when the four segments are there to draw.
pub(crate) fn query_workspace(state: &AppState, theme: tok::Theme, dark: bool) -> El {
    // N1: a query job in flight says so above whatever the area shows —
    // the placeholder before the first result, the stale grid of the run
    // before it.
    let loading = state.query_loading.is_some().then(|| {
        row()
            .key("query-loading")
            .w_full()
            .h(20.0)
            .center_y()
            .child(text("Running…".to_owned()).font(type_::LABEL).color(theme.text_secondary))
    });
    let results = match &state.result_grid {
        Some(grid) => crate::data_grid::result_panel(
            grid,
            crate::data_grid::GridSlot::Result,
            theme.text,
            theme.text_secondary,
            theme.control_off,
            dark,
        ),
        None => col()
            .key("query-results")
            .w_full()
            .h(120.0)
            .px(12.0)
            .py(8.0)
            .radius(8.0)
            .fill(theme.control_off)
            .child(text("No results").font(type_::LABEL).color(theme.text_secondary)),
    };
    col()
        .key("query-tab")
        .w_full()
        .gap(8.0)
        .child(crate::sql_editor::panel(&state.sql, theme.text, theme.text_secondary, dark))
        .child(
            row()
                .key("query-toolbar")
                .w_full()
                .h(28.0)
                .gap(8.0)
                .center_y()
                .child(
                    row()
                        .key("sql-run")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text("Run").font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.run_query()),
                )
                .child(
                    row()
                        .key("sql-cancel")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text("Cancel").font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.cancel_query()),
                )
                .child(
                    text("Command+Enter runs the statement at the cursor")
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                ),
        )
        .children(loading)
        .child(results)
}
pub(crate) fn query_selected(state: &AppState) -> bool {
    state.tabs.selected().is_some_and(|tab| tab.kind == TabKind::Query)
}
