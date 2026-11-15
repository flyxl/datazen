use std::borrow::Cow;

use rune::{El, GrowthSpec, ScrollPos, TextStyle, col, el, oklch, row, text, text_input};

use super::{DataGrid, COL_W, ROW_H};

/// Which grid a render call targets: the table page's or the query result's.
/// The result keys are prefixed so both can be mounted in one frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GridSlot {
    Table,
    Result,
    /// The sync wizard's preview: the same look and keys as a result page; the
    /// tools page and the workspace never mount at once.
    SyncPreview,
}

impl GridSlot {
    fn key(self, base: &str) -> String {
        match self {
            Self::Table => base.to_owned(),
            Self::Result | Self::SyncPreview => format!("result-{base}"),
        }
    }
}

pub(crate) fn grid_of<'a>(state: &'a crate::AppState, slot: GridSlot) -> &'a DataGrid {
    match slot {
        GridSlot::Table => &state.grid,
        GridSlot::Result => state.result_grid.as_ref().expect("the result grid is mounted"),
        GridSlot::SyncPreview => state
            .sync_preview_grid
            .as_ref()
            .expect("the sync preview grid is mounted"),
    }
}

/// The scrolling sheet the table page and the query result share: a sticky header
/// over a window of virtual rows.
fn sheet(
    grid: &DataGrid,
    slot: GridSlot,
    ink: rune::Color,
    well: rune::Color,
    height: f32,
    dark: bool,
) -> El {
    let scroll_x = grid.scroll_x();
    let width = COL_W * grid.cols as f32;
    let sheet = col()
        .key(slot.key("data-grid-sheet"))
        .w(width)
        .child(header_row(grid, slot, ink, dark).sticky_top(0.0))
        .child(
            col()
                .key(slot.key("data-grid-rows"))
                .w(width)
                .growth(GrowthSpec::default())
                .virtual_rows(
                    grid.rows as usize,
                    ROW_H,
                    move |_state: &crate::AppState, index| {
                        Cow::Owned(slot.key(&format!("grid-row-{index}")))
                    },
                    move |state: &crate::AppState, index| {
                        data_row(grid_of(state, slot), slot, index as u32, ink, dark)
                    },
                )
                .virtual_rows_version(1u32),
        );
    // C2: the grid reads its shape and the current cell to the a11y tree. The
    // value carries the page's rows × columns; the descendant the focused
    // cell points at, when that cell is mounted.
    let (focus_row, focus_col) = grid.focus();
    let scroll = col()
        .key(slot.key("data-grid-scroll"))
        .w_full()
        .h(height)
        .fill(well)
        .radius(8.0)
        .role("grid")
        .label(grid.table_label())
        .a11y_value(format!("{} rows × {} columns", grid.rows, grid.cols))
        .active_descendant(slot.key(&format!("cell-{focus_row}-{focus_col}")))
        .scroll_xy()
        .on_scroll_offset(move |at: ScrollPos| {
            scroll_x.set(at.offset[0]);
        })
        .child(sheet);
    scroll
}

pub fn panel(
    state: &crate::AppState,
    ink: rune::Color,
    muted: rune::Color,
    well: rune::Color,
    dark: bool,
) -> El {
    let grid = &state.grid;
    let status = status_line(grid);
    let mut panel = col()
        .key("data-grid")
        .w_full()
        .h(484.0)
        .gap(8.0)
        .child(text(status).font(label_style()).color(muted).key("data-grid-status"));
    if let Some(reason) = grid.read_only_reason() {
        panel = panel.child(
            row()
                .key("grid-readonly")
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(well)
                .child(text(format!("Read-only: {reason}")).font(label_style()).color(muted)),
        );
    }
    panel
        .child(filter_bar(state, ink, muted, well))
        .child(fk_bar(state, ink, muted, well))
        .child(sheet(grid, GridSlot::Table, ink, well, 380.0, dark))
        .child(footer(state, ink, muted))
}

