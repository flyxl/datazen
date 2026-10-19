mod execution;
mod opening;
mod release;
#[cfg(feature = "webdriver")]
pub(crate) mod webdriver_dispatch;
use super::{resource_budget::DesktopResourceBudget, resource_mapping as map};
use crate::{db::DriverRegistry, store::Store};
use async_trait::async_trait;
use datazen_driver_api::{resource as dr, session as ds, QueryExecutionId};
use datazen_platform_api::ports::budget::BudgetCoordinator as _;
use datazen_runtime::{
    connection as rc,
    registry::{audit::CapabilityVersions, backend::*},
};
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

pub type ResultPublisher =
    Arc<dyn Fn(&rc::ExecutionId, serde_json::Value) -> Result<(), rc::ProviderError> + Send + Sync>;

/// 一条已打开的物理资源。
///
/// `pub(super)` 是为了让 [`super::live_source`] 能把 `resource_key` / `provider_id`
/// 投影成端口要的 `ConnectionHandle`。**只开这两个投影**，不开 `provider`
/// （那是可执行面）也不开 `handle` 本体（`dr::ResourceHandle` 是可变所有权凭证，
/// 交出去等于把关闭权交给出向端口的调用方）。
pub(super) struct Resource {
    provider: Arc<dyn dr::ResourceProvider>,
    handle: dr::ResourceHandle,
    baseline: dr::Baseline,
    connection_id: String,
    config_revision: u64,
    epoch: u64,
    revision: tokio::sync::Mutex<u64>,
    last_context: tokio::sync::Mutex<rc::SessionContext>,
    cancels: tokio::sync::Mutex<HashMap<String, String>>,
    transactions: tokio::sync::Mutex<Vec<ds::SessionHandleRef>>,
    finalized_transaction: tokio::sync::Mutex<Option<ds::TransactionObservation>>,
    protocol_drained: std::sync::atomic::AtomicBool,
    _budget_scope: Arc<dyn dr::BudgetPort>,
    logical: std::sync::Mutex<Option<datazen_runtime::resource::LogicalSessionPermit>>,
    purpose: datazen_runtime::resource::LeasePurpose,
}

impl Resource {
    /// 建资源时写入的 `OpenResource::runtime_epoch`。出向端口拿它比对会话句柄
    /// 的 epoch，不一致就说明这一行是上一个纪元的残留（INV-07）。
    pub(super) fn epoch(&self) -> u64 {
        self.epoch
    }

    /// 投影出 `ConnectionHandle` 的 `id`。**不是**连接本身，只是路由元数据。
    pub(super) fn connection_id_of_resource(&self) -> String {
        self.handle.resource_key().to_owned()
    }

    /// 投影出 `ConnectionHandle` 的 `pool_id`。
    pub(super) fn pool_id_of_resource(&self) -> String {
        self.handle.provider_id().to_owned()
    }
}

