use serde_json::Value;

use super::host::WorkflowPanelHost;
use super::rules::variable_defaults;
use super::types::{
    ConfirmKind, ExecuteParams, PanelTab, WorkflowPanelEffect, WorkflowPanelLabels,
    WorkflowPanelOptions, WorkflowPanelState, WorkflowStoreView,
};
use crate::WorkflowListItem;

pub struct WorkflowPanel {
    pub options: WorkflowPanelOptions,
    pub labels: WorkflowPanelLabels,
    pub state: WorkflowPanelState,
}

impl WorkflowPanel {
    pub fn new(options: WorkflowPanelOptions, labels: WorkflowPanelLabels) -> Self {
        Self {
            options,
            labels,
            state: WorkflowPanelState::default(),
        }
    }

    fn apply_store(&mut self, view: WorkflowStoreView) {
        self.state.workflows = view.workflows;
        self.state.workflows_loading = view.loading;
        self.state.is_executing = view.executing;
        self.state.workflow_error = view.error;
        self.state.result = view.result;
    }

    pub fn mount(&mut self, host: &mut dyn WorkflowPanelHost) {
        let view = host.load_workflows();
        self.apply_store(view);
        self.state.workflows_dir = host.workflow_get_dir();
        self.state.saved_connections = host.get_connections();
    }

    pub fn refresh_workflows(&mut self, host: &mut dyn WorkflowPanelHost) {
        let view = host.load_workflows();
        self.apply_store(view);
    }

    pub fn reload(&mut self, host: &mut dyn WorkflowPanelHost) {
        let _ = host.workflow_reload();
        self.refresh_workflows(host);
    }

    pub fn set_tab(&mut self, host: &mut dyn WorkflowPanelHost, tab: PanelTab) {
        let entering_history = self.state.tab != PanelTab::History && tab == PanelTab::History;
        self.state.tab = tab;
        if entering_history {
            self.state.history_items = host.workflow_history_list();
        }
    }

    pub fn select_workflow(
        &mut self,
        host: &mut dyn WorkflowPanelHost,
        workflow: &WorkflowListItem,
    ) {
        self.state.selected_workflow = Some(workflow.clone());
        host.clear_workflow_result();
        self.state.result = None;
        self.state.variables = variable_defaults(workflow);
    }

    pub fn execute(&mut self, host: &mut dyn WorkflowPanelHost) {
        let selected = match self.state.selected_workflow.clone() {
            Some(workflow) => workflow,
            None => return,
        };
        host.execute_workflow(&ExecuteParams {
            workflow_id: selected.id,
            variables: self.state.variables.clone(),
            connection_id: self.options.db_session_id.clone(),
        });
        self.state.history_items = host.workflow_history_list();
    }

    pub fn create(&mut self) {
        self.state.draft = Value::Null;
        self.state.editing_id = None;
        self.state.show_form = true;
    }

    pub fn edit(&mut self, host: &dyn WorkflowPanelHost, id: &str) {
        match host.workflow_get(id) {
            Ok(definition) => {
                self.state.draft = definition;
                self.state.editing_id = Some(id.to_owned());
                self.state.show_form = true;
            }
            Err(message) => self.state.feedback = error_text(&message),
        }
    }

    pub fn delete(&mut self, host: &mut dyn WorkflowPanelHost, id: &str) {
        let title = self.labels.delete.clone();
        let message = self.labels.delete_confirm.clone();
        if !host.confirm(ConfirmKind::DeleteWorkflow, &title, &message) {
            return;
        }
        match host.workflow_delete(id) {
            Ok(()) => {
                if self
                    .state
                    .selected_workflow
                    .as_ref()
                    .map(|workflow| workflow.id.as_str())
                    == Some(id)
                {
                    self.state.selected_workflow = None;
                    host.clear_workflow_result();
                    self.state.result = None;
                }
                self.refresh_workflows(host);
            }
            Err(message) => self.state.feedback = error_text(&message),
        }
    }

    pub fn save(&mut self, host: &mut dyn WorkflowPanelHost) -> Vec<WorkflowPanelEffect> {
        match host.workflow_save(&self.state.draft) {
            Ok(()) => {
                self.state.show_form = false;
                self.state.feedback = self.labels.saved.clone();
                self.refresh_workflows(host);
                vec![WorkflowPanelEffect::ScheduleFeedbackClear {
                    delay_ms: self.options.feedback_delay_ms,
                }]
            }
            Err(message) => {
                self.state.feedback = error_text(&message);
                Vec::new()
            }
        }
    }

    pub fn clear_feedback(&mut self) {
        self.state.feedback.clear();
    }

    pub fn view_history(&mut self, host: &dyn WorkflowPanelHost, id: &str) {
        match host.workflow_history_get(id) {
            Ok(entry) => self.state.history_detail = Some(entry.result),
            Err(message) => self.state.feedback = error_text(&message),
        }
    }

    pub fn clear_history(&mut self, host: &dyn WorkflowPanelHost) {
        let title = self.labels.history_clear.clone();
        let message = self.labels.history_clear_confirm.clone();
        if !host.confirm(ConfirmKind::ClearHistory, &title, &message) {
            return;
        }
        match host.workflow_history_clear() {
            Ok(()) => {
                self.state.history_items.clear();
                self.state.history_detail = None;
            }
            Err(message) => self.state.feedback = error_text(&message),
        }
    }

    pub fn close_form(&mut self) {
        self.state.show_form = false;
    }

    pub fn close_history_detail(&mut self) {
        self.state.history_detail = None;
    }

    pub fn open_docs(&self, host: &dyn WorkflowPanelHost) -> WorkflowPanelEffect {
        let topic = "workflows";
        host.open_docs(topic);
        WorkflowPanelEffect::DocsOpened {
            topic: topic.to_owned(),
        }
    }

    pub fn is_loading(&self) -> bool {
        super::rules::is_loading(self.options.locales_ready, self.state.workflows_loading)
    }
}

fn error_text(message: &str) -> String {
    super::rules::error_text(message)
}
