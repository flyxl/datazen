//! WR-F7 目标元数据切片：`ResourceExecutionHost` 端口的 server 实现。
//!
//! modules `workflow::application::resource_execution` 定义的端口是
//! 无状态 MCP 目标命令七工具（`query` / `list_databases` / `list_tables` /
//! `search_tables` / `get_schema` / `explain_query` / `describe_table`）与
//! Wapp 只读读取路径的执行入口：`read_target_command` 用例吃这个端口，
//! 宿主负责「profile 权威校验 → 配置版本闸 → 驱动解析 → 规范化目标 →
//! `PreparedExecution` 组装」。桌面实现是 `AppState`
//! （`resolve_execution_target` 的元数据缓存/门锁机器）；本件是同一端口
//! 的 server 形态。
//!
//! ## 与桌面机器的形状差异（登记）
//!
//! 1. **元数据直取，无缓存/门锁**：桌面 `resolve_execution_target` 把驱动
//!    形状与命令要求写进跨请求缓存并持门锁序列化（IPC 面多请求共享）；
//!    server 侧每次 `prepare` 现建快照（驱动目录 v1 为空表，直取没有
//!    竞争面），快照生命周期就是本次解析——语义等价，登记差异。
//! 2. **服务主体残留（W8-a P3 授权种子后收窄）**：本宿主内部构造的
//!    `RequestContext` 仍用合成服务主体 `server-mcp-tool`——**闸序读半边**
//!    （`profile` / `driver` / `safe_mode` / `prepare` 的 profile 读取）
//!    组织口径正确（org 来自装配配置），主体合成仅残留于审计行。
//!    **执行面已收口**：七工具经 `stateless_exec::PrincipalScopedHost`
//!    （P3 种子，`PerCallHost` 无状态镜像）在 `prepare` 返回后把
//!    `PreparedExecution.context` 覆写为请求真主体（`CallIdentity` 传播）；
//!    tableData 读半边（`target_runner.rs`）执行面直接用 `run()` 的真主体。
//!    端口签名手术（四方法全带 ctx，读半边同步真主体化）为后续刀。
//!    driverGateway 过渡域的 execute 路径同残留，随其接线刀收口。
//!    （`stateless_exec` 模块头同款登记。）
//! 3. **租约纪律**：`PreparedExecution.use_cases` 经
//!    [`crate::execution::lease::ServerRuntimeLeaseSource::resolve`]
//!    每调用现取（runtime 可能被吊销，端口语义禁止缓存用例面），共享
//!    sink / 会话目录与租约源同实例（publisher 接线的对偶纪律）。
//!
//! 验收（模块级）：`prepare` 的闸序 = profile 存在 → 未禁用 → 版本一致
//! → 连接配置存在 → 驱动可用（v1 空目录 = 逐字「Driver unavailable」）
//! → 目标规范化 → 用例面组装。`f7_wiring_tests` 钉住可达分支。

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;

use datazen::application::target::{TargetMetadataProvider, TargetResolver, TargetSource};
use datazen::connection::domain::catalog::{ConnectionRepository, ConnectionScope};
use datazen::connection::domain::profile::ProfileRepository;
use datazen::execution::target::{
    ExecutionTarget, LayerRequirement, NamespaceTarget, TargetCanonicalIdRules, TargetCaseFolding,
    TargetCaseRules, TargetNamespaceLayer, TargetNamespacePathSegment, TargetNamespaceShape,
    TargetRequirements,
};
use datazen::settings::SettingsRepository;
use datazen::shared::context::RequestContext;
use datazen::shared::id::{
    ArtifactId, ClientInstanceId, ConnectionId, Counter, OrganizationId, PrincipalId, RequestId,
};
use datazen::table_editing::ports::ArtifactChunkReader;
use datazen::workflow::application::resource_execution::{
    PreparedExecution, ResourceExecutionHost,
};
use datazen::workflow::domain::error::WorkflowError;
use datazen_driver_api::namespace::{
    CaseFolding, CommandTargetRequirements, NamespaceLevelKind, NamespaceShape,
    TargetLevelRequirement,
};
use datazen_driver_api::{CommandCatalog, ConnectionConfig, DatabaseDriver, LegacyCommandCatalog};

use crate::execution::lease::RuntimeLeaseSource;
use crate::storage::{ServerConnections, ServerProfiles, ServerSettingsStore};

/// v1 合成服务主体（见模块头登记第 2 条）。
const SERVICE_PRINCIPAL: &str = "server-mcp-tool";
/// 请求上下文的客户端实例标注（与 driver_exec 的 `server-driver-exec` 同族）。
const CLIENT_INSTANCE: &str = "server-resource-host";

