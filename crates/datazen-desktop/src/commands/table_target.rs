//! Table browsing uses independent target executions; it never borrows a tree/editor handle.
//!
//! Host-boundary F4b: the read/preview algorithm (statement assembly, batch
//! layout, count fallback, result merge) lives in
//! `datazen::table_browsing::api`; this file keeps only the transport glue —
//! profile/driver lookups, platform assembly — and the desktop
//! implementation of the [`TargetCommandRunner`] port (target resolution,
//! submission, polling, artifact collection).
use super::{AppState, CommandError};
use crate::platform::{into_command_error, into_execution_target, PlatformAdapter};
// 端口实现里调的是 ConnectionUseCases 的 trait 方法（get_session /
// issue_submission_token / execute_in_session / execute_at_target /
// get_execution）；RuntimeConnectionUseCases 只是结构体，不带来方法。
use datazen::application::sessions::ConnectionUseCases;
use datazen::execution::application::idempotency::IdempotentOperation;
use datazen::execution::application::requests::{CommandCall, ExecuteAtTargetRequest};
use datazen::execution::application::RuntimeConnectionUseCases;
use datazen::execution::ExecutionState;
use datazen::shared::context::RequestContext;
use datazen::shared::error::{ApiError, ApiErrorCode};
use datazen::shared::id::Counter;
use datazen::table_browsing::api::{
    preview_table_read, read_table_page, TableReadError, TargetBinding, TargetCommandRunner,
};
use datazen_driver_api::TableDataResult;
use serde_json::Value as Json;

// The request/response DTOs moved to `datazen::table_browsing::api` with the
// use cases; these re-exports keep the `table_target::` paths this module's
// callers (the commands in `data.rs` and the target-read tests) already use.
pub use datazen::table_browsing::api::{
    TableReadPreviewRequest, TableReadPreviewSql, TableReadRequest,
};

/// The desktop implementation of the F4b execution port.
///
/// Runs a command at an explicit target (resolving it through the platform
/// adapter first) or inside a manually held session, then polls the execution
/// to a terminal state and collects every artifact chunk it published. The
/// algorithm that decides *which* commands to run lives in
/// `datazen::table_browsing::api`; this side only knows *how* the desktop
/// executes one.
struct DesktopTargetRunner<'a> {
    adapter: &'a PlatformAdapter,
    runtime: &'a RuntimeConnectionUseCases,
}

#[async_trait::async_trait]
impl TargetCommandRunner for DesktopTargetRunner<'_> {
    async fn run(
        &self,
        context: &RequestContext,
        binding: &TargetBinding,
        command: &str,
        input: Json,
    ) -> Result<Vec<Json>, ApiError> {
        let receipt = if let Some(handle) = binding.manual_handle.as_ref() {
            let session = self.runtime.get_session(context, handle.clone()).await?;
            if session.connection_id != binding.target.connection_id
                || session.config_revision != binding.revision
                || session.observed_context.namespace != binding.target.namespace
            {
                return Err(ApiError::new(
                    ApiErrorCode::InvalidArgument,
                    "Manual table session target changed",
                ));
            }
            let token = self
                .runtime
                .issue_submission_token(
                    context,
                    IdempotentOperation::ExecuteInSession,
                    Some(handle),
                )
                .await?;
            self.runtime
                .execute_in_session(
                    context,
                    datazen::execution::application::requests::ExecuteInSessionRequest {
                        handle: handle.clone(),
                        expected_context_revision: Some(session.context_revision),
                        call: CommandCall {
                            command: command.into(),
                            input,
                        },
                        idempotency_key: token.idempotency_key,
                    },
                )
                .await?
        } else {
            let canonical = self
                .adapter
                .resolve_execution_target(context, binding.revision, command, &binding.target)
                .await?;
            let target = into_execution_target(canonical);
            let token = self
                .runtime
                .issue_submission_token(context, IdempotentOperation::ExecuteAtTarget, None)
                .await?;
            self.runtime
                .execute_at_target(
                    context,
                    ExecuteAtTargetRequest {
                        target,
                        expected_config_revision: binding.revision,
                        call: CommandCall {
                            command: command.into(),
                            input,
                        },
                        idempotency_key: token.idempotency_key,
                    },
                )
                .await?
        };
        let view = loop {
            let view = self
                .runtime
                .get_execution(context, receipt.execution_id.clone())
                .await?;
            if view.state.is_terminal() {
                break view;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        };
        if view.state != ExecutionState::Succeeded {
            return Err(ApiError::new(
                ApiErrorCode::InvalidArgument,
                format!(
                    "Table execution failed: {:?}; effect outcome: {:?}",
                    view.error_code, view.effect_outcome
                ),
            ));
        }
        let mut outputs = Vec::new();
        for artifact in view.artifact_ids {
            let first = self
                .runtime
                .read_artifact_chunk(context, artifact.clone(), Counter::ZERO)
                .await?;
            for index in 0..first.published_chunk_count.get() {
                let chunk = if index == 0 {
                    first.clone()
                } else {
                    self.runtime
                        .read_artifact_chunk(context, artifact.clone(), Counter::new(index))
                        .await?
                };
                if let Some(output) = chunk.output {
                    outputs.push(output);
                }
            }
        }
        Ok(outputs)
    }
}

