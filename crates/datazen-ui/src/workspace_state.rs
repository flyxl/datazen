//! AppState method implementations.

#![allow(unused_imports)]
use crate::state_def::*;
use crate::views::{next_connection, workflow_yaml};
use crate::constants::*;
use crate::data_grid::DataGrid;
use crate::sql_editor::SqlEditor;
use crate::types::{Section, TableView, Tool};
use crate::{icons, splitter};
use datazen_ui_api::{
    AdapterDelivery, AdapterJob, Catalog, ColumnDraft, ColumnFilter, ColumnSort, ConnectionProfile,
    DashboardWidget, DESKTOP_NOT_IMPLEMENTED, DesktopAdapter, MemoryWorkspace, ProgressSnapshot,
    QueryApi, Setting, SettingsApi, TabKind, Tabs, TablePage, UnimplementedAdapter, Workflow,
    WorkflowStepResult, WorkspaceApi,
};
#[cfg(not(target_arch = "wasm32"))]
use datazen_ui_api::UseCaseAdapter;
use rune::tok::{self, type_};
use rune::{
    App, AppSpec, Color, Cx, El, FileDialogOptions, FileDialogResult, Ime, Key, KeyEvent,
    TextEdit, Titlebar, Tx, col, oklch, row, text, text_input,
};

impl AppState {
    pub(crate) fn show_table(&mut self, title: &str) -> bool {
        // N1: a live host takes the read as a job and answers on its own
        // schedule. Until the delivery lands the grid keeps its previous
        // page and the footer says the read is in flight; a declined job
        // falls through to the synchronous paths below (H4's contract).
        let generation = self.next_read_generation();
        let job = AdapterJob::ReadTable {
            generation,
            table: title.to_owned(),
            page: self.table_page,
            page_size: self.table_page_size(),
            sort: self.sort.clone(),
            filter: self.filter.clone(),
        };
        if self.desktop.begin_job(job) {
            self.table_loading = Some(generation);
            self.active = Section::Workspace;
            return true;
        }
        // H4: the desktop adapter's read use case first; the port's memory
        // use case answers while the adapter is unwired. When both fail the
        // grid keeps its previous page.
        let page = self.table_page;
        let size = self.table_page_size();
        let sort = self.sort.clone();
        let filter = self.filter.clone();
        let read = self
            .desktop
            .read_table_page(title, page, size, sort.as_ref(), filter.as_ref())
            .or_else(|_| {
                self.workspace
                    .open_table(title, page, size, sort.as_ref(), filter.as_ref())
            });
        match read {
            Ok(page) => {
                self.table_page = page.page;
                self.grid.load_page(&page);
                self.grid.set_editable(page.read_only.is_none());
                self.apply_table_column_widths();
                self.active = Section::Workspace;
                true
            }
            Err(_) => false,
        }
    }

    /// Reopen the selected table tab at the current page, sort and filter.
    pub(crate) fn reload_table_page(&mut self) {
        if let Some(tab) = self.tabs.selected() {
            if tab.kind == TabKind::Table {
                self.show_table(&tab.title.clone());
            }
        }
    }
    pub(crate) fn next_table_page(&mut self) {
        if !self.grid.has_page() || self.grid.page() + 1 >= self.grid.pages() {
            return;
        }
        self.table_page += 1;
        self.reload_table_page();
    }
    pub(crate) fn prev_table_page(&mut self) {
        if !self.grid.has_page() || self.grid.page() == 0 {
            return;
        }
        self.table_page = self.table_page.saturating_sub(1);
        self.reload_table_page();
    }

    /// A header click cycles its column: none, ascending, descending, back to none.
    pub(crate) fn cycle_sort(&mut self, column: u32) {
        let Some(name) = self.grid.column_name(column).map(str::to_owned) else {
            return;
        };
        let next = match &self.sort {
            Some(sort) if sort.column == name && !sort.descending => {
                Some(ColumnSort { column: name, descending: true })
            }
            Some(sort) if sort.column == name => None,
            _ => Some(ColumnSort { column: name, descending: false }),
        };
        self.sort = next;
        self.table_page = 0;
        self.reload_table_page();
    }

    /// The filter bar's Enter: read the editor, target the chosen column, return to page 1.
    pub(crate) fn apply_filter(&mut self) {
        let column = self.grid.column_name(self.filter_column).map(str::to_owned);
        let text = self.filter_editor.text.trim().to_owned();
        self.filter = match (column, text.is_empty()) {
            (Some(column), false) => Some(ColumnFilter { column, contains: text }),
            _ => None,
        };
        self.table_page = 0;
        self.reload_table_page();
    }
    pub(crate) fn cycle_filter_column(&mut self) {
        if self.grid.cols == 0 {
            return;
        }
        self.filter_column = (self.filter_column + 1) % self.grid.cols;
        if self.filter.is_some() {
            self.apply_filter();
        }
    }

