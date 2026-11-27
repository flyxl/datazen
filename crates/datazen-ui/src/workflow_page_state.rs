//! AppState method implementations for the workflow page port (T145) —
//! `packages/frontend/src/windows/workflow/WorkflowPage.tsx`'s state halves:
//! the panels, the run flow, the edit flow (visual/yaml), the confirm asks and
//! the seams the module docs in `views/workflow/workflow_page.rs` name. The
//! lite inline editor's state half it replaces lived in
//! `schema_workflows_state.rs` (`workflow_editing` and its three methods).
//!
//! The view halves read these through `AppState` directly; the React → Rune
//! state mapping is the table in `workflow_page.rs`'s module docs.

#![allow(unused_imports)]
use crate::state_def::*;
use crate::views::HistoryListItem;
use crate::views::workflow_page::{
    first_shown_index, parse_lite_workflow_yaml, workflow_definition_to_draft,
    workflow_draft_to_steps, WorkflowConfirmKind, WorkflowConfirmRequest, WorkflowContextMenuRequest,
    WorkflowEditPanel, WorkflowEditorMode, WorkflowHistoryPanel, WorkflowListItem, WorkflowPanel,
    WorkflowRunPanel, WorkflowSideTab, WorkflowStepOrder,
};
use crate::views::workflow_yaml;
use crate::views::WorkflowConnectionOption;
use crate::i18n::t;
use crate::sql_editor::SqlEditor;

use rune::{TextEdit, Tx};
use rune::el::PointerPress;
use datazen_ui_api::{AdapterJob, WorkspaceApi};

impl AppState {
    // ── the mount-time halves (React's page `useEffect`) ─────────────────────

    /// React's `loadWorkflows` (the store's fetch): the host registry's lite
    /// projection becomes the page's list. The adapter answers in memory, so
    /// the loading flag's window is this call (React's `await` has no async
    /// boundary to cross here) — the store's error slot clears with the
    /// answer, the wapps fetch's shape.
    pub fn fetch_workflow_list(&mut self) {
        self.workflow_list_loading = true;
        self.workflow_list = match self.desktop.native_workflow_list() {
            Ok(flows) => flows.into_iter().map(WorkflowListItem::from_native).collect(),
            Err(_) => self.host_workflows().iter().map(WorkflowListItem::from_workflow).collect(),
        };
        self.workflow_list_loading = false;
        self.workflow_store_error = None;
    }

    /// React's `reloadConnections` (the page's `savedConnections`): the
    /// adapter's connection list mapped onto the form's options (the lite
    /// profile carries no database, so the multi-db rule reads it as absent —
    /// the projection the keys table documents). The unwired adapter's fixed
    /// error keeps the previous list (React's unhandled rejection).
    pub fn reload_workflow_connections(&mut self) {
        let profiles = self
            .desktop
            .list_connections()
            .unwrap_or_else(|_| self.workspace.connections().to_vec());
        self.workflow_form_connections = profiles
            .into_iter()
            .map(|profile| WorkflowConnectionOption {
                id: profile.id,
                name: profile.name,
                database_type: profile.engine,
                database: None,
            })
            .collect();
    }

    /// React's `handleReload` press: `workflowReload()` (the host's dir
    /// rescan command — no adapter method, the seam) then `loadWorkflows()`.
    /// This port runs the refetch half it owns; the rescan command lands with
    /// the workflow host wiring.
    pub fn reload_workflows(&mut self) {
        self.fetch_workflow_list();
    }

    // ── the panel lifecycle ──────────────────────────────────────────────────

    /// React's `nextPanelId(prefix)` — the module-scope counter's next id.
    fn next_workflow_panel_id(&mut self, prefix: &str) -> String {
        self.workflow_panel_counter += 1;
        format!("{prefix}-{}", self.workflow_panel_counter)
    }

    /// The active panel (React's `activePanel` — the find by id, `None` when
    /// the active id names no open panel).
    pub fn active_workflow_panel(&self) -> Option<&WorkflowPanel> {
        let id = self.workflow_active_panel.as_deref()?;
        self.workflow_panels.iter().find(|panel| panel.id() == id)
    }

