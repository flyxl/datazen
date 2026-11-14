//! DataGrid proof of concept.
//!
//! Rows come from `virtual_rows`: the list is long, and only the mounted window is built.
//! Columns are not virtualized by Rune, so each mounted row builds a window of cells plus the
//! frozen first column. The window follows the horizontal scroll offset.
//!
//! Cell text, the selection rectangle and the editor live on the grid, not on the row element.
//! A row that scrolls away is destroyed. The values are not.

use std::borrow::Cow;
use std::collections::HashMap;

use rune::{
    El, GrowthSpec, Key, KeyEvent, ScrollPos, TextEdit, TextStyle, Tx, col, el, oklch, row, text,
    text_input,
};

pub const ROW_H: f32 = 28.0;
pub const COL_W: f32 = 120.0;
pub const ROW_COUNT: u32 = 20_000;
pub const COL_COUNT: u32 = 200;
/// Cells built for one mounted row, besides the frozen first column.
pub const COL_WINDOW: u32 = 12;

pub struct DataGrid {
    pub rows: u32,
    pub cols: u32,
    edits: HashMap<(u32, u32), String>,
    source: Option<Vec<Vec<String>>>,
    columns: Vec<String>,
    anchor: (u32, u32),
    focus: (u32, u32),
    editing: bool,
    editable: bool,
    page: u32,
    page_size: u32,
    total_rows: u32,
    /// The sort the loaded page was opened with, as a column index.
    sort: Option<(u32, bool)>,
    /// Why the loaded page is read-only; None means writable.
    read_only: Option<String>,
    /// The loaded page's table name, for panel titles.
    table: Option<String>,
    pub(crate) editor: TextEdit,
    /// Horizontal scroll, in points. `on_scroll_offset` writes it; it cannot touch this struct.
    scroll_x: rune::Live<f32>,
    origin_col: u32,
}

impl Default for DataGrid {
    fn default() -> Self {
        Self::new()
    }
}

impl DataGrid {
    pub fn new() -> Self {
        Self {
            rows: ROW_COUNT,
            cols: COL_COUNT,
            edits: HashMap::new(),
            source: None,
            columns: Vec::new(),
            anchor: (0, 0),
            focus: (0, 0),
            editing: false,
            editable: true,
            page: 0,
            page_size: 0,
            total_rows: 0,
            sort: None,
            read_only: None,
            table: None,
            editor: TextEdit::new(String::new()),
            scroll_x: rune::Live::new(0.0),
            origin_col: 0,
        }
    }

    pub fn page(&self) -> u32 {
        self.page
    }

    pub fn page_size(&self) -> u32 {
        self.page_size
    }

    /// Rows in the whole table behind this page.
    pub fn total_rows(&self) -> u32 {
        self.total_rows
    }

    /// Pages in the whole table at the current page size (at least 1).
    pub fn pages(&self) -> u32 {
        if self.page_size == 0 {
            return 1;
        }
        self.total_rows.div_ceil(self.page_size)
    }

    /// The sort column the loaded page carries, as (column index, descending).
    pub fn sort(&self) -> Option<(u32, bool)> {
        self.sort
    }

    /// Why the loaded page is read-only.
    pub fn read_only_reason(&self) -> Option<&str> {
        self.read_only.as_deref()
    }

    /// The loaded page's table name, for panel titles.
    pub fn table_label(&self) -> &str {
        self.table.as_deref().unwrap_or("result")
    }

    pub fn origin_col(&self) -> u32 {
        self.origin_col
    }

    /// True once a table page has been loaded. The fresh grid is only the POC
    /// demo surface and is not a product page.
    pub fn has_page(&self) -> bool {
        self.source.is_some()
    }

    pub fn focus(&self) -> (u32, u32) {
        self.focus
    }

    pub fn set_editable(&mut self, editable: bool) {
        self.editable = editable;
        if !editable {
            self.editing = false;
            self.editor.focused = false;
        }
    }

    pub fn is_editable(&self) -> bool {
        self.editable
    }

    pub fn is_editing(&self) -> bool {
        self.editing
    }

    pub fn scroll_x(&self) -> rune::Live<f32> {
        self.scroll_x.clone()
    }

