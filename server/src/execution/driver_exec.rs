//! WR-F7：驱动执行全部服务端化 —— `SessionBackend` 端口的 server v1 实现。
//!
//! 端口（`modules::execution::runtime::registry::backend::SessionBackend`）的
//! server 形态：**没有 stdio 子进程**，驱动连接在本进程内直连——物理资源经
//! `driver-api` 的不透明资源契约（`dr::ResourceProvider`，由
//! [`DriverRegistry`] 按已装配的驱动工厂解析）取得，配置全文（含凭据）只从
//! `ConnectionRepository`（`storage/connections.rs` 的 server 适配器，明文经
//! `ServerSecretVault` 密封落库、get 时拼回）在**服务端内存**里拼出，任何
//! wire 出口都只携带不透明 `resource_id`（`OpenedResource` 的硬要求）。
//!
//! 与桌面 `DesktopSessionBackend`（`apps/desktop/.../session_backend.rs`）
//! 的对照（契约逐条落地，桌面特有面在此登记为缺口，见各字段与方法注释）：
//!
//! | SessionBackend 契约 | 本实现 | 缺口（登记） |
//! | --- | --- | --- |
//! | `open`：物理资源打开 → `OpenedResource` | ✅ describe → acquire → observe；CAS（configRevision/enabled）沿 ProfileRepository | 池化/空闲回收/隔离区（桌面 `ProductionLeaseManager`）未移植：v1 每次新开物理连接，`close` 即真关 |
//! | `open_for_command` | 透传 `open`（与桌面默认形状一致） | 元数据命令专用短租约（`MetadataInspection` purpose）未分流 |
//! | `execute` | ✅ 事务三命令 + `execute_on_resource` + 上下文版本推进 + cancelHandle 公布 + SQL 守卫（只读位从目录回读） | 执行期 CAS 复核（configRevision 再校验）、结果映射探针/查询历史/结果上限（`execution_settings`）未移植；宿主元数据命令走 provider 通用分发 |
//! | `cancel` | ✅ cancelHandle 逐字比对 + `request_cancel` | — |
//! | `finalize_handles` | ✅ commit/rollback 按处置 + `remaining` 计数 | — |
//! | `close` | ✅ `registered_handles` 守卫 + protocol 排空检查 + 真关 | 归池（`ReturnedToPool`）不可能出现：无池 |
//! | `observability_snapshot` | 默认 `None` | 池台账随池化切片补 |
//! | 物理预算 | 构造注入 `Arc<dyn dr::BudgetPort>` | server `budget::ServerBudgetCoordinator` → `dr::BudgetPort` 桥接归主线接线 |
//! | 出站白名单 | 构造注入 [`EgressGate`] | 主线把它接到 `ServerNetworkProvider` 白名单；**未接线 = 不设防**（桌面同权） |
//!
//! 多组织：v1 构造注入目录 scope（`ConnectionScope`）——与
//! `ServerConnections`「每组织一份 scope」的口径一致；owner 不携带组织的
//! `ClientSession`（MCP/工作区）路径也靠它定位目录行。按请求组织分发随
//! P9 多实例切片（`OpenResource` 端口本身不带组织事实，登记为端口缺口）。
//!
//! ## 浏览器零数据库凭据（P7，WR-F7 核实记录）
//!
//! 凭据全文（`ConnectionRepository::get` 拼回的 `ConnectionConfig`，含
//! password）只存在于本后端进程内存，向下交给 `describe_resource` /
//! `acquire_resource`（provider 契约本身禁止把配置回传）；向上（wire）的
//! 出口只有 `OpenedResource.resource_id`（不透明 id，类型注释即硬要求）与
//! `SessionView`（modules `connection/session.rs`，字段面无凭据）。
//! `ServerConnections` 目前没有任何 HTTP/router 出口；主线接连接目录
//! 路由时**必须**投影净化视图，不得把 `get` 的全文直接回显（登记为
//! 主线接线红线）。MCP wire（`mcp/wire.rs`）与 transfer 提交 DTO 已核实
//! 无凭据字段。

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;

