use std::cell::RefCell;
use std::collections::BTreeMap;

use super::*;
use crate::{
    query_drop_handler::{
        ChartConfigLike, DashboardWidgetRequest, ExplainResultLike, RetryValidationInputLike,
        WorkflowEffect, WorkflowOptions, WorkflowState,
    },
    DiagnosisContext, DiagnosisContextInput, QueryDiagnosisSchemaState,
};

struct RecordingHost {
    diagnosis: Option<DiagnosisContext>,
    explain_enabled: bool,
    retry_ok: bool,
    final_ok: bool,
    explain_result: Result<ExplainResultLike, String>,
    latest: Option<RetryValidationInputLike>,
    confirm: bool,
    dashboard_error: Option<String>,
    widget_error: Option<String>,
    explain_calls: RefCell<Vec<String>>,
    confirm_calls: RefCell<Vec<(String, String)>>,
    execute_calls: RefCell<Vec<String>>,
    sql_updates: RefCell<Vec<(String, String)>>,
    saved_boards: RefCell<Vec<String>>,
    widgets: RefCell<Vec<DashboardWidgetRequest>>,
    messages: RefCell<Vec<(String, String)>>,
}

impl Default for RecordingHost {
    fn default() -> Self {
        Self {
            diagnosis: None,
            explain_enabled: false,
            retry_ok: false,
            final_ok: false,
            explain_result: Ok(ExplainResultLike::default()),
            latest: None,
            confirm: false,
            dashboard_error: None,
            widget_error: None,
            explain_calls: RefCell::new(Vec::new()),
            confirm_calls: RefCell::new(Vec::new()),
            execute_calls: RefCell::new(Vec::new()),
            sql_updates: RefCell::new(Vec::new()),
            saved_boards: RefCell::new(Vec::new()),
            widgets: RefCell::new(Vec::new()),
            messages: RefCell::new(Vec::new()),
        }
    }
}

impl WorkflowHost for RecordingHost {
    fn translate(&self, key: &str) -> String {
        format!("i18n:{key}")
    }

    fn build_diagnosis(&self, input: &DiagnosisContextInput) -> Option<DiagnosisContext> {
        let _ = input;
        self.diagnosis.clone()
    }

    fn explain_enabled(&self, _context: &DiagnosisContext) -> bool {
        self.explain_enabled
    }

    fn validate_retry(
        &self,
        _context: &DiagnosisContext,
        _input: &RetryValidationInputLike,
    ) -> bool {
        self.retry_ok
    }

    fn validate_final_retry(&self, _input: &RetryValidationInputLike) -> bool {
        self.final_ok
    }

    fn fix_sql(&self, _context: &DiagnosisContext, next_sql: &str) -> String {
        format!("FIXED:{next_sql}")
    }

    fn run_explain(
        &self,
        key: &str,
        _connection_id: &str,
        _sql: &str,
        _database: Option<&str>,
        _schema: Option<&str>,
        _bound: Option<&BTreeMap<String, String>>,
    ) -> Result<ExplainResultLike, String> {
        self.explain_calls.borrow_mut().push(key.to_owned());
        self.explain_result.clone()
    }

    fn panel_title(&self, _panel_id: &str) -> Option<String> {
        Some("Panel".to_owned())
    }

    fn save_dashboard(&self, name: &str) -> Result<(String, String), String> {
        self.saved_boards.borrow_mut().push(name.to_owned());
        match &self.dashboard_error {
            Some(error) => Err(error.clone()),
            None => Ok(("d1".to_owned(), name.to_owned())),
        }
    }

    fn create_widget(&self, request: &DashboardWidgetRequest) -> Result<(String, String), String> {
        self.widgets.borrow_mut().push(request.clone());
        match &self.widget_error {
            Some(error) => Err(error.clone()),
            None => Ok(("d1".to_owned(), "Board".to_owned())),
        }
    }

    fn read_latest_retry_input(
        &self,
        _panel_id: &str,
        _values: &BTreeMap<String, String>,
        _pane_id: Option<&str>,
    ) -> Option<RetryValidationInputLike> {
        self.latest.clone()
    }

    fn confirm(&self, title: &str, message: &str, _label: &str, _kind: &str) -> bool {
        self.confirm_calls
            .borrow_mut()
            .push((title.to_owned(), message.to_owned()));
        self.confirm
    }

    fn run_execute(&self, kind: &str) {
        self.execute_calls.borrow_mut().push(kind.to_owned());
    }

