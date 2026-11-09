//! Execution and transaction ownership for a table ChangeSet.
//!
//! Execution order is fixed: new rows first (a child row may reference a
//! parent inserted in the same batch), then updates, then deletes — all inside
//! one transaction. The Desktop host and a future web host both call this
//! function with handles they obtained from their own platform adapter, so the
//! transaction rules cannot drift between transports.

/// The owned session opened with a namespace that is not the plan's, so the
/// plan no longer applies. The caller's only safe move is to preview again.
pub(crate) fn stale_table_target_error() -> FeatureError {
    FeatureError::Validation(
        "grid.commit.stalePlan: Table target could not be confirmed on its owned session; preview the changes again"
            .into(),
    )
}

/// The caller's manual transaction drifted away from the plan (target changed,
/// or the transaction is gone). Same user action as any stale plan.
pub(crate) fn stale_manual_target_error() -> FeatureError {
    FeatureError::Validation(
        "grid.commit.stalePlan: Manual table transaction target changed or is no longer active; preview the changes and commit again"
            .into(),
    )
}

use super::change_set::{
    bound_statement_for_plan, validate_immutable_plan, validate_table_context,
    CommitPendingChangesRequest, CommitPendingChangesResponse, PlannedStatement, RowChangePlan,
    RowCommitStatementResult,
};
use super::insert_plan::bound_insert_statement_for_plan;
use super::{validate_commit_facts, validate_connection_facts, ConnectionFacts, TableColumnSource};
use crate::connection::domain::NamespaceTarget;
use crate::execution::application::use_cases::ConnectionUseCases;
use crate::execution::domain::session::ContextConfidence;
use crate::execution::domain::ExecutionTarget;
use crate::execution::requests::*;
use crate::execution::{IdempotentOperation, SessionHandle};
use crate::shared::context::{OwnerRef, RequestContext};
use crate::shared::id::ConnectionId;
use crate::table_editing::error::TableEditingError as FeatureError;
use crate::table_editing::ports::ArtifactChunkReader;
use datazen_driver_api::{BoundSqlStatement, DatabaseDriver};
use std::sync::Arc;

struct TableSession {
    runtime: Arc<dyn ConnectionUseCases>,
    artifacts: Arc<dyn ArtifactChunkReader>,
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
    ) -> Result<Option<u64>, FeatureError> {
        let handle = self
            .handle
            .as_ref()
            .ok_or_else(|| FeatureError::Internal("table session closed".into()))?;
        let session = self
            .runtime
            .get_session(&self.context, handle.clone())
            .await
            .map_err(FeatureError::from)?;
        let token = self
            .runtime
            .issue_submission_token(
                &self.context,
                IdempotentOperation::ExecuteInSession,
                Some(handle),
            )
            .await
            .map_err(FeatureError::from)?;
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
            .map_err(FeatureError::from)?;
        let view = loop {
            let view = self
                .runtime
                .get_execution(&self.context, receipt.execution_id.clone())
                .await
                .map_err(FeatureError::from)?;
            if view.state.is_terminal() {
                break view;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        };
        if view.state != crate::execution::ExecutionState::Succeeded {
            return Err(FeatureError::Validation(format!(
                "Table execution failed: {:?}; effect outcome: {:?}",
                view.error_code, view.effect_outcome
            )));
        }
        for artifact in view.artifact_ids {
            let first = self
                .artifacts
                .read_chunk(&self.context, &artifact, crate::shared::id::Counter::ZERO)
                .await
                .map_err(FeatureError::from)?;
            for index in 0..first.published_chunk_count.get() {
                let chunk = self
                    .artifacts
                    .read_chunk(
                        &self.context,
                        &artifact,
                        crate::shared::id::Counter::new(index),
                    )
                    .await
                    .map_err(FeatureError::from)?;
                if let Some(affected) = chunk.output.as_ref().and_then(rows_affected_of_output) {
                    return Ok(Some(affected));
                }
            }
        }
        Ok(None)
    }
}

