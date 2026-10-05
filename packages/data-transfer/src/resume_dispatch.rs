//! Narrow per-table dispatch into the bounded in-table resume executor.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use datazen_driver_api::TableSchema;

use datazen_driver_api::{ConnectionHandle, DatabaseDriver};

use super::error::TransferError;
use super::execute::ValueFormatter;
use super::model::{
    ColumnMapping, TableExecutionOutcome, TableExecutionResult, TableInspectResult,
    TableMappingStatus, TransferJob, TransferMode, TransferRecordset, WriteMode,
};
use super::recordset::SourceScope;
use super::resume::{self, ChunkedTableResult, ChunkedTransferContext, TransferResumeCheckpoint};

pub struct ResumeChunkContext<'a, 'checkpoint, 'formatter> {
    pub(super) source_driver: &'a dyn DatabaseDriver,
    pub(super) source_handle: &'a ConnectionHandle,
    pub(super) target_driver: &'a dyn DatabaseDriver,
    pub(super) target_handle: &'a ConnectionHandle,
    pub(super) job: &'a TransferJob,
    pub(super) table: &'a TableInspectResult,
    pub(super) source_schema: &'a TableSchema,
    pub(super) source_scope: &'a SourceScope,
    pub(super) source_table_ref: &'a str,
    pub(super) target_table_ref: &'a str,
    pub(super) source_quote: char,
    pub(super) target_session_id: &'a str,
    pub(super) source_family: &'a str,
    pub(super) target_family: &'a str,
    pub(super) recordset: Option<&'a TransferRecordset>,
    pub(super) columns: &'a [&'a ColumnMapping],
    pub(super) formatter: &'a ValueFormatter<'formatter>,
    pub(super) cancelled: Option<Arc<AtomicBool>>,
    pub(super) write_started: Option<&'a AtomicBool>,
    pub(super) checkpoint: Option<&'checkpoint mut dyn TransferResumeCheckpoint>,
}

pub enum ResumeChunkDispatch {
    NotApplicable,
    Executed(ChunkedTableResult),
    Rejected {
        result: TableExecutionResult,
        later_tables_reason: &'static str,
    },
}

