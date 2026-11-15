use std::collections::HashMap;

use rune::TextEdit;

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
    pub(crate) anchor: (u32, u32),
    pub(crate) focus: (u32, u32),
    pub(crate) editing: bool,
    editable: bool,
    page: u32,
    page_size: u32,
    total_rows: u32,
    /// The sort the loaded page was opened with, as a column index.
    sort: Option<(u32, bool)>,
    /// The loaded page's primary-key column names (N6): the commit batch
    /// addresses rows by them.
    pk: Vec<String>,
    /// Why the loaded page is read-only; None means writable.
    read_only: Option<String>,
    /// The loaded page's table name, for panel titles.
    table: Option<String>,
    pub(crate) editor: TextEdit,
    /// Horizontal scroll, in points. `on_scroll_offset` writes it; it cannot touch this struct.
    scroll_x: rune::Live<f32>,
    origin_col: u32,
}

impl std::fmt::Debug for DataGrid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DataGrid")
            .field("rows", &self.rows)
            .field("cols", &self.cols)
            .field("page", &self.page)
            .field("page_size", &self.page_size)
            .field("total_rows", &self.total_rows)
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
            anchor: (0, 0),
            focus: (0, 0),
            editing: false,
            editable: true,
            page: 0,
            page_size: 0,
            total_rows: 0,
            sort: None,
            pk: Vec::new(),
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
        self.pk = page.primary_key.clone();
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
    pub(crate) fn original(&self, row: u32, col: u32) -> String {
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

    pub(crate) fn nudge(&mut self, dr: i32, dc: i32, extend: bool) {
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

    pub(crate) fn rect(&self) -> (u32, u32, u32, u32) {
        let r0 = self.anchor.0.min(self.focus.0);
        let r1 = self.anchor.0.max(self.focus.0);
        let c0 = self.anchor.1.min(self.focus.1);
        let c1 = self.anchor.1.max(self.focus.1);
        (r0, r1, c0, c1)
    }

    pub(crate) fn covers(&self, row: u32, col: u32) -> bool {
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