/// 无租约源时的构造错误占位不适用：本件构造即持有租约源。
pub struct ServerResourceExecutionHost {
    connections: Arc<ServerConnections>,
    profiles: Arc<ServerProfiles>,
    registry: Arc<datazen::execution::runtime::driver_registry::DriverRegistry>,
    settings: Arc<ServerSettingsStore>,
    scope: ConnectionScope,
    organization_id: String,
    lease: Arc<crate::execution::lease::ServerRuntimeLeaseSource>,
}

impl ServerResourceExecutionHost {
    /// 构造（bootstrap 装配入口）。
    ///
    /// `registry` 与执行后端共享同一实例（v1 空驱动目录 = 「驱动不可用」
    /// 显式应答；链接闭包随部署构建切片）。`lease` 与 MCP / sessions 域
    /// 路由同一租约源——三消费者共享 sink 与会话目录。
    pub fn new(
        connections: Arc<ServerConnections>,
        profiles: Arc<ServerProfiles>,
        registry: Arc<datazen::execution::runtime::driver_registry::DriverRegistry>,
        settings: Arc<ServerSettingsStore>,
        lease: Arc<crate::execution::lease::ServerRuntimeLeaseSource>,
        organization_id: &str,
    ) -> Self {
        Self {
            connections,
            profiles,
            registry,
            settings,
            scope: ConnectionScope::new(organization_id),
            organization_id: organization_id.to_owned(),
            lease,
        }
    }

    fn context(&self) -> RequestContext {
        RequestContext::new(
            OrganizationId::new(self.organization_id.clone()),
            PrincipalId::new(SERVICE_PRINCIPAL.to_owned()),
            None,
            ClientInstanceId::new(CLIENT_INSTANCE.to_owned()),
            RequestId::new(uuid::Uuid::new_v4().to_string()),
            None,
        )
    }

    fn connection_failed(connection_id: &str, message: &str) -> WorkflowError {
        WorkflowError::ConnectionFailed {
            connection_id: connection_id.to_owned(),
            message: message.to_owned(),
        }
    }

    /// 从驱动与命名空间元数据提供者现建目标解析器（登记第 1 条：无缓存）。
    fn resolver(&self, driver: &dyn DatabaseDriver, shape: &NamespaceShape) -> TargetResolver {
        let shape = namespace_shape(shape);
        let mut commands: HashMap<String, TargetRequirements> = HashMap::new();
        // 命令目录只读不执行（桌面同源注释）：驱动登记的目标要求 +
        // 宿主托管的元数据命令要求。
        let catalog = LegacyCommandCatalog::new(driver);
        for definition in catalog.command_definitions() {
            if let Some(requirements) = catalog.target_requirements(&definition.id) {
                commands.insert(definition.id, command_requirements(requirements));
            }
        }
        for (command, requirements) in host_metadata_command_requirements() {
            commands.insert(command.to_owned(), command_requirements(requirements));
        }
        let metadata: TargetMetadataProvider = Arc::new(move |_connection_id, command| {
            Some((shape.clone(), commands.get(command).copied()))
        });
        TargetResolver::new(metadata)
    }
}

#[async_trait]
impl ResourceExecutionHost for ServerResourceExecutionHost {
    async fn profile(&self, id: &str) -> Result<(ConnectionConfig, u64, bool), WorkflowError> {
        let ctx = self.context();
        let profile = self
            .profiles
            .get(&ctx, ConnectionId::new(id.to_owned()))
            .await
            .map_err(|error| Self::connection_failed(id, &error.to_string()))?
            .ok_or_else(|| Self::connection_failed(id, "profile unavailable"))?;
        let config = self
            .connections
            .get(&self.scope, id)
            .await
            .map_err(|error| Self::connection_failed(id, &error.to_string()))?
            .ok_or_else(|| Self::connection_failed(id, "connection config unavailable"))?;
        Ok((
            config,
            profile.config_revision.counter().get(),
            profile.enabled,
        ))
    }

    async fn driver(
        &self,
        config: &ConnectionConfig,
    ) -> Result<Arc<dyn DatabaseDriver>, WorkflowError> {
        self.registry
            .get(&config.database_type)
            .await
            .ok_or_else(|| WorkflowError::Driver("Driver unavailable".into()))
    }

    async fn safe_mode(&self) -> bool {
        // 读取失败按最严档（fail-closed）——safe_mode = true 拒写。
        match self.settings.read_settings().await {
            Ok(shared) => shared.execution.safe_mode,
            Err(_) => true,
        }
    }