    /// The active run panel (React's `isWorkflowRunPanel(activePanel)` read).
    pub fn active_workflow_run_panel(&self) -> Option<&WorkflowRunPanel> {
        match self.active_workflow_panel() {
            Some(WorkflowPanel::Run(run)) => Some(run),
            _ => None,
        }
    }

    /// React's `openRunPanel` — a run panel for the workflow is reactivated;
    /// a new one opens as the active panel with the store's last result
    /// cleared (`clearWorkflowResult`).
    fn open_workflow_run_panel(&mut self, workflow: &WorkflowListItem) {
        if let Some(existing) = self.workflow_panels.iter().find(|panel| {
            matches!(panel, WorkflowPanel::Run(run) if run.workflow_id == workflow.id)
        }) {
            self.workflow_active_panel = Some(existing.id().to_owned());
            return;
        }
        let id = self.next_workflow_panel_id("wf");
        self.workflow_panels.push(WorkflowPanel::Run(WorkflowRunPanel {
            workflow_id: workflow.id.clone(),
            workflow_name: workflow.name.clone(),
            // React's `new Date().toISOString()` at open time.
            started_at: chrono::Local::now().to_rfc3339(),
            result: None,
            is_executing: false,
            variables: workflow.variables.iter().cloned().map(|definition| {
                let initial = TextEdit::new(definition.default.clone().unwrap_or_default());
                (definition, initial)
            }).collect(),
            id,
        }));
        let active = self.workflow_panels.last().map(|panel| panel.id().to_owned());
        self.workflow_active_panel = active;
        self.workflow_active_step = None;
        // React's `clearWorkflowResult()` — the store's slot (the page's other
        // panels keep their own results).
        self.workflow_results.clear();
        self.workflow_execution = None;
    }

    /// Each run tab keeps its own input state when another tab is selected.
    pub fn select_workflow_list_item(&mut self, workflow: &WorkflowListItem) {
        self.workflow_variable_select = None;
        self.open_workflow_run_panel(workflow);
    }

    /// React's `handleAiCreate` — the ai-create panel opens once; a second
    /// press reactivates the open one.
    pub fn open_workflow_ai_create_panel(&mut self) {
        if let Some(existing) = self
            .workflow_panels
            .iter()
            .find(|panel| matches!(panel, WorkflowPanel::AiCreate { .. }))
        {
            self.workflow_active_panel = Some(existing.id().to_owned());
            return;
        }
        let id = self.next_workflow_panel_id("ai");
        self.workflow_panels.push(WorkflowPanel::AiCreate { id: id.clone() });
        self.workflow_active_panel = Some(id);
    }

    /// React's `handleEdit` press — the registry lookup (`workflowGet`) then
    /// `openEditPanel(editingId, workflowDefinitionToDraft(workflow))`. The
    /// lookup's miss is React's catch branch (the feedback line).
    pub fn edit_workflow_from_list(&mut self, workflow_id: &str) {
        let Some(flow) = self
            .host_workflows()
            .into_iter()
            .find(|flow| flow.name == workflow_id)
        else {
            self.workflow_feedback = format!("workflow not found: {workflow_id}");
            return;
        };
        let draft = workflow_definition_to_draft(&flow);
        self.open_workflow_edit_panel_with(Some(workflow_id), draft);
    }

    /// React's `handleCreate` press — the create flow over `emptyDraft()`.
    pub fn open_workflow_edit_panel(&mut self, editing_id: Option<&str>) {
        let draft = crate::views::workflow_form::WorkflowDraft::empty();
        self.open_workflow_edit_panel_with(editing_id, draft);
    }

