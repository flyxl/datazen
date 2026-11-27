//! View functions for the DataZen Rune UI.
#![allow(unused_imports)]

use super::*;
use crate::i18n::t;

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
use rune::{col, row, text, Color, Cx, El};

use crate::ui::palette;
use rune::{FileDialogOptions, TextEdit, Tx};

// The windows/workspace subtree ports. One mod line per ported React file;
// the workspace/ directory mirrors the React layout (WappIcon.tsx is its
// first port — the manifest-icon-or-puzzle glyph the wapp page shell,
// default cards, navigator and tab bar reuse).
mod native_explain;
mod native_query_export;
mod query_history;
pub mod wapp_icon;
// T067: the host-side shell around a sandboxed wapp page — the lazy entry
// resolve with its per-wapp cache, the 10s load watchdog, the reload/retry
// recovery and the loading/missing/ready body branches.
pub mod wapp_page_shell;
// T111: the workspace mode's default right-hand view — the wapp-page card
// grid (icon well, title, description, version byline) and the empty
// state's jump-to-extensions shortcut.
pub mod workspace_default_cards;
// T145: the workspace mode's left rail — one item per page contributed by
// an enabled wapp (icon, title, description) with the active tab tinted,
// and the empty state's jump-to-extensions escape hatch.
pub mod workspace_navigator;
// T174: the workspace tab strip — one entry per open wapp page tab (icon,
// truncated title, hover-revealed close) with the active tab's accent
// underline; renders nothing while no tab is open.
pub mod workspace_tab_bar;
// T200: the workspace mode's capstone layout — the navigator rail, the
// resize seam, the tab strip over the default card grid or the stacked
// preserved shells, plus the three host effects (the initial wapp load,
// the external-refresh diff-close, the `wapps:open-page` deep link).
pub mod workspace_view;
// T224: the workspace pages derivation — the flattened enabled-wapp ×
// contributed-page rows (install order, per-page title fallback) plus the
// `wapps:open-page` event name/payload, the tab key builder and the
// resolve-then-open `openWappPage` the navigator, cards and deep link share.
pub mod workspace_pages;

pub fn page_body(
    state: &AppState,
    theme: tok::Theme,
    dark: bool,
    layer: &mut Vec<El>,
) -> El {
    match state.active {
        Section::Connections => connection_list(state, theme, dark),
        Section::Workspace => workspace_body(state, theme, dark, layer),
        Section::Settings => settings_page(state, theme, dark),
        Section::Tools => tools_page(state, theme, dark),
        Section::Dashboards => dashboards_page(state, theme, dark),
        // T145: the workflow window's page port — the lite editor that stood
        // here until this port swapped the whole section.
        Section::Workflows => workflow_page(state, theme, dark),
        Section::Wapps => wapp_management_page(state, theme, dark),
    }
}

/// The areas the migration has not reached. Each says so, and says where
/// the product still serves it: the React interface, which stays.
/// The workspace is driven by the tabs: a query tab shows the SQL editor above the
/// results, a loaded table page shows the grid, and anything else is an empty state.
/// The old standalone editor and demo-grid sections are no longer product pages.
pub fn workspace_body(
    state: &AppState,
    theme: tok::Theme,
    dark: bool,
    layer: &mut Vec<El>,
) -> El {
    if let Some(workbench) = &state.native_kv {
        return super::native_kv::workbench_view(workbench, theme, dark);
    }
    let table_selected = state.tabs.selected().is_some_and(|tab| tab.kind == TabKind::Table);
    let content = if query_selected(state) {
        query_workspace(state, theme, dark, layer)
    } else if let Some((database, schema)) = &state.er_schema {
        er_view(state, database, schema, theme, dark)
    } else if let Some(error) = state.table_read_error.as_ref().filter(|_| table_selected) {
        col().key("table-read-error").w_full().padding(16.0).child(
            text(error.clone()).font(type_::LABEL).color(theme.text).w_full(),
        )
    } else if table_selected && state.grid.has_page() {
        // A retained table page is not active after its last tab is closed.
        table_workspace(state, theme, dark)
    } else {
        workspace_empty(theme)
    };
    if !state.ai_chat_open {
        return content;
    }
    // T009-S7: the AI assistant aside (React `ContentViewDrawers`'s
    // right-hand `<aside>`) rides beside the workspace content; the
    // content takes the leftover width and the panel's header selects
    // push their listboxes into the root-level `layer`.
    row()
        .key("workspace-with-ai-chat")
        .w_full()
        .h_full()
        .child(
            col()
                .key("workspace-content")
                .grow(1.0)
                .min_w(0.0)
                .min_h(0.0)
                .child(content),
        )
        .child(ai_chat_aside(state, theme, dark, layer))
}