    async fn prepare(
        &self,
        _id: &str,
        revision: u64,
        command: &str,
        target: ExecutionTarget,
    ) -> Result<PreparedExecution, WorkflowError> {
        let connection_id = target.connection_id.clone();
        let ctx = self.context();
        let profile = self
            .profiles
            .get(&ctx, connection_id.clone())
            .await
            .map_err(|error| Self::connection_failed(_id, &error.to_string()))?
            .ok_or_else(|| Self::connection_failed(_id, "profile unavailable"))?;
        if !profile.enabled {
            return Err(Self::connection_failed(_id, "profile unavailable"));
        }
        if profile.config_revision.counter().get() != revision {
            return Err(WorkflowError::Validation(
                "connection configuration changed".into(),
            ));
        }
        let config = self
            .connections
            .get(&self.scope, connection_id.as_str())
            .await
            .map_err(|error| Self::connection_failed(_id, &error.to_string()))?
            .ok_or_else(|| Self::connection_failed(_id, "connection config unavailable"))?;
        let driver = self
            .registry
            .get(&config.database_type)
            .await
            .ok_or_else(|| WorkflowError::Driver("Driver unavailable".into()))?;
        let provider = self
            .registry
            .resource_provider(&config.database_type)
            .await
            .map_err(|lookup| {
                // fail-closed 文案与 driver_exec 同源：逐字携带 registry
                // 的原因（配置事实，不含机密）。
                WorkflowError::Validation(lookup.to_string())
            })?;
        let resolver = self.resolver(driver.as_ref(), provider.namespace_shape());
        let canonical = resolver
            .compute(
                command,
                &connection_id,
                &target.namespace,
                target.object.as_ref(),
                &TargetSource::Request,
            )
            .map_err(|error| WorkflowError::Validation(error.message))?;
        // 租约纪律（模块头登记第 3 条）：每调用现取。
        let runtime = self
            .lease
            .resolve()
            .await
            .map_err(WorkflowError::Validation)?;
        Ok(PreparedExecution {
            artifacts: Arc::new(ServerArtifactChunks(runtime.clone())),
            use_cases: runtime,
            context: ctx,
            target: into_execution_target(canonical),
        })
    }
}

/// 结果块读取的 server 形态（桌面 `HostArtifactChunks` 的同构投影）：
/// 转发给 Runtime 门面的 `read_artifact_chunk`（含其授权与分页）。
///
/// W8-a P2 起 `pub(crate)`：`target_runner` 的 manual_handle 分支把租约
/// 现取的 runtime 投影到同一 `ArtifactChunkReader` 端口形状，与 target
/// 分支（`PreparedExecution.artifacts`）共享收集逻辑。
pub(crate) struct ServerArtifactChunks(
    pub(crate) Arc<datazen::execution::application::orchestration::RuntimeConnectionUseCases>,
);

#[async_trait]
impl ArtifactChunkReader for ServerArtifactChunks {
    async fn read_chunk(
        &self,
        ctx: &RequestContext,
        artifact: &ArtifactId,
        index: Counter,
    ) -> Result<datazen::execution::ResultChunk, datazen::shared::error::ApiError> {
        self.0
            .read_artifact_chunk(ctx, artifact.clone(), index)
            .await
    }
}

/// `CanonicalTarget` → `ExecutionTarget`（桌面 `target_metadata.rs` 同构投影）。
fn into_execution_target(
    canonical: datazen::execution::target::CanonicalTarget,
) -> ExecutionTarget {
    let datazen::execution::target::CanonicalTarget {
        connection_id,
        namespace,
        object,
    } = canonical;
    ExecutionTarget {
        connection_id,
        namespace: NamespaceTarget::new(
            namespace.database.map(|id| id.as_str().to_owned()),
            namespace.catalog.map(|id| id.as_str().to_owned()),
            namespace.schema.map(|id| id.as_str().to_owned()),
            namespace
                .path
                .into_iter()
                .map(|id| id.as_str().to_owned())
                .collect(),
        ),
        object,
    }
}

// ── 驱动元数据映射（桌面 `target_metadata.rs` 的同构投影；驱动词汇 →
//    执行域目标词汇，逐字段一致） ──

fn namespace_shape(shape: &NamespaceShape) -> TargetNamespaceShape {
    let mut required = Vec::new();
    let mut optional = Vec::new();
    for level in &shape.levels {
        if !level.exists {
            continue;
        }
        if level.required {
            required.push(namespace_layer(level.kind));
        } else {
            optional.push(namespace_layer(level.kind));
        }
    }
    TargetNamespaceShape {
        required,
        optional,
        path_segments: shape
            .path_segments
            .iter()
            .map(|segment| TargetNamespacePathSegment {
                position: segment.position,
                kind: namespace_layer(segment.kind),
                meaning: segment.meaning.clone(),
            })
            .collect(),
        case_rules: TargetCaseRules {
            unquoted: case_folding(shape.case_rules.unquoted),
            quoted: case_folding(shape.case_rules.quoted),
        },
        canonical_id_rules: TargetCanonicalIdRules {
            case_sensitive: shape.canonical_id_rules.case_sensitive,
            delimiter: shape.canonical_id_rules.delimiter.clone(),
        },
        aliases: shape.aliases.clone(),
    }
}

