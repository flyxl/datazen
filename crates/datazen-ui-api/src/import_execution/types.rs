use std::cell::Cell;

use serde_json::{Map, Value};

pub const IMPORT_DEFAULT_MAX_ROWS_PER_BATCH: usize = 500;
pub const IMPORT_DEFAULT_MAX_SQL_BYTES: usize = 512 * 1024;
pub const IMPORT_DEFAULT_MAX_BATCHES_PER_CHUNK: usize = 8;

pub const IMPORT_STREAM_GAP: [&str; 4] = [
    "queryCommands.executeQueryStream",
    "queryCommands.cancelQuery",
    "executionStarted",
    "statementEnd",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportColumnPair {
    pub source: String,
    pub target: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportSqlOptions {
    pub table: String,
    pub target_columns: Vec<String>,
    pub database_type: Option<String>,
}

pub struct ImportSqlDialect {
    pub escape_ident: Box<dyn Fn(&str, Option<&str>) -> String>,
    pub escape_value: Box<dyn Fn(Option<&str>, &Value) -> String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportBatch {
    pub row_start: usize,
    pub row_count: usize,
    pub sql: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportChunk {
    pub batches: Vec<ImportBatch>,
    pub sql: String,
    pub row_start: usize,
    pub row_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportCommitMode {
    Transaction,
    Autocommit,
}

impl ImportCommitMode {
    pub fn as_str(self) -> &'static str {
        match self {
            ImportCommitMode::Transaction => "transaction",
            ImportCommitMode::Autocommit => "autocommit",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportErrorStrategy {
    Stop,
    Skip,
}

impl ImportErrorStrategy {
    pub fn as_str(self) -> &'static str {
        match self {
            ImportErrorStrategy::Stop => "stop",
            ImportErrorStrategy::Skip => "skip",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlanImportChunksOptions {
    pub table: String,
    pub target_columns: Vec<String>,
    pub database_type: Option<String>,
    pub rows: Vec<Map<String, Value>>,
    pub column_pairs: Vec<ImportColumnPair>,
    pub commit_mode: ImportCommitMode,
    pub max_rows_per_batch: usize,
    pub max_sql_bytes: usize,
    pub max_batches_per_chunk: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportProgressStatus {
    Running,
    Completed,
    Stopped,
    Cancelled,
}

impl ImportProgressStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ImportProgressStatus::Running => "running",
            ImportProgressStatus::Completed => "completed",
            ImportProgressStatus::Stopped => "stopped",
            ImportProgressStatus::Cancelled => "cancelled",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportProgress {
    pub status: ImportProgressStatus,
    pub processed_rows: usize,
    pub committed_rows: usize,
    pub failed_rows: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportRowFailure {
    pub row_index: usize,
    pub source_line: Option<i64>,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportFinalStatus {
    Completed,
    Stopped,
    Cancelled,
}

impl ImportFinalStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ImportFinalStatus::Completed => "completed",
            ImportFinalStatus::Stopped => "stopped",
            ImportFinalStatus::Cancelled => "cancelled",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportRunResult {
    pub status: ImportFinalStatus,
    pub committed_rows: usize,
    pub failed_rows: Vec<ImportRowFailure>,
    pub stopped_by: Option<ImportRowFailure>,
}

pub struct ImportTransport {
    pub run_stream: Box<dyn FnMut(&str) -> Result<u64, String>>,
    pub cancel_in_flight: Box<dyn FnMut()>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AbortSignal {
    aborted: Cell<bool>,
}

impl AbortSignal {
    pub fn abort(&self) {
        self.aborted.set(true);
    }

    pub fn is_aborted(&self) -> bool {
        self.aborted.get()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportStreamEvent {
    ExecutionStarted { execution_id: String },
    StatementEnd { rows_affected: u64 },
    Other,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportStreamProgress {
    execution_id: Option<String>,
    total_affected: u64,
}

impl ImportStreamProgress {
    pub fn on_event(&mut self, event: &ImportStreamEvent) {
        match event {
            ImportStreamEvent::ExecutionStarted { execution_id } => {
                self.execution_id = Some(execution_id.clone());
            }
            ImportStreamEvent::StatementEnd { rows_affected } => {
                self.total_affected += rows_affected;
            }
            ImportStreamEvent::Other => (),
        }
    }

    pub fn execution_id(&self) -> Option<&str> {
        self.execution_id.as_deref()
    }

    pub fn total_affected(&self) -> u64 {
        self.total_affected
    }
}
