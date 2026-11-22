use std::collections::BTreeMap;

use crate::QueryDiagnosisSchemaState;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DroppedTable {
    pub table_name: String,
    pub schema: Option<String>,
    pub connection_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChartConfigLike {
    pub chart_type: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExplainResultLike {
    pub text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RetryValidationInputLike {
    pub sql: String,
    pub context_fingerprint: Option<String>,
    pub bound_params: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DashboardWidgetRequest {
    pub dashboard_id: String,
    pub connection_id: String,
    pub sql: String,
    pub title: Option<String>,
    pub view_mode: String,
    pub chart_config: Option<ChartConfigLike>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorkflowState {
    pub diagnosis_visible: bool,
    pub explain_result: Option<ExplainResultLike>,
    pub explain_loading: bool,
    pub explain_error: Option<String>,
    pub show_explain: bool,
    pub add_to_dashboard_open: bool,
    pub favorite_name: String,
    pub show_favorite_dialog: bool,
    pub favorite_dialog_sql: String,
    pub pending_fav_sql: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum WorkflowEffect {
    SetDiagnosisVisible(bool),
    SetExplainLoading(bool),
    SetExplainError(Option<String>),
    SetExplainResult(Option<ExplainResultLike>),
    SetShowExplain(bool),
    SetAddToDashboardOpen(bool),
    SetFavoriteDialog {
        sql: String,
        name: String,
        open: bool,
    },
    UpdateSql {
        panel_id: String,
        sql: String,
        pane: Option<String>,
    },
    EmitDashboardChanged {
        dashboard_id: String,
    },
    OpenDashboard {
        dashboard_id: String,
        name: String,
    },
    ShowMessage {
        text: String,
        kind: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorkflowOptions {
    pub panel_id: String,
    pub pane_id: Option<String>,
    pub connection_id: String,
    pub db_session_id: String,
    pub database_type: Option<String>,
    pub connection_name: Option<String>,
    pub database: String,
    pub schema: Option<String>,
    pub sql: String,
    pub error: Option<String>,
    pub chart_config: Option<ChartConfigLike>,
    pub result_view_mode: String,
    pub active_result_rows_length: usize,
    pub bound_payload: Option<BTreeMap<String, String>>,
    pub schema_state: QueryDiagnosisSchemaState,
    pub server_version: Option<String>,
    pub selected_database: Option<String>,
}