/// The single desktop mapping table for the table-read use-case errors.
///
/// `Runtime` flows through `into_command_error` untouched — `ResourceBusy` and
/// friends must keep their disposable IPC identity; `Validation` / `Internal`
/// are the same variants (and byte-identical messages) the host built inline
/// before F4b.
impl From<TableReadError> for CommandError {
    fn from(error: TableReadError) -> Self {
        match error {
            TableReadError::Runtime(error) => into_command_error(error),
            TableReadError::Validation(message) => CommandError::Validation(message),
            TableReadError::Internal(message) => CommandError::Internal(message),
        }
    }
}

pub async fn read(
    state: &AppState,
    request: TableReadRequest,
) -> Result<TableDataResult, CommandError> {
    // Input validation only: the api re-checks the context, the page size and
    // the filters because they must also hold when a second transport calls
    // it. The host copies keep the error order of this transport stable.
    super::validate_table_context(&request.context)?;
    if request.page_size == 0 || request.page_size > 10000 {
        return Err(CommandError::Validation("Invalid table page size".into()));
    }
    let (config, metadata) = state
        .store
        .platform_profile(&request.context.connection_id)
        .await
        .ok_or_else(|| CommandError::NotFound("Connection profile unavailable".into()))?;
    if !metadata.enabled || config.database_type != request.context.driver_type {
        return Err(CommandError::Validation(
            "Table connection context changed".into(),
        ));
    }
    let driver = state
        .driver_registry
        .get_sql_driver_by_name(config.database_type.as_str())
        .await
        .ok_or_else(|| CommandError::NotConfigured("SQL driver unavailable".into()))?;
    // The filter list is checked before anything is built or fetched: a filter
    // this database cannot serve, or one that is missing a value, has to fail
    // before the statement is assembled and before the connection is used.
    // The shared layer owns the judgement; the host only maps the refusal.
    datazen::services::validate_filters(
        driver.supported_filter_operators(),
        request.filters.as_deref().unwrap_or_default(),
    )?;
    let adapter = state.platform_require().map_err(into_command_error)?;
    let context = adapter.begin_call().context().clone();
    let runtime = adapter.runtime().map_err(into_command_error)?;
    let runner = DesktopTargetRunner {
        adapter,
        runtime: runtime.as_ref(),
    };
    read_table_page(
        driver.as_ref(),
        request,
        Counter::new(metadata.config_revision),
        context,
        &runner,
    )
    .await
    .map_err(CommandError::from)
}

/// Show the statements a table read would send, without sending them (DB-15).
pub async fn preview(
    state: &AppState,
    request: TableReadPreviewRequest,
) -> Result<TableReadPreviewSql, CommandError> {
    // The same input validation the real read applies, for the same reason:
    // keep this transport's error order stable; the api re-checks.
    super::validate_table_context(&request.context)?;
    if request.page_size == 0 || request.page_size > 10000 {
        return Err(CommandError::Validation("Invalid table page size".into()));
    }
    let (config, metadata) = state
        .store
        .platform_profile(&request.context.connection_id)
        .await
        .ok_or_else(|| CommandError::NotFound("Connection profile unavailable".into()))?;
    if !metadata.enabled || config.database_type != request.context.driver_type {
        return Err(CommandError::Validation(
            "Table connection context changed".into(),
        ));
    }
    let driver = state
        .driver_registry
        .get_sql_driver_by_name(config.database_type.as_str())
        .await
        .ok_or_else(|| CommandError::NotConfigured("SQL driver unavailable".into()))?;
    // The same refusal a real read gives: a preview of a statement this
    // database would refuse is a lie, not a preview.
    datazen::services::validate_filters(
        driver.supported_filter_operators(),
        request.filters.as_deref().unwrap_or_default(),
    )?;
    let adapter = state.platform_require().map_err(into_command_error)?;
    let context = adapter.begin_call().context().clone();
    let runtime = adapter.runtime().map_err(into_command_error)?;
    let runner = DesktopTargetRunner {
        adapter,
        runtime: runtime.as_ref(),
    };
    preview_table_read(
        driver.as_ref(),
        request,
        Counter::new(metadata.config_revision),
        context,
        &runner,
    )
    .await
    .map_err(CommandError::from)
}