use datazen::connection::domain::{ConnectionRepository, ConnectionScope, ProfileReader};
use datazen::execution::runtime::connection as rc;
use datazen::execution::runtime::driver_mapping as map;
use datazen::execution::runtime::driver_registry::DriverRegistry;
use datazen::execution::runtime::registry::audit::CapabilityVersions;
use datazen::execution::runtime::registry::backend::{
    CancelOnResource, CapabilityProvenance, CloseResource, CloseResourceOutcome,
    ExecuteOnResource, FinalizeHandles, HandleDisposition, HandleFinalization, OpenedResource,
    OpenResource, ResourceCancel, ResourceExecution, SessionBackend,
};
use datazen::shared::context::RequestContext;
use datazen::shared::id::{ClientInstanceId, OrganizationId, PrincipalId, RequestId};
use datazen_driver_api::resource as dr;
use datazen_driver_api::session as ds;
use datazen_driver_api::QueryExecutionId;

/// 执行结果发布钩子（与桌面 `ResultPublisher` 同形状）：块/语句元数据经它
/// 交给宿主的结果通道。server 形态由主线接到 `RuntimeResultSink`（接线清单
/// 见 WR-F7 报告）；未安装时 `execute` fail-closed（不静默吞结果）。
pub type ResultPublisher = Arc<
    dyn Fn(
            rc::ExecutionId,
            serde_json::Value,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<(), rc::ProviderError>> + Send>,
        > + Send
        + Sync,
>;

/// 出站闸门：open 前对 `ConnectionConfig` 的端点做部署侧校验（SSRF 白名单）。
/// 主线接 `ServerNetworkProvider` 的白名单判定；`None` = 不设防（与桌面同权，
/// server 部署必须接线——验收门槛 P7「浏览器零数据库凭据」的同族加固）。
pub type EgressGate = Arc<dyn Fn(&datazen_driver_api::ConnectionConfig) -> Result<(), String> + Send + Sync>;

/// 一条已打开的物理资源。字段都是**宿主自己的账**：驱动状态一律从
/// provider 回读（observe_session），这里不镜像驱动事实。
struct Resource {
    provider: Arc<dyn dr::ResourceProvider>,
    handle: dr::ResourceHandle,
    connection_id: String,
    epoch: u64,
    revision: tokio::sync::Mutex<u64>,
    last_context: tokio::sync::Mutex<rc::SessionContext>,
    cancels: tokio::sync::Mutex<HashMap<String, String>>,
    protocol_drained: std::sync::atomic::AtomicBool,
}

/// server 形态的驱动执行后端。无池 v1：`open` 每次新开物理连接、`close`
/// 确认关闭；池化/空闲回收是下一刀（缺口已在模块文档登记）。
pub struct ServerDriverExecBackend {
    registry: Arc<DriverRegistry>,
    connections: Arc<dyn ConnectionRepository>,
    profiles: Arc<dyn ProfileReader>,
    scope: ConnectionScope,
    budget: Arc<dyn dr::BudgetPort>,
    egress: Option<EgressGate>,
    publisher: RwLock<Option<ResultPublisher>>,
    resources: tokio::sync::RwLock<HashMap<String, Arc<Resource>>>,
    /// dbSessionId → resource_id（出向端口反向索引，与桌面 `by_session` 同职责）。
    by_session: tokio::sync::RwLock<HashMap<String, String>>,
}

impl std::fmt::Debug for ServerDriverExecBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerDriverExecBackend")
            .field("connections", &"ConnectionRepository")
            .field("profiles", &"ProfileReader")
            .field("registry", &"DriverRegistry")
            .finish()
    }
}

impl ServerDriverExecBackend {
    /// 构造。所有协作者由宿主（bootstrap，主线接线）显式递交：
    /// `connections`/`profiles` 是 storage 适配器，`registry` 是
    /// `DriverRegistry::with_factories` 的静态形态，`budget` 是物理预算端口，
    /// `egress` 是出站白名单闸门（部署必须接线）。
    pub fn new(
        registry: Arc<DriverRegistry>,
        connections: Arc<dyn ConnectionRepository>,
        profiles: Arc<dyn ProfileReader>,
        scope: ConnectionScope,
        budget: Arc<dyn dr::BudgetPort>,
        egress: Option<EgressGate>,
    ) -> Self {
        Self {
            registry,
            connections,
            profiles,
            scope,
            budget,
            egress,
            publisher: RwLock::new(None),
            resources: tokio::sync::RwLock::new(HashMap::new()),
            by_session: tokio::sync::RwLock::new(HashMap::new()),
        }
    }

