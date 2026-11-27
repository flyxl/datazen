//! AppState method implementations.

#![allow(unused_imports)]
use crate::constants::*;
use crate::data_grid::DataGrid;
use crate::sql_editor::SqlEditor;
use crate::state_def::*;
use crate::types::{Section, TableView, Tool};
use crate::views::next_connection;
use crate::views::workflow_yaml;
use crate::views::WorkflowExecutionResult;
use crate::views::WorkflowFormSlots;
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
    pub fn host_workflows(&self) -> Vec<Workflow> {
        self.desktop
            .workflows()
            .unwrap_or_else(|_| self.workspace.workflows())
    }

    /// The dashboard's widgets in force (N13): the host's dashboard once it
    /// persists one, the memory port's while it does not.
    pub fn host_widgets(&self) -> Vec<DashboardWidget> {
        self.desktop
            .dashboard_widgets()
            .unwrap_or_else(|_| self.workspace.dashboard_widgets())
    }

    /// One widget's chart data (N13): the host's when its dashboard answers,
    /// the memory port's otherwise.
    pub fn host_widget_bars(&self, id: u64) -> Vec<(String, u32)> {
        self.desktop
            .widget_bars(id)
            .unwrap_or_else(|_| self.workspace.widget_bars(id).unwrap_or_default())
    }

    /// One structure read with the host first (N10): the live catalog's
    /// answer when the host has one, the memory port's otherwise. E1: the
    /// views no longer call these — the job fills the cache they render,
    /// and these serve the declined path (the demo process's memory port,
    /// memory-speed) that fills the same cache on the spot.
    pub fn structure_ddl(&self, table: &str) -> Result<String, String> {
        self.desktop
            .table_ddl(table)
            .or_else(|_| self.workspace.table_ddl(table))
    }
    pub fn structure_columns(
        &self,
        table: &str,
    ) -> Result<Vec<datazen_ui_api::ColumnSpec>, String> {
        self.desktop
            .table_columns(table)
            .or_else(|_| self.workspace.table_columns(table))
    }
    pub fn structure_indexes(&self, table: &str) -> Result<Vec<datazen_ui_api::IndexSpec>, String> {
        self.desktop
            .table_indexes(table)
            .or_else(|_| self.workspace.table_indexes(table))
    }
    pub fn structure_foreign_keys(
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
    pub fn open_er(&mut self) {
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
            self.er_selected = None;
            self.er_search.clear();
            self.er_zoom = 1.0;
            self.active = Section::Workspace;
            for table in tables {
                self.begin_structure_read(&table);
            }
        }
    }
    pub fn close_er(&mut self) {
        self.er_schema = None;
        self.er_selected = None;
        self.er_search.clear();
        self.er_zoom = 1.0;
    }

    /// The selected table tab's title, if any.
    pub fn selected_table(&self) -> Option<String> {
        let tab = self.tabs.selected()?;
        (tab.kind == TabKind::Table).then(|| tab.title.clone())
    }

    /// Queue an add-column draft from the draft inputs.
    pub fn add_column_draft(&mut self) {
        let Some(table) = self.selected_table() else {
            return;
        };
        let name = self.draft_name.text.trim().to_owned();
        if name.is_empty() {
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
            .push(ColumnDraft::AddColumn { name, sql_type });
        self.draft_name = TextEdit::new(String::new());
        self.draft_type = TextEdit::new(String::new());
    }

    /// Apply the table's drafts through the port. A failure keeps the drafts.
    pub fn apply_column_drafts(&mut self) {
        let Some(table) = self.selected_table() else {
            return;
        };
        let Some(drafts) = self.column_drafts.get(&table) else {
            return;
        };
        if drafts.is_empty() {
            return;
        }
        let sql = drafts
            .iter()
            .map(|draft| draft.sql(&table))
            .collect::<Vec<_>>()
            .join("\n");
        if self.workspace.apply_ddl(&table, &sql).is_ok() {
            self.column_drafts.remove(&table);
            // E1: the applied DDL changed the table's structure — the cache
            // the views render is refetched through the same job path.
            self.begin_structure_read(&table);
        }
    }

    /// Queue a retype draft for the column named in the draft inputs.
    pub fn add_retype_draft(&mut self) {
        let Some(table) = self.selected_table() else {
            return;
        };
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
    /// Save the workflow the visual form carries — the form's footer Save
    /// (React's `onSave` caller: `workflowDraftToDefinition` →
    /// `workflowSave`): the draft's name and its steps through the host's
    /// registry use case (N13), the memory port under it. A successful save
    /// closes the edit panel, writes the feedback line and reloads the list
    /// (the page's `handleSave`); a failure writes the feedback line
    /// (React's catch). The lite editor's own save (of `workflow_editing`)
    /// went with the lite page this port replaced.
    pub fn save_workflow_form(&mut self) {
        let name = self.workflow_draft.name_trimmed().to_owned();
        let steps = crate::views::workflow_page::workflow_draft_to_steps(&self.workflow_draft);
        match self.save_workflow_through_host(&name, &steps) {
            Ok(()) => {
                let active = self.workflow_active_panel.clone();
                if let Some(id) = active {
                    if matches!(
                        self.workflow_panels.iter().find(|panel| panel.id() == id),
                        Some(crate::views::workflow_page::WorkflowPanel::Edit(_))
                    ) {
                        self.close_workflow_panel(&id);
                    }
                }
                // React's 2s feedback reset is the timer seam — the line
                // stays until the next feedback write.
                self.workflow_feedback = crate::i18n::t("workflows.saved").to_owned();
                self.fetch_workflow_list();
            }
            Err(error) => {
                self.workflow_feedback = error;
            }
        }
    }

    /// The registry save both workflow save halves share (the lite editor's
    /// save and this port's edit-pane/form saves): the host's use case first
    /// (N13); its fixed not-implemented error means the demo process, and the
    /// memory port answers instead.
    pub fn save_workflow_through_host(
        &mut self,
        name: &str,
        steps: &[String],
    ) -> Result<(), String> {
        match self.desktop.save_workflow(name, steps) {
            Ok(()) => Ok(()),
            Err(error) if error == DESKTOP_NOT_IMPLEMENTED => {
                self.workspace.save_workflow(name, steps)
            }
            Err(error) => Err(error),
        }
    }

    /// The form's footer Cancel (React's `onCancel` → the caller's
    /// `closePanel`): the edit panel closes and the form's interaction
    /// state resets with the unmount; the run read the lite editor cleared
    /// goes with it — the compact variant's own tests pin that half. The
    /// page's own edit-pane Cancel is the shared action bar's
    /// (`close_edit_panel`, React's page-level `closePanel` exactly).
    pub fn close_workflow_editor(&mut self) {
        let active = self.workflow_active_panel.clone();
        if let Some(id) = active {
            self.close_workflow_panel(&id);
        }
        self.reset_workflow_form_caches();
        self.workflow_results.clear();
        self.workflow_execution = None;
        self.workflow_step_expanded.clear();
        self.workflow_step_chart.clear();
    }

    /// T019: one run's answers land together — the lite step report the
    /// summary rows read, and the projection the execution-result panel
    /// renders from. T145: the page's executing run panel takes the answer
    /// too (React's `setPanels` in `runWorkflowInPanel`) and re-seeds the
    /// step view at the order's first shown step.
    pub fn set_workflow_results(&mut self, results: Vec<WorkflowStepResult>) {
        let report = WorkflowExecutionResult::from_step_results(&results);
        self.set_workflow_report(results, report);
    }

    pub(crate) fn set_native_workflow_report(&mut self, result: datazen_ui_api::NativeWorkflowRun) {
        let report = WorkflowExecutionResult::from_native_report(result.report);
        self.set_workflow_report(result.summaries, report);
    }

    fn set_workflow_report(&mut self, results: Vec<WorkflowStepResult>, report: WorkflowExecutionResult) {
        self.workflow_execution = Some(report);
        self.workflow_results = results;
        let order = self.workflow_step_order();
        let answer = self.workflow_execution.clone();
        if let Some(panel) = self
            .workflow_panels
            .iter_mut()
            .find_map(|panel| match panel {
                crate::views::workflow_page::WorkflowPanel::Run(run) if run.is_executing => {
                    Some(run)
                }
                _ => None,
            })
        {
            panel.is_executing = false;
            panel.result = answer;
            self.workflow_active_step = self.workflow_execution.as_ref().and_then(|result| {
                crate::views::workflow_page::first_shown_index(result.steps.len(), order)
            });
        }
    }

    /// T019: a failed run reports nothing; both reads of it go together.
    /// T145: the catch branch resets the executing panel (React's
    /// `isExecuting: false`).
    pub fn clear_workflow_results(&mut self) {
        self.workflow_results.clear();
        self.workflow_execution = None;
        for panel in &mut self.workflow_panels {
            if let crate::views::workflow_page::WorkflowPanel::Run(run) = panel {
                run.is_executing = false;
            }
        }
    }

    /// T019: the execution-result panel's per-row expansion state.
    pub fn toggle_workflow_step_expanded(&mut self, step_id: &str) {
        if !self.workflow_step_expanded.remove(step_id) {
            self.workflow_step_expanded.insert(step_id.to_owned());
        }
    }

    /// T019: the execution-result panel's per-row table/chart view state.
    pub fn set_workflow_step_chart(&mut self, step_id: &str, chart: bool) {
        if chart {
            self.workflow_step_chart.insert(step_id.to_owned());
        } else {
            self.workflow_step_chart.remove(step_id);
        }
    }

    /// T070: the visual workflow form's slots over the real state — the
    /// wiring [`WorkflowForm`] consumes. The page's edit pane (T145) is the
    /// production caller; the form's tests drive the same slots.
    pub fn workflow_form_slots(&self) -> WorkflowFormSlots<AppState> {
        WorkflowFormSlots {
            draft: |s: &AppState| &s.workflow_draft,
            draft_mut: |s: &mut AppState| &mut s.workflow_draft,
            open: |s: &AppState| s.workflow_form_open,
            set_open: |s: &mut AppState, which| s.workflow_form_open = which,
            input_edits: |s: &mut AppState| &mut s.workflow_form_input_edits,
            input_edits_read: |s: &AppState| &s.workflow_form_input_edits,
            commands: |s: &AppState| &s.workflow_form_commands,
            commands_loading: |s: &AppState| &s.workflow_form_commands_loading,
            databases: |s: &AppState| &s.workflow_form_databases,
            databases_loading: |s: &AppState| &s.workflow_form_databases_loading,
            on_save: |s: &mut AppState| s.save_workflow_form(),
            on_cancel: |s: &mut AppState| s.close_workflow_editor(),
        }
    }

    /// T070: the form's ephemeral interaction state (React resets it by
    /// unmounting the form — the AI panel's `showForm` toggle); the draft
    /// itself stays. The page's edit-panel lifecycle (T145) is the production
    /// caller.
    pub fn reset_workflow_form_caches(&mut self) {
        self.workflow_form_open = None;
        self.workflow_form_input_edits.clear();
    }

    /// W7: run a workflow; the results list below is read-only.
    /// T8: the host's engine runs as a job (every step kind it supports); a
    /// declined job falls back to the sync paths — the host's N13 runner,
    /// then the memory port's demo runner.
    ///
    /// T145: the lite editor's run entry (`run_edited_workflow`) went with
    /// the lite page; the page's run core (`run_workflow_by_name` in
    /// `workflow_page_state.rs`) is this same path, and the answer lands
    /// through `set_workflow_results`, which also resolves the page's
    /// executing run panel.
    pub fn next_workflow_generation(&mut self) -> u32 {
        self.workflow_generation += 1;
        self.workflow_generation
    }

    /// Explicitly create the first real local board. A stale control cannot
    /// create a board after leaving the Dashboard route.
    pub(crate) fn create_first_dashboard(&mut self, name: &str) {
        if self.active != Section::Dashboards {
            return;
        }
        self.dashboard_create_error = self.desktop.create_first_native_dashboard(name).err();
    }

    pub(crate) fn begin_native_dashboard_report(&mut self) {
        if self.active == Section::Dashboards
            && !self.dashboard_report_open
            && matches!(self.desktop.native_dashboard_board(), Ok(Some(_)))
        {
            self.clear_dashboard_report_draft();
            self.dashboard_report_open = true;
        }
    }

    pub(crate) fn cancel_native_dashboard_report(&mut self) {
        if self.active == Section::Dashboards && self.dashboard_report_open {
            self.clear_dashboard_report_draft();
            self.dashboard_report_open = false;
        }
    }

    pub(crate) fn save_native_dashboard_report(&mut self) {
        if self.active != Section::Dashboards
            || !self.dashboard_report_open
            || !matches!(self.desktop.native_dashboard_board(), Ok(Some(_)))
            || !self.desktop.native_dashboard_report_session_open()
        {
            return;
        }
        let saved = self.desktop.save_dashboard_widget(
            self.widget_title.text.trim(),
            self.widget_query.text.trim(),
        );
        if self.finish_dashboard_report_save(saved) {
            self.dashboard_report_open = false;
        }
    }

    fn clear_dashboard_report_draft(&mut self) {
        self.widget_error = None;
        self.widget_title = TextEdit::new(String::new());
        self.widget_query = TextEdit::new(String::new());
    }

    /// Save the widget form. An empty title is refused. The host's
    /// dashboard use case runs first (N13); its fixed not-implemented error
    /// means the demo process, and the memory port answers instead; any
    /// other failure is real and the form shows it.
    pub fn save_widget_form(&mut self) {
        let title = self.widget_title.text.trim().to_owned();
        let query = self.widget_query.text.trim().to_owned();
        let saved = match self.desktop.save_dashboard_widget(&title, &query) {
            Ok(id) => Ok(id),
            Err(error) if error == DESKTOP_NOT_IMPLEMENTED => {
                self.workspace.save_dashboard_widget(&title, &query)
            }
            Err(error) => Err(error),
        };
        self.finish_dashboard_report_save(saved);
    }

    fn finish_dashboard_report_save(&mut self, saved: Result<u64, String>) -> bool {
        match saved {
            Ok(_) => {
                self.clear_dashboard_report_draft();
                // E2: the new widget's bars ask for themselves through the
                // same path as an explicit Refresh.
                self.refresh_widget_bars();
                true
            }
            Err(error) => {
                self.widget_error = Some(error);
                false
            }
        }
    }
}
