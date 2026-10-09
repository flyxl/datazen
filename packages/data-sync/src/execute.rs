//! Dedicated execute path: transaction + parameterized SQL. Not `execute_query`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use datazen_driver_api::Value;
use serde::{Deserialize, Serialize};

use super::error::DataSyncError;
use super::model::{ChangeOperation, ConflictPolicy};
use super::sql::{IdentityInsertTarget, SqlStatement};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncConflict {
    pub table: String,
    pub operation: ChangeOperation,
    pub row_key: Vec<Value>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionResult {
    pub applied: usize,
    pub rolled_back: bool,
    /// Explains a confirmed rollback. A missing reason means execution
    /// committed successfully or returned an error with an unknown outcome.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollback_reason: Option<String>,
    /// Total rows reported by the database for successful statements.
    /// Defaults during deserialization so older persisted responses remain valid.
    #[serde(default)]
    pub affected_rows: u64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub skipped: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conflicts: Vec<SyncConflict>,
}

fn is_zero(value: &usize) -> bool {
    *value == 0
}

/// What the Data Sync executor can prove about this run. Unknown means that
/// the transaction may have committed, but the application did not receive a
/// confirmed commit or rollback response.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionOutcome {
    NotStarted,
    Committed,
    RolledBack,
    Unknown,
}

/// IPC response that keeps the legacy result fields flat while adding an
/// explicit, evidence-based outcome. Preflight errors are returned as
/// `not_started` so callers do not incorrectly fence a run as unknown.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataSyncExecutionResponse {
    #[serde(flatten)]
    pub result: ExecutionResult,
    pub outcome: ExecutionOutcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl DataSyncExecutionResponse {
    pub fn from_result(result: ExecutionResult) -> Self {
        let outcome = if result.rolled_back {
            ExecutionOutcome::RolledBack
        } else {
            ExecutionOutcome::Committed
        };
        Self {
            result,
            outcome,
            error: None,
        }
    }

    pub fn failed_before_start(_error: impl Into<String>) -> Self {
        Self {
            result: ExecutionResult {
                applied: 0,
                rolled_back: false,
                rollback_reason: None,
                affected_rows: 0,
                skipped: 0,
                conflicts: Vec::new(),
            },
            outcome: ExecutionOutcome::NotStarted,
            // Raw driver errors can include connection URIs, filesystem paths,
            // SQL fragments, and filter values. This field crosses IPC inside
            // an Ok payload, so keep it intentionally generic.
            error: Some(
                "Execution did not start. Check the plan and endpoint context, then compare again."
                    .into(),
            ),
        }
    }

    pub fn unknown(_error: impl Into<String>) -> Self {
        Self {
            result: ExecutionResult {
                applied: 0,
                rolled_back: false,
                rollback_reason: None,
                affected_rows: 0,
                skipped: 0,
                conflicts: Vec::new(),
            },
            outcome: ExecutionOutcome::Unknown,
            error: Some("Commit or rollback could not be confirmed. Compare current data before continuing.".into()),
        }
    }
}

#[async_trait]
pub trait StatementExecutor: Send {
    fn is_read_only(&self) -> bool;
    async fn begin(&mut self) -> Result<(), DataSyncError>;
    async fn execute(&mut self, sql: &str, params: &[Value]) -> Result<u64, DataSyncError>;
    async fn set_identity_insert(
        &mut self,
        _target: &IdentityInsertTarget,
        _enabled: bool,
    ) -> Result<(), DataSyncError> {
        Err(DataSyncError::validation(
            "executor does not support session-scoped identity insertion",
        ))
    }
    async fn discard_connection(&mut self) -> Result<(), DataSyncError> {
        Err(DataSyncError::validation(
            "executor cannot discard a connection after identity cleanup failed",
        ))
    }
    async fn commit(&mut self) -> Result<(), DataSyncError>;
    async fn rollback(&mut self) -> Result<(), DataSyncError>;
}

#[derive(Default)]
struct IdentityInsertState {
    /// A toggle may be on after an ON attempt, even when the driver returned
    /// an error. Keep this until OFF succeeds or the connection is discarded.
    active: Option<IdentityInsertTarget>,
    cleanup_error: Option<String>,
}

impl IdentityInsertState {
    async fn set_for_statement(
        &mut self,
        executor: &mut dyn StatementExecutor,
        target: Option<&IdentityInsertTarget>,
    ) -> Result<(), DataSyncError> {
        if self.active.as_ref() == target {
            return Ok(());
        }
        if let Some(current) = self.active.clone() {
            match executor.set_identity_insert(&current, false).await {
                Ok(()) => self.active = None,
                Err(error) => {
                    self.cleanup_error = Some(error.to_string());
                    return Err(error);
                }
            }
        }
        if let Some(target) = target {
            // ON may have taken effect even if its reply is lost. Record the
            // target before awaiting so every failure path attempts OFF.
            self.active = Some(target.clone());
            executor.set_identity_insert(target, true).await?;
        }
        Ok(())
    }

    async fn rollback(
        &mut self,
        executor: &mut dyn StatementExecutor,
        reason: &str,
    ) -> Result<String, DataSyncError> {
        if self.active.is_some() && self.cleanup_error.is_none() {
            if let Some(target) = self.active.clone() {
                if let Err(error) = executor.set_identity_insert(&target, false).await {
                    self.cleanup_error = Some(error.to_string());
                } else {
                    self.active = None;
                }
            }
        }

        let rollback_result = executor.rollback().await;
        let discard_result = if self.cleanup_error.is_some() {
            Some(executor.discard_connection().await)
        } else {
            None
        };

        if let Err(error) = rollback_result {
            let cleanup = self
                .cleanup_error
                .as_deref()
                .map(|message| format!("; identity insert cleanup failed: {message}"))
                .unwrap_or_default();
            let discard = discard_result
                .as_ref()
                .and_then(|result| result.as_ref().err())
                .map(|error| format!("; connection discard failed: {error}"))
                .unwrap_or_default();
            return Err(DataSyncError::outcome_unknown(format!(
                "{reason}{cleanup}; rollback failed, outcome UNKNOWN: {error}{discard}"
            )));
        }

        if let Some(Err(error)) = discard_result {
            return Err(DataSyncError::outcome_unknown(format!(
                "{reason}; identity insert cleanup failed: {}; connection discard failed, session state UNKNOWN: {error}",
                self.cleanup_error.as_deref().unwrap_or("unknown error")
            )));
        }

        Ok(match self.cleanup_error.as_deref() {
            Some(error) => format!(
                "{reason}; identity insert cleanup failed: {error}; target connection discarded after rollback"
            ),
            None => reason.to_string(),
        })
    }
}

/// Produces one bounded group of statements at a time for large sync plans.
/// A source error after execution has started is handled like a statement
/// failure: the active target transaction is rolled back.
#[async_trait]
pub trait StatementBatchSource: Send {
    async fn next_batch(&mut self) -> Result<Option<Vec<SqlStatement>>, DataSyncError>;
}