    /// Commit every staged edit to the stored rows, then reload the page.
    pub(crate) fn commit_staged_edits(&mut self) {
        // N1/N6: a live host takes the batch as a job. The dirty marks stay
        // until the delivery lands — the job twin of H5's rule that a real
        // failure keeps them.
        if let Some(batch) = self.grid.commit_batch() {
            let generation = self.read_generation;
            if self.desktop.begin_job(AdapterJob::CommitCells { generation, batch }) {
                self.commit_loading = Some(generation);
                return;
            }
        }
        // H5: the desktop adapter's commit use case first — the grid's staged
        // edits travel as one batch. The fixed not-implemented error means
        // the host has not wired the use case yet, and the port's own staged
        // edits answer instead; any other failure is real and keeps the
        // dirty marks (no reload, so the grid keeps its staged state).
        let table = self.grid.table_label().to_owned();
        let edits = self.grid.staged_edits();
        match self.desktop.commit_cells(&table, &edits) {
            Ok(_) => self.reload_table_page(),
            Err(error) if error == DESKTOP_NOT_IMPLEMENTED => {
                if self.workspace.commit_staged().is_ok() {
                    self.reload_table_page();
                }
            }
            Err(_) => {}
        }
    }

    /// Drop every staged edit and reload the page back to the stored values.
    pub(crate) fn rollback_staged_edits(&mut self) {
        self.workspace.rollback_staged();
        self.reload_table_page();
    }

    /// Load a query result into the shared grid, read-only, below the editor.
    pub(crate) fn load_result_page(&mut self, columns: Vec<String>, rows: Vec<Vec<String>>) {
        let page = self.workspace.open_result(columns, rows);
        let mut grid = DataGrid::new();
        grid.load_page(&page);
        grid.set_editable(false);
        self.result_grid = Some(grid);
        // T1: a fresh result starts with the editor holding the keyboard.
        self.result_focused = false;
    }

    /// Bump and return the next query generation. Every run and every cancel
    /// starts one.
    pub(crate) fn next_query_generation(&mut self) -> u32 {
        self.query_generation += 1;
        self.query_generation
    }

    /// A result page arrives for a generation. Stale generations — superseded by
    /// a newer run or invalidated by a cancel — are dropped on the floor.
    pub(crate) fn deliver_query_result(&mut self, generation: u32, page: TablePage) {
        if generation != self.query_generation {
            return;
        }
        self.load_result_page(page.columns, page.rows);
    }

    /// Cancel the in-flight query: bumping the generation invalidates every
    /// outstanding one, so their results are dropped when they arrive. A
    /// query job in flight also stops waiting: its marker leaves with the
    /// generation, and a late delivery finds only the stale check.
    pub(crate) fn cancel_query(&mut self) {
        self.next_query_generation();
        self.query_loading = None;
    }

    /// Run the statement at the cursor. A failed execution changes nothing; a
    /// successful one lands in the result grid only if still current.
    pub(crate) fn run_query(&mut self) {
        let generation = self.next_query_generation();
        let Some(sql) = self.sql.statement_at_cursor() else {
            return;
        };
        // N1: a live host runs the statement as a job; the editor keeps its
        // document and the result area says the query is in flight until the
        // delivery lands (dropped when its generation went stale, as in Q5).
        let job = AdapterJob::RunQuery { generation, sql: sql.clone() };
        if self.desktop.begin_job(job) {
            self.query_loading = Some(generation);
            return;
        }
        match self.workspace.execute(&sql) {
            Ok(page) => self.deliver_query_result(generation, page),
            Err(_) => {}
        }
    }

    /// Bump and return the next table read generation. Every read request —
    /// a fresh open, a page turn, a reload — starts one, so a delivery from
    /// a read the state has moved past is dropped (N1, plan invariant 3).
    pub(crate) fn next_read_generation(&mut self) -> u32 {
        self.read_generation += 1;
        self.read_generation
    }

    /// Bump and return the next structure generation (E1). Every structure
    /// request — a table open, a view switch, an ER open — starts one, so a
    /// delivery from a read the table's entry has moved past is dropped.
    pub(crate) fn next_structure_generation(&mut self) -> u32 {
        self.structure_generation += 1;
        self.structure_generation
    }