/// 产物摘要的 `rowsAffected` 读取，兼容两种既有产物形状：顶层字段（桌面
/// 侧历史形状与测试假件）与 `commandResult` 包裹的 `data` 内字段——
/// `LegacyResourceAdapter` 非 query 命令的产物是
/// `{"kind":"commandResult","data":{"rowsAffected":N}}`
/// （`execute_standard_sql_command` 的应答被 adapter 原样包裹），server
/// 形态的写执行链读的是后者。摘要字段的两种历史形状都认，不做形状裁决。
fn rows_affected_of_output(output: &serde_json::Value) -> Option<u64> {
    output
        .get("rowsAffected")
        .and_then(serde_json::Value::as_u64)
        .or_else(|| {
            output
                .get("data")
                .and_then(|data| data.get("rowsAffected"))
                .and_then(serde_json::Value::as_u64)
        })
}

/// The plan's writes in execution order: new rows first (a child row may
/// reference a parent inserted in the same batch), then updates, then deletes.
/// Both commit halves (the ChangeSet's `execute_plan` and the result-cell's
/// `commit_result_cell_changes`) share the order and the statement rendering,
/// so the two transports run the same statements in the same sequence.
fn plan_writes<'a>(
    driver: &dyn DatabaseDriver,
    plan: &'a RowChangePlan,
) -> Result<Vec<(&'static str, &'a PlannedStatement, BoundSqlStatement)>, FeatureError> {
    let mut writes = Vec::new();
    for change in &plan.inserts {
        let statement = bound_insert_statement_for_plan(driver, &plan.table.table, change)?;
        writes.push(("insert", change, statement));
    }
    for (operation, changes) in [("update", &plan.updates), ("delete", &plan.deletes)] {
        for change in changes {
            let statement =
                bound_statement_for_plan(driver, &plan.table.table, change, operation == "delete")?;
            writes.push((operation, change, statement));
        }
    }
    Ok(writes)
}

