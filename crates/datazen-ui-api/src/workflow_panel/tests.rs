use std::cell::RefCell;
use std::collections::BTreeMap;

use serde_json::{json, Value};

use super::*;
use crate::{StepExecutionResult, WorkflowExecutionResult, WorkflowRunRecord, WorkflowVariable};

fn labels() -> WorkflowPanelLabels {
    WorkflowPanelLabels {
        title: "Workflows".to_owned(),
        history_title: "History".to_owned(),
        create: "Create".to_owned(),
        reload: "Reload".to_owned(),
        docs_help: "Help".to_owned(),
        storage_dir: "Directory".to_owned(),
        storage_dir_hint: "Hint".to_owned(),
        empty: "No workflows".to_owned(),
        edit: "Edit".to_owned(),
        delete: "Delete".to_owned(),
        delete_confirm: "Delete it?".to_owned(),
        saved: "Saved".to_owned(),
        loading: "Loading".to_owned(),
        running: "Running".to_owned(),
        run: "Run".to_owned(),
        select_connection: "Pick one".to_owned(),
        var_type_connection: "Connection".to_owned(),
        history_clear: "Clear".to_owned(),
        history_clear_confirm: "Clear all?".to_owned(),
    }
}

fn variable(name: &str, kind: &str, default: Option<Value>) -> WorkflowVariable {
    WorkflowVariable {
        name: name.to_owned(),
        r#type: kind.to_owned(),
        description: name.to_owned(),
        required: None,
        default,
    }
}

fn workflow(id: &str, variables: Vec<WorkflowVariable>) -> crate::WorkflowListItem {
    crate::WorkflowListItem {
        id: id.to_owned(),
        name: format!("Flow {id}"),
        description: "desc".to_owned(),
        variables,
        scheduled: None,
    }
}

fn result() -> WorkflowExecutionResult {
    WorkflowExecutionResult {
        success: true,
        final_output: "done".to_owned(),
        steps: Vec::<StepExecutionResult>::new(),
        total_time_ms: 5,
        error: None,
    }
}

fn history_item(id: &str) -> crate::HistoryListItem {
    crate::HistoryListItem {
        id: id.to_owned(),
        workflow_id: "wf".to_owned(),
        workflow_name: "Flow".to_owned(),
        success: true,
        total_time_ms: 5,
        created_at: "2026-01-01T00:00:00Z".to_owned(),
    }
}

#[derive(Default)]
struct RecordingHost {
    workflows: Vec<crate::WorkflowListItem>,
    loading: bool,
    executing: bool,
    error: Option<String>,
    result: Option<WorkflowExecutionResult>,
    dir: String,
    connections: Vec<ConnectionSummary>,
    history: Vec<crate::HistoryListItem>,
    details: BTreeMap<String, WorkflowRunRecord>,
    confirms: RefCell<Vec<(ConfirmKind, String, String)>>,
    confirm_answer: bool,
    executions: RefCell<Vec<ExecuteParams>>,
    deletes: RefCell<Vec<String>>,
    saves: RefCell<Vec<Value>>,
    reloads: RefCell<usize>,
    docs: RefCell<Vec<String>>,
    clears: RefCell<usize>,
    gets: BTreeMap<String, Value>,
    delete_error: Option<String>,
    save_error: Option<String>,
    history_error: Option<String>,
}

impl WorkflowPanelHost for RecordingHost {
    fn load_workflows(&mut self) -> WorkflowStoreView {
        WorkflowStoreView {
            workflows: self.workflows.clone(),
            loading: self.loading,
            executing: self.executing,
            error: self.error.clone(),
            result: self.result.clone(),
        }
    }

    fn workflow_get_dir(&self) -> String {
        self.dir.clone()
    }

    fn get_connections(&self) -> Vec<ConnectionSummary> {
        self.connections.clone()
    }

    fn workflow_history_list(&self) -> Vec<crate::HistoryListItem> {
        self.history.clone()
    }