pub async fn execute_statements(
    statements: &[SqlStatement],
    executor: &mut dyn StatementExecutor,
    cancelled: Option<Arc<AtomicBool>>,
) -> Result<ExecutionResult, DataSyncError> {
    execute_statements_with_policy(statements, executor, cancelled, ConflictPolicy::Abort).await
}

pub async fn execute_statements_with_policy(
    statements: &[SqlStatement],
    executor: &mut dyn StatementExecutor,
    cancelled: Option<Arc<AtomicBool>>,
    conflict_policy: ConflictPolicy,
) -> Result<ExecutionResult, DataSyncError> {
    if statements.is_empty() {
        return Err(DataSyncError::validation(
            "change set is empty; nothing to execute",
        ));
    }
    if executor.is_read_only() {
        return Err(DataSyncError::validation(
            "target connection is read-only; Data Synchronization cannot execute",
        ));
    }
    if cancelled.as_ref().is_some_and(|c| c.load(Ordering::SeqCst)) {
        return Err(DataSyncError::not_started(
            "execute cancelled before any changes were applied",
        ));
    }

    if let Err(error) = executor.begin().await {
        return Err(DataSyncError::not_started(format!(
            "transaction could not start; no changes were applied: {error}"
        )));
    }
    let mut applied = 0usize;
    let mut affected_rows = 0u64;
    let mut skipped = 0usize;
    let mut conflicts = Vec::new();
    let mut identity_insert = IdentityInsertState::default();
    for stmt in statements {
        if cancelled.as_ref().is_some_and(|c| c.load(Ordering::SeqCst)) {
            let reason = identity_insert
                .rollback(executor, "execute cancelled; all changes were rolled back")
                .await?;
            return Ok(ExecutionResult {
                applied,
                rolled_back: true,
                rollback_reason: Some(reason),
                affected_rows,
                skipped: 0,
                conflicts: Vec::new(),
            });
        }
        if let Err(error) = identity_insert
            .set_for_statement(executor, stmt.identity_insert.as_ref())
            .await
        {
            let reason = identity_insert
                .rollback(executor, &format!("identity insert setup failed: {error}"))
                .await?;
            return Ok(ExecutionResult {
                applied,
                rolled_back: true,
                rollback_reason: Some(reason),
                affected_rows,
                skipped: 0,
                conflicts: Vec::new(),
            });
        }
        let operation = stmt.operation;
        match executor.execute(&stmt.sql, &stmt.parameters).await {
            Ok(affected) => {
                if matches!(operation, ChangeOperation::Update | ChangeOperation::Delete)
                    && affected == 0
                {
                    let message = format!(
                        "optimistic sync conflict: {} for table '{}' affected zero rows",
                        match operation {
                            ChangeOperation::Update => "UPDATE",
                            ChangeOperation::Delete => "DELETE",
                            _ => "write",
                        },
                        stmt.table
                    );
                    if conflict_policy == ConflictPolicy::Skip {
                        skipped += 1;
                        conflicts.push(SyncConflict {
                            table: stmt.table.clone(),
                            operation,
                            row_key: stmt.row_key.clone(),
                            message,
                        });
                        continue;
                    }
                    let reason = identity_insert.rollback(executor, &message).await?;
                    return Ok(ExecutionResult {
                        applied,
                        rolled_back: true,
                        rollback_reason: Some(reason),
                        affected_rows,
                        skipped: 0,
                        conflicts: vec![SyncConflict {
                            table: stmt.table.clone(),
                            operation,
                            row_key: stmt.row_key.clone(),
                            message,
                        }],
                    });
                }
                applied += 1;
                affected_rows += affected;
            }
            Err(err) => {
                let reason = format!("execution failed after {applied} statements: {err}");
                let reason = identity_insert.rollback(executor, &reason).await?;
                return Ok(ExecutionResult {
                    applied,
                    rolled_back: true,
                    rollback_reason: Some(reason),
                    affected_rows,
                    skipped: 0,
                    conflicts: Vec::new(),
                });
            }
        }
    }
    if cancelled.as_ref().is_some_and(|c| c.load(Ordering::SeqCst)) {
        let reason = identity_insert
            .rollback(executor, "execute cancelled; all changes were rolled back")
            .await?;
        return Ok(ExecutionResult {
            applied,
            rolled_back: true,
            rollback_reason: Some(reason),
            affected_rows,
            skipped: 0,
            conflicts: Vec::new(),
        });
    }
    if let Err(error) = identity_insert.set_for_statement(executor, None).await {
        let reason = identity_insert
            .rollback(
                executor,
                &format!("cannot disable identity insert before commit: {error}"),
            )
            .await?;
        return Ok(ExecutionResult {
            applied,
            rolled_back: true,
            rollback_reason: Some(reason),
            affected_rows,
            skipped: 0,
            conflicts: Vec::new(),
        });
    }
    executor.commit().await.map_err(|error| {
        DataSyncError::outcome_unknown(format!(
            "commit result could not be confirmed; outcome UNKNOWN: {error}"
        ))
    })?;
    Ok(ExecutionResult {
        applied,
        rolled_back: false,
        rollback_reason: None,
        affected_rows,
        skipped,
        conflicts,
    })
}

