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
//! | `query` / `list_databases` / `list_tables` / `search_tables` / `get_schema` / `explain_query` / `describe_table` | ⛔ fail-closed 登记 | 目标命令路径需要 `ResourceExecutionHost::prepare` 的规范化目标解析（桌面 `resolve_execution_target` 的元数据缓存/门锁机器），随目标元数据切片接线；在此之前不返回看似成功的结果 |
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

/// 无状态工具分发件。构造即持有全部端口；`call` 与宿主分发的 `other`
/// 分支对接（见 [`crate::mcp::runtime::ModulesBackend`]）。
pub struct StatelessToolExec {
    connections: Arc<dyn RedactedConnectionSource>,
    catalog: Arc<dyn WorkflowCatalog>,
    allowed_connection_ids: Vec<String>,
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
        }
    }

    /// 分发一个无状态工具。工具名不属于无状态集合时返回与现状一致的
    /// 「未接线」错误（宿主 match 的 session 三分支先于本件）。
    pub async fn call(&self, tool: &str, arguments: &Value) -> Result<String, ToolCallError> {
        let result = match tool {
            "list_connections" => {
                tools::list_connections(self.connections.as_ref(), &self.allowed_connection_ids)
                    .await
            }
            "list_workflows" => tools::list_workflows(self.catalog.as_ref()).await,
            "run_workflow" => {
                let workflow_id = arguments
                    .get("workflow_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| ToolCallError::WithHelp {
                        tool: tool.to_owned(),
                        reason: "missing required input property: workflow_id".into(),
                    })?;
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
                    datazen::mcp::domain::permission::McpPermissionMode::SafeWrite,
                )
                .await
            }
            "query" | "list_databases" | "list_tables" | "search_tables" | "get_schema"
            | "explain_query" | "describe_table" => {
                return Err(ToolCallError::WithHelp {
                    tool: tool.to_owned(),
                    reason: "target-command tools require the server resource-execution host \
                             (canonical target metadata resolution); they arrive with the \
                             WR-F7 metadata slice and are not silently approximated here"
                        .into(),
                });
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
