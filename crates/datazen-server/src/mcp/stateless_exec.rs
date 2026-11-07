//! WR-F2/WR-F7：MCP 无状态工具的 server 分发件。
//!
//! 工具定义与协议面（13 工具）在 modules（golden parity 测试钉住）；本模块
//! 只做 server 形态的**端口装配**：把 `storage/` 仓储适配成 modules
//! `tools.rs` 的目录端口（`RedactedConnectionSource` / `WorkflowCatalog`）
//! 并逐工具分发。
//!
//! ## 本切片已接 / 未接（登记）
//!
//! | 工具 | 状态 | 依据 |
//! | --- | --- | --- |
//! | `list_connections` | ✅ 真实现（脱敏投影 + 白名单过滤） | 目录端口 server 适配器（本文件） |
//! | `list_workflows` | ✅ 真实现（server v1 无工作流存储 → 空目录） | `EmptyWorkflowCatalog`：空目录是诚实事实 |
//! | `run_workflow` | ✅ 经空目录 → `Workflow not found`（参数类） | 同上 |
//! | `query` / `list_databases` / `list_tables` / `search_tables` / `get_schema` / `explain_query` / `describe_table` | ✅ 目标机器接线（`TargetMachine` + `execution::resource_host`） | `ResourceExecutionHost` 端口 server 实现：profile/版本/连接/驱动闸后走 `TargetResolver` 规范化目标；驱动闸语义随部署构建切片（`driver-*` feature 链接闭包）。**W8-a P3 授权种子**：每调用包 [`PrincipalScopedHost`]——执行面（`prepared.context`）携带请求真主体（`CallIdentity` → `request_context`，会话工具 `PerCallHost` 的无状态镜像）；闸序读半边委托共享宿主（主体合成残留登记见 `resource_host.rs` 第 2 条）。构造未调 `with_target_machine` 时保留 fail-closed 登记（不返回看似成功的结果） |
//!
//! 会话三工具（open/use/close）不走本件——它们在
//! [`crate::mcp::runtime::ModulesBackend`] 的有状态分发里。

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use datazen::connection::domain::{ConnectionRepository, ConnectionScope};
use datazen::mcp::application::tools::{
    self, RedactedConnection, RedactedConnectionSource, WorkflowCatalog,
};
use datazen::settings::ExecutionSettings;
use datazen::shared::context::RequestContext;
use datazen::workflow::application::resource_execution::ResourceExecutionHost;
use datazen::workflow::domain::error::WorkflowError;
use datazen::workflow::domain::model::{WorkflowDefinition, WorkflowListItem};

use crate::mcp::wire::ToolCallError;
use crate::storage::ServerConnections;

/// `RedactedConnectionSource` 的 server 适配器：连接目录的**脱敏投影**。
///
/// 投影字段 = `RedactedConnection` 的字段面（id/name/databaseType/host/
/// database）——`ConnectionRepository::list` 返回的全文（含凭据与驱动
/// options）不出本函数：凭据字段（username/password 与 driver options 全树）
/// 不进入投影。这是 driver_exec.rs 头部「凭据无 wire 出口」红线在 MCP 面
/// 的同类落地。
pub struct ServerRedactedConnections {
    connections: Arc<ServerConnections>,
    scope: ConnectionScope,
}

impl ServerRedactedConnections {
    /// 构造。scope = 本部署的组织（MCP 工具没有自己的请求上下文，宿主
    /// 状态是唯一的组织身份来源——tools.rs 端口注释的 server 落法）。
    pub fn new(connections: Arc<ServerConnections>, organization_id: &str) -> Self {
        Self {
            connections,
            scope: ConnectionScope::new(organization_id),
        }
    }
}

#[async_trait]
impl RedactedConnectionSource for ServerRedactedConnections {
    async fn list_redacted(&self) -> Result<Vec<RedactedConnection>, String> {
        let configs = self
            .connections
            .list(&self.scope)
            .await
            .map_err(|error| error.to_string())?;
        Ok(configs
            .into_iter()
            .map(|config| RedactedConnection {
                id: config.id,
                name: config.name,
                database_type: config.database_type,
                host: config.host,
                database: config.database,
            })
            .collect())
    }
}

/// server v1 的工作流目录：本部署尚无工作流存储（v7 迁移无 workflow 表），
/// 空目录是诚实事实——`list_workflows` 返回 `[]`，`run_workflow` 走参数类
/// 「不存在」错误。工作流存储切片落地后替换本实现。
pub struct EmptyWorkflowCatalog;