/// Execute a pre-generated first page and then request further statement
/// pages while keeping one transaction open for the entire plan.
///
/// The caller generates the first non-empty page before claiming the plan or
/// opening a transaction. Later page-generation failures roll back prior
/// writes from this run.
pub async fn execute_statement_batches_with_policy(
    first_batch: Vec<SqlStatement>,
    source: &mut dyn StatementBatchSource,
    executor: &mut dyn StatementExecutor,
    cancelled: Option<Arc<AtomicBool>>,
    conflict_policy: ConflictPolicy,
) -> Result<ExecutionResult, DataSyncError> {
    if first_batch.is_empty() {
        return Err(DataSyncError::validation(
            "change set is empty; nothing to execute",
        ));
    }
    if executor.is_read_only() {
        return Err(DataSyncError::validation(
            "target connection is read-only; Data Synchronization cannot execute",
        ));
    }
    if cancelled.as_ref().is_some_and(|c| c.load(Ordering::SeqCst)) {
        return Err(DataSyncError::not_started(
            "execute cancelled before any changes were applied",
        ));
    }

    if let Err(error) = executor.begin().await {
        return Err(DataSyncError::not_started(format!(
            "transaction could not start; no changes were applied: {error}"
        )));
    }

    let mut applied = 0usize;
    let mut affected_rows = 0u64;
    let mut skipped = 0usize;
    let mut conflicts = Vec::new();
    let mut identity_insert = IdentityInsertState::default();
    let mut batch = Some(first_batch);
    loop {
        let statements = match batch.take() {
            Some(statements) => statements,
            None => match source.next_batch().await {
                Ok(Some(statements)) if !statements.is_empty() => statements,
                Ok(Some(_)) => continue,
                Ok(None) => break,
                Err(error) => {
                    let reason =
                        format!("statement generation failed after {applied} statements: {error}");
                    let reason = identity_insert.rollback(executor, &reason).await?;
                    return Ok(ExecutionResult {
                        applied,
                        rolled_back: true,
                        rollback_reason: Some(reason),
                        affected_rows,
                        skipped: 0,
                        conflicts: Vec::new(),
                    });
                }
            },
        };

        for stmt in statements {
            if cancelled.as_ref().is_some_and(|c| c.load(Ordering::SeqCst)) {
                let reason = identity_insert
                    .rollback(executor, "execute cancelled; all changes were rolled back")
                    .await?;
                return Ok(ExecutionResult {
                    applied,
                    rolled_back: true,
                    rollback_reason: Some(reason),
                    affected_rows,
                    skipped: 0,
                    conflicts: Vec::new(),
                });
            }
            if let Err(error) = identity_insert
                .set_for_statement(executor, stmt.identity_insert.as_ref())
                .await
            {
                let reason = identity_insert
                    .rollback(executor, &format!("identity insert setup failed: {error}"))
                    .await?;
                return Ok(ExecutionResult {
                    applied,
                    rolled_back: true,
                    rollback_reason: Some(reason),
                    affected_rows,
                    skipped: 0,
                    conflicts: Vec::new(),
                });
            }
            let operation = stmt.operation;
            match executor.execute(&stmt.sql, &stmt.parameters).await {
                Ok(affected) => {
                    if matches!(operation, ChangeOperation::Update | ChangeOperation::Delete)
                        && affected == 0
                    {
                        let message = format!(
                            "optimistic sync conflict: {} for table '{}' affected zero rows",
                            match operation {
                                ChangeOperation::Update => "UPDATE",
                                ChangeOperation::Delete => "DELETE",
                                _ => "write",
                            },
                            stmt.table
                        );
                        if conflict_policy == ConflictPolicy::Skip {
                            skipped += 1;
                            conflicts.push(SyncConflict {
                                table: stmt.table,
                                operation,
                                row_key: stmt.row_key,
                                message,
                            });
                            continue;
                        }
                        let reason = identity_insert.rollback(executor, &message).await?;
                        return Ok(ExecutionResult {
                            applied,
                            rolled_back: true,
                            rollback_reason: Some(reason),
                            affected_rows,
                            skipped: 0,
                            conflicts: vec![SyncConflict {
                                table: stmt.table,
                                operation,
                                row_key: stmt.row_key,
                                message,
                            }],
                        });
                    }
                    applied += 1;
                    affected_rows += affected;
                }
                Err(error) => {
                    let reason = format!("execution failed after {applied} statements: {error}");
                    let reason = identity_insert.rollback(executor, &reason).await?;
                    return Ok(ExecutionResult {
                        applied,
                        rolled_back: true,
                        rollback_reason: Some(reason),
                        affected_rows,
                        skipped: 0,
                        conflicts: Vec::new(),
                    });
                }
            }
        }
    }

    if let Err(error) = identity_insert.set_for_statement(executor, None).await {
        let reason = identity_insert
            .rollback(
                executor,
                &format!("cannot disable identity insert before commit: {error}"),
            )
            .await?;
        return Ok(ExecutionResult {
            applied,
            rolled_back: true,
            rollback_reason: Some(reason),
            affected_rows,
            skipped: 0,
            conflicts: Vec::new(),
        });
    }

    if cancelled.as_ref().is_some_and(|c| c.load(Ordering::SeqCst)) {
        let reason = identity_insert
            .rollback(executor, "execute cancelled; all changes were rolled back")
            .await?;
        return Ok(ExecutionResult {
            applied,
            rolled_back: true,
            rollback_reason: Some(reason),
            affected_rows,
            skipped: 0,
            conflicts: Vec::new(),
        });
    }

    executor.commit().await.map_err(|error| {
        DataSyncError::outcome_unknown(format!(
            "commit result could not be confirmed; outcome UNKNOWN: {error}"
        ))
    })?;
    Ok(ExecutionResult {
        applied,
        rolled_back: false,
        rollback_reason: None,
        affected_rows,
        skipped,
        conflicts,
    })
}

#[derive(Default)]
pub struct RecordingExecutor {
    pub read_only: bool,
    pub fail_at: Option<usize>,
    pub zero_at: Option<usize>,
    pub fail_identity_toggle: Option<(String, bool)>,
    pub fail_discard: bool,
    pub cancel_after_execute: Option<Arc<AtomicBool>>,
    pub calls: Vec<String>,
    pub begun: bool,
}

#[async_trait]
impl StatementExecutor for RecordingExecutor {
    fn is_read_only(&self) -> bool {
        self.read_only
    }

    async fn begin(&mut self) -> Result<(), DataSyncError> {
        self.calls.push("begin".into());
        self.begun = true;
        Ok(())
    }

    async fn execute(&mut self, sql: &str, params: &[Value]) -> Result<u64, DataSyncError> {
        self.calls.push(format!("execute:{}:{}", params.len(), sql));
        let execute_index = self
            .calls
            .iter()
            .filter(|call| call.starts_with("execute:"))
            .count()
            - 1;
        if self.fail_at == Some(execute_index) {
            return Err(DataSyncError::validation("injected failure"));
        }
        if let Some(cancelled) = &self.cancel_after_execute {
            cancelled.store(true, Ordering::SeqCst);
        }
        Ok(if self.zero_at == Some(execute_index) {
            0
        } else {
            1
        })
    }

    async fn set_identity_insert(
        &mut self,
        target: &IdentityInsertTarget,
        enabled: bool,
    ) -> Result<(), DataSyncError> {
        self.calls.push(format!(
            "identity:{}:{}:{}:{}",
            if enabled { "on" } else { "off" },
            target.database,
            target.schema.as_deref().unwrap_or_default(),
            target.table
        ));
        if self
            .fail_identity_toggle
            .as_ref()
            .is_some_and(|(table, toggle)| table == &target.table && *toggle == enabled)
        {
            return Err(DataSyncError::validation(
                "injected identity toggle failure",
            ));
        }
        Ok(())
    }

    async fn discard_connection(&mut self) -> Result<(), DataSyncError> {
        self.calls.push("discard".into());
        if self.fail_discard {
            return Err(DataSyncError::validation("injected discard failure"));
        }
        Ok(())
    }

    async fn commit(&mut self) -> Result<(), DataSyncError> {
        self.calls.push("commit".into());
        self.begun = false;
        Ok(())
    }