    /// E1: ask for one table's structure as a job — the four reads (DDL,
    /// columns, indexes, foreign keys) travel together and land as one
    /// delivery. A host that declines the job keeps the synchronous reads:
    /// the decline promises the synchronous methods serve, and the demo
    /// process's memory port is memory-speed — the same cache fills on the
    /// spot and the render path stays adapter-free either way.
    pub(crate) fn begin_structure_read(&mut self, table: &str) {
        let generation = self.next_structure_generation();
        let job = AdapterJob::ReadStructure { generation, table: table.to_owned() };
        if self.desktop.begin_job(job) {
            self.structures.insert(
                table.to_owned(),
                StructureState { generation, loading: true, error: None, data: None },
            );
            return;
        }
        let data = datazen_ui_api::TableStructure {
            ddl: self.structure_ddl(table).unwrap_or_default(),
            columns: self.structure_columns(table).unwrap_or_default(),
            indexes: self.structure_indexes(table).unwrap_or_default(),
            foreign_keys: self.structure_foreign_keys(table).unwrap_or_default(),
        };
        self.structures.insert(
            table.to_owned(),
            StructureState { generation, loading: false, error: None, data: Some(data) },
        );
    }

    /// Bump and return the next widget-bars generation (E2). Every bars
    /// request — a dashboards open, a widget save, a manual refresh —
    /// starts one, so a delivery from a read the widget's entry has moved
    /// past is dropped.
    pub(crate) fn next_widget_bars_generation(&mut self) -> u32 {
        self.widget_bars_generation += 1;
        self.widget_bars_generation
    }

    /// E2: ask for one widget's bars as a job — the widget's query runs
    /// over the live session on the worker's runtime. A host that declines
    /// the job keeps the synchronous bars read (the demo process's memory
    /// port) and the same cache fills on the spot.
    pub(crate) fn begin_widget_bars_read(&mut self, id: u64) {
        let generation = self.next_widget_bars_generation();
        let job = AdapterJob::WidgetBars { generation, id };
        if self.desktop.begin_job(job) {
            self.widget_bars.insert(
                id,
                WidgetBarsState { generation, loading: true, error: None, bars: None },
            );
            return;
        }
        let bars = self.host_widget_bars(id);
        self.widget_bars.insert(
            id,
            WidgetBarsState { generation, loading: false, error: None, bars: Some(bars) },
        );
    }

    /// E2: ask for every widget's bars — the page's open, a save and the
    /// manual refresh button all restart the whole chart set.
    pub(crate) fn refresh_widget_bars(&mut self) {
        let ids: Vec<u64> = self.host_widgets().iter().map(|widget| widget.id).collect();
        for id in ids {
            self.begin_widget_bars_read(id);
        }
    }

