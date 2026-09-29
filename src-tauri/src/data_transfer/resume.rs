//! Bounded keyset pages and checkpoint-safe execution for Data Transfer.
//!
//! This path is deliberately narrower than ordinary Transfer: both database
//! drivers must provide read snapshots, both relations must advertise a
//! consistent snapshot, and the source must have a complete, ordered PK.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use datazen_driver_api::TableSchema;

use crate::data_sync::sql::quote_ident_sql;
use crate::db::{ConnectionHandle, DatabaseDriver, TransactionHandle, Value};

use super::error::TransferError;
use super::execute::{map_row_values, ValueFormatter};
use super::model::{
    ColumnMapping, TableExecutionOutcome, TableExecutionResult, TableInspectResult, TransferJob,
    MAX_TRANSFER_BATCH_SIZE,
};
use super::recordset::{resolve_recordset, SourceScope};

const MAX_BOUND_QUERY_PARAMETERS: usize = 60_000;

fn confirmed_chunk_outcome(scope_finished: bool) -> TableExecutionOutcome {
    if scope_finished {
        TableExecutionOutcome::Committed
    } else {
        TableExecutionOutcome::PartiallyApplied
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ResumeTableProgress {
    pub(crate) source_table: String,
    pub(crate) target_table: String,
    pub(crate) key_columns: Vec<String>,
    pub(crate) chunk_size: u32,
    pub(crate) source_fingerprint: String,
    pub(crate) cursor: Option<Vec<Value>>,
    pub(crate) rows_seen: u64,
}

/// A synchronous server-side bridge to the opaque checkpoint store. Methods
/// are called only after source reads or an acknowledged target commit.
pub(crate) trait TransferResumeCheckpoint: Send {
    fn renew(&mut self) -> Result<(), TransferError>;

    fn prepare_table(
        &mut self,
        source_table: &str,
        target_table: &str,
        key_columns: &[String],
        chunk_size: u32,
        source_fingerprint: &str,
    ) -> Result<ResumeTableProgress, TransferError>;

    fn advance_table(
        &mut self,
        source_table: &str,
        cursor: Vec<Value>,
        rows_seen: u64,
    ) -> Result<(), TransferError>;

    fn token(&self) -> Option<String>;
    fn has_table_progress(&self, source_table: &str) -> bool;
    fn table_has_committed_chunks(&self, source_table: &str) -> bool;
    fn is_invalidated(&self) -> bool;
    fn invalidate(&mut self);
}

pub(crate) struct ChunkedTransferContext<'a, 'checkpoint, 'formatter> {
    pub(crate) source_driver: &'a dyn DatabaseDriver,
    pub(crate) source_handle: &'a ConnectionHandle,
    pub(crate) target_driver: &'a dyn DatabaseDriver,
    pub(crate) target_handle: &'a ConnectionHandle,
    pub(crate) job: &'a TransferJob,
    pub(crate) table: &'a TableInspectResult,
    pub(crate) source_schema: &'a TableSchema,
    pub(crate) target_schema: &'a TableSchema,
    pub(crate) source_scope: &'a SourceScope,
    pub(crate) source_table_ref: &'a str,
    pub(crate) target_table_ref: &'a str,
    pub(crate) source_quote: char,
    pub(crate) target_type: &'a str,
    pub(crate) columns: &'a [&'a ColumnMapping],
    pub(crate) formatter: &'a ValueFormatter<'formatter>,
    pub(crate) cancelled: Option<Arc<AtomicBool>>,
    pub(crate) write_started: Option<&'a AtomicBool>,
    pub(crate) checkpoint: &'checkpoint mut dyn TransferResumeCheckpoint,
}

#[derive(Debug)]
pub(crate) struct ChunkedTableResult {
    pub(crate) result: TableExecutionResult,
    pub(crate) cancelled: bool,
    pub(crate) confirmed_rows: u64,
    pub(crate) stop_later_tables_reason: Option<&'static str>,
}

