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

pub(crate) fn export_section(state: &AppState, theme: tok::Theme) -> El {
    let mut section = col()
        .key("export-section")
        .w_full()
        .gap(6.0)
        .child(text("Export".to_owned()).font(type_::TITLE).color(theme.text))
        .child(
            row()
                .key("export-form")
                .w_full()
                .h(28.0)
                .gap(8.0)
                .center_y()
                .child(
                    row()
                        .key("export-format")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(
                            text(format!("format: {}", state.export_format()))
                                .font(type_::LABEL)
                                .color(theme.text),
                        )
                        .on_press(|state: &mut AppState| state.cycle_export_format()),
                )
                .child(
                    text_input(&state.export_range, type_::LABEL)
                        .key("export-range-input")
                        .w(220.0)
                        .h(24.0)
                        .on_edit(|state: &mut AppState| &mut state.export_range),
                )
                .child(
                    row()
                        .key("export-start")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text("Export").font(type_::LABEL).color(theme.text))
                        .on_press_tx(|state: &mut AppState, tx: &mut Tx| state.begin_export_form(tx)),
                ),
        );
    if let Some(error) = &state.export_error {
        section = section.child(
            row()
                .key("export-error")
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(text(error.clone()).font(type_::LABEL).color(theme.text_secondary)),
        );
    }
    if let Some(job) = state.export_job {
        section = section.child(
            row()
                .key("export-job")
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(text(format!("job #{job}")).font(type_::LABEL).color(theme.text_secondary))
                .child(
                    row()
                        .key("export-refresh")
                        .h(20.0)
                        .px(6.0)
                        .center_y()
                        .radius(4.0)
                        .fill(theme.control_off)
                        .child(text("Refresh").font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.refresh_export_progress()),
                ),
        );
    }
    if let Some(snapshot) = &state.export_snapshot {
        section = section.child(progress_panel(snapshot, "export", theme));
    }
    section
}

/// One job's progress drawn from a single snapshot: status, percent, the log.
/// Export and backup share this component.
pub(crate) fn progress_panel(snapshot: &ProgressSnapshot, prefix: &str, theme: tok::Theme) -> El {
    let mut panel = col()
        .key(format!("{prefix}-progress"))
        .w_full()
        .gap(4.0)
        .px(8.0)
        .py(6.0)
        .radius(8.0)
        .fill(theme.control_off)
        .child(
            row()
                .key(format!("{prefix}-status"))
                .w_full()
                .h(24.0)
                .center_y()
                .child(
                    text(format!("{} · {}%", snapshot.status, snapshot.percent))
                        .font(type_::LABEL)
                        .color(theme.text),
                ),
        );
    for (index, line) in snapshot.log.iter().enumerate() {
        panel = panel.child(
            row()
                .key(format!("{prefix}-log-{index}"))
                .w_full()
                .h(18.0)
                .center_y()
                .child(
                    text(line.clone())
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                ),
        );
    }
    panel
}

