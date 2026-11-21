//! Port of `packages/frontend/src/commands/transfer.ts` (T415) — the
//! data-transfer legacy command family and its typed wire contracts.
//!
//! Every payload is key-for-key with the React wire (JS
//! `undefined`-valued keys drop under `JSON.stringify`, so "absent"
//! is the wire form — the `?`-optional fields stay `Option` with
//! `skip_serializing_if`, and the explicit `?? null` call sites spell
//! `null`).
//!
//! ## What maps where
//!
//! - The `invoke<T>(command, payload)` wrappers are
//!   [`TransferMethod`] + the `*_request` shapers (the desktop
//!   adapter's `execute_driver_command` rail — the commands/`
//!   convention).
//! - The typed contracts (TransferMode / WriteMode / the mapping /
//!   recordset / preview / execution families) are the serde VMs,
//!   camelCase on the wire exactly as the React interfaces spell.
//! - `FilterCondition` (`types/filters.ts`) is the filter chip's own
//!   contract — the transfer wire carries it opaquely
//!   (`TransferSourceFilter::filters` as JSON values), the same
//!   opaque-JSON ruling the settings pass-throughs carry.
//! - `execute`'s `VITE_E2E` capture window
//!   (`__dataTransferRunCalls`) is the E2E harness's global — the
//!   Rust port has no such global; the E2E capture rides the host
//!   (the adapter observes the calls), documented as the delta.
//! - `DEFAULT_TRANSFER_OPTIONS` is the literal, verbatim.
//!
//! Known deltas: the E2E run-capture global (above); `schema?:
//! string | null` optionals serialize as absent-when-`None` except
//! where the React call sites spell an explicit `null`.

// No production consumer yet (the data-transfer window track wires
// the faces); this crate's tests are the callers meanwhile.
#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// `TransferMode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum TransferMode {
    Structure,
    Data,
    StructureAndData,
}

/// `WriteMode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum WriteMode {
    Insert,
    TruncateInsert,
    DropCreateInsert,
}

/// `TransferMappingStatus`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum TransferMappingStatus {
    #[serde(rename = "MATCHED")]
    Matched,
    #[serde(rename = "CREATE_NEW")]
    CreateNew,
    #[serde(rename = "UNMAPPED_SOURCE")]
    UnmappedSource,
    #[serde(rename = "UNMAPPED_TARGET")]
    UnmappedTarget,
    #[serde(rename = "DISABLED")]
    Disabled,
    #[serde(rename = "INCOMPATIBLE")]
    Incompatible,
}

/// `TransferEndpoint`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TransferEndpoint {
    pub db_session_id: String,
    pub database: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
}

/// `TransferColumnMapping`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TransferColumnMapping {
    pub source_column: String,
    pub target_column: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skip: Option<bool>,
    /// Native target DDL type override (cross-dialect create-new).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_native_type: Option<String>,
}

/// `TransferSourceFilter` — the structured, parameterized source row
/// filter (`filters` carries the `FilterCondition` wire opaquely).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TransferSourceFilter {
    pub filters: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logic: Option<String>,
}

/// `TransferRecordsetBound`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TransferRecordsetBound {
    /// Text stays lossless on the IPC boundary; the source driver
    /// binds it using the inspected column type.
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inclusive: Option<bool>,
}

/// `TransferRecordsetTupleBound`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TransferRecordsetTupleBound {
    pub values: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inclusive: Option<bool>,
}

/// `TransferRecordsetTupleRange`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TransferRecordsetTupleRange {
    pub columns: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<TransferRecordsetTupleBound>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<TransferRecordsetTupleBound>,
}

/// `TransferRecordset` — a stable source recordset selection; never
/// a restart checkpoint.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TransferRecordset {
    /// Legacy scalar source column, retained for existing IPC and
    /// profiles.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<TransferRecordsetBound>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<TransferRecordsetBound>,
    /// Complete source primary-key tuple, in the declared key order.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tuple_range: Option<TransferRecordsetTupleRange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
}

