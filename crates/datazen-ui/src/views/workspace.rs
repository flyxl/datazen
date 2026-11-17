//! View functions for the DataZen Rune UI.
#![allow(unused_imports)]

use super::*;

use crate::constants::*;
use crate::icons::{self, section_icon};
use crate::state_def::{AppState, StructureState, WidgetBarsState};
use crate::types::{ResultView, Section, TableView, Tool};
use crate::ui::field::TextField;
use crate::{splitter, sql_editor};
use datazen_ui_api::{
    ColumnDraft, ConnectionProfile, DashboardWidget, MemoryWorkspace, ProgressSnapshot, QueryApi,
    Setting, SettingsApi, TabKind, Workflow, WorkflowStepResult, WorkspaceApi,
};
use rune::tok::{self, type_};
use rune::{col, oklch, row, text, Color, Cx, El};
use rune::{FileDialogOptions, TextEdit, Tx};

pub(crate) fn page_body(state: &AppState, theme: tok::Theme, dark: bool) -> El {
    match state.active {
        Section::Connections => connection_list(state, theme, dark),
        Section::Workspace => workspace_body(state, theme, dark),
        Section::Settings => settings_page(state, theme, dark),
        Section::Tools => tools_page(state, theme, dark),
        Section::Dashboards => dashboards_page(state, theme, dark),
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
            text(
                "Open a table from the session object tree, or start a new query from the sidebar.",
            )
            .font(type_::LABEL)
            .color(theme.text_secondary)
            .w_full(),
        )
}
pub(crate) fn table_workspace(state: &AppState, theme: tok::Theme, dark: bool) -> El {
    let title = state
        .tabs
        .selected()
        .map(|tab| tab.title.clone())
        .unwrap_or_default();
    let _ = &title;
    let body = match state.table_view {
        TableView::Data => crate::data_grid::panel(
            state,
            theme.text,
            theme.text_secondary,
            theme.control_off,
            dark,
        ),
        TableView::Ddl => ddl_view(state, &title, theme),
        TableView::Columns => columns_view(state, &title, theme, dark),
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
            .fill(if active {
                tok::accent(0, true)
            } else {
                theme.control_off
            })
            .child(text(label).font(type_::LABEL).color(theme.text))
            .on_press(move |state: &mut AppState| state.set_table_view(view))
    };
    row()
        .key("table-view-switcher")
        .w_full()
        .h(28.0)
        .gap(6.0)
        .center_y()
        .child(entry(
            "view-data",
            "Data",
            TableView::Data,
            state.table_view == TableView::Data,
        ))
        .child(entry(
            "view-columns",
            "Columns",
            TableView::Columns,
            state.table_view == TableView::Columns,
        ))
        .child(entry(
            "view-indexes",
            "Indexes",
            TableView::Indexes,
            state.table_view == TableView::Indexes,
        ))
        .child(entry(
            "view-fks",
            "Foreign keys",
            TableView::ForeignKeys,
            state.table_view == TableView::ForeignKeys,
        ))
        .child(entry(
            "view-ddl",
            "DDL",
            TableView::Ddl,
            state.table_view == TableView::Ddl,
        ))
}

