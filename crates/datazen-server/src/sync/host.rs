//! Data Sync 执行宿主的服务端端口适配（W8 刀3e-4a）。
//!
//! 共享层 [`RuntimeDataSyncHost`]（刀3e-4a 下沉）承担全部形态中立的执行
//! 宿主职责；本文件只实现三个服务端端口：
//!
//! - [`ServerSyncConfigs`] —— 连接档案回读走 [`ServerConnections`]（按
//!   `ctx.organization_id` 组织隔离；桌面是单租户 store，这里是多组织
//!   目录）；
//! - [`ServerSyncArtifacts`] —— ChangeSet 工件与已确认选区挂共享层
//!   [`SyncPlanRepository`]（组织键隔离的进程内注册表——`runtime_state`
//!   模块头预登记的归属：不是宿主私产）；
//! - [`ServerFailureSink`] —— 失败消息落装配态 [`SyncRuntimeState`]
//!   （3e-3 立仓储、本刀接线为唯一生产者）。
//!
//! 组装入口 [`server_execution_host`]：runtime（租约源 resolve 的当前
//! 单元）+ 受理 ctx + 三个端口 + sync 适配器注册表 → 包上共享的失败
//! 记录装饰器。桌面 `host::desktop_execution_host` 的服务端对偶。

use std::sync::Arc;

use async_trait::async_trait;

use datazen::migration::data_sync::error::DataSyncError;
use datazen::migration::data_sync::job::artifact::{ChangeBlock, ChangeSetArtifact};
use datazen::migration::data_sync::job::host::ArtifactRepositoryGuard;
use datazen::migration::data_sync::job::runtime_host::{
    FailureSink, HostConnectionConfigs, RecordingDataSyncHost, RuntimeDataSyncHost,
    SyncArtifactStore,
};
use datazen::migration::data_sync::plan_store::{change_set_artifact_id, SyncPlanRepository};
use datazen::shared::context::RequestContext;
use datazen::shared::scope::OrganizationScope;
use datazen_driver_api::ConnectionConfig;

use crate::schema_diff::endpoints::SessionEndpoints;
use crate::storage::connections::ServerConnections;

use super::runtime_state::SyncRuntimeState;

/// 服务端连接档案回读（组织隔离目录）。
pub struct ServerSyncConfigs {
    connections: Arc<ServerConnections>,
}

impl ServerSyncConfigs {
    pub fn new(connections: Arc<ServerConnections>) -> Self {
        Self { connections }
    }
}

#[async_trait]
impl HostConnectionConfigs for ServerSyncConfigs {
    async fn connection_config(
        &self,
        ctx: &RequestContext,
        connection_id: &str,
    ) -> Option<ConnectionConfig> {
        use datazen::connection::domain::catalog::ConnectionRepository as _;
        self.connections
            .get(
                &datazen::connection::domain::ConnectionScope::new(ctx.organization_id.as_str()),
                connection_id,
            )
            .await
            .ok()
            .flatten()
    }
}

/// 服务端工件/选区注册表：共享层 plan 仓储（组织键隔离）。
///
/// `selection` 的消费语义（读取即消费 + 版本闸 + 子集核验）与桌面
/// `host/selection.rs` 逐字对齐——差异只在存储位置（组织键 vs 进程全局
/// 静态表）。
pub struct ServerSyncArtifacts {
    plans: Arc<dyn SyncPlanRepository>,
    scope: OrganizationScope,
}

impl ServerSyncArtifacts {
    pub fn new(plans: Arc<dyn SyncPlanRepository>, scope: OrganizationScope) -> Self {
        Self { plans, scope }
    }
}

#[async_trait]
impl SyncArtifactStore for ServerSyncArtifacts {
    async fn store_artifact(
        &self,
        artifact: ChangeSetArtifact,
    ) -> Result<ArtifactRepositoryGuard, DataSyncError> {
        let artifact_id = change_set_artifact_id(&artifact.plan_id);
        self.plans.store_artifact(&self.scope, artifact);
        Ok(ArtifactRepositoryGuard { artifact_id })
    }

