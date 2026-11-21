use serde_json::Value;

use crate::{BatchExportDataFormat, BatchExportMode, BatchExportOutputMode};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportScope {
    CurrentPage,
    Selected,
    EntireTable,
    FilteredTable,
}

impl ExportScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CurrentPage => "current_page",
            Self::Selected => "selected",
            Self::EntireTable => "entire_table",
            Self::FilteredTable => "filtered_table",
        }
    }

    pub fn is_provided(self) -> bool {
        matches!(self, Self::CurrentPage | Self::Selected)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FetchColumn {
    pub name: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExportContext {
    pub table_name: String,
    pub scope: ExportScope,
    pub columns: Vec<FetchColumn>,
    pub column_names: Option<Vec<String>>,
    pub rows: Option<Vec<Vec<Value>>>,
    pub db_session_id: Option<String>,
    pub database: Option<String>,
    pub schema: Option<String>,
    pub database_type: Option<String>,
    pub sql: Option<String>,
    pub mode: Option<BatchExportMode>,
    pub ddl: Option<String>,
    pub data_format: BatchExportDataFormat,
    pub output_mode: Option<BatchExportOutputMode>,
    pub primary_key: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportProgress {
    pub table: String,
    pub rows_written: u64,
    pub rows_total: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FetchOutcome {
    Saved,
    Cancelled,
    Aborted,
}

impl FetchOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Saved => "saved",
            Self::Cancelled => "cancelled",
            Self::Aborted => "aborted",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataFetcherKind {
    Provided,
    Sql,
}

impl DataFetcherKind {
    pub fn needs_backend(self) -> bool {
        matches!(self, Self::Sql)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FetchTableSource {
    Table,
    Sql { sql: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FetchTableInput {
    pub table_name: String,
    pub schema: Option<String>,
    pub columns: Vec<String>,
    pub source: FetchTableSource,
    pub primary_key: Option<String>,
    pub ddl: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FetchRequest {
    pub db_session_id: String,
    pub cancel_token: String,
    pub database_type: Option<String>,
    pub database: Option<String>,
    pub schema: Option<String>,
    pub mode: BatchExportMode,
    pub data_format: BatchExportDataFormat,
    pub output_mode: BatchExportOutputMode,
    pub tables: Vec<FetchTableInput>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FetchResult {
    Saved(u64),
    Cancelled,
    Aborted { partial_deleted: bool },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_literals_and_the_provided_split() {
        assert_eq!(ExportScope::CurrentPage.as_str(), "current_page");
        assert_eq!(ExportScope::Selected.as_str(), "selected");
        assert_eq!(ExportScope::EntireTable.as_str(), "entire_table");
        assert_eq!(ExportScope::FilteredTable.as_str(), "filtered_table");
        assert!(ExportScope::CurrentPage.is_provided());
        assert!(ExportScope::Selected.is_provided());
        assert!(!ExportScope::EntireTable.is_provided());
        assert!(!ExportScope::FilteredTable.is_provided());
    }

    #[test]
    fn outcomes_and_fetcher_kinds_keep_their_literals() {
        assert_eq!(FetchOutcome::Saved.as_str(), "saved");
        assert_eq!(FetchOutcome::Cancelled.as_str(), "cancelled");
        assert_eq!(FetchOutcome::Aborted.as_str(), "aborted");
        assert!(!DataFetcherKind::Provided.needs_backend());
        assert!(DataFetcherKind::Sql.needs_backend());
    }

    #[test]
    fn table_source_and_result_shapes_are_distinct() {
        let table = FetchTableSource::Table;
        let sql = FetchTableSource::Sql { sql: "SELECT 1".to_owned() };
        assert_ne!(table, sql);
        assert_eq!(FetchResult::Saved(3), FetchResult::Saved(3));
        assert_eq!(FetchResult::Cancelled, FetchResult::Cancelled);
        assert_eq!(FetchResult::Aborted { partial_deleted: true }, FetchResult::Aborted { partial_deleted: true });
    }
}