/// E1: the honest state of one table's structure read — a read in flight,
/// its failure sentence, or the "not loaded" hint for an entry that never
/// landed. None when the four segments are there to draw.
pub(crate) fn query_workspace(state: &AppState, theme: tok::Theme, dark: bool) -> El {
    let sidebar_w =
        splitter::splitter_size(state, QUERY_SIDEBAR_WIDTH_KEY, QUERY_SIDEBAR_WIDTH_DEFAULT) as f32;
    let editor_h =
        splitter::splitter_size(state, QUERY_EDITOR_HEIGHT_KEY, QUERY_EDITOR_HEIGHT_DEFAULT) as f32;

    let sidebar = query_sidebar(state, theme);
    let sidebar_split = splitter::splitter_handle(
        state,
        QUERY_SIDEBAR_WIDTH_KEY,
        splitter::SplitAxis::Vertical,
        QUERY_SIDEBAR_WIDTH_MIN,
        QUERY_SIDEBAR_WIDTH_MAX,
        QUERY_SIDEBAR_WIDTH_DEFAULT,
    );

    let loading = state.query_loading.is_some().then(|| {
        row()
            .key("query-loading")
            .w_full()
            .h(20.0)
            .center_y()
            .child(
                text("Running…".to_owned())
                    .font(type_::LABEL)
                    .color(theme.text_secondary),
            )
    });

    let notice = state.query_notice.as_ref().map(|msg| {
        row().key("query-notice").w_full().h(20.0).center_y().child(
            text(msg.clone())
                .font(type_::LABEL)
                .color(theme.text_secondary),
        )
    });

    let result_tabs = query_result_tabs_bar(state, theme);
    let result_body = match &state.result_grid {
        Some(grid) => result_workspace(state, grid, theme, dark),
        None => col()
            .key("query-results")
            .w_full()
            .h(120.0)
            .px(12.0)
            .py(8.0)
            .radius(8.0)
            .fill(theme.control_off)
            .child(
                text("No results")
                    .font(type_::LABEL)
                    .color(theme.text_secondary),
            ),
    };

    let editor_split = splitter::splitter_handle(
        state,
        QUERY_EDITOR_HEIGHT_KEY,
        splitter::SplitAxis::Horizontal,
        QUERY_EDITOR_HEIGHT_MIN,
        QUERY_EDITOR_HEIGHT_MAX,
        QUERY_EDITOR_HEIGHT_DEFAULT,
    );

    let main = col()
        .key("query-main")
        .grow(1.0)
        .min_w(0.0)
        .gap(6.0)
        .child(
            col()
                .key("query-editor-pane")
                .w_full()
                .min_w(0.0)
                .h(editor_h)
                .child(crate::sql_editor::panel(
                    &state.sql,
                    theme.text,
                    theme.text_secondary,
                    dark,
                )),
        )
        .child(query_toolbar(state, theme))
        .child(editor_split)
        .children(notice)
        .children(loading)
        .child(result_tabs)
        .child(result_body);

    row()
        .key("query-tab")
        .w_full()
        .h_full()
        .gap(0.0)
        .child(
            col()
                .key("query-sidebar-pane")
                .w(sidebar_w)
                .h_full()
                .child(sidebar),
        )
        .child(sidebar_split)
        .child(main)
}

fn toolbar_button(key: &'static str, label: &str, theme: tok::Theme) -> El {
    row()
        .key(key)
        .h(24.0)
        .px(8.0)
        .center_y()
        .radius(6.0)
        .fill(theme.control_off)
        .child(text(label.to_owned()).font(type_::LABEL).color(theme.text))
}

pub(crate) fn query_toolbar(_state: &AppState, theme: tok::Theme) -> El {
    row()
        .key("query-toolbar")
        .w_full()
        .h(28.0)
        .gap(6.0)
        .center_y()
        .child(toolbar_button("sql-run", "Run", theme).on_press(|s: &mut AppState| s.run_query()))
        .child(
            toolbar_button("sql-cancel", "Cancel", theme)
                .on_press(|s: &mut AppState| s.cancel_query()),
        )
        .child(
            toolbar_button("sql-format", "Format", theme)
                .on_press(|s: &mut AppState| s.format_query()),
        )
        .child(
            toolbar_button("sql-explain", "Explain", theme)
                .on_press(|s: &mut AppState| s.explain_query()),
        )
        .child(
            toolbar_button("sql-tx-begin", "Begin", theme)
                .on_press(|s: &mut AppState| s.query_transaction("Begin transaction")),
        )
        .child(
            toolbar_button("sql-tx-commit", "Commit", theme)
                .on_press(|s: &mut AppState| s.query_transaction("Commit")),
        )
        .child(
            toolbar_button("sql-tx-rollback", "Rollback", theme)
                .on_press(|s: &mut AppState| s.query_transaction("Rollback")),
        )
        .child(
            text("Command+Enter runs the statement at the cursor")
                .font(type_::LABEL)
                .color(theme.text_secondary),
        )
}