    /// A host job's answer arrived on the poll path. The loading marker of
    /// the finished request always clears; the state itself is written only
    /// when the delivery's generation is still the current one.
    pub(crate) fn deliver_host_job(&mut self, delivery: AdapterDelivery) {
        match delivery {
            AdapterDelivery::ConnectionOpened { generation, id, catalog } => {
                if self.connect_loading == Some(generation) {
                    self.connect_loading = None;
                }
                if generation != self.read_generation {
                    return;
                }
                match catalog {
                    Ok(catalog) => {
                        // The session opened: the live catalog is the truth, the
                        // workspace starts fresh and the first table opens
                        // through the same read path as every other one.
                        // E1: a new session is a new truth — the structure
                        // cache empties with it.
                        self.structures.clear();
                        // E2: the widgets' queries bind the connection —
                        // their bars reload with the new session.
                        self.widget_bars.clear();
                        // E3: the new session's catalog walks start clean.
                        self.inbound_refs = None;
                        self.inbound_loading = None;
                        self.fk_error = None;
                        self.fk_breadcrumbs.clear();
                        self.collapsed.clear();
                        self.table_page = 0;
                        self.sort = None;
                        self.filter = None;
                        self.filter_column = 0;
                        self.filter_editor = TextEdit::new(String::new());
                        self.desktop_catalog = Some(catalog.clone());
                        self.desktop_connection = Some(id);
                        self.active = Section::Workspace;
                        let first = catalog
                            .databases
                            .first()
                            .and_then(|database| database.schemas.first())
                            .and_then(|schema| schema.tables.first().cloned());
                        if let Some(table) = first {
                            self.show_table(&table);
                        }
                    }
                    Err(sentence) => {
                        // The page never left the connections list; the
                        // sentence lands in the adapter's notice row.
                        self.desktop_error = Some(sentence);
                    }
                }
            }
            AdapterDelivery::ValueSearched { generation, hits } => {
                if self.value_search_loading == Some(generation) {
                    self.value_search_loading = None;
                }
                if generation != self.value_search_generation {
                    return;
                }
                match hits {
                    Ok(hits) => {
                        self.value_search_hits = hits;
                        self.value_search_error = None;
                    }
                    Err(error) => {
                        self.value_search_hits.clear();
                        self.value_search_error = Some(error);
                    }
                }
            }
            AdapterDelivery::TablePage { generation, page } => {
                if self.table_loading == Some(generation) {
                    self.table_loading = None;
                }
                if generation != self.read_generation {
                    return;
                }
                if let Ok(page) = page {
                    self.table_page = page.page;
                    self.grid.load_page(&page);
                    self.grid.set_editable(page.read_only.is_none());
                    self.apply_table_column_widths();
                }
            }
            AdapterDelivery::StructureRead { generation, table, structure } => {
                // E1: the answer lands only on the entry still waiting for
                // this generation — a newer read (a view switch, a reopen,
                // the DDL drafts' apply) replaced the entry and the late
                // answer drops on the floor.
                let Some(entry) = self.structures.get_mut(&table) else {
                    return;
                };
                if entry.generation != generation {
                    return;
                }
                entry.loading = false;
                match structure {
                    Ok(data) => {
                        entry.data = Some(data);
                        entry.error = None;
                    }
                    Err(error) => {
                        entry.data = None;
                        entry.error = Some(error);
                    }
                }
            }
            AdapterDelivery::WidgetBarsRead { generation, id, bars } => {
                // E2: the answer lands only on the entry still waiting for
                // this generation — a newer request (a reopen, a save, the
                // refresh button) replaced the entry and the late answer
                // drops.
                let Some(entry) = self.widget_bars.get_mut(&id) else {
                    return;
                };
                if entry.generation != generation {
                    return;
                }
                entry.loading = false;
                match bars {
                    Ok(bars) => {
                        entry.bars = Some(bars);
                        entry.error = None;
                    }
                    Err(error) => {
                        entry.bars = None;
                        entry.error = Some(error);
                    }
                }
            }
            AdapterDelivery::InboundRefsRead { generation, table, refs } => {
                // E3: the scan's answer lands on the current generation and
                // the loaded table only — a newer ask replaced the request,
                // and a table the grid has moved past keeps its list to
                // itself.
                if self.inbound_loading == Some(generation) {
                    self.inbound_loading = None;
                }
                if generation != self.inbound_generation || table != self.grid.table_label() {
                    return;
                }
                match refs {
                    Ok(refs) => {
                        self.inbound_refs = Some(refs);
                        self.fk_error = None;
                    }
                    Err(error) => {
                        self.inbound_refs = None;
                        self.fk_error = Some(error);
                    }
                }
            }
            AdapterDelivery::CommitCount { generation, count } => {
                if self.commit_loading == Some(generation) {
                    self.commit_loading = None;
                }
                if generation != self.read_generation || count.is_err() {
                    return;
                }
                self.reload_table_page();
            }
            AdapterDelivery::QueryPage { generation, page } => {
                if self.query_loading == Some(generation) {
                    self.query_loading = None;
                }
                if let Ok(page) = page {
                    self.deliver_query_result(generation, page);
                }
            }
            AdapterDelivery::WorkflowRun { generation, results } => {
                if self.workflow_loading == Some(generation) {
                    self.workflow_loading = None;
                }
                if generation != self.workflow_generation {
                    return;
                }
                match results {
                    Ok(results) => self.workflow_results = results,
                    Err(_) => self.workflow_results.clear(),
                }
            }
        }
    }

    /// Any tab event lands in the workspace: the selected tab decides what the
    /// workspace shows, so the tab strip and its content stay on one page.
    pub(crate) fn sync_grid_to_selected_tab(&mut self) {
        self.active = Section::Workspace;
        self.er_schema = None;
        self.table_page = 0;
        self.filter = None;
        self.filter_column = 0;
        self.filter_editor = TextEdit::new(String::new());
        let selected = self.tabs.selected().map(|tab| (tab.kind, tab.title.clone()));
        match selected {
            Some((TabKind::Table, title)) => {
                self.show_table(&title);
            }
            Some((TabKind::Query, _)) | None => {}
        }
    }
    pub(crate) fn close_tab(&mut self, id: u32) -> bool {
        let previous = self.tabs.selected().map(|tab| tab.id);
        if !self.tabs.close(id) {
            return false;
        }
        if self.tabs.selected().map(|tab| tab.id) != previous {
            self.sync_grid_to_selected_tab();
        }
        true
    }
    pub(crate) fn commit_grid_edit(&mut self) {
        if !self.grid.is_editable() {
            return;
        }
        let (row, col) = self.grid.focus();
        let column = self.grid.column_name(col).map(str::to_owned);
        self.grid.commit_edit();
        if let Some(column) = column {
            let value = self.grid.cell(row, col);
            let _ = self.workspace.stage_cell(row, &column, value);
        }
    }
}
