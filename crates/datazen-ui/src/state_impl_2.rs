//! AppState method implementations.

use crate::state_def::*;
use crate::views::{next_connection, workflow_yaml};
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
    pub(crate) fn export_answered(
        &mut self,
        table: String,
        format: String,
        range: String,
        result: FileDialogResult,
    ) {
        let (path, notice) = match result {
            Ok(Some(path)) => (Some(path.display().to_string()), None),
            Ok(None) => {
                self.export_job = None;
                self.export_error = Some("the export was cancelled; nothing was written".into());
                return;
            }
            Err(error) => (
                None,
                Some(format!(
                    "the path picker is unavailable ({error}); exporting to the default directory"
                )),
            ),
        };
        let started = self.desktop.start_export(&table, &format, &range, path.as_deref());
        let job = match started {
            Ok(job) => Some(job),
            Err(error) if error == DESKTOP_NOT_IMPLEMENTED => {
                self.workspace.start_export(&format, &range).ok()
            }
            Err(error) => {
                self.export_job = None;
                self.export_error = Some(error);
                return;
            }
        };
        match job {
            Some(job) => {
                self.export_job = Some(job);
                self.export_error = notice;
                self.export_snapshot = self.job_progress(job);
            }
            None => {
                self.export_job = None;
                self.export_error = Some("the export was not accepted".into());
            }
        }
    }

    /// One job's progress: the host's if it owns the job, the port's
    /// otherwise (N11).
    pub(crate) fn job_progress(&mut self, job: u64) -> Option<ProgressSnapshot> {
        if let Some(snapshot) = self.desktop.host_job_progress(job) {
            return Some(snapshot);
        }
        self.workspace.job_progress(job)
    }

    /// Pull one progress snapshot. No streaming, no polling.
    pub(crate) fn refresh_export_progress(&mut self) {
        if let Some(job) = self.export_job {
            self.export_snapshot = self.job_progress(job);
        }
    }

    /// The import wizard's step 1: a source is required to move on.
    pub(crate) fn import_next(&mut self) {
        if self.import_source.text.trim().is_empty() {
            self.import_error = Some("a source is required".into());
            return;
        }
        self.import_error = None;
        self.import_step = 1;
    }

    /// Step 2: a target table is required to finish.
    pub(crate) fn import_finish(&mut self) {
        if self.import_target.text.trim().is_empty() {
            self.import_error = Some("a target table is required".into());
            return;
        }
        self.import_error = None;
        self.import_step = 2;
    }

    pub(crate) fn import_back(&mut self) {
        self.import_step = 0;
        self.import_error = None;
    }

    /// T2: the backup's save path is picked first; the answer in
    /// `backup_answered` starts the job (or cancels, or falls back to the
    /// default directory when no dialog could show).
    pub(crate) fn begin_backup_form(&mut self, tx: &mut Tx) {
        let options = FileDialogOptions {
            title: "Backup the live database".into(),
            file_name: Some("datazen-backup.sql".into()),
            ..Default::default()
        }
        .filter("SQL dump", &["sql"]);
        tx.save_file(options, |state: &mut AppState, result, _| {
            state.backup_answered(result);
        });
    }

    /// The save dialog's answer for a backup (T2).
    pub(crate) fn backup_answered(&mut self, result: FileDialogResult) {
        let (path, notice) = match result {
            Ok(Some(path)) => (Some(path.display().to_string()), None),
            Ok(None) => {
                self.backup_error = Some("the backup was cancelled; nothing was written".into());
                return;
            }
            Err(error) => (
                None,
                Some(format!(
                    "the path picker is unavailable ({error}); backing up to the default directory"
                )),
            ),
        };
        let started = self.desktop.start_backup(path.as_deref());
        let job = match started {
            Ok(job) => Some(job),
            Err(error) if error == DESKTOP_NOT_IMPLEMENTED => self.workspace.start_backup().ok(),
            Err(error) => {
                self.backup_error = Some(error);
                return;
            }
        };
        match job {
            Some(job) => {
                self.backup_job = Some(job);
                self.backup_error = notice;
                self.backup_snapshot = self.job_progress(job);
            }
            None => self.backup_error = Some("the backup was not accepted".into()),
        }
    }

    /// Stop the backup and pull the stopped snapshot. The host's job first
    /// (N11); the memory port's stop stays the fallback's.
    pub(crate) fn stop_backup(&mut self) {
        let Some(job) = self.backup_job else { return };
        let stopped = match self.desktop.stop_host_job(job) {
            Ok(()) => Ok(()),
            Err(error) if error == DESKTOP_NOT_IMPLEMENTED => self.workspace.stop_job(job),
            Err(error) => Err(error),
        };
        match stopped {
            Ok(()) => {
                self.backup_error = None;
                self.backup_snapshot = self.job_progress(job);
            }
            Err(error) => self.backup_error = Some(error),
        }
    }

    pub(crate) fn refresh_backup_progress(&mut self) {
        if let Some(job) = self.backup_job {
            self.backup_snapshot = self.job_progress(job);
        }
    }

    pub(crate) fn set_tool(&mut self, tool: Tool) {
        self.tool = tool;
    }

    pub(crate) fn cycle_compare_source(&mut self) {
        self.compare_source = next_connection(&self.workspace, &self.compare_source);
    }

    pub(crate) fn cycle_compare_target(&mut self) {
        self.compare_target = next_connection(&self.workspace, &self.compare_target);
    }

    /// D8: apply the plan through the port. A failure keeps the plan (the
    /// difference is still there), and so does the error.
    pub(crate) fn apply_compare_plan(&mut self) {
        let plan = match self.workspace.compare_plan(&self.compare_source, &self.compare_target) {
            Ok(plan) => plan,
            Err(error) => {
                self.compare_error = Some(error);
                return;
            }
        };
        match self.workspace.run_compare_plan(&plan) {
            Ok(job) => {
                self.compare_job = Some(job);
                self.compare_snapshot = self.workspace.job_progress(job);
                self.compare_error = None;
            }
            Err(error) => {
                self.compare_job = None;
                self.compare_error = Some(error);
            }
        }
    }

    pub(crate) fn refresh_compare_progress(&mut self) {
        if let Some(job) = self.compare_job {
            self.compare_snapshot = self.workspace.job_progress(job);
        }
    }

    /// The columns of a table (its ColumnSpec names).
    pub(crate) fn table_columns_of(&self, table: &str) -> Vec<String> {
        self.workspace
            .table_columns(table)
            .unwrap_or_default()
            .into_iter()
            .map(|column| column.name)
            .collect()
    }

    pub(crate) fn cycle_transfer_table(&mut self, side: bool) {
        let tables = self.workspace.connection_tables("local");
        let current = if side { &self.transfer_left } else { &self.transfer_right };
        let next = match tables.iter().position(|name| name == current) {
            Some(index) => tables[(index + 1) % tables.len()].clone(),
            None => tables.first().cloned().unwrap_or_default(),
        };
        if side {
            self.transfer_left = next;
        } else {
            self.transfer_right = next;
        }
        self.transfer_pair_left = self.table_columns_of(&self.transfer_left).first().cloned().unwrap_or_default();
        self.transfer_pair_right = self.table_columns_of(&self.transfer_right).first().cloned().unwrap_or_default();
    }

    pub(crate) fn cycle_transfer_pair_column(&mut self, side: bool) {
        let (table, current) = if side {
            (&self.transfer_left, self.transfer_pair_left.clone())
        } else {
            (&self.transfer_right, self.transfer_pair_right.clone())
        };
        let columns = self.table_columns_of(table);
        let next = match columns.iter().position(|name| *name == current) {
            Some(index) => columns[(index + 1) % columns.len()].clone(),
            None => columns.first().cloned().unwrap_or_default(),
        };
        if side {
            self.transfer_pair_left = next;
        } else {
            self.transfer_pair_right = next;
        }
    }

    /// Pair the two picked columns. A column that already appears in a pair —
    /// a duplicate name — cannot pair again.
    pub(crate) fn pair_transfer_columns(&mut self) {
        let left = self.transfer_pair_left.clone();
        let right = self.transfer_pair_right.clone();
        if left.is_empty() || right.is_empty() {
            self.transfer_error = Some("pick a column on both ends".into());
            return;
        }
        if self.transfer_pairs.iter().any(|(l, r)| *l == left || *r == right) {
            self.transfer_error =
                Some(format!("{left} or {right} is already paired — a column pairs once"));
            return;
        }
        self.transfer_pairs.push((left, right));
        self.transfer_error = None;
    }

    /// Open the confirmation row. Cancelling it leaves no job.
    pub(crate) fn begin_transfer_confirm(&mut self) {
        self.transfer_confirming = true;
    }

    pub(crate) fn cancel_transfer(&mut self) {
        self.transfer_confirming = false;
    }

    pub(crate) fn confirm_transfer(&mut self) {
        self.transfer_confirming = false;
        let pairs = self.transfer_pairs.clone();
        match self.workspace.run_transfer(&pairs) {
            Ok(job) => {
                self.transfer_job = Some(job);
                self.transfer_snapshot = self.workspace.job_progress(job);
                self.transfer_error = None;
            }
            Err(error) => {
                self.transfer_job = None;
                self.transfer_error = Some(error);
            }
        }
    }

    pub(crate) fn refresh_transfer_progress(&mut self) {
        if let Some(job) = self.transfer_job {
            self.transfer_snapshot = self.workspace.job_progress(job);
        }
    }

    pub(crate) fn edit_workflow(&mut self, name: &str) {
        if let Some(flow) = self.host_workflows().into_iter().find(|flow| flow.name == name) {
            self.workflow_yaml = SqlEditor::from_doc(workflow_yaml(&flow.name, &flow.steps));
            self.workflow_editing = Some((flow.name, flow.steps));
        }
    }

    /// Move a step up or down. The first step cannot move up, the last cannot
    /// move down; everything else swaps with its neighbour.
    pub(crate) fn move_workflow_step(&mut self, index: usize, up: bool) {
        let Some((_, steps)) = &mut self.workflow_editing else { return };
        let target = if up { index.checked_sub(1) } else { Some(index + 1) };
        match target {
            Some(target) if target < steps.len() => steps.swap(index, target),
            _ => {}
        }
    }

    /// Save the edited workflow: the host's registry use case first (N13);
    /// its fixed not-implemented error means the demo process, and the
    /// memory port answers instead.
    pub(crate) fn save_workflow_form(&mut self) {
        let Some((name, steps)) = self.workflow_editing.clone() else { return };
        let saved = match self.desktop.save_workflow(&name, &steps) {
            Ok(()) => Ok(()),
            Err(error) if error == DESKTOP_NOT_IMPLEMENTED => {
                self.workspace.save_workflow(&name, &steps)
            }
            Err(error) => Err(error),
        };
        if saved.is_ok() {
            self.workflow_editing = None;
        }
    }

    pub(crate) fn close_workflow_editor(&mut self) {
        self.workflow_editing = None;
        self.workflow_results.clear();
    }

    /// W7: run the edited workflow; the results list below is read-only.
    /// T8: the host's engine runs as a job (every step kind it supports); a
    /// declined job falls back to the sync paths — the host's N13 runner,
    /// then the memory port's demo runner.
    pub(crate) fn run_edited_workflow(&mut self) {
        let Some((name, _)) = self.workflow_editing.clone() else { return };
        let generation = self.next_workflow_generation();
        let job = AdapterJob::RunWorkflow { generation, name: name.clone() };
        if self.desktop.begin_job(job) {
            self.workflow_loading = Some(generation);
            return;
        }
        let ran = match self.desktop.run_workflow(&name) {
            Ok(results) => Ok(results),
            Err(error) if error == DESKTOP_NOT_IMPLEMENTED => {
                self.workspace.run_workflow(&name)
            }
            Err(error) => Err(error),
        };
        match ran {
            Ok(results) => self.workflow_results = results,
            Err(_) => self.workflow_results.clear(),
        }
    }

    pub(crate) fn next_workflow_generation(&mut self) -> u32 {
        self.workflow_generation += 1;
        self.workflow_generation
    }

    /// Save the widget form. An empty title is refused. The host's
    /// dashboard use case runs first (N13); its fixed not-implemented error
    /// means the demo process, and the memory port answers instead; any
    /// other failure is real and the form shows it.
    pub(crate) fn save_widget_form(&mut self) {
        let title = self.widget_title.text.trim().to_owned();
        let query = self.widget_query.text.trim().to_owned();
        let saved = match self.desktop.save_dashboard_widget(&title, &query) {
            Ok(id) => Ok(id),
            Err(error) if error == DESKTOP_NOT_IMPLEMENTED => {
                self.workspace.save_dashboard_widget(&title, &query)
            }
            Err(error) => Err(error),
        };
        match saved {
            Ok(_) => {
                self.widget_error = None;
                self.widget_title = TextEdit::new(String::new());
                self.widget_query = TextEdit::new(String::new());
                // E2: the new widget's bars ask for themselves — every
                // widget's read restarts, the same refresh the page's
                // button gives.
                self.refresh_widget_bars();
            }
            Err(error) => self.widget_error = Some(error),
        }
    }

    /// The left table's columns not yet paired.
    pub(crate) fn transfer_unmapped(&self, side: bool) -> Vec<String> {
        let (table, used_left): (String, Vec<String>) = if side {
            (self.transfer_left.clone(), self.transfer_pairs.iter().map(|(l, _)| l.clone()).collect())
        } else {
            (self.transfer_right.clone(), self.transfer_pairs.iter().map(|(_, r)| r.clone()).collect())
        };
        self.table_columns_of(&table)
            .into_iter()
            .filter(|column| !used_left.contains(column))
            .collect()
    }

    /// Cycle the sync source through the connections.
    pub(crate) fn cycle_sync_source(&mut self) {
        self.sync_source = next_connection(&self.workspace, &self.sync_source);
    }

    pub(crate) fn cycle_sync_target(&mut self) {
        self.sync_target = next_connection(&self.workspace, &self.sync_target);
    }

    pub(crate) fn toggle_sync_table(&mut self, table: &str) {
        if !self.sync_tables.insert(table.to_owned()) {
            self.sync_tables.remove(table);
        }
    }

    pub(crate) fn sync_select_all(&mut self) {
        self.sync_tables = self
            .workspace
            .connection_tables(&self.sync_source)
            .into_iter()
            .collect();
    }

    pub(crate) fn sync_clear_tables(&mut self) {
        self.sync_tables.clear();
    }

    /// Step 2 done: at least one table must be picked.
    pub(crate) fn sync_next_tables(&mut self) {
        if self.sync_tables.is_empty() {
            self.sync_error = Some("pick at least one table".into());
            return;
        }
        self.sync_error = None;
        self.sync_step = 2;
    }

    /// The sync options' labels, in order.
    pub(crate) const SYNC_OPTION_LABELS: [&'static str; 3] =
        ["Create missing tables", "Drop extra tables", "Copy data"];

    pub(crate) fn toggle_sync_option(&mut self, index: usize) {
        if let Some(option) = self.sync_options.get_mut(index) {
            *option = !*option;
        }
    }

    /// D5: run the sync. No selected tables, no job.
    pub(crate) fn sync_run(&mut self) {
        let source = self.sync_source.clone();
        let target = self.sync_target.clone();
        let tables: Vec<String> = self.sync_tables.iter().cloned().collect();
        match self.workspace.run_sync(&source, &target, &tables) {
            Ok(job) => {
                self.sync_job = Some(job);
                self.sync_snapshot = self.workspace.job_progress(job);
                self.sync_error = None;
            }
            Err(error) => {
                self.sync_job = None;
                self.sync_error = Some(error);
            }
        }
    }

    pub(crate) fn refresh_sync_progress(&mut self) {
        if let Some(job) = self.sync_job {
            self.sync_snapshot = self.workspace.job_progress(job);
        }
    }

    /// Step 3 done: build the read-only preview of the first picked table.
    pub(crate) fn sync_show_preview(&mut self) {
        let Some(table) = self.sync_tables.iter().next().cloned() else {
            self.sync_error = Some("pick at least one table".into());
            return;
        };
        let source = self.sync_source.clone();
        match self.workspace.sync_preview(&source, &table) {
            Ok(page) => {
                let mut grid = DataGrid::new();
                grid.load_page(&page);
                grid.set_editable(false);
                self.sync_preview_grid = Some(grid);
                self.sync_error = None;
                self.sync_step = 3;
            }
            Err(error) => self.sync_error = Some(error),
        }
    }

    /// Step 1 done: the port validates the two endpoints.
    pub(crate) fn sync_next_endpoints(&mut self) {
        let source = self.sync_source.clone();
        let target = self.sync_target.clone();
        match self.workspace.sync_endpoints(&source, &target) {
            Ok(()) => {
                self.sync_error = None;
                self.sync_step = 1;
            }
            Err(error) => self.sync_error = Some(error),
        }
    }

    /// Flip the theme. The only write goes to the settings in force.
    pub(crate) fn toggle_theme(&mut self) {
        let dark = self
            .ui_settings()
            .iter()
            .find_map(|setting| match setting {
                Setting::Theme { dark } => Some(*dark),
                _ => None,
            })
            .unwrap_or(true);
        let _ = self.write_ui_setting(Setting::Theme { dark: !dark });
    }

    /// Open the ER diagram for the current schema: the selected table's schema,
    /// else the first.
    pub(crate) fn open_er(&mut self) {
        let catalog = self.catalog();
        let Some(database) = catalog.databases.first() else {
            return;
        };
        let selected = self.tabs.selected().map(|tab| tab.title.clone());
        let schema = selected
            .and_then(|table| {
                database
                    .schemas
                    .iter()
                    .find(|schema| schema.tables.contains(&table))
                    .map(|schema| schema.name.clone())
            })
            .or_else(|| database.schemas.first().map(|schema| schema.name.clone()));
        if let Some(schema) = schema {
            // E1: the ER boxes render the structure cache only; the open is
            // the one moment the reads are asked for — one job per table of
            // the schema, each landing in its own cache entry.
            let tables = database
                .schemas
                .iter()
                .find(|node| node.name == schema)
                .map(|node| node.tables.clone())
                .unwrap_or_default();
            self.er_schema = Some((database.name.clone(), schema));
            self.active = Section::Workspace;
            for table in tables {
                self.begin_structure_read(&table);
            }
        }
    }

    pub(crate) fn close_er(&mut self) {
        self.er_schema = None;
    }

    /// The selected table tab's title, if any.
    pub(crate) fn selected_table(&self) -> Option<String> {
        let tab = self.tabs.selected()?;
        (tab.kind == TabKind::Table).then(|| tab.title.clone())
    }

    /// Queue an add-column draft from the draft inputs.
    pub(crate) fn add_column_draft(&mut self) {
        let Some(table) = self.selected_table() else { return };
        let name = self.draft_name.text.trim().to_owned();
        if name.is_empty() {
            return;
        }
        let sql_type = if self.draft_type.text.trim().is_empty() {
            "text".to_owned()
        } else {
            self.draft_type.text.trim().to_owned()
        };
        self.column_drafts.entry(table).or_default().push(ColumnDraft::AddColumn { name, sql_type });
        self.draft_name = TextEdit::new(String::new());
        self.draft_type = TextEdit::new(String::new());
    }

    /// Apply the table's drafts through the port. A failure keeps the drafts.
    pub(crate) fn apply_column_drafts(&mut self) {
        let Some(table) = self.selected_table() else { return };
        let Some(drafts) = self.column_drafts.get(&table) else { return };
        if drafts.is_empty() {
            return;
        }
        let sql = drafts.iter().map(|draft| draft.sql(&table)).collect::<Vec<_>>().join("\n");
        if self.workspace.apply_ddl(&table, &sql).is_ok() {
            self.column_drafts.remove(&table);
            // E1: the applied DDL changed the table's structure — the cache
            // the views render is refetched through the same job path.
            self.begin_structure_read(&table);
        }
    }

    /// Queue a retype draft for the column named in the draft inputs.
    pub(crate) fn add_retype_draft(&mut self) {
        let Some(table) = self.selected_table() else { return };
        let column = self.draft_name.text.trim().to_owned();
        if column.is_empty() {
            return;
        }
        let sql_type = if self.draft_type.text.trim().is_empty() {
            "text".to_owned()
        } else {
            self.draft_type.text.trim().to_owned()
        };
        self.column_drafts
            .entry(table)
            .or_default()
            .push(ColumnDraft::ChangeType { column, sql_type });
        self.draft_name = TextEdit::new(String::new());
        self.draft_type = TextEdit::new(String::new());
    }

    pub(crate) fn open_connection(&mut self, id: &str) {
        // N3: a live host opens the connection as a job — real driver IO off
        // the frame loop. Until the catalog (or the failure sentence) lands
        // the page stays put and says the connect is in flight; the memory
        // port's instant path answers when the host declines.
        self.record_recent_connection(id);
        let generation = self.next_read_generation();
        if self
            .desktop
            .begin_job(AdapterJob::OpenConnection { generation, id: id.to_owned() })
        {
            self.connect_loading = Some(generation);
            return;
        }
        if self.workspace.connect(id).is_err() {
            return;
        }
        self.collapsed.clear();
        self.table_page = 0;
        self.sort = None;
        self.filter = None;
        self.filter_column = 0;
        self.filter_editor = TextEdit::new(String::new());
        // H4: the initial page goes through the same read path as every
        // other one: the adapter's use case first, the port as the fallback.
        self.show_table("customers");
    }

    /// Record a connection as recently opened in session UI state (A1). Not
    /// persisted and no contract change; keeps the most recent entries first.
    pub(crate) fn record_recent_connection(&mut self, id: &str) {
        self.recent_connection_ids.retain(|existing| existing != id);
        self.recent_connection_ids.insert(0, id.to_owned());
        if self.recent_connection_ids.len() > 8 {
            self.recent_connection_ids.truncate(8);
        }
    }

    /// Connections visible to the UI: the desktop adapter's list when wired,
    /// otherwise the memory port's demo list.
    pub(crate) fn visible_connections(&self) -> Vec<ConnectionProfile> {
        self.desktop_connections
            .clone()
            .unwrap_or_else(|| self.workspace.connections().to_vec())
    }

    pub(crate) fn disconnect(&mut self) {
        self.workspace.disconnect();
        self.tabs = Tabs::new();
        self.grid.set_editable(false);
        self.table_page = 0;
        self.sort = None;
        self.filter = None;
        self.filter_column = 0;
        self.filter_editor = TextEdit::new(String::new());
        self.result_grid = None;
        self.result_focused = false;
        self.collapsed.clear();
        self.er_schema = None;
        self.er_positions.clear();
        self.er_drag = None;
        // N1: a read the disconnect invalidates must not land later, and no
        // loading marker outlives the session it belonged to. N3: the live
        // catalog belongs to the session too.
        self.next_read_generation();
        self.table_loading = None;
        self.commit_loading = None;
        self.query_loading = None;
        self.connect_loading = None;
        self.desktop_catalog = None;
        self.desktop_connection = None;
        self.structures.clear();
        self.widget_bars.clear();
        self.inbound_refs = None;
        self.inbound_loading = None;
        self.fk_error = None;
    }

    pub(crate) fn open_table_tab(&mut self, title: &str) -> Option<u32> {
        let id = self.tabs.open(TabKind::Table, title)?;
        // E1: opening a table asks for its structure — the four views and
        // the outbound FK navigation render the cached answer only.
        self.begin_structure_read(title);
        self.sync_grid_to_selected_tab();
        Some(id)
    }

    pub(crate) fn open_query_tab(&mut self, title: &str) -> Option<u32> {
        let id = self.tabs.open(TabKind::Query, title)?;
        self.sync_grid_to_selected_tab();
        Some(id)
    }

    pub(crate) fn select_tab(&mut self, id: u32) -> bool {
        if !self.tabs.select(id) {
            return false;
        }
        self.sync_grid_to_selected_tab();
        true
    }

}