/// L5: workspace panel tab bar (48pt). A tablist with roving keyboard
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
        let underline = if selected { tok::accent(0, true) } else { Color::TRANSPARENT };
        let tab_item = row()
            .key(format!("tab-{id}"))
            .role("tab")
            .label(tab.title.clone())
            .focusable(true)
            .h(48.0)
            .px(12.0)
            .gap(8.0)
            .center_y()
            .fill(if selected { theme.control_off } else { theme.bg })
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
pub(crate) fn object_tree(state: &AppState, theme: tok::Theme) -> El {
    let catalog = state.catalog();
    let filter = state.navigator_filter.text.trim().to_lowercase();
    let mut rows: Vec<El> = Vec::new();
    for database in &catalog.databases {
        let db_match = filter.is_empty() || database.name.to_lowercase().contains(&filter);
        let mut schema_rows: Vec<El> = Vec::new();
        let mut db_has_match = db_match;
        for schema in &database.schemas {
            let schema_match = db_match || schema.name.to_lowercase().contains(&filter);
            let mut table_rows: Vec<El> = Vec::new();
            for name in &schema.tables {
                if !filter.is_empty()
                    && !db_match
                    && !schema_match
                    && !name.to_lowercase().contains(&filter)
                {
                    continue;
                }
                let title = name.clone();
                table_rows.push(
                    row()
                        .key(format!("tree-table-{name}"))
                        .w_full()
                        .h(28.0)
                        .px(32.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text(name.clone()).font(type_::LABEL).color(theme.text))
                        .on_press(move |state: &mut AppState| {
                            state.open_table_tab(&title);
                        }),
                );
            }
            if table_rows.is_empty() && !(filter.is_empty() || schema_match) {
                continue;
            }
            db_has_match = true;
            let schema_path = format!("{}/{}", database.name, schema.name);
            let schema_key = format!("tree-schema-{}-{}", database.name, schema.name);
            let collapsed = state.collapsed.contains(&schema_path) && filter.is_empty();
            schema_rows.push(
                row()
                    .key(schema_key)
                    .w_full()
                    .h(28.0)
                    .px(20.0)
                    .center_y()
                    .radius(6.0)
                    .fill(theme.control_off)
                    .child(
                        text(format!("{} · schema", schema.name))
                            .font(type_::LABEL)
                            .color(theme.text_secondary),
                    )
                    .on_press(move |state: &mut AppState| state.toggle_expand(&schema_path)),
            );
            if !collapsed {
                schema_rows.extend(table_rows);
            }
        }
        if !db_has_match && !filter.is_empty() {
            continue;
        }
        let db_path = database.name.clone();
        let db_collapsed = state.collapsed.contains(&database.name) && filter.is_empty();
        rows.push(
            row()
                .key(format!("tree-db-{}", database.name))
                .w_full()
                .h(28.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(
                    text(format!("{} · database", database.name))
                        .font(type_::LABEL)
                        .color(theme.text),
                )
                .on_press(move |state: &mut AppState| state.toggle_expand(&db_path)),
        );
        if !db_collapsed {
            rows.extend(schema_rows);
        }
    }
    if rows.is_empty() {
        let msg = if filter.is_empty() {
            "No objects"
        } else {
            "No matches"
        };
        rows.push(
            row()
                .key("object-tree-empty")
                .w_full()
                .h(28.0)
                .px(8.0)
                .center_y()
                .child(text(msg).font(type_::LABEL).color(theme.text_secondary)),
        );
    }
    let count = rows.len() as f32;
    let height = if count == 0.0 { 0.0 } else { count * 28.0 + (count - 1.0) * 4.0 };
    let mut tree = col().key("object-tree").w_full().h(height).gap(4.0);
    for row in rows {
        tree = tree.child(row);
    }
    tree
}

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

pub(crate) fn open_query_button(theme: tok::Theme) -> El {
    row()
        .key("open-query")
        .w_full()
        .h(32.0)
        .px(8.0)
        .center_y()
        .radius(6.0)
        .fill(theme.control_off)
        .child(text("New query").font(type_::LABEL).color(theme.text))
        .on_press(|state: &mut AppState| {
            state.open_query_tab("Query");
        })
}

pub(crate) fn er_button(state: &AppState, theme: tok::Theme) -> El {
    if !state.session_open() {
        return row().key("er-button-absent").h(0.0);
    }
    row()
        .key("er-button")
        .w_full()
        .h(28.0)
        .px(8.0)
        .center_y()
        .radius(6.0)
        .fill(theme.control_off)
        .child(text("ER diagram").font(type_::LABEL).color(theme.text))
        .on_press(|state: &mut AppState| state.open_er())
}

pub(crate) fn refresh_button(state: &AppState, theme: tok::Theme) -> El {
    if !state.session_open() {
        return row().key("tree-refresh-absent").h(0.0);
    }
    row()
        .key("tree-refresh")
        .w_full()
        .h(28.0)
        .px(8.0)
        .center_y()
        .radius(6.0)
        .fill(theme.control_off)
        .child(text("Refresh catalog").font(type_::LABEL).color(theme.text))
        .on_press(|state: &mut AppState| state.refresh_catalog())
}

pub(crate) fn disconnect_button(state: &AppState, theme: tok::Theme) -> El {
    if !state.session_open() {
        return row().key("disconnect-absent").h(0.0);
    }
    row()
        .key("disconnect")
        .w_full()
        .h(32.0)
        .px(8.0)
        .center_y()
        .radius(6.0)
        .fill(theme.control_off)
        .child(text("Disconnect").font(type_::LABEL).color(theme.text))
        .on_press(|state: &mut AppState| state.disconnect())
}
