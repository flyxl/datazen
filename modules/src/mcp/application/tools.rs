//! MCP 工具的业务用例（M1：无状态工具——元数据、查询、连接目录与工作流）。
//!
//! 工具在协议上说什么、怎么说——参数组装、共享读取、结果解析/过滤与
//! JSON 格式化——归本模块，桌面与未来的 web Server 形态共用同一份。
//! `#[tool]` 宏、Input 的 `JsonSchema`、rmcp `McpError` 映射与连接白名单
//! 检查（`ensure_allowed`）留宿主绑定层：它们是传输形状与装配参数。
//!
//! 依赖方向（M1 前置刀解环后成立）：mcp → workflow（`runtime_reads` 读取
//! 用例、`ResourceExecutionHost` 与 `WorkflowHost` 端口、注册表读取）、
//! mcp → ai（`resolve_query_limit`）、mcp → settings（`ExecutionSettings`
//! 的结果上限）。工具文本逐字保留宿主历史行为。
//!
//! 错误通道用中立的 [`ToolError`] 两分：参数/校验类（宿主映射
//! `invalid_params` + help 块）与内部失败（宿主映射 `internal_error`）。
//! 该两分原本就存在于宿主错误映射里，共享化后由本层显式承载。

use crate::ai::application::db_tools::resolve_query_limit;
use crate::execution::domain::permission::{self, McpPermissionMode};
use crate::mcp::application::describe::format_table_description;
use crate::mcp::domain::allowlist;
use crate::settings::ExecutionSettings;
use crate::workflow::application::host::WorkflowHost;
use crate::workflow::application::resource_execution::ResourceExecutionHost;
use crate::workflow::application::runtime_reads;
use crate::workflow::domain::model::{WorkflowDefinition, WorkflowListItem};
use datazen_driver_api::{DatabaseType, TableSchema};

/// 工具错误的两分通道。
///
/// - [`ToolError::InvalidParams`]：调用方的参数或权限问题（SQL 被档位
///   拒绝、workflow 不存在）。宿主映射 `invalid_params` 并附工具帮助块。
/// - [`ToolError::Internal`]：读取或执行失败（连接不可用、命令失败、
///   序列化失败）。宿主映射 `internal_error`。
pub enum ToolError {
    InvalidParams(String),
    Internal(String),
}

impl ToolError {
    /// The message, regardless of channel — for hosts that map both
    /// channels to the same protocol error shape.
    pub fn into_message(self) -> String {
        match self {
            ToolError::InvalidParams(message) | ToolError::Internal(message) => message,
        }
    }
}

fn internal(error: String) -> ToolError {
    ToolError::Internal(error)
}

/// `list_databases`：列出连接服务器上的全部数据库。
pub async fn list_databases(
    host: &dyn ResourceExecutionHost,
    connection_id: &str,
) -> Result<String, ToolError> {
    let output = runtime_reads::read_target_command(
        host,
        connection_id,
        "list_databases",
        serde_json::json!({}),
    )
    .await
    .map_err(internal)?;
    serde_json::to_string_pretty(&datazen_driver_api::parse_databases_from_command(&output))
        .map_err(|error| internal(error.to_string()))
}

/// `list_tables`：列出数据库的表（名称、类型与行数）。
pub async fn list_tables(
    host: &dyn ResourceExecutionHost,
    connection_id: &str,
    database: Option<String>,
    schema: Option<String>,
) -> Result<String, ToolError> {
    let output = runtime_reads::read_target_command(
        host,
        connection_id,
        "list_tables",
        serde_json::json!({ "database": database, "schema": schema }),
    )
    .await
    .map_err(internal)?;
    serde_json::to_string_pretty(&datazen_driver_api::parse_tables_from_command(&output))
        .map_err(|error| internal(error.to_string()))
}