    fn clear_workflow_result(&mut self) {
        *self.clears.borrow_mut() += 1;
    }

    fn execute_workflow(&mut self, params: &ExecuteParams) {
        self.executions.borrow_mut().push(params.clone());
    }

    fn workflow_get(&self, id: &str) -> Result<Value, String> {
        self.gets
            .get(id)
            .cloned()
            .ok_or_else(|| "not found".to_owned())
    }

    fn workflow_delete(&self, id: &str) -> Result<(), String> {
        self.deletes.borrow_mut().push(id.to_owned());
        match &self.delete_error {
            Some(error) => Err(error.clone()),
            None => Ok(()),
        }
    }

    fn workflow_save(&self, definition: &Value) -> Result<(), String> {
        self.saves.borrow_mut().push(definition.clone());
        match &self.save_error {
            Some(error) => Err(error.clone()),
            None => Ok(()),
        }
    }

    fn workflow_reload(&self) -> Result<(), String> {
        *self.reloads.borrow_mut() += 1;
        Ok(())
    }

    fn workflow_history_get(&self, id: &str) -> Result<WorkflowRunRecord, String> {
        match &self.history_error {
            Some(error) => Err(error.clone()),
            None => self
                .details
                .get(id)
                .cloned()
                .ok_or_else(|| "missing".to_owned()),
        }
    }

    fn workflow_history_clear(&self) -> Result<(), String> {
        Ok(())
    }

    fn confirm(&self, kind: ConfirmKind, title: &str, message: &str) -> bool {
        self.confirms
            .borrow_mut()
            .push((kind, title.to_owned(), message.to_owned()));
        self.confirm_answer
    }

    fn open_docs(&self, topic: &str) {
        self.docs.borrow_mut().push(topic.to_owned());
    }
}

fn host() -> RecordingHost {
    RecordingHost {
        dir: "/data/wf".to_owned(),
        connections: vec![ConnectionSummary {
            id: "c1".to_owned(),
            name: "Prod".to_owned(),
            database_type: "postgres".to_owned(),
            database: Some("app".to_owned()),
        }],
        confirm_answer: true,
        ..RecordingHost::default()
    }
}

fn panel() -> WorkflowPanel {
    WorkflowPanel::new(
        WorkflowPanelOptions {
            db_session_id: Some("session-7".to_owned()),
            ..WorkflowPanelOptions::default()
        },
        labels(),
    )
}

#[test]
fn variable_defaults_use_js_string_coercion_and_empty_for_null() {
    let variables = vec![
        variable("s", "string", Some(json!("hi"))),
        variable("n", "number", Some(json!(7))),
        variable("b", "string", Some(json!(true))),
        variable("nil", "string", Some(Value::Null)),
        variable("absent", "string", None),
    ];
    let defaults = variable_defaults(&workflow("wf", variables));
    assert_eq!(defaults["s"], "hi");
    assert_eq!(defaults["n"], "7");
    assert_eq!(defaults["b"], "true");
    assert_eq!(defaults["nil"], "");
    assert_eq!(defaults["absent"], "");
}

#[test]
fn feedback_tone_and_loading_gate_follow_the_react_rules() {
    assert_eq!(feedback_tone("Error: boom"), FeedbackTone::Error);
    assert_eq!(feedback_tone("error: boom"), FeedbackTone::Error);
    assert_eq!(feedback_tone("Saved"), FeedbackTone::Info);
    assert!(is_loading(false, false));
    assert!(is_loading(true, true));
    assert!(!is_loading(true, false));
}

#[test]
fn mounting_reads_workflows_directory_and_connections() {
    let mut h = host();
    h.workflows = vec![workflow("wf", Vec::new())];
    h.error = Some("old".to_owned());
    h.result = Some(result());
    let mut p = panel();
    p.mount(&mut h);
    assert_eq!(p.state.workflows.len(), 1);
    assert_eq!(p.state.workflows_dir, "/data/wf");
    assert_eq!(p.state.saved_connections.len(), 1);
    assert_eq!(p.state.workflow_error.as_deref(), Some("old"));
    assert!(p.state.result.is_some());
}