#[async_trait]
impl WorkflowCatalog for EmptyWorkflowCatalog {
    async fn list(&self) -> Result<Vec<WorkflowListItem>, String> {
        Ok(Vec::new())
    }

    async fn get(&self, _workflow_id: &str) -> Result<Option<WorkflowDefinition>, String> {
        Ok(None)
    }
}

/// 目标命令机器：`ResourceExecutionHost` 端口实现 + 查询上限等执行设置。
/// 权限档位 v1 固定 `SafeWrite`（与 `run_workflow` 同源；随设置切片读
/// `ExecutionSettings::mcp_permission_mode`）。
pub struct TargetMachine {
    pub host: Arc<dyn ResourceExecutionHost>,
    pub settings: ExecutionSettings,
}

/// 每调用主体作用域宿主（W8-a P3 授权种子，`PerCallHost` 的无状态镜像）：
/// 把传输层的真实主体上下文（`CallIdentity` → `request_context`）注入
/// `PreparedExecution.context`——工作流执行面的全部调用
/// （token / execute / poll / collect）随之携带请求真主体，不再落到
/// `server-mcp-tool` 合成主体上。
///
/// 闸序读半边（`profile` / `driver` / `safe_mode`）委托共享宿主：组织
/// 口径正确（org 来自装配配置），主体合成残留登记见
/// `resource_host.rs` 模块头第 2 条——端口签名手术（四方法全带 ctx）
/// 为后续刀，不在种子范围内。
///
/// W8 刀1：定义上移到 [`crate::execution::resource_host`]（非门控区），
/// aiHost 目标执行与七工具共用同一装饰器；此处 `use` 再导出保持
/// 既有调用点与测试路径不变。
use crate::execution::resource_host::PrincipalScopedHost;

/// 无状态工具分发件。构造即持有全部端口；`call` 与宿主分发的 `other`
/// 分支对接（见 [`crate::mcp::runtime::ModulesBackend`]）。
pub struct StatelessToolExec {
    connections: Arc<dyn RedactedConnectionSource>,
    catalog: Arc<dyn WorkflowCatalog>,
    allowed_connection_ids: Vec<String>,
    /// 目标命令机器（七工具的执行入口）：未装配时七工具保持 fail-closed
    /// 登记（见模块头状态表）。
    target: Option<TargetMachine>,
}

/// 从工具参数取必填字符串。
fn required_str<'a>(tool: &str, arguments: &'a Value, key: &str) -> Result<&'a str, ToolCallError> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| ToolCallError::WithHelp {
            tool: tool.to_owned(),
            reason: format!("missing required input property: {key}"),
        })
}

/// 从工具参数取可选字符串。
fn optional_str(arguments: &Value, key: &str) -> Option<String> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
}

impl StatelessToolExec {
    /// 构造（bootstrap 装配入口）。
    pub fn new(
        connections: Arc<dyn RedactedConnectionSource>,
        catalog: Arc<dyn WorkflowCatalog>,
        allowed_connection_ids: Vec<String>,
    ) -> Self {
        Self {
            connections,
            catalog,
            allowed_connection_ids,
            target: None,
        }
    }

    /// 装配目标命令机器（七工具真分发；不调则保持 fail-closed 登记）。
    pub fn with_target_machine(mut self, target: TargetMachine) -> Self {
        self.target = Some(target);
        self
    }