pub struct DesktopSessionBackend {
    registry: Arc<DriverRegistry>,
    store: Arc<Store>,
    budget: Arc<datazen_runtime::budget::InProcessBudgetCoordinator>,
    organization: datazen_platform_api::id::OrganizationId,
    principal: datazen_platform_api::id::PrincipalId,
    leases: datazen_runtime::resource::ProductionLeaseManager,
    resources: tokio::sync::RwLock<HashMap<String, Arc<Resource>>>,
    /// dbSessionId → 已打开的物理 resource_id。**出向端口唯一能用的反向索引。**
    ///
    /// 为什么必须单独一张表：`resources` 的 key 是不透明 `resource_id`（uuid），而
    /// [`LiveConnectionRequest`] 只带 `SessionHandle { dbSessionId, runtimeEpoch }`。
    /// 没有这张表，宿主在收到借出请求时**没有任何办法**从 handle 找到那一份已经活着的
    /// 物理资源——只剩两条路：新建连接（端口契约明令禁止），或凭空拒绝。
    ///
    /// 登记与摘除都发生在 `open` / [`Self::unbind_live_resource`]，与 `resources`
    /// 同步写入，因此不会出现「表里有行、物理资源已关」的窗口。
    /// key 的来源是 `OpenResource::db_session_id`（actor 从 `OpenRequest` 透传）。
    by_session: tokio::sync::RwLock<HashMap<String, String>>,
    publisher: RwLock<Option<ResultPublisher>>,
}
impl DesktopSessionBackend {
    pub fn new(registry: Arc<DriverRegistry>, store: Arc<Store>, physical_limit: u32) -> Self {
        Self::new_scoped(
            registry,
            store,
            physical_limit,
            datazen_platform_api::id::OrganizationId::new(super::identity::LOCAL_ORGANIZATION_ID),
            super::identity::local_principal().unwrap_or_else(|| {
                datazen_platform_api::id::PrincipalId::new("unresolved-desktop-user")
            }),
        )
    }
    pub fn new_scoped(
        registry: Arc<DriverRegistry>,
        store: Arc<Store>,
        physical_limit: u32,
        organization: datazen_platform_api::id::OrganizationId,
        principal: datazen_platform_api::id::PrincipalId,
    ) -> Self {
        Self {
            registry,
            store,
            budget: Arc::new(datazen_runtime::budget::InProcessBudgetCoordinator::new(
                // PostgreSQL metadata admission requires a data connection and
                // a control connection atomically. Reserve a complete lease
                // so retained editors cannot consume its remaining slot.
                datazen_runtime::budget::BudgetConfig::from_parts(physical_limit, [1, 2, 2, 0]),
                organization.clone(),
            )),
            organization,
            principal,
            leases: datazen_runtime::resource::LeaseManager::production(),
            resources: tokio::sync::RwLock::new(HashMap::new()),
            by_session: tokio::sync::RwLock::new(HashMap::new()),
            publisher: RwLock::new(None),
        }
    }
    pub fn set_publisher(&self, publisher: ResultPublisher) -> Result<(), rc::ProviderError> {
        *self.publisher.write().map_err(|_| {
            rc::ProviderError::ProtocolError("result publisher unavailable".into())
        })? = Some(publisher);
        Ok(())
    }
    async fn resource(&self, id: &str) -> Result<Arc<Resource>, rc::ProviderError> {
        self.resources.read().await.get(id).cloned().ok_or_else(|| {
            rc::ProviderError::SessionLost("physical resource no longer exists".into())
        })
    }

    /// 出向端口的查找入口：给一个会话句柄，交出**已经开着**的那份物理资源。
    ///
    /// 失败一律如实：句柄不认识、物理资源已关、epoch 不匹配，三种都是拒绝，
    /// 没有一种会去新建连接。`runtime_epoch` 由调用方（活连接端口）从
    /// [`SessionHandle`] 带进来，与 [`Resource::epoch`]（建资源时写入的
    /// `OpenResource::runtime_epoch`）逐字比对——不一致就说明这一行是上一个
    /// 纪元的残留，交给它是 INV-07 违规。
    ///
    /// 绑定在 [`SessionBackend::open`] 里随资源一起写入（来源是
    /// `OpenResource::db_session_id`），因此「借出成功」蕴含「资源此刻在册」。
    pub async fn live_resource_for_session(
        &self,
        db_session_id: &str,
        runtime_epoch: u64,
    ) -> Option<Arc<Resource>> {
        let resource_id = self.by_session.read().await.get(db_session_id)?.clone();
        let resource = self.resource(&resource_id).await.ok()?;
        (resource.epoch() == runtime_epoch).then_some(resource)
    }