/// The ER page: one box per table of the open schema. Wide tables (more than 12
/// columns) draw collapsed, showing the column count. Boxes drag; their
/// positions live in UI state.
pub fn workspace_empty(theme: tok::Theme) -> El {
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
pub fn table_workspace(state: &AppState, theme: tok::Theme, dark: bool) -> El {
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
pub fn view_switcher(state: &AppState, theme: tok::Theme) -> El {
    let entry = |key: &str, label: &str, view: TableView, active: bool| {
        row()
            .key(key)
            .h(24.0)
            .px(10.0)
            .center_y()
            .radius(6.0)
            .fill(if active {
                crate::ui::theme::accent(true)
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
            t("connWin.data"),
            TableView::Data,
            state.table_view == TableView::Data,
        ))
        .child(entry(
            "view-columns",
            t("connWin.fields"),
            TableView::Columns,
            state.table_view == TableView::Columns,
        ))
        .child(entry(
            "view-indexes",
            t("connWin.indexes"),
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
            t("objectSearch.ddl"),
            TableView::Ddl,
            state.table_view == TableView::Ddl,
        ))
}

/// E1: the honest state of one table's structure read — a read in flight,
/// its failure sentence, or the "not loaded" hint for an entry that never
/// landed. None when the four segments are there to draw.
pub fn query_workspace(
    state: &AppState, theme: tok::Theme, dark: bool, layer: &mut Vec<El>,
) -> El {
    let sidebar_w =
        splitter::splitter_size(state, QUERY_SIDEBAR_WIDTH_KEY, QUERY_SIDEBAR_WIDTH_DEFAULT) as f32;
    let editor_h =
        splitter::splitter_size(state, QUERY_EDITOR_HEIGHT_KEY, QUERY_EDITOR_HEIGHT_DEFAULT) as f32;

    let sidebar = query_sidebar(state, theme, dark);
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
                text(t("query.running").to_owned())
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

    let selected_result = state.selected_query_result.and_then(|id| {
        state.query_result_tabs.iter().find(|tab| tab.id == id)
    });
    let execution_stats = selected_result.and_then(|tab| tab.execution_stats);
    let selected_error = selected_result.and_then(|tab| {
        tab.error_key.map(|key| (tab.id, key, tab.error_copied_until.is_some()))
    });
    let elapsed_ms = execution_stats.map(|stats| stats.elapsed_ms);
    let command_stats = execution_stats.and_then(|stats| {
        stats.affected_rows.map(|count| (count, stats.elapsed_ms))
    });
    let result_body = match (selected_error, command_stats, &state.result_grid) {
        (Some((tab, key, copied)), _, _) => {
            use crate::ui::{copyable::CopyableError, kit::kit};
            Some(col().key("result-workspace").w_full().px(8.0).py(6.0).child(
                CopyableError::new("query-error-message", t(key))
                    .copy_button(true)
                    .copied(copied)
                    .on_copy_tx(move |s: &mut AppState, tx| {
                        s.mark_query_error_copied(tab, tx.clock());
                    })
                    .build::<AppState>(kit(dark)),
            ))
        }
        (None, Some((affected_rows, execution_time_ms)), _) => {
            use crate::views::result_workspace::execution_summary_card::{
                build, ExecutionSummaryCardVm, SummaryResultVm,
            };
            let summary = ExecutionSummaryCardVm {
                result: SummaryResultVm {
                    rows_affected: Some(affected_rows),
                    execution_time_ms,
                    ..SummaryResultVm::default()
                },
                copied: false,
            };
            Some(col().key("result-workspace").w_full()
                .child(build::<AppState>(&summary, None, None, dark)))
        }
        (None, None, Some(grid)) => Some(result_workspace(
            state, grid, theme, dark, elapsed_ms,
            execution_stats.and_then(|stats| stats.truncated_to),
        )),
        (None, None, None) => None,
    };

    let explain = native_explain::panel(state, theme);
    // A saved/user-adjusted split only applies when something actually owns
    // the space below it. The idle view has a single shortcut footer.
    let split = loading.is_some()
        || state.native_query_selected()
        || !state.query_result_tabs.is_empty()
        || explain.is_some()
        || result_body.is_some();
    let editor = col()
        .key("query-editor-pane")
        .w_full()
        .min_w(0.0)
        .min_h(QUERY_EDITOR_HEIGHT_MIN as f32)
        .border_sides([0.0, 0.0, 1.0, 0.0], theme.separator)
        .child(crate::sql_editor::panel_with_completion_scope(
            &state.sql,
            theme.text,
            theme.text_secondary,
            dark,
            state.catalog_completion_current(),
            state.query_editor_viewport(),
            state.editor_font_size(),
            state.sql_syntax_theme(),
        ));
    let editor = if split { editor.h(editor_h).shrink() } else { editor.grow(1.0) };

    native_query_export::dialog(state, dark, layer);
    let mut main = col()
        .key("query-main")
        .grow(1.0)
        .min_w(0.0)
        .min_h(0.0)
        .h_full()
        .child(query_toolbar(state, theme, build_query_database_picker(state, theme, dark, layer)))
        .child(editor)
        .children(notice);

    if split {
        let editor_split = splitter::splitter_handle(
            state,
            QUERY_EDITOR_HEIGHT_KEY,
            splitter::SplitAxis::Horizontal,
            QUERY_EDITOR_HEIGHT_MIN,
            QUERY_EDITOR_HEIGHT_MAX,
            QUERY_EDITOR_HEIGHT_DEFAULT,
        )
        .h(6.0)
        .fill(theme.separator.alpha(0.25));
        main = main
            .child(editor_split)
            .children(loading)
            .child(query_result_tabs_bar(state, theme))
            .children(explain)
            .children(result_body);
    } else {
        main = main.child(
            row()
                .key("query-results")
                .w_full()
                .h(30.0)
                .center_y()
                .justify_center()
                .child(
                    text(t("query.shortcutHint"))
                        .key("query-idle-hint")
                        .color(crate::ui::kit::kit(dark).c.text_muted)
                        .font(rune::TextStyle::new(12.0, 400.0)),
                ),
        );
    }

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

fn build_query_database_picker(
    state: &AppState, theme: tok::Theme, dark: bool, layer: &mut Vec<El>,
) -> Option<El> {
    use crate::ui::kit::kit;
    use rune_kit::Select;

    let (tab, context) = state.query_database_selection()?;
    if state.native_query.is_some() || state.query_loading.is_some() {
        // Preserve the current value and toolbar geometry while a query owns the target.
        return Some(toolbar_button("query-context-database-current", &context.current.database,
                                   theme));
    }
    let select = Select::new("query-context-database", &context.choices)
        .current(Some(context.current.database.as_str()))
        .width(158.0)
        .open(state.query_database_picker_tab == Some(tab))
        .on_toggle(move |state: &mut AppState| {
            state.query_database_picker_tab =
                (state.query_database_picker_tab != Some(tab)).then_some(tab);
        })
        .on_close(|state: &mut AppState| state.query_database_picker_tab = None)
        .on_choose(move |state: &mut AppState, value: &str, _tx: &mut Tx| {
            state.choose_query_database(tab, value);
        });
    Some(select.build(kit(dark), layer))
}

fn query_toolbar(state: &AppState, theme: tok::Theme, database_picker: Option<El>) -> El {
    let mut bar = row()
        .key("query-toolbar-content")
        .h_full()
        .gap(8.0)
        .px(12.0)
        .center_y()
        .child(toolbar_button("sql-run", t("query.run"), theme).on_press(|s: &mut AppState| s.run_query()));
    if state.desktop.supports_native_query_script() {
        bar = bar.child(
            toolbar_button("sql-run-script", t("query.runSelectScript"), theme)
                .on_press(|s: &mut AppState| s.run_query_script()),
        );
    }
    if !state.desktop.supports_native_query_stop() || state.native_query_selected() {
        bar = bar.child(
            toolbar_button("sql-cancel", t("common.cancel"), theme)
                .on_press(|s: &mut AppState| s.cancel_query()),
        );
    }
    bar = bar.children(database_picker).child(
            toolbar_button("sql-format", t("query.format"), theme)
                .on_press(|s: &mut AppState| s.format_query()),
        )
        // React's query error panel has a separate AI action,
        // t("query.explainError"); the toolbar's Explain is explain.title.
        .child(
            toolbar_button("sql-explain", t("explain.title"), theme)
                .on_press(|s: &mut AppState| s.explain_query()),
        )
        .child(
            toolbar_button("sql-tx-begin", t("query.beginTx"), theme)
                .on_press(|s: &mut AppState| s.query_transaction("Begin transaction")),
        )
        .child(
            toolbar_button("sql-tx-commit", t("query.commitTx"), theme)
                .on_press(|s: &mut AppState| s.query_transaction("Commit")),
        )
        .child(
            toolbar_button("sql-tx-rollback", t("query.rollbackTx"), theme)
                .on_press(|s: &mut AppState| s.query_transaction("Rollback")),
        );
    if state.native_query_selected() || state.query_loading.is_some() {
        bar = bar.child(
            text("Command+Enter runs the statement at the cursor")
                .font(type_::LABEL)
                .color(theme.text_secondary),
        );
    }
    // Keep every existing action reachable on narrow panes instead of
    // clipping trailing controls (same geometry/overflow rule as React).
    row()
        .key("query-toolbar")
        .role("toolbar")
        .w_full()
        .min_w(0.0)
        .h(36.0)
        .fill(theme.control_track)
        .border_sides([0.0, 0.0, 1.0, 0.0], theme.separator)
        .scroll_x()
        .child(bar)
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
                .child(text(label).key(format!("query-result-label-{id}")).font(type_::LABEL).color(theme.text))
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

fn query_sidebar(state: &AppState, theme: tok::Theme, dark: bool) -> El {
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
                .on_press(move |s: &mut AppState| s.select_query_sidebar(panel)),
        );
    }

    let content = match state.query_sidebar_panel {
        crate::types::QuerySidebarPanel::Favorites => query_favorites_panel(state, theme),
        crate::types::QuerySidebarPanel::History => query_history::panel(state, theme, dark),
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

fn query_favorites_panel(state: &AppState, theme: tok::Theme) -> El {
    let favorites = state.host_favorites();
    let mut panel = col().key("query-favorites-panel").w_full().gap(4.0);
    if favorites.is_empty() {
        panel = panel.child(
            text(t("query.noFavorites"))
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
    elapsed_ms: Option<u64>,
    truncated_to: Option<u32>,
) -> El {
    let toggle = result_view_toggle(state, theme);
    let find_bar = result_find_bar(state, grid, theme, dark);
    let status = result_status_bar(state, grid, theme, elapsed_ms);
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
        .children(native_query_export::status(state, theme))
        .child(find_bar)
        .child(status)
        .children(truncated_to.map(|limit| {
            let message = t("query.resultTruncated").replace("{limit}", &limit.to_string());
            text(message).key("result-truncated").role("status").font(type_::LABEL)
                .color(crate::ui::kit::kit(dark).c.warning.fg)
        }))
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
                crate::ui::theme::accent(true)
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
            t("chart.viewTable"),
            ResultView::Table,
            state.result_view == ResultView::Table,
        ))
        .child(entry(
            "result-view-chart",
            t("chart.viewChart"),
            ResultView::Chart,
            state.result_view == ResultView::Chart,
        ))
        .children(native_query_export::trigger(state, theme))
}

/// A5: find bar for the query result grid — search, next/prev, match count.
fn result_find_bar(state: &AppState, grid: &crate::data_grid::DataGrid, theme: tok::Theme, dark: bool) -> El {
    let count = grid.find_hit_count();
    let current = grid.find_current_index();
    let count_label = if grid.find_needle().trim().is_empty() {
        String::new()
    } else if count == 0 {
        t("select.noMatches").to_owned()
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
                .child(text(t("resultFind.openLabel")).font(type_::LABEL).color(theme.text))
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

fn result_duration(key: &'static str, elapsed_ms: u64, theme: tok::Theme) -> El {
    text(format!("{elapsed_ms} ms")).key(key).font(type_::LABEL).color(theme.text_secondary)
}

/// A5: status bar for the result workspace (rows/cols/state/selection).
fn result_status_bar(
    state: &AppState,
    grid: &crate::data_grid::DataGrid,
    theme: tok::Theme,
    elapsed_ms: Option<u64>,
) -> El {
    let exec = if state.query_loading.is_some() {
        t("query.running").to_owned()
    } else {
        t("main.ready").to_owned()
    };
    let (r0, r1, c0, c1) = grid.rect();
    let selection = if !grid.has_selection() {
        "none".to_owned()
    } else if grid.focus() == (0, 0)
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
        .gap(12.0)
        .center_y()
        .radius(6.0)
        .fill(theme.control_off)
        .child(
            text(line)
                .key("result-status-text")
                .font(type_::LABEL)
                .color(theme.text_secondary),
        )
        .children(elapsed_ms.map(|ms| result_duration("query-result-duration", ms, theme)))
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
                        .fill(palette::chart_bar_accent(false)),
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

pub fn query_selected(state: &AppState) -> bool {
    state
        .tabs
        .selected()
        .is_some_and(|tab| tab.kind == TabKind::Query)
}