    /// 安装结果发布钩子（主线把 `RuntimeResultSink` 的发布路径接进来）。
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

    /// 出向端口的查找入口（与桌面 `live_resource_for_session` 同契约）：
    /// 句柄不认识 / 资源已关 / epoch 不匹配都拒绝，**绝不新建连接**。
    pub async fn live_resource_for_session(
        &self,
        db_session_id: &str,
        runtime_epoch: u64,
    ) -> Option<Arc<Resource>> {
        let resource_id = self.by_session.read().await.get(db_session_id)?.clone();
        let resource = self.resource(&resource_id).await.ok()?;
        (resource.epoch == runtime_epoch).then_some(resource)
    }

    fn request_context(&self, owner: &rc::OwnerRef) -> RequestContext {
        // ClientSession owner 不携带组织事实（端口缺口，模块文档已登记）：
        // 目录 scope 沿构造注入的 v1 裁决。
        RequestContext::new(
            OrganizationId::new(self.scope.organization_id().to_owned()),
            PrincipalId::new(owner_principal(owner)),
            None,
            ClientInstanceId::new("server-driver-exec"),
            RequestId::new(uuid::Uuid::new_v4().to_string()),
            None,
        )
    }

    /// 打开物理资源（`open` / `open_for_command` 共用主体）。
    async fn open_resource(
        &self,
        request: OpenResource,
        command: &str,
    ) -> Result<OpenedResource, rc::ProviderError> {
        let _ = command; // v1：元数据短租约分流未移植（缺口登记），全部按交互查询。
        let ctx = self.request_context(&request.owner);
        let profile = self
            .profiles
            .get(&ctx, request.connection_id.clone())
            .await
            .map_err(|error| rc::ProviderError::HostRejected(error.to_string()))?
            .ok_or_else(|| rc::ProviderError::SessionNotFound("profile unavailable".into()))?;
        if !profile.enabled
            || profile.config_revision.counter().get() != request.config_revision.0
        {
            return Err(rc::ProviderError::HostRejected(
                "profile disabled or revision changed".into(),
            ));
        }
        let config = self
            .connections
            .get(&self.scope, request.connection_id.as_str())
            .await
            .map_err(|error| rc::ProviderError::HostRejected(error.to_string()))?
            .ok_or_else(|| {
                rc::ProviderError::SessionNotFound("connection config unavailable".into())
            })?;
        // 出站白名单（P7 同族加固）：端点不在部署白名单内 → 拒绝，且消息
        // 不含机密（只含主机判定事实，与 ServerNetworkProvider 的登记一致）。
        if let Some(egress) = &self.egress {
            egress(&config).map_err(rc::ProviderError::HostRejected)?;
        }
        let provider = self
            .registry
            .resource_provider(&config.database_type)
            .await
            .map_err(|lookup| {
                // fail-closed 文案（验收项）：驱动类型没装配就说没装配，
                // 逐字携带 registry 的原因（配置事实，不含机密）。
                rc::ProviderError::HostRejected(lookup.to_string())
            })?;
        let target = map::namespace(&request.initial_target);
        let identity_scope = dr::IdentityScope::default();
        let descriptor = provider
            .describe_resource(&dr::DescribeResourceRequest {
                connection_config: config.clone(),
                identity_scope: identity_scope.clone(),
                target: target.clone(),
                purpose: dr::ResourcePurpose::InteractiveQuery,
            })
            .await
            .map_err(map::error)?;
        // v1 从简：成本策略只接受「一次物理连接」的可执行答案，其余拒绝
        // （不静默按 1 计——预算账必须是驱动的自报，不是宿主的猜测）。
        let cost = match descriptor.connection_cost_policy {
            dr::ConnectionCostPolicy::PoolBounded { max_physical_connections }
                if max_physical_connections > 0 =>
            {
                max_physical_connections
            }
            dr::ConnectionCostPolicy::DeclaredConservative { declared_cost, hard_cap }
                if declared_cost > 0 && hard_cap.is_none_or(|cap| cap >= declared_cost) =>
            {
                declared_cost
            }
            _ => {
                return Err(rc::ProviderError::ResourceBusy(
                    "driver connection cost refused under strict budget",
                ))
            }
        };
        let baseline = dr::Baseline::new(descriptor.initialization_requirements);
        let handle = provider
            .acquire_resource(
                &dr::AcquireResourceRequest {
                    connection_config: config,
                    target,
                    scope: dr::ResourceScope {
                        purpose: dr::ResourcePurpose::InteractiveQuery,
                        max_physical_connections: cost,
                        holds_open_transaction: false,
                        pin_for_streaming: false,
                    },
                    identity_scope,
                    baseline,
                },
                &self.budget,
            )
            .await
            .map_err(map::error)?;
        let observation = match provider.observe_session(&handle).await {
            Ok(observation) => observation,
            Err(error) => {
                // 观测失败 = 资源不可用：确认关闭（预算由 provider 在
                // close_resource 里恰好释放一次），失败如实上抛。
                let confirmed = matches!(
                    provider.close_resource(&handle).await,
                    Ok(ds::CloseDisposition::Closed)
                );
                if !confirmed {
                    tracing::warn!("resource observation failed and close was unconfirmed");
                }
                return Err(map::error(error));
            }
        };
        let capabilities = provider.capabilities();
        let context = map::context(&observation.context);
        let opened = OpenedResource {
            context: context.clone(),
            capabilities: CapabilityVersions {
                contract: capabilities.snapshot.protocol_version.to_string(),
                driver_api: capabilities.snapshot.driver_version.clone(),
            },
            capability_snapshot: CapabilityProvenance {
                driver_id: capabilities.snapshot.driver_id.clone(),
                driver_version: capabilities.snapshot.driver_version.clone(),
                protocol_version: capabilities.snapshot.protocol_version,
                capability_revision: capabilities.snapshot.capability_revision,
                confirmed: capabilities.snapshot.confirmed.clone(),
            },
            resource_id: uuid::Uuid::new_v4().to_string(),
            driver_supports_cancel: capabilities
                .capabilities
                .precise_cancel
                .accepts_precise_cancel(),
        };
        self.resources.write().await.insert(
            opened.resource_id.clone(),
            Arc::new(Resource {
                provider,
                handle,
                connection_id: request.connection_id.as_str().to_owned(),
                epoch: request.runtime_epoch,
                revision: tokio::sync::Mutex::new(0),
                last_context: tokio::sync::Mutex::new(context),
                cancels: tokio::sync::Mutex::new(HashMap::new()),
                protocol_drained: std::sync::atomic::AtomicBool::new(observation.protocol_drained),
            }),
        );
        // 出向端口反向索引：与资源登记同步写入（同帧，桌面同款不变式）。
        self.by_session
            .write()
            .await
            .insert(request.db_session_id.as_str().to_owned(), opened.resource_id.clone());
        Ok(opened)
    }
}