    /// 出向端口绑定的写入端。只由 [`super::PlatformAdapter`] 调用。
    ///
    /// 绑定前先确认资源**真的还在表里**：顺序颠倒会造出「映射指向一个已被摘除的
    /// 物理资源」的行，而那种行正是端口要防的「凭空交出一份死引用」。
    /// 重复绑定同一会话是覆盖（替换旧 epoch 的残留），不是叠加。
    pub async fn bind_live_resource(&self, db_session_id: &str, resource_id: &str) {
        if self.resources.read().await.contains_key(resource_id) {
            self.by_session
                .write()
                .await
                .insert(db_session_id.to_owned(), resource_id.to_owned());
        }
    }
}
struct Sink {
    id: rc::ExecutionId,
    publisher: ResultPublisher,
}
#[async_trait]
impl dr::ResultSink for Sink {
    async fn write(&self, chunk: dr::ResultChunk) -> Result<(), dr::ResourceError> {
        // An empty legacy chunk represents a complete zero-row statement,
        // including providers that cannot report its affected-row count.
        // Keep its SQL and identity on the summary channel; only non-empty
        // batches are row data.
        if chunk.rows.is_empty() {
            return self
                .write_statement_summary(dr::StatementSummary {
                    statement_index: chunk.statement_index,
                    sql: chunk.sql,
                    rows_affected: chunk.rows_affected,
                })
                .await;
        }
        if chunk.rows.len() > 1024 {
            return Err(dr::ResourceError::SinkRejected {
                reason: "driver result block row limit".into(),
            });
        }
        self.write_output(serde_json::json!({"kind":"rows","statementIndex":chunk.statement_index,"sql":chunk.sql,"rows":chunk.rows,"rowsAffected":chunk.rows_affected})).await
    }
    async fn write_output(&self, output: serde_json::Value) -> Result<(), dr::ResourceError> {
        struct Limited {
            bytes: usize,
        }
        impl std::io::Write for Limited {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.bytes = self.bytes.saturating_add(bytes.len());
                if self.bytes > 32 * 1024 * 1024 {
                    return Err(std::io::Error::other("result output exceeds limit"));
                }
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        serde_json::to_writer(&mut Limited { bytes: 0 }, &output).map_err(|_| {
            dr::ResourceError::SinkRejected {
                reason: "command output byte limit".into(),
            }
        })?;
        (self.publisher)(&self.id, output).map_err(|_| dr::ResourceError::SinkRejected {
            reason: "command output publication failed".into(),
        })
    }
    async fn complete(&self) -> Result<(), dr::ResourceError> {
        Ok(())
    }
    async fn fail(&self, reason: &str) -> Result<(), dr::ResourceError> {
        self.write_output(serde_json::json!({"kind":"executionError","message":super::diagnostics::safe_message(reason)})).await
    }
}
#[async_trait]
impl SessionBackend for DesktopSessionBackend {
    async fn open(&self, request: OpenResource) -> Result<OpenedResource, rc::ProviderError> {
        self.open_for_command(request, "").await
    }
    async fn open_for_command(
        &self,
        request: OpenResource,
        command: &str,
    ) -> Result<OpenedResource, rc::ProviderError> {
        self.open_resource(request, command).await
    }
    /// 只读观测快照：池/台账数字来自租约台账，预算三个数字来自预算协调器。
    ///
    /// 两处都只是**读取**：台账被毒化时返回 None（读不到 != 没有资源），预算读不到
    /// 时同样不给半真半假的报告。
    async fn observability_snapshot(
        &self,
    ) -> Option<datazen_runtime::resource::RuntimeObservabilityReport> {
        let ledger = self.leases.snapshot()?;
        let limit = self
            .budget
            .with_ledger(|ledger| u64::from(ledger.config().service_physical_connections));
        let budget = self.budget.snapshot().await.ok()?;
        Some(ledger.with_budget(limit, budget.in_use.get(), budget.granted_total.get()))
    }

