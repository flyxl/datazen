use std::collections::BTreeMap;

use crate::{
    build_query_panel_diagnosis_context, DiagnosisContext, DiagnosisContextInput, PanelView,
    QueryContractsHost, QueryDiagnosisExecution, QueryDiagnosisSchemaState,
    QueryPanelDiagnosisContextInput,
};

use super::types::{
    DashboardWidgetRequest, ExplainResultLike, RetryValidationInputLike, WorkflowEffect,
    WorkflowOptions, WorkflowState,
};

pub trait WorkflowHost {
    fn translate(&self, key: &str) -> String;

    fn build_diagnosis(&self, input: &DiagnosisContextInput) -> Option<DiagnosisContext>;

    fn explain_enabled(&self, context: &DiagnosisContext) -> bool;

    fn validate_retry(&self, context: &DiagnosisContext, input: &RetryValidationInputLike) -> bool;

    fn validate_final_retry(&self, input: &RetryValidationInputLike) -> bool;

    fn fix_sql(&self, context: &DiagnosisContext, next_sql: &str) -> String;

    fn run_explain(
        &self,
        key: &str,
        connection_id: &str,
        sql: &str,
        database: Option<&str>,
        schema: Option<&str>,
        bound: Option<&BTreeMap<String, String>>,
    ) -> Result<ExplainResultLike, String>;

    fn panel_title(&self, panel_id: &str) -> Option<String>;

    fn save_dashboard(&self, name: &str) -> Result<(String, String), String>;

    fn create_widget(&self, request: &DashboardWidgetRequest) -> Result<(String, String), String>;

    fn read_latest_retry_input(
        &self,
        panel_id: &str,
        param_values: &BTreeMap<String, String>,
        pane_id: Option<&str>,
    ) -> Option<RetryValidationInputLike>;

    fn confirm(&self, title: &str, message: &str, label: &str, kind: &str) -> bool;

    fn run_execute(&self, kind: &str);

    fn update_sql(&self, panel_id: &str, sql: &str, pane_id: Option<&str>);

    fn show_message(&self, text: &str, kind: &str);
}

struct ContractsBridge<'a> {
    host: &'a dyn WorkflowHost,
}

impl QueryContractsHost for ContractsBridge<'_> {
    fn build_diagnosis_context(&self, input: &DiagnosisContextInput) -> Option<DiagnosisContext> {
        self.host.build_diagnosis(input)
    }

    fn find_panel_execution(
        &self,
        _panel_id: &str,
        _pane_id: Option<&str>,
    ) -> Option<(PanelView, QueryDiagnosisExecution)> {
        None
    }

    fn active_connection(&self, _connection_id: &str) -> Option<crate::ActiveConnectionView> {
        None
    }

    fn schema_state(&self, _db_session_id: &str) -> QueryDiagnosisSchemaState {
        QueryDiagnosisSchemaState::default()
    }

    fn sql_parameter_policy(&self, _database_type: Option<&str>) -> String {
        "standard".to_owned()
    }

    fn parse_sql_param_count(&self, _sql: &str, _policy: &str) -> usize {
        0
    }

    fn params_to_payload(
        &self,
        _param_values: &BTreeMap<String, String>,
    ) -> BTreeMap<String, String> {
        BTreeMap::new()
    }
}

pub struct QueryPanelWorkflows {
    pub options: WorkflowOptions,
    pub state: WorkflowState,
    pub diagnosis: Option<DiagnosisContext>,
    pub param_values: BTreeMap<String, String>,
}

impl QueryPanelWorkflows {
    pub fn new(options: WorkflowOptions) -> Self {
        Self {
            options,
            state: WorkflowState::default(),
            diagnosis: None,
            param_values: BTreeMap::new(),
        }
    }

    pub fn refresh_diagnosis(&mut self, host: &dyn WorkflowHost) {
        let input = QueryPanelDiagnosisContextInput {
            execution: QueryDiagnosisExecution {
                sql: self.options.sql.clone(),
                error: self.options.error.clone(),
            },
            connection_id: self.options.connection_id.clone(),
            db_session_id: self.options.db_session_id.clone(),
            database_type: self.options.database_type.clone(),
            connection_name: self.options.connection_name.clone(),
            database: self.options.database.clone(),
            schema: self.options.schema.clone(),
            server_version: self.options.server_version.clone(),
            schema_state: self.options.schema_state.clone(),
        };
        let bridge = ContractsBridge { host };
        self.diagnosis = build_query_panel_diagnosis_context(&input, &bridge);
    }

    fn current_retry_input(&self) -> RetryValidationInputLike {
        RetryValidationInputLike {
            sql: self.options.sql.clone(),
            context_fingerprint: self
                .diagnosis
                .as_ref()
                .map(|context| context.context_fingerprint.clone()),
            bound_params: self.options.bound_payload.clone().unwrap_or_default(),
        }
    }