    /// React's `openEditPanel(editingId, draft)` — a panel for the same
    /// editing id is reactivated; a new one opens as the active panel with
    /// the yaml text the mode switch reads. React's `workflowGetYaml` has no
    /// adapter method, so the document is the lite writer over the same
    /// projection (the create stub's shape — the name and one `SELECT 1`
    /// step — is the writer's too).
    fn open_workflow_edit_panel_with(
        &mut self,
        editing_id: Option<&str>,
        draft: crate::views::workflow_form::WorkflowDraft,
    ) {
        if let Some(existing) = self.workflow_panels.iter().find(|panel| {
            matches!(panel, WorkflowPanel::Edit(edit) if edit.editing_id.as_deref() == editing_id)
        }) {
            self.workflow_active_panel = Some(existing.id().to_owned());
            return;
        }
        let name = if editing_id.is_some() {
            draft.name.text.clone()
        } else {
            // React's create stub serializes the empty draft as written
            // (`name: draft.name`), so the empty name writes empty.
            draft.name.text.clone()
        };
        let steps = if editing_id.is_some() {
            workflow_draft_to_steps(&draft)
        } else {
            // React's stub: one `SELECT 1` query step, not the draft's empty
            // first step.
            vec!["SELECT 1".to_owned()]
        };
        self.workflow_yaml = SqlEditor::from_doc(workflow_yaml(&name, &steps));
        self.workflow_draft = draft;
        // React's fresh mount: the form's interaction state starts clean.
        self.reset_workflow_form_caches();
        self.workflow_editor_mode = WorkflowEditorMode::Visual;
        let id = self.next_workflow_panel_id("edit");
        self.workflow_panels.push(WorkflowPanel::Edit(WorkflowEditPanel {
            editing_id: editing_id.map(str::to_owned),
            id,
        }));
        let active = self.workflow_panels.last().map(|panel| panel.id().to_owned());
        self.workflow_active_panel = active;
    }

    /// React's `openHistoryPanel` — a history panel for the id is reactivated
    /// (and re-seeded at the order's first step); a new one opens from the
    /// entry `workflowHistoryGet` would answer. The lean list item is the
    /// entry this build holds (the success flag and the total time — no step
    /// detail; the full entry is the history use case's seam).
    pub fn open_workflow_history_detail(&mut self, history_id: &str, name: &str) {
        if let Some(existing) = self
            .workflow_panels
            .iter()
            .find(|panel| matches!(panel, WorkflowPanel::History(history) if history.history_id == history_id))
        {
            let reseed = existing
                .execution_result()
                .and_then(|result| first_shown_index(result.steps.len(), self.workflow_step_order));
            self.workflow_active_panel = Some(existing.id().to_owned());
            self.workflow_active_step = reseed;
            return;
        }
        let (created_at, result) = match self.desktop.get_native_workflow_history(history_id) {
            Ok(Some(run)) => (
                run.created_at,
                crate::views::WorkflowExecutionResult::from_native_report(run.result),
            ),
            Ok(None) => {
                self.workflow_feedback = format!("history entry not found: {history_id}");
                return;
            }
            Err(error) if error == datazen_ui_api::DESKTOP_NOT_IMPLEMENTED => {
                let Some(item) = self.workflow_history_items.iter().find(|item| item.id == history_id)
                else {
                    self.workflow_feedback = format!("history entry not found: {history_id}");
                    return;
                };
                (item.created_at.clone(), crate::views::WorkflowExecutionResult {
                    success: item.success,
                    total_time_ms: item.total_time_ms,
                    steps: Vec::new(),
                    final_output: String::new(),
                    error: None,
                })
            }
            Err(error) => {
                self.workflow_operation_error = Some(error);
                return;
            }
        };
        let id = self.next_workflow_panel_id("hist");
        let first = first_shown_index(result.steps.len(), self.workflow_step_order);
        let panel = WorkflowHistoryPanel {
            id,
            history_id: history_id.to_owned(),
            workflow_name: name.to_owned(),
            created_at,
            result,
        };
        let active = panel.id.clone();
        self.workflow_panels.push(WorkflowPanel::History(panel));
        self.workflow_active_panel = Some(active);
        self.workflow_active_step = first;
    }

