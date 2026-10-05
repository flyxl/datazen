//! Batch INSERT execute path (same-family and IR).

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
#[cfg(any(test, all(debug_assertions, feature = "webdriver")))]
use std::sync::Mutex;

use datazen_driver_api::TableSchema;

use datazen_data_sync::sql::{qualify_relation_sql, quote_ident_sql};
use datazen_driver_api::{ConnectionHandle, DatabaseDriver, Value};
use crate::transfer::adapter::SyncTargetAdapter;
use crate::transfer::ir::IRType;

use super::error::TransferError;
use super::model::{
    ColumnMapping, TableExecutionOutcome, TableExecutionResult, TableInspectResult,
    TransferExecutionResult, TransferJob, TransferMode, WriteMode,
};
use super::structure::{drop_and_recreate_table, table_eligible_for_data};

/// One-shot fault seam used only by unit tests and debug webdriver builds.
/// It is keyed to one exact target table and is consumed only after the real
/// driver commit call has returned success.
#[cfg(any(test, all(debug_assertions, feature = "webdriver")))]
static TEST_COMMIT_ACK_LOSS_TABLE: Mutex<Option<String>> = Mutex::new(None);

#[cfg(any(test, all(debug_assertions, feature = "webdriver")))]
pub fn arm_test_commit_ack_loss(target_table: &str) -> Result<(), String> {
    let target_table = target_table.trim();
    if target_table.is_empty() {
        return Err("target table must be non-empty".into());
    }
    let mut armed = TEST_COMMIT_ACK_LOSS_TABLE
        .lock()
        .map_err(|_| "Data Transfer test fault state is unavailable".to_string())?;
    if armed.is_some() {
        return Err("a Data Transfer commit acknowledgement loss is already armed".into());
    }
    *armed = Some(target_table.to_string());
    Ok(())
}

#[cfg(any(test, all(debug_assertions, feature = "webdriver")))]
pub fn clear_test_commit_ack_loss() -> bool {
    TEST_COMMIT_ACK_LOSS_TABLE
        .lock()
        .map(|mut armed| armed.take().is_some())
        .unwrap_or(false)
}

#[cfg(any(test, all(debug_assertions, feature = "webdriver")))]
pub fn consume_test_commit_ack_loss(target_table: &str) -> bool {
    let Ok(mut armed) = TEST_COMMIT_ACK_LOSS_TABLE.lock() else {
        return false;
    };
    if armed.as_deref() == Some(target_table) {
        armed.take();
        true
    } else {
        false
    }
}

pub struct DropCreateContext<'a> {
    pub src_adapter: &'a dyn crate::transfer::adapter::SyncSourceAdapter,
    pub tgt_adapter: &'a dyn SyncTargetAdapter,
    pub src_driver: &'a dyn DatabaseDriver,
    pub src_handle: &'a ConnectionHandle,
    pub tgt_driver: &'a dyn DatabaseDriver,
    pub tgt_handle: &'a ConnectionHandle,
    pub source_schemas: &'a HashMap<String, TableSchema>,
    /// The immutable structure plan already performed the destructive preamble.
    pub structure_precreated: bool,
}

pub enum ValueFormatter<'a> {
    SameFamily,
    Ir {
        tgt_adapter: &'a dyn SyncTargetAdapter,
        source_column_ir_types: &'a HashMap<String, HashMap<String, IRType>>,
    },
}

/// Logical relation identity keeps catalog/database and schema separate. Schema
/// defaults are resolved at the command boundary before execution reaches here.
pub fn is_self_table_overwrite(
    source: &super::model::Endpoint,
    target: &super::model::Endpoint,
    source_table: &str,
    target_table: &str,
) -> bool {
    source.db_session_id == target.db_session_id
        && source.database == target.database
        && source.normalized_schema() == target.normalized_schema()
        && source_table == target_table
}

pub fn validate_no_self_table_overwrite(
    job: &TransferJob,
    inspected: &[TableInspectResult],
) -> Result<(), TransferError> {
    let target = job.database_target()?;
    for table in inspected
        .iter()
        .filter(|table| table_eligible_for_data(table, job))
    {
        if is_self_table_overwrite(
            &job.source,
            target,
            &table.source_table,
            &table.target_table,
        ) {
            return Err(TransferError::validation(format!(
                "self-overwrite of table '{}' is not allowed",
                table.source_table
            )));
        }
    }
    Ok(())
}