fn owner_principal(owner: &rc::OwnerRef) -> String {
    match owner {
        rc::OwnerRef::Editor { principal_id, .. } => principal_id.as_str().to_owned(),
        _ => "server-session".to_owned(),
    }
}

/// 结果发布 Sink（与桌面 `Sink` 同职责：块行 + 空块 = 完整语句摘要 + 上限）。
struct Sink {
    id: rc::ExecutionId,
    publisher: ResultPublisher,
}

#[async_trait]
impl dr::ResultSink for Sink {
    async fn write(&self, chunk: dr::ResultChunk) -> Result<(), dr::ResourceError> {
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
        self.write_output(
            serde_json::json!({"kind":"rows","statementIndex":chunk.statement_index,"sql":chunk.sql,"rows":chunk.rows,"rowsAffected":chunk.rows_affected}),
        )
        .await
    }
    async fn write_output(&self, output: serde_json::Value) -> Result<(), dr::ResourceError> {
        (self.publisher)(self.id.clone(), output)
            .await
            .map_err(|_| dr::ResourceError::SinkRejected {
                reason: "command output publication failed".into(),
            })
    }
    async fn complete(&self) -> Result<(), dr::ResourceError> {
        Ok(())
    }
    async fn fail(&self, reason: &str) -> Result<(), dr::ResourceError> {
        // 诊断消息只走安全词面（与桌面 diagnostics::safe_message 同裁定）：
        // 驱动错误可能内嵌连接参数，fail 路径一律不给原文。
        let _ = reason;
        self.write_output(serde_json::json!({"kind":"executionError","message":"executionFailed"}))
            .await
    }
}

