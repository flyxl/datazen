//! Table ChangeSets own a short session and never join an editor transaction.
use super::{data::*, AppState, CommandError};
use crate::platform::into_command_error;
use datazen_application::{dto::requests::*, sessions::ConnectionUseCases};
use datazen_platform_api::{
    context::{OwnerRef, RequestContext},
    dto::{idempotency::IdempotentOperation, session::SessionHandle},
    id::ConnectionId,
    target::{ExecutionTarget, NamespaceTarget},
};
use datazen_runtime::application::RuntimeConnectionUseCases;
use std::sync::Arc;

struct TableSession {
    runtime: Arc<RuntimeConnectionUseCases>,
    context: RequestContext,
    handle: Option<SessionHandle>,
}
impl Drop for TableSession {
    fn drop(&mut self) {
        let Some(handle) = self.handle.take() else {
            return;
        };
        let runtime = self.runtime.clone();
        let context = self.context.clone();
        if let Ok(executor) = tokio::runtime::Handle::try_current() {
            executor.spawn(async move {
                if let Err(error) = runtime
                    .close_session(
                        &context,
                        CloseSessionRequest {
                            handle,
                            mode: CloseMode::RollbackAndClose,
                        },
                    )
                    .await
                {
                    tracing::warn!(code = ?error.code, "table session cleanup failed");
                }
            });
        }
    }
}
impl TableSession {
    async fn execute(
        &self,
        command: &str,
        input: serde_json::Value,
    ) -> Result<Option<u64>, CommandError> {
        let handle = self
            .handle
            .as_ref()
            .ok_or_else(|| CommandError::Internal("table session closed".into()))?;
        let session = self
            .runtime
            .get_session(&self.context, handle.clone())
            .await
            .map_err(into_command_error)?;
        let token = self
            .runtime
            .issue_submission_token(
                &self.context,
                IdempotentOperation::ExecuteInSession,
                Some(handle),
            )
            .await
            .map_err(into_command_error)?;
        let receipt = self
            .runtime
            .execute_in_session(
                &self.context,
                ExecuteInSessionRequest {
                    handle: handle.clone(),
                    expected_context_revision: Some(session.context_revision),
                    call: CommandCall {
                        command: command.into(),
                        input,
                    },
                    idempotency_key: token.idempotency_key,
                },
            )
            .await
            .map_err(into_command_error)?;
        let view = loop {
            let view = self
                .runtime
                .get_execution(&self.context, receipt.execution_id.clone())
                .await
                .map_err(into_command_error)?;
            if view.state.is_terminal() {
                break view;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        };
        if view.state != datazen_platform_api::dto::execution::ExecutionState::Succeeded {
            return Err(CommandError::Validation(format!(
                "Table execution failed: {:?}; effect outcome: {:?}",
                view.error_code, view.effect_outcome
            )));
        }
        for artifact in view.artifact_ids {
            let first = self
                .runtime
                .read_artifact_chunk(
                    &self.context,
                    artifact.clone(),
                    datazen_platform_api::id::Counter::ZERO,
                )
                .await
                .map_err(into_command_error)?;
            for index in 0..first.published_chunk_count.get() {
                let chunk = self
                    .runtime
                    .read_artifact_chunk(
                        &self.context,
                        artifact.clone(),
                        datazen_platform_api::id::Counter::new(index),
                    )
                    .await
                    .map_err(into_command_error)?;
                if let Some(affected) = chunk
                    .output
                    .as_ref()
                    .and_then(|output| output.get("rowsAffected"))
                    .and_then(serde_json::Value::as_u64)
                {
                    return Ok(Some(affected));
                }
            }
        }
        Ok(None)
    }
}

pub(super) async fn execute_plan(
    state: &AppState,
    driver: &dyn crate::db::DatabaseDriver,
    plan: &RowChangePlan,
    manual_handle: Option<SessionHandle>,
) -> Result<CommitPendingChangesResponse, CommandError> {
    let mut writes = Vec::new();
    for (delete, changes) in [(false, &plan.updates), (true, &plan.deletes)] {
        for change in changes {
            let statement =
                super::data::bound_statement_for_plan(driver, &plan.table.table, change, delete)?;
            writes.push((delete, change, statement));
        }
    }
    if writes.is_empty() {
        return Ok(CommitPendingChangesResponse {
            plan_id: plan.plan_id.clone(),
            fingerprint: plan.fingerprint.clone(),
            statements: Vec::new(),
            affected_rows: 0,
        });
    }
    let adapter = state.platform_require().map_err(into_command_error)?;
    let context = adapter.begin_call().context().clone();
    let runtime = adapter.runtime().map_err(into_command_error)?;
    let manual = manual_handle.is_some();
    let opened = if let Some(handle) = manual_handle {
        let view = runtime
            .get_session(&context, handle)
            .await
            .map_err(into_command_error)?;
        if view.connection_id.as_str() != plan.table.connection_id
            || view.observed_context.namespace.database != plan.table.database
            || view.observed_context.namespace.schema != plan.table.schema
            || !matches!(
                view.observed_context.transaction_state,
                datazen_platform_api::dto::session::TransactionState::Active
            )
        {
            return Err(CommandError::Validation(
                "Manual table transaction target changed or transaction not active".into(),
            ));
        }
        view
    } else {
        let token = runtime
            .issue_submission_token(&context, IdempotentOperation::OpenSession, None)
            .await
            .map_err(into_command_error)?;
        runtime
            .open_session(
                &context,
                OpenSessionRequest {
                    initial_target: ExecutionTarget::new(
                        ConnectionId::new(&plan.table.connection_id),
                        NamespaceTarget::new(
                            plan.table.database.clone(),
                            None,
                            plan.table.schema.clone(),
                            Vec::new(),
                        ),
                    ),
                    owner: OwnerRef::ClientSession {
                        client_instance_id: context.client_instance_id.clone(),
                        purpose: "tableChangeSet".into(),
                    },
                    idempotency_key: token.idempotency_key,
                },
            )
            .await
            .map_err(into_command_error)?
            .session
    };
    let mut session = TableSession {
        runtime,
        context,
        handle: Some(opened.handle),
    };
    if opened.observed_context.namespace.database != plan.table.database
        || plan
            .table
            .schema
            .as_ref()
            .is_some_and(|schema| opened.observed_context.namespace.schema.as_ref() != Some(schema))
    {
        if manual {
            session.handle = None;
        }
        return Err(CommandError::Validation(
            "Table target could not be confirmed on its owned session".into(),
        ));
    }
    if plan.config_revision != Some(opened.config_revision) {
        if manual {
            session.handle = None;
        }
        return Err(CommandError::Validation(
            "Connection configuration changed before execution; preview again".into(),
        ));
    }
    let result = async {
        if !manual {
            session
                .execute(
                    "platform.beginTransaction",
                    serde_json::json!({"defersCommit":true}),
                )
                .await?;
        }
        let mut statements = Vec::new();
        for (delete, change, statement) in writes {
            let affected = session
                .execute(
                    "execute",
                    serde_json::json!({
                        "sql": statement.sql, "boundParameters": statement.parameters,
                    }),
                )
                .await?
                .ok_or_else(|| {
                    CommandError::Validation("Driver did not report affected rows".into())
                })?;
            if affected != 1 {
                return Err(CommandError::Validation(format!(
                    "Row identity affected {affected} rows; refusing ambiguous write"
                )));
            }
            statements.push(RowCommitStatementResult {
                operation: if delete { "delete" } else { "update" }.into(),
                row_identity: change.row_identity.clone(),
                affected_rows: affected,
            });
        }
        if !manual {
            session
                .execute(
                    "platform.commitTransaction",
                    serde_json::json!({"defersCommit":true}),
                )
                .await?;
        }
        Ok(CommitPendingChangesResponse {
            plan_id: plan.plan_id.clone(),
            fingerprint: plan.fingerprint.clone(),
            affected_rows: statements.len() as u64,
            statements,
        })
    }
    .await;
    if manual {
        // The table owner retains control of its explicit transaction even on
        // failure; neither Drop nor this ChangeSet may commit/rollback/close it.
        session.handle = None;
        return result;
    }
    let handle = session
        .handle
        .as_ref()
        .cloned()
        .ok_or_else(|| CommandError::Internal("table session missing".into()))?;
    let closed = session
        .runtime
        .close_session(
            &session.context,
            CloseSessionRequest {
                handle,
                // commit's receipt is already terminal. Cleanup must also work when
                // subsequent context observation is unknown; it cannot change the
                // proven committed effect of the response.
                mode: CloseMode::RollbackAndClose,
            },
        )
        .await;
    if closed.is_ok() {
        session.handle = None;
    }
    match (result, closed) {
        (Ok(response), Ok(_)) => Ok(response),
        (Ok(_), Err(error)) => Err(CommandError::Validation(format!("Changes committed; session cleanup failed: {:?}. Reload before further edits.", error.code))),
        (Err(error), Ok(_)) => Err(error),
        (Err(error), Err(_)) => Err(CommandError::Validation(format!("{error}; rollback could not be confirmed. Reload and verify the target before retrying."))),
    }
}