pub fn active_column_mappings(mappings: &[ColumnMapping]) -> Vec<&ColumnMapping> {
    mappings.iter().filter(|m| !m.skip).collect()
}

#[allow(dead_code)] // tested; thin wrapper over build_truncate_sql_ref
pub fn build_truncate_sql(table: &str, quote: char) -> String {
    build_truncate_sql_ref(&quote_ident_sql(table, quote))
}

pub fn build_truncate_sql_ref(table_ref: &str) -> String {
    format!("TRUNCATE TABLE {table_ref}")
}

pub fn map_row_values(
    source_row: &[Option<Value>],
    source_schema: &TableSchema,
    columns: &[&ColumnMapping],
) -> Result<Vec<Option<Value>>, TransferError> {
    if source_row.len() != columns.len() {
        return Err(TransferError::validation(format!(
            "projected row has {} values, expected {}",
            source_row.len(),
            columns.len()
        )));
    }
    let mut projected = Vec::with_capacity(columns.len());
    for (value, col) in source_row.iter().zip(columns) {
        let source_column = source_schema
            .columns
            .iter()
            .find(|source| source.name == col.source_column)
            .ok_or_else(|| {
                TransferError::validation(format!(
                    "source column '{}' not found",
                    col.source_column
                ))
            })?;

        // Some MySQL text columns with binary collations are surfaced by the
        // driver as bytes. Preserve their text meaning when the source schema
        // confirms a character type; never reinterpret binary columns this way.
        let value = match value {
            Some(Value::Bytes(bytes)) if is_textual_source_type(&source_column.data_type) => {
                let text = std::str::from_utf8(bytes).map_err(|_| {
                    TransferError::validation(format!(
                        "source text column '{}' contains bytes that are not valid UTF-8",
                        col.source_column
                    ))
                })?;
                Some(Value::String(text.to_owned()))
            }
            value => value.clone(),
        };
        projected.push(value);
    }
    Ok(projected)
}

fn is_textual_source_type(data_type: &str) -> bool {
    let normalized = data_type.trim().to_ascii_lowercase();
    let base = normalized.split('(').next().unwrap_or(&normalized).trim();
    matches!(
        base,
        "char"
            | "character"
            | "varchar"
            | "character varying"
            | "nchar"
            | "nvarchar"
            | "national char"
            | "national character"
            | "national character varying"
            | "text"
            | "tinytext"
            | "mediumtext"
            | "longtext"
            | "citext"
            | "enum"
            | "set"
    )
}

pub async fn execute_transfer_data(
    src_driver: &dyn DatabaseDriver,
    src_handle: &ConnectionHandle,
    tgt_driver: &dyn DatabaseDriver,
    tgt_handle: &ConnectionHandle,
    job: &TransferJob,
    inspected: &[TableInspectResult],
    source_schemas: &HashMap<String, TableSchema>,
    formatter: &ValueFormatter<'_>,
    drop_create: Option<&DropCreateContext<'_>>,
    target_read_only: bool,
    cancelled: Option<Arc<AtomicBool>>,
    completed_tables: Option<&HashSet<String>>,
) -> Result<TransferExecutionResult, TransferError> {
    execute_transfer_data_with_write_observer(
        src_driver,
        src_handle,
        tgt_driver,
        tgt_handle,
        job,
        inspected,
        source_schemas,
        formatter,
        drop_create,
        target_read_only,
        cancelled,
        completed_tables,
        None,
    )
    .await
}