pub async fn execute_plan(
    driver: &dyn DatabaseDriver,
    plan: &RowChangePlan,
    manual_handle: Option<SessionHandle>,
    runtime: Arc<dyn ConnectionUseCases>,
    artifacts: Arc<dyn ArtifactChunkReader>,
    context: RequestContext,
) -> Result<CommitPendingChangesResponse, FeatureError> {
    // Execution order is fixed: new rows first (a child row may reference a
    // parent inserted in the same batch), then updates, then deletes. All three
    // share the transaction opened below.
    let writes = plan_writes(driver, plan)?;
    if writes.is_empty() {
        return Ok(CommitPendingChangesResponse {
            plan_id: plan.plan_id.clone(),
            fingerprint: plan.fingerprint.clone(),
            statements: Vec::new(),
            affected_rows: 0,
        });
    }
    // The host owns platform access: it passes the runtime engine and request
    // context in, so this module never touches Tauri or the desktop adapter.
    let manual = manual_handle.is_some();
    let opened = if let Some(handle) = manual_handle {
        let view = runtime
            .get_session(&context, handle)
            .await
            .map_err(FeatureError::from)?;
        if view.connection_id.as_str() != plan.table.connection_id
            || view.observed_context.namespace.database != plan.table.database
            || view.observed_context.namespace.schema != plan.table.schema
            || !matches!(
                view.observed_context.transaction_state,
                crate::execution::TransactionState::Active
            )
        {
            return Err(stale_manual_target_error());
        }
        view
    } else {
        let token = runtime
            .issue_submission_token(&context, IdempotentOperation::OpenSession, None)
            .await
            .map_err(FeatureError::from)?;
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
            .map_err(FeatureError::from)?
            .session
    };
    let mut session = TableSession {
        runtime,
        artifacts,
        context,
        handle: Some(opened.handle),
    };
    // 目标复验只在「会话上下文已确认」时拿 namespace 说话：
    // `ContextConfidence::Confirmed` 是观察事实的唯一准入（`SessionContext`
    // 文档；`SessionContext::unobserved()` 刻意不携带 namespace，「不拿
    // initialTarget 假装确认」是可检查裁决）。unobservable 观察（legacy
    // 驱动无读回 API——`LegacyResourceAdapter::observe_session` 全 unknown，
    // server 形态的全部驱动都走这条路）携带不了 namespace：那是**观察能力
    // 缺失**，不是目标漂移；把 Unknown 当漂移拒，owned-session 提交在所有
    // legacy 驱动下结构性不可服务。漂移的真实防线不在此处：开立目标由本
    // 函数自己铸造（initial_target 就是 plan 的 database/schema），物理
    // 开立按请求 target 执行，且 config_revision 复验（下方）守的是非
    // 观察事实。
    if opened.observed_context.confidence == ContextConfidence::Confirmed
        && (opened.observed_context.namespace.database != plan.table.database
            || plan.table.schema.as_ref().is_some_and(|schema| {
                opened.observed_context.namespace.schema.as_ref() != Some(schema)
            }))
    {
        if manual {
            session.handle = None;
        }
        return Err(stale_table_target_error());
    }
    if plan.config_revision != Some(opened.config_revision) {
        if manual {
            session.handle = None;
        }
        return Err(FeatureError::Validation(
            "grid.commit.stalePlan: Connection configuration changed before execution; preview again".into(),
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
        for (operation, change, statement) in writes {
            let affected = session
                .execute(
                    "execute",
                    serde_json::json!({
                        "sql": statement.sql, "boundParameters": statement.parameters,
                    }),
                )
                .await?
                .ok_or_else(|| {
                    FeatureError::Validation("grid.commit.affectedMismatch: Driver did not report affected rows".into())
                })?;
            if affected != 1 {
                return Err(FeatureError::Validation(if operation == "insert" {
                    format!("grid.commit.affectedMismatch: INSERT affected {affected} rows; refusing ambiguous write")
                } else {
                    format!("grid.commit.affectedMismatch: Row identity affected {affected} rows; refusing ambiguous write")
                }));
            }
            statements.push(RowCommitStatementResult {
                // An inserted row's identity is only known after a returning
                // clause backfills it; without one the host does not guess.
                operation: operation.into(),
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
        .ok_or_else(|| FeatureError::Internal("table session missing".into()))?;
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
        (Ok(_), Err(error)) => Err(FeatureError::Validation(format!("Changes committed; session cleanup failed: {:?}. Reload before further edits.", error.code))),
        (Err(error), Ok(_)) => Err(error),
        (Err(error), Err(_)) => Err(FeatureError::Validation(format!("{error}; rollback could not be confirmed. Reload and verify the target before retrying."))),
    }
}

/// The result-cell commit half's stale refusal: the plan's target and the
/// original session no longer agree (the session moved, or the result's
/// session was recreated). Same user action as any stale plan.
pub(crate) fn stale_result_target_error() -> FeatureError {
    FeatureError::Validation(
        "grid.commit.stalePlan: Result session no longer matches the plan's target; execute the query again and preview the change"
            .into(),
    )
}

/// Execute a fingerprint-confirmed single-cell plan in the session that
/// produced the result (the original-session binding behind
/// `commitQueryResultCell`).
///
/// The gates are [`super::commit_changes`]' five — context facts, the config
/// CAS, the read-only refusal, namespace drift and the immutable-plan rebuild
/// against freshly loaded columns — via the same helpers, in the same order.
/// The execution half differs on purpose:
///
/// * the plan runs in the **caller's session** (`handle`) through
///   `execute_in_session` — the original-session binding is the point of the
///   feature, so no session is opened and none is closed (the client keeps
///   the result grid and may edit more cells on it);
/// * no transaction is begun or committed here: transaction semantics follow
///   the session (`w8-knife4-result-cell-design.md` §1 step 6). A session
///   inside an explicit transaction keeps the write in it; an autocommit
///   session commits the single statement — the desktop write path behaves
///   the same way, and an ambiguous affected-row report is refused with
///   `grid.commit.affectedMismatch` telling the caller to inspect the target.
/// * the session re-validation is capability-aware (the same ruling as the
///   owned-session path above): `connection_id` is a registry fact and is
///   always checked, while namespace claims are observations — checked only
///   when the driver could actually observe them. Every server-shape driver
///   is unobservable today, so treating an absent observation as drift would
///   make the endpoint structurally unserviceable.
#[allow(clippy::too_many_arguments)]
pub async fn commit_result_cell_changes(
    driver: &dyn DatabaseDriver,
    request: CommitPendingChangesRequest,
    facts: &ConnectionFacts,
    columns: &dyn TableColumnSource,
    handle: SessionHandle,
    runtime: Arc<dyn ConnectionUseCases>,
    artifacts: Arc<dyn ArtifactChunkReader>,
    context: RequestContext,
) -> Result<CommitPendingChangesResponse, FeatureError> {
    validate_table_context(&request.plan.table)?;
    validate_connection_facts(facts, &request.plan.table)?;
    validate_commit_facts(&request, facts)?;
    let loaded = columns.table_columns(&request.plan.table).await?;
    let plan = validate_immutable_plan(
        driver,
        &request.db_session_id,
        &request.plan,
        &request.fingerprint,
        &loaded,
    )?;
    let writes = plan_writes(driver, &plan)?;
    if writes.is_empty() {
        return Ok(CommitPendingChangesResponse {
            plan_id: plan.plan_id.clone(),
            fingerprint: plan.fingerprint.clone(),
            statements: Vec::new(),
            affected_rows: 0,
        });
    }
    let view = runtime
        .get_session(&context, handle.clone())
        .await
        .map_err(FeatureError::from)?;
    if view.connection_id.as_str() != plan.table.connection_id {
        return Err(stale_result_target_error());
    }
    if view.observed_context.confidence == ContextConfidence::Confirmed
        && (view.observed_context.namespace.database != plan.table.database
            || plan.table.schema.as_ref().is_some_and(|schema| {
                view.observed_context.namespace.schema.as_ref() != Some(schema)
            }))
    {
        return Err(stale_result_target_error());
    }
    if plan.config_revision != Some(view.config_revision) {
        return Err(FeatureError::Validation(
            "grid.commit.stalePlan: Connection configuration changed before execution; preview again"
                .into(),
        ));
    }
    let mut session = TableSession {
        runtime,
        artifacts,
        context,
        handle: Some(handle),
    };
    let result = async {
        let mut statements = Vec::new();
        for (operation, change, statement) in writes {
            let affected = session
                .execute(
                    "execute",
                    serde_json::json!({
                        "sql": statement.sql, "boundParameters": statement.parameters,
                    }),
                )
                .await?
                .ok_or_else(|| {
                    FeatureError::Validation(
                        "grid.commit.affectedMismatch: Driver did not report affected rows"
                            .into(),
                    )
                })?;
            if affected != 1 {
                return Err(FeatureError::Validation(format!(
                    "grid.commit.affectedMismatch: Result edit affected {affected} rows; inspect the target and current transaction before retrying"
                )));
            }
            statements.push(RowCommitStatementResult {
                operation: operation.into(),
                row_identity: change.row_identity.clone(),
                affected_rows: affected,
            });
        }
        Ok(CommitPendingChangesResponse {
            plan_id: plan.plan_id.clone(),
            fingerprint: plan.fingerprint.clone(),
            affected_rows: statements.len() as u64,
            statements,
        })
    }
    .await;
    // The original session stays alive whatever the outcome: it is the
    // client's live query session, never this commit's resource. Clearing the
    // handle keeps `TableSession`'s Drop from closing it.
    session.handle = None;
    result
}