#[test]
fn selecting_a_workflow_seeds_defaults_and_clears_the_result() {
    let mut h = host();
    let mut p = panel();
    p.state.result = Some(result());
    let wf = workflow("wf", vec![variable("name", "string", Some(json!("Ann")))]);
    p.select_workflow(&mut h, &wf);
    assert_eq!(p.state.variables["name"], "Ann");
    assert_eq!(p.state.result, None);
    assert_eq!(*h.clears.borrow(), 1);
}

#[test]
fn execute_sends_the_db_session_id_and_reloads_history() {
    let mut h = host();
    h.history = vec![history_item("run")];
    let mut p = panel();
    p.select_workflow(&mut h, &workflow("wf-9", Vec::new()));
    p.state.variables.insert("x".to_owned(), "1".to_owned());
    p.execute(&mut h);
    let calls = h.executions.borrow();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].workflow_id, "wf-9");
    assert_eq!(calls[0].connection_id.as_deref(), Some("session-7"));
    assert_eq!(calls[0].variables["x"], "1");
    drop(calls);
    assert_eq!(p.state.history_items.len(), 1);
}

#[test]
fn execute_without_a_selection_does_nothing() {
    let mut h = host();
    let mut p = panel();
    p.execute(&mut h);
    assert!(h.executions.borrow().is_empty());
}

#[test]
fn create_opens_a_fresh_form_and_edit_failure_reports_feedback() {
    let mut h = host();
    let mut p = panel();
    p.create();
    assert!(p.state.show_form);
    assert_eq!(p.state.editing_id, None);
    assert_eq!(p.state.draft, Value::Null);

    p.close_form();
    p.edit(&h, "missing");
    assert!(!p.state.show_form);
    assert_eq!(p.state.feedback, error_text("not found"));
}

#[test]
fn edit_success_opens_the_form_with_the_loaded_definition() {
    let h = host();
    let mut h = h;
    h.gets.insert("wf".to_owned(), json!({"steps": []}));
    let mut p = panel();
    p.edit(&h, "wf");
    assert!(p.state.show_form);
    assert_eq!(p.state.editing_id.as_deref(), Some("wf"));
    assert_eq!(p.state.draft, json!({"steps": []}));
}

#[test]
fn delete_is_gated_by_confirmation_and_clears_the_selected_result() {
    let mut h = host();
    h.workflows = vec![workflow("wf", Vec::new())];
    let mut p = panel();
    p.state.selected_workflow = Some(workflow("wf", Vec::new()));
    p.state.result = Some(result());

    h.confirm_answer = false;
    p.delete(&mut h, "wf");
    assert!(h.deletes.borrow().is_empty());
    assert!(p.state.selected_workflow.is_some());

    h.confirm_answer = true;
    p.delete(&mut h, "wf");
    assert_eq!(h.deletes.borrow().as_slice(), &["wf".to_owned()]);
    assert_eq!(p.state.selected_workflow, None);
    assert_eq!(p.state.result, None);
    assert_eq!(*h.clears.borrow(), 1);
    let confirms = h.confirms.borrow();
    assert_eq!(confirms[0].0, ConfirmKind::DeleteWorkflow);
    assert_eq!(confirms[0].1, "Delete");
}

#[test]
fn saving_schedules_feedback_clear_and_reloads_the_list() {
    let mut h = host();
    let mut p = panel();
    p.state.show_form = true;
    p.state.draft = json!({"name": "new"});
    let effects = p.save(&mut h);
    assert!(!p.state.show_form);
    assert_eq!(p.state.feedback, "Saved");
    assert_eq!(feedback_tone(&p.state.feedback), FeedbackTone::Info);
    assert_eq!(
        effects,
        vec![WorkflowPanelEffect::ScheduleFeedbackClear { delay_ms: 2_000 }]
    );
    assert_eq!(h.saves.borrow().as_slice(), &[json!({"name": "new"})]);
    p.clear_feedback();
    assert_eq!(p.state.feedback, "");
}

