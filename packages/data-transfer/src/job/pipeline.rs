//! 有界管道（§6.2）：固定源 reader → 有界 typed row batch → IR 转换 →
//! 有界参数批次 → 固定目标 writer/transaction → commit → checkpoint。
//!
//! - 每 pipeline 缓冲初值 8 MiB（[`PIPELINE_INITIAL_BYTES`]），计入解码行、
//!   转换副本与待发送参数；单值超限明确失败、不扩张缓冲绕过限制。
//! - 批大小受行上限、字节上限与 driver 参数三上限约束。
//! - 慢目标时源暂停（单执行任务 + 在途写入必须先结算）；取消/错误唤醒两端，
//!   停止新读取、终结快照、等待在途写入实际结果后 cleanup。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use datazen_driver_api::{ConnectionHandle, DatabaseDriver, TransactionHandle, Value};
use datazen_platform_api::dto::job::{CommitBoundary, JobProgress};
use datazen_platform_api::id::Counter;
use sha2::{Digest, Sha256};

use crate::error::TransferError;
use crate::execute::ValueFormatter;
use crate::model::{
    ColumnMapping, TableExecutionOutcome, TableExecutionResult, TableInspectResult, TransferJob,
};
use crate::recordset::SourceScope;
use crate::resume::{ChunkedTableResult, TransferResumeCheckpoint};

use crate::job::checkpoint::commit_boundary;

mod budget;
mod page;

pub use budget::{row_bytes, value_bytes, PipelineBudget, PIPELINE_INITIAL_BYTES};
pub(crate) use page::{read_page_within_budget, PageSource};

/// 有界管道执行上下文（与 ChunkedTransferContext 对齐，但带字节账与逐批边界）。
pub struct BoundedPipelineContext<'a> {
    pub job: &'a TransferJob,
    pub table: &'a TableInspectResult,
    pub source_schema: &'a datazen_driver_api::TableSchema,
    pub target_schema: &'a datazen_driver_api::TableSchema,
    pub source_driver: &'a dyn DatabaseDriver,
    pub source_handle: &'a ConnectionHandle,
    pub target_driver: &'a dyn DatabaseDriver,
    pub target_handle: &'a ConnectionHandle,
    pub source_scope: &'a SourceScope,
    pub source_table_ref: &'a str,
    pub target_table_ref: &'a str,
    pub source_quote: char,
    pub target_type: &'a str,
    pub columns: &'a [&'a ColumnMapping],
    pub formatter: &'a ValueFormatter<'a>,
    pub cancelled: Option<Arc<AtomicBool>>,
    pub write_started: Option<&'a AtomicBool>,
    pub checkpoint: &'a mut dyn TransferResumeCheckpoint,
}

pub struct BoundedTableOutcome {
    pub result: ChunkedTableResult,
    pub boundaries: Vec<CommitBoundary>,
    pub progress: JobProgress,
    pub max_buffer_bytes: usize,
}

fn payload_digest(params: &[Value]) -> String {
    let mut hasher = Sha256::new();
    for value in params {
        match value {
            Value::Null => hasher.update(b"null"),
            Value::String(s) => {
                hasher.update(s.as_bytes());
            }
            Value::Bytes(b) => hasher.update(b),
            other => hasher.update(format!("{other:?}").as_bytes()),
        }
    }
    format!("{:x}", hasher.finalize())
}

