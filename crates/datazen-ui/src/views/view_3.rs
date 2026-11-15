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

pub(crate) fn sync_tool(state: &AppState, theme: tok::Theme, dark: bool) -> El {
    let mut section = col().key("sync-tool").w_full().gap(6.0);
    match state.sync_step {
        0 => section = section
            .child(
                row()
                    .key("sync-step-1")
                    .w_full()
                    .h(28.0)
                    .gap(8.0)
                    .center_y()
                    .child(
                        row()
                            .key("sync-source")
                            .h(24.0)
                            .px(8.0)
                            .center_y()
                            .radius(6.0)
                            .fill(theme.control_off)
                            .child(
                                text(format!("source: {}", state.sync_source))
                                    .font(type_::LABEL)
                                    .color(theme.text),
                            )
                            .on_press(|state: &mut AppState| state.cycle_sync_source()),
                    )
                    .child(
                        row()
                            .key("sync-target")
                            .h(24.0)
                            .px(8.0)
                            .center_y()
                            .radius(6.0)
                            .fill(theme.control_off)
                            .child(
                                text(format!("target: {}", state.sync_target))
                                    .font(type_::LABEL)
                                    .color(theme.text),
                            )
                            .on_press(|state: &mut AppState| state.cycle_sync_target()),
                    )
                    .child(
                        row()
                            .key("sync-endpoints-next")
                            .h(24.0)
                            .px(8.0)
                            .center_y()
                            .radius(6.0)
                            .fill(theme.control_off)
                            .child(text("Next").font(type_::LABEL).color(theme.text))
                            .on_press(|state: &mut AppState| state.sync_next_endpoints()),
                    ),
            ),
        1 => {
            section = section.child(
                row()
                    .key("sync-step-1-done")
                    .w_full()
                    .h(24.0)
                    .center_y()
                    .child(
                        text(format!("{} → {}", state.sync_source, state.sync_target))
                            .font(type_::LABEL)
                            .color(theme.text_secondary),
                    ),
            );
            let mut tables = col().key("sync-step-2").w_full().gap(4.0);
            for table in state.workspace.connection_tables(&state.sync_source) {
                let selected = state.sync_tables.contains(&table);
                tables = tables.child(
                    row()
                        .key(format!("sync-table-{table}"))
                        .w(220.0)
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(if selected { tok::accent(0, true) } else { theme.control_off })
                        .child(text(table.clone()).font(type_::LABEL).color(theme.text))
                        .on_press(move |state: &mut AppState| state.toggle_sync_table(&table)),
                );
            }
            section = section
                .child(tables)
                .child(
                    row()
                        .key("sync-table-controls")
                        .w_full()
                        .h(28.0)
                        .gap(8.0)
                        .center_y()
                        .child(
                            row()
                                .key("sync-select-all")
                                .h(24.0)
                                .px(8.0)
                                .center_y()
                                .radius(6.0)
                                .fill(theme.control_off)
                                .child(text("Select all").font(type_::LABEL).color(theme.text))
                                .on_press(|state: &mut AppState| state.sync_select_all()),
                        )
                        .child(
                            row()
                                .key("sync-clear")
                                .h(24.0)
                                .px(8.0)
                                .center_y()
                                .radius(6.0)
                                .fill(theme.control_off)
                                .child(text("Clear").font(type_::LABEL).color(theme.text))
                                .on_press(|state: &mut AppState| state.sync_clear_tables()),
                        )
                        .child(
                            row()
                                .key("sync-tables-next")
                                .h(24.0)
                                .px(8.0)
                                .center_y()
                                .radius(6.0)
                                .fill(theme.control_off)
                                .child(text("Next").font(type_::LABEL).color(theme.text))
                                .on_press(|state: &mut AppState| state.sync_next_tables()),
                        ),
                );
        }
        2 => {
            section = section.child(
                row()
                    .key("sync-step-1-done")
                    .w_full()
                    .h(24.0)
                    .center_y()
                    .child(
                        text(format!("{} → {}", state.sync_source, state.sync_target))
                            .font(type_::LABEL)
                            .color(theme.text_secondary),
                    ),
            );
            let mut options = col().key("sync-step-3").w_full().gap(4.0);
            for (index, label) in AppState::SYNC_OPTION_LABELS.iter().enumerate() {
                let on = state.sync_options[index];
                options = options.child(
                    row()
                        .key(format!("sync-option-{index}"))
                        .w(260.0)
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(if on { tok::accent(0, true) } else { theme.control_off })
                        .child(
                            text(format!("{}: {}", if on { "on" } else { "off" }, label))
                                .font(type_::LABEL)
                                .color(theme.text),
                        )
                        .on_press(move |state: &mut AppState| state.toggle_sync_option(index)),
                );
            }
            section = section
                .child(options)
                .child(
                    row()
                        .key("sync-preview-next")
                        .w(120.0)
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text("Preview").font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.sync_show_preview()),
                );
        }
        3 => {
            section = section.child(
                row()
                    .key("sync-step-1-done")
                    .w_full()
                    .h(24.0)
                    .center_y()
                    .child(
                        text(format!("{} → {}", state.sync_source, state.sync_target))
                            .font(type_::LABEL)
                            .color(theme.text_secondary),
                    ),
            );
            if let Some(grid) = &state.sync_preview_grid {
                section = section.child(crate::data_grid::result_panel(
                    grid,
                    crate::data_grid::GridSlot::SyncPreview,
                    theme.text,
                    theme.text_secondary,
                    theme.control_off,
                    dark,
                ));
            }
            section = section.child(
                row()
                    .key("sync-run-controls")
                    .w_full()
                    .h(28.0)
                    .gap(8.0)
                    .center_y()
                    .child(
                        row()
                            .key("sync-run")
                            .h(24.0)
                            .px(8.0)
                            .center_y()
                            .radius(6.0)
                            .fill(theme.control_off)
                            .child(text("Run sync").font(type_::LABEL).color(theme.text))
                            .on_press(|state: &mut AppState| state.sync_run()),
                    )
                    .child(
                        row()
                            .key("sync-refresh")
                            .h(24.0)
                            .px(8.0)
                            .center_y()
                            .radius(6.0)
                            .fill(theme.control_off)
                            .child(text("Refresh").font(type_::LABEL).color(theme.text))
                            .on_press(|state: &mut AppState| state.refresh_sync_progress()),
                    ),
            );
            if let Some(snapshot) = &state.sync_snapshot {
                section = section.child(progress_panel(snapshot, "sync", theme));
            }
        }
        _ => section = section.child(
            row()
                .key("sync-step-1-done")
                .w_full()
                .h(24.0)
                .center_y()
                .child(
                    text(format!("{} → {}", state.sync_source, state.sync_target))
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                ),
        ),
    }
    if let Some(error) = &state.sync_error {
        section = section.child(
            row()
                .key("sync-error")
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(text(error.clone()).font(type_::LABEL).color(theme.text_secondary)),
        );
    }
    section
}

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
pub(crate) fn er_view(state: &AppState, database: &str, schema: &str, theme: tok::Theme, dark: bool) -> El {
    let catalog = state.catalog();
    let tables: Vec<String> = catalog
        .databases
        .iter()
        .find(|db| db.name == database)
        .and_then(|db| db.schemas.iter().find(|node| node.name == schema))
        .map(|node| node.tables.clone())
        .unwrap_or_default();
    let mut canvas = col()
        .key("er-canvas")
        .w_full()
        .h(420.0)
        .radius(8.0)
        .fill(theme.control_off);
    for (index, table) in tables.iter().enumerate() {
        // E1: the ER boxes read the structure cache only — the open asked
        // for every table of the schema, and a box whose read has not
        // landed says so instead of drawing a phantom empty table.
        let columns = state
            .structures
            .get(table)
            .and_then(|entry| entry.data.as_ref())
            .map(|data| data.columns.as_slice());
        let default = ((index % 3) as f32 * 230.0, (index / 3) as f32 * 180.0);
        let (x, y) = state.er_positions.get(table).copied().unwrap_or(default);
        canvas = canvas.child(er_box(table, columns, x, y, theme, dark));
    }
    col()
        .key("er-page")
        .w_full()
        .gap(8.0)
        .child(
            row()
                .key("er-toolbar")
                .w_full()
                .h(28.0)
                .gap(8.0)
                .center_y()
                .child(
                    text(format!("ER · {database}/{schema}"))
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                )
                .child(
                    row()
                        .key("er-close")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text("Close").font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.close_er()),
                ),
        )
        .child(canvas)
}