/// `search_tables`：按名称模式（大小写不敏感子串）搜表，带结果上限。
///
/// 返回 matched / totalMatches / totalTables 三元组——总数在截断前统计，
/// 模型能据此判断还有多少没看到。
pub async fn search_tables(
    host: &dyn ResourceExecutionHost,
    connection_id: &str,
    database: Option<String>,
    schema: Option<String>,
    pattern: &str,
    limit: Option<u64>,
) -> Result<String, ToolError> {
    let limit = limit.unwrap_or(20) as usize;
    let output = runtime_reads::read_target_command(
        host,
        connection_id,
        "list_tables",
        serde_json::json!({ "database": database, "schema": schema }),
    )
    .await
    .map_err(internal)?;
    let tables = datazen_driver_api::parse_tables_from_command(&output);
    let pattern_lower = pattern.to_lowercase();
    let matched: Vec<_> = tables
        .iter()
        .filter(|table| table.name.to_lowercase().contains(&pattern_lower))
        .take(limit)
        .collect();
    let total_matches = tables
        .iter()
        .filter(|table| table.name.to_lowercase().contains(&pattern_lower))
        .count();
    serde_json::to_string_pretty(&serde_json::json!({
        "matched": matched,
        "totalMatches": total_matches,
        "totalTables": tables.len(),
        "pattern": pattern,
    }))
    .map_err(|error| internal(error.to_string()))
}

/// `query`：执行 SQL 并返回 JSON 结果。
///
/// 行数上限取 `resolve_query_limit`（默认 100，上限 50000）与执行设置的
/// `apply_result_limit` 的交；SQL 先过权限档位分级（stdio 模式与 GUI 同
/// 一条守卫，不是旁路）。档位拒绝是参数类错误（调用方可换语句重试），
/// 执行失败是内部错误。
pub async fn query(
    host: &dyn ResourceExecutionHost,
    connection_id: &str,
    sql: &str,
    limit: Option<u32>,
    mode: McpPermissionMode,
    settings: &ExecutionSettings,
) -> Result<String, ToolError> {
    permission::check_sql_allowed(sql, mode).map_err(ToolError::InvalidParams)?;
    let mut command_input = serde_json::json!({
        "sql": sql,
        "limit": resolve_query_limit(limit),
    });
    settings.apply_result_limit(&mut command_input);
    let output = runtime_reads::read_target_command(host, connection_id, "query", command_input)
        .await
        .map_err(internal)?;
    serde_json::to_string_pretty(&output).map_err(|error| internal(error.to_string()))
}

/// `get_schema`：表的完整结构（列、类型、主键、外键与索引），包一层数组。
pub async fn get_schema(
    host: &dyn ResourceExecutionHost,
    connection_id: &str,
    database: Option<String>,
    schema: Option<String>,
    table: &str,
) -> Result<String, ToolError> {
    let output = runtime_reads::read_target_command(
        host,
        connection_id,
        "get_table_schema",
        serde_json::json!({
            "database": database,
            "schema": schema,
            "table": table,
        }),
    )
    .await
    .map_err(internal)?;
    serde_json::to_string_pretty(&vec![output["schema"].clone()])
        .map_err(|error| internal(error.to_string()))
}

/// `explain_query`：取 SQL 的执行计划（EXPLAIN）。
pub async fn explain_query(
    host: &dyn ResourceExecutionHost,
    connection_id: &str,
    sql: &str,
) -> Result<String, ToolError> {
    let output = runtime_reads::read_target_command(
        host,
        connection_id,
        "explain",
        serde_json::json!({ "sql": sql }),
    )
    .await
    .map_err(internal)?;
    serde_json::to_string_pretty(&output).map_err(|error| internal(error.to_string()))
}

/// `describe_table`：表的人类可读描述（`format_table_description`）。
pub async fn describe_table(
    host: &dyn ResourceExecutionHost,
    connection_id: &str,
    database: Option<String>,
    schema: Option<String>,
    table: &str,
) -> Result<String, ToolError> {
    let output = runtime_reads::read_target_command(
        host,
        connection_id,
        "get_table_schema",
        serde_json::json!({
            "database": database,
            "schema": schema,
            "table": table,
        }),
    )
    .await
    .map_err(internal)?;
    let schema = serde_json::from_value::<TableSchema>(output["schema"].clone())
        .map_err(|error| internal(error.to_string()))?;
    Ok(format_table_description(table, &schema))
}

