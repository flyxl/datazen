use serde_json::Value;

pub const PAGE_SIZE: usize = 500;
pub const DIFF_DETAIL_DOM_GAP: [&str; 4] = [
    "useEffect table reset",
    "Checkbox onChange",
    "table row rendering",
    "pagination Button onClick",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataSyncOperation {
    Insert,
    Update,
    Delete,
    Unchanged,
}

impl DataSyncOperation {
    pub fn as_str(self) -> &'static str {
        match self {
            DataSyncOperation::Insert => "INSERT",
            DataSyncOperation::Update => "UPDATE",
            DataSyncOperation::Delete => "DELETE",
            DataSyncOperation::Unchanged => "UNCHANGED",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SyncOptions {
    pub insert: bool,
    pub update: bool,
    pub delete: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DataSyncRowChange {
    pub operation: DataSyncOperation,
    pub key: Vec<Value>,
    pub source_row: Option<Vec<Option<Value>>>,
    pub target_row: Option<Vec<Option<Value>>>,
    pub changed_columns: Vec<String>,
    pub selected: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DataSyncTableResult {
    pub source_table: String,
    pub target_table: String,
    pub columns: Option<Vec<String>>,
    pub rows: Option<Vec<DataSyncRowChange>>,
    pub page_size: Option<usize>,
    pub row_count: Option<usize>,
}

pub fn operation_allowed(operation: DataSyncOperation, options: &SyncOptions) -> bool {
    match operation {
        DataSyncOperation::Insert => options.insert,
        DataSyncOperation::Update => options.update,
        DataSyncOperation::Delete => options.delete,
        DataSyncOperation::Unchanged => false,
    }
}

pub fn row_key_string(key: &[Value]) -> String {
    match serde_json::to_string(key) {
        Ok(text) => text,
        Err(_) => format!("{key:?}"),
    }
}

pub fn format_cell(value: Option<&Value>) -> String {
    match value {
        None => "NULL".to_owned(),
        Some(Value::Null) => "NULL".to_owned(),
        Some(Value::Bool(flag)) => flag.to_string(),
        Some(Value::String(text)) => text.clone(),
        Some(Value::Number(number)) => number.to_string(),
        Some(value @ (Value::Array(_) | Value::Object(_))) => value.to_string(),
    }
}

pub fn operation_badge_class(operation: DataSyncOperation) -> &'static str {
    match operation {
        DataSyncOperation::Insert => "bg-green-500/15 text-green-700 dark:text-green-400",
        DataSyncOperation::Update => "bg-accent/15 text-accent",
        DataSyncOperation::Delete => "bg-red-500/15 text-red-700 dark:text-red-400",
        DataSyncOperation::Unchanged => "bg-surface-alt text-fg-muted",
    }
}