/// One table box. Collapsed boxes show the column count instead of every
/// column; a box whose structure read has not landed says so (E1).
pub(crate) fn er_box(
    table: &str,
    columns: Option<&[datazen_ui_api::ColumnSpec]>,
    x: f32,
    y: f32,
    theme: tok::Theme,
    dark: bool,
) -> El {
    let collapsed = columns.is_some_and(|columns| columns.len() > 12);
    let table = table.to_owned();
    let drag_table = table.clone();
    let box_fill = if dark { oklch(0.24, 0.03, 250.0) } else { oklch(0.96, 0.01, 250.0) };
    let mut class_box = col()
        .key(format!("er-box-{table}"))
        .absolute(x, y)
        .w(200.0)
        .radius(8.0)
        .fill(box_fill)
        .px(8.0)
        .py(6.0)
        .gap(2.0)
        .child(
            text(table.to_owned())
                .key(format!("er-title-{table}"))
                .font(type_::TITLE)
                .color(theme.text),
        )
        .on_drag(move |state: &mut AppState, drag: &mut rune::Drag, _: &mut Tx| {
            match drag.phase {
                rune::Phase::Down => {
                    state.er_drag = Some((drag_table.clone(), (x, y)));
                }
                rune::Phase::Move => {
                    if let Some((dragged, (start_x, start_y))) = &state.er_drag {
                        if *dragged == drag_table {
                            state
                                .er_positions
                                .insert(drag_table.clone(), ((start_x + drag.dx).max(0.0), (start_y + drag.dy).max(0.0)));
                        }
                    }
                }
                rune::Phase::Up => {
                    state.er_drag = None;
                }
            }
        });
    if collapsed {
        class_box = class_box.child(
            text(format!("{} columns", columns.map_or(0, |columns| columns.len())))
                .key(format!("er-collapsed-{table}"))
                .font(type_::LABEL)
                .color(theme.text_secondary),
        );
    } else if let Some(columns) = columns {
        for (index, column) in columns.iter().enumerate() {
            class_box = class_box.child(
                text(column.name.clone())
                    .key(format!("er-col-{table}-{index}"))
                    .font(type_::LABEL)
                    .color(theme.text_secondary),
            );
        }
    } else {
        class_box = class_box.child(
            text("structure not loaded")
                .key(format!("er-pending-{table}"))
                .font(type_::LABEL)
                .color(theme.text_secondary),
        );
    }
    class_box
}