    pub fn cell(&self, row: u32, col: u32) -> String {
        if let Some(value) = self.edits.get(&(row, col)) {
            return value.clone();
        }
        if let Some(rows) = &self.source {
            if let Some(record) = rows.get(row as usize) {
                if let Some(value) = record.get(col as usize) {
                    return value.clone();
                }
            }
        }
        format!("r{row}c{col}")
    }

    pub fn column_name(&self, col: u32) -> Option<&str> {
        self.columns.get(col as usize).map(String::as_str)
    }

    pub fn load_page(&mut self, page: &datazen_ui_api::TablePage) {
        if self.editing {
            self.editing = false;
            self.editor.focused = false;
        }
        self.source = Some(page.rows.clone());
        self.columns = page.columns.clone();
        self.rows = page.rows.len() as u32;
        self.cols = page.columns.len() as u32;
        self.page = page.page;
        self.page_size = page.page_size;
        self.total_rows = page.total_rows;
        self.sort = page.sort.as_ref().and_then(|sort| {
            page.columns
                .iter()
                .position(|name| name == &sort.column)
                .map(|index| (index as u32, sort.descending))
        });
        self.read_only = page.read_only.clone();
        self.table = Some(page.table.clone());
        self.edits.clear();
        self.anchor = (0, 0);
        self.focus = (0, 0);
        self.origin_col = 0;
        self.scroll_x.set(0.0);
    }

    pub fn select(&mut self, row: u32, col: u32) {
        if self.editing {
            self.commit_edit();
        }
        let row = row.min(self.rows.saturating_sub(1));
        let col = col.min(self.cols.saturating_sub(1));
        self.anchor = (row, col);
        self.focus = (row, col);
    }

    pub fn begin_edit(&mut self) {
        if !self.editable {
            return;
        }
        if self.editing {
            self.commit_edit();
        }
        self.editor = TextEdit::new(self.cell(self.focus.0, self.focus.1));
        self.editor.focused = true;
        self.editing = true;
    }

    pub fn commit_edit(&mut self) {
        if !self.editing {
            return;
        }
        self.edits.insert(self.focus, self.editor.text.clone());
        self.editing = false;
        // The editor element is about to unmount; an editor that keeps the IME
        // focus eats Command keys that belong to the grid (copy, select-all).
        self.editor.focused = false;
    }

    /// The loaded page's value at a cell, or the POC fallback when no page is loaded.
    fn original(&self, row: u32, col: u32) -> String {
        self.source
            .as_ref()
            .and_then(|rows| rows.get(row as usize))
            .and_then(|record| record.get(col as usize))
            .cloned()
            .unwrap_or_else(|| format!("r{row}c{col}"))
    }

    /// Cells whose displayed value differs from the loaded page's value. An edit
    /// that lands back on the original value is not dirty.
    pub fn dirty_cells(&self) -> u32 {
        self.edits
            .iter()
            .filter(|((row, col), value)| self.original(*row, *col) != **value)
            .count() as u32
    }

    pub fn copy_tsv(&self) -> String {
        let (r0, r1, c0, c1) = self.rect();
        let mut lines = Vec::new();
        for r in r0..=r1 {
            let mut cells = Vec::new();
            for c in c0..=c1 {
                cells.push(self.cell(r, c));
            }
            lines.push(cells.join("\t"));
        }
        lines.join("\n")
    }

    /// Paste TSV with the focus cell as the top-left. Cells outside the grid are dropped.
    pub fn paste_tsv(&mut self, text: &str) {
        if self.editing {
            self.commit_edit();
        }
        let (row0, col0) = self.focus;
        for (dr, line) in text.split('\n').enumerate() {
            let line = line.trim_end_matches('\r');
            if line.is_empty() {
                continue;
            }
            for (dc, value) in line.split('\t').enumerate() {
                let Some(row) = (row0 as usize).checked_add(dr) else { continue };
                let Some(col) = (col0 as usize).checked_add(dc) else { continue };
                if row < self.rows as usize && col < self.cols as usize {
                    self.edits.insert((row as u32, col as u32), value.to_owned());
                }
            }
        }
    }

    /// Move the column window to the horizontal scroll offset. True when the window changed,
    /// so the view has to build different cells.
    pub fn sync_scroll(&mut self) -> bool {
        let raw = (self.scroll_x.get() / COL_W).floor();
        let origin = if raw <= 0.0 { 0 } else { raw as u32 };
        let origin = origin.min(self.cols.saturating_sub(1));
        if origin == self.origin_col {
            false
        } else {
            self.origin_col = origin;
            true
        }
    }