#[async_trait]
impl SessionBackend for ServerDriverExecBackend {
    async fn open(&self, request: OpenResource) -> Result<OpenedResource, rc::ProviderError> {
        self.open_resource(request, "").await
    }

    async fn open_for_command(
        &self,
        request: OpenResource,
        command: &str,
    ) -> Result<OpenedResource, rc::ProviderError> {
        self.open_resource(request, command).await
    }

    async fn execute(
        &self,
        request: ExecuteOnResource,
    ) -> Result<ResourceExecution, rc::ProviderError> {
        let resource = self.resource(&request.resource_id).await?;
        let mut input = request.command.input.clone();
        if matches!(
            request.command.command.as_str(),
            "query" | "execute" | "query_stream" | "explain"
        ) {
            if let Some(params) = input.get("params").cloned() {
                let sql = input
                    .get("sql")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| {
                        rc::ProviderError::HostRejected("SQL input unavailable".into())
                    })?;
                let bound_sql = datazen::execution::domain::sql_guard::apply_params(sql, &params)
                    .map_err(rc::ProviderError::HostRejected)?;
                if let Some(object) = input.as_object_mut() {
                    object.insert("sql".into(), serde_json::Value::String(bound_sql));
                    object.remove("params");
                }
            }
            let sql = input
                .get("sql")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| rc::ProviderError::HostRejected("SQL input unavailable".into()))?;
            // SQL 守卫沿用模块域实现；只读连接从目录回读（守卫需要这份事实），
            // safe-mode 设置未接（缺口登记，桌面 safe_mode=false 同权）。
            let config = self
                .connections
                .get(&self.scope, resource.connection_id.as_str())
                .await
                .map_err(|error| rc::ProviderError::HostRejected(error.to_string()))?
                .ok_or_else(|| {
                    rc::ProviderError::HostRejected("connection config unavailable".into())
                })?;
            datazen::execution::domain::sql_guard::check_sql(sql, config.read_only, false)
                .map_err(rc::ProviderError::HostRejected)?;
        }
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
            publisher,
        };
        let call = dr::CommandCall::new(request.command.command.clone(), input);
        let result = match call.command.as_str() {
            "platform.beginTransaction" => resource
                .provider
                .begin_transaction(
                    &resource.handle,
                    &serde_json::from_value::<ds::TransactionOptions>(call.input.clone())
                        .map_err(|_| {
                            rc::ProviderError::HostRejected("invalid transaction options".into())
                        })?,
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
        resource.cancels.lock().await.remove(request.execution_id.as_str());
        let observation = resource.provider.observe_session(&resource.handle).await;
        let (completion, transaction, driver_failed) = match result {
            Ok((completion, transaction)) => (completion, transaction, false),
            Err(error) => {
                // 失败也要有可读出口：发布执行错误事件（安全词面），再上抛。
                // 连接 id 只进日志（本端账面），不进 wire 诊断。
                tracing::warn!(
                    connection_id = %resource.connection_id,
                    execution_id = %request.execution_id.as_str(),
                    "resource execution failed"
                );
                let _ = dr::ResultSink::write_output(
                    &sink,
                    serde_json::json!({"kind":"executionError","message":"executionFailed"}),
                )
                .await;
                return Err(map::error(error));
            }
        };
        let drained = completion.as_ref().is_some_and(|c| c.protocol_drained)
            || transaction.is_some()
            || observation.as_ref().is_ok_and(|o| o.protocol_drained);
        resource.protocol_drained.store(drained, std::sync::atomic::Ordering::Release);
        let observation = observation.map_err(map::error)?;
        let context_after = completion
            .as_ref()
            .map(|c| &c.context_after)
            .unwrap_or(&observation.context)
            .clone();
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
        let driver_handles = completion
            .as_ref()
            .map(|c| c.session_handles.as_slice())
            .unwrap_or(observation.handles.as_slice())
            .to_vec();
        if driver_handles.iter().any(|handle| {
            !handle.belongs_to(resource.handle.resource_key(), resource.handle.runtime_epoch())
        }) {
            return Err(rc::ProviderError::HostRejected(
                "driver handle bound to another resource".into(),
            ));
        }
        let mut revision = resource.revision.lock().await;
        let mut previous = resource.last_context.lock().await;
        let observed_context = map::context(&context_after);
        *revision = (*revision).max(request.expected_context_revision);
        if *previous != observed_context {
            *revision = revision.checked_add(1).ok_or_else(|| {
                rc::ProviderError::ProtocolError("context revision exhausted".into())
            })?;
            *previous = observed_context.clone();
        }
        let context_revision = *revision;
        drop(previous);
        drop(revision);
        Ok(ResourceExecution {
            execution_id: request.execution_id.clone(),
            stream_id: rc::StreamId::new(format!("result:{}", request.execution_id.as_str())),
            state,
            effect_outcome: map::effect(effect_outcome),
            context_after: observed_context,
            context_revision,
            handles: map::handles(&driver_handles, &request.resource_id, resource.epoch),
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
            return Err(rc::ProviderError::HostRejected("cancel binding mismatch".into()));
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
                return Err(rc::ProviderError::HostRejected("cancel target not registered".into()))
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
        let active = request.handles.iter().filter(|handle| !handle.closed).count();
        let effect = if active > 0 {
            let transaction = match request.disposition {
                HandleDisposition::Commit => {
                    resource.provider.commit_transaction(&resource.handle).await
                }
                HandleDisposition::Rollback => {
                    resource.provider.rollback_transaction(&resource.handle).await
                }
            }
            .map_err(map::error)?;
            if transaction.state == ds::TransactionState::Idle
                && matches!(
                    transaction.effect,
                    Some(ds::EffectOutcome::Completed | ds::EffectOutcome::RolledBack)
                )
            {
                resource
                    .protocol_drained
                    .store(true, std::sync::atomic::Ordering::Release);
            }
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
            // 宿主账上还有活句柄：拒绝关闭（「driver 说 Clean」不是放行条件）。
            return Err(rc::ProviderError::CleanupFailed(
                "registered handles remain".into(),
            ));
        }
        let observation = resource
            .provider
            .observe_session(&resource.handle)
            .await
            .unwrap_or_else(|_| ds::SessionObservation::unobservable());
        if !(observation.protocol_drained
            || resource
                .protocol_drained
                .load(std::sync::atomic::Ordering::Acquire))
            || !resource.cancels.lock().await.is_empty()
            || observation
                .handles
                .iter()
                .any(|handle| !handle.closed)
            || matches!(
                observation.transaction.state,
                ds::TransactionState::Active | ds::TransactionState::Aborted
            )
        {
            // 关不掉/没排空：摘出向端口绑定（资源不再可借出）后如实上报
            // 不可判定——绝不谎报 Closed。
            self.unbind_by_resource(&request.resource_id).await;
            return Ok(CloseResourceOutcome::Undecidable {
                reason: "protocol not drained or live handles",
            });
        }
        let confirmed = matches!(
            resource.provider.close_resource(&resource.handle).await,
            Ok(ds::CloseDisposition::Closed)
        );
        self.resources
            .write()
            .await
            .remove(request.resource_id.as_str());
        self.unbind_by_resource(&request.resource_id).await;
        if confirmed {
            Ok(CloseResourceOutcome::Closed)
        } else {
            Ok(CloseResourceOutcome::Undecidable {
                reason: "driver close unconfirmed",
            })
        }
    }
}

impl ServerDriverExecBackend {
    /// 按 `resource_id` 摘出向端口绑定。只有 [`SessionBackend::close`] 与
    /// 不可判定路径调用——「关物理资源」与「摘绑定」必须同帧发生。
    async fn unbind_by_resource(&self, resource_id: &str) {
        self.by_session
            .write()
            .await
            .retain(|_, bound| bound != resource_id);
    }
}