    /// React's `closePanel(panelId)` — the panel leaves the list; the active
    /// id follows to the neighbour at the closed tab's slot (or none when the
    /// last one closed). Closing the *active* panel unmounts its content —
    /// the S-owned interaction slots reset with it (React's per-mount row
    /// state); closing an edit panel resets the form's interaction state
    /// either way (the form unmounts inside it).
    pub fn close_workflow_panel(&mut self, panel_id: &str) {
        let Some(index) = self
            .workflow_panels
            .iter()
            .position(|panel| panel.id() == panel_id)
        else {
            return;
        };
        let was_active = self.workflow_active_panel.as_deref() == Some(panel_id);
        let closed = self.workflow_panels.remove(index);
        if was_active {
            // React: `next[Math.min(idx, next.length - 1)].id`, else `null`.
            let neighbour = self
                .workflow_panels
                .get(index.min(self.workflow_panels.len().saturating_sub(1)))
                .map(|panel| panel.id().to_owned())
                .or_else(|| {
                    self.workflow_panels
                        .last()
                        .map(|panel| panel.id().to_owned())
                });
            self.workflow_active_panel = neighbour;
            // The active panel's content unmounts with it.
            self.workflow_variable_select = None;
            self.workflow_step_expanded.clear();
            self.workflow_step_chart.clear();
            self.workflow_chart_expanded.clear();
            self.workflow_chart_split.clear();
            self.workflow_chart_export_open.clear();
        }
        if matches!(closed, WorkflowPanel::Edit(_)) {
            self.reset_workflow_form_caches();
        }
    }

    /// React's panel-tab click — the panel becomes the active one and the
    /// step view re-seeds at the result's first shown step.
    pub fn select_workflow_panel(&mut self, panel_id: &str) {
        self.workflow_active_panel = Some(panel_id.to_owned());
        self.workflow_variable_select = None;
        self.workflow_active_step = self
            .workflow_panels
            .iter()
            .find(|panel| panel.id() == panel_id)
            .and_then(|panel| panel.execution_result())
            .and_then(|result| first_shown_index(result.steps.len(), self.workflow_step_order));
    }

    // ── the run flow ──────────────────────────────────────────────────────────

    /// React's `handleExecute` → `runWorkflowInPanel(panelId, workflowId,
    /// vars)`: the native job uses only this panel's declarations and inputs.
    /// Empty inputs are left to the engine's default/required handling.
    pub fn execute_active_workflow(&mut self) {
        let Some(run) = self.active_workflow_run_panel() else {
            return;
        };
        let panel_id = run.id.clone();
        let workflow_id = run.workflow_id.clone();
        let mut variables = std::collections::BTreeMap::new();
        for (definition, edit) in &run.variables {
            if edit.text.is_empty() {
                continue;
            }
            let value = if definition.var_type == "number" {
                match edit.text.parse::<serde_json::Number>() {
                    Ok(number) => serde_json::Value::Number(number),
                    Err(_) => {
                        self.workflow_operation_error = Some(format!("Invalid number: {}", definition.name));
                        return;
                    }
                }
            } else {
                serde_json::Value::String(edit.text.clone())
            };
            variables.insert(definition.name.clone(), value);
        }
        self.workflow_operation_error = None;
        if let Some(panel) = self.workflow_panels.iter_mut().find(|panel| panel.id() == panel_id) {
            if let WorkflowPanel::Run(run) = panel {
                run.is_executing = true;
            }
        }
        self.workflow_active_step = None;
        // React's `clearWorkflowResult()` before the run.
        self.workflow_results.clear();
        self.workflow_execution = None;
        self.run_workflow_by_name(&workflow_id, variables);
    }

    /// The run's engine half (the lite editor's `run_edited_workflow` core,
    /// renamed): the job path first, the sync fallback under it (the host's
    /// N13 runner, then the memory port's demo runner). The answer lands
    /// through `set_workflow_results` (the delivery arm and this fallback
    /// share it), which also resolves the executing panel.
    fn run_workflow_by_name(
        &mut self,
        name: &str,
        variables: std::collections::BTreeMap<String, serde_json::Value>,
    ) {
        let generation = self.next_workflow_generation();
        let job = AdapterJob::RunWorkflow {
            generation,
            name: name.to_owned(),
            variables,
        };
        if self.desktop.begin_job(job) {
            self.workflow_loading = Some(generation);
            return;
        }
        let ran = match self.desktop.run_workflow(name) {
            Ok(results) => Ok(results),
            Err(error) if error == datazen_ui_api::DESKTOP_NOT_IMPLEMENTED => {
                self.workspace.run_workflow(name)
            }
            Err(error) => Err(error),
        };
        match ran {
            Ok(results) => self.set_workflow_results(results),
            Err(_) => self.clear_workflow_results(),
        }
    }