/// `TransferTableMapping`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TransferTableMapping {
    pub source_table: String,
    pub target_table: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_new: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column_mappings: Option<Vec<TransferColumnMapping>>,
    /// Execute this CREATE instead of auto-generated DDL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ddl_override: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_filter: Option<TransferSourceFilter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recordset: Option<TransferRecordset>,
}

/// `TransferOptions`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TransferOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch_size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_on_error: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirmed_destructive: Option<bool>,
    /// Explicitly use target-default collation when source text
    /// semantics cannot be preserved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_target_default_collation: Option<bool>,
}

/// `DEFAULT_TRANSFER_OPTIONS`, the literal verbatim.
pub(crate) const DEFAULT_TRANSFER_OPTIONS: TransferOptions = TransferOptions {
    batch_size: Some(500),
    stop_on_error: Some(true),
    confirmed_destructive: Some(false),
    use_target_default_collation: Some(false),
};

/// The SQL artifact's text encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum TransferSqlEncoding {
    #[serde(rename = "utf8")]
    Utf8,
    #[serde(rename = "utf8Bom")]
    Utf8Bom,
    #[serde(rename = "utf16Le")]
    Utf16Le,
    #[serde(rename = "utf16Be")]
    Utf16Be,
}

/// The SQL artifact's compression.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum TransferSqlCompression {
    #[serde(rename = "none")]
    None,
    #[serde(rename = "gzip")]
    Gzip,
}

/// `TransferSqlFileTarget` — the opaque host token the native SQL
/// save dialog returns.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TransferSqlFileTarget {
    pub file_token: String,
    /// Registered SQL driver used to render the artifact; omitted
    /// means source dialect.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database_type: Option<String>,
    /// Output text encoding; omitted means UTF-8 without a BOM.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoding: Option<TransferSqlEncoding>,
    /// Output compression; omitted means an uncompressed .sql
    /// artifact.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compression: Option<TransferSqlCompression>,
    /// Optional target catalog/database qualifier for the generated
    /// SQL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    /// Optional target schema qualifier for the generated SQL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
}

/// `TransferJob`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TransferJob {
    pub source: TransferEndpoint,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<TransferEndpoint>,
    /// Opaque host token returned by the native SQL save dialog.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sql_file_target: Option<TransferSqlFileTarget>,
    pub mode: TransferMode,
    pub write_mode: WriteMode,
    pub tables: Vec<TransferTableMapping>,
    pub options: TransferOptions,
}

/// `TransferProfile`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TransferProfile {
    pub version: u32,
    pub id: String,
    pub name: String,
    pub source_connection_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_connection_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_database: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_database: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_schema: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_schema: Option<String>,
    /// `'database' | 'sqlFile'`.
    pub destination_mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sql_file_dialect: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sql_file_encoding: Option<TransferSqlEncoding>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sql_file_compression: Option<TransferSqlCompression>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sql_file_database: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sql_file_schema: Option<String>,
    pub mode: TransferMode,
    pub write_mode: WriteMode,
    pub tables: Vec<TransferTableMapping>,
    pub options: TransferOptions,
    pub created_at: String,
    pub updated_at: String,
}

/// `TransferTableResult`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TransferTableResult {
    pub source_table: String,
    pub target_table: String,
    pub status: TransferMappingStatus,
    pub create_new: bool,
    pub enabled: bool,
    pub column_mappings: Vec<TransferColumnMapping>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_columns: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_primary_keys: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_columns: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_column_types: Option<serde_json::Map<String, Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ddl_override: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub incompatible_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_row_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_filter: Option<TransferSourceFilter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recordset: Option<TransferRecordset>,
}

/// `TransferDdlPreview`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TransferDdlPreview {
    pub source_table: String,
    pub target_table: String,
    pub ddl: String,
    /// `'table' | 'index' | 'foreignKey' | 'dropTable'`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depends_on: Option<Vec<String>>,
}