fn query_result_tabs_bar(state: &AppState, theme: tok::Theme) -> El {
    let mut bar = row()
        .key("query-result-tabs")
        .w_full()
        .h(28.0)
        .gap(4.0)
        .center_y()
        .px(4.0);
    if state.query_result_tabs.is_empty() {
        bar = bar.child(
            text("Results")
                .key("query-result-tabs-empty")
                .font(type_::LABEL)
                .color(theme.text_secondary),
        );
        return bar;
    }
    for tab in &state.query_result_tabs {
        let id = tab.id;
        let selected = state.selected_query_result == Some(id);
        let label = if tab.pinned {
            format!("📌 {}", tab.label)
        } else {
            tab.label.clone()
        };
        bar = bar.child(
            row()
                .key(format!("query-result-tab-{id}"))
                .h(22.0)
                .px(8.0)
                .gap(6.0)
                .center_y()
                .radius(6.0)
                .fill(if selected {
                    theme.card
                } else {
                    theme.control_off
                })
                .child(text(label).font(type_::LABEL).color(theme.text))
                .child(
                    text(if tab.pinned { "Unpin" } else { "Pin" })
                        .key(format!("query-result-pin-{id}"))
                        .font(type_::LABEL)
                        .color(theme.text_secondary)
                        .on_press(move |s: &mut AppState| s.toggle_pin_query_result_tab(id)),
                )
                .child(
                    text("×")
                        .key(format!("query-result-close-{id}"))
                        .font(type_::LABEL)
                        .color(theme.text_secondary)
                        .on_press(move |s: &mut AppState| {
                            let _ = s.close_query_result_tab(id);
                        }),
                )
                .on_press(move |s: &mut AppState| {
                    let _ = s.select_query_result_tab(id);
                }),
        );
    }
    bar
}