/// T5: the foreign-key navigation bar. The focused cell's column decides
/// what shows: a "go to the referenced row" chip when the column carries an
/// outbound key, and the "Referenced by" list of the tables pointing at
/// this one.
fn fk_bar(state: &crate::AppState, ink: rune::Color, muted: rune::Color, well: rune::Color) -> El {
    let mut bar = row()
        .key("fk-navigation")
        .w_full()
        .h(28.0)
        .gap(8.0)
        .center_y();
    if let Some((key, index)) = state.outbound_fk_of_focus() {
        let target = key
            .target_columns
            .get(index)
            .cloned()
            .unwrap_or_else(|| "?".into());
        bar = bar.child(
            row()
                .key("fk-go-to")
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(well)
                .child(
                    text(format!("\u{2192} {} · {}", key.target_table, target))
                        .font(label_style())
                        .color(ink),
                )
                .on_press(|state: &mut crate::AppState| state.go_to_referenced_row()),
        );
    }
    let inbound_open = state.inbound_refs.is_some();
    bar = bar.child(
        row()
            .key("fk-referenced-by")
            .h(24.0)
            .px(8.0)
            .center_y()
            .radius(6.0)
            .fill(well)
            .child(
                text(if inbound_open { "Referenced by \u{00d7}" } else { "Referenced by\u{2026}" })
                    .font(label_style())
                    .color(ink),
            )
            .on_press(move |state: &mut crate::AppState| {
                if inbound_open {
                    state.close_inbound_refs();
                } else {
                    state.load_inbound_refs();
                }
            }),
    );
    if let Some(error) = &state.fk_error {
        bar = bar.child(
            text(error.clone())
                .key("fk-error")
                .font(label_style())
                .color(muted),
        );
    }
    let mut column = col().key("fk-bar").w_full().gap(4.0).child(bar);
    // E3: the scan runs as a job; the panel says so until its delivery.
    if state.inbound_loading.is_some() {
        column = column.child(
            text("scanning the references…".to_owned())
                .key("fk-inbound-loading")
                .font(label_style())
                .color(muted),
        );
    }
    if let Some(refs) = &state.inbound_refs {
        let mut list = col().key("fk-inbound-list").w_full().gap(4.0);
        if refs.is_empty() {
            list = list.child(
                text("No table references this one".to_owned())
                    .key("fk-inbound-empty")
                    .font(label_style())
                    .color(muted),
            );
        }
        for (index, reference) in refs.iter().enumerate() {
            list = list.child(
                row()
                    .key(format!("fk-inbound-{index}"))
                    .w_full()
                    .h(24.0)
                    .px(8.0)
                    .center_y()
                    .radius(6.0)
                    .fill(well)
                    .child(
                        text(format!(
                            "\u{2190} {} · {} (references {})",
                            reference.table, reference.column, reference.referenced_column
                        ))
                        .font(label_style())
                        .color(ink),
                    )
                    .on_press(move |state: &mut crate::AppState| {
                        state.locate_inbound_ref(index);
                    }),
            );
        }
        column = column.child(list);
    }
    column
}

/// The query result area: the same sheet as the table page, without the filter
/// bar and the footer, and not editable. The sync preview reuses it.
pub fn result_panel(
    grid: &DataGrid,
    slot: GridSlot,
    ink: rune::Color,
    muted: rune::Color,
    well: rune::Color,
    dark: bool,
) -> El {
    col()
        .key("result-grid")
        .w_full()
        .gap(8.0)
        .child(
            text(format!("{} · {} rows × {} cols", grid.table_label(), grid.rows, grid.cols))
                .font(label_style())
                .color(muted)
                .key("result-grid-status"),
        )
        .child(sheet(grid, slot, ink, well, 200.0, dark))
}

/// The status line above the grid: page shape, column window, selection, dirty cells.
pub(crate) fn status_line(grid: &DataGrid) -> String {
    let (r0, r1, c0, c1) = grid.rect();
    format!(
        "{}×{} · window at column {} · selection r{r0}c{c0}..r{r1}c{c1} · {} dirty",
        grid.rows,
        grid.cols,
        grid.origin_col(),
        grid.dirty_cells()
    )
}