    async fn rollback(&mut self) -> Result<(), DataSyncError> {
        self.calls.push("rollback".into());
        self.begun = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ChangeOperation;
    use std::collections::VecDeque;

    struct VecBatchSource(VecDeque<Result<Option<Vec<SqlStatement>>, DataSyncError>>);

    #[async_trait]
    impl StatementBatchSource for VecBatchSource {
        async fn next_batch(&mut self) -> Result<Option<Vec<SqlStatement>>, DataSyncError> {
            self.0.pop_front().unwrap_or(Ok(None))
        }
    }

    fn batch_source(batches: impl IntoIterator<Item = Vec<SqlStatement>>) -> VecBatchSource {
        VecBatchSource(
            batches
                .into_iter()
                .map(|batch| Ok(Some(batch)))
                .chain(std::iter::once(Ok(None)))
                .collect(),
        )
    }

    fn stmt(sql: &str) -> SqlStatement {
        SqlStatement {
            table: "t".into(),
            operation: ChangeOperation::Insert,
            sql: sql.into(),
            preview_sql: sql.into(),
            parameters: vec![Value::Integer(1)],
            row_key: vec![Value::Integer(1)],
            identity_insert: None,
        }
    }

    fn identity_stmt(sql: &str, table: &str) -> SqlStatement {
        let mut statement = stmt(sql);
        statement.identity_insert = Some(IdentityInsertTarget {
            database: "db".into(),
            schema: Some("dbo".into()),
            table: table.into(),
        });
        statement
    }

    #[test]
    fn identity_insert_target_is_not_serialized_into_sql_preview_payloads() {
        let statement = identity_stmt("INSERT", "users");
        let json = serde_json::to_value(&statement).unwrap();
        assert!(json.get("identityInsert").is_none());
        let decoded: SqlStatement = serde_json::from_value(json).unwrap();
        assert!(decoded.identity_insert.is_none());
    }

    #[tokio::test]
    async fn test_tester_stream_rejects_empty_first_batch_before_opening_a_transaction() {
        let mut exec = RecordingExecutor::default();
        let mut source = batch_source(Vec::<Vec<SqlStatement>>::new());
        let error = execute_statement_batches_with_policy(
            Vec::new(),
            &mut source,
            &mut exec,
            None,
            ConflictPolicy::Abort,
        )
        .await
        .unwrap_err();

        assert!(error.to_string().contains("change set is empty"));
        assert!(exec.calls.is_empty());
    }

    #[tokio::test]
    async fn test_tester_stream_rejects_read_only_before_opening_a_transaction() {
        let mut exec = RecordingExecutor {
            read_only: true,
            ..RecordingExecutor::default()
        };
        let mut source = batch_source(Vec::<Vec<SqlStatement>>::new());
        let error = execute_statement_batches_with_policy(
            vec![stmt("INSERT")],
            &mut source,
            &mut exec,
            None,
            ConflictPolicy::Abort,
        )
        .await
        .unwrap_err();

        assert!(error.to_string().contains("read-only"));
        assert!(exec.calls.is_empty());
    }

    #[tokio::test]
    async fn test_tester_stream_cancelled_before_begin_does_not_open_a_transaction() {
        let mut exec = RecordingExecutor::default();
        let mut source = batch_source(Vec::<Vec<SqlStatement>>::new());
        let cancelled = Arc::new(AtomicBool::new(true));
        let error = execute_statement_batches_with_policy(
            vec![stmt("INSERT")],
            &mut source,
            &mut exec,
            Some(cancelled),
            ConflictPolicy::Abort,
        )
        .await
        .unwrap_err();

        assert!(
            matches!(error, DataSyncError::NotStarted(message) if message.contains("before any changes"))
        );
        assert!(exec.calls.is_empty());
    }

    #[tokio::test]
    async fn test_tester_stream_begin_failure_reports_no_writes() {
        struct BeginFailureExecutor(Vec<String>);
        #[async_trait]
        impl StatementExecutor for BeginFailureExecutor {
            fn is_read_only(&self) -> bool {
                false
            }
            async fn begin(&mut self) -> Result<(), DataSyncError> {
                self.0.push("begin".into());
                Err(DataSyncError::validation("injected begin failure"))
            }
            async fn execute(&mut self, _: &str, _: &[Value]) -> Result<u64, DataSyncError> {
                self.0.push("execute".into());
                Ok(1)
            }
            async fn commit(&mut self) -> Result<(), DataSyncError> {
                self.0.push("commit".into());
                Ok(())
            }
            async fn rollback(&mut self) -> Result<(), DataSyncError> {
                self.0.push("rollback".into());
                Ok(())
            }
        }

        let mut exec = BeginFailureExecutor(Vec::new());
        let mut source = batch_source(Vec::<Vec<SqlStatement>>::new());
        let error = execute_statement_batches_with_policy(
            vec![stmt("INSERT")],
            &mut source,
            &mut exec,
            None,
            ConflictPolicy::Abort,
        )
        .await
        .unwrap_err();

        assert!(
            matches!(error, DataSyncError::NotStarted(message) if message.contains("no changes were applied"))
        );
        assert_eq!(exec.0, vec!["begin"]);
    }

    #[tokio::test]
    async fn test_tester_stream_skips_empty_generated_batches_and_continues_in_order() {
        let mut exec = RecordingExecutor::default();
        let mut source = VecBatchSource(VecDeque::from([
            Ok(Some(Vec::new())),
            Ok(Some(vec![stmt("page-2")])),
            Ok(None),
        ]));
        let result = execute_statement_batches_with_policy(
            vec![stmt("page-1")],
            &mut source,
            &mut exec,
            None,
            ConflictPolicy::Abort,
        )
        .await
        .unwrap();

        assert_eq!(result.applied, 2);
        assert_eq!(
            exec.calls,
            vec!["begin", "execute:1:page-1", "execute:1:page-2", "commit"]
        );
    }

    #[tokio::test]
    async fn commits_all_statements() {
        let mut exec = RecordingExecutor::default();
        let result = execute_statements(&[stmt("INSERT 1"), stmt("INSERT 2")], &mut exec, None)
            .await
            .unwrap();
        assert_eq!(result.applied, 2);
        assert_eq!(result.affected_rows, 2);
        assert!(!result.rolled_back);
        assert_eq!(
            exec.calls,
            vec![
                "begin".to_string(),
                "execute:1:INSERT 1".into(),
                "execute:1:INSERT 2".into(),
                "commit".into(),
            ]
        );
    }

    #[tokio::test]
    async fn identity_insert_is_cleaned_for_non_batched_execution() {
        let mut exec = RecordingExecutor::default();
        let result = execute_statements(&[identity_stmt("INSERT", "users")], &mut exec, None)
            .await
            .unwrap();

        assert!(!result.rolled_back);
        assert_eq!(
            exec.calls,
            vec![
                "begin",
                "identity:on:db:dbo:users",
                "execute:1:INSERT",
                "identity:off:db:dbo:users",
                "commit"
            ]
        );
    }

    #[tokio::test]
    async fn commit_response_loss_is_unknown_whether_commit_reached_the_server_or_not() {
        struct CommitResponseLostExecutor {
            commit_reached_server: bool,
            server_committed: Arc<AtomicBool>,
        }
        #[async_trait]
        impl StatementExecutor for CommitResponseLostExecutor {
            fn is_read_only(&self) -> bool {
                false
            }
            async fn begin(&mut self) -> Result<(), DataSyncError> {
                Ok(())
            }
            async fn execute(&mut self, _: &str, _: &[Value]) -> Result<u64, DataSyncError> {
                Ok(1)
            }
            async fn commit(&mut self) -> Result<(), DataSyncError> {
                if self.commit_reached_server {
                    self.server_committed.store(true, Ordering::SeqCst);
                }
                Err(DataSyncError::validation("commit response lost"))
            }
            async fn rollback(&mut self) -> Result<(), DataSyncError> {
                Ok(())
            }
        }

        for commit_reached_server in [false, true] {
            for use_batches in [false, true] {
                let server_committed = Arc::new(AtomicBool::new(false));
                let mut executor = CommitResponseLostExecutor {
                    commit_reached_server,
                    server_committed: server_committed.clone(),
                };
                let mut source = batch_source(Vec::<Vec<SqlStatement>>::new());
                let result = if use_batches {
                    execute_statement_batches_with_policy(
                        vec![stmt("INSERT")],
                        &mut source,
                        &mut executor,
                        None,
                        ConflictPolicy::Abort,
                    )
                    .await
                } else {
                    execute_statements(&[stmt("INSERT")], &mut executor, None).await
                };
                let error = result.unwrap_err();
                assert!(matches!(error, DataSyncError::OutcomeUnknown(_)));
                assert!(error.to_string().contains("outcome UNKNOWN"));
                assert_eq!(
                    server_committed.load(Ordering::SeqCst),
                    commit_reached_server,
                    "fake server outcome is not evidence available to the caller"
                );
            }
        }
    }

    #[test]
    fn execution_response_error_fields_never_echo_credentials_or_paths() {
        let sensitive = "mysql://root:super-secret@localhost/db /Users/alice/private.sql";
        let not_started = DataSyncExecutionResponse::failed_before_start(sensitive);
        let unknown = DataSyncExecutionResponse::unknown(sensitive);

        for response in [not_started, unknown] {
            let error = response.error.expect("safe user-facing message");
            assert!(!error.contains("super-secret"));
            assert!(!error.contains("/Users/alice"));
            assert!(!error.contains("mysql://"));
        }
    }

    #[test]
    fn affected_rows_is_wire_compatible_with_older_results() {
        let old: ExecutionResult =
            serde_json::from_str(r#"{"applied":2,"rolledBack":false}"#).unwrap();
        assert_eq!(old.affected_rows, 0);
        let json = serde_json::to_value(ExecutionResult {
            applied: 1,
            rolled_back: false,
            rollback_reason: None,
            affected_rows: 3,
            skipped: 0,
            conflicts: Vec::new(),
        })
        .unwrap();
        assert_eq!(json["affectedRows"], 3);
    }

    #[tokio::test]
    async fn read_only_never_begins() {
        let mut exec = RecordingExecutor {
            read_only: true,
            ..RecordingExecutor::default()
        };
        let err = execute_statements(&[stmt("INSERT")], &mut exec, None)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("read-only"));
        assert!(exec.calls.is_empty());
    }

    #[tokio::test]
    async fn empty_set_rejected() {
        let mut exec = RecordingExecutor::default();
        assert!(execute_statements(&[], &mut exec, None).await.is_err());
    }

    #[tokio::test]
    async fn failure_rolls_back() {
        let mut exec = RecordingExecutor {
            fail_at: Some(1),
            ..RecordingExecutor::default()
        };
        let result = execute_statements(&[stmt("A"), stmt("B")], &mut exec, None)
            .await
            .unwrap();
        assert!(result.rolled_back);
        assert!(result
            .rollback_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("execution failed after 1")));
        assert!(exec.calls.contains(&"rollback".to_string()));
        assert!(!exec.calls.contains(&"commit".to_string()));
    }