/// `TransferWritePlan`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TransferWritePlan {
    pub source_table: String,
    pub target_table: String,
    pub write_mode: WriteMode,
    pub mapped_columns: Vec<TransferColumnMapping>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_rows: Option<u64>,
    pub preamble: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_filter_preview: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recordset_preview: Option<String>,
}

/// `TransferPreview`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TransferPreview {
    pub plan_id: String,
    pub pairing_path: String,
    pub mode: TransferMode,
    pub write_mode: WriteMode,
    pub ddl: Vec<TransferDdlPreview>,
    pub write_plans: Vec<TransferWritePlan>,
    pub warnings: Vec<String>,
    pub can_execute: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_reason: Option<String>,
}

/// `TransferTableExecution`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TransferTableExecution {
    pub source_table: String,
    pub target_table: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rows_inserted: Option<u64>,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// `'committed' | 'rolledBack' | 'partiallyApplied' |
    /// 'notStarted' | 'unknown' | null`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
}

/// `TransferExecutionResult`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TransferExecutionResult {
    pub tables: Vec<TransferTableExecution>,
    pub rows_inserted: u64,
    pub cancelled: bool,
    pub partial: bool,
    /// Opaque table-boundary token for a safe database-only resume.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resume_token: Option<String>,
}

/// `TransferRunSelection`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TransferRunSelection {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_tables: Option<Vec<String>>,
}

/// `TransferRunOptions`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TransferRunOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirmed_destructive: Option<bool>,
}

/// `TransferRunRequest`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TransferRunRequest {
    pub plan_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selection: Option<TransferRunSelection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<TransferRunOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resume_token: Option<String>,
}

/// `TransferPairingView`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TransferPairingView {
    pub path: String,
    pub supported: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// The legacy `invoke` commands (the desktop rail).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TransferMethod {
    GetProfiles,
    SaveProfile,
    DeleteProfile,
    PickSqlFile,
    ClassifyPair,
    Inspect,
    InspectSqlFile,
    Preview,
    Execute,
    Cancel,
}

impl TransferMethod {
    /// The legacy command name.
    pub(crate) fn wire_name(self) -> &'static str {
        match self {
            Self::GetProfiles => "get_transfer_profiles",
            Self::SaveProfile => "save_transfer_profile",
            Self::DeleteProfile => "delete_transfer_profile",
            Self::PickSqlFile => "pick_data_transfer_sql_file",
            Self::ClassifyPair => "classify_transfer_pair",
            Self::Inspect => "inspect_data_transfer",
            Self::InspectSqlFile => "inspect_sql_file_transfer",
            Self::Preview => "preview_data_transfer",
            Self::Execute => "execute_data_transfer",
            Self::Cancel => "cancel_data_transfer",
        }
    }
}

/// `getProfiles()`.
pub(crate) fn get_profiles_request() -> (TransferMethod, Value) {
    (TransferMethod::GetProfiles, json!({}))
}

/// `saveProfile(profile)`.
pub(crate) fn save_profile_request(profile: &TransferProfile) -> (TransferMethod, Value) {
    (
        TransferMethod::SaveProfile,
        json!({ "profile": profile }),
    )
}

/// `deleteProfile(profileId)`.
pub(crate) fn delete_profile_request(profile_id: &str) -> (TransferMethod, Value) {
    (
        TransferMethod::DeleteProfile,
        json!({ "profileId": profile_id }),
    )
}

/// `pickSqlFile()`.
pub(crate) fn pick_sql_file_request() -> (TransferMethod, Value) {
    (TransferMethod::PickSqlFile, json!({}))
}

/// `classifyPair(sourceDatabaseType, targetDatabaseType)`.
pub(crate) fn classify_pair_request(
    source_database_type: &str,
    target_database_type: &str,
) -> (TransferMethod, Value) {
    (
        TransferMethod::ClassifyPair,
        json!({
            "sourceDatabaseType": source_database_type,
            "targetDatabaseType": target_database_type,
        }),
    )
}

