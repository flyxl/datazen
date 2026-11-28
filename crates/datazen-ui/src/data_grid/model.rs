use std::collections::HashMap;

use datazen_ui_api::{NativeTablePage, NativeTableRowCount};
use rune::TextEdit;

pub const ROW_H: f32 = 28.0;
pub const COL_W: f32 = 120.0;
/// Default width of the row-number gutter (A4).
pub const GUTTER_W: f32 = 40.0;
pub const ROW_COUNT: u32 = 20_000;
pub const COL_COUNT: u32 = 200;
/// Cells built for one mounted row, besides the frozen first column.
pub const COL_WINDOW: u32 = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Selection {
    None,
    Cells,
    Row(u32),
}

struct DragSelection {
    selection: Selection,
    anchor: (u32, u32),
    focus: (u32, u32),
    scroll: (f32, f32),
}

pub struct DataGrid {
    pub rows: u32,
    pub cols: u32,
    edits: HashMap<(u32, u32), String>,
    source: Option<Vec<Vec<String>>>,
    columns: Vec<String>,
    column_types: Vec<String>,
    /// Per-column widths in points (A4), persisted per table.
    col_widths: Vec<u32>,
    pub anchor: (u32, u32),
    pub focus: (u32, u32),
    selection: Selection,
    drag_selection: Option<DragSelection>,
    pub editing: bool,
    editable: bool,
    page: u32,
    page_size: u32,
    row_count: NativeTableRowCount,
    bounded_offset: bool,
    /// The sort the loaded page was opened with, as a column index.
    sort: Option<(u32, bool)>,
    /// The loaded page's primary-key column names (N6): the commit batch
    /// addresses rows by them.
    pk: Vec<String>,
    /// Why the loaded page is read-only; None means writable.
    read_only: Option<String>,
    /// The loaded page's table name, for panel titles.
    table: Option<String>,
    pub editor: TextEdit,
    /// Horizontal scroll, in points. `on_scroll_offset` writes it; it cannot touch this struct.
    scroll_x: rune::Live<f32>,
    // Only the table sheet observes Y: a virtualized source row can disappear
    // before Rune can send its captured cell a final pointer event.
    table_scroll_y: rune::Live<f32>,
    origin_col: u32,
    /// Result/find state (A5): the active needle, every matching cell in
    /// row-major order, and the current hit's index in that list.
    find_needle: String,
    find_hits: Vec<(u32, u32)>,
    find_index: Option<usize>,
}