    pub fn handle_explain(&mut self, host: &dyn WorkflowHost) -> Vec<WorkflowEffect> {
        if self.options.sql.trim().is_empty() {
            return Vec::new();
        }
        self.state.explain_loading = true;
        self.state.explain_error = None;
        self.state.show_explain = true;
        let pane =
            super::drop::drop_pane_key(&self.options.panel_id, self.options.pane_id.as_deref());
        let result = host.run_explain(
            &pane,
            &self.options.connection_id,
            &self.options.sql,
            self.options.selected_database.as_deref(),
            self.options.schema.as_deref(),
            self.options.bound_payload.as_ref(),
        );
        let mut effects = vec![
            WorkflowEffect::SetExplainLoading(true),
            WorkflowEffect::SetShowExplain(true),
        ];
        match result {
            Ok(result) => {
                self.state.explain_result = Some(result.clone());
                effects.push(WorkflowEffect::SetExplainResult(Some(result)));
            }
            Err(error) => {
                self.state.explain_result = None;
                self.state.explain_error = Some(error.clone());
                effects.push(WorkflowEffect::SetExplainResult(None));
                effects.push(WorkflowEffect::SetExplainError(Some(error)));
            }
        }
        self.state.explain_loading = false;
        effects.push(WorkflowEffect::SetExplainLoading(false));
        effects
    }

    pub fn handle_apply_fix_sql(
        &mut self,
        host: &dyn WorkflowHost,
        next_sql: &str,
    ) -> Vec<WorkflowEffect> {
        let context = match self.diagnosis.as_ref() {
            Some(context) => context,
            None => return Vec::new(),
        };
        let draft = host.fix_sql(context, next_sql);
        host.update_sql(
            &self.options.panel_id,
            &draft,
            self.options.pane_id.as_deref(),
        );
        vec![WorkflowEffect::UpdateSql {
            panel_id: self.options.panel_id.clone(),
            sql: draft,
            pane: self.options.pane_id.clone(),
        }]
    }

    pub fn handle_retry(&mut self, host: &dyn WorkflowHost) -> Vec<WorkflowEffect> {
        let context = match self.diagnosis.as_ref() {
            Some(context) => context,
            None => return Vec::new(),
        };
        let current = self.current_retry_input();
        if !host.validate_retry(context, &current) {
            return Vec::new();
        }
        let title = host.translate("query.retry");
        let message = host.translate("query.retryConfirm");
        if !host.confirm(&title, &message, &title, "info") {
            return Vec::new();
        }
        let latest = match host.read_latest_retry_input(
            &self.options.panel_id,
            &self.param_values,
            self.options.pane_id.as_deref(),
        ) {
            Some(input) => input,
            None => return Vec::new(),
        };
        if !host.validate_final_retry(&latest) {
            return Vec::new();
        }
        host.run_execute("full");
        Vec::new()
    }

    pub fn handle_add_to_dashboard_confirm(
        &mut self,
        host: &dyn WorkflowHost,
        dashboard_id: &str,
        new_name: Option<&str>,
    ) -> Vec<WorkflowEffect> {
        if self.options.sql.trim().is_empty() || self.options.active_result_rows_length == 0 {
            return Vec::new();
        }
        self.state.add_to_dashboard_open = false;
        let mut effects = vec![WorkflowEffect::SetAddToDashboardOpen(false)];
        let outcome = (|| -> Result<(String, String), String> {
            let target = if dashboard_id == "new" {
                let name = new_name
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned)
                    .unwrap_or_else(|| host.translate("query.dashboard.defaultName"));
                host.save_dashboard(&name)
            } else {
                let title = host.panel_title(&self.options.panel_id).unwrap_or_default();
                Ok((dashboard_id.to_owned(), title))
            }?;
            let request = DashboardWidgetRequest {
                dashboard_id: target.0.clone(),
                connection_id: self.options.connection_id.clone(),
                sql: self.options.sql.clone(),
                title: host.panel_title(&self.options.panel_id),
                view_mode: self.options.result_view_mode.clone(),
                chart_config: self.options.chart_config.clone(),
            };
            let _ = target.1;
            host.create_widget(&request)
        })();
        match outcome {
            Ok((id, name)) => {
                effects.push(WorkflowEffect::EmitDashboardChanged {
                    dashboard_id: id.clone(),
                });
                effects.push(WorkflowEffect::OpenDashboard {
                    dashboard_id: id,
                    name,
                });
            }
            Err(error) => {
                host.show_message(&error, "error");
                effects.push(WorkflowEffect::ShowMessage {
                    text: error,
                    kind: "error".to_owned(),
                });
            }
        }
        effects
    }

    pub fn open_add_favorite_dialog(&mut self, next_sql: &str) -> Vec<WorkflowEffect> {
        let trimmed = next_sql.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }
        self.state.pending_fav_sql = trimmed.to_owned();
        self.state.favorite_dialog_sql = trimmed.to_owned();
        self.state.favorite_name.clear();
        self.state.show_favorite_dialog = true;
        vec![WorkflowEffect::SetFavoriteDialog {
            sql: trimmed.to_owned(),
            name: String::new(),
            open: true,
        }]
    }

    pub fn handle_explain_error(&self, host: &dyn WorkflowHost) -> Vec<WorkflowEffect> {
        match self.diagnosis.as_ref() {
            Some(context) if host.explain_enabled(context) => {
                vec![WorkflowEffect::SetDiagnosisVisible(true)]
            }
            _ => Vec::new(),
        }
    }
}