    async fn load_artifact(
        &self,
        plan_id: &str,
    ) -> Result<Option<ChangeSetArtifact>, DataSyncError> {
        Ok(self.plans.load_artifact(&self.scope, plan_id))
    }

    async fn selection(
        &self,
        plan_id: &str,
        selection_revision: u64,
    ) -> Result<Vec<ChangeBlock>, DataSyncError> {
        let artifact = self
            .plans
            .load_artifact(&self.scope, plan_id)
            .ok_or_else(|| {
                DataSyncError::validation(format!(
                    "plan {plan_id} is not available; compare again before applying"
                ))
            })?;
        if artifact.plan_id != plan_id {
            return Err(DataSyncError::validation(format!(
                "plan {plan_id} does not match the stored ChangeSet"
            )));
        }
        let stored = self
            .plans
            .take_selection(&self.scope, plan_id)
            .ok_or_else(|| {
                DataSyncError::validation(format!(
                    "plan {plan_id} was already submitted for apply; compare again"
                ))
            })?;
        if stored.selection.revision != selection_revision {
            return Err(DataSyncError::validation(format!(
                "selection revision {selection_revision} does not match the reviewed revision {}; review the selection again",
                stored.selection.revision
            )));
        }
        datazen::migration::data_sync::plan_store::select_artifact_blocks(
            &artifact,
            &stored.selection,
            &stored.options,
        )
        .map_err(|error| {
            DataSyncError::validation(format!("reviewed selection cannot be applied: {error}"))
        })
    }
}

/// 服务端失败消息落点：装配态运行态（读回侧 `get`/`details` 的 Failed 覆写源）。
pub struct ServerFailureSink {
    runtime: SyncRuntimeState,
}

impl ServerFailureSink {
    pub fn new(runtime: SyncRuntimeState) -> Self {
        Self { runtime }
    }
}

impl FailureSink for ServerFailureSink {
    fn record(&self, job_id: &str, message: String) {
        self.runtime.record_failure(job_id, message);
    }
}

/// 服务端执行宿主组装（共享 [`RuntimeDataSyncHost`] + 服务端端口 + 失败
/// 记录装饰器）。
///
/// `endpoints.lease` 每调用现取当前 runtime 单元（CM-63：禁止跨请求缓存
/// ——组装发生在受理请求内，租约一次、随 Job 生命周期）。
#[allow(clippy::too_many_arguments)]
pub async fn server_execution_host(
    endpoints: &SessionEndpoints,
    ctx: &RequestContext,
    connections: Arc<ServerConnections>,
    plans: Arc<dyn SyncPlanRepository>,
    runtime_state: SyncRuntimeState,
    adapters: Arc<
        datazen::migration::data_transfer::transfer::adapter_registry::SyncAdapterRegistry,
    >,
    source: datazen::migration::data_sync::model::Endpoint,
    target: datazen::migration::data_sync::model::Endpoint,
    filters: std::collections::HashMap<
        String,
        datazen::migration::data_sync::filter::SyncSourceFilter,
    >,
    job_id: &str,
) -> Result<Arc<dyn datazen::migration::data_sync::job::host::DataSyncHost>, String> {
    let runtime = endpoints.lease.resolve().await?;
    let host = RuntimeDataSyncHost::new(
        runtime,
        ctx.clone(),
        Arc::new(ServerSyncConfigs::new(connections)),
        Arc::new(ServerSyncArtifacts::new(
            plans,
            OrganizationScope {
                organization_id: ctx.organization_id.clone(),
            },
        )),
        adapters,
        source,
        target,
        filters,
    );
    Ok(Arc::new(RecordingDataSyncHost::new(
        Arc::new(host),
        job_id,
        Arc::new(ServerFailureSink::new(runtime_state)),
    )))
}
