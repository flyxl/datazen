//! Shared request helpers for Data Sync IPC.

use crate::data_sync::{
    ConflictPolicy, LargeValueMode, MatchingStrategy, SyncOptions, SyncSourceFilter, TableMapping,
    TableResult,
};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SyncOptionsInput {
    pub insert: Option<bool>,
    pub update: Option<bool>,
    pub delete: Option<bool>,
    pub matching_strategy: Option<MatchingStrategy>,
    pub batch_size: Option<u32>,
    pub large_value_mode: Option<LargeValueMode>,
    pub conflict_policy: Option<ConflictPolicy>,
}

impl SyncOptionsInput {
    pub fn into_options(self) -> SyncOptions {
        let mut opts = SyncOptions::default();
        if let Some(v) = self.insert {
            opts.insert = v;
        }
        if let Some(v) = self.update {
            opts.update = v;
        }
        if let Some(v) = self.delete {
            opts.delete = v;
        }
        if let Some(v) = self.matching_strategy {
            opts.matching_strategy = v;
        }
        if let Some(v) = self.batch_size {
            opts.batch_size = v;
        }
        if let Some(v) = self.large_value_mode {
            opts.large_value_mode = v;
        }
        if let Some(v) = self.conflict_policy {
            opts.conflict_policy = v;
        }
        opts
    }
}

pub fn resolve_options(input: Option<SyncOptionsInput>) -> SyncOptions {
    input.unwrap_or_default().into_options()
}

/// Normalize selected databases for self-sync checks (UI selection wins over config default).
pub fn resolve_db_name(selected: Option<&str>, config_default: Option<&str>) -> String {
    selected
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or(config_default.map(str::trim).filter(|s| !s.is_empty()))
        .unwrap_or("")
        .to_string()
}

pub fn is_self_sync(
    source_db_session_id: &str,
    target_db_session_id: &str,
    source_database: &str,
    target_database: &str,
    source_schema: Option<&str>,
    target_schema: Option<&str>,
) -> bool {
    if source_db_session_id != target_db_session_id {
        return false;
    }
    if source_database != target_database {
        return false;
    }
    normalize_schema(source_schema) == normalize_schema(target_schema)
}

fn normalize_schema(schema: Option<&str>) -> Option<&str> {
    schema.map(str::trim).filter(|s| !s.is_empty())
}

/// Keep only tables whose `TableInfo.schema` matches (case-sensitive, PostgreSQL).
pub fn filter_tables_by_schema(
    tables: Vec<datazen_driver_api::TableInfo>,
    schema: Option<&str>,
) -> Vec<datazen_driver_api::TableInfo> {
    match normalize_schema(schema) {
        Some(s) => tables
            .into_iter()
            .filter(|t| t.schema.as_deref() == Some(s))
            .collect(),
        None => tables,
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct GenerateSqlRequest {
    pub source_db_session_id: String,
    pub target_db_session_id: String,
    pub tables: Vec<TableResult>,
    pub options: SyncOptionsInput,
    pub source_database: Option<String>,
    pub target_database: Option<String>,
    pub source_schema: Option<String>,
    pub target_schema: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableMappingInput {
    pub source_table: String,
    pub target_table: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

impl From<TableMappingInput> for TableMapping {
    fn from(value: TableMappingInput) -> Self {
        TableMapping {
            source_table: value.source_table,
            target_table: value.target_table,
            enabled: value.enabled,
            matching_columns: Vec::new(),
            source_filter: None,
        }
    }
}

/// Everything one data sync comparison needs: the two sessions, the scope
/// and the selection.
///
/// The desktop window and the workflow migration step ask for the same
/// comparison, so the shape of the ask belongs to the feature: one contract
/// for the desktop host and a web host alike.
#[derive(Debug, Clone)]
pub struct SyncComparisonRequest {
    pub source_db_session_id: String,
    pub target_db_session_id: String,
    /// The source tables to compare.
    pub tables: Vec<String>,
    /// Client-chosen job id; the host mints one when it is empty.
    pub job_id: Option<String>,
    pub source_database: Option<String>,
    pub target_database: Option<String>,
    pub source_schema: Option<String>,
    pub target_schema: Option<String>,
    pub options: SyncOptions,
    /// The table mapping (source/target pairs) behind the comparison.
    pub mappings: Vec<TableMapping>,
    /// Per-source-table filters.
    pub source_filters: std::collections::HashMap<String, SyncSourceFilter>,
}