/// `inspect(…)`: the `?? null` spellings are the wire's.
pub(crate) fn inspect_request(
    source_db_session_id: &str,
    target_db_session_id: &str,
    mode: TransferMode,
    source_database: Option<&str>,
    target_database: Option<&str>,
    tables: Option<&[TransferTableMapping]>,
) -> (TransferMethod, Value) {
    (
        TransferMethod::Inspect,
        json!({
            "sourceDbSessionId": source_db_session_id,
            "targetDbSessionId": target_db_session_id,
            "sourceDatabase": source_database,
            "targetDatabase": target_database,
            "mode": mode,
            "tables": tables,
        }),
    )
}

/// `inspectSqlFile(…)`.
pub(crate) fn inspect_sql_file_request(
    source_db_session_id: &str,
    mode: TransferMode,
    source_database: Option<&str>,
    source_schema: Option<&str>,
    target_database_type: Option<&str>,
    tables: Option<&[TransferTableMapping]>,
) -> (TransferMethod, Value) {
    (
        TransferMethod::InspectSqlFile,
        json!({
            "sourceDbSessionId": source_db_session_id,
            "sourceDatabase": source_database,
            "sourceSchema": source_schema,
            "targetDatabaseType": target_database_type,
            "mode": mode,
            "tables": tables,
        }),
    )
}

/// `preview(job)`.
pub(crate) fn preview_request(job: &TransferJob) -> (TransferMethod, Value) {
    (TransferMethod::Preview, json!({ "job": job }))
}

/// `execute(request, profile?)` — the profile spread is an ABSENT key
/// without one (`{ request, ...(profile ? { profile } : {}) }`).
pub(crate) fn execute_request(
    request: &TransferRunRequest,
    profile: Option<&Value>,
) -> (TransferMethod, Value) {
    let mut payload = json!({ "request": request });
    if let Some(profile) = profile {
        payload["profile"] = profile.clone();
    }
    (TransferMethod::Execute, payload)
}

