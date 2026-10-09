//! Failure-recording decorator around the Data Sync host port.
//!
//! The runtime deliberately does not carry handler error text (§2.3: the
//! handler never mutates Job state), and `StageRecord` has no outcome/error
//! field, so the host is the only layer that knows *why* a Job failed. This
//! decorator is the single production write point into `state::FAILURES`,
//! which is what `jobs` reads back to reconstruct the IPC error message
//! instead of inventing one.

use std::sync::Arc;

use async_trait::async_trait;
use datazen_runtime::job::CancelToken;

use crate::data_sync::job::artifact::{ChangeSetArtifact, RelationIdentity};
use crate::data_sync::job::host::{
    ArtifactStoreGuard, DataSyncHost, EndpointSession, KeysetPageSource, TableSchemaPair,
    TargetExecutor, TransactionScope,
};
use crate::data_sync::{
    DataSyncError, Endpoint, RowChange, SqlStatement, SyncOptions, SyncSourceFilter, TableMapping,
};

use super::state;

/// Wraps the real host and persists the last observed rejection per Job.
pub(crate) struct RecordingHost {
    inner: Arc<dyn DataSyncHost>,
    job_id: String,
}

impl RecordingHost {
    pub(crate) fn new(inner: Arc<dyn DataSyncHost>, job_id: impl Into<String>) -> Self {
        Self {
            inner,
            job_id: job_id.into(),
        }
    }

    /// Persist a host rejection, then hand the error back unchanged so the
    /// handler keeps its own failure classification (`NotStarted` vs
    /// `OutcomeUnknown` vs conflict).
    fn record<T>(&self, outcome: Result<T, DataSyncError>) -> Result<T, DataSyncError> {
        if let Err(error) = &outcome {
            state::record_failure(&self.job_id, error.to_string());
        }
        outcome
    }
}

#[async_trait]
impl DataSyncHost for RecordingHost {
    async fn open_endpoint(&self, endpoint: &Endpoint) -> Result<EndpointSession, DataSyncError> {
        self.record(self.inner.open_endpoint(endpoint).await)
    }

    async fn close_endpoint(&self, session: EndpointSession) {
        self.inner.close_endpoint(session).await;
    }

    async fn mapped_table_schemas(
        &self,
        source: &EndpointSession,
        target: &EndpointSession,
        mappings: &[TableMapping],
    ) -> Result<Vec<TableSchemaPair>, DataSyncError> {
        self.record(
            self.inner
                .mapped_table_schemas(source, target, mappings)
                .await,
        )
    }

    async fn table_reader(
        &self,
        session: &EndpointSession,
        table: &str,
        filter: Option<&SyncSourceFilter>,
        cancel: &CancelToken,
    ) -> Result<Box<dyn KeysetPageSource>, DataSyncError> {
        self.record(
            self.inner
                .table_reader(session, table, filter, cancel)
                .await,
        )
    }

    async fn store_artifact(
        &self,
        artifact: ChangeSetArtifact,
    ) -> Result<ArtifactStoreGuard, DataSyncError> {
        self.record(self.inner.store_artifact(artifact).await)
    }

    async fn load_artifact(
        &self,
        plan_id: &str,
    ) -> Result<Option<ChangeSetArtifact>, DataSyncError> {
        self.record(self.inner.load_artifact(plan_id).await)
    }

    async fn selection(
        &self,
        plan_id: &str,
        selection_revision: u64,
    ) -> Result<Vec<crate::data_sync::job::ChangeBlock>, DataSyncError> {
        self.record(self.inner.selection(plan_id, selection_revision).await)
    }

    async fn check_permissions(&self, session: &EndpointSession) -> Result<(), DataSyncError> {
        self.record(self.inner.check_permissions(session).await)
    }

    async fn transaction_scope(
        &self,
        session: &EndpointSession,
    ) -> Result<TransactionScope, DataSyncError> {
        self.record(self.inner.transaction_scope(session).await)
    }

    async fn target_executor(
        &self,
        session: &EndpointSession,
        cancel: &CancelToken,
    ) -> Result<Box<dyn TargetExecutor>, DataSyncError> {
        self.record(self.inner.target_executor(session, cancel).await)
    }

    async fn generate_statements(
        &self,
        relation: &RelationIdentity,
        source_table: &str,
        target_table: &str,
        changes: Vec<RowChange>,
        options: &SyncOptions,
    ) -> Result<Vec<SqlStatement>, DataSyncError> {
        self.record(
            self.inner
                .generate_statements(relation, source_table, target_table, changes, options)
                .await,
        )
    }

    fn now(&self) -> String {
        self.inner.now()
    }

    /// 阶段自身的拒绝（结构门闸、keyset 契约……）也必须落到同一个 Job 的失败
    /// 记录里，否则 runtime 只会交回一个没有任何原因的 `Failed`。
    fn record_stage_failure(&self, stage: &str, reason: String) {
        self.inner.record_stage_failure(stage, reason.clone());
        state::record_failure(&self.job_id, reason);
    }
}