pub async fn execute_transfer_data_with_write_observer(
    src_driver: &dyn DatabaseDriver,
    src_handle: &ConnectionHandle,
    tgt_driver: &dyn DatabaseDriver,
    tgt_handle: &ConnectionHandle,
    job: &TransferJob,
    inspected: &[TableInspectResult],
    source_schemas: &HashMap<String, TableSchema>,
    formatter: &ValueFormatter<'_>,
    drop_create: Option<&DropCreateContext<'_>>,
    target_read_only: bool,
    cancelled: Option<Arc<AtomicBool>>,
    completed_tables: Option<&HashSet<String>>,
    write_started: Option<&AtomicBool>,
) -> Result<TransferExecutionResult, TransferError> {
    execute_transfer_data_with_resume_checkpoint(
        src_driver,
        src_handle,
        tgt_driver,
        tgt_handle,
        job,
        inspected,
        source_schemas,
        formatter,
        drop_create,
        target_read_only,
        cancelled,
        completed_tables,
        write_started,
        None,
    )
    .await
}

mod dispatcher;
pub use dispatcher::execute_transfer_data_with_resume_checkpoint;

pub async fn execute_same_family_data(
    src_driver: &dyn DatabaseDriver,
    src_handle: &ConnectionHandle,
    tgt_driver: &dyn DatabaseDriver,
    tgt_handle: &ConnectionHandle,
    job: &TransferJob,
    inspected: &[TableInspectResult],
    source_schemas: &HashMap<String, TableSchema>,
    target_read_only: bool,
    cancelled: Option<Arc<AtomicBool>>,
) -> Result<TransferExecutionResult, TransferError> {
    let formatter = ValueFormatter::SameFamily;
    execute_transfer_data(
        src_driver,
        src_handle,
        tgt_driver,
        tgt_handle,
        job,
        inspected,
        source_schemas,
        &formatter,
        None,
        target_read_only,
        cancelled,
        None,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_overwrite_detected_by_complete_logical_relation() {
        let mut source = super::super::model::Endpoint {
            db_session_id: "session".into(),
            database: "catalog".into(),
            schema: None,
        };
        let mut target = source.clone();
        assert!(is_self_table_overwrite(&source, &target, "users", "users"));
        assert!(!is_self_table_overwrite(
            &source, &target, "users", "clients"
        ));
        target.schema = Some("  ".into());
        assert!(is_self_table_overwrite(&source, &target, "users", "users"));
        source.schema = Some(" selected ".into());
        target.schema = Some("selected".into());
        assert!(is_self_table_overwrite(&source, &target, "users", "users"));
        target.schema = Some("other".into());
        assert!(!is_self_table_overwrite(&source, &target, "users", "users"));
        target.schema = source.schema.clone();
        target.database = "other_catalog".into();
        assert!(!is_self_table_overwrite(&source, &target, "users", "users"));
        target.database = source.database.clone();
        target.db_session_id = "other_session".into();
        assert!(!is_self_table_overwrite(&source, &target, "users", "users"));
    }

    #[test]
    fn truncate_sql_quotes_table() {
        let sql = build_truncate_sql("users", '"');
        assert_eq!(sql, r#"TRUNCATE TABLE "users""#);
    }

    #[test]
    fn cross_family_source_select_uses_postgres_double_quotes() {
        let cols = vec![ColumnMapping {
            source_column: "id".into(),
            target_column: "id".into(),
            skip: false,
            target_native_type: None,
        }];
        let refs: Vec<&ColumnMapping> = cols.iter().collect();
        let src_quote = '"';
        let select_cols: Vec<String> = refs
            .iter()
            .map(|c| quote_ident_sql(&c.source_column, src_quote))
            .collect();
        let src_table_ref =
            qualify_relation_sql("postgresql", Some("goecoride"), None, "users", src_quote);
        let sql = format!("SELECT {} FROM {}", select_cols.join(", "), src_table_ref);
        assert_eq!(sql, r#"SELECT "id" FROM "users""#);
    }

    #[test]
    fn mysql_transfer_uses_catalog_qualified_table_refs() {
        let src_ref = qualify_relation_sql("mysql", Some("srcdb"), None, "users", '`');
        let tgt_ref = qualify_relation_sql("mysql", Some("tgtdb"), None, "users", '`');
        assert_eq!(src_ref, "`srcdb`.`users`");
        assert_eq!(tgt_ref, "`tgtdb`.`users`");
        assert_eq!(
            build_truncate_sql_ref(&tgt_ref),
            "TRUNCATE TABLE `tgtdb`.`users`"
        );
    }
}