/// `cancel(jobId)`.
pub(crate) fn cancel_request(job_id: &str) -> (TransferMethod, Value) {
    (TransferMethod::Cancel, json!({ "jobId": job_id }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wire_names_are_the_legacy_commands() {
        assert_eq!(TransferMethod::GetProfiles.wire_name(), "get_transfer_profiles");
        assert_eq!(TransferMethod::PickSqlFile.wire_name(), "pick_data_transfer_sql_file");
        assert_eq!(TransferMethod::ClassifyPair.wire_name(), "classify_transfer_pair");
        assert_eq!(TransferMethod::Inspect.wire_name(), "inspect_data_transfer");
        assert_eq!(TransferMethod::InspectSqlFile.wire_name(), "inspect_sql_file_transfer");
        assert_eq!(TransferMethod::Preview.wire_name(), "preview_data_transfer");
        assert_eq!(TransferMethod::Execute.wire_name(), "execute_data_transfer");
        assert_eq!(TransferMethod::Cancel.wire_name(), "cancel_data_transfer");
    }

    #[test]
    fn default_options_are_the_react_literal() {
        assert_eq!(DEFAULT_TRANSFER_OPTIONS.batch_size, Some(500));
        assert_eq!(DEFAULT_TRANSFER_OPTIONS.stop_on_error, Some(true));
        assert_eq!(DEFAULT_TRANSFER_OPTIONS.confirmed_destructive, Some(false));
        assert_eq!(
            DEFAULT_TRANSFER_OPTIONS.use_target_default_collation,
            Some(false)
        );
        // The wire spelling.
        let wire = serde_json::to_value(DEFAULT_TRANSFER_OPTIONS).unwrap();
        assert_eq!(wire["batchSize"], 500);
        assert_eq!(wire["stopOnError"], true);
        assert_eq!(wire["confirmedDestructive"], false);
        assert_eq!(wire["useTargetDefaultCollation"], false);
    }

    #[test]
    fn the_enums_spell_their_wire_literals() {
        assert_eq!(
            serde_json::to_value(TransferMode::StructureAndData).unwrap(),
            "structureAndData"
        );
        assert_eq!(
            serde_json::to_value(WriteMode::TruncateInsert).unwrap(),
            "truncateInsert"
        );
        assert_eq!(
            serde_json::to_value(TransferMappingStatus::UnmappedTarget).unwrap(),
            "UNMAPPED_TARGET"
        );
        assert_eq!(
            serde_json::to_value(TransferSqlEncoding::Utf16Le).unwrap(),
            "utf16Le"
        );
        assert_eq!(
            serde_json::to_value(TransferSqlCompression::Gzip).unwrap(),
            "gzip"
        );
    }

    #[test]
    fn the_explicit_null_call_sites_spell_null() {
        let (_, payload) = inspect_request(
            "s1",
            "t1",
            TransferMode::Data,
            None,
            None,
            None,
        );
        assert_eq!(payload["sourceDatabase"], Value::Null);
        assert_eq!(payload["targetDatabase"], Value::Null);
        assert_eq!(payload["tables"], Value::Null);
        assert_eq!(payload["mode"], "data");

        let (_, payload) = inspect_sql_file_request(
            "s1",
            TransferMode::Structure,
            None,
            None,
            None,
            None,
        );
        assert_eq!(payload["sourceSchema"], Value::Null);
        assert_eq!(payload["targetDatabaseType"], Value::Null);
        // The SQL-file inspect has NO target session key at all.
        assert!(payload.get("targetDbSessionId").is_none());
    }

    #[test]
    fn the_execute_profile_spread_is_absent_without_one() {
        let request = TransferRunRequest {
            plan_id: "plan-1".to_owned(),
            selection: None,
            options: None,
            job_id: Some("job-2".to_owned()),
            resume_token: None,
        };
        let (_, with) = execute_request(&request, Some(&json!({ "id": "p", "revision": "r" })));
        assert_eq!(with["profile"]["id"], "p");
        assert_eq!(with["request"]["planId"], "plan-1");
        assert_eq!(with["request"]["jobId"], "job-2");
        let (_, without) = execute_request(&request, None);
        assert!(without.get("profile").is_none());
    }

    #[test]
    fn optionals_stay_absent_on_the_wire() {
        // A table mapping with only the required keys.
        let mapping = TransferTableMapping {
            source_table: "s".to_owned(),
            target_table: "t".to_owned(),
            create_new: None,
            enabled: None,
            column_mappings: None,
            ddl_override: None,
            source_filter: None,
            recordset: None,
        };
        let wire = serde_json::to_value(&mapping).unwrap();
        assert_eq!(wire["sourceTable"], "s");
        assert!(wire.get("createNew").is_none(), "absent, not null");
        assert!(wire.get("columnMappings").is_none());
        // A recordset's tuple range serializes its camelCase keys.
        let recordset = TransferRecordset {
            order_by: Some("id".to_owned()),
            start: None,
            end: None,
            tuple_range: Some(TransferRecordsetTupleRange {
                columns: vec!["a".to_owned(), "b".to_owned()],
                start: Some(TransferRecordsetTupleBound {
                    values: vec!["1".to_owned(), "2".to_owned()],
                    inclusive: Some(true),
                }),
                end: None,
            }),
            limit: Some(10),
        };
        let wire = serde_json::to_value(&recordset).unwrap();
        assert_eq!(wire["orderBy"], "id");
        assert_eq!(wire["tupleRange"]["columns"][0], "a");
        assert_eq!(wire["tupleRange"]["start"]["inclusive"], true);
        assert_eq!(wire["limit"], 10);
    }

    #[test]
    fn the_shapers_carry_their_keys() {
        let (_, payload) = delete_profile_request("p1");
        assert_eq!(payload["profileId"], "p1");
        let (_, payload) = classify_pair_request("mysql", "postgres");
        assert_eq!(payload["sourceDatabaseType"], "mysql");
        assert_eq!(payload["targetDatabaseType"], "postgres");
        let (_, payload) = cancel_request("job-9");
        assert_eq!(payload["jobId"], "job-9");
    }
}
