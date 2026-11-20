//! T390: `windows/data-sync/clientSqlPreview.ts` ported to Rune.
//!
//! The client-side SQL preview projections: the statement filter by
//! operation, the preview text join, and the no-wire fallback that renders
//! the reviewed rows as comment lines.
//!
//! # Cross-segment dependency note
//!
//! React imports `operationAllowed` from `mappingView.ts` — that file's
//! own port is **T419 (another machine, not landed)**. Following the
//! `table_data_dirty_index` / T561 precedent, the one pure function this
//! module needs is carried here transliterated line for line. Do not
//! extend it: when T419 lands, this module should call that one and drop
//! the copy.
//!
//! # `formatCell` keeps its own port
//!
//! `lib/formatters.ts`'s `formatCell` already has the crate-side mirror
//! (`support::formatters::format_cell`), so the preview uses that one
//! directly.
//!
//! # Documented deviations from React
//!
//! * `table.rows ?? []` is the plain `Vec` mirror T024 carries.
//! * The `'…'` (U+2026) fallback of an empty changed-columns list is the
//!   same single character the React template literal embeds.

// The production consumers are the Data Sync preview step (T226) and its
// SqlPreview (T371); until they land, the unit tests are the only callers.
#![allow(dead_code)] // wired by the Data Sync preview step (T226/T371)

use crate::support::formatters::format_cell;
use crate::views::{DataSyncOperation, DataSyncRowChange, DataSyncTableResult, SyncOptions};

/// React `operationAllowed` (`mappingView.ts:155-166`), the subset this
/// module needs. T419 (its real home) replaces this copy when it lands.
fn operation_allowed(operation: DataSyncOperation, options: &SyncOptions) -> bool {
    match operation {
        DataSyncOperation::Insert => options.insert,
        DataSyncOperation::Update => options.update,
        DataSyncOperation::Delete => options.delete,
        DataSyncOperation::Unchanged => false,
    }
}

/// React `DataSyncSqlStatement` (`backend-client/types/dataSync.ts:312`,
/// aliased in `commands/sync.ts`): one generated statement of the
/// reviewed selection (the SQL preview face).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DataSyncSqlStatement {
    pub table: String,
    pub operation: DataSyncOperation,
    pub sql: String,
    pub preview_sql: String,
    pub parameters: Vec<serde_json::Value>,
    pub row_key: Vec<serde_json::Value>,
}

/// The operation filter of the preview toolbar: all, or one operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum OpFilter {
    #[default]
    All,
    Op(DataSyncOperation),
}

/// React `filterStatementsByOp`: the filter itself.
pub(crate) fn filter_statements_by_op(
    statements: &[DataSyncSqlStatement],
    op_filter: OpFilter,
) -> Vec<DataSyncSqlStatement> {
    match op_filter {
        OpFilter::All => statements.to_vec(),
        OpFilter::Op(op) => statements
            .iter()
            .filter(|statement| statement.operation == op)
            .cloned()
            .collect(),
    }
}

/// React `statementsToPreviewText`: the filtered statements joined on
/// newlines, `previewSql || sql` per statement, with the empty-filter
/// placeholder.
pub(crate) fn statements_to_preview_text(
    statements: &[DataSyncSqlStatement],
    op_filter: OpFilter,
) -> String {
    let filtered = filter_statements_by_op(statements, op_filter);
    if filtered.is_empty() {
        return "-- (no statements)".to_owned();
    }
    filtered
        .iter()
        .map(|statement| {
            if statement.preview_sql.is_empty() {
                statement.sql.clone()
            } else {
                statement.preview_sql.clone()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// React `buildClientSqlPreview`: the no-wire fallback — one comment
/// line per selected, allowed, non-unchanged row of every matched table.
pub(crate) fn build_client_sql_preview(
    tables: &[DataSyncTableResult],
    options: &SyncOptions,
) -> String {
    let mut lines: Vec<String> = Vec::new();
    for table in tables {
        if table.status != crate::views::DataSyncMappingStatus::Matched {
            continue;
        }
        let target = if table.target_table.is_empty() {
            table.source_table.as_str()
        } else {
            table.target_table.as_str()
        };
        for row in &table.rows {
            if !row.selected || row.operation == DataSyncOperation::Unchanged {
                continue;
            }
            if !operation_allowed(row.operation, options) {
                continue;
            }
            let pk = row
                .key
                .iter()
                .map(format_cell)
                .collect::<Vec<_>>()
                .join(", ");
            match row.operation {
                DataSyncOperation::Insert => {
                    lines.push(format!("-- INSERT INTO {target} (pk: {pk})"));
                }
                DataSyncOperation::Update => {
                    let cols = if row.changed_columns.is_empty() {
                        "…".to_owned()
                    } else {
                        row.changed_columns.join(", ")
                    };
                    lines.push(format!("-- UPDATE {target} SET {cols} WHERE pk IN ({pk})"));
                }
                DataSyncOperation::Delete => {
                    lines.push(format!("-- DELETE FROM {target} WHERE pk IN ({pk})"));
                }
                DataSyncOperation::Unchanged => {}
            }
        }
    }
    if lines.is_empty() {
        "-- (no selected changes)".to_owned()
    } else {
        lines.join("\n")
    }
}

/// React's `row.selected` reads in this file walk `DataSyncRowChange`;
/// the re-export keeps the helper's signature honest for future callers.
pub(crate) type PreviewRow = DataSyncRowChange;