    /// 分发一个无状态工具。工具名不属于无状态集合时返回与现状一致的
    /// 「未接线」错误（宿主 match 的 session 三分支先于本件）。
    pub async fn call(
        &self,
        context: RequestContext,
        tool: &str,
        arguments: &Value,
    ) -> Result<String, ToolCallError> {
        let result: Result<String, tools::ToolError> = match tool {
            "list_connections" => {
                tools::list_connections(self.connections.as_ref(), &self.allowed_connection_ids)
                    .await
            }
            "list_workflows" => tools::list_workflows(self.catalog.as_ref()).await,
            "run_workflow" => {
                let workflow_id = required_str(tool, arguments, "workflow_id")?;
                let connection_id = arguments.get("connection_id").and_then(Value::as_str);
                let variables = arguments.get("variables").cloned().unwrap_or(Value::Null);
                // 权限模式随装配参数传入（与桌面同源：ExecutionSettings 的
                // mcpPermissionMode）；目标执行路径未接前，run_workflow 只能
                // 走「工作流不存在」——空目录下这是唯一可达分支。
                tools::run_workflow(
                    self.catalog.as_ref(),
                    &UnwiredWorkflowHost,
                    workflow_id,
                    connection_id,
                    &variables,
                    datazen::execution::domain::permission::McpPermissionMode::SafeWrite,
                )
                .await
            }
            "query" | "list_databases" | "list_tables" | "search_tables" | "get_schema"
            | "explain_query" | "describe_table" => {
                let Some(machine) = &self.target else {
                    return Err(ToolCallError::WithHelp {
                        tool: tool.to_owned(),
                        reason: "target-command tools require the server resource-execution host \
                                 (canonical target metadata resolution); they arrive with the \
                                 WR-F7 metadata slice and are not silently approximated here"
                            .into(),
                    });
                };
                // W8-a P3 授权种子：每调用包一层主体作用域宿主——执行面
                // （prepared.context）携带请求真主体；闸序读半边委托共享
                // 宿主（残留登记见模块头与 resource_host.rs 第 2 条）。
                let host = PrincipalScopedHost {
                    inner: machine.host.clone(),
                    context,
                };
                let connection_id = required_str(tool, arguments, "connection_id")?;
                let database = optional_str(arguments, "database");
                let schema = optional_str(arguments, "schema");
                match tool {
                    "query" => {
                        let sql = required_str(tool, arguments, "sql")?;
                        let limit = arguments
                            .get("limit")
                            .and_then(Value::as_u64)
                            .map(|value| value as u32);
                        tools::query(
                            &host,
                            connection_id,
                            sql,
                            limit,
                            datazen::execution::domain::permission::McpPermissionMode::SafeWrite,
                            &machine.settings,
                        )
                        .await
                    }
                    "list_databases" => tools::list_databases(&host, connection_id).await,
                    "list_tables" => {
                        tools::list_tables(&host, connection_id, database, schema).await
                    }
                    "search_tables" => {
                        let pattern = required_str(tool, arguments, "pattern")?;
                        let limit = arguments.get("limit").and_then(Value::as_u64);
                        tools::search_tables(&host, connection_id, database, schema, pattern, limit)
                            .await
                    }
                    "get_schema" => {
                        let table = required_str(tool, arguments, "table")?;
                        tools::get_schema(&host, connection_id, database, schema, table).await
                    }
                    "explain_query" => {
                        let sql = required_str(tool, arguments, "sql")?;
                        tools::explain_query(&host, connection_id, sql).await
                    }
                    "describe_table" => {
                        let table = required_str(tool, arguments, "table")?;
                        tools::describe_table(&host, connection_id, database, schema, table).await
                    }
                    _ => unreachable!("outer arm lists exactly these seven tool names"),
                }
            }
            other => {
                return Err(ToolCallError::internal(format!(
                    "tool '{other}' is not wired in the server form yet (mainline assembly pending)"
                )));
            }
        };
        result.map_err(|error| match error {
            tools::ToolError::InvalidParams(reason) => ToolCallError::WithHelp {
                tool: tool.to_owned(),
                reason,
            },
            tools::ToolError::Internal(reason) => ToolCallError::Internal(reason),
        })
    }
}

/// `run_workflow` 的宿主端口占位：空目录下 `run_workflow` 在取定义一步就
/// 以参数类错误退出，宿主端口永远不会被执行——类型上仍需一份实现以满足
/// 签名。任何可达路径都是登记缺口。
struct UnwiredWorkflowHost;

#[async_trait::async_trait]
impl datazen::workflow::application::host::WorkflowHost for UnwiredWorkflowHost {
    async fn connection_name(&self, _connection_id: &str) -> Result<String, WorkflowError> {
        Err(WorkflowError::Validation(
            "workflow host is not wired in the server form yet".into(),
        ))
    }

    async fn execute_command(
        &self,
        _step: &datazen::workflow::domain::command::WorkflowCommandStep,
        _workflow_connection: Option<&str>,
        _permission_mode: Option<datazen::mcp::domain::permission::McpPermissionMode>,
        _deadline: Option<tokio::time::Instant>,
    ) -> Result<datazen_driver_api::CommandResult, WorkflowError> {
        Err(WorkflowError::Validation(
            "workflow host is not wired in the server form yet".into(),
        ))
    }

    async fn complete_ai(&self, _prompt: String) -> Result<String, WorkflowError> {
        Err(WorkflowError::Validation(
            "workflow host is not wired in the server form yet".into(),
        ))
    }
}

#[cfg(test)]
mod principal_seed_tests {
    //! W8-a P3 授权种子的钉子：`PrincipalScopedHost.prepare` 必须把
    //! `PreparedExecution.context` 覆写为调用侧真实主体——工作流执行面
    //! （token/execute/poll/collect）携带请求主体，绝不落合成主体。

