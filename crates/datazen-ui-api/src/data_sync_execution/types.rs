use crate::{
    ComparisonTableRow, DataSyncJobKind, DataSyncSyncState, SelectedComparisonRow, SyncOptions,
};

pub const DATA_SYNC_EXECUTION_SURFACE: [&str; 9] = [
    "runExecute",
    "handleExecute",
    "canExecute",
    "planExecution",
    "planFinalize",
    "ExecutionEffect",
    "ExecutionPlan",
    "GuardOutcome",
    "DATA_SYNC_EXECUTION_GAP",
];

pub const DATA_SYNC_EXECUTION_GAP: [&str; 7] = [
    "React hook state, useCallback and refs wiring",
    "crypto.randomUUID job ids and generation fences",
    "syncCommands execute/compare IPC and async timing",
    "refreshEndpointSessions and cross-window refresh",
    "i18n t() for progress/error/status messages",
    "ref cleanup in finally (write/cancel/job refs)",
    "paged comparison contractVersion detection",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableSelection {
    pub source_table: String,
    pub target_table: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExecutionInput {
    pub source_id: String,
    pub target_id: String,
    pub target_read_only: bool,
    pub write_outcome_uncertain: bool,
    pub has_selected_deletes: bool,
    pub mapping_results: Vec<ComparisonTableRow>,
    pub selected_rows: Vec<TableSelection>,
    pub table_selections: Vec<TableSelection>,
    pub sync_options: SyncOptions,
    pub source_database: String,
    pub target_database: String,
    pub source_schema: Option<String>,
    pub target_schema: Option<String>,
    pub execute_job_id: String,
    pub recompare_job_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionRefresh {
    Ready {
        source_session_id: Option<String>,
        target_session_id: Option<String>,
    },
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecuteResult {
    pub outcome: Option<String>,
    pub rolled_back: bool,
    pub error: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CompareResult {
    Rows(Vec<ComparisonTableRow>),
    Paged {
        tables: Vec<ComparisonTableRow>,
        contract_version: String,
    },
    Failed(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExecutionResponses {
    pub sessions: SessionRefresh,
    pub execute: Option<ExecuteResult>,
    pub compare: Option<CompareResult>,
    pub stale: bool,
    pub cancel_requested_job: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExecutionEffect {
    Error {
        message: String,
        open: bool,
    },
    State(DataSyncSyncState),
    Progress(String),
    ClearProgress,
    Status(String),
    LastResult(Option<String>),
    Uncertain(bool),
    StepResult,
    RefreshJobs,
    ExecuteCall {
        source_session: String,
        target_session: String,
        job_id: String,
        selected_rows: Vec<TableSelection>,
        table_selections: Vec<TableSelection>,
    },
    CompareCall {
        job_id: String,
        tables: Vec<String>,
        filters: Vec<(String, String)>,
    },
    MergeMappings(Vec<ComparisonTableRow>),
    ClearSelection,
    SelectedRows(Vec<SelectedComparisonRow>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExecutionPlan {
    pub effects: Vec<ExecutionEffect>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FinalizeInput {
    pub execute_job_id: String,
    pub recompare_job_id: Option<String>,
    pub ref_job_id: Option<String>,
    pub ref_job_kind: Option<DataSyncJobKind>,
    pub cancel_requested: Option<String>,
    pub cancelling_status: Option<String>,
    pub current_status: String,
    pub cancelling_message: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FinalizePlan {
    pub clear_write_in_flight: bool,
    pub clear_status: bool,
    pub clear_execute_refs: bool,
    pub clear_compare_refs: bool,
    pub clear_cancel_request: bool,
}