impl std::fmt::Debug for DataGrid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DataGrid")
            .field("rows", &self.rows)
            .field("cols", &self.cols)
            .field("page", &self.page)
            .field("page_size", &self.page_size)
            .field("row_count", &self.row_count)
            .field("table", &self.table)
            .field("read_only", &self.read_only)
            .field("origin_col", &self.origin_col)
            .finish_non_exhaustive()
    }
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
            column_types: Vec::new(),
            col_widths: Vec::new(),
            anchor: (0, 0),
            focus: (0, 0),
            selection: Selection::Cells,
            drag_selection: None,
            editing: false,
            editable: true,
            page: 0,
            page_size: 0,
            row_count: NativeTableRowCount::Exact(0),
            bounded_offset: false,
            sort: None,
            pk: Vec::new(),
            read_only: None,
            table: None,
            editor: TextEdit::new(String::new()),
            scroll_x: rune::Live::new(0.0),
            table_scroll_y: rune::Live::new(0.0),
            origin_col: 0,
            find_needle: String::new(),
            find_hits: Vec::new(),
            find_index: None,
        }
    }

    pub fn page(&self) -> u32 {
        self.page
    }

    pub fn page_size(&self) -> u32 {
        self.page_size
    }

    /// Legacy exact-only shorthand for result grids and existing callers.
    /// Interactive table displays must read row_count(), not this shorthand.
    pub fn total_rows(&self) -> u32 {
        match self.row_count {
            NativeTableRowCount::Exact(total) => u32::try_from(total).unwrap_or(u32::MAX),
            _ => 0,
        }
    }

    pub fn row_count(&self) -> NativeTableRowCount {
        self.row_count
    }

    pub(crate) fn exact_page_count(&self) -> Option<u64> {
        let NativeTableRowCount::Exact(total) = self.row_count else {
            return None;
        };
        Some(if self.page_size == 0 {
            1
        } else {
            total.div_ceil(u64::from(self.page_size))
        })
    }

    /// Exact page count (legacy callers); not a paging bound for other kinds.
    pub fn pages(&self) -> u32 {
        u32::try_from(self.exact_page_count().unwrap_or(0)).unwrap_or(u32::MAX)
    }

    pub fn can_next_page(&self) -> bool {
        if !self.has_page() || self.page_size == 0 || self.page == u32::MAX {
            return false;
        }
        match self.exact_page_count() {
            Some(pages) => u64::from(self.page) + 1 < pages,
            None => self.bounded_offset && self.rows == self.page_size,
        }
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

    pub(super) fn table_scroll_y(&self) -> rune::Live<f32> {
        self.table_scroll_y.clone()
    }

    /// A4: width of one data column in points (persisted per table).
    pub fn col_w(&self, col: u32) -> f32 {
        self.col_widths
            .get(col as usize)
            .copied()
            .unwrap_or(COL_W as u32) as f32
    }

    /// A4: set one column width in points.
    pub fn set_col_w(&mut self, col: u32, width: u32) {
        let width = width.max(40);
        if self.col_widths.len() < self.cols as usize {
            self.col_widths.resize(self.cols as usize, COL_W as u32);
        }
        if let Some(slot) = self.col_widths.get_mut(col as usize) {
            *slot = width;
        }
    }

    /// A4: all column widths in order (length = cols).
    pub fn column_widths(&self) -> Vec<u32> {
        let mut widths = vec![COL_W as u32; self.cols as usize];
        for (i, w) in self.col_widths.iter().enumerate() {
            if i < widths.len() {
                widths[i] = *w;
            }
        }
        widths
    }

    /// A4: replace column widths (by index), clamped to cols.
    pub fn set_column_widths(&mut self, widths: Vec<u32>) {
        let mut normalized = vec![COL_W as u32; self.cols as usize];
        for (i, w) in widths.into_iter().enumerate() {
            if i < normalized.len() {
                normalized[i] = w.max(40);
            }
        }
        self.col_widths = normalized;
    }

    /// A4: x offset of a data column's left edge within the data sheet.
    pub fn x_of_col(&self, col: u32) -> f32 {
        let end = col.min(self.cols);
        (0..end).map(|c| self.col_w(c)).sum()
    }

    /// A4: total width of all data columns.
    pub fn sheet_data_width(&self) -> f32 {
        self.x_of_col(self.cols)
    }

    /// A4: width of a contiguous span of data columns [from, to).
    pub fn span_width(&self, from: u32, to: u32) -> f32 {
        if to <= from {
            return 0.0;
        }
        let to = to.min(self.cols);
        (from.min(self.cols)..to).map(|c| self.col_w(c)).sum()
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

    /// Only the current native table delivery can supply declared types.
    pub(crate) fn column_type(&self, col: u32) -> Option<&str> {
        self.column_types.get(col as usize).map(String::as_str).filter(|ty| !ty.trim().is_empty())
    }

    /// The column's index on the loaded page, by name.
    fn column_index(&self, name: &str) -> Option<u32> {
        self.columns
            .iter()
            .position(|column| column == name)
            .map(|index| index as u32)
    }

    /// The loaded page's sort, in the port's vocabulary (N6's batch echo).
    fn sort_name(&self) -> Option<datazen_ui_api::ColumnSort> {
        let (index, descending) = self.sort?;
        Some(datazen_ui_api::ColumnSort {
            column: self.column_name(index)?.to_owned(),
            descending,
        })
    }

    pub fn load_page(&mut self, page: &datazen_ui_api::TablePage) {
        self.load_page_with_count(
            page,
            NativeTableRowCount::Exact(u64::from(page.total_rows)),
            false,
        );
    }

    pub fn load_native_table_page(&mut self, read: &NativeTablePage) {
        self.load_page_with_count(&read.page, read.row_count, read.bounded_offset);
        if read.column_types.len() == self.columns.len() {
            self.column_types.clone_from(&read.column_types);
        }
    }

    fn load_page_with_count(
        &mut self,
        page: &datazen_ui_api::TablePage,
        row_count: NativeTableRowCount,
        bounded_offset: bool,
    ) {
        if self.editing {
            self.editing = false;
            self.editor.focused = false;
        }
        self.source = Some(page.rows.clone());
        self.columns = page.columns.clone();
        self.column_types.clear();
        self.rows = page.rows.len() as u32;
        self.cols = page.columns.len() as u32;
        self.page = page.page;
        self.page_size = page.page_size;
        self.row_count = row_count;
        self.bounded_offset = bounded_offset;
        self.pk = page.primary_key.clone();
        self.sort = page.sort.as_ref().and_then(|sort| {
            page.columns
                .iter()
                .position(|name| name == &sort.column)
                .map(|index| (index as u32, sort.descending))
        });
        self.read_only = page.read_only.clone();
        self.table = Some(page.table.clone());
        self.col_widths = vec![COL_W as u32; self.cols as usize];
        self.edits.clear();
        self.anchor = (0, 0);
        self.focus = (0, 0);
        self.clear_selection();
        self.origin_col = 0;
        self.scroll_x.set(0.0);
        self.table_scroll_y.set(0.0);
        self.clear_find();
    }

    pub fn select(&mut self, row: u32, col: u32) {
        self.drag_selection = None;
        if self.editing {
            self.commit_edit();
        }
        let row = row.min(self.rows.saturating_sub(1));
        let col = col.min(self.cols.saturating_sub(1));
        self.anchor = (row, col);
        self.focus = (row, col);
        self.selection = if self.rows > 0 && self.cols > 0 {
            Selection::Cells
        } else {
            Selection::None
        };
    }

    /// Extend an existing cell range from its original anchor. A row
    /// selection is not a cell anchor, so shift-click starts a new range.
    pub fn click_cell(&mut self, row: u32, col: u32, extend: bool) {
        if !extend || self.selection != Selection::Cells || self.editing
            || row >= self.rows || col >= self.cols
        {
            self.select(row, col);
            return;
        }
        self.focus = (row, col);
    }

    /// Only a real drag (not a click or an inline edit) takes ownership of the
    /// cell selection. Save the previous selection so Escape can cancel it.
    pub(super) fn begin_cell_drag(&mut self, row: u32, col: u32) -> bool {
        if self.editing || row >= self.rows || col >= self.cols {
            return false;
        }
        if self.drag_selection.is_none() {
            let saved = DragSelection {
                selection: self.selection,
                anchor: self.anchor,
                focus: self.focus,
                scroll: (self.scroll_x.get(), self.table_scroll_y.get()),
            };
            self.select(row, col);
            self.drag_selection = Some(saved);
        }
        true
    }

    pub(super) fn extend_cell_drag(&mut self, row: u32, col: u32) {
        if self.drag_selection.is_some() && row < self.rows && col < self.cols {
            self.focus = (row, col);
        }
    }

    pub(super) fn finish_cell_drag(&mut self, cancelled: bool) {
        if let Some(saved) = self.drag_selection.take() {
            if cancelled {
                self.selection = saved.selection;
                self.anchor = saved.anchor;
                self.focus = saved.focus;
            }
        }
    }

    /// The scroll viewport deliberately cancels rather than extending through
    /// recycled cells. The app also calls this when the grid is unmounted.
    pub(crate) fn cancel_table_drag_if_needed(&mut self, mounted: bool) -> bool {
        let displaced = self.drag_selection.as_ref().is_some_and(|drag| {
            let (x, y) = drag.scroll;
            !mounted || (x - self.scroll_x.get()).abs() > 0.5
                || (y - self.table_scroll_y.get()).abs() > 0.5
        });
        if displaced {
            self.finish_cell_drag(true);
        }
        displaced
    }

    /// Row selection has no active cell; anchor and focus remain navigation
    /// coordinates, not endpoints of the selected row.
    pub fn select_row(&mut self, row: u32) {
        if row >= self.rows || self.cols == 0 {
            return;
        }
        if self.editing {
            self.commit_edit();
        }
        self.drag_selection = None;
        self.selection = Selection::Row(row);
    }

    pub fn selected_row(&self) -> Option<u32> {
        match self.selection {
            Selection::Row(row) => Some(row),
            _ => None,
        }
    }

    /// Navigation coordinates can exist even though no cells are selected.
    pub fn has_selection(&self) -> bool {
        self.selection != Selection::None && self.rows > 0 && self.cols > 0
    }

    pub fn clear_selection(&mut self) {
        self.drag_selection = None;
        self.selection = Selection::None;
        self.anchor = self.focus;
    }

    pub fn begin_edit(&mut self) {
        if !self.editable || self.rows == 0 || self.cols == 0 || self.selected_row().is_some() {
            return;
        }
        if self.editing {
            self.commit_edit();
        }
        self.selection = Selection::Cells;
        self.editor = TextEdit::new(self.cell(self.focus.0, self.focus.1));
        self.editor.focused = true;
        self.editing = true;
    }

    /// Cancel the transient cell editor without changing its staged or source value.
    pub fn cancel_edit(&mut self) {
        self.editing = false;
        self.editor.focused = false;
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
    pub fn original(&self, row: u32, col: u32) -> String {
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

    /// The staged edits as one batch for the commit use case: (row, column
    /// name, value), rows numbered as the loaded page numbers them.
    pub fn staged_edits(&self) -> Vec<(u32, String, String)> {
        let mut edits: Vec<(u32, String, String)> = self
            .edits
            .iter()
            .filter_map(|((row, col), value)| {
                self.column_name(*col)
                    .map(|column| (*row, column.to_owned(), value.clone()))
            })
            .collect();
        edits.sort_by_key(|(row, _, _)| *row);
        edits
    }

    /// The staged edits as one commit batch (N6): each edited row addressed
    /// by the primary-key values the page showed, each change carrying the
    /// original the page loaded — the model the commit use case validates
    /// against, so a row that changed underneath fails as a stale original
    /// instead of being silently overwritten.
    pub fn commit_batch(&self) -> Option<datazen_ui_api::CellEditBatch> {
        let table = self.table_label().to_owned();
        if table.is_empty() {
            return None;
        }
        let staged = self.staged_edits();
        if staged.is_empty() {
            return None;
        }
        let mut rows: Vec<datazen_ui_api::RowEdit> = Vec::new();
        for (row, column, value) in staged {
            let original = self.original(row, self.column_index(&column)?);
            let entry = match rows.iter_mut().find(|edit| edit.index == row) {
                Some(entry) => entry,
                None => {
                    // The row's identity: each PK column's value as the page
                    // showed it. A missing PK column name means the page
                    // never carried a usable key — the host will refuse the
                    // commit, which is the honest answer.
                    let identity = self
                        .pk
                        .iter()
                        .map(|name| {
                            let index = self.column_index(name).unwrap_or(u32::MAX);
                            let value = if index == u32::MAX {
                                String::new()
                            } else {
                                self.original(row, index)
                            };
                            (name.clone(), value)
                        })
                        .collect();
                    rows.push(datazen_ui_api::RowEdit {
                        index: row,
                        identity,
                        changes: Vec::new(),
                    });
                    rows.last_mut().expect("just pushed")
                }
            };
            entry.changes.push((column, original, value));
        }
        Some(datazen_ui_api::CellEditBatch {
            table,
            page: self.page,
            page_size: self.page_size,
            sort: self.sort_name(),
            filter: None,
            rows,
        })
    }

    pub fn copy_tsv(&self) -> String {
        if !self.has_selection() {
            return String::new();
        }
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
        if self.selected_row().is_some() {
            return;
        }
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
                let Some(row) = (row0 as usize).checked_add(dr) else {
                    continue;
                };
                let Some(col) = (col0 as usize).checked_add(dc) else {
                    continue;
                };
                if row < self.rows as usize && col < self.cols as usize {
                    self.edits
                        .insert((row as u32, col as u32), value.to_owned());
                }
            }
        }
    }

    /// Move the column window to the horizontal scroll offset. True when the window changed,
    /// so the view has to build different cells.
    pub fn sync_scroll(&mut self) -> bool {
        let x = self.scroll_x.get().max(0.0);
        let mut origin = 0u32;
        let mut acc = 0.0f32;
        if self.cols == 0 {
            return false;
        }
        for c in 0..self.cols {
            let next = acc + self.col_w(c);
            if next > x + 0.5 {
                origin = c;
                break;
            }
            acc = next;
            origin = c.saturating_add(1).min(self.cols.saturating_sub(1));
        }
        let origin = origin.min(self.cols.saturating_sub(1));
        if origin == self.origin_col {
            false
        } else {
            self.origin_col = origin;
            true
        }
    }

    pub fn nudge(&mut self, dr: i32, dc: i32, extend: bool) {
        if self.editing || self.rows == 0 || self.cols == 0 {
            return;
        }
        let row = self.selected_row().unwrap_or(self.focus.0);
        let r = (row as i32 + dr).clamp(0, self.rows as i32 - 1) as u32;
        let c = (self.focus.1 as i32 + dc).clamp(0, self.cols as i32 - 1) as u32;
        self.focus = (r, c);
        if !extend || self.selection != Selection::Cells {
            self.anchor = (r, c);
        }
        self.selection = Selection::Cells;
        self.reveal_focus();
    }

    fn reveal_focus(&mut self) {
        let col = self.focus.1;
        let end = self.origin_col.saturating_add(COL_WINDOW);
        if col < self.origin_col || col >= end {
            self.origin_col = col.saturating_sub(COL_WINDOW / 2);
            self.scroll_x.set(self.x_of_col(self.origin_col));
        }
    }

    pub fn rect(&self) -> (u32, u32, u32, u32) {
        if let Some(row) = self.selected_row() {
            return (row, row, 0, self.cols.saturating_sub(1));
        }
        let r0 = self.anchor.0.min(self.focus.0);
        let r1 = self.anchor.0.max(self.focus.0);
        let c0 = self.anchor.1.min(self.focus.1);
        let c1 = self.anchor.1.max(self.focus.1);
        (r0, r1, c0, c1)
    }

    pub fn covers(&self, row: u32, col: u32) -> bool {
        if !self.has_selection() {
            return false;
        }
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

    /// A5: clear the active find — no hits, no highlight.
    pub fn clear_find(&mut self) {
        self.find_needle.clear();
        self.find_hits.clear();
        self.find_index = None;
    }

    /// A5: the search needle currently applied to this grid.
    pub fn find_needle(&self) -> &str {
        &self.find_needle
    }

    /// A5: total number of matching cells.
    pub fn find_hit_count(&self) -> usize {
        self.find_hits.len()
    }

    /// A5: 1-based index of the current hit, or 0 when there are no hits.
    pub fn find_current_index(&self) -> usize {
        self.find_index.map(|i| i + 1).unwrap_or(0)
    }

    /// A5: the current hit's cell, if any.
    pub fn current_find_hit(&self) -> Option<(u32, u32)> {
        self.find_index.and_then(|i| self.find_hits.get(i).copied())
    }

    /// A5: true when the cell is any match of the active find.
    pub fn is_find_hit(&self, row: u32, col: u32) -> bool {
        !self.find_hits.is_empty() && self.find_hits.contains(&(row, col))
    }

    /// A5: true when the cell is the current (navigated) match.
    pub fn is_current_find_hit(&self, row: u32, col: u32) -> bool {
        self.current_find_hit() == Some((row, col))
    }

    /// A5: recompute find hits for `needle` across the whole grid in row-major
    /// order. An empty needle clears the find. The current hit becomes the first
    /// hit at or after the focused cell (wrapping to the first hit) so navigation
    /// continues from the user's position; focus moves to that hit.
    pub fn set_find_needle(&mut self, needle: &str) {
        let needle = needle.to_owned();
        self.find_needle = needle;
        self.recompute_find_hits(true);
    }

    fn recompute_find_hits(&mut self, move_focus: bool) {
        self.find_hits.clear();
        self.find_index = None;
        let needle = self.find_needle.trim().to_owned();
        if needle.is_empty() {
            return;
        }
        let needle_lc = needle.to_lowercase();
        for r in 0..self.rows {
            for c in 0..self.cols {
                if self.cell(r, c).to_lowercase().contains(&needle_lc) {
                    self.find_hits.push((r, c));
                }
            }
        }
        if self.find_hits.is_empty() {
            return;
        }
        // Pick the first hit at/after focus so search feels anchored.
        let start = self.focus;
        let idx = self
            .find_hits
            .iter()
            .position(|&(r, c)| (r, c) >= start)
            .unwrap_or(0);
        self.find_index = Some(idx);
        if move_focus {
            self.focus_find_hit();
        }
    }

    fn focus_find_hit(&mut self) {
        if let Some((r, c)) = self.current_find_hit() {
            self.anchor = (r, c);
            self.focus = (r, c);
            self.selection = Selection::Cells;
            self.reveal_focus();
        }
    }

    /// A5: advance to the next hit, wrapping around. Returns true when a hit
    /// was selected.
    pub fn find_next(&mut self) -> bool {
        if self.find_hits.is_empty() {
            return false;
        }
        let next = match self.find_index {
            Some(i) => (i + 1) % self.find_hits.len(),
            None => 0,
        };
        self.find_index = Some(next);
        self.focus_find_hit();
        true
    }

    /// A5: move to the previous hit, wrapping around. Returns true when a hit
    /// was selected.
    pub fn find_prev(&mut self) -> bool {
        if self.find_hits.is_empty() {
            return false;
        }
        let len = self.find_hits.len();
        let prev = match self.find_index {
            Some(0) | None => len - 1,
            Some(i) => i - 1,
        };
        self.find_index = Some(prev);
        self.focus_find_hit();
        true
    }
}