/// 单表有界数据阶段：读页 → 转换 → 参数批次 → 事务写入 → commit → checkpoint。
pub async fn execute_bounded_table(
    context: &mut BoundedPipelineContext<'_>,
) -> Result<BoundedTableOutcome, TransferError> {
    use crate::resume::fingerprint::{
        effective_chunk_size, fingerprint_source_rows, remaining_page_limit, source_projection,
    };
    use crate::resume::{resumable_primary_key, supports_chunk_driver};

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
            "source relation is not proven to use a stable transactional snapshot",
        ));
    }
    if context
        .target_schema
        .table_options
        .supports_consistent_snapshot
        != Some(true)
    {
        return Err(TransferError::unsupported(
            "target relation is not proven transactional",
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
            .map(|column| { datazen_data_sync::sql::quote_ident_sql(column, context.source_quote) })
            .collect::<Vec<_>>()
            .join(", "),
        context.source_table_ref
    );
    let row_limit = recordset
        .map(|recordset| crate::recordset::resolve_recordset(recordset, context.source_schema))
        .transpose()?
        .and_then(|resolved| resolved.limit)
        .map(|limit| limit as u64);

    let mut budget = PipelineBudget::new();
    let mut progress = JobProgress::default();

    // 源快照证明：打不开快照/指纹取证失败都禁止续写（§6.3）。
    let (snapshot, source_fingerprint) = {
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
                        context.checkpoint.invalidate();
                        Ok(BoundedTableOutcome {
                            result: ChunkedTableResult {
                                result: TableExecutionResult::database(
                                    &context.table.source_table,
                                    &context.table.target_table,
                                    None,
                                    TableExecutionOutcome::NotStarted,
                                    Some(format!(
                                        "{error}; source snapshot rollback outcome is UNKNOWN and the resume token was fenced: {rollback_error}"
                                    )),
                                ),
                                cancelled: false,
                                confirmed_rows: 0,
                                stop_later_tables_reason: Some(
                                    "not started because source snapshot cleanup was not confirmed and resume was fenced",
                                ),
                            },
                            boundaries: Vec::new(),
                            progress,
                            max_buffer_bytes: budget.max_used(),
                        })
                    }
                };
            }
        }
    };

    let mut saved = match context.checkpoint.prepare_table(
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
                    context.checkpoint.invalidate();
                    Ok(BoundedTableOutcome {
                        result: ChunkedTableResult {
                            result: TableExecutionResult::database(
                                &context.table.source_table,
                                &context.table.target_table,
                                None,
                                TableExecutionOutcome::NotStarted,
                                Some(format!(
                                    "{error}; source snapshot rollback outcome is UNKNOWN and the resume token was fenced: {rollback_error}"
                                )),
                            ),
                            cancelled: false,
                            confirmed_rows: 0,
                            stop_later_tables_reason: Some(
                                "not started because source snapshot cleanup was not confirmed and resume was fenced",
                            ),
                        },
                        boundaries: Vec::new(),
                        progress,
                        max_buffer_bytes: budget.max_used(),
                    })
                }
            };
        }
    };

    let mut rows_inserted = 0u64;
    let mut terminal_error: Option<String> = None;
    let mut was_cancelled = false;
    let mut target_transaction_attempted = false;
    let mut boundaries: Vec<CommitBoundary> = Vec::new();
    let mut batch_index = 0u64;

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
            let _ = context
                .source_driver
                .cancel_query(context.source_handle)
                .await;
            let _ = context
                .target_driver
                .cancel_query(context.target_handle)
                .await;
            break;
        }
        let Some(limit) = remaining_page_limit(chunk_size, saved.rows_seen, row_limit) else {
            break;
        };
        // 读页在字节额度内完成：读之前按行数预留额度，额度不够就缩窗重读同一
        // 游标位置，只有 1 行仍超限才明确失败。
        let source = PageSource {
            driver: context.source_driver,
            handle: context.source_handle,
            scope: context.source_scope,
            quote: context.source_quote,
            schema: context.source_schema,
            columns: context.columns,
        };
        let mut prepared = match read_page_within_budget(
            source,
            &mut budget,
            &select_from,
            &projection,
            &keys,
            saved.cursor.as_deref(),
            limit,
        )
        .await
        {
            Ok(Some(page)) => page,
            Ok(None) => break,
            Err(error) => {
                terminal_error = Some(error.to_string());
                break;
            }
        };
        let page_len = prepared.rows.len();
        let converted_bytes = prepared.converted_bytes;
        progress.read = Counter::new(progress.read.get().saturating_add(page_len as u64));
        progress.converted = Counter::new(
            progress
                .converted
                .get()
                .saturating_add(prepared.projected.len() as u64),
        );

        let mut next_cursor = Vec::with_capacity(keys.len());
        for index in &cursor_indexes {
            let value = prepared
                .rows
                .last()
                .and_then(|row| row.get(*index))
                .cloned()
                .flatten();
            let Some(value) = value else {
                terminal_error = Some("source primary-key cursor contains NULL".into());
                break;
            };
            if !crate::resume::fingerprint::cursor_value_supported(&value) {
                terminal_error =
                    Some("source primary-key cursor has an unsupported decoded value type".into());
                break;
            }
            next_cursor.push(value);
        }
        if terminal_error.is_some() {
            // 游标没取到，这一页不会写：解码行与转换副本一次性还账。
            budget.release(prepared.page_bytes + converted_bytes);
            break;
        }
        // 游标已取走，解码行不再需要：只剩转换副本与待发送参数留在账上。
        prepared.release_source_rows(&mut budget);
        let projected = prepared.projected;

        if context
            .cancelled
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::SeqCst))
        {
            was_cancelled = true;
            let _ = context
                .source_driver
                .cancel_query(context.source_handle)
                .await;
            let _ = context
                .target_driver
                .cancel_query(context.target_handle)
                .await;
            break;
        }

        let (sql, params) = match crate::writer::bound_insert_batch(
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
        // 待发送参数计入缓冲账；参数装不进剩余额度就明确失败，不扩张缓冲。
        let param_bytes: usize = params.iter().map(|p| value_bytes(Some(p))).sum();
        if !budget.try_account(param_bytes) {
            terminal_error = Some(format!(
                "one batch needs {param_bytes} more bytes of pipeline buffer, over the {} byte bound; lower the batch size",
                budget.capacity()
            ));
            break;
        }
        progress.attempted = Counter::new(
            progress
                .attempted
                .get()
                .saturating_add(projected.len() as u64),
        );

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
                    budget.release(converted_bytes + param_bytes);
                    let source_close =
                        rollback_source_snapshot(context.source_driver, snapshot).await;
                    return Ok(BoundedTableOutcome {
                        result: ChunkedTableResult {
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
                        },
                        boundaries,
                        progress,
                        max_buffer_bytes: budget.max_used(),
                    });
                }
                budget.release(converted_bytes + param_bytes);
                break;
            }
            Ok(affected) => {
                if context
                    .cancelled
                    .as_ref()
                    .is_some_and(|flag| flag.load(Ordering::SeqCst))
                {
                    was_cancelled = true;
                    let _ = context
                        .source_driver
                        .cancel_query(context.source_handle)
                        .await;
                    if let Err(rollback_error) = context.target_driver.rollback(tx).await {
                        context.checkpoint.invalidate();
                        budget.release(converted_bytes + param_bytes);
                        let source_close =
                            rollback_source_snapshot(context.source_driver, snapshot).await;
                        return Ok(BoundedTableOutcome {
                            result: ChunkedTableResult {
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
                            },
                            boundaries,
                            progress,
                            max_buffer_bytes: budget.max_used(),
                        });
                    }
                    budget.release(converted_bytes + param_bytes);
                    break;
                }
                if let Err(error) = context.target_driver.commit(tx).await {
                    context.checkpoint.invalidate();
                    budget.release(converted_bytes + param_bytes);
                    let source_close =
                        rollback_source_snapshot(context.source_driver, snapshot).await;
                    progress.unknown = Counter::new(progress.unknown.get().saturating_add(1));
                    return Ok(BoundedTableOutcome {
                        result: ChunkedTableResult {
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
                        },
                        boundaries,
                        progress,
                        max_buffer_bytes: budget.max_used(),
                    });
                }

                // 提交确认 → 版本化边界 + checkpoint advance。
                let next_rows_seen = saved.rows_seen.saturating_add(page_len as u64);
                if let Err(error) = context.checkpoint.advance_table(
                    &context.table.source_table,
                    next_cursor.clone(),
                    next_rows_seen,
                ) {
                    context.checkpoint.invalidate();
                    budget.release(converted_bytes + param_bytes);
                    let source_close =
                        rollback_source_snapshot(context.source_driver, snapshot).await;
                    let confirmed_rows = rows_inserted.saturating_add(affected);
                    let scope_finished = page_len < limit as usize
                        || row_limit.is_some_and(|total_limit| next_rows_seen >= total_limit);
                    let outcome = if scope_finished {
                        TableExecutionOutcome::Committed
                    } else {
                        TableExecutionOutcome::PartiallyApplied
                    };
                    return Ok(BoundedTableOutcome {
                        result: ChunkedTableResult {
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
                        },
                        boundaries,
                        progress,
                        max_buffer_bytes: budget.max_used(),
                    });
                }
                rows_inserted = rows_inserted.saturating_add(affected);
                progress.committed =
                    Counter::new(progress.committed.get().saturating_add(affected));
                boundaries.push(commit_boundary(
                    "data",
                    &format!("{}#b{}", context.table.source_table, batch_index),
                    &payload_digest(&params),
                    affected,
                    "target-commit-ack",
                    &format!("sha256:{source_fingerprint}"),
                    None,
                ));
                batch_index += 1;
                // 提交后释放该批账；在途未提交批期间字节持续占用（背压）。
                budget.release(converted_bytes + param_bytes);
                saved.cursor = Some(next_cursor);
                saved.rows_seen = next_rows_seen;
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
        let has_committed_chunks = saved.rows_seen > 0;
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
        return Ok(BoundedTableOutcome {
            result: ChunkedTableResult {
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
            },
            boundaries,
            progress,
            max_buffer_bytes: budget.max_used(),
        });
    }

    let has_committed_chunks = saved.rows_seen > 0;
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
    Ok(BoundedTableOutcome {
        result: ChunkedTableResult {
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
        },
        boundaries,
        progress,
        max_buffer_bytes: budget.max_used(),
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