    pub fn workflow_variable(&self, name: &str) -> Option<&str> {
        self.active_workflow_run_panel()?.variables.iter()
            .find(|(definition, _)| definition.name == name)
            .map(|(_, edit)| edit.text.as_str())
    }

    pub fn set_workflow_variable(&mut self, name: &str, value: String) {
        if let Some(panel_id) = self.workflow_active_panel.clone() {
            if let Some(edit) = self.edit_workflow_variable(&panel_id, name) {
                *edit = TextEdit::new(value);
            }
        }
    }

    pub(crate) fn edit_workflow_variable(&mut self, panel_id: &str, name: &str) -> Option<&mut TextEdit> {
        self.workflow_panels.iter_mut().find_map(|panel| match panel {
            WorkflowPanel::Run(run) if run.id == panel_id => {
                run.variables.iter_mut().find(|(definition, _)| definition.name == name).map(|(_, edit)| edit)
            }
            _ => None,
        })
    }

    // ── the edit flow ─────────────────────────────────────────────────────────

    /// React's `setEditMode(panelId, editorMode)` — the switch is real in both
    /// directions: yaml writes the lite document over the draft's projection
    /// (`draftToYamlObject` + `YAML.stringify`), visual parses the document
    /// back (`parseWorkflowYaml` + `yamlObjectToDraft` — the lean parse of
    /// exactly the writer's shape). A document the lean parse refuses is
    /// React's parse-error branch: the mode stays, the operation error names
    /// it.
    pub fn set_workflow_editor_mode(&mut self, mode: WorkflowEditorMode) {
        if self.workflow_editor_mode == mode {
            // React: `if (p.editorMode === editorMode) return p;`.
            return;
        }
        match mode {
            WorkflowEditorMode::Yaml => {
                let name = self.workflow_draft.name.text.clone();
                let steps = workflow_draft_to_steps(&self.workflow_draft);
                self.workflow_yaml = SqlEditor::from_doc(workflow_yaml(&name, &steps));
                self.workflow_editor_mode = mode;
            }
            WorkflowEditorMode::Visual => {
                match parse_lite_workflow_yaml(self.workflow_yaml.doc()) {
                    Some((name, steps)) => {
                        let flow = datazen_ui_api::Workflow { name, steps };
                        self.workflow_draft = workflow_definition_to_draft(&flow);
                        // React's remounted form starts its interaction clean.
                        self.reset_workflow_form_caches();
                        self.workflow_editor_mode = mode;
                    }
                    None => {
                        self.workflow_operation_error = Some(
                            t("workflows.editor.parseError").replace(
                                "{error}",
                                "the document is not the lite yaml writer's shape (T646's \
                                 parseWorkflowYaml lands the full grammar)",
                            ),
                        );
                    }
                }
            }
        }
    }

    /// Whether the workflow page's yaml document should hold the keyboard
    /// (`app_key`/`app_ime`'s routing condition — the lite editor's
    /// `workflow_editing` gate this replaces): the section is showing, the
    /// active panel is the edit pane and the mode is yaml.
    pub fn workflow_yaml_focused(&self) -> bool {
        self.active == crate::types::Section::Workflows
            && matches!(
                self.active_workflow_panel(),
                Some(WorkflowPanel::Edit(_))
            )
            && self.workflow_editor_mode == WorkflowEditorMode::Yaml
    }

    /// React's shared action bar's Save — the visual mode saves the draft
    /// (`handleSave` → `workflowDraftToDefinition` → `workflowSave`), the
    /// yaml mode the document (`handleSaveYaml` → `parseWorkflowYaml` →
    /// `workflowSaveYaml`; the lean parse reads what this build writes, and
    /// the save rides the registry surface both halves share). A successful
    /// save closes the panel, writes the feedback line and reloads the list;
    /// a failure writes the feedback line (React's catch).
    pub fn save_workflow_edit_panel(&mut self) {
        let (name, steps) = if self.workflow_editor_mode == WorkflowEditorMode::Visual {
            (
                self.workflow_draft.name.text.trim().to_owned(),
                workflow_draft_to_steps(&self.workflow_draft),
            )
        } else {
            match parse_lite_workflow_yaml(self.workflow_yaml.doc()) {
                Some((name, steps)) => (name, steps),
                None => {
                    self.workflow_feedback = t("workflows.editor.parseError")
                        .replace("{error}", "the document is not the lite yaml writer's shape")
                        .to_owned();
                    return;
                }
            }
        };
        match self.save_workflow_through_host(&name, &steps) {
            Ok(()) => {
                let active = self.workflow_active_panel.clone();
                if let Some(id) = active {
                    self.close_workflow_panel(&id);
                }
                // React's 2s feedback reset is the timer seam — the line
                // stays until the next feedback write.
                self.workflow_feedback = t("workflows.saved").to_owned();
                self.fetch_workflow_list();
            }
            Err(error) => {
                self.workflow_feedback = error;
            }
        }
    }

