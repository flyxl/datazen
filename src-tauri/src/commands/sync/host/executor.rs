//! Driver-backed implementations of the two handler ports that own a live
//! lease: the keyset reader and the batch target executor.
//!
//! Both hold the `Arc<AtomicBool>` handed down by
//! [`CancelToken::flag`](datazen_runtime::job::CancelToken::flag) — that is the
//! kernel's own bit, not a copy of it, so a cancel recorded by
//! `JobRepository::request_cancel` reaches these checkpoints while the stage is
//! still running (runtime.rs:362 `watch_cancel_request`).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use datazen_driver_api::{
    ConnectionHandle, DatabaseDriver, SyncKeyValue, TransactionHandle, Value,
};

use crate::data_sync::job::artifact::RelationIdentity;
use crate::data_sync::job::host::{KeysetPageSource, TargetExecutor};
use crate::data_sync::{DataSyncError, Row, RowPageSource, SqlStatement};

use super::super::keyset_source::DriverKeysetSource;

/// `DriverKeysetSource` adapted to the handler port. The port is foreign to
/// the host, so the existing driver reader is wrapped instead of rewritten.
pub(crate) struct HostKeysetSource {
    inner: DriverKeysetSource,
    cancel: Arc<AtomicBool>,
}

impl HostKeysetSource {
    pub(crate) fn new(inner: DriverKeysetSource, cancel: Arc<AtomicBool>) -> Self {
        Self { inner, cancel }
    }
}

#[async_trait]
impl KeysetPageSource for HostKeysetSource {
    async fn next_page(
        &mut self,
        after_key: Option<&[Value]>,
        limit: u32,
    ) -> Result<Vec<Row>, DataSyncError> {
        // A long keyset walk has to stop between pages, so the page loop checks
        // the same bit the runtime flips (§5.3).
        if self.cancel.load(Ordering::SeqCst) {
            return Err(cancelled("compare cancelled before page"));
        }
        self.inner.next_page(after_key, limit).await
    }

    fn normalize_key(&self, key: &[Value]) -> Result<Vec<SyncKeyValue>, DataSyncError> {
        self.inner.normalize_key(key)
    }
}

/// One fixed target lease for the whole apply Job: every batch begins,
/// verifies, writes and commits on this single session (§5.3).
pub(crate) struct HostTargetExecutor {
    driver: Arc<dyn DatabaseDriver>,
    handle: ConnectionHandle,
    tx: Option<TransactionHandle>,
    cancel: Arc<AtomicBool>,
    /// Precomputed `SELECT … WHERE pk IS NOT NULL` per table name.
    read_by_key_sql: std::collections::HashMap<String, String>,
}

impl HostTargetExecutor {
    pub(crate) fn new(
        driver: Arc<dyn DatabaseDriver>,
        handle: ConnectionHandle,
        cancel: Arc<AtomicBool>,
        read_by_key_sql: std::collections::HashMap<String, String>,
    ) -> Self {
        Self {
            driver,
            handle,
            tx: None,
            cancel,
            read_by_key_sql,
        }
    }
}

/// A cancel observed before the commit point still rolls back only the
/// current batch: these checkpoints read the kernel's own bit, which the
/// per-stage cancel watcher flips mid-run (§5.3).
fn cancelled(what: &str) -> DataSyncError {
    DataSyncError::cancelled(format!("execute cancelled: {what}"))
}

#[async_trait]
impl TargetExecutor for HostTargetExecutor {
    async fn begin(&mut self) -> Result<(), DataSyncError> {
        self.tx = Some(
            self.driver
                .begin_transaction(&self.handle)
                .await
                .map_err(|error| DataSyncError::outcome_unknown(format!("begin: {error}")))?,
        );
        Ok(())
    }

    async fn read_by_key(
        &mut self,
        relation: &RelationIdentity,
        key: &[Value],
    ) -> Result<Option<Row>, DataSyncError> {
        if self.cancel.load(Ordering::SeqCst) {
            return Err(cancelled("batch cancelled before verify"));
        }
        let sql = self
            .read_by_key_sql
            .get(&relation.table)
            .ok_or_else(|| {
                DataSyncError::validation(
                    "target relation metadata missing; compare again to refresh the plan",
                )
            })?
            .clone();
        let result = self
            .driver
            .query_with_params(&self.handle, &sql, key)
            .await
            .map_err(|error| DataSyncError::outcome_unknown(format!("read_by_key: {error}")))?;
        Ok(result.rows.into_iter().next())
    }

    async fn execute(&mut self, statement: &SqlStatement) -> Result<u64, DataSyncError> {
        if self.cancel.load(Ordering::SeqCst) {
            return Err(cancelled("batch cancelled before write"));
        }
        if let Some(target) = statement.identity_insert.as_ref() {
            self.driver
                .set_identity_insert(
                    &self.handle,
                    &target.database,
                    target.schema.as_deref(),
                    &target.table,
                    true,
                )
                .await
                .map_err(|error| DataSyncError::validation(error.to_string()))?;
        }
        let affected = self
            .driver
            .execute_with_params(&self.handle, &statement.sql, &statement.parameters)
            .await
            .map_err(|error| DataSyncError::outcome_unknown(format!("execute: {error}")))?;
        if let Some(target) = statement.identity_insert.as_ref() {
            let _ = self
                .driver
                .set_identity_insert(
                    &self.handle,
                    &target.database,
                    target.schema.as_deref(),
                    &target.table,
                    false,
                )
                .await;
        }
        Ok(affected)
    }

    async fn commit(&mut self) -> Result<(), DataSyncError> {
        if let Some(tx) = self.tx.take() {
            #[cfg(feature = "webdriver")]
            match super::super::exec::take_e2e_commit_fault() {
                1 => {
                    self.driver.commit(tx).await.map_err(|error| {
                        DataSyncError::outcome_unknown(format!("commit: {error}"))
                    })?;
                    return Err(DataSyncError::outcome_unknown(
                        "E2E injected commit response loss after commit",
                    ));
                }
                2 => {
                    let _ = self.driver.rollback(tx).await;
                    return Err(DataSyncError::outcome_unknown(
                        "E2E injected transaction boundary response loss before commit",
                    ));
                }
                _ => {}
            }
            self.driver
                .commit(tx)
                .await
                .map_err(|error| DataSyncError::outcome_unknown(format!("commit: {error}")))?;
        }
        Ok(())
    }

    async fn rollback(&mut self) -> Result<(), DataSyncError> {
        if let Some(tx) = self.tx.take() {
            self.driver
                .rollback(tx)
                .await
                .map_err(|error| DataSyncError::validation(error.to_string()))?;
        }
        Ok(())
    }

    async fn discard(&mut self) -> Result<(), DataSyncError> {
        self.tx = None;
        self.driver
            .discard_connection(&self.handle)
            .await
            .map_err(|error| DataSyncError::validation(error.to_string()))
    }
}