fn query_sidebar(state: &AppState, theme: tok::Theme) -> El {
    let mut tabs = row()
        .key("query-sidebar-tabs")
        .w_full()
        .h(28.0)
        .gap(4.0)
        .center_y();
    for panel in crate::types::QuerySidebarPanel::ALL {
        let active = state.query_sidebar_panel == panel;
        tabs = tabs.child(
            row()
                .key(format!(
                    "query-sidebar-tab-{}",
                    panel.label().to_lowercase()
                ))
                .h(22.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(if active {
                    theme.card
                } else {
                    theme.control_off
                })
                .child(
                    text(panel.label().to_owned())
                        .font(type_::LABEL)
                        .color(theme.text),
                )
                .on_press(move |s: &mut AppState| s.query_sidebar_panel = panel),
        );
    }

    let content = match state.query_sidebar_panel {
        crate::types::QuerySidebarPanel::Favorites => query_favorites_panel(state, theme),
        crate::types::QuerySidebarPanel::History => query_history_panel(theme),
        crate::types::QuerySidebarPanel::Snippets => query_snippets_panel(state, theme),
    };

    col()
        .key("query-sidebar")
        .w_full()
        .h_full()
        .gap(6.0)
        .px(8.0)
        .py(8.0)
        .fill(theme.control_off)
        .child(text("Query library").font(type_::TITLE).color(theme.text))
        .child(tabs)
        .child(content)
}

fn query_history_panel(theme: tok::Theme) -> El {
    col().key("query-history-panel").w_full().gap(6.0).child(
        text("Query history is not exposed to the Rune UI in this host.")
            .font(type_::LABEL)
            .color(theme.text_secondary)
            .w_full(),
    )
}

fn query_favorites_panel(state: &AppState, theme: tok::Theme) -> El {
    let favorites = state.host_favorites();
    let mut panel = col().key("query-favorites-panel").w_full().gap(4.0);
    if favorites.is_empty() {
        panel = panel.child(
            text("No favorites yet")
                .key("query-favorites-empty")
                .font(type_::LABEL)
                .color(theme.text_secondary),
        );
        return panel;
    }
    for (index, favorite) in favorites.iter().enumerate() {
        let title = favorite.title.clone();
        panel = panel.child(
            row()
                .key(format!("query-favorite-{index}"))
                .w_full()
                .h(24.0)
                .px(6.0)
                .center_y()
                .radius(6.0)
                .fill(theme.bg)
                .child(text(title).font(type_::LABEL).color(theme.text))
                .on_press(move |s: &mut AppState| s.open_favorite(index)),
        );
    }
    panel
}

fn query_snippets_panel(state: &AppState, theme: tok::Theme) -> El {
    let snippets = state.host_snippets();
    let mut panel = col().key("query-snippets-panel").w_full().gap(4.0);
    if snippets.is_empty() {
        panel = panel.child(
            text("No snippets yet")
                .key("query-snippets-empty")
                .font(type_::LABEL)
                .color(theme.text_secondary),
        );
        return panel;
    }
    for (index, snippet) in snippets.iter().enumerate() {
        let sql = snippet.sql.clone();
        let name = snippet.name.clone();
        panel = panel.child(
            row()
                .key(format!("query-snippet-{index}"))
                .w_full()
                .h(24.0)
                .px(6.0)
                .center_y()
                .radius(6.0)
                .fill(theme.bg)
                .child(text(name).font(type_::LABEL).color(theme.text))
                .on_press(move |s: &mut AppState| s.insert_snippet_sql(&sql)),
        );
    }
    panel
}

/// A5: the result workspace — table/chart toggle, find bar, status and body.
fn result_workspace(
    state: &AppState,
    grid: &crate::data_grid::DataGrid,
    theme: tok::Theme,
    dark: bool,
) -> El {
    let toggle = result_view_toggle(state, theme);
    let find_bar = result_find_bar(state, grid, theme, dark);
    let status = result_status_bar(state, grid, theme);
    let body = match state.result_view {
        ResultView::Table => crate::data_grid::result_panel(
            grid,
            crate::data_grid::GridSlot::Result,
            theme.text,
            theme.text_secondary,
            theme.control_off,
            dark,
        ),
        ResultView::Chart => result_chart_view(grid, theme),
    };
    col()
        .key("result-workspace")
        .w_full()
        .gap(6.0)
        .child(toggle)
        .child(find_bar)
        .child(status)
        .child(body)
}

fn result_view_toggle(state: &AppState, theme: tok::Theme) -> El {
    let entry = |key: &str, label: &str, view: ResultView, active: bool| {
        row()
            .key(key)
            .h(24.0)
            .px(10.0)
            .center_y()
            .radius(6.0)
            .fill(if active {
                tok::accent(0, true)
            } else {
                theme.control_off
            })
            .child(text(label).font(type_::LABEL).color(theme.text))
            .on_press(move |state: &mut AppState| state.set_result_view(view))
    };
    row()
        .key("result-view-toggle")
        .w_full()
        .h(28.0)
        .gap(6.0)
        .center_y()
        .child(entry(
            "result-view-table",
            "Table",
            ResultView::Table,
            state.result_view == ResultView::Table,
        ))
        .child(entry(
            "result-view-chart",
            "Chart",
            ResultView::Chart,
            state.result_view == ResultView::Chart,
        ))
}

/// A5: find bar for the query result grid — search, next/prev, match count.
fn result_find_bar(state: &AppState, grid: &crate::data_grid::DataGrid, theme: tok::Theme, dark: bool) -> El {
    let count = grid.find_hit_count();
    let current = grid.find_current_index();
    let count_label = if grid.find_needle().trim().is_empty() {
        String::new()
    } else if count == 0 {
        "No matches".to_owned()
    } else {
        format!("{current} of {count}")
    };
    row()
        .key("result-find-bar")
        .w_full()
        .h(28.0)
        .gap(6.0)
        .center_y()
        .child(
            TextField::new("result-find-input", &state.result_find)
                .height(24.0)
                .width(220.0)
                .on_edit(|state: &mut AppState| &mut state.result_find)
                .on_submit(|state: &mut AppState, tx| {
                    state.apply_result_find();
                    tx.blur();
                })
                .build(dark),
        )
        .child(
            row()
                .key("result-find-apply")
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(text("Find").font(type_::LABEL).color(theme.text))
                .on_press(|state: &mut AppState| state.apply_result_find()),
        )
        .child(
            row()
                .key("result-find-prev")
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(text("Prev").font(type_::LABEL).color(theme.text))
                .on_press(|state: &mut AppState| state.result_find_prev()),
        )
        .child(
            row()
                .key("result-find-next")
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(text("Next").font(type_::LABEL).color(theme.text))
                .on_press(|state: &mut AppState| state.result_find_next()),
        )
        .child(
            text(count_label)
                .key("result-find-count")
                .font(type_::LABEL)
                .color(theme.text_secondary),
        )
}

/// A5: status bar for the result workspace (rows/cols/state/selection).
fn result_status_bar(state: &AppState, grid: &crate::data_grid::DataGrid, theme: tok::Theme) -> El {
    let exec = if state.query_loading.is_some() {
        "Running…".to_owned()
    } else {
        "Ready".to_owned()
    };
    let (r0, r1, c0, c1) = grid.rect();
    let selection = if grid.focus() == (0, 0)
        && grid.anchor == (0, 0)
        && r0 == 0
        && r1 == 0
        && c0 == 0
        && c1 == 0
    {
        // no explicit multi-selection info beyond focus; show focus cell
        let (fr, fc) = grid.focus();
        format!("r{fr}c{fc}")
    } else {
        format!("r{r0}c{c0}..r{r1}c{c1}")
    };
    let line = format!(
        "{} · {} rows × {} cols · selection {selection}",
        exec, grid.rows, grid.cols
    );
    row()
        .key("result-status-bar")
        .w_full()
        .h(24.0)
        .px(8.0)
        .center_y()
        .radius(6.0)
        .fill(theme.control_off)
        .child(
            text(line)
                .key("result-status-text")
                .font(type_::LABEL)
                .color(theme.text_secondary),
        )
}

/// A5: derive chart bars from a result grid (label, numeric value).
fn result_chart_bars(grid: &crate::data_grid::DataGrid) -> Option<Vec<(String, u32)>> {
    if grid.rows == 0 || grid.cols == 0 {
        return None;
    }
    // Value column: first column with any parseable numeric value.
    let mut value_col: Option<u32> = None;
    for c in 0..grid.cols {
        let has_num = (0..grid.rows).any(|r| grid.cell(r, c).trim().parse::<f64>().is_ok());
        if has_num {
            value_col = Some(c);
            break;
        }
    }
    let value_col = value_col?;
    // Label column: first column different from value, else row index.
    let label_col = if grid.cols > 1 {
        (0..grid.cols).find(|&c| c != value_col).unwrap_or(0)
    } else {
        value_col
    };
    let mut bars = Vec::new();
    for r in 0..grid.rows {
        let label = if label_col == value_col {
            format!("r{r}")
        } else {
            let v = grid.cell(r, label_col);
            if v.is_empty() {
                format!("r{r}")
            } else {
                v
            }
        };
        let value = grid
            .cell(r, value_col)
            .trim()
            .parse::<f64>()
            .unwrap_or(0.0)
            .abs()
            .round() as u32;
        bars.push((label, value));
    }
    Some(bars)
}

/// A5: chart view of the result grid, using the same bar-chart primitive style.
fn result_chart_view(grid: &crate::data_grid::DataGrid, theme: tok::Theme) -> El {
    let Some(bars) = result_chart_bars(grid) else {
        return col()
            .key("result-chart")
            .w_full()
            .h(220.0)
            .px(12.0)
            .py(10.0)
            .radius(8.0)
            .fill(theme.control_off)
            .child(
                text("Chart unavailable: no numeric data".to_owned())
                    .font(type_::LABEL)
                    .color(theme.text_secondary),
            );
    };
    let max = bars
        .iter()
        .map(|(_, value)| *value)
        .max()
        .unwrap_or(1)
        .max(1);
    let mut chart = row()
        .key("result-chart")
        .w_full()
        .h(220.0)
        .gap(10.0)
        .px(8.0)
        .py(8.0)
        .radius(8.0)
        .fill(theme.control_off);
    for (index, (label, value)) in bars.iter().enumerate() {
        let height = if *value == 0 {
            2.0
        } else {
            12.0 + (*value as f32 / max as f32) * 160.0
        };
        let short_label = if label.chars().count() > 8 {
            let mut s: String = label.chars().take(7).collect();
            s.push('…');
            s
        } else {
            label.clone()
        };
        chart = chart.child(
            col()
                .key(format!("result-bar-{index}"))
                .w(56.0)
                .gap(4.0)
                .child(
                    row()
                        .key(format!("result-block-{index}"))
                        .w(34.0)
                        .h(height)
                        .radius(3.0)
                        .fill(oklch(0.72, 0.13, 250.0)),
                )
                .child(
                    text(format!("{short_label} {value}"))
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                ),
        );
    }
    chart
}

pub(crate) fn query_selected(state: &AppState) -> bool {
    state
        .tabs
        .selected()
        .is_some_and(|tab| tab.kind == TabKind::Query)
}
