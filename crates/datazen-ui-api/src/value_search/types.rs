use crate::types::ColumnSchema;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueSearchMode {
    Contains,
    Exact,
    Prefix,
    Suffix,
    NotContains,
}

impl ValueSearchMode {
    pub fn as_str(self) -> &'static str {
        match self {
            ValueSearchMode::Contains => "contains",
            ValueSearchMode::Exact => "exact",
            ValueSearchMode::Prefix => "prefix",
            ValueSearchMode::Suffix => "suffix",
            ValueSearchMode::NotContains => "notContains",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueSearchTarget {
    pub table: String,
    pub schema: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueSearchTableResult {
    pub table: String,
    pub schema: Option<String>,
    pub matches: Option<i64>,
    pub columns: Vec<String>,
    pub reason: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueSearchStatus {
    Idle,
    Running,
    Paused,
    Done,
    Cancelled,
}

impl ValueSearchStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ValueSearchStatus::Idle => "idle",
            ValueSearchStatus::Running => "running",
            ValueSearchStatus::Paused => "paused",
            ValueSearchStatus::Done => "done",
            ValueSearchStatus::Cancelled => "cancelled",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ValueSearchProgress {
    pub status: ValueSearchStatus,
    pub completed: usize,
    pub total: usize,
    pub current: Option<String>,
    pub results: Vec<ValueSearchTableResult>,
    pub elapsed_ms: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueSearchLocateIntent {
    pub table: String,
    pub schema: Option<String>,
    pub database: String,
    pub value: String,
    pub mode: ValueSearchMode,
    pub columns: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchTableValuesRequest {
    pub db_session_id: String,
    pub database: String,
    pub schema: Option<String>,
    pub table: String,
    pub columns: Vec<String>,
    pub value: String,
    pub mode: ValueSearchMode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchTableValuesResponse {
    pub table: String,
    pub matches: i64,
    pub searched_columns: Vec<String>,
}

pub type SchemaReader = Box<dyn Fn(&ValueSearchTarget) -> Result<Vec<ColumnSchema>, String>>;
pub type TableValueSearcher = Box<dyn Fn(&SearchTableValuesRequest) -> Result<SearchTableValuesResponse, String>>;
pub type Clock = Box<dyn Fn() -> i64>;
pub type DriverExtraTypes = Box<dyn Fn(&str) -> Vec<crate::ValueSearchTypeGroup>>;
