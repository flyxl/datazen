//! AppState method implementations.

#![allow(unused_imports)]
use crate::state_def::*;
use crate::views::next_connection;
use crate::views::WorkflowExecutionResult;
use crate::views::workflow_yaml;
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
    pub(crate) fn host_workflows(&self) -> Vec<Workflow> {
        self.desktop
            .workflows()
            .unwrap_or_else(|_| self.workspace.workflows())
    }

    /// The dashboard's widgets in force (N13): the host's dashboard once it
    /// persists one, the memory port's while it does not.
    pub(crate) fn host_widgets(&self) -> Vec<DashboardWidget> {
        self.desktop
            .dashboard_widgets()
            .unwrap_or_else(|_| self.workspace.dashboard_widgets())
    }

    /// One widget's chart data (N13): the host's when its dashboard answers,
    /// the memory port's otherwise.
    pub(crate) fn host_widget_bars(&self, id: u64) -> Vec<(String, u32)> {
        self.desktop
            .widget_bars(id)
            .unwrap_or_else(|_| self.workspace.widget_bars(id).unwrap_or_default())
    }

    /// One structure read with the host first (N10): the live catalog's
    /// answer when the host has one, the memory port's otherwise. E1: the
    /// views no longer call these — the job fills the cache they render,
    /// and these serve the declined path (the demo process's memory port,
    /// memory-speed) that fills the same cache on the spot.
    pub(crate) fn structure_ddl(&self, table: &str) -> Result<String, String> {
        self.desktop
            .table_ddl(table)
            .or_else(|_| self.workspace.table_ddl(table))
    }
    pub(crate) fn structure_columns(&self, table: &str) -> Result<Vec<datazen_ui_api::ColumnSpec>, String> {
        self.desktop
            .table_columns(table)
            .or_else(|_| self.workspace.table_columns(table))
    }
    pub(crate) fn structure_indexes(&self, table: &str) -> Result<Vec<datazen_ui_api::IndexSpec>, String> {
        self.desktop
            .table_indexes(table)
            .or_else(|_| self.workspace.table_indexes(table))
    }
    pub(crate) fn structure_foreign_keys(
        &self,
        table: &str,
    ) -> Result<Vec<datazen_ui_api::ForeignKeySpec>, String> {
        self.desktop
            .table_foreign_keys(table)
            .or_else(|_| self.workspace.table_foreign_keys(table))
    }

    /// Write one setting: the host's use case first (N8); its fixed
    /// not-implemented error means the demo process, and the memory port
    /// answers instead; any other failure is real and the caller shows it.
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
        // T019: closing the editor unmounts the execution-result panel —
        // React's row `useState`s reset with it, so the interaction sets go.
        self.workflow_execution = None;
        self.workflow_step_expanded.clear();
        self.workflow_step_chart.clear();
    }

    /// T019: one run's answers land together — the lite step report the
    /// summary rows read, and the projection the execution-result panel
    /// renders from.
    pub(crate) fn set_workflow_results(&mut self, results: Vec<WorkflowStepResult>) {
        self.workflow_execution = Some(WorkflowExecutionResult::from_step_results(&results));
        self.workflow_results = results;
    }

    /// T019: a failed run reports nothing; both reads of it go together.
    pub(crate) fn clear_workflow_results(&mut self) {
        self.workflow_results.clear();
        self.workflow_execution = None;
    }

    /// T019: the execution-result panel's per-row expansion state.
    pub(crate) fn toggle_workflow_step_expanded(&mut self, step_id: &str) {
        if !self.workflow_step_expanded.remove(step_id) {
            self.workflow_step_expanded.insert(step_id.to_owned());
        }
    }

    /// T019: the execution-result panel's per-row table/chart view state.
    pub(crate) fn set_workflow_step_chart(&mut self, step_id: &str, chart: bool) {
        if chart {
            self.workflow_step_chart.insert(step_id.to_owned());
        } else {
            self.workflow_step_chart.remove(step_id);
        }
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
            Ok(results) => self.set_workflow_results(results),
            Err(_) => self.clear_workflow_results(),
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
}