pub(crate) fn supports_chunk_driver(driver_type: &str) -> bool {
    matches!(
        driver_type.trim().to_ascii_lowercase().as_str(),
        "postgresql" | "postgres" | "mysql"
    )
}

/// Build the bounded page statement. Key values are bound after filter/range
/// values; the page limit is a server-selected literal constrained by the
/// TransferOptions hard maximum.
pub(crate) fn build_keyset_page(
    select_from: &str,
    where_sql: Option<&str>,
    base_params: &[Value],
    key_columns: &[String],
    cursor: Option<&[Value]>,
    limit: u32,
    quote: char,
    mut placeholder: impl FnMut(usize, Option<&str>) -> Result<String, TransferError>,
    column_type: impl Fn(&str) -> Option<String>,
) -> Result<(String, Vec<Value>), TransferError> {
    if limit == 0 || limit > MAX_TRANSFER_BATCH_SIZE {
        return Err(TransferError::validation(format!(
            "keyset page limit must be between 1 and {MAX_TRANSFER_BATCH_SIZE}"
        )));
    }
    if key_columns.is_empty() || cursor.is_some_and(|values| values.len() != key_columns.len()) {
        return Err(TransferError::validation(
            "keyset cursor does not match the complete source primary key",
        ));
    }

    let mut predicates = Vec::new();
    if let Some(where_sql) = where_sql {
        let predicate = where_sql
            .trim()
            .strip_prefix("WHERE ")
            .unwrap_or(where_sql.trim());
        if !predicate.is_empty() {
            predicates.push(format!("({predicate})"));
        }
    }
    let mut params = base_params.to_vec();
    if let Some(cursor) = cursor {
        let mut markers = Vec::with_capacity(key_columns.len());
        for (key, value) in key_columns.iter().zip(cursor) {
            if matches!(value, Value::Null) {
                return Err(TransferError::validation(
                    "source returned a NULL primary-key value; resume is disabled",
                ));
            }
            let index = params.len() + 1;
            markers.push(placeholder(index, column_type(key).as_deref())?);
            params.push(value.clone());
        }
        let quoted = key_columns
            .iter()
            .map(|key| quote_ident_sql(key, quote))
            .collect::<Vec<_>>();
        let left = if quoted.len() == 1 {
            quoted[0].clone()
        } else {
            format!("({})", quoted.join(", "))
        };
        let right = if markers.len() == 1 {
            markers[0].clone()
        } else {
            format!("({})", markers.join(", "))
        };
        predicates.push(format!("{left} > {right}"));
    }
    if params.len() > MAX_BOUND_QUERY_PARAMETERS {
        return Err(TransferError::validation(format!(
            "bounded source page requires {} parameters; the safe limit is {MAX_BOUND_QUERY_PARAMETERS}",
            params.len()
        )));
    }
    let mut sql = select_from.to_string();
    if !predicates.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&predicates.join(" AND "));
    }
    let order = key_columns
        .iter()
        .map(|key| format!("{} ASC", quote_ident_sql(key, quote)))
        .collect::<Vec<_>>()
        .join(", ");
    sql.push_str(&format!(" ORDER BY {order} LIMIT {limit}"));
    Ok((sql, params))
}

