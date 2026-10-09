//! Core Data Transfer types (Navicat-style one-way copy).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::error::TransferError;
use super::filter::SourceFilter;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Endpoint {
    pub db_session_id: String,
    pub database: String,
    pub schema: Option<String>,
}

/// A destination selected through the native save dialog. The token is an
/// opaque server-side handle; clients never submit a filesystem path or SQL
/// text to the transfer commands.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SqlFileEncoding {
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
}

impl Default for SqlFileEncoding {
    fn default() -> Self {
        Self::Utf8
    }
}

impl SqlFileEncoding {
    /// Parse the stable profile/workflow spelling while keeping the IPC enum
    /// strict: unknown values never silently fall back to UTF-8.
    pub fn parse_profile(value: &str) -> Result<Self, TransferError> {
        match value.trim() {
            "utf8" | "UTF-8" => Ok(Self::Utf8),
            "utf8Bom" | "utf-8-bom" | "UTF-8-BOM" => Ok(Self::Utf8Bom),
            "utf16Le" | "utf-16le" | "UTF-16LE" => Ok(Self::Utf16Le),
            "utf16Be" | "utf-16be" | "UTF-16BE" => Ok(Self::Utf16Be),
            other => Err(TransferError::validation(format!(
                "unsupported SQL-file encoding '{other}'"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SqlFileCompression {
    None,
    Gzip,
}

impl Default for SqlFileCompression {
    fn default() -> Self {
        Self::None
    }
}

impl SqlFileCompression {
    /// Parse the stable profile/workflow spelling while keeping the IPC enum
    /// strict: unknown values never silently fall back to no compression.
    pub fn parse_profile(value: &str) -> Result<Self, TransferError> {
        match value.trim() {
            "none" | "None" => Ok(Self::None),
            "gzip" | "GZIP" | "gz" => Ok(Self::Gzip),
            other => Err(TransferError::validation(format!(
                "unsupported SQL-file compression '{other}'"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SqlFileTarget {
    pub file_token: String,
    /// Optional registered driver id used to render the SQL artifact. When
    /// omitted, the source driver's dialect is used for backwards
    /// compatibility.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database_type: Option<String>,
    /// Optional target catalog/database used when the selected SQL dialect
    /// supports qualifying relations with a database name (for example
    /// MySQL, ClickHouse, or SQL Server).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    /// Optional target schema. When either qualifier is supplied, the target
    /// SQL never falls back to the source endpoint's schema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    /// Optional text encoding for the generated artifact. Omitted preserves
    /// the historical UTF-8 output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encoding: Option<SqlFileEncoding>,
    /// Optional compression for the generated artifact. Omitted preserves
    /// the historical uncompressed output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compression: Option<SqlFileCompression>,
}

impl SqlFileTarget {
    pub fn normalized_database_type(&self) -> Option<&str> {
        self.database_type
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }

    pub fn normalized_database(&self) -> Option<&str> {
        self.database
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }

    pub fn normalized_schema(&self) -> Option<&str> {
        self.schema
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }

    /// Validate and canonicalize the optional target relation qualifiers.
    ///
    /// Accepting only one identifier segment prevents an IPC caller from
    /// smuggling an already-qualified relation or SQL fragment into output.
    pub fn normalize_qualifiers(&mut self) -> Result<(), TransferError> {
        self.database = normalize_sql_identifier("target database/catalog", self.database.take())?;
        self.schema = normalize_sql_identifier("target schema", self.schema.take())?;
        Ok(())
    }

    pub fn validate_qualifiers(&self) -> Result<(), TransferError> {
        let mut copy = self.clone();
        copy.normalize_qualifiers()
    }

    pub fn has_explicit_scope(&self) -> bool {
        self.normalized_database().is_some() || self.normalized_schema().is_some()
    }

    pub fn normalized_encoding(&self) -> SqlFileEncoding {
        self.encoding.unwrap_or_default()
    }

    pub fn normalized_compression(&self) -> SqlFileCompression {
        self.compression.unwrap_or_default()
    }
}

fn normalize_sql_identifier(
    field: &str,
    value: Option<String>,
) -> Result<Option<String>, TransferError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return Ok(None);
    };
    let valid_first = first == '_' || first.is_ascii_alphabetic();
    let valid_rest = chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric());
    if !valid_first || !valid_rest {
        return Err(TransferError::validation(format!(
            "{field} must be one SQL identifier segment (letters, digits, '_' or '$'); dotted or quoted qualifiers are not allowed"
        )));
    }
    Ok(Some(value.to_string()))
}

impl Endpoint {
    #[allow(dead_code)]
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
    /// Native DDL type on the target (e.g. `VARCHAR(255)`); cross-dialect create-new only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_native_type: Option<String>,
}

/// A bounded source recordset selected by deterministic source key order.
///
/// `order_by`/`start`/`end` retain the original scalar IPC/profile form.
/// `tuple_range` is the unambiguous extension for complete composite primary
/// keys. Bounds are JSON on the IPC boundary so the server can convert every
/// component using inspected source column types before binding them. This is
/// a selection scope, never a resumable checkpoint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransferRecordsetBound {
    pub value: serde_json::Value,
    #[serde(default = "default_true")]
    pub inclusive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransferRecordset {
    /// Legacy scalar source column. Omitted only when a tuple range is used or
    /// when a single effective primary-key column supplies the default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<TransferRecordsetBound>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<TransferRecordsetBound>,
    /// Complete, ordered composite primary-key range. Its presence is distinct
    /// from the legacy scalar shape, which keeps old profiles byte-for-byte
    /// stable when deserialized and serialized again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tuple_range: Option<TransferRecordsetTupleRange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransferRecordsetTupleRange {
    /// Must exactly match the complete source primary key in declared order.
    pub columns: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<TransferRecordsetTupleBound>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<TransferRecordsetTupleBound>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransferRecordsetTupleBound {
    /// One typed scalar value for each key column, preserving IPC precision.
    pub values: Vec<serde_json::Value>,
    #[serde(default = "default_true")]
    pub inclusive: bool,
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
    /// When set, structure phase executes this SQL instead of auto-generated CREATE TABLE.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ddl_override: Option<String>,
    /// Optional structured predicate applied to source rows during data copy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_filter: Option<SourceFilter>,
    /// Optional deterministic source recordset selection. This does not
    /// represent a restart checkpoint or persisted OFFSET.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recordset: Option<TransferRecordset>,
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
            ddl_override: None,
            source_filter: None,
            recordset: None,
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
    /// Explicitly use target-default character semantics when source
    /// collations cannot be represented by the target database.
    #[serde(default)]
    pub use_target_default_collation: bool,
}

/// Upper bound for both a source keyset page and its target INSERT. Keeping
/// this at the API boundary prevents a caller from turning a bounded query
/// into an arbitrarily large materialized result or parameter batch.
pub const MAX_TRANSFER_BATCH_SIZE: u32 = 500;

impl Default for TransferOptions {
    fn default() -> Self {
        Self {
            batch_size: 500,
            stop_on_error: true,
            confirmed_destructive: false,
            use_target_default_collation: false,
        }
    }
}

impl TransferOptions {
    pub fn validate(&self) -> Result<(), TransferError> {
        if self.batch_size == 0 || self.batch_size > MAX_TRANSFER_BATCH_SIZE {
            return Err(TransferError::validation(format!(
                "batchSize must be between 1 and {MAX_TRANSFER_BATCH_SIZE}"
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TransferJob {
    pub source: Endpoint,
    /// Database target for the original transfer flow. SQL-file transfers set
    /// this to `None` and provide `sql_file_target` instead.
    #[serde(default)]
    pub target: Option<Endpoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sql_file_target: Option<SqlFileTarget>,
    pub mode: TransferMode,
    pub write_mode: WriteMode,
    pub tables: Vec<TableMapping>,
    pub options: TransferOptions,
}

impl TransferJob {
    pub fn validate_destination(&self) -> Result<(), TransferError> {
        match (&self.target, &self.sql_file_target) {
            (Some(_), None) | (None, Some(_)) => Ok(()),
            (Some(_), Some(_)) => Err(TransferError::validation(
                "transfer must choose either a database target or a SQL file target",
            )),
            (None, None) => Err(TransferError::validation(
                "transfer requires a database target or a SQL file target",
            )),
        }
    }

    pub fn database_target(&self) -> Result<&Endpoint, TransferError> {
        self.target.as_ref().ok_or_else(|| {
            TransferError::validation("transfer job does not have a database target")
        })
    }
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
    pub source_primary_keys: Vec<String>,
    #[serde(default)]
    pub target_columns: Vec<String>,
    #[serde(default)]
    pub source_column_types: HashMap<String, String>,
    /// Native target column types captured during inspection. This is used by
    /// Data Transfer preflight to catch narrowing before the first write.
    #[serde(default)]
    pub target_column_types: HashMap<String, TransferTargetColumnType>,
    pub incompatible_reason: Option<String>,
    pub source_row_count: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recordset: Option<TransferRecordset>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct TransferTargetColumnType {
    pub native_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub character_set: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collation: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum DdlPreviewKind {
    #[default]
    Table,
    Index,
    ForeignKey,
    DropTable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DdlPreviewItem {
    pub source_table: String,
    pub target_table: String,
    pub ddl: String,
    /// The immutable SQL-file structure plan uses this to keep table DDL
    /// ahead of secondary objects and constraints during execution.
    #[serde(default)]
    pub kind: DdlPreviewKind,
    /// Source tables that must be present before this object can be emitted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
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
    /// Parameterized source WHERE preview. Values remain bound server-side.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_filter_preview: Option<String>,
    /// Parameterized ORDER BY/bounds/LIMIT reviewed for this source table.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recordset_preview: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TransferPreview {
    /// Opaque server-side plan token. A preview produced by the command layer
    /// always contains one; pure preview builders leave it empty until the
    /// command has captured the immutable execution snapshot.
    pub plan_id: String,
    pub pairing_path: String,
    pub mode: TransferMode,
    pub write_mode: WriteMode,
    pub ddl: Vec<DdlPreviewItem>,
    pub write_plans: Vec<WritePlanItem>,
    pub warnings: Vec<String>,
    pub can_execute: bool,
    pub block_reason: Option<String>,
}

/// The only mutable choices accepted after a preview has produced a plan.
/// Table names refer to source tables already present in the plan; callers
/// cannot replace mappings, endpoints, DDL or row payloads.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransferRunSelection {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_tables: Option<Vec<String>>,
}

/// Run-time controls that are safe to choose at the final confirmation step.
/// Batch/error policy is intentionally fixed in the immutable preview plan.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransferRunOptions {
    #[serde(default)]
    pub confirmed_destructive: bool,
}

/// Execute a previously previewed Transfer plan.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransferRunRequest {
    pub plan_id: String,
    #[serde(default)]
    pub selection: TransferRunSelection,
    #[serde(default)]
    pub options: TransferRunOptions,
    /// Optional cancellation token. This is a job registry key, not an
    /// alternate execution payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    /// Opaque server-owned table-boundary checkpoint from a cancelled or
    /// partial database transfer. It never contains a row offset or payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_token: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TableExecutionOutcome {
    Committed,
    RolledBack,
    /// A destructive DDL preamble is confirmed, but the row transaction did
    /// not commit. For example, a truncate succeeded before a later rollback.
    PartiallyApplied,
    NotStarted,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TableExecutionResult {
    pub source_table: String,
    pub target_table: String,
    /// `None` means the server cannot prove how many rows reached the target.
    pub rows_inserted: Option<u64>,
    pub success: bool,
    pub error: Option<String>,
    /// Database transfers report transaction state. SQL-file output has no
    /// database transaction and leaves this absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<TableExecutionOutcome>,
}

impl TableExecutionResult {
    pub fn database(
        source_table: impl Into<String>,
        target_table: impl Into<String>,
        rows_inserted: Option<u64>,
        outcome: TableExecutionOutcome,
        error: Option<String>,
    ) -> Self {
        Self {
            source_table: source_table.into(),
            target_table: target_table.into(),
            rows_inserted,
            success: outcome == TableExecutionOutcome::Committed,
            error,
            outcome: Some(outcome),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct TransferExecutionResult {
    pub tables: Vec<TableExecutionResult>,
    pub rows_inserted: u64,
    pub cancelled: bool,
    pub partial: bool,
    /// Present only when a bounded database transfer can be resumed safely.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TransferPairingView {
    pub path: String,
    pub supported: bool,
    pub family: Option<String>,
    pub reason: Option<String>,
}