    fn update_sql(&self, _panel_id: &str, sql: &str, _pane_id: Option<&str>) {
        self.sql_updates
            .borrow_mut()
            .push((sql.to_owned(), String::new()));
    }

    fn show_message(&self, text: &str, kind: &str) {
        self.messages
            .borrow_mut()
            .push((text.to_owned(), kind.to_owned()));
    }
}

fn options() -> WorkflowOptions {
    WorkflowOptions {
        panel_id: "p".to_owned(),
        pane_id: Some("side".to_owned()),
        connection_id: "c".to_owned(),
        db_session_id: "s".to_owned(),
        database_type: Some("postgres".to_owned()),
        connection_name: Some("Prod".to_owned()),
        database: "app".to_owned(),
        schema: Some("public".to_owned()),
        sql: "SELECT 1".to_owned(),
        error: None,
        chart_config: Some(ChartConfigLike {
            chart_type: "bar".to_owned(),
        }),
        result_view_mode: "table".to_owned(),
        active_result_rows_length: 2,
        bound_payload: None,
        schema_state: QueryDiagnosisSchemaState::default(),
        server_version: Some("16".to_owned()),
        selected_database: Some("app".to_owned()),
    }
}

fn host() -> RecordingHost {
    RecordingHost {
        diagnosis: Some(DiagnosisContext {
            context_fingerprint: "fp".to_owned(),
        }),
        explain_result: Ok(ExplainResultLike {
            text: "plan".to_owned(),
        }),
        latest: Some(RetryValidationInputLike {
            sql: "SELECT 1".to_owned(),
            context_fingerprint: Some("fp".to_owned()),
            bound_params: BTreeMap::new(),
        }),
        ..RecordingHost::default()
    }
}

fn has_effect(effects: &[WorkflowEffect], expected: &WorkflowEffect) -> bool {
    effects.iter().any(|effect| effect == expected)
}

#[test]
fn diagnosis_is_built_from_the_contracts_port() {
    let h = host();
    let mut workflows = QueryPanelWorkflows::new(options());
    workflows.refresh_diagnosis(&h);
    assert_eq!(
        workflows
            .diagnosis
            .as_ref()
            .map(|c| c.context_fingerprint.as_str()),
        Some("fp")
    );
}

#[test]
fn explain_requires_sql_and_tracks_success_and_failure() {
    let h = host();
    let mut workflows = QueryPanelWorkflows::new(options());
    workflows.options.sql = "  ".to_owned();
    assert!(workflows.handle_explain(&h).is_empty());
    assert!(h.explain_calls.borrow().is_empty());

    workflows.options.sql = "SELECT 1".to_owned();
    let effects = workflows.handle_explain(&h);
    assert_eq!(h.explain_calls.borrow().as_slice(), &["p::side".to_owned()]);
    assert_eq!(
        workflows
            .state
            .explain_result
            .as_ref()
            .map(|r| r.text.as_str()),
        Some("plan")
    );
    assert!(!workflows.state.explain_loading);
    assert!(workflows.state.show_explain);
    assert!(has_effect(
        &effects,
        &WorkflowEffect::SetExplainLoading(false)
    ));

    let mut failing = host();
    failing.explain_result = Err("boom".to_owned());
    let effects = workflows.handle_explain(&failing);
    assert_eq!(workflows.state.explain_error.as_deref(), Some("boom"));
    assert!(has_effect(
        &effects,
        &WorkflowEffect::SetExplainError(Some("boom".to_owned()))
    ));
}

#[test]
fn fix_sql_updates_the_panel_with_the_pane_argument() {
    let h = host();
    let mut workflows = QueryPanelWorkflows::new(options());
    workflows.refresh_diagnosis(&h);
    let effects = workflows.handle_apply_fix_sql(&h, "SELECT 2");
    assert_eq!(h.sql_updates.borrow().len(), 1);
    assert_eq!(h.sql_updates.borrow()[0].0, "FIXED:SELECT 2");
    assert!(has_effect(
        &effects,
        &WorkflowEffect::UpdateSql {
            panel_id: "p".to_owned(),
            sql: "FIXED:SELECT 2".to_owned(),
            pane: Some("side".to_owned()),
        }
    ));
}