/// One text filter on one column: a column chip that cycles, a text field, the row count.
fn filter_bar(state: &crate::AppState, ink: rune::Color, muted: rune::Color, well: rune::Color) -> El {
    let column = state
        .grid
        .column_name(state.filter_column)
        .map(str::to_owned)
        .unwrap_or_else(|| "-".into());
    row()
        .key("grid-filter")
        .w_full()
        .h(28.0)
        .gap(8.0)
        .center_y()
        .child(
            row()
                .key("filter-column")
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(well)
                .child(text(format!("in: {column}")).font(label_style()).color(ink))
                .on_press(|state: &mut crate::AppState| state.cycle_filter_column()),
        )
        .child(
            text_input(&state.filter_editor, label_style())
                .key("filter-input")
                .w(220.0)
                .h(24.0)
                .on_edit(|state: &mut crate::AppState| &mut state.filter_editor)
                .on_submit(|state: &mut crate::AppState, tx| {
                    state.apply_filter();
                    tx.blur();
                }),
        )
        .child(
            text(format!("{} rows", state.grid.total_rows()))
                .font(label_style())
                .color(muted),
        )
}

/// Prev / next paging over the table behind the loaded page, and the host
/// job markers (N1): a read in flight says "Loading…", a commit in flight
/// says "Saving…". The markers live in the footer's fixed height, so the
/// grid's layout does not shift while a job runs.
fn footer(state: &crate::AppState, ink: rune::Color, muted: rune::Color) -> El {
    let grid = &state.grid;
    let mut footer = row()
        .key("grid-footer")
        .w_full()
        .h(28.0)
        .gap(8.0)
        .center_y();
    if state.commit_loading.is_some() {
        footer = footer.child(
            row()
                .key("commit-loading")
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .child(text("Saving…").font(label_style()).color(muted)),
        );
    } else if state.table_loading.is_some() {
        footer = footer.child(
            row()
                .key("table-loading")
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .child(text("Loading…").font(label_style()).color(muted)),
        );
    }
    footer
        .child(
            row()
                .key("page-prev")
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .child(text("‹ Prev").font(label_style()).color(ink))
                .on_press(|state: &mut crate::AppState| state.prev_table_page()),
        )
        .child(
            text(format!("Page {} / {}", grid.page() + 1, grid.pages()))
                .key("grid-page-label")
                .font(label_style())
                .color(muted),
        )
        .child(
            row()
                .key("page-next")
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .child(text("Next ›").font(label_style()).color(ink))
                .on_press(|state: &mut crate::AppState| state.next_table_page()),
        )
        .child(
            text(format!("{} per page · {} rows", grid.page_size(), grid.total_rows()))
                .font(label_style())
                .color(muted),
        )
        .child(
            row()
                .key("commit-staged")
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .child(text("Commit").font(label_style()).color(ink))
                .on_press(|state: &mut crate::AppState| state.commit_staged_edits()),
        )
        .child(
            row()
                .key("rollback-staged")
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .child(text("Rollback").font(label_style()).color(ink))
                .on_press(|state: &mut crate::AppState| state.rollback_staged_edits()),
        )
}

fn header_row(grid: &DataGrid, slot: GridSlot, ink: rune::Color, dark: bool) -> El {
    let header_bg = if dark { oklch(0.22, 0.02, 250.0) } else { oklch(0.92, 0.01, 250.0) };
    let mut line = row().key(slot.key("data-grid-header")).h(ROW_H).fill(header_bg);
    let mut cursor = 0u32;
    for col_index in grid.built_columns() {
        if col_index > cursor {
            line = line.child(el().w((col_index - cursor) as f32 * COL_W).h(ROW_H));
        }
        line = line.child(header_cell(grid, slot, col_index, ink, dark));
        cursor = col_index + 1;
    }
    if cursor < grid.cols {
        line = line.child(el().w((grid.cols - cursor) as f32 * COL_W).h(ROW_H));
    }
    line
}

fn data_row(grid: &DataGrid, slot: GridSlot, row_index: u32, ink: rune::Color, dark: bool) -> El {
    paint_row(grid, slot, row_index, ink, dark)
}