    /// React's shared action bar's Cancel (`onCancel` → `closePanel`).
    pub fn close_edit_panel(&mut self) {
        let Some(id) = self.workflow_active_panel.clone() else {
            return;
        };
        self.close_workflow_panel(&id);
    }

    // ── the confirm asks ─────────────────────────────────────────────────────

    /// React's `handleDelete` ask — the `useConfirmDialog` gate holds the
    /// pending request (the wapps remove pattern; a new ask is a fresh
    /// dialog mount through the epoch).
    pub fn ask_workflow_delete(&mut self, workflow_id: &str) {
        self.workflow_confirm_request = Some(WorkflowConfirmRequest {
            kind: WorkflowConfirmKind::DeleteWorkflow(workflow_id.to_owned()),
            title: t("workflows.delete").to_owned(),
            message: t("workflows.deleteConfirm").to_owned(),
        });
        self.workflow_confirm_epoch = self.workflow_confirm_epoch.wrapping_add(1);
    }

    /// React's `handleClearHistory` ask — the same gate, the history kind.
    pub fn ask_workflow_history_clear(&mut self) {
        self.workflow_confirm_request = Some(WorkflowConfirmRequest {
            kind: WorkflowConfirmKind::ClearHistory,
            title: t("workflows.history.clear").to_owned(),
            message: t("workflows.history.clearConfirm").to_owned(),
        });
        self.workflow_confirm_epoch = self.workflow_confirm_epoch.wrapping_add(1);
    }

    /// The confirm dialog's answer (React's `confirmWf(...)` promise
    /// resolving): cancel only clears the ask; confirm runs the ask's
    /// command through its own branch.
    pub fn answer_workflow_confirm(&mut self, ok: bool) {
        let Some(request) = self.workflow_confirm_request.take() else {
            return;
        };
        if !ok {
            // React: `if (!confirmed) return;`.
            return;
        }
        match request.kind {
            WorkflowConfirmKind::DeleteWorkflow(workflow_id) => {
                if let Err(error) = self.delete_workflow(&workflow_id) {
                    // React's catch: the operation error over the translated
                    // template.
                    self.workflow_operation_error =
                        Some(t("workflows.operationFailed").replace("{error}", &error));
                }
            }
            WorkflowConfirmKind::ClearHistory => {
                self.clear_workflow_history();
            }
        }
    }

    /// Run removal only after the page's actual confirmation.
    pub fn delete_workflow(&mut self, workflow_id: &str) -> Result<(), String> {
        self.desktop.delete_visible_workflow(workflow_id)?;
        self.workflow_operation_error = None;
        self.workflow_deleted(workflow_id);
        Ok(())
    }

    /// The confirmed workflow's run panels close and the list reloads.
    pub fn workflow_deleted(&mut self, workflow_id: &str) {
        let active_id = self.workflow_active_panel.clone();
        self.workflow_panels.retain(|panel| {
            !matches!(panel, WorkflowPanel::Run(run) if run.workflow_id == workflow_id)
        });
        if active_id
            .as_deref()
            .is_some_and(|id| self.workflow_panels.iter().all(|panel| panel.id() != id))
        {
            self.workflow_active_panel = self.workflow_panels.last().map(|panel| panel.id().to_owned());
        }
        self.fetch_workflow_list();
    }

