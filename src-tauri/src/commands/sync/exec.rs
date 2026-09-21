//! Dedicated Data Sync execute IPC (bypasses sql_guard / execute_query).

use super::super::error::{CmdExt, CommandError};
use super::super::AppState;
use crate::data_sync::{execute_statements, ExecutionResult, SqlStatement, StatementExecutor};
use crate::db::{ConnectionHandle, DatabaseDriver, SqlTarget, TransactionHandle, Value};
use async_trait::async_trait;
use std::sync::Arc;

struct LiveExecutor {
    driver: Arc<dyn DatabaseDriver>,
    handle: ConnectionHandle,
    read_only: bool,
    tx: Option<TransactionHandle>,
    /// The database this sync writes into. Statements carry it so a driver
    /// whose connection is scoped to one database (PostgreSQL) resolves the
    /// right catalog — and so an open transaction bound to a *different*
    /// database fails loudly instead of writing to the wrong one.
    target_database: Option<String>,
    target_schema: Option<String>,
}

#[async_trait]
impl StatementExecutor for LiveExecutor {
    fn is_read_only(&self) -> bool {
        self.read_only
    }

    async fn begin(&mut self) -> Result<(), crate::data_sync::DataSyncError> {
        let tx = self
            .driver
            .begin_transaction(&self.handle)
            .await
            .map_err(|e| crate::data_sync::DataSyncError::validation(e.to_string()))?;
        self.tx = Some(tx);
        Ok(())
    }

    async fn execute(
        &mut self,
        sql: &str,
        params: &[Value],
    ) -> Result<u64, crate::data_sync::DataSyncError> {
        let result = self
            .driver
            .query_with_params_at(
                &self.handle,
                sql,
                params,
                SqlTarget::new(
                    self.target_database.as_deref(),
                    self.target_schema.as_deref(),
                ),
            )
            .await
            .map_err(|e| crate::data_sync::DataSyncError::validation(e.to_string()))?;
        Ok(result.rows_affected.unwrap_or(1))
    }

    async fn commit(&mut self) -> Result<(), crate::data_sync::DataSyncError> {
        if let Some(tx) = self.tx.take() {
            self.driver
                .commit(tx)
                .await
                .map_err(|e| crate::data_sync::DataSyncError::validation(e.to_string()))?;
        }
        Ok(())
    }

    async fn rollback(&mut self) -> Result<(), crate::data_sync::DataSyncError> {
        if let Some(tx) = self.tx.take() {
            self.driver
                .rollback(tx)
                .await
                .map_err(|e| crate::data_sync::DataSyncError::validation(e.to_string()))?;
        }
        Ok(())
    }
}

pub(crate) async fn execute_data_sync_impl(
    state: &AppState,
    target_db_session_id: String,
    statements: Vec<SqlStatement>,
    job_id: Option<String>,
    target_database: Option<String>,
    target_schema: Option<String>,
) -> Result<ExecutionResult, CommandError> {
    let config = state
        .connection_manager
        .get_session_config(&target_db_session_id)
        .await
        .cmd_err("execute_data_sync")?;
    let (driver, handle) = state
        .connection_manager
        .get_session(&target_db_session_id)
        .await
        .cmd_err("execute_data_sync")?;
    let mut executor = LiveExecutor {
        driver,
        handle,
        read_only: config.read_only,
        tx: None,
        target_database,
        target_schema,
    };
    let cancelled = match job_id.as_deref() {
        Some(id) => Some(super::jobs::ensure_job(id).await),
        None => None,
    };
    let result = execute_statements(&statements, &mut executor, cancelled)
        .await
        .map_err(CommandError::from);
    if let Some(id) = job_id.as_deref() {
        super::jobs::remove_job(id).await;
    }
    result
}