pub(crate) async fn execute_chunked_table(
    context: ChunkedTransferContext<'_, '_, '_>,
) -> Result<ChunkedTableResult, TransferError> {
    let mapping = context
        .job
        .tables
        .iter()
        .find(|mapping| mapping.source_table == context.table.source_table);
    let recordset = mapping.and_then(|mapping| mapping.recordset.as_ref());
    let keys = resumable_primary_key(
        context.source_schema,
        recordset,
        &context.source_driver.driver_type(),
    )?;
    if !supports_chunk_driver(context.target_type) {
        return Err(TransferError::unsupported(format!(
            "target driver '{}' has no verified bounded chunk transaction contract",
            context.target_type
        )));
    }
    if context
        .source_schema
        .table_options
        .supports_consistent_snapshot
        != Some(true)
    {
        return Err(TransferError::unsupported(
            "source relation is not proven to use a stable transactional snapshot (PG ordinary table or MySQL InnoDB required)",
        ));
    }
    if context
        .target_schema
        .table_options
        .supports_consistent_snapshot
        != Some(true)
    {
        return Err(TransferError::unsupported(
            "target relation is not proven transactional (PG ordinary table or MySQL InnoDB required)",
        ));
    }
    if context.source_handle.id == context.target_handle.id {
        return Err(TransferError::unsupported(
            "in-table resume requires separate source and target database sessions",
        ));
    }

    let projection = source_projection(context.columns, &keys);
    let chunk_size = effective_chunk_size(
        context.job.options.batch_size,
        context.columns.len(),
        context.target_driver.max_bound_parameters(),
    )?;
    let cursor_indexes = keys
        .iter()
        .map(|key| {
            projection
                .iter()
                .position(|column| column == key)
                .ok_or_else(|| TransferError::validation("key projection is incomplete"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let select_from = format!(
        "SELECT {} FROM {}",
        projection
            .iter()
            .map(|column| quote_ident_sql(column, context.source_quote))
            .collect::<Vec<_>>()
            .join(", "),
        context.source_table_ref
    );
    let row_limit = recordset
        .map(|recordset| resolve_recordset(recordset, context.source_schema))
        .transpose()?
        .and_then(|resolved| resolved.limit)
        .map(|limit| limit as u64);
    let source_fingerprint = {
        let snapshot = context
            .source_driver
            .begin_read_snapshot(context.source_handle)
            .await
            .map_err(|error| {
                TransferError::unsupported(format!(
                    "source driver could not open a stable read snapshot: {error}"
                ))
            })?;
        let fingerprint = fingerprint_source_rows(
            context.source_driver,
            context.source_handle,
            &select_from,
            context.source_scope,
            &keys,
            &cursor_indexes,
            &projection,
            context.source_schema,
            context.source_quote,
            chunk_size,
            row_limit,
            context.checkpoint,
            context.cancelled.as_deref(),
        )
        .await;
        match fingerprint {
            Ok(fingerprint) => (snapshot, fingerprint),
            Err(error) => {
                let rollback = context.source_driver.rollback(snapshot).await;
                return match rollback {
                    Ok(()) => Err(error),
                    Err(rollback_error) => {
                        let prior_chunks = context
                            .checkpoint
                            .table_has_committed_chunks(&context.table.source_table);
                        context.checkpoint.invalidate();
                        Ok(ChunkedTableResult {
                            result: TableExecutionResult::database(
                                &context.table.source_table,
                                &context.table.target_table,
                                None,
                                if prior_chunks {
                                    TableExecutionOutcome::PartiallyApplied
                                } else {
                                    TableExecutionOutcome::NotStarted
                                },
                                Some(format!(
                                    "{error}; source snapshot rollback outcome is UNKNOWN and the resume token was fenced: {rollback_error}"
                                )),
                            ),
                            cancelled: false,
                            confirmed_rows: 0,
                            stop_later_tables_reason: Some(
                                "not started because source snapshot cleanup was not confirmed and resume was fenced",
                            ),
                        })
                    }
                };
            }
        }
    };
    let (snapshot, source_fingerprint) = source_fingerprint;
    let mut progress = match context.checkpoint.prepare_table(
        &context.table.source_table,
        &context.table.target_table,
        &keys,
        chunk_size,
        &source_fingerprint,
    ) {
        Ok(progress) => progress,
        Err(error) => {
            let rollback = context.source_driver.rollback(snapshot).await;
            return match rollback {
                Ok(()) => Err(error),
                Err(rollback_error) => {
                    let prior_chunks = context
                        .checkpoint
                        .table_has_committed_chunks(&context.table.source_table);
                    context.checkpoint.invalidate();
                    Ok(ChunkedTableResult {
                        result: TableExecutionResult::database(
                            &context.table.source_table,
                            &context.table.target_table,
                            None,
                            if prior_chunks {
                                TableExecutionOutcome::PartiallyApplied
                            } else {
                                TableExecutionOutcome::NotStarted
                            },
                            Some(format!(
                                "{error}; source snapshot rollback outcome is UNKNOWN and the resume token was fenced: {rollback_error}"
                            )),
                        ),
                        cancelled: false,
                        confirmed_rows: 0,
                        stop_later_tables_reason: Some(
                            "not started because source snapshot cleanup was not confirmed and resume was fenced",
                        ),
                    })
                }
            };
        }
    };
    if progress
        .cursor
        .as_ref()
        .is_some_and(|cursor| cursor.len() != keys.len())
    {
        context.checkpoint.invalidate();
        return match context.source_driver.rollback(snapshot).await {
            Ok(()) => Err(TransferError::validation(
                "resume cursor no longer matches the source primary-key order; return to preview",
            )),
            Err(error) => Ok(ChunkedTableResult {
                result: TableExecutionResult::database(
                    &context.table.source_table,
                    &context.table.target_table,
                    None,
                    if context
                        .checkpoint
                        .table_has_committed_chunks(&context.table.source_table)
                    {
                        TableExecutionOutcome::PartiallyApplied
                    } else {
                        TableExecutionOutcome::NotStarted
                    },
                    Some(format!(
                        "resume cursor is invalid; source snapshot rollback outcome is UNKNOWN and the resume token was fenced: {error}"
                    )),
                ),
                cancelled: false,
                confirmed_rows: 0,
                stop_later_tables_reason: Some(
                    "not started because source snapshot cleanup was not confirmed and resume was fenced",
                ),
            }),
        };
    }

    let mut rows_inserted = 0u64;
    let mut terminal_error = None;
    let mut was_cancelled = false;
    let mut target_transaction_attempted = false;
    loop {
        if let Err(error) = context.checkpoint.renew() {
            context.checkpoint.invalidate();
            terminal_error = Some(format!(
                "resume execution lease could not be renewed; further writes are fenced: {error}"
            ));
            break;
        }
        if context
            .cancelled
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::SeqCst))
        {
            was_cancelled = true;
            break;
        }
        let Some(limit) = remaining_page_limit(chunk_size, progress.rows_seen, row_limit) else {
            break;
        };
        let query = match build_page_for_context(
            context.source_driver,
            &select_from,
            context.source_scope,
            &keys,
            progress.cursor.as_deref(),
            limit,
            context.source_quote,
            context.source_schema,
        ) {
            Ok(query) => query,
            Err(error) => {
                terminal_error = Some(error.to_string());
                break;
            }
        };
        let page = match context
            .source_driver
            .query_with_params(context.source_handle, &query.0, &query.1)
            .await
        {
            Ok(page) => page,
            Err(error) => {
                terminal_error = Some(format!("bounded source page read failed: {error}"));
                break;
            }
        };
        if let Err(error) = validate_page(&page, &projection, limit as usize) {
            terminal_error = Some(error.to_string());
            break;
        }
        if page.rows.is_empty() {
            break;
        }
        let mut next_cursor = Vec::with_capacity(keys.len());
        for index in &cursor_indexes {
            let value = page
                .rows
                .last()
                .and_then(|row| row.get(*index))
                .cloned()
                .flatten();
            let Some(value) = value else {
                terminal_error = Some("source primary-key cursor contains NULL".into());
                break;
            };
            if !cursor_value_supported(&value) {
                terminal_error =
                    Some("source primary-key cursor has an unsupported decoded value type".into());
                break;
            }
            next_cursor.push(value);
        }
        if terminal_error.is_some() {
            break;
        }
        let projected = page
            .rows
            .iter()
            .map(|row| {
                map_row_values(
                    &row[..context.columns.len()],
                    context.source_schema,
                    context.columns,
                )
            })
            .collect::<Result<Vec<_>, _>>();
        let projected = match projected {
            Ok(rows) => rows,
            Err(error) => {
                terminal_error = Some(error.to_string());
                break;
            }
        };
        if context
            .cancelled
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::SeqCst))
        {
            was_cancelled = true;
            break;
        }
        let (sql, params) = match super::writer::bound_insert_batch(
            context.target_driver,
            &context.table.source_table,
            context.target_table_ref,
            context.columns,
            context.target_schema,
            &projected,
            context.formatter,
        ) {
            Ok(statement) => statement,
            Err(error) => {
                terminal_error = Some(error.to_string());
                break;
            }
        };
        let tx = match context
            .target_driver
            .begin_transaction(context.target_handle)
            .await
        {
            Ok(tx) => tx,
            Err(error) => {
                terminal_error = Some(format!("cannot start target chunk transaction: {error}"));
                break;
            }
        };
        target_transaction_attempted = true;
        if let Some(write_started) = context.write_started {
            write_started.store(true, Ordering::SeqCst);
        }
        match context
            .target_driver
            .execute_with_params(context.target_handle, &sql, &params)
            .await
        {
            Err(error) => {
                terminal_error = Some(format!("target chunk write failed: {error}"));
                if let Err(rollback_error) = context.target_driver.rollback(tx).await {
                    context.checkpoint.invalidate();
                    let source_close =
                        rollback_source_snapshot(context.source_driver, snapshot).await;
                    return Ok(ChunkedTableResult {
                        result: TableExecutionResult::database(
                            &context.table.source_table,
                            &context.table.target_table,
                            None,
                            TableExecutionOutcome::Unknown,
                            Some(
                                format!(
                                    "{}; target rollback outcome is UNKNOWN: {rollback_error}",
                                    terminal_error
                                        .as_deref()
                                        .unwrap_or("target chunk write failed")
                                ) + &source_close,
                            ),
                        ),
                        cancelled: false,
                        confirmed_rows: rows_inserted,
                        stop_later_tables_reason: None,
                    });
                }
                break;
            }
            Ok(affected) => {
                if context
                    .cancelled
                    .as_ref()
                    .is_some_and(|flag| flag.load(Ordering::SeqCst))
                {
                    was_cancelled = true;
                    if let Err(rollback_error) = context.target_driver.rollback(tx).await {
                        context.checkpoint.invalidate();
                        let source_close =
                            rollback_source_snapshot(context.source_driver, snapshot).await;
                        return Ok(ChunkedTableResult {
                            result: TableExecutionResult::database(
                                &context.table.source_table,
                                &context.table.target_table,
                                None,
                                TableExecutionOutcome::Unknown,
                                Some(format!("target rollback outcome is UNKNOWN: {rollback_error}{source_close}")),
                            ),
                            cancelled: false,
                            confirmed_rows: rows_inserted,
                            stop_later_tables_reason: None,
                        });
                    }
                    break;
                }
                if let Err(error) = context.target_driver.commit(tx).await {
                    context.checkpoint.invalidate();
                    let source_close =
                        rollback_source_snapshot(context.source_driver, snapshot).await;
                    return Ok(ChunkedTableResult {
                        result: TableExecutionResult::database(
                            &context.table.source_table,
                            &context.table.target_table,
                            None,
                            TableExecutionOutcome::Unknown,
                            Some(format!(
                                "target chunk commit outcome is UNKNOWN: {error}{source_close}"
                            )),
                        ),
                        cancelled: false,
                        confirmed_rows: rows_inserted,
                        stop_later_tables_reason: None,
                    });
                }
                #[cfg(any(test, all(debug_assertions, feature = "webdriver")))]
                if super::execute::consume_test_commit_ack_loss(&context.table.target_table) {
                    context.checkpoint.invalidate();
                    let source_close =
                        rollback_source_snapshot(context.source_driver, snapshot).await;
                    return Ok(ChunkedTableResult {
                        result: TableExecutionResult::database(
                            &context.table.source_table,
                            &context.table.target_table,
                            None,
                            TableExecutionOutcome::Unknown,
                            Some(format!("debug test seam: target chunk committed but its acknowledgement was dropped{source_close}")),
                        ),
                        cancelled: false,
                        confirmed_rows: rows_inserted,
                            stop_later_tables_reason: None,
                    });
                }
                let next_rows_seen = progress.rows_seen.saturating_add(page.rows.len() as u64);
                if let Err(error) = context.checkpoint.advance_table(
                    &context.table.source_table,
                    next_cursor.clone(),
                    next_rows_seen,
                ) {
                    context.checkpoint.invalidate();
                    let source_close =
                        rollback_source_snapshot(context.source_driver, snapshot).await;
                    let confirmed_rows = rows_inserted.saturating_add(affected);
                    let scope_finished = page.rows.len() < limit as usize
                        || row_limit.is_some_and(|total_limit| next_rows_seen >= total_limit);
                    let outcome = confirmed_chunk_outcome(scope_finished);
                    return Ok(ChunkedTableResult {
                        result: TableExecutionResult::database(
                            &context.table.source_table,
                            &context.table.target_table,
                            Some(confirmed_rows),
                            outcome,
                            Some(format!(
                                "target chunk committed but checkpoint advancement failed; replay is fenced: {error}"
                            ) + &source_close),
                        ),
                        cancelled: false,
                        confirmed_rows,
                        stop_later_tables_reason: Some(
                            "not started because checkpoint advancement failed after a confirmed target commit; resume was fenced",
                        ),
                    });
                }
                rows_inserted = rows_inserted.saturating_add(affected);
                progress.cursor = Some(next_cursor);
                progress.rows_seen = next_rows_seen;
            }
        }
        if terminal_error.is_some() || was_cancelled {
            break;
        }
    }

    let source_snapshot_result = if terminal_error.is_some() || was_cancelled {
        context.source_driver.rollback(snapshot).await
    } else {
        context.source_driver.commit(snapshot).await
    };
    if let Err(error) = source_snapshot_result {
        context.checkpoint.invalidate();
        let has_committed_chunks = progress.rows_seen > 0;
        let outcome = if has_committed_chunks {
            if terminal_error.is_some() || was_cancelled {
                TableExecutionOutcome::PartiallyApplied
            } else {
                TableExecutionOutcome::Committed
            }
        } else if target_transaction_attempted {
            TableExecutionOutcome::RolledBack
        } else {
            TableExecutionOutcome::NotStarted
        };
        return Ok(ChunkedTableResult {
            result: TableExecutionResult::database(
                &context.table.source_table,
                &context.table.target_table,
                Some(rows_inserted),
                outcome,
                Some(format!(
                    "source snapshot close outcome is UNKNOWN; target chunk outcomes remain confirmed and resume is fenced: {error}"
                )),
            ),
            cancelled: was_cancelled,
            confirmed_rows: rows_inserted,
            stop_later_tables_reason: Some(
                "not started because source snapshot cleanup was not confirmed and resume was fenced",
            ),
        });
    }

    let has_committed_chunks = progress.rows_seen > 0;
    let outcome = if terminal_error.is_some() || was_cancelled {
        if has_committed_chunks {
            TableExecutionOutcome::PartiallyApplied
        } else if target_transaction_attempted {
            TableExecutionOutcome::RolledBack
        } else {
            TableExecutionOutcome::NotStarted
        }
    } else {
        TableExecutionOutcome::Committed
    };
    Ok(ChunkedTableResult {
        result: TableExecutionResult::database(
            &context.table.source_table,
            &context.table.target_table,
            Some(rows_inserted),
            outcome,
            terminal_error,
        ),
        cancelled: was_cancelled,
        confirmed_rows: rows_inserted,
        stop_later_tables_reason: None,
    })
}

async fn rollback_source_snapshot(
    driver: &dyn DatabaseDriver,
    snapshot: TransactionHandle,
) -> String {
    match driver.rollback(snapshot).await {
        Ok(()) => String::new(),
        Err(error) => format!("; source snapshot rollback outcome is UNKNOWN: {error}"),
    }
}

mod fingerprint;
#[cfg(test)]
mod tests;

pub(crate) use fingerprint::resumable_primary_key;
use fingerprint::{
    build_page_for_context, cursor_value_supported, effective_chunk_size, fingerprint_source_rows,
    hash_value, remaining_page_limit, source_projection, validate_page,
};