    fn nudge(&mut self, dr: i32, dc: i32, extend: bool) {
        if self.editing {
            return;
        }
        let r = (self.focus.0 as i32 + dr).clamp(0, self.rows as i32 - 1) as u32;
        let c = (self.focus.1 as i32 + dc).clamp(0, self.cols as i32 - 1) as u32;
        self.focus = (r, c);
        if !extend {
            self.anchor = (r, c);
        }
        self.reveal_focus();
    }

    fn reveal_focus(&mut self) {
        let col = self.focus.1;
        let end = self.origin_col.saturating_add(COL_WINDOW);
        if col < self.origin_col || col >= end {
            self.origin_col = col.saturating_sub(COL_WINDOW / 2);
            self.scroll_x.set(self.origin_col as f32 * COL_W);
        }
    }

    fn rect(&self) -> (u32, u32, u32, u32) {
        let r0 = self.anchor.0.min(self.focus.0);
        let r1 = self.anchor.0.max(self.focus.0);
        let c0 = self.anchor.1.min(self.focus.1);
        let c1 = self.anchor.1.max(self.focus.1);
        (r0, r1, c0, c1)
    }

    fn covers(&self, row: u32, col: u32) -> bool {
        let (r0, r1, c0, c1) = self.rect();
        row >= r0 && row <= r1 && col >= c0 && col <= c1
    }

    /// Columns this row should build: the frozen first column, then the window.
    pub fn built_columns(&self) -> Vec<u32> {
        let mut cols = Vec::with_capacity(COL_WINDOW as usize + 1);
        let start = self.origin_col.min(self.cols.saturating_sub(1));
        let end = start.saturating_add(COL_WINDOW).min(self.cols);
        if start > 0 {
            cols.push(0);
        }
        for col in start..end {
            if !cols.contains(&col) {
                cols.push(col);
            }
        }
        cols
    }
}

pub fn on_key(grid: &mut DataGrid, ev: &KeyEvent, tx: &mut Tx) -> bool {
    if grid.editing {
        return false;
    }
    if ev.is_command('c') {
        tx.copy(grid.copy_tsv());
        return true;
    }
    let extend = ev.mods.shift;
    match ev.key {
        Key::Left => grid.nudge(0, -1, extend),
        Key::Right => grid.nudge(0, 1, extend),
        Key::Up => grid.nudge(-1, 0, extend),
        Key::Down => grid.nudge(1, 0, extend),
        Key::Enter => grid.begin_edit(),
        _ => return false,
    }
    true
}

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
fn sheet(grid: &DataGrid, slot: GridSlot, ink: rune::Color, well: rune::Color, height: f32) -> El {
    let scroll_x = grid.scroll_x();
    let width = COL_W * grid.cols as f32;
    let sheet = col()
        .key(slot.key("data-grid-sheet"))
        .w(width)
        .child(header_row(grid, slot, ink).sticky_top(0.0))
        .child(
            col()
                .key(slot.key("data-grid-rows"))
                .w(width)
                .growth(GrowthSpec::default())
                .virtual_rows(
                    grid.rows as usize,
                    ROW_H,
                    move |_state: &crate::AppState, index| Cow::Owned(slot.key(&format!("grid-row-{index}"))),
                    move |state: &crate::AppState, index| data_row(grid_of(state, slot), slot, index as u32, ink),
                )
                .virtual_rows_version(1u32),
        );
    let scroll = col()
        .key(slot.key("data-grid-scroll"))
        .w_full()
        .h(height)
        .fill(well)
        .radius(8.0)
        .scroll_xy()
        .on_scroll_offset(move |at: ScrollPos| {
            scroll_x.set(at.offset[0]);
        })
        .child(sheet);
    if slot == GridSlot::Table {
        scroll.on_paste(|state: &mut crate::AppState, text, _| state.grid.paste_tsv(text))
    } else {
        scroll
    }
}

pub fn panel(state: &crate::AppState, ink: rune::Color, muted: rune::Color, well: rune::Color) -> El {
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
        .child(sheet(grid, GridSlot::Table, ink, well, 380.0))
        .child(footer(grid, ink, muted))
}