/// `list_connections` 的脱敏目录条目（宿主装配，字段已脱敏）。
pub struct RedactedConnection {
    pub id: String,
    pub name: String,
    pub database_type: DatabaseType,
    pub host: Option<String>,
    pub database: Option<String>,
}

/// `list_connections` 的目录端口：连接的脱敏视图由持有目录的形态提供。
///
/// 桌面实现经 `require_connection_service` 读本地目录；web 形态读服务端
/// 组织目录。白名单过滤不在此端口——它是 MCP server 实例的装配参数。
#[async_trait::async_trait]
pub trait RedactedConnectionSource: Send + Sync {
    async fn list_redacted(&self) -> Result<Vec<RedactedConnection>, String>;
}

/// `list_connections`：配置的数据库连接（id、名称、类型与主机），
/// 过连接白名单。
pub async fn list_connections(
    source: &dyn RedactedConnectionSource,
    allowed_connection_ids: &[String],
) -> Result<String, ToolError> {
    let connections = source
        .list_redacted()
        .await
        .map_err(internal)?;
    let result: Vec<serde_json::Value> = connections
        .into_iter()
        .filter(|connection| {
            allowlist::is_connection_allowed(&connection.id, allowed_connection_ids)
        })
        .map(|connection| {
            serde_json::json!({
                "id": connection.id,
                "name": connection.name,
                "databaseType": format!("{:?}", connection.database_type),
                "host": connection.host,
                "database": connection.database,
            })
        })
        .collect();
    serde_json::to_string_pretty(&result).map_err(|error| internal(format!("Error: {error}")))
}

/// `list_workflows` / `run_workflow` 的注册表端口。
///
/// 桌面实现是 `WorkflowRegistry` + 组织作用域；web 形态是服务端注册表。
/// 作用域由实现持有——MCP 工具没有自己的请求上下文，宿主状态就是唯一
/// 的组织身份来源。
#[async_trait::async_trait]
pub trait WorkflowCatalog: Send + Sync {
    /// 用户可见的工作流清单（列表条目形态）。
    async fn list(&self) -> Result<Vec<WorkflowListItem>, String>;
    /// 按 id 取一个工作流定义；不存在时返回 `None`。
    async fn get(&self, workflow_id: &str) -> Result<Option<WorkflowDefinition>, String>;
}

/// `list_workflows`：列出用户定义的工作流。
pub async fn list_workflows(catalog: &dyn WorkflowCatalog) -> Result<String, ToolError> {
    let workflows = catalog.list().await.map_err(internal)?;
    serde_json::to_string_pretty(&workflows).map_err(|error| internal(error.to_string()))
}

/// `run_workflow`：按 id 执行一个工作流。
///
/// 不存在是参数类错误（调用方换 id 重试）；执行失败是内部错误。行数上限
/// 沿用工作流查询的共享常量。
pub async fn run_workflow(
    catalog: &dyn WorkflowCatalog,
    host: &dyn WorkflowHost,
    workflow_id: &str,
    connection_id: Option<&str>,
    variables: &serde_json::Value,
    mode: McpPermissionMode,
) -> Result<String, ToolError> {
    let workflow = catalog
        .get(workflow_id)
        .await
        .map_err(internal)?
        .ok_or_else(|| {
            ToolError::InvalidParams(format!("Workflow '{workflow_id}' not found"))
        })?;
    let result = crate::workflow::engine::executor::WorkflowExecutor::execute_with_options(
        &workflow,
        host,
        connection_id,
        variables,
        crate::workflow::engine::executor::WorkflowExecuteOptions {
            permission_mode: Some(mode),
            query_row_limit: Some(
                crate::workflow::engine::executor::WORKFLOW_QUERY_ROW_LIMIT,
            ),
        },
    )
    .await
    .map_err(internal)?;
    Ok(result.final_output)
}

#[cfg(test)]
mod tests {
    //! 行为钉子集中在 `workflow::application::runtime_reads_tests`（端口伪造
    //! 与文案）；本层的增量是纯装配（JSON 组装与格式化），由宿主侧
    //! golden 契约（`mcp_external_contract.json`）与既有集成旅程覆盖。
}