fn command_requirements(requirements: CommandTargetRequirements) -> TargetRequirements {
    TargetRequirements {
        database: layer_requirement(requirements.namespace.database),
        catalog: layer_requirement(requirements.namespace.catalog),
        schema: layer_requirement(requirements.namespace.schema),
        path: if requirements.namespace.path_forbidden {
            LayerRequirement::Forbidden
        } else {
            LayerRequirement::Optional
        },
        allows_object: requirements.allows_object,
        requires_object: requirements.requires_object,
        allows_session_defaults: requirements.allows_session_defaults,
    }
}

/// 宿主托管的元数据命令的目标准入规则（schema 元数据命令被宿主截获，
/// 不出现在驱动发现的命令定义里，逐条显式登记；桌面同源）。
fn host_metadata_command_requirements() -> Vec<(&'static str, CommandTargetRequirements)> {
    let namespace_only = CommandTargetRequirements::default();
    let mut catalog = CommandTargetRequirements::default();
    catalog.namespace.database = TargetLevelRequirement::Required;

    [
        ("list_catalog", catalog),
        ("read_relation_columns", namespace_only),
        ("read_relation_schema", namespace_only),
        ("refresh_schema_metadata", namespace_only),
        ("get_structure_capabilities", namespace_only),
        ("plan_table_structure_changes", namespace_only),
    ]
    .into()
}

fn namespace_layer(kind: NamespaceLevelKind) -> TargetNamespaceLayer {
    match kind {
        NamespaceLevelKind::Database => TargetNamespaceLayer::Database,
        NamespaceLevelKind::Catalog => TargetNamespaceLayer::Catalog,
        NamespaceLevelKind::Schema => TargetNamespaceLayer::Schema,
    }
}

fn case_folding(rule: CaseFolding) -> TargetCaseFolding {
    match rule {
        CaseFolding::Lowercases => TargetCaseFolding::Lowercases,
        CaseFolding::Preserved => TargetCaseFolding::Preserved,
        CaseFolding::Unknown => TargetCaseFolding::Unknown,
    }
}

fn layer_requirement(requirement: TargetLevelRequirement) -> LayerRequirement {
    match requirement {
        TargetLevelRequirement::Required => LayerRequirement::Required,
        TargetLevelRequirement::Optional => LayerRequirement::Optional,
        TargetLevelRequirement::Forbidden => LayerRequirement::Forbidden,
    }
}

/// 每调用身份包装（P3 授权种子的宿主侧装饰器，W8 刀1 从
/// `mcp::stateless_exec` 上移至此共享——aiHost 目标执行与无状态 MCP
/// 七工具共用同一条真主体注入路径）。
///
/// 执行面（token / execute / poll）的 `RequestContext` 由 `prepared.context`
/// 决定（`resource_execution::execute_bound` 全程取 `prepared.context`）；
/// 共享宿主的 `prepare` 产出的是合成服务主体（登记第 2 条），本包装在
/// `prepare` 返回后覆写为请求真主体——闸序读半边（profile / driver /
/// safe_mode）委托 inner，主体合成残留照登记，不因包装而扩散。
pub struct PrincipalScopedHost {
    pub inner: Arc<dyn ResourceExecutionHost>,
    pub context: RequestContext,
}

#[async_trait]
impl ResourceExecutionHost for PrincipalScopedHost {
    async fn profile(&self, id: &str) -> Result<(ConnectionConfig, u64, bool), WorkflowError> {
        self.inner.profile(id).await
    }

    async fn driver(
        &self,
        config: &ConnectionConfig,
    ) -> Result<Arc<dyn DatabaseDriver>, WorkflowError> {
        self.inner.driver(config).await
    }

    async fn safe_mode(&self) -> bool {
        self.inner.safe_mode().await
    }

    async fn prepare(
        &self,
        id: &str,
        revision: u64,
        command: &str,
        target: datazen::execution::ExecutionTarget,
    ) -> Result<PreparedExecution, WorkflowError> {
        let mut prepared = self.inner.prepare(id, revision, command, target).await?;
        prepared.context = self.context.clone();
        Ok(prepared)
    }
}