/// A table tab shows the view switcher above either the data grid or the DDL.
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
pub(crate) fn structure_notice(state: &AppState, table: &str) -> Option<String> {
    match state.structures.get(table) {
        None => Some(
            "the table's structure is not loaded — switch the view or reopen the table".into(),
        ),
        Some(StructureState { loading: true, .. }) => {
            Some("reading the table's structure…".into())
        }
        Some(StructureState { error: Some(error), .. }) => Some(error.clone()),
        Some(StructureState { data: Some(_), .. }) => None,
        Some(StructureState { data: None, .. }) => Some(
            "the table's structure is not loaded — switch the view or reopen the table".into(),
        ),
    }
}

/// One thin notice row for a structure view (E1): the loading, error or
/// not-loaded sentence in the list's own skin.
pub(crate) fn notice_row(key: &str, sentence: &str, theme: tok::Theme) -> El {
    row()
        .key(key.to_owned())
        .w_full()
        .h(28.0)
        .px(8.0)
        .center_y()
        .child(text(sentence.to_owned()).font(type_::LABEL).color(theme.text_secondary))
}

/// The column list: name, type, nullability — one row per column, plus the
/// drafts section. The official list is read-only; drafts are separate.
pub(crate) fn columns_view(state: &AppState, title: &str, theme: tok::Theme) -> El {
    // E1: the cache is the only read — the job's columns segment renders.
    let columns = state
        .structures
        .get(title)
        .and_then(|entry| entry.data.as_ref())
        .map(|data| data.columns.clone())
        .unwrap_or_default();
    let header = |label: &str, width: f32| {
        row()
            .w(width)
            .h(24.0)
            .px(8.0)
            .center_y()
            .child(text(label).font(type_::LABEL).color(theme.text_secondary))
    };
    let mut list = col()
        .key("columns-list")
        .w_full()
        .radius(8.0)
        .fill(theme.control_off)
        .child(
            row()
                .key("columns-header")
                .w_full()
                .h(24.0)
                .child(header("Column", 240.0))
                .child(header("Type", 140.0))
                .child(header("Nullable", 100.0)),
        );
    for (index, column) in columns.iter().enumerate() {
        let cell = |value: &str, width: f32| {
            row()
                .w(width)
                .h(24.0)
                .px(8.0)
                .center_y()
                .child(text(value.to_owned()).font(type_::LABEL).color(theme.text))
        };
        list = list.child(
            row()
                .key(format!("column-row-{index}"))
                .w_full()
                .h(24.0)
                .child(cell(&column.name, 240.0))
                .child(cell(&column.sql_type, 140.0))
                .child(cell(if column.nullable { "yes" } else { "no" }, 100.0)),
        );
    }
    if let Some(sentence) = structure_notice(state, title) {
        list = list.child(notice_row("columns-structure-notice", &sentence, theme));
    }
    let mut list = list.child(column_drafts_section(state, title, theme));
    list
}