#[test]
fn retry_walks_validation_confirmation_and_the_fresh_gate() {
    let mut h = host();
    let mut workflows = QueryPanelWorkflows::new(options());
    workflows.refresh_diagnosis(&h);
    h.retry_ok = false;
    assert!(workflows.handle_retry(&h).is_empty());
    assert!(h.confirm_calls.borrow().is_empty());

    h.retry_ok = true;
    h.confirm = false;
    assert!(workflows.handle_retry(&h).is_empty());
    assert_eq!(h.confirm_calls.borrow().len(), 1);

    h.confirm = true;
    h.latest = None;
    assert!(workflows.handle_retry(&h).is_empty());
    assert!(h.execute_calls.borrow().is_empty());

    h.latest = Some(RetryValidationInputLike {
        sql: "SELECT 1".to_owned(),
        context_fingerprint: None,
        bound_params: BTreeMap::new(),
    });
    h.final_ok = false;
    assert!(workflows.handle_retry(&h).is_empty());
    assert!(h.execute_calls.borrow().is_empty());

    h.final_ok = true;
    workflows.handle_retry(&h);
    assert_eq!(h.execute_calls.borrow().as_slice(), &["full".to_owned()]);
}

#[test]
fn dashboard_creation_requires_sql_and_rows_then_creates_and_broadcasts() {
    let h = host();
    let mut workflows = QueryPanelWorkflows::new(options());
    workflows.options.sql.clear();
    assert!(workflows
        .handle_add_to_dashboard_confirm(&h, "d1", None)
        .is_empty());
    workflows.options.sql = "SELECT 1".to_owned();
    workflows.options.active_result_rows_length = 0;
    assert!(workflows
        .handle_add_to_dashboard_confirm(&h, "d1", None)
        .is_empty());
    assert!(h.widgets.borrow().is_empty());

    workflows.options.active_result_rows_length = 1;
    let effects = workflows.handle_add_to_dashboard_confirm(&h, "new", Some("  Board  "));
    assert_eq!(h.saved_boards.borrow().as_slice(), &["Board".to_owned()]);
    assert_eq!(h.widgets.borrow()[0].dashboard_id, "d1");
    assert_eq!(h.widgets.borrow()[0].title.as_deref(), Some("Panel"));
    assert!(has_effect(
        &effects,
        &WorkflowEffect::EmitDashboardChanged {
            dashboard_id: "d1".to_owned()
        }
    ));
    assert!(has_effect(
        &effects,
        &WorkflowEffect::OpenDashboard {
            dashboard_id: "d1".to_owned(),
            name: "Board".to_owned(),
        }
    ));
}

#[test]
fn a_dashboard_failure_reports_a_message() {
    let h = host();
    let mut workflows = QueryPanelWorkflows::new(options());
    let mut failing = h;
    failing.widget_error = Some("denied".to_owned());
    let effects = workflows.handle_add_to_dashboard_confirm(&failing, "d1", None);
    assert_eq!(
        failing.messages.borrow().as_slice(),
        &[("denied".to_owned(), "error".to_owned())]
    );
    assert!(has_effect(
        &effects,
        &WorkflowEffect::ShowMessage {
            text: "denied".to_owned(),
            kind: "error".to_owned(),
        }
    ));
}

#[test]
fn favorite_dialog_keeps_the_trimmed_sql_and_ignores_blank_input() {
    let h = host();
    let mut workflows = QueryPanelWorkflows::new(options());
    assert!(workflows.open_add_favorite_dialog("  ").is_empty());
    let effects = workflows.open_add_favorite_dialog("  SELECT 1  ");
    assert_eq!(workflows.state.pending_fav_sql, "SELECT 1");
    assert_eq!(workflows.state.favorite_name, "");
    assert!(workflows.state.show_favorite_dialog);
    assert!(has_effect(
        &effects,
        &WorkflowEffect::SetFavoriteDialog {
            sql: "SELECT 1".to_owned(),
            name: String::new(),
            open: true,
        }
    ));
}

#[test]
fn explain_error_entry_is_gated_by_the_action() {
    let h = host();
    let mut workflows = QueryPanelWorkflows::new(options());
    workflows.refresh_diagnosis(&h);
    assert!(workflows.handle_explain_error(&h).is_empty());
    let mut enabled = host();
    enabled.explain_enabled = true;
    assert_eq!(
        workflows.handle_explain_error(&enabled),
        vec![WorkflowEffect::SetDiagnosisVisible(true)]
    );
    let _: Option<WorkflowState> = Some(workflows.state.clone());
}