    #[tokio::test]
    async fn zero_row_update_is_a_conflict_and_rolls_back() {
        struct ZeroRowsExecutor {
            calls: Vec<String>,
        }
        #[async_trait]
        impl StatementExecutor for ZeroRowsExecutor {
            fn is_read_only(&self) -> bool {
                false
            }
            async fn begin(&mut self) -> Result<(), DataSyncError> {
                self.calls.push("begin".into());
                Ok(())
            }
            async fn execute(&mut self, _: &str, _: &[Value]) -> Result<u64, DataSyncError> {
                self.calls.push("execute".into());
                Ok(0)
            }
            async fn commit(&mut self) -> Result<(), DataSyncError> {
                self.calls.push("commit".into());
                Ok(())
            }
            async fn rollback(&mut self) -> Result<(), DataSyncError> {
                self.calls.push("rollback".into());
                Ok(())
            }
        }
        let mut exec = ZeroRowsExecutor { calls: Vec::new() };
        let mut update = stmt("UPDATE t");
        update.operation = ChangeOperation::Update;
        let result = execute_statements(&[update], &mut exec, None)
            .await
            .unwrap();
        assert!(result.rolled_back);
        assert!(result
            .rollback_reason
            .as_deref()
            .is_some_and(|message| message.contains("zero rows")));
        assert_eq!(result.conflicts.len(), 1);
        assert_eq!(exec.calls, vec!["begin", "execute", "rollback"]);
    }

    #[tokio::test]
    async fn failed_rollback_keeps_the_write_outcome_unknown() {
        struct RollbackFailureExecutor;
        #[async_trait]
        impl StatementExecutor for RollbackFailureExecutor {
            fn is_read_only(&self) -> bool {
                false
            }
            async fn begin(&mut self) -> Result<(), DataSyncError> {
                Ok(())
            }
            async fn execute(&mut self, _: &str, _: &[Value]) -> Result<u64, DataSyncError> {
                Ok(0)
            }
            async fn commit(&mut self) -> Result<(), DataSyncError> {
                panic!("a conflicted statement must never commit")
            }
            async fn rollback(&mut self) -> Result<(), DataSyncError> {
                Err(DataSyncError::validation("connection lost during rollback"))
            }
        }

        let mut update = stmt("UPDATE t");
        update.operation = ChangeOperation::Update;
        let error = execute_statements(&[update], &mut RollbackFailureExecutor, None)
            .await
            .unwrap_err();
        assert!(error
            .to_string()
            .contains("rollback failed, outcome UNKNOWN"));
    }