/// The query result area: the same sheet as the table page, without the filter
/// bar and the footer, and not editable. The sync preview reuses it.
pub fn result_panel(
    grid: &DataGrid,
    slot: GridSlot,
    ink: rune::Color,
    muted: rune::Color,
    well: rune::Color,
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
        .child(sheet(grid, slot, ink, well, 200.0))
}

/// The status line above the grid: page shape, column window, selection, dirty cells.
fn status_line(grid: &DataGrid) -> String {
    let (r0, r1, c0, c1) = grid.rect();
    format!(
        "{}×{} · window at column {} · selection r{r0}c{c0}..r{r1}c{c1} · {} dirty",
        grid.rows, grid.cols, grid.origin_col, grid.dirty_cells()
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

/// Prev / next paging over the table behind the loaded page.
fn footer(grid: &DataGrid, ink: rune::Color, muted: rune::Color) -> El {
    row()
        .key("grid-footer")
        .w_full()
        .h(28.0)
        .gap(8.0)
        .center_y()
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

fn header_row(grid: &DataGrid, slot: GridSlot, ink: rune::Color) -> El {
    let mut line = row().key(slot.key("data-grid-header")).h(ROW_H).fill(oklch(0.22, 0.02, 250.0));
    let mut cursor = 0u32;
    for col_index in grid.built_columns() {
        if col_index > cursor {
            line = line.child(el().w((col_index - cursor) as f32 * COL_W).h(ROW_H));
        }
        line = line.child(header_cell(grid, slot, col_index, ink));
        cursor = col_index + 1;
    }
    if cursor < grid.cols {
        line = line.child(el().w((grid.cols - cursor) as f32 * COL_W).h(ROW_H));
    }
    line
}

fn data_row(grid: &DataGrid, slot: GridSlot, row_index: u32, ink: rune::Color) -> El {
    paint_row(grid, slot, row_index, ink)
}

fn paint_row(grid: &DataGrid, slot: GridSlot, row_index: u32, ink: rune::Color) -> El {
    let cols = grid.built_columns();
    let mut line = row().h(ROW_H);
    let mut cursor = 0u32;
    for col_index in cols {
        if col_index > cursor {
            line = line.child(el().w((col_index - cursor) as f32 * COL_W).h(ROW_H));
        }
        line = line.child(cell(grid, slot, row_index, col_index, ink));
        cursor = col_index + 1;
    }
    if cursor < grid.cols {
        line = line.child(el().w((grid.cols - cursor) as f32 * COL_W).h(ROW_H));
    }
    line
}

fn header_cell(grid: &DataGrid, slot: GridSlot, col_index: u32, ink: rune::Color) -> El {
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
        .w(COL_W)
        .h(ROW_H)
        .px(8.0)
        .center_y()
        .child(text(label).font(label_style()).color(ink));
    if slot == GridSlot::Table {
        cell = cell.on_press(move |state: &mut crate::AppState| state.cycle_sort(col_index));
    }
    if col_index == 0 {
        cell = cell.sticky_left(0.0).fill(oklch(0.24, 0.03, 250.0));
    }
    cell
}

fn cell(grid: &DataGrid, slot: GridSlot, row_index: u32, col_index: u32, ink: rune::Color) -> El {
    let selected = grid.covers(row_index, col_index);
    let active = grid.focus == (row_index, col_index);
    let frozen = col_index == 0;
    let fill = if active {
        oklch(0.45, 0.09, 250.0)
    } else if selected {
        oklch(0.38, 0.05, 250.0)
    } else if frozen {
        oklch(0.2, 0.02, 250.0)
    } else if row_index % 2 == 0 {
        oklch(0.18, 0.01, 250.0)
    } else {
        oklch(0.16, 0.01, 250.0)
    };
    let mut box_el = row()
        .key(slot.key(&format!("cell-{row_index}-{col_index}")))
        .w(COL_W)
        .h(ROW_H)
        .px(8.0)
        .center_y()
        .fill(fill);
    if slot == GridSlot::Table {
        box_el = box_el.on_press(move |state: &mut crate::AppState| state.grid.select(row_index, col_index));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mounted_row_builds_a_window_not_every_column() {
        let grid = DataGrid::new();
        let cols = grid.built_columns();
        assert_eq!(cols.len(), COL_WINDOW as usize);
        assert!(cols.contains(&0));
        assert!(!cols.contains(&40));
    }

    #[test]
    fn scroll_offset_moves_the_column_window_and_keeps_the_frozen_column() {
        let mut grid = DataGrid::new();
        grid.scroll_x.set(COL_W * 50.0 + 4.0);
        assert!(grid.sync_scroll());
        assert_eq!(grid.origin_col(), 50);
        let cols = grid.built_columns();
        assert!(cols.contains(&0), "frozen column stays built");
        assert!(cols.contains(&50));
        assert!(!cols.contains(&11));
        assert!(cols.len() <= COL_WINDOW as usize + 1);
    }

    #[test]
    fn paste_writes_the_rectangle_and_copy_reads_it_back() {
        let mut grid = DataGrid::new();
        grid.select(2, 3);
        grid.paste_tsv("A\tB\nC\tD");
        assert_eq!(grid.cell(2, 3), "A");
        assert_eq!(grid.cell(2, 4), "B");
        assert_eq!(grid.cell(3, 3), "C");
        assert_eq!(grid.cell(3, 4), "D");
        assert_eq!(grid.cell(2, 5), "r2c5");
        grid.focus = (3, 4);
        grid.anchor = (2, 3);
        assert_eq!(grid.copy_tsv(), "A\tB\nC\tD");
    }

    #[test]
    fn a_cell_edited_back_to_its_original_value_is_not_dirty() {
        let mut grid = DataGrid::new();
        assert_eq!(grid.dirty_cells(), 0);
        grid.select(2, 2);
        grid.begin_edit();
        grid.editor.insert("x");
        grid.commit_edit();
        assert_eq!(grid.cell(2, 2), "r2c2x");
        assert_eq!(grid.dirty_cells(), 1, "an edited cell is dirty");

        grid.begin_edit();
        grid.editor = TextEdit::new(String::new());
        grid.editor.insert("r2c2");
        grid.commit_edit();
        assert_eq!(grid.cell(2, 2), "r2c2");
        assert_eq!(grid.dirty_cells(), 0, "edited back to the original value");
    }

    #[test]
    fn the_status_line_counts_dirty_cells_not_edits() {
        let mut grid = DataGrid::new();
        grid.select(1, 1);
        grid.begin_edit();
        grid.editor.insert("z");
        grid.commit_edit();
        grid.begin_edit();
        grid.editor = TextEdit::new(String::new());
        grid.editor.insert("r1c1");
        grid.commit_edit();
        let status = status_line(&grid);
        assert!(status.contains("0 dirty"), "status says 0 dirty: {status}");
        assert!(status.contains("selection r1c1..r1c1"), "selection stays: {status}");
    }

    #[test]
    fn loading_the_next_page_keeps_the_column_window_and_the_frozen_column() {
        let columns: Vec<String> = (0..30).map(|i| format!("c{i}")).collect();
        let rows: Vec<Vec<String>> =
            (0..50).map(|r| columns.iter().map(|c| format!("{c}-{r}")).collect()).collect();
        let page = |rows: Vec<Vec<String>>, page: u32| datazen_ui_api::TablePage {
            table: "t".into(),
            columns: columns.clone(),
            rows,
            page,
            page_size: 25,
            total_rows: 50,
            sort: None,
            filter: None,
            read_only: None,
        };
        let mut grid = DataGrid::new();
        grid.load_page(&page(rows[..25].to_vec(), 0));
        grid.scroll_x.set(COL_W * 20.0);
        assert!(grid.sync_scroll());
        assert_eq!(grid.origin_col(), 20);

        grid.load_page(&page(rows[25..].to_vec(), 1));
        assert_eq!(grid.page(), 1);
        assert_eq!(grid.pages(), 2);
        assert_eq!(grid.origin_col(), 0, "a fresh page starts at the first column");
        grid.scroll_x.set(COL_W * 20.0);
        assert!(grid.sync_scroll());
        let cols = grid.built_columns();
        assert!(cols.contains(&0), "the frozen column stays");
        assert!(cols.contains(&20));
    }

    #[test]
    fn editing_commits_into_the_cell_and_survives_as_data() {
        let mut grid = DataGrid::new();
        grid.select(4, 6);
        grid.begin_edit();
        grid.editor.insert("雪");
        grid.commit_edit();
        assert!(!grid.is_editing());
        assert_eq!(grid.cell(4, 6), "r4c6雪");
    }
}