    async fn reclaim_idle_resources(&self) {
        self.retire_idle(self.leases.idle_to_retire(None)).await;
        for id in self.leases.quarantine_to_verify() {
            let Ok(resource) = self.resource(id.as_str()).await else {
                continue;
            };
            // Close the original resource only; timeout cannot prove physical closure.
            let confirmed = matches!(
                tokio::time::timeout(
                    std::time::Duration::from_secs(10),
                    resource.provider.close_resource(&resource.handle)
                )
                .await,
                Ok(Ok(ds::CloseDisposition::Closed))
            );
            if self.leases.closed(&id, confirmed).is_ok() && confirmed {
                self.resources.write().await.remove(id.as_str());
            }
        }
    }
    async fn execute(
        &self,
        request: ExecuteOnResource,
    ) -> Result<ResourceExecution, rc::ProviderError> {
        self.execute_resource(request).await
    }
    async fn cancel(&self, request: CancelOnResource) -> Result<ResourceCancel, rc::ProviderError> {
        let resource = self.resource(&request.resource_id).await?;
        if resource
            .cancels
            .lock()
            .await
            .get(request.execution_id.as_str())
            != Some(&request.cancel_handle)
        {
            return Err(rc::ProviderError::HostRejected(
                "cancel binding mismatch".into(),
            ));
        }
        let receipt = resource
            .provider
            .request_cancel(
                &resource.handle,
                &QueryExecutionId::new(request.execution_id.as_str()),
            )
            .await
            .map_err(map::error)?;
        let disposition = match receipt.disposition {
            ds::CancelDisposition::Requested => rc::port::CancelDisposition::Requested,
            ds::CancelDisposition::NotRegistered => {
                return Err(rc::ProviderError::HostRejected(
                    "cancel target not registered".into(),
                ))
            }
            ds::CancelDisposition::AlreadyFinished => rc::port::CancelDisposition::AlreadyFinished,
            ds::CancelDisposition::Unsupported => rc::port::CancelDisposition::Unsupported,
        };
        let state = receipt.state.ok_or_else(|| {
            rc::ProviderError::ProtocolError("cancel receipt lacks execution state".into())
        })?;
        Ok(ResourceCancel {
            disposition,
            state: map::state(state),
        })
    }
    async fn finalize_handles(
        &self,
        request: FinalizeHandles,
    ) -> Result<HandleFinalization, rc::ProviderError> {
        self.finalize_resource_handles(request).await
    }
    async fn close(
        &self,
        request: CloseResource,
    ) -> Result<CloseResourceOutcome, rc::ProviderError> {
        self.close_resource_lease(request).await
    }
}

impl DesktopSessionBackend {
    async fn retire_idle(&self, ids: Vec<rc::ResourceId>) {
        for id in ids {
            let Ok(resource) = self.resource(id.as_str()).await else {
                continue;
            };
            let host = datazen_runtime::resource::HostConditionSnapshot::default();
            if self
                .leases
                .returning(
                    &id,
                    datazen_runtime::resource::DriverCleanVerdict::reported_unclean(),
                    &host,
                )
                .is_err()
            {
                continue;
            }
            let confirmed = matches!(
                resource.provider.close_resource(&resource.handle).await,
                Ok(ds::CloseDisposition::Closed)
            );
            let _ = self.leases.closed(&id, confirmed);
            if confirmed {
                self.resources.write().await.remove(id.as_str());
            }
        }
    }

    /// 按 `resource_id` 摘绑定。由 [`SessionBackend::close`] 调用——它是
    /// **唯一**能保证「关物理资源」与「摘出向端口绑定」同帧发生的地方。
    ///
    /// 为什么不在应用层 `close_session` 里做：那条路要先拿到 `dbSessionId`，
    /// 而 `close_registered` 已经可能失败/被拒（`CloseRejected` 表示什么都没发生）。
    /// 在应用层摘等于把「关没关」与「摘没摘」拆成两次判断，其中一次与另一次不一致时
    /// 就留下一份指向已关资源的绑定。放在这里，两者由同一个 `close` 调用绑定。
    ///
    /// 三次 `CloseResourceOutcome` 全部摘：`Closed` 与 `Undecidable` 的资源都不该再
    /// 被借出，`Err` 路径根本走不到这里（前面的守卫已 return）。
    pub(super) async fn unbind_live_resource_by_resource(&self, resource_id: &str) {
        self.by_session
            .write()
            .await
            .retain(|_, bound| bound != resource_id);
    }
}