    #[tokio::test]
    async fn skip_policy_commits_other_rows_and_reports_conflicts() {
        struct MixedRowsExecutor {
            calls: Vec<String>,
            executions: usize,
        }
        #[async_trait]
        impl StatementExecutor for MixedRowsExecutor {
            fn is_read_only(&self) -> bool {
                false
            }
            async fn begin(&mut self) -> Result<(), DataSyncError> {
                self.calls.push("begin".into());
                Ok(())
            }
            async fn execute(&mut self, _: &str, _: &[Value]) -> Result<u64, DataSyncError> {
                self.calls.push("execute".into());
                let affected = if self.executions == 0 { 0 } else { 1 };
                self.executions += 1;
                Ok(affected)
            }
            async fn commit(&mut self) -> Result<(), DataSyncError> {
                self.calls.push("commit".into());
                Ok(())
            }
            async fn rollback(&mut self) -> Result<(), DataSyncError> {
                self.calls.push("rollback".into());
                Ok(())
            }
        }
        let mut first = stmt("UPDATE first");
        first.operation = ChangeOperation::Update;
        first.row_key = vec![Value::Integer(1)];
        let mut second = stmt("UPDATE second");
        second.operation = ChangeOperation::Update;
        second.row_key = vec![Value::Integer(2)];
        let mut exec = MixedRowsExecutor {
            calls: Vec::new(),
            executions: 0,
        };
        let result =
            execute_statements_with_policy(&[first, second], &mut exec, None, ConflictPolicy::Skip)
                .await
                .unwrap();
        assert_eq!(result.applied, 1);
        assert_eq!(result.skipped, 1);
        assert_eq!(result.conflicts.len(), 1);
        assert!(matches!(
            result.conflicts[0].row_key.as_slice(),
            [Value::Integer(1)]
        ));
        assert_eq!(exec.calls, vec!["begin", "execute", "execute", "commit"]);
    }

    #[tokio::test]
    async fn force_policy_still_aborts_on_missing_primary_key_row() {
        struct ZeroRowsExecutor;
        #[async_trait]
        impl StatementExecutor for ZeroRowsExecutor {
            fn is_read_only(&self) -> bool {
                false
            }
            async fn begin(&mut self) -> Result<(), DataSyncError> {
                Ok(())
            }
            async fn execute(&mut self, _: &str, _: &[Value]) -> Result<u64, DataSyncError> {
                Ok(0)
            }
            async fn commit(&mut self) -> Result<(), DataSyncError> {
                panic!("must not commit")
            }
            async fn rollback(&mut self) -> Result<(), DataSyncError> {
                Ok(())
            }
        }
        let mut update = stmt("UPDATE t");
        update.operation = ChangeOperation::Update;
        let result = execute_statements_with_policy(
            &[update],
            &mut ZeroRowsExecutor,
            None,
            ConflictPolicy::Force,
        )
        .await
        .unwrap();
        assert!(result.rolled_back);
        assert_eq!(result.conflicts.len(), 1);
    }