fn paint_row(grid: &DataGrid, slot: GridSlot, row_index: u32, ink: rune::Color, dark: bool) -> El {
    let cols = grid.built_columns();
    // The key matches the virtual list's row key, so the row reads its place
    // in the set from the list's index.
    let mut line = row()
        .key(slot.key(&format!("grid-row-{row_index}")))
        .role("row")
        .h(ROW_H);
    let mut cursor = 0u32;
    for col_index in cols {
        if col_index > cursor {
            line = line.child(el().w((col_index - cursor) as f32 * COL_W).h(ROW_H));
        }
        line = line.child(cell(grid, slot, row_index, col_index, ink, dark));
        cursor = col_index + 1;
    }
    if cursor < grid.cols {
        line = line.child(el().w((grid.cols - cursor) as f32 * COL_W).h(ROW_H));
    }
    line
}

fn header_cell(
    grid: &DataGrid,
    slot: GridSlot,
    col_index: u32,
    ink: rune::Color,
    dark: bool,
) -> El {
    let label = grid
        .column_name(col_index)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("c{col_index}"));
    let label = match grid.sort() {
        Some((col, true)) if col == col_index => format!("{label} ↓"),
        Some((col, false)) if col == col_index => format!("{label} ↑"),
        _ => label,
    };
    let mut cell = row()
        .key(slot.key(&format!("head-{col_index}")))
        .role("columnheader")
        .w(COL_W)
        .h(ROW_H)
        .px(8.0)
        .center_y()
        .child(text(label).font(label_style()).color(ink));
    if slot == GridSlot::Table {
        cell = cell.on_press(move |state: &mut crate::AppState| state.cycle_sort(col_index));
    }
    if col_index == 0 {
        let frozen_bg = if dark { oklch(0.24, 0.03, 250.0) } else { oklch(0.90, 0.01, 250.0) };
        cell = cell.sticky_left(0.0).fill(frozen_bg);
    }
    cell
}

fn cell(
    grid: &DataGrid,
    slot: GridSlot,
    row_index: u32,
    col_index: u32,
    ink: rune::Color,
    dark: bool,
) -> El {
    let selected = grid.covers(row_index, col_index);
    let active = grid.focus == (row_index, col_index);
    let frozen = col_index == 0;
    let fill = if active {
        if dark { oklch(0.45, 0.09, 250.0) } else { oklch(0.78, 0.10, 250.0) }
    } else if selected {
        if dark { oklch(0.38, 0.05, 250.0) } else { oklch(0.86, 0.05, 250.0) }
    } else if frozen {
        if dark { oklch(0.20, 0.02, 250.0) } else { oklch(0.94, 0.01, 250.0) }
    } else if row_index % 2 == 0 {
        if dark { oklch(0.18, 0.01, 250.0) } else { oklch(0.99, 0.005, 250.0) }
    } else if dark {
        oklch(0.16, 0.01, 250.0)
    } else {
        oklch(0.965, 0.005, 250.0)
    };
    let mut box_el = row()
        .key(slot.key(&format!("cell-{row_index}-{col_index}")))
        .role("gridcell")
        .w(COL_W)
        .h(ROW_H)
        .px(8.0)
        .center_y()
        .fill(fill);
    if slot == GridSlot::Table {
        box_el = box_el
            .on_press(move |state: &mut crate::AppState| state.grid.select(row_index, col_index));
    } else if slot == GridSlot::Result {
        // T1: a pressed result cell takes the selection and the keyboard; the
        // editor's surface and typing hand the keys back.
        box_el = box_el.on_press(move |state: &mut crate::AppState| {
            if let Some(grid) = state.result_grid.as_mut() {
                grid.select(row_index, col_index);
            }
            state.result_focused = true;
        });
    }
    if frozen {
        box_el = box_el.sticky_left(0.0);
    }
    if active && grid.editing {
        box_el.child(
            text_input(&grid.editor, editor_font())
                .key(slot.key("grid-editor"))
                .w(COL_W - 16.0)
                .h(ROW_H - 4.0)
                .on_edit(|state: &mut crate::AppState| &mut state.grid.editor)
                .on_submit(|state: &mut crate::AppState, tx| {
                    state.commit_grid_edit();
                    tx.blur();
                }),
        )
    } else {
        box_el.child(text(grid.cell(row_index, col_index)).font(label_style()).color(ink))
    }
}

fn label_style() -> TextStyle {
    TextStyle::new(12.0, 400.0)
}

fn editor_font() -> TextStyle {
    TextStyle::new(12.0, 400.0)
}
