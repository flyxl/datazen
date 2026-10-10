use super::{resource_budget::DesktopResourceBudget, resource_mapping as map};
use crate::{db::DriverRegistry, store::Store};
use async_trait::async_trait;
use datazen_driver_api::{resource as dr, session as ds, QueryExecutionId};
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
    epoch: u64,
    revision: tokio::sync::Mutex<u64>,
    last_context: tokio::sync::Mutex<rc::SessionContext>,
    cancels: tokio::sync::Mutex<HashMap<String, String>>,
    _short_permit: Option<tokio::sync::OwnedSemaphorePermit>,
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
    budget: Arc<dyn dr::BudgetPort>,
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
    short_resources: Arc<tokio::sync::Semaphore>,
}
impl DesktopSessionBackend {
    pub fn new(registry: Arc<DriverRegistry>, store: Arc<Store>, physical_limit: u32) -> Self {
        Self {
            registry,
            store,
            budget: Arc::new(DesktopResourceBudget::new(physical_limit)),
            resources: tokio::sync::RwLock::new(HashMap::new()),
            by_session: tokio::sync::RwLock::new(HashMap::new()),
            publisher: RwLock::new(None),
            short_resources: Arc::new(tokio::sync::Semaphore::new(2)),
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
        (self.publisher)(&self.id, serde_json::json!({"kind":"rows","statementIndex":chunk.statement_index,"sql":chunk.sql,"rows":chunk.rows,"rowsAffected":chunk.rows_affected})).map_err(|_| dr::ResourceError::SinkRejected { reason: "result publication failed".into() })
    }
    async fn complete(&self) -> Result<(), dr::ResourceError> {
        Ok(())
    }
    async fn fail(&self, _reason: &str) -> Result<(), dr::ResourceError> {
        Ok(())
    }
}
#[async_trait]
impl SessionBackend for DesktopSessionBackend {
    async fn open(&self, request: OpenResource) -> Result<OpenedResource, rc::ProviderError> {
        let (config, metadata) = self
            .store
            .platform_profile(request.connection_id.as_str())
            .await
            .ok_or_else(|| rc::ProviderError::SessionNotFound("profile unavailable".into()))?;
        if !metadata.enabled || metadata.config_revision != request.config_revision.0 {
            return Err(rc::ProviderError::HostRejected(
                "profile disabled or revision changed".into(),
            ));
        }
        let provider = self
            .registry
            .resource_provider(&config.database_type)
            .await
            .map_err(|_| rc::ProviderError::CapabilityUnsupported)?;
        if matches!(request.owner, rc::OwnerRef::Editor { .. }) {
            provider
                .capabilities()
                .require_stateful_session()
                .map_err(|_| rc::ProviderError::CapabilityUnsupported)?;
        }
        let purpose = if matches!(request.owner, rc::OwnerRef::ClientSession { .. }) {
            dr::ResourcePurpose::MetadataInspection
        } else {
            dr::ResourcePurpose::InteractiveQuery
        };
        let target = map::namespace(&request.initial_target);
        let identity_scope = dr::IdentityScope::default();
        let descriptor = provider
            .describe_resource(&dr::DescribeResourceRequest {
                connection_config: config.clone(),
                identity_scope: identity_scope.clone(),
                target: target.clone(),
                purpose,
            })
            .await
            .map_err(map::error)?;
        let baseline = dr::Baseline::new(descriptor.initialization_requirements);
        let short_permit = if matches!(request.owner, rc::OwnerRef::ClientSession { .. }) {
            Some(
                self.short_resources
                    .clone()
                    .acquire_owned()
                    .await
                    .map_err(|_| rc::ProviderError::ResourceBusy("short resource pool closed"))?,
            )
        } else {
            None
        };
        let handle = provider
            .acquire_resource(
                &dr::AcquireResourceRequest {
                    connection_config: config,
                    target,
                    scope: dr::ResourceScope {
                        purpose,
                        max_physical_connections: 1,
                        holds_open_transaction: false,
                        pin_for_streaming: false,
                    },
                    identity_scope,
                    baseline: baseline.clone(),
                },
                &self.budget,
            )
            .await
            .map_err(map::error)?;
        let observation = match provider.observe_session(&handle).await {
            Ok(observation) => observation,
            Err(error) => {
                let _ = provider.close_resource(&handle).await;
                return Err(map::error(error));
            }
        };
        let resource_id = uuid::Uuid::new_v4().to_string();
        let capabilities = provider.capabilities();
        let opened = OpenedResource {
            context: map::context(&observation.context),
            capabilities: CapabilityVersions::new(
                capabilities.snapshot.protocol_version.to_string(),
                capabilities.snapshot.driver_version.clone(),
            ),
            resource_id: resource_id.clone(),
            driver_supports_cancel: capabilities
                .capabilities
                .precise_cancel
                .accepts_precise_cancel(),
        };
        self.resources.write().await.insert(
            resource_id.clone(),
            Arc::new(Resource {
                provider,
                handle,
                baseline,
                epoch: request.runtime_epoch,
                revision: tokio::sync::Mutex::new(0),
                last_context: tokio::sync::Mutex::new(map::context(&observation.context)),
                cancels: tokio::sync::Mutex::new(HashMap::new()),
                _short_permit: short_permit,
            }),
        );
        // 出向端口的反向索引：没有这一行，借出请求就只能新建连接或凭空拒绝。
        // 先确认资源真的在表里再绑（`insert` 刚刚做完，这一步因此恒真，但留着
        // 是为了让「绑定 ⇒ 资源在册」这条不变式在代码里看得见，而不是靠调用顺序）。
        self.bind_live_resource(request.db_session_id.as_str(), &resource_id)
            .await;
        Ok(opened)
    }
    async fn execute(
        &self,
        request: ExecuteOnResource,
    ) -> Result<ResourceExecution, rc::ProviderError> {
        let resource = self.resource(&request.resource_id).await?;
        let publisher = self
            .publisher
            .read()
            .map_err(|_| rc::ProviderError::ProtocolError("publisher unavailable".into()))?
            .clone()
            .ok_or_else(|| rc::ProviderError::ProtocolError("publisher not installed".into()))?;
        let execution = QueryExecutionId::new(request.execution_id.as_str());
        let cancel_handle = uuid::Uuid::new_v4().to_string();
        resource
            .cancels
            .lock()
            .await
            .insert(request.execution_id.as_str().into(), cancel_handle.clone());
        request.cancel_handle_sink.publish(&cancel_handle);
        let sink = Sink {
            id: request.execution_id.clone(),
            publisher: publisher.clone(),
        };
        let call = dr::CommandCall::new(
            request.command.command.clone(),
            request.command.input.clone(),
        );
        let result = match call.command.as_str() {
            "platform.beginTransaction" => resource
                .provider
                .begin_transaction(
                    &resource.handle,
                    &serde_json::from_value::<ds::TransactionOptions>(call.input.clone()).map_err(
                        |_| rc::ProviderError::HostRejected("invalid transaction options".into()),
                    )?,
                )
                .await
                .map(|transaction| (None, Some(transaction))),
            "platform.commitTransaction" => resource
                .provider
                .commit_transaction(&resource.handle)
                .await
                .map(|transaction| (None, Some(transaction))),
            "platform.rollbackTransaction" => resource
                .provider
                .rollback_transaction(&resource.handle)
                .await
                .map(|transaction| (None, Some(transaction))),
            _ => resource
                .provider
                .execute_on_resource(&resource.handle, &execution, &call, &sink)
                .await
                .map(|completion| (Some(completion), None)),
        };
        resource
            .cancels
            .lock()
            .await
            .remove(request.execution_id.as_str());
        let observation = resource.provider.observe_session(&resource.handle).await;
        let (completion, transaction, driver_failed) = match result {
            Ok((completion, transaction)) => (completion, transaction, false),
            Err(_) => (None, None, true),
        };
        let observation = observation.map_err(map::error)?;
        let context_after = completion
            .as_ref()
            .map(|c| &c.context_after)
            .unwrap_or(&observation.context);
        let effect_outcome = completion
            .as_ref()
            .map(|c| c.effect_outcome)
            .or_else(|| transaction.as_ref().and_then(|t| t.effect))
            .unwrap_or(if driver_failed {
                ds::EffectOutcome::Unknown
            } else {
                ds::EffectOutcome::NotStarted
            });
        let state = completion
            .as_ref()
            .map(|c| match c.completion_status {
                ds::CompletionStatus::Succeeded => rc::ExecutionState::Succeeded,
                ds::CompletionStatus::Failed { .. } => rc::ExecutionState::Failed,
                ds::CompletionStatus::Cancelled { .. } => rc::ExecutionState::Cancelled,
            })
            .unwrap_or(if effect_outcome == ds::EffectOutcome::Unknown {
                rc::ExecutionState::Failed
            } else {
                rc::ExecutionState::Succeeded
            });
        if let Some(completion) = &completion {
            for (index, statement) in completion.statement_results.iter().enumerate() {
                publisher(
                    &request.execution_id,
                    serde_json::json!({"kind":"statementMetadata","statementIndex":index,"columns":statement.columns,"rowsAffected":statement.rows_affected,"truncated":statement.truncated}),
                )?;
            }
        }
        let driver_handles = completion
            .as_ref()
            .map(|c| c.session_handles.as_slice())
            .unwrap_or(observation.handles.as_slice());
        if driver_handles.iter().any(|handle| {
            !handle.belongs_to(
                resource.handle.resource_key(),
                resource.handle.runtime_epoch(),
            )
        }) {
            return Err(rc::ProviderError::HostRejected(
                "driver handle bound to another resource".into(),
            ));
        }
        let mut revision = resource.revision.lock().await;
        let mut previous = resource.last_context.lock().await;
        let observed_context = map::context(context_after);
        *revision = (*revision).max(request.expected_context_revision);
        if *previous != observed_context {
            *revision = revision.checked_add(1).ok_or_else(|| {
                rc::ProviderError::ProtocolError("context revision exhausted".into())
            })?;
            *previous = observed_context.clone();
        }
        Ok(ResourceExecution {
            execution_id: request.execution_id.clone(),
            stream_id: rc::StreamId::new(format!("result:{}", request.execution_id.as_str())),
            state,
            effect_outcome: map::effect(effect_outcome),
            context_after: observed_context,
            context_revision: *revision,
            handles: map::handles(driver_handles, &request.resource_id, resource.epoch),
            cancel_handle,
        })
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
        let resource = self.resource(&request.resource_id).await?;
        if request.handles.iter().any(|handle| {
            handle.resource_id.as_str() != request.resource_id
                || handle.runtime_epoch.get() != resource.epoch
        }) {
            return Err(rc::ProviderError::HostRejected(
                "handle resource binding mismatch".into(),
            ));
        }
        if request
            .handles
            .iter()
            .any(|handle| !handle.closed && handle.kind != rc::HandleKind::Transaction)
        {
            return Err(rc::ProviderError::CapabilityUnsupported);
        }
        let before = resource
            .provider
            .observe_session(&resource.handle)
            .await
            .map_err(map::error)?;
        let active = request
            .handles
            .iter()
            .filter(|handle| !handle.closed)
            .count();
        let effect = if active > 0 {
            let transaction = match request.disposition {
                HandleDisposition::Commit => {
                    resource.provider.commit_transaction(&resource.handle).await
                }
                HandleDisposition::Rollback => {
                    resource
                        .provider
                        .rollback_transaction(&resource.handle)
                        .await
                }
            }
            .map_err(map::error)?;
            transaction.effect.unwrap_or(ds::EffectOutcome::Unknown)
        } else {
            ds::EffectOutcome::NotStarted
        };
        let after = resource
            .provider
            .observe_session(&resource.handle)
            .await
            .map_err(map::error)?;
        let remaining = after.handles.iter().filter(|handle| !handle.closed).count();
        if active == 0 && before.handles.iter().any(|handle| !handle.closed) {
            return Err(rc::ProviderError::CleanupFailed(
                "unregistered driver handles".into(),
            ));
        }
        Ok(HandleFinalization {
            finalized: request.handles.len().saturating_sub(remaining),
            remaining,
            effect_outcome: map::effect(effect),
        })
    }
    async fn close(
        &self,
        request: CloseResource,
    ) -> Result<CloseResourceOutcome, rc::ProviderError> {
        let resource = self.resource(&request.resource_id).await?;
        if request.registered_handles != 0 {
            return Err(rc::ProviderError::CleanupFailed(
                "registered handles remain".into(),
            ));
        }
        let observation = resource
            .provider
            .observe_session(&resource.handle)
            .await
            .map_err(map::error)?;
        if observation.handles.iter().any(|handle| !handle.closed)
            || matches!(
                observation.transaction.state,
                ds::TransactionState::Active
                    | ds::TransactionState::Aborted
                    | ds::TransactionState::Unknown
            )
        {
            return Ok(CloseResourceOutcome::Undecidable {
                reason: "resource state unresolved",
            });
        }
        let _reset = resource
            .provider
            .reset_resource(&resource.handle, &resource.baseline)
            .await;
        // 出向端口绑定与物理资源同帧摘除：`Closed` 与 `Undecidable` 都不该再被借出。
        self.unbind_live_resource_by_resource(&request.resource_id)
            .await;
        match resource
            .provider
            .close_resource(&resource.handle)
            .await
            .map_err(map::error)?
        {
            ds::CloseDisposition::Closed => {
                self.resources.write().await.remove(&request.resource_id);
                Ok(CloseResourceOutcome::Closed)
            }
            ds::CloseDisposition::CloseUnconfirmed => Ok(CloseResourceOutcome::Undecidable {
                reason: "driver close unconfirmed",
            }),
        }
    }
}

impl DesktopSessionBackend {
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