/// The drafts area under the column list: inputs, the queued drafts, separate
/// from the official columns.
pub(crate) fn column_drafts_section(state: &AppState, title: &str, theme: tok::Theme) -> El {
    let drafts = state.column_drafts.get(title).cloned().unwrap_or_default();
    let mut section = col()
        .key("column-drafts")
        .w_full()
        .gap(6.0)
        .child(
            row()
                .key("draft-inputs")
                .w_full()
                .h(28.0)
                .gap(8.0)
                .center_y()
                .child(
                    text_input(&state.draft_name, type_::LABEL)
                        .key("draft-name-input")
                        .w(140.0)
                        .h(24.0)
                        .on_edit(|state: &mut AppState| &mut state.draft_name),
                )
                .child(
                    text_input(&state.draft_type, type_::LABEL)
                        .key("draft-type-input")
                        .w(110.0)
                        .h(24.0)
                        .on_edit(|state: &mut AppState| &mut state.draft_type),
                )
                .child(
                    row()
                        .key("draft-add")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text("+ Column").font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.add_column_draft()),
                )
                .child(
                    row()
                        .key("draft-retype")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text("Retype").font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.add_retype_draft()),
                ),
        );
    for (index, draft) in drafts.iter().enumerate() {
        let label = match draft {
            ColumnDraft::AddColumn { name, sql_type } => format!("add {name} {sql_type}"),
            ColumnDraft::ChangeType { column, sql_type } => format!("retype {column} → {sql_type}"),
        };
        section = section.child(
            row()
                .key(format!("draft-row-{index}"))
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(text(label).font(type_::LABEL).color(theme.text_secondary)),
        );
    }
    if !drafts.is_empty() {
        section = section.child(
            row()
                .key("draft-apply")
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(text("Apply").font(type_::LABEL).color(theme.text))
                .on_press(|state: &mut AppState| state.apply_column_drafts()),
        );
    }
    if !drafts.is_empty() {
        let preview = drafts
            .iter()
            .map(|draft| draft.sql(title))
            .collect::<Vec<_>>()
            .join("\n");
        let mut sql_block = col()
            .key("draft-sql")
            .w_full()
            .px(12.0)
            .py(8.0)
            .radius(8.0)
            .fill(theme.control_off);
        for line in preview.lines() {
            sql_block = sql_block.child(
                text(line.to_owned())
                    .font(crate::sql_editor::editor_font())
                    .color(theme.text),
            );
        }
        section = section.child(sql_block);
    }
    section
}

/// The index list: name, columns in index order, uniqueness.
pub(crate) fn indexes_view(state: &AppState, title: &str, theme: tok::Theme) -> El {
    // E1: the cache is the only read — the job's indexes segment renders.
    let indexes = state
        .structures
        .get(title)
        .and_then(|entry| entry.data.as_ref())
        .map(|data| data.indexes.clone())
        .unwrap_or_default();
    let header = |label: &str, width: f32| {
        row()
            .w(width)
            .h(24.0)
            .px(8.0)
            .center_y()
            .child(text(label).font(type_::LABEL).color(theme.text_secondary))
    };
    let mut list = col()
        .key("indexes-list")
        .w_full()
        .radius(8.0)
        .fill(theme.control_off)
        .child(
            row()
                .key("indexes-header")
                .w_full()
                .h(24.0)
                .child(header("Index", 260.0))
                .child(header("Columns", 320.0))
                .child(header("Unique", 100.0)),
        );
    for (index, spec) in indexes.iter().enumerate() {
        let cell = |value: &str, width: f32| {
            row()
                .w(width)
                .h(24.0)
                .px(8.0)
                .center_y()
                .child(text(value.to_owned()).font(type_::LABEL).color(theme.text))
        };
        list = list.child(
            row()
                .key(format!("index-row-{index}"))
                .w_full()
                .h(24.0)
                .child(cell(&spec.name, 260.0))
                .child(cell(&spec.columns.join(", "), 320.0))
                .child(cell(if spec.unique { "yes" } else { "no" }, 100.0)),
        );
    }
    if let Some(sentence) = structure_notice(state, title) {
        list = list.child(notice_row("indexes-structure-notice", &sentence, theme));
    }
    list
}
