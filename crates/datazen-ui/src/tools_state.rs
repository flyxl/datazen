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
}