    pub fn clear_workflow_history(&mut self) {
        match self.desktop.clear_native_workflow_history() {
            Ok(()) => {
                self.workflow_operation_error = None;
                self.workflow_history_cleared();
            }
            Err(error) if error == datazen_ui_api::DESKTOP_NOT_IMPLEMENTED => {
                self.workflow_operation_error = Some(
                    "workflow history clear needs the live host; no history adapter was provided".to_owned(),
                );
            }
            Err(error) => self.workflow_operation_error = Some(error),
        }
    }

    pub fn load_workflow_history(&mut self) {
        match self.desktop.list_native_workflow_history() {
            Ok(rows) => {
                self.workflow_operation_error = None;
                self.workflow_history_loaded(rows.into_iter().map(|row| HistoryListItem {
                    id: row.id,
                    workflow_id: row.workflow_id,
                    workflow_name: row.workflow_name,
                    success: row.success,
                    total_time_ms: row.total_time_ms,
                    created_at: row.created_at,
                }).collect());
            }
            Err(error) if error == datazen_ui_api::DESKTOP_NOT_IMPLEMENTED => {}
            Err(error) => self.workflow_operation_error = Some(error),
        }
    }

    pub fn workflow_history_loaded(&mut self, items: Vec<HistoryListItem>) {
        self.workflow_history_items = items;
    }

    pub fn workflow_history_cleared(&mut self) {
        self.workflow_history_items.clear();
    }

    // ── the side tab and the misc presses ─────────────────────────────────────

    /// React's side-tab click; entering the history tab runs the load
    /// (`useEffect`'s `sideTab === 'history'` arm — the seam's no-op).
    pub fn set_workflow_side_tab(&mut self, tab: WorkflowSideTab) {
        self.workflow_side_tab = tab;
        if tab == WorkflowSideTab::History {
            self.load_workflow_history();
        }
    }

    /// React's `toggleStepOrder` — the order flips; the settingsStore write
    /// (`updateSettings`) is the settings track's seam.
    pub fn toggle_workflow_step_order(&mut self) {
        self.workflow_step_order = match self.workflow_step_order {
            WorkflowStepOrder::Desc => WorkflowStepOrder::Asc,
            WorkflowStepOrder::Asc => WorkflowStepOrder::Desc,
        };
    }

    /// React's step-tab click (`setActiveStepIndex(i)`).
    pub fn set_workflow_active_step(&mut self, index: usize) {
        self.workflow_active_step = Some(index);
    }

    /// React's operation-error dismiss.
    pub fn dismiss_workflow_operation_error(&mut self) {
        self.workflow_operation_error = None;
    }

    /// React's add-to-dashboard press — the dialog flag the modal layer's
    /// pending mount reads (T018's dialog port swaps it).
    pub fn open_add_to_dashboard(&mut self) {
        self.workflow_add_dashboard_open = true;
    }

    /// The pending dialog's close (React's `onClose`) — the flag goes.
    pub fn close_add_to_dashboard(&mut self) {
        self.workflow_add_dashboard_open = false;
    }

    /// React's `handleOpenDir` — `settingsCommands.openWorkflowsDir` (the
    /// browser-mode catch is silent; here the seam says itself). Unreachable
    /// while the dir seam keeps the button hidden — the wired press for the
    /// day the get-dir command lands.
    pub fn open_workflow_dir(&mut self) {
        self.workflow_feedback =
            "opening the workflows directory needs the desktop host; the open-dir use case is \
             not wired into this Rune build yet"
                .to_owned();
    }

    /// React's docs button — `openDocsWindow('workflows')` needs the window
    /// manager; the Rune app is single-window, so the press says so (the
    /// feedback line is the seam the module docs name).
    pub fn open_workflow_docs(&mut self) {
        self.workflow_feedback =
            "the workflows docs window needs the window manager; no second window exists in \
             this Rune build yet"
                .to_owned();
    }

    /// The list row's secondary press — the native context-menu seam's
    /// trigger record (the workflow id and the window point; the entries
    /// builder and the native host are T646's).
    pub fn open_workflow_context_menu(&mut self, workflow_id: &str, point: PointerPress) {
        self.workflow_context_menu = Some(WorkflowContextMenuRequest {
            workflow_id: workflow_id.to_owned(),
            point: (point.x, point.y),
        });
    }
}
