//! Desktop adapter for the shared Data Sync execution host (W8 刀3e-4a).
//!
//! `HostDataSync` / `HostKeysetSource` / `HostTargetExecutor` / `RecordingHost`
//! 已下沉共享层（`datazen::migration::data_sync::job::runtime_host`——形态
//! 中立的部分一个字不改地共用，桌面与团队服务器同源）。本文件只剩三个
//! 形态端口的桌面适配：
//!
//! - [`DesktopSyncConfigs`] —— 连接档案回读走单租户 `DesktopRepository`
//!   （组织维度由本地组织恒定，忽略 ctx）；
//! - [`DesktopArtifactStore`] —— ChangeSet 工件与已确认选区走进程内静态表
//!   （[`state`] + [`selection`]，语义与下沉前逐字一致）；
//! - [`DesktopFailureSink`] —— 宿主侧失败消息落静态 FAILURES 表。
//!
//! 一条既有纪律随迁不变：一个 Data Sync Job 的整个生命周期只通过一条
//! runtime 会话与其活连接租约对话；取消位由阶段的 `CancelToken` 原样
//! 递给共享宿主（`table_reader` / `target_executor` 收内核那一位）。

pub(crate) mod selection;
pub(crate) mod state;

use std::sync::Arc;

use async_trait::async_trait;

use crate::data_sync::job::artifact::{ChangeBlock, ChangeSetArtifact};
use crate::data_sync::job::host::ArtifactRepositoryGuard;
use crate::data_sync::job::runtime_host::{
    CommitFaultHook, FailureSink, HostConnectionConfigs, RecordingDataSyncHost,
    RuntimeDataSyncHost, SyncArtifactStore,
};
use crate::data_sync::{DataSyncError, Endpoint, SyncSourceFilter};
use crate::store::DesktopRepository;
use datazen::shared::context::RequestContext;
use datazen_driver_api::ConnectionConfig;

/// 单租户连接档案回读（桌面形态端口实现）。
pub(crate) struct DesktopSyncConfigs {
    pub(crate) store: Arc<DesktopRepository>,
}

#[async_trait]
impl HostConnectionConfigs for DesktopSyncConfigs {
    async fn connection_config(
        &self,
        _ctx: &RequestContext,
        connection_id: &str,
    ) -> Option<ConnectionConfig> {
        self.store.get_connection(connection_id).await
    }
}

/// 桌面工件/选区注册表：进程内静态表（语义与下沉前的 `selection.rs` 同源）。
pub(crate) struct DesktopArtifactStore;

#[async_trait]
impl SyncArtifactStore for DesktopArtifactStore {
    async fn store_artifact(
        &self,
        artifact: ChangeSetArtifact,
    ) -> Result<ArtifactRepositoryGuard, DataSyncError> {
        selection::store_artifact(artifact)
    }

    async fn load_artifact(
        &self,
        plan_id: &str,
    ) -> Result<Option<ChangeSetArtifact>, DataSyncError> {
        Ok(state::load_artifact(plan_id))
    }

    async fn selection(
        &self,
        plan_id: &str,
        selection_revision: u64,
    ) -> Result<Vec<ChangeBlock>, DataSyncError> {
        selection::selection(plan_id, selection_revision)
    }
}

/// 桌面失败消息落点：静态 FAILURES 表（`safe_failure_of` 的唯一生产写点）。
pub(crate) struct DesktopFailureSink;

impl FailureSink for DesktopFailureSink {
    fn record(&self, job_id: &str, message: String) {
        state::record_failure(job_id, message);
    }
}

/// webdriver e2e 的提交故障钩子：读走 `exec.rs` 静态原子上 armed 的一次故障。
#[cfg(feature = "webdriver")]
pub(crate) struct E2eCommitFault;

#[cfg(feature = "webdriver")]
impl CommitFaultHook for E2eCommitFault {
    fn take(&self) -> u8 {
        super::exec::take_e2e_commit_fault()
    }
}

/// 桌面形态的执行宿主组装（共享 [`RuntimeDataSyncHost`] + 端口适配 +
/// 失败记录装饰器）。
#[allow(clippy::too_many_arguments)]
pub(crate) fn desktop_execution_host(
    runtime: Arc<datazen::execution::application::RuntimeConnectionUseCases>,
    call_context: RequestContext,
    store: Arc<DesktopRepository>,
    adapters: Arc<crate::transfer::adapter_registry::SyncAdapterRegistry>,
    source: Endpoint,
    target: Endpoint,
    filters: std::collections::HashMap<String, SyncSourceFilter>,
    job_id: &str,
) -> Arc<dyn crate::data_sync::job::host::DataSyncHost> {
    let mut host = RuntimeDataSyncHost::new(
        runtime,
        call_context,
        Arc::new(DesktopSyncConfigs { store }),
        Arc::new(DesktopArtifactStore),
        adapters,
        source,
        target,
        filters,
    );
    #[cfg(feature = "webdriver")]
    {
        host = host.with_commit_fault(Arc::new(E2eCommitFault));
    }
    Arc::new(RecordingDataSyncHost::new(
        Arc::new(host),
        job_id,
        Arc::new(DesktopFailureSink),
    ))
}