pub async fn attempt_resume_chunk(
    mut context: ResumeChunkContext<'_, '_, '_>,
) -> ResumeChunkDispatch {
    let saved_progress = context
        .checkpoint
        .as_deref()
        .is_some_and(|checkpoint| checkpoint.has_table_progress(&context.table.source_table));
    let saved_committed_chunk = context.checkpoint.as_deref().is_some_and(|checkpoint| {
        checkpoint.table_has_committed_chunks(&context.table.source_table)
    });
    let data_mode = context.job.mode == TransferMode::Data;
    let insert_mode = context.job.write_mode == WriteMode::Insert;
    let existing_target = context.table.status == TableMappingStatus::Matched;
    let source_driver_supported = resume::supports_chunk_driver(context.source_family);
    let target_driver_supported = resume::supports_chunk_driver(context.target_family);
    let sessions_are_distinct =
        context.job.source.db_session_id.as_str() != context.target_session_id;
    let source_snapshot_safe = context
        .source_schema
        .table_options
        .supports_consistent_snapshot
        == Some(true);
    let candidate = data_mode
        && insert_mode
        && existing_target
        && source_driver_supported
        && target_driver_supported
        && sessions_are_distinct
        && source_snapshot_safe;

    if !candidate {
        tracing::debug!(
            table = %context.table.source_table,
            data_mode,
            insert_mode,
            existing_target,
            source_family = %context.source_family,
            source_driver_supported,
            target_family = %context.target_family,
            target_driver_supported,
            sessions_are_distinct,
            source_snapshot_safe,
            saved_progress,
            "bounded row-resume candidate rejected before execution"
        );
        if saved_progress {
            invalidate(&mut context);
            return rejected(
                context.table,
                saved_committed_chunk,
                "resume contract changed (driver, table mode, or source session); token was invalidated",
                "not started because the in-table resume contract changed",
            );
        }
        return ResumeChunkDispatch::NotApplicable;
    }

    let Some(checkpoint) = context.checkpoint else {
        tracing::debug!(
            table = %context.table.source_table,
            "bounded row-resume candidate had no checkpoint hook; using table-boundary path"
        );
        return ResumeChunkDispatch::NotApplicable;
    };
    let target = match context.job.database_target() {
        Ok(target) => target,
        Err(_) => return ResumeChunkDispatch::NotApplicable,
    };
    let target_schema = match super::metadata::load_table_schema(
        context.target_driver,
        context.target_handle,
        target,
        &context.table.target_table,
    )
    .await
    {
        Ok(schema) => schema,
        Err(error) => {
            checkpoint.invalidate();
            return rejected(
                context.table,
                saved_committed_chunk,
                &format!("resume target metadata could not be verified: {error}"),
                "not started because resume target metadata could not be verified",
            );
        }
    };
    let source_key = resume::resumable_primary_key(
        context.source_schema,
        context.recordset,
        context.source_family,
    );
    let target_is_snapshot_safe =
        target_schema.table_options.supports_consistent_snapshot == Some(true);
    if !target_is_snapshot_safe {
        checkpoint.invalidate();
        return rejected(
            context.table,
            saved_committed_chunk,
            "target relation no longer has a verified transactional boundary; token was invalidated",
            "not started because target transaction safety could not be reverified",
        );
    }
    if let Err(error) = source_key {
        tracing::debug!(
            table = %context.table.source_table,
            reason = %error,
            saved_progress,
            "bounded row-resume primary-key contract rejected"
        );
        if !saved_progress {
            return ResumeChunkDispatch::NotApplicable;
        }
        checkpoint.invalidate();
        return rejected(
            context.table,
            saved_committed_chunk,
            &format!("resume contract changed; token was invalidated: {error}"),
            "not started because the saved in-table resume contract changed",
        );
    }

    match resume::execute_chunked_table(ChunkedTransferContext {
        source_driver: context.source_driver,
        source_handle: context.source_handle,
        target_driver: context.target_driver,
        target_handle: context.target_handle,
        job: context.job,
        table: context.table,
        source_schema: context.source_schema,
        target_schema: &target_schema,
        source_scope: context.source_scope,
        source_table_ref: context.source_table_ref,
        target_table_ref: context.target_table_ref,
        source_quote: context.source_quote,
        target_type: context.target_family,
        columns: context.columns,
        formatter: context.formatter,
        cancelled: context.cancelled,
        write_started: context.write_started,
        checkpoint: &mut *checkpoint,
    })
    .await
    {
        Ok(result) => ResumeChunkDispatch::Executed(result),
        Err(TransferError::Cancelled(error)) => ResumeChunkDispatch::Executed(ChunkedTableResult {
            result: TableExecutionResult::database(
                &context.table.source_table,
                &context.table.target_table,
                Some(0),
                if saved_committed_chunk {
                    TableExecutionOutcome::PartiallyApplied
                } else {
                    TableExecutionOutcome::NotStarted
                },
                Some(error),
            ),
            cancelled: true,
            confirmed_rows: 0,
            stop_later_tables_reason: None,
        }),
        Err(error) => {
            let invalidated = checkpoint.is_invalidated();
            let now_has_progress = checkpoint.has_table_progress(&context.table.source_table);
            if invalidated || now_has_progress {
                if invalidated {
                    checkpoint.invalidate();
                    return rejected(
                        context.table,
                        saved_committed_chunk,
                        &error.to_string(),
                        "not started because resume validation failed and its token was invalidated",
                    );
                }
                ResumeChunkDispatch::Executed(ChunkedTableResult {
                    result: TableExecutionResult::database(
                        &context.table.source_table,
                        &context.table.target_table,
                        Some(0),
                        if saved_committed_chunk {
                            TableExecutionOutcome::PartiallyApplied
                        } else {
                            TableExecutionOutcome::NotStarted
                        },
                        Some(error.to_string()),
                    ),
                    cancelled: false,
                    confirmed_rows: 0,
                    stop_later_tables_reason: None,
                })
            } else {
                tracing::debug!(
                    table = %context.table.source_table,
                    reason = %error,
                    "bounded row-resume initialization failed before progress; falling back to atomic table transfer"
                );
                ResumeChunkDispatch::NotApplicable
            }
        }
    }
}

fn invalidate(context: &mut ResumeChunkContext<'_, '_, '_>) {
    if let Some(checkpoint) = context.checkpoint.as_deref_mut() {
        checkpoint.invalidate();
    }
}

fn rejected(
    table: &TableInspectResult,
    committed: bool,
    message: &str,
    later_tables_reason: &'static str,
) -> ResumeChunkDispatch {
    ResumeChunkDispatch::Rejected {
        result: TableExecutionResult::database(
            &table.source_table,
            &table.target_table,
            Some(0),
            if committed {
                TableExecutionOutcome::PartiallyApplied
            } else {
                TableExecutionOutcome::NotStarted
            },
            Some(message.to_string()),
        ),
        later_tables_reason,
    }
}
