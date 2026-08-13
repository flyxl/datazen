//! Dedicated Data Sync execute IPC (bypasses sql_guard / execute_query).

use super::super::error::{CmdExt, CommandError};
use super::super::AppState;
use crate::data_sync::{execute_statements, ExecutionResult, SqlStatement, StatementExecutor};
use crate::db::{ConnectionHandle, DatabaseDriver, TransactionHandle, Value};
use async_trait::async_trait;
use std::sync::Arc;

struct LiveExecutor {
    driver: Arc<dyn DatabaseDriver>,
    handle: ConnectionHandle,
    read_only: bool,
    tx: Option<TransactionHandle>,
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
            .query_with_params(&self.handle, sql, params)
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
    target_connection_id: String,
    statements: Vec<SqlStatement>,
    job_id: Option<String>,
) -> Result<ExecutionResult, CommandError> {
    let config = state
        .connection_manager
        .get_connection_config(&target_connection_id)
        .await
        .cmd_err("execute_data_sync")?;
    let (driver, handle) = state
        .connection_manager
        .get_connection(&target_connection_id)
        .await
        .cmd_err("execute_data_sync")?;
    let mut executor = LiveExecutor {
        driver,
        handle,
        read_only: config.read_only,
        tx: None,
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