    use std::sync::Arc;

    use async_trait::async_trait;

    use datazen::execution::application::orchestration::RuntimeConnectionUseCases;
    use datazen::execution::ExecutionTarget;
    use datazen::shared::context::RequestContext;
    use datazen::shared::error::{ApiError, ApiErrorCode};
    use datazen::shared::id::{
        ArtifactId, ClientInstanceId, ConnectionId, Counter, OrganizationId, PrincipalId, RequestId,
    };
    use datazen::table_editing::ports::ArtifactChunkReader;
    use datazen::workflow::application::resource_execution::PreparedExecution;
    use datazen::workflow::domain::error::WorkflowError;
    use datazen_driver_api::{ConnectionConfig, DatabaseDriver};

    use super::{PrincipalScopedHost, ResourceExecutionHost};
    use crate::execution::lease::RuntimeLeaseSource;
    use crate::execution::lease_use_cases::LeaseConnectionUseCases;

    fn context(principal: &str) -> RequestContext {
        RequestContext::new(
            OrganizationId::new("org-test"),
            PrincipalId::new(principal),
            None,
            ClientInstanceId::new("test"),
            RequestId::new("req-principal-seed"),
            None,
        )
    }

    /// 租约替身：不解析（执行面不参与本断言，类型上需一份实现）。
    struct ErrLeaseSource;

    #[async_trait]
    impl RuntimeLeaseSource for ErrLeaseSource {
        async fn resolve(&self) -> Result<Arc<RuntimeConnectionUseCases>, String> {
            Err("test double: lease not resolvable".into())
        }
    }

    struct NoChunks;

    #[async_trait]
    impl ArtifactChunkReader for NoChunks {
        async fn read_chunk(
            &self,
            _ctx: &RequestContext,
            _artifact: &ArtifactId,
            _index: Counter,
        ) -> Result<datazen::execution::ResultChunk, ApiError> {
            Err(ApiError::new(
                ApiErrorCode::CapabilityUnsupported,
                "test double: artifacts not resolvable",
            ))
        }
    }

    /// 内层宿主替身：`prepare` 返回携带合成主体的 `PreparedExecution`
    /// （模拟共享宿主的现状），闸序读半边拒绝——不参与本断言。
    struct MarkerHost;

    #[async_trait]
    impl ResourceExecutionHost for MarkerHost {
        async fn profile(&self, _id: &str) -> Result<(ConnectionConfig, u64, bool), WorkflowError> {
            Err(WorkflowError::Validation(
                "test double: profile gate not exercised here".into(),
            ))
        }

        async fn driver(
            &self,
            _config: &ConnectionConfig,
        ) -> Result<Arc<dyn DatabaseDriver>, WorkflowError> {
            Err(WorkflowError::Validation(
                "test double: driver gate not exercised here".into(),
            ))
        }

        async fn safe_mode(&self) -> bool {
            true
        }

        async fn prepare(
            &self,
            _id: &str,
            _revision: u64,
            _command: &str,
            _target: ExecutionTarget,
        ) -> Result<PreparedExecution, WorkflowError> {
            Ok(PreparedExecution {
                use_cases: Arc::new(LeaseConnectionUseCases::new(Arc::new(ErrLeaseSource))),
                artifacts: Arc::new(NoChunks),
                context: context("server-mcp-tool"),
                target: ExecutionTarget {
                    connection_id: ConnectionId::new("c".to_owned()),
                    namespace: datazen::execution::target::NamespaceTarget::database("app"),
                    object: None,
                },
            })
        }
    }

    #[tokio::test]
    async fn principal_scoped_host_overrides_the_prepared_context() {
        let scoped = PrincipalScopedHost {
            inner: Arc::new(MarkerHost),
            context: context("request-principal"),
        };
        let prepared = scoped
            .prepare(
                "c",
                0,
                "query",
                ExecutionTarget {
                    connection_id: ConnectionId::new("c".to_owned()),
                    namespace: datazen::execution::target::NamespaceTarget::database("app"),
                    object: None,
                },
            )
            .await
            .expect("the seed wrapper delegates prepare and rewrites the context");
        assert_eq!(
            prepared.context.principal_id.as_str(),
            "request-principal",
            "the execution face carries the request's real principal"
        );
        assert_ne!(
            prepared.context.principal_id.as_str(),
            "server-mcp-tool",
            "the synthetic service principal never reaches the execution face"
        );
    }
}
