//! AppState method implementations.

#![allow(unused_imports)]
use crate::constants::*;
use crate::data_grid::DataGrid;
use crate::sql_editor::SqlEditor;
use crate::state_def::*;
use crate::types::{Section, TableView, Tool};
use crate::views::{next_connection, workflow_yaml};
use crate::{icons, splitter};
use datazen_ui_api::{
    AdapterDelivery, AdapterJob, Catalog, ColumnDraft, ColumnFilter, ColumnSort, ConnectionProfile,
    DashboardWidget, DesktopAdapter, MemoryWorkspace, ProgressSnapshot, QueryApi, Setting,
    SettingsApi, TabKind, TablePage, Tabs, UnimplementedAdapter, Workflow, WorkflowStepResult,
    WorkspaceApi, DESKTOP_NOT_IMPLEMENTED,
};
use rune::tok::{self, type_};
use rune::{
    col, oklch, row, text, text_input, App, AppSpec, Color, Cx, El, FileDialogOptions,
    FileDialogResult, Ime, Key, KeyEvent, TextEdit, Titlebar, Tx,
};

impl AppState {
    pub fn show_table(&mut self, title: &str) -> bool {
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
            self.table_read_error = None;
            if self.grid.table_label() != title {
                self.grid = DataGrid::new();
                self.grid.set_editable(false);
            }
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
    pub fn reload_table_page(&mut self) {
        if let Some(tab) = self.tabs.selected() {
            if tab.kind == TabKind::Table {
                self.show_table(&tab.title.clone());
            }
        }
    }
    pub fn next_table_page(&mut self) {
        if !self.grid.has_page() || self.grid.page() + 1 >= self.grid.pages() {
            return;
        }
        self.table_page += 1;
        self.reload_table_page();
    }
    pub fn prev_table_page(&mut self) {
        if !self.grid.has_page() || self.grid.page() == 0 {
            return;
        }
        self.table_page = self.table_page.saturating_sub(1);
        self.reload_table_page();
    }

    /// A header click cycles its column: none, ascending, descending, back to none.
    pub fn cycle_sort(&mut self, column: u32) {
        let Some(name) = self.grid.column_name(column).map(str::to_owned) else {
            return;
        };
        let next = match &self.sort {
            Some(sort) if sort.column == name && !sort.descending => Some(ColumnSort {
                column: name,
                descending: true,
            }),
            Some(sort) if sort.column == name => None,
            _ => Some(ColumnSort {
                column: name,
                descending: false,
            }),
        };
        self.sort = next;
        self.table_page = 0;
        self.reload_table_page();
    }

    /// The filter bar's Enter: read the editor, target the chosen column, return to page 1.
    pub fn apply_filter(&mut self) {
        let column = self.grid.column_name(self.filter_column).map(str::to_owned);
        let text = self.filter_editor.text.trim().to_owned();
        self.filter = match (column, text.is_empty()) {
            (Some(column), false) => Some(ColumnFilter {
                column,
                contains: text,
            }),
            _ => None,
        };
        self.table_page = 0;
        self.reload_table_page();
    }
    pub fn cycle_filter_column(&mut self) {
        if self.grid.cols == 0 {
            return;
        }
        self.filter_column = (self.filter_column + 1) % self.grid.cols;
        if self.filter.is_some() {
            self.apply_filter();
        }
    }

    /// Commit every staged edit to the stored rows, then reload the page.
    pub fn commit_staged_edits(&mut self) {
        // N1/N6: a live host takes the batch as a job. The dirty marks stay
        // until the delivery lands — the job twin of H5's rule that a real
        // failure keeps them.
        if let Some(batch) = self.grid.commit_batch() {
            let generation = self.read_generation;
            if self
                .desktop
                .begin_job(AdapterJob::CommitCells { generation, batch })
            {
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
    pub fn rollback_staged_edits(&mut self) {
        self.workspace.rollback_staged();
        self.reload_table_page();
    }

    fn park_selected_query_result_grid(&mut self) {
        let grid = self.result_grid.take();
        if let Some(id) = self.selected_query_result {
            if let Some(tab) = self.query_result_tabs.iter_mut().find(|t| t.id == id) {
                tab.grid = grid;
                return;
            }
        }
        // No selected tab: keep legacy grid only (should be rare).
        self.result_grid = grid;
    }

    fn restore_selected_query_result_grid(&mut self) {
        self.result_grid = None;
        if let Some(id) = self.selected_query_result {
            if let Some(index) = self.query_result_tabs.iter().position(|t| t.id == id) {
                self.result_grid = self.query_result_tabs[index].grid.take();
            }
        }
    }

    fn ensure_query_result_tab_for_generation(&mut self, generation: u32) -> u32 {
        if let Some(id) = self
            .query_result_tabs
            .iter()
            .find(|t| t.generation == Some(generation))
            .map(|t| t.id)
        {
            return id;
        }
        self.add_query_result_tab(Some(generation))
    }

    fn add_query_result_tab(&mut self, generation: Option<u32>) -> u32 {
        let id = self.next_query_result_id;
        self.next_query_result_id += 1;
        let label = format!("Result {}", self.query_result_tabs.len() + 1);
        self.query_result_tabs.push(QueryResultTab {
            id,
            label,
            generation,
            execution_stats: None,
            error_key: None,
            error_copied_until: None,
            csv_source: None,
            csv_status: None,
            grid: None,
            pinned: false,
        });
        id
    }

    /// A3: select a query result sub-tab by id.
    pub fn select_query_result_tab(&mut self, id: u32) -> bool {
        if !self.query_result_tabs.iter().any(|t| t.id == id) {
            return false;
        }
        self.park_selected_query_result_grid();
        self.selected_query_result = Some(id);
        self.restore_selected_query_result_grid();
        self.result_focused = false;
        true
    }

    /// A3: toggle pin state for a result sub-tab.
    pub fn toggle_pin_query_result_tab(&mut self, id: u32) {
        if let Some(tab) = self.query_result_tabs.iter_mut().find(|t| t.id == id) {
            tab.pinned = !tab.pinned;
        }
    }

    /// A3: close a result sub-tab (pinned or not) if present.
    pub fn close_query_result_tab(&mut self, id: u32) -> bool {
        let Some(index) = self.query_result_tabs.iter().position(|t| t.id == id) else {
            return false;
        };
        let was_selected = self.selected_query_result == Some(id);
        if was_selected {
            // drop active grid (it is not parked in the tab)
            self.result_grid = None;
            self.selected_query_result = None;
        }
        self.query_result_tabs.remove(index);
        if was_selected {
            self.selected_query_result = self.query_result_tabs.last().map(|t| t.id);
            self.restore_selected_query_result_grid();
            self.result_focused = false;
        }
        true
    }

    /// A3: honest toolbar affordances when no host capability exists.
    pub fn query_action_notice(&mut self, notice: impl Into<String>) {
        self.query_notice = Some(notice.into());
    }

    pub fn format_query(&mut self) {
        // No formatter capability in the current Rune/editor contract; keep the
        // document unchanged and report honestly.
        self.query_action_notice("Format is not available in this host");
    }

    pub fn explain_query(&mut self) {
        if !self.desktop.supports_native_explain() {
            self.query_action_notice("Explain is not available in this host");
            return;
        }
        if self.connect_loading.is_some() || self.native_query.is_some() || self.query_loading.is_some() {
            return;
        }
        let Some(tab) = self.tabs.selected().filter(|tab| tab.kind == TabKind::Query).map(|tab| tab.id) else {
            return;
        };
        if !self.bind_query_editor_for_execution() {
            return;
        }
        let sql = match self.sql.query_execution_text(|sql| self.desktop.editor_statement_ranges(sql)) {
            Ok(Some(sql)) => sql,
            Ok(None) => return,
            Err(error) => {
                self.query_action_notice(error);
                return;
            }
        };
        let target = self.query_database_target();
        let generation = self.next_query_generation();
        self.query_database_picker_tab = None;
        if self.desktop.begin_native_explain(generation, sql.clone(), target) {
            self.query_notice = None;
            self.query_loading = Some(generation);
            self.native_query = Some(crate::native_query_close::NativeQuery::new(generation, tab, sql));
            self.query_plan = Some(crate::state_def::NativeExplainState {
                tab_id: tab,
                generation,
                outcome: None,
            });
        } else {
            self.query_action_notice("The query plan could not start.");
        }
    }

    pub fn query_transaction(&mut self, action: &str) {
        self.query_action_notice(format!("{action} is not available in this host"));
    }

    pub fn insert_snippet_sql(&mut self, sql: &str) {
        if !sql.is_empty() {
            self.sql.insert(sql);
            self.result_focused = false;
        }
    }

    /// Load a query result into the selected result tab's grid (and legacy mirror).
    pub fn load_result_page(&mut self, columns: Vec<String>, rows: Vec<Vec<String>>) {
        let page = self.workspace.open_result(columns, rows);
        let mut grid = DataGrid::new();
        grid.load_page(&page);
        grid.set_editable(false);
        self.result_grid = Some(grid);
        self.result_focused = false;
        // A5: a fresh result starts in table view with no active find.
        self.result_view = crate::types::ResultView::Table;
        self.result_find = rune::TextEdit::new(String::new());
    }

    /// Bump and return the next query generation. Every run and every cancel
    /// starts one.
    pub fn next_query_generation(&mut self) -> u32 {
        self.query_generation += 1;
        self.query_generation
    }

    /// A result page arrives for a generation. Stale generations — superseded by
    /// a newer run or invalidated by a cancel — are dropped on the floor.
    /// A3: lands in the matching result tab and makes it selected.
    pub fn deliver_query_result(&mut self, generation: u32, page: TablePage) {
        self.deliver_query_result_with_stats(generation, page, None);
    }

    fn deliver_query_result_with_stats(
        &mut self,
        generation: u32,
        page: TablePage,
        execution_stats: Option<datazen_ui_api::QueryExecutionStats>,
    ) {
        if generation != self.query_generation {
            return;
        }
        self.park_selected_query_result_grid();
        let tab_id = self.ensure_query_result_tab_for_generation(generation);
        self.selected_query_result = Some(tab_id);
        self.restore_selected_query_result_grid();
        if let Some(tab) = self.query_result_tabs.iter_mut().find(|tab| tab.id == tab_id) {
            tab.execution_stats = execution_stats;
        }
        if execution_stats.is_some_and(|stats| stats.affected_rows.is_some()) {
            self.result_grid = None;
            self.result_focused = false;
        } else {
            self.load_result_page(page.columns, page.rows);
        }
        self.park_selected_query_result_grid();
        self.restore_selected_query_result_grid();
    }

    fn deliver_query_failure(&mut self, generation: u32, error: &str) {
        if generation != self.query_generation {
            return;
        }
        // Surface a stable classified reason. Other driver/transport detail
        // is deliberately not copied into the UI or system clipboard.
        let key = match error {
            "the statement failed: Cancelled" => return,
            "the statement failed: SqlError" => "query.nativeStatementRejected",
            _ => "query.nativeStatementFailed",
        };
        if let Some(tab) = self.query_result_tabs.iter_mut()
            .find(|tab| tab.generation == Some(generation))
        {
            tab.error_key = Some(key);
            tab.error_copied_until = None;
        }
    }

    pub(crate) fn mark_query_error_copied(&mut self, id: u32, now: f64) {
        if let Some(tab) = self.query_result_tabs.iter_mut().find(|tab| tab.id == id) {
            tab.error_copied_until = Some(now + 1.5);
        }
    }

    pub(crate) fn query_copy_feedback_tick(&mut self, now: f64) -> bool {
        let mut dirty = false;
        for tab in &mut self.query_result_tabs {
            if tab.error_copied_until.is_some_and(|until| now >= until) {
                tab.error_copied_until = None;
                dirty = true;
            }
        }
        dirty
    }

    /// Cancel the in-flight query: bumping the generation invalidates every
    /// outstanding one, so their results are dropped when they arrive. A
    /// query job in flight also stops waiting: its marker leaves with the
    /// generation, and a late delivery finds only the stale check.
    pub fn cancel_query(&mut self) {
        if self.desktop.supports_native_query_stop() {
            if let Some(generation) = self.native_query.as_ref().map(|query| query.generation) {
                self.request_running_query_stop(generation, false);
            }
            return;
        }
        self.next_query_generation();
        self.query_loading = None;
    }

    pub(crate) fn query_database_selection(&self) -> Option<(u32, datazen_ui_api::QueryDatabaseContext)> {
        if !self.query_editor_matches_connection() {
            return None;
        }
        let tab = self.tabs.selected().filter(|tab| tab.kind == TabKind::Query)?;
        let mut context = self.desktop.native_query_database_context()?;
        if self.desktop_connection.as_deref() != Some(context.current.connection_id.as_str())
            || self.connect_loading.is_some()
        {
            return None;
        }
        if let Some(target) = self.query_database_targets.get(&tab.id) {
            context.current = target.clone();
        }
        Some((tab.id, context))
    }

    pub(crate) fn native_from_completion_scope(&self) -> Option<crate::sql_editor::CompletionScope> {
        if self.active != Section::Workspace || self.connect_loading.is_some() || self.desktop_catalog.is_none()
            || !self.query_editor_matches_connection() {
            return None;
        }
        let tab = self.tabs.selected().filter(|tab| tab.kind == TabKind::Query)?;
        let default = self.desktop.native_from_catalog_target()?;
        if self.desktop_connection.as_deref() != Some(default.connection_id.as_str()) {
            return None;
        }
        let target = if let Some(target) = self.query_database_targets.get(&tab.id) {
            let context = self.desktop.native_query_database_context()?;
            if target.connection_id != default.connection_id
                || context.current.connection_id != default.connection_id
                || !context.choices.contains(&target.database)
            {
                return None;
            }
            target.clone()
        } else {
            default
        };
        Some((tab.id, target))
    }

    /// A stale scope may not render, accept a key, or consume a new completion.
    pub(crate) fn catalog_completion_current(&self) -> bool {
        self.sql.completion_scope().is_none_or(|old|
            self.native_from_completion_scope().as_ref() == Some(old))
    }

    pub(crate) fn refresh_native_from_completion(&mut self) {
        if let Some(scope) = self.native_from_completion_scope() {
            if let Some(catalog) = self.desktop_catalog.as_ref() {
                self.sql.refresh_from_catalog(scope, catalog);
            }
        }
    }

    fn query_database_target(&self) -> Option<datazen_ui_api::NativeQueryDatabase> {
        let tab = self.tabs.selected().filter(|tab| tab.kind == TabKind::Query)?;
        // Preserve a previous explicit choice until session replacement clears
        // it. If it is stale the worker must reject, not choose a fallback.
        self.query_database_targets.get(&tab.id).cloned()
            .or_else(|| self.query_database_selection().map(|(_, context)| context.current))
    }

    pub(crate) fn choose_query_database(&mut self, expected_tab: u32, database: &str) {
        self.query_database_picker_tab = None;
        if self.native_query.is_some() || self.query_loading.is_some() {
            return;
        }
        let Some((tab, mut context)) = self.query_database_selection() else { return };
        if tab == expected_tab && context.choices.iter().any(|choice| choice == database) {
            context.current.database = database.to_owned();
            self.query_database_targets.insert(tab, context.current);
            self.sql.clear_catalog_completion();
        }
    }

    /// Run the entire document explicitly. Plain Run and its keyboard
    /// shortcut still use only the selected/caret statement.
    pub fn run_query_script(&mut self) {
        if self.connect_loading.is_some() || !self.desktop.supports_native_query_script() {
            return;
        }
        if self.native_query.is_some() || self.query_loading.is_some() {
            self.query_action_notice("A query is still running.");
            return;
        }
        let Some(tab_id) = self.tabs.selected().filter(|tab| tab.kind == TabKind::Query).map(|tab| tab.id) else {
            return;
        };
        if !self.bind_query_editor_for_execution() {
            return;
        }
        let sql = self.sql.doc().to_owned();
        let generation = self.next_query_generation();
        self.query_database_picker_tab = None;
        let started = if let Some(target) = self.query_database_target() {
            self.desktop.begin_native_query_at(generation, sql.clone(), target, true)
        } else {
            self.desktop.begin_native_query_script(generation, sql.clone())
        };
        if started {
            self.query_notice = None;
            self.query_loading = Some(generation);
            self.native_query = Some(crate::native_query_close::NativeQuery::new(generation, tab_id, sql));
        } else {
            self.query_action_notice("The SELECT script could not start.");
        }
    }

    fn deliver_query_script(&mut self, generation: u32, pages: Result<Vec<TablePage>, String>) {
        self.finish_running_query(generation, false);
        if self.query_loading == Some(generation) {
            self.query_loading = None;
        }
        if generation != self.query_generation {
            return;
        }
        let pages = match pages {
            Ok(pages) => pages,
            Err(error) => {
                self.query_action_notice(error);
                return;
            }
        };
        let mut pages = pages.into_iter();
        let Some(first) = pages.next() else { return };
        self.deliver_query_result(generation, first);
        let first_id = self.selected_query_result;
        for page in pages {
            self.park_selected_query_result_grid();
            self.selected_query_result = Some(self.add_query_result_tab(None));
            self.load_result_page(page.columns, page.rows);
        }
        if let Some(first_id) = first_id {
            self.select_query_result_tab(first_id);
        }
    }

    /// Run the statement at the cursor. A failed execution changes nothing; a
    /// successful one lands in the result grid only if still current.
    /// A3: creates a new result tab for this run and selects it (pinned tabs
    /// remain; unpinned prior tabs are kept for switching). A synchronous
    /// failure restores the previously selected result so the UI stays as it was.
    pub fn run_query(&mut self) {
        if self.connect_loading.is_some() || !self.bind_query_editor_for_execution() {
            return;
        }
        let native = self.desktop.supports_native_query_stop();
        if native && self.native_query.is_some() {
            self.query_action_notice("A query is still running.");
            return;
        }
        let sql = match self
            .sql
            .query_execution_text(|sql| self.desktop.editor_statement_ranges(sql))
        {
            Ok(Some(sql)) => sql,
            Ok(None) => return,
            Err(error) => {
                self.query_action_notice(error);
                return;
            }
        };
        let previous_selected = self.selected_query_result;
        let generation = self.next_query_generation();
        self.query_notice = None;
        self.park_selected_query_result_grid();
        let tab_id = self.ensure_query_result_tab_for_generation(generation);
        self.selected_query_result = Some(tab_id);
        self.restore_selected_query_result_grid();
        self.result_focused = false;
        // N1: a live host runs the statement as a job; the editor keeps its
        // document and the result area says the query is in flight until the
        // delivery lands (dropped when its generation went stale, as in Q5).
        self.query_database_picker_tab = None;
        let target = self.query_database_target();
        let started = if let Some(target) = target.as_ref() {
            self.desktop.begin_native_query_at(generation, sql.clone(), target.clone(), false)
        } else {
            self.desktop.begin_job(AdapterJob::RunQuery { generation, sql: sql.clone() })
        };
        if started {
            self.query_loading = Some(generation);
            if native {
                if let Some(tab) = self.tabs.selected() {
                    self.native_query = Some(crate::native_query_close::NativeQuery::new(
                        generation, tab.id, sql,
                    ));
                }
            }
            return;
        }
        if target.is_some() {
            // Never execute on the fallback connection if the explicit target
            // became stale between selection and dispatch.
            let _ = self.close_query_result_tab(tab_id);
            self.query_action_notice("The selected query database is no longer available.");
            return;
        }
        match self.workspace.execute(&sql) {
            Ok(page) => self.deliver_query_result(generation, page),
            Err(_) => {
                // Synchronous failure: drop the empty pending tab and restore the
                // previous result view.
                self.query_loading = None;
                let _ = self.close_query_result_tab(tab_id);
                if self.selected_query_result != previous_selected {
                    if let Some(prev) = previous_selected {
                        let _ = self.select_query_result_tab(prev);
                    }
                }
            }
        }
    }

    /// Bump and return the next table read generation. Every read request —
    /// a fresh open, a page turn, a reload — starts one, so a delivery from
    /// a read the state has moved past is dropped (N1, plan invariant 3).
    pub fn next_read_generation(&mut self) -> u32 {
        self.read_generation += 1;
        self.read_generation
    }

    /// Bump and return the next structure generation (E1). Every structure
    /// request — a table open, a view switch, an ER open — starts one, so a
    /// delivery from a read the table's entry has moved past is dropped.
    pub fn next_structure_generation(&mut self) -> u32 {
        self.structure_generation += 1;
        self.structure_generation
    }

    /// E1: ask for one table's structure as a job — the four reads (DDL,
    /// columns, indexes, foreign keys) travel together and land as one
    /// delivery. A host that declines the job keeps the synchronous reads:
    /// the decline promises the synchronous methods serve, and the demo
    /// process's memory port is memory-speed — the same cache fills on the
    /// spot and the render path stays adapter-free either way.
    pub fn begin_structure_read(&mut self, table: &str) {
        let generation = self.next_structure_generation();
        let job = AdapterJob::ReadStructure {
            generation,
            table: table.to_owned(),
        };
        if self.desktop.begin_job(job) {
            self.structures.insert(
                table.to_owned(),
                StructureState {
                    generation,
                    loading: true,
                    error: None,
                    data: None,
                },
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
            StructureState {
                generation,
                loading: false,
                error: None,
                data: Some(data),
            },
        );
    }

    /// Bump and return the next widget-bars generation (E2). Every bars
    /// request — a dashboards open, a widget save, a manual refresh —
    /// starts one, so a delivery from a read the widget's entry has moved
    /// past is dropped.
    pub fn next_widget_bars_generation(&mut self) -> u32 {
        self.widget_bars_generation += 1;
        self.widget_bars_generation
    }

    /// E2: ask for one widget's bars as a job — the widget's query runs
    /// over the live session on the worker's runtime. A host that declines
    /// the job keeps the synchronous bars read (the demo process's memory
    /// port) and the same cache fills on the spot.
    pub fn begin_widget_bars_read(&mut self, id: u64) {
        let generation = self.next_widget_bars_generation();
        let job = AdapterJob::WidgetBars { generation, id };
        if self.desktop.begin_job(job) {
            self.widget_bars.insert(
                id,
                WidgetBarsState {
                    generation,
                    loading: true,
                    error: None,
                    bars: None,
                },
            );
            return;
        }
        let bars = self.host_widget_bars(id);
        self.widget_bars.insert(
            id,
            WidgetBarsState {
                generation,
                loading: false,
                error: None,
                bars: Some(bars),
            },
        );
    }

    /// E2: ask for every widget's bars — the page's open, a save and the
    /// manual refresh button all restart the whole chart set.
    pub fn refresh_widget_bars(&mut self) {
        let ids: Vec<u64> = self.host_widgets().iter().map(|widget| widget.id).collect();
        for id in ids {
            self.begin_widget_bars_read(id);
        }
    }

    /// A5: switch the query result presentation (table/chart).
    pub fn set_result_view(&mut self, view: crate::types::ResultView) {
        self.result_view = view;
    }

    /// A5: apply the current find-bar text to the result grid.
    pub fn apply_result_find(&mut self) {
        let needle = self.result_find.text.clone();
        if let Some(grid) = self.result_grid.as_mut() {
            grid.set_find_needle(&needle);
        }
    }

    /// A5: move to the next find hit in the result grid.
    pub fn result_find_next(&mut self) {
        if let Some(grid) = self.result_grid.as_mut() {
            grid.find_next();
        }
    }

    /// A5: move to the previous find hit in the result grid.
    pub fn result_find_prev(&mut self) {
        if let Some(grid) = self.result_grid.as_mut() {
            grid.find_prev();
        }
    }

    /// A host job's answer arrived on the poll path. The loading marker of
    /// the finished request always clears; the state itself is written only
    /// when the delivery's generation is still the current one.
    pub fn deliver_host_job(&mut self, delivery: AdapterDelivery) {
        match delivery {
            AdapterDelivery::NativeTransferRead {
                epoch,
                side,
                result,
            } => {
                self.native_transfer.read_completed(epoch, side, result);
            }
            AdapterDelivery::NativeTransferReviewed { epoch, result } => {
                self.native_transfer.review_completed(epoch, result);
            }
            AdapterDelivery::NativeTransferFinished { epoch, result } => {
                self.native_transfer.run_completed(epoch, result);
            }
            AdapterDelivery::NativeCsvReviewed { epoch, result } => {
                self.csv_reviewed(epoch, result);
            }
            AdapterDelivery::NativeCsvAppended { epoch, result } => {
                self.csv_appended(epoch, result);
            }
            AdapterDelivery::NativeSyncPreviewed { epoch, result } => {
                self.native_transfer.sync_completed(epoch, result);
            }
            AdapterDelivery::NativeSchemaCompared { epoch, result } => {
                self.native_transfer.schema_completed(epoch, result);
            }
            AdapterDelivery::ConnectionProbed { request_id, result } => {
                if self.connection_dialog_open && self.connection_probe_pending
                    && request_id == self.connection_probe_id
                {
                    self.connection_probe_pending = false;
                    self.connection_probe_result = Some(result);
                }
            }
            AdapterDelivery::RedisProbed { epoch, result } => {
                if self.connection_dialog_open && epoch == self.connection_dialog_epoch {
                    self.redis_probe_pending = false;
                    self.redis_probe_result = Some(result);
                }
            }
            AdapterDelivery::KvRead { request, result } => self.receive_kv(request, result),
            AdapterDelivery::ConnectionOpened {
                generation,
                id,
                catalog,
            } => {
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
                        let kv = self
                            .visible_connections()
                            .into_iter()
                            .find(|profile| profile.id == id)
                            .and_then(|profile| {
                                datazen_ui_api::native_kv::native_kv_driver(&profile.engine)
                            });
                        self.native_kv = kv.map(|kind| {
                            crate::views::native_kv::KvWorkbench::new(kind, id.clone(), generation)
                        });
                        self.query_database_picker_tab = None;
                        self.query_plan = None;
                        self.cancel_query_editor_input();
                        self.desktop_catalog = Some(catalog.clone());
                        self.desktop_connection = Some(id);
                        self.grid = DataGrid::new();
                        self.grid.set_editable(false);
                        self.table_read_error = None;
                        self.query_history.reset_context();
                        self.refresh_query_history_if_open();
                        self.active = Section::Workspace;
                        if self.native_kv.is_some() {
                            self.tabs = Default::default();
                            self.reset_query_editor_tabs();
                            self.query_database_targets.clear();
                            self.desktop_error = None;
                            self.kv_next_page();
                            return;
                        }
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
                match page {
                    Ok(page) => {
                        self.table_read_error = None;
                        self.table_page = page.page;
                        self.grid.load_page(&page);
                        self.grid.set_editable(page.read_only.is_none());
                        self.apply_table_column_widths();
                    }
                    Err(error) => {
                        self.grid.set_editable(false);
                        self.table_read_error = Some(error);
                    }
                }
            }
            AdapterDelivery::StructureRead {
                generation,
                table,
                structure,
            } => {
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
            AdapterDelivery::WidgetBarsRead {
                generation,
                id,
                bars,
            } => {
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
            AdapterDelivery::InboundRefsRead {
                generation,
                table,
                refs,
            } => {
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
            AdapterDelivery::CellPreviewRead { nonce, result } => {
                self.deliver_cell_preview(nonce, result);
            }
            AdapterDelivery::CommitCount { generation, count } => {
                self.finish_reviewed_commit(generation, &count);
                if self.commit_loading == Some(generation) {
                    self.commit_loading = None;
                }
                if generation != self.read_generation || count.is_err() {
                    return;
                }
                self.reload_table_page();
            }
            AdapterDelivery::QueryHistoryRead { generation, result } => {
                self.query_history.deliver(generation, result);
            }
            AdapterDelivery::QueryTerminal { generation } => {
                self.finish_running_query(generation, true);
            }
            AdapterDelivery::QueryStopRejected { generation } => {
                self.note_query_stop_rejected(generation);
            }
            AdapterDelivery::QueryExplained { generation, result } => {
                self.finish_running_query(generation, false);
                if self.query_loading == Some(generation) {
                    self.query_loading = None;
                }
                if let Some(plan) = self.query_plan.as_mut().filter(|plan| {
                    plan.generation == generation && self.query_generation == generation
                }) {
                    plan.outcome = Some(result.map_err(|_| "The query plan could not be prepared.".into()));
                }
            }
            AdapterDelivery::QueryScriptPages { generation, pages } => {
                self.deliver_query_script(generation, pages);
                self.refresh_query_history_if_open();
            }
            AdapterDelivery::NativeQueryCsvSaved { result_id, result } => {
                if let Some(tab) = self.query_result_tabs.iter_mut().find(|tab| tab.id == result_id) {
                    use crate::state_def::NativeQueryCsvStatus;
                    if matches!(tab.csv_status, Some(NativeQueryCsvStatus::Saving)) {
                        tab.csv_status = Some(match result {
                            Ok(()) => NativeQueryCsvStatus::Saved,
                            Err(error) => NativeQueryCsvStatus::Failed(error),
                        });
                    }
                }
            }
            AdapterDelivery::QueryPage { generation, page, execution_stats, csv_source } => {
                self.finish_running_query(generation, false);
                if self.query_loading == Some(generation) {
                    self.query_loading = None;
                }
                match page {
                    Ok(page) => {
                        self.deliver_query_result_with_stats(generation, page, execution_stats);
                        if self.query_generation == generation {
                            if let Some(tab) = self.query_result_tabs.iter_mut()
                                .find(|tab| tab.generation == Some(generation))
                            {
                                tab.csv_source = csv_source;
                            }
                        }
                    }
                    Err(error) => self.deliver_query_failure(generation, &error),
                }
                self.refresh_query_history_if_open();
            }
            AdapterDelivery::NativeWorkflowRun { generation, results } => {
                if self.workflow_loading == Some(generation) {
                    self.workflow_loading = None;
                }
                if generation != self.workflow_generation {
                    return;
                }
                match results {
                    Ok(result) => self.set_native_workflow_report(result),
                    Err(error) => {
                        self.clear_workflow_results();
                        self.workflow_operation_error = Some(error);
                    }
                }
            }
            AdapterDelivery::WorkflowRun {
                generation,
                results,
            } => {
                if self.workflow_loading == Some(generation) {
                    self.workflow_loading = None;
                }
                if generation != self.workflow_generation {
                    return;
                }
                // T019: the lite report and the execution-result panel's
                // projection of it land together.
                match results {
                    Ok(results) => self.set_workflow_results(results),
                    Err(_) => self.clear_workflow_results(),
                }
            }
        }
    }

    /// Any tab event lands in the workspace: the selected tab decides what the
    /// workspace shows, so the tab strip and its content stay on one page.
    pub fn sync_grid_to_selected_tab(&mut self) {
        self.sync_query_editor_to_selected_tab();
        self.sql.clear_catalog_completion();
        self.active = Section::Workspace;
        self.close_er();
        self.table_page = 0;
        self.filter = None;
        self.filter_column = 0;
        self.filter_editor = TextEdit::new(String::new());
        let selected = self
            .tabs
            .selected()
            .map(|tab| (tab.kind, tab.title.clone()));
        match selected {
            Some((TabKind::Table, title)) => {
                self.show_table(&title);
            }
            Some((TabKind::Query, _)) | None => {}
        }
    }
    pub fn close_tab(&mut self, id: u32) -> bool {
        if self.maybe_confirm_running_tab_close(id) {
            return false;
        }
        self.close_tab_now(id)
    }

    pub(crate) fn close_tab_now(&mut self, id: u32) -> bool {
        let previous = self.tabs.selected().map(|tab| tab.id);
        if !self.tabs.close(id) {
            return false;
        }
        self.discard_query_editor_tab(id);
        self.query_database_targets.remove(&id);
        if self.query_plan.as_ref().is_some_and(|plan| plan.tab_id == id) {
            self.query_plan = None;
        }
        if self.query_database_picker_tab == Some(id) {
            self.query_database_picker_tab = None;
        }
        if self.tabs.selected().map(|tab| tab.id) != previous {
            self.sync_grid_to_selected_tab();
        }
        true
    }
    pub fn commit_grid_edit(&mut self) {
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