#[test]
fn a_failed_save_reports_an_error_without_scheduling_a_clear() {
    let mut h = host();
    h.save_error = Some("disk full".to_owned());
    let mut p = panel();
    p.state.show_form = true;
    assert!(p.save(&mut h).is_empty());
    assert!(p.state.show_form);
    assert_eq!(p.state.feedback, error_text("disk full"));
    assert_eq!(feedback_tone(&p.state.feedback), FeedbackTone::Error);
}

#[test]
fn reload_and_entering_the_history_tab_each_load_once() {
    let mut h = host();
    h.history = vec![history_item("r1")];
    let mut p = panel();
    p.reload(&mut h);
    assert_eq!(*h.reloads.borrow(), 1);
    p.set_tab(&mut h, PanelTab::Workflows);
    assert!(p.state.history_items.is_empty());
    p.set_tab(&mut h, PanelTab::History);
    assert_eq!(p.state.history_items.len(), 1);
    p.set_tab(&mut h, PanelTab::History);
    assert_eq!(p.state.history_items.len(), 1);
}

#[test]
fn history_detail_and_clear_are_confirmed_and_observable() {
    let mut h = host();
    let run = WorkflowRunRecord {
        id: "run".to_owned(),
        workflow_id: "wf".to_owned(),
        workflow_name: "Flow".to_owned(),
        variables: BTreeMap::new(),
        result: result(),
        created_at: "now".to_owned(),
    };
    h.details.insert("run".to_owned(), run);
    h.history = vec![history_item("run")];
    let mut p = panel();
    p.view_history(&h, "run");
    assert!(p.state.history_detail.is_some());
    p.close_history_detail();
    assert_eq!(p.state.history_detail, None);

    h.confirm_answer = true;
    p.clear_history(&h);
    assert!(p.state.history_items.is_empty());
    assert_eq!(
        h.confirms.borrow().last().map(|entry| entry.0),
        Some(ConfirmKind::ClearHistory)
    );
}

#[test]
fn connection_variables_get_a_select_with_the_placeholder_first() {
    let h = host();
    let inputs = variable_input(
        "connection",
        &h.connections,
        &labels().select_connection,
        true,
    );
    match inputs {
        VariableInput::Connection { options, disabled } => {
            assert!(disabled);
            assert_eq!(options.len(), 2);
            assert_eq!(
                options[0],
                WorkflowSelectOption {
                    value: String::new(),
                    label: "Pick one".to_owned()
                }
            );
            assert_eq!(options[1].value, "c1");
        }
        VariableInput::Text { .. } => panic!("expected a connection select"),
    }
    assert!(is_connection_variable("connection"));
    assert!(!is_connection_variable("string"));
    match variable_input("string", &[], "", false) {
        VariableInput::Text { disabled, .. } => assert!(!disabled),
        VariableInput::Connection { .. } => panic!("expected a text input"),
    }
}

#[test]
fn docs_opening_is_an_observable_effect() {
    let h = host();
    let p = panel();
    let effect = p.open_docs(&h);
    assert_eq!(
        effect,
        WorkflowPanelEffect::DocsOpened {
            topic: "workflows".to_owned()
        }
    );
    assert_eq!(h.docs.borrow().as_slice(), &["workflows".to_owned()]);
}

#[test]
fn gap_and_surface_are_pinned_and_root_reexports_match() {
    assert_eq!(WORKFLOW_PANEL_SURFACE.len(), 9);
    assert!(WORKFLOW_PANEL_SURFACE.contains(&"WorkflowPanel"));
    assert_eq!(WORKFLOW_PANEL_GAP.len(), 8);
    assert_eq!(crate::WORKFLOW_PANEL_SURFACE.len(), 9);
    assert_eq!(crate::WORKFLOW_PANEL_GAP.len(), 8);
    let _: Option<crate::PanelTab> = Some(PanelTab::History);
}
