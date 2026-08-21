//! Core Data Transfer types (Navicat-style one-way copy).

use serde::{Deserialize, Serialize};

use super::error::TransferError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Endpoint {
    pub connection_id: String,
    pub database: String,
    pub schema: Option<String>,
}

impl Endpoint {
    pub fn normalized_schema(&self) -> Option<&str> {
        self.schema
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum TransferMode {
    Structure,
    #[default]
    Data,
    StructureAndData,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum WriteMode {
    #[default]
    Insert,
    TruncateInsert,
    DropCreateInsert,
}

impl WriteMode {
    pub fn is_destructive(self) -> bool {
        matches!(self, Self::TruncateInsert | Self::DropCreateInsert)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ColumnMapping {
    pub source_column: String,
    pub target_column: String,
    #[serde(default)]
    pub skip: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TableMapping {
    pub source_table: String,
    pub target_table: String,
    #[serde(default)]
    pub create_new: bool,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub column_mappings: Vec<ColumnMapping>,
}

fn default_true() -> bool {
    true
}

impl TableMapping {
    pub fn auto(source_table: impl Into<String>) -> Self {
        let name = source_table.into();
        Self {
            source_table: name.clone(),
            target_table: name,
            create_new: false,
            enabled: true,
            column_mappings: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TransferOptions {
    pub batch_size: u32,
    pub stop_on_error: bool,
    #[serde(default)]
    pub confirmed_destructive: bool,
}

impl Default for TransferOptions {
    fn default() -> Self {
        Self {
            batch_size: 500,
            stop_on_error: true,
            confirmed_destructive: false,
        }
    }
}

impl TransferOptions {
    pub fn validate(&self) -> Result<(), TransferError> {
        if self.batch_size == 0 {
            return Err(TransferError::validation(
                "batchSize must be greater than 0",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TransferJob {
    pub source: Endpoint,
    pub target: Endpoint,
    pub mode: TransferMode,
    pub write_mode: WriteMode,
    pub tables: Vec<TableMapping>,
    pub options: TransferOptions,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TableMappingStatus {
    Matched,
    CreateNew,
    UnmappedSource,
    UnmappedTarget,
    Disabled,
    Incompatible,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TableInspectResult {
    pub source_table: String,
    pub target_table: String,
    pub status: TableMappingStatus,
    pub create_new: bool,
    pub enabled: bool,
    pub column_mappings: Vec<ColumnMapping>,
    #[serde(default)]
    pub source_columns: Vec<String>,
    #[serde(default)]
    pub target_columns: Vec<String>,
    pub incompatible_reason: Option<String>,
    pub source_row_count: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DdlPreviewItem {
    pub source_table: String,
    pub target_table: String,
    pub ddl: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WritePlanItem {
    pub source_table: String,
    pub target_table: String,
    pub write_mode: WriteMode,
    pub mapped_columns: Vec<ColumnMapping>,
    pub estimated_rows: Option<u64>,
    pub preamble: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TransferPreview {
    pub pairing_path: String,
    pub mode: TransferMode,
    pub write_mode: WriteMode,
    pub ddl: Vec<DdlPreviewItem>,
    pub write_plans: Vec<WritePlanItem>,
    pub warnings: Vec<String>,
    pub can_execute: bool,
    pub block_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TableExecutionResult {
    pub source_table: String,
    pub target_table: String,
    pub rows_inserted: u64,
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct TransferExecutionResult {
    pub tables: Vec<TableExecutionResult>,
    pub rows_inserted: u64,
    pub cancelled: bool,
    pub partial: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TransferPairingView {
    pub path: String,
    pub supported: bool,
    pub family: Option<String>,
    pub reason: Option<String>,
}
