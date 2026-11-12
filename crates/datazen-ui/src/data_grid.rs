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
            editor: TextEdit::new(String::new()),
            scroll_x: rune::Live::new(0.0),
            origin_col: 0,
        }
    }

    pub fn origin_col(&self) -> u32 {
        self.origin_col
    }

    pub fn focus(&self) -> (u32, u32) {
        self.focus
    }

    pub fn set_editable(&mut self, editable: bool) {
        self.editable = editable;
        if !editable {
            self.editing = false;
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
        }
        self.source = Some(page.rows.clone());
        self.columns = page.columns.clone();
        self.rows = page.rows.len() as u32;
        self.cols = page.columns.len() as u32;
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

pub fn panel(grid: &DataGrid, ink: rune::Color, muted: rune::Color, well: rune::Color) -> El {
    let (r0, r1, c0, c1) = grid.rect();
    let status = format!(
        "{}×{} · window at column {} · selection r{r0}c{c0}..r{r1}c{c1} · {} edit{}",
        grid.rows,
        grid.cols,
        grid.origin_col,
        grid.edits.len(),
        if grid.edits.len() == 1 { "" } else { "s" }
    );
    let scroll_x = grid.scroll_x();
    let width = COL_W * grid.cols as f32;
    let sheet = col()
        .key("data-grid-sheet")
        .w(width)
        .child(header_row(grid, ink).sticky_top(0.0))
        .child(
            col()
                .key("data-grid-rows")
                .w(width)
                .growth(GrowthSpec::default())
                .virtual_rows(
                    grid.rows as usize,
                    ROW_H,
                    |_state: &crate::AppState, index| Cow::Owned(format!("grid-row-{index}")),
                    move |state: &crate::AppState, index| data_row(&state.grid, index as u32, ink),
                )
                .virtual_rows_version(1u32),
        );
    col()
        .key("data-grid")
        .w_full()
        .h(420.0)
        .gap(8.0)
        .child(text(status).font(label_style()).color(muted).key("data-grid-status"))
        .child(
            col()
                .key("data-grid-scroll")
                .w_full()
                .h(380.0)
                .fill(well)
                .radius(8.0)
                .scroll_xy()
                .on_scroll_offset(move |at: ScrollPos| {
                    scroll_x.set(at.offset[0]);
                })
                .on_paste(|state: &mut crate::AppState, text, _| state.grid.paste_tsv(text))
                .child(sheet),
        )
}

fn header_row(grid: &DataGrid, ink: rune::Color) -> El {
    let mut line = row().key("data-grid-header").h(ROW_H).fill(oklch(0.22, 0.02, 250.0));
    let mut cursor = 0u32;
    for col_index in grid.built_columns() {
        if col_index > cursor {
            line = line.child(el().w((col_index - cursor) as f32 * COL_W).h(ROW_H));
        }
        line = line.child(header_cell(grid, col_index, ink));
        cursor = col_index + 1;
    }
    if cursor < grid.cols {
        line = line.child(el().w((grid.cols - cursor) as f32 * COL_W).h(ROW_H));
    }
    line
}

fn data_row(grid: &DataGrid, row_index: u32, ink: rune::Color) -> El {
    paint_row(grid, row_index, ink)
}

fn paint_row(grid: &DataGrid, row_index: u32, ink: rune::Color) -> El {
    let cols = grid.built_columns();
    let mut line = row().h(ROW_H);
    let mut cursor = 0u32;
    for col_index in cols {
        if col_index > cursor {
            line = line.child(el().w((col_index - cursor) as f32 * COL_W).h(ROW_H));
        }
        line = line.child(cell(grid, row_index, col_index, ink));
        cursor = col_index + 1;
    }
    if cursor < grid.cols {
        line = line.child(el().w((grid.cols - cursor) as f32 * COL_W).h(ROW_H));
    }
    line
}

fn header_cell(grid: &DataGrid, col_index: u32, ink: rune::Color) -> El {
    let label = grid
        .column_name(col_index)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("c{col_index}"));
    let mut cell = row()
        .key(format!("head-{col_index}"))
        .w(COL_W)
        .h(ROW_H)
        .px(8.0)
        .center_y()
        .child(text(label).font(label_style()).color(ink));
    if col_index == 0 {
        cell = cell.sticky_left(0.0).fill(oklch(0.24, 0.03, 250.0));
    }
    cell
}

fn cell(grid: &DataGrid, row_index: u32, col_index: u32, ink: rune::Color) -> El {
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
        .key(format!("cell-{row_index}-{col_index}"))
        .w(COL_W)
        .h(ROW_H)
        .px(8.0)
        .center_y()
        .fill(fill)
        .on_press(move |state: &mut crate::AppState| state.grid.select(row_index, col_index));
    if frozen {
        box_el = box_el.sticky_left(0.0);
    }
    if active && grid.editing {
        box_el.child(
            text_input(&grid.editor, editor_font())
                .key("grid-editor")
                .w(COL_W - 16.0)
                .h(ROW_H - 4.0)
                .on_edit(|state: &mut crate::AppState| &mut state.grid.editor)
                .on_submit(|state: &mut crate::AppState, _| state.commit_grid_edit()),
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