    #[tokio::test]
    async fn force_policy_does_not_skip_insert_errors() {
        let mut exec = RecordingExecutor {
            fail_at: Some(0),
            ..RecordingExecutor::default()
        };
        let result = execute_statements_with_policy(
            &[stmt("INSERT duplicate")],
            &mut exec,
            None,
            ConflictPolicy::Force,
        )
        .await
        .unwrap();
        assert!(result.rolled_back);
        assert!(result
            .rollback_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("execution failed")));
        assert!(exec.calls.contains(&"rollback".to_string()));
        assert!(!exec.calls.contains(&"commit".to_string()));
    }

    #[tokio::test]
    async fn cancel_before_start() {
        let mut exec = RecordingExecutor::default();
        let flag = Arc::new(AtomicBool::new(true));
        let error = execute_statements(&[stmt("A")], &mut exec, Some(flag))
            .await
            .unwrap_err();
        assert!(
            matches!(error, DataSyncError::NotStarted(message) if message.contains("before any changes"))
        );
        assert!(exec.calls.is_empty());
    }

    #[tokio::test]
    async fn begin_failure_confirms_no_statements_were_applied() {
        struct BeginFailureExecutor;
        #[async_trait]
        impl StatementExecutor for BeginFailureExecutor {
            fn is_read_only(&self) -> bool {
                false
            }
            async fn begin(&mut self) -> Result<(), DataSyncError> {
                Err(DataSyncError::validation("database rejected transaction"))
            }
            async fn execute(&mut self, _: &str, _: &[Value]) -> Result<u64, DataSyncError> {
                panic!("failed begin must prevent statement execution")
            }
            async fn commit(&mut self) -> Result<(), DataSyncError> {
                panic!("failed begin must prevent commit")
            }
            async fn rollback(&mut self) -> Result<(), DataSyncError> {
                panic!("failed begin must not attempt rollback")
            }
        }

        let error = execute_statements(&[stmt("INSERT")], &mut BeginFailureExecutor, None)
            .await
            .unwrap_err();
        assert!(
            matches!(error, DataSyncError::NotStarted(message) if message.contains("no changes were applied"))
        );
    }

    #[tokio::test]
    async fn cancel_mid_run_rolls_back() {
        let _exec = RecordingExecutor::default();
        let flag = Arc::new(AtomicBool::new(false));
        // First execute will run; flip cancel after begin by wrapping — simulate mid-loop
        // by setting flag after begin via fail path: set flag true before second statement
        // Use a custom executor... simpler: set flag true immediately after we know begin ran
        // by using fail_at None and pre-set flag after constructing, then...
        // Mid-loop: start false, but RecordingExecutor can't flip. Set flag true after first
        // execute by using a second test executor. Here we set flag true before call then
        // that's cancel_before_start. For mid-run, toggle after begin using a helper executor.
        struct FlipOnExecute {
            inner: RecordingExecutor,
            flag: Arc<AtomicBool>,
        }
        #[async_trait]
        impl StatementExecutor for FlipOnExecute {
            fn is_read_only(&self) -> bool {
                false
            }
            async fn begin(&mut self) -> Result<(), DataSyncError> {
                self.inner.begin().await
            }
            async fn execute(&mut self, sql: &str, params: &[Value]) -> Result<u64, DataSyncError> {
                let r = self.inner.execute(sql, params).await;
                self.flag.store(true, Ordering::SeqCst);
                r
            }
            async fn commit(&mut self) -> Result<(), DataSyncError> {
                self.inner.commit().await
            }
            async fn rollback(&mut self) -> Result<(), DataSyncError> {
                self.inner.rollback().await
            }
        }
        let mut exec = FlipOnExecute {
            inner: RecordingExecutor::default(),
            flag: flag.clone(),
        };
        let result = execute_statements(&[stmt("A"), stmt("B")], &mut exec, Some(flag))
            .await
            .unwrap();
        assert!(result.rolled_back);
        assert_eq!(result.applied, 1);
        assert_eq!(result.affected_rows, 1);
        assert!(exec.inner.calls.contains(&"rollback".to_string()));
    }

    #[tokio::test]
    async fn statement_batches_preserve_order_across_page_boundaries() {
        let mut exec = RecordingExecutor::default();
        let mut source = batch_source(vec![vec![stmt("page-2-a"), stmt("page-2-b")]]);
        let result = execute_statement_batches_with_policy(
            vec![stmt("page-1")],
            &mut source,
            &mut exec,
            None,
            ConflictPolicy::Abort,
        )
        .await
        .unwrap();

        assert_eq!(result.applied, 3);
        assert_eq!(
            exec.calls,
            vec![
                "begin",
                "execute:1:page-1",
                "execute:1:page-2-a",
                "execute:1:page-2-b",
                "commit"
            ]
        );
    }

    #[tokio::test]
    async fn identity_insert_stays_enabled_across_pages_and_closes_before_commit() {
        let mut exec = RecordingExecutor::default();
        let mut source = batch_source(vec![vec![identity_stmt("page-2", "users")]]);
        let result = execute_statement_batches_with_policy(
            vec![identity_stmt("page-1", "users")],
            &mut source,
            &mut exec,
            None,
            ConflictPolicy::Abort,
        )
        .await
        .unwrap();

        assert!(!result.rolled_back);
        assert_eq!(
            exec.calls,
            vec![
                "begin",
                "identity:on:db:dbo:users",
                "execute:1:page-1",
                "execute:1:page-2",
                "identity:off:db:dbo:users",
                "commit"
            ]
        );
    }

    #[tokio::test]
    async fn identity_insert_switches_tables_only_after_turning_the_previous_table_off() {
        let mut exec = RecordingExecutor::default();
        let mut source = batch_source(vec![vec![identity_stmt("second", "orders")]]);
        let result = execute_statement_batches_with_policy(
            vec![identity_stmt("first", "users")],
            &mut source,
            &mut exec,
            None,
            ConflictPolicy::Abort,
        )
        .await
        .unwrap();

        assert!(!result.rolled_back);
        assert_eq!(
            exec.calls,
            vec![
                "begin",
                "identity:on:db:dbo:users",
                "execute:1:first",
                "identity:off:db:dbo:users",
                "identity:on:db:dbo:orders",
                "execute:1:second",
                "identity:off:db:dbo:orders",
                "commit"
            ]
        );
    }

    #[tokio::test]
    async fn identity_on_failure_attempts_off_then_rolls_back_without_dml() {
        let mut exec = RecordingExecutor {
            fail_identity_toggle: Some(("users".into(), true)),
            ..RecordingExecutor::default()
        };
        let mut source = batch_source(Vec::<Vec<SqlStatement>>::new());
        let result = execute_statement_batches_with_policy(
            vec![identity_stmt("must-not-run", "users")],
            &mut source,
            &mut exec,
            None,
            ConflictPolicy::Abort,
        )
        .await
        .unwrap();

        assert!(result.rolled_back);
        assert_eq!(
            exec.calls,
            vec![
                "begin",
                "identity:on:db:dbo:users",
                "identity:off:db:dbo:users",
                "rollback"
            ]
        );
    }

    #[tokio::test]
    async fn identity_off_failure_rolls_back_then_discards_the_connection() {
        let mut exec = RecordingExecutor {
            fail_identity_toggle: Some(("users".into(), false)),
            ..RecordingExecutor::default()
        };
        let mut source = batch_source(Vec::<Vec<SqlStatement>>::new());
        let result = execute_statement_batches_with_policy(
            vec![identity_stmt("insert", "users")],
            &mut source,
            &mut exec,
            None,
            ConflictPolicy::Abort,
        )
        .await
        .unwrap();

        assert!(result.rolled_back);
        assert!(result.rollback_reason.as_deref().is_some_and(|reason| {
            reason.contains("identity insert cleanup failed")
                && reason.contains("target connection discarded")
        }));
        assert_eq!(
            exec.calls,
            vec![
                "begin",
                "identity:on:db:dbo:users",
                "execute:1:insert",
                "identity:off:db:dbo:users",
                "rollback",
                "discard"
            ]
        );
    }

    #[tokio::test]
    async fn identity_dml_failure_turns_mode_off_before_rollback() {
        let mut exec = RecordingExecutor {
            fail_at: Some(0),
            ..RecordingExecutor::default()
        };
        let mut source = batch_source(Vec::<Vec<SqlStatement>>::new());
        let result = execute_statement_batches_with_policy(
            vec![identity_stmt("insert-fails", "users")],
            &mut source,
            &mut exec,
            None,
            ConflictPolicy::Abort,
        )
        .await
        .unwrap();

        assert!(result.rolled_back);
        assert_eq!(
            exec.calls,
            vec![
                "begin",
                "identity:on:db:dbo:users",
                "execute:1:insert-fails",
                "identity:off:db:dbo:users",
                "rollback"
            ]
        );
    }

    #[tokio::test]
    async fn identity_source_failure_turns_mode_off_before_rollback() {
        let mut exec = RecordingExecutor::default();
        let mut source = VecBatchSource(VecDeque::from([Err(DataSyncError::validation(
            "injected page generation failure",
        ))]));
        let result = execute_statement_batches_with_policy(
            vec![identity_stmt("insert", "users")],
            &mut source,
            &mut exec,
            None,
            ConflictPolicy::Abort,
        )
        .await
        .unwrap();

        assert!(result.rolled_back);
        assert_eq!(
            exec.calls,
            vec![
                "begin",
                "identity:on:db:dbo:users",
                "execute:1:insert",
                "identity:off:db:dbo:users",
                "rollback"
            ]
        );
    }

    #[tokio::test]
    async fn identity_insert_is_cleaned_on_cancellation_after_the_last_page() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut exec = RecordingExecutor {
            cancel_after_execute: Some(cancelled.clone()),
            ..RecordingExecutor::default()
        };
        let mut source = batch_source(Vec::<Vec<SqlStatement>>::new());
        let result = execute_statement_batches_with_policy(
            vec![identity_stmt("insert", "users")],
            &mut source,
            &mut exec,
            Some(cancelled),
            ConflictPolicy::Abort,
        )
        .await
        .unwrap();

        assert!(result.rolled_back);
        assert_eq!(
            exec.calls,
            vec![
                "begin",
                "identity:on:db:dbo:users",
                "execute:1:insert",
                "identity:off:db:dbo:users",
                "rollback"
            ]
        );
    }

    #[tokio::test]
    async fn optimistic_conflict_disables_identity_mode_before_rollback() {
        let mut update = stmt("update");
        update.operation = ChangeOperation::Update;
        let mut exec = RecordingExecutor {
            zero_at: Some(1),
            ..RecordingExecutor::default()
        };
        let mut source = batch_source(Vec::<Vec<SqlStatement>>::new());
        let result = execute_statement_batches_with_policy(
            vec![identity_stmt("insert", "users"), update],
            &mut source,
            &mut exec,
            None,
            ConflictPolicy::Abort,
        )
        .await
        .unwrap();

        assert!(result.rolled_back);
        assert_eq!(
            exec.calls,
            vec![
                "begin",
                "identity:on:db:dbo:users",
                "execute:1:insert",
                "identity:off:db:dbo:users",
                "execute:1:update",
                "rollback"
            ]
        );
    }

    #[tokio::test]
    async fn identity_cleanup_failure_and_failed_discard_report_unknown() {
        let mut exec = RecordingExecutor {
            fail_identity_toggle: Some(("users".into(), false)),
            fail_discard: true,
            ..RecordingExecutor::default()
        };
        let mut source = batch_source(Vec::<Vec<SqlStatement>>::new());
        let error = execute_statement_batches_with_policy(
            vec![identity_stmt("insert", "users")],
            &mut source,
            &mut exec,
            None,
            ConflictPolicy::Abort,
        )
        .await
        .unwrap_err();

        assert!(error.to_string().contains("session state UNKNOWN"));
        assert_eq!(
            exec.calls,
            vec![
                "begin",
                "identity:on:db:dbo:users",
                "execute:1:insert",
                "identity:off:db:dbo:users",
                "rollback",
                "discard"
            ]
        );
    }

    #[tokio::test]
    async fn statement_source_error_after_a_page_rolls_back_prior_writes() {
        let mut exec = RecordingExecutor::default();
        let mut source = VecBatchSource(VecDeque::from([Err(DataSyncError::validation(
            "injected later page failure",
        ))]));
        let result = execute_statement_batches_with_policy(
            vec![stmt("page-1")],
            &mut source,
            &mut exec,
            None,
            ConflictPolicy::Abort,
        )
        .await
        .unwrap();

        assert!(result.rolled_back);
        assert_eq!(result.applied, 1);
        assert!(result
            .rollback_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("statement generation failed after 1")));
        assert!(exec.calls.contains(&"rollback".to_string()));
        assert!(!exec.calls.contains(&"commit".to_string()));
    }

    #[tokio::test]
    async fn test_tester_later_batch_statement_failure_rolls_back_prior_page() {
        let mut exec = RecordingExecutor {
            fail_at: Some(1),
            ..RecordingExecutor::default()
        };
        let mut source = batch_source(vec![vec![stmt("page-2")]]);
        let result = execute_statement_batches_with_policy(
            vec![stmt("page-1")],
            &mut source,
            &mut exec,
            None,
            ConflictPolicy::Abort,
        )
        .await
        .expect("a later-page statement failure should be reported as a confirmed rollback");

        assert!(result.rolled_back);
        assert_eq!(result.applied, 1);
        assert!(result
            .rollback_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("execution failed after 1 statements")));
        assert_eq!(
            exec.calls,
            vec!["begin", "execute:1:page-1", "execute:1:page-2", "rollback"]
        );
    }

    #[tokio::test]
    async fn later_page_failure_with_rollback_failure_reports_unknown_outcome() {
        struct RollbackFailureExecutor(RecordingExecutor);

        #[async_trait]
        impl StatementExecutor for RollbackFailureExecutor {
            fn is_read_only(&self) -> bool {
                false
            }
            async fn begin(&mut self) -> Result<(), DataSyncError> {
                self.0.begin().await
            }
            async fn execute(&mut self, sql: &str, params: &[Value]) -> Result<u64, DataSyncError> {
                self.0.execute(sql, params).await
            }
            async fn commit(&mut self) -> Result<(), DataSyncError> {
                self.0.commit().await
            }
            async fn rollback(&mut self) -> Result<(), DataSyncError> {
                self.0.calls.push("rollback-failed".into());
                Err(DataSyncError::validation("injected rollback failure"))
            }
        }

        let mut exec = RollbackFailureExecutor(RecordingExecutor::default());
        let mut source = VecBatchSource(VecDeque::from([Err(DataSyncError::validation(
            "injected later page failure",
        ))]));
        let error = execute_statement_batches_with_policy(
            vec![stmt("page-1")],
            &mut source,
            &mut exec,
            None,
            ConflictPolicy::Abort,
        )
        .await
        .unwrap_err();

        assert!(error.to_string().contains("outcome UNKNOWN"));
        assert!(error.to_string().contains("injected rollback failure"));
        assert_eq!(
            exec.0.calls.last().map(String::as_str),
            Some("rollback-failed")
        );
    }

    #[tokio::test]
    async fn cancellation_between_statement_pages_rolls_back_prior_page() {
        struct FlipOnExecute {
            inner: RecordingExecutor,
            flag: Arc<AtomicBool>,
        }
        #[async_trait]
        impl StatementExecutor for FlipOnExecute {
            fn is_read_only(&self) -> bool {
                false
            }
            async fn begin(&mut self) -> Result<(), DataSyncError> {
                self.inner.begin().await
            }
            async fn execute(&mut self, sql: &str, params: &[Value]) -> Result<u64, DataSyncError> {
                let result = self.inner.execute(sql, params).await;
                self.flag.store(true, Ordering::SeqCst);
                result
            }
            async fn commit(&mut self) -> Result<(), DataSyncError> {
                self.inner.commit().await
            }
            async fn rollback(&mut self) -> Result<(), DataSyncError> {
                self.inner.rollback().await
            }
        }

        let flag = Arc::new(AtomicBool::new(false));
        let mut exec = FlipOnExecute {
            inner: RecordingExecutor::default(),
            flag: flag.clone(),
        };
        let mut source = batch_source(vec![vec![stmt("page-2")]]);
        let result = execute_statement_batches_with_policy(
            vec![stmt("page-1")],
            &mut source,
            &mut exec,
            Some(flag),
            ConflictPolicy::Abort,
        )
        .await
        .unwrap();

        assert!(result.rolled_back);
        assert_eq!(result.applied, 1);
        assert!(exec.inner.calls.contains(&"rollback".to_string()));
        assert!(!exec.inner.calls.contains(&"execute:1:page-2".to_string()));
    }

    #[tokio::test]
    async fn streamed_skip_policy_reports_conflict_and_commits_following_page() {
        struct MixedRowsExecutor {
            calls: Vec<String>,
            executions: usize,
        }
        #[async_trait]
        impl StatementExecutor for MixedRowsExecutor {
            fn is_read_only(&self) -> bool {
                false
            }
            async fn begin(&mut self) -> Result<(), DataSyncError> {
                self.calls.push("begin".into());
                Ok(())
            }
            async fn execute(&mut self, _: &str, _: &[Value]) -> Result<u64, DataSyncError> {
                self.calls.push("execute".into());
                let affected = if self.executions == 0 { 0 } else { 1 };
                self.executions += 1;
                Ok(affected)
            }
            async fn commit(&mut self) -> Result<(), DataSyncError> {
                self.calls.push("commit".into());
                Ok(())
            }
            async fn rollback(&mut self) -> Result<(), DataSyncError> {
                self.calls.push("rollback".into());
                Ok(())
            }
        }

        let mut update = stmt("UPDATE first");
        update.operation = ChangeOperation::Update;
        let mut followup = stmt("UPDATE second");
        followup.operation = ChangeOperation::Update;
        let mut source = batch_source(vec![vec![followup]]);
        let mut exec = MixedRowsExecutor {
            calls: Vec::new(),
            executions: 0,
        };
        let result = execute_statement_batches_with_policy(
            vec![update],
            &mut source,
            &mut exec,
            None,
            ConflictPolicy::Skip,
        )
        .await
        .unwrap();

        assert!(!result.rolled_back);
        assert_eq!(result.applied, 1);
        assert_eq!(result.skipped, 1);
        assert_eq!(result.conflicts.len(), 1);
        assert_eq!(exec.calls, vec!["begin", "execute", "execute", "commit"]);
    }
}
