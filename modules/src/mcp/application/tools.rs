//! MCP 工具的业务用例（M1 切片一：无状态的元数据与查询七工具）。
//!
//! 工具在协议上说什么、怎么说——参数组装、共享读取、结果解析/过滤与
//! JSON 格式化——归本模块，桌面与未来的 web Server 形态共用同一份。
//! `#[tool]` 宏、Input 的 `JsonSchema`、rmcp `McpError` 映射与连接白名单
//! 检查（`ensure_allowed`）留宿主绑定层：它们是传输形状与装配参数。
//!
//! 依赖方向（M1 前置刀解环后成立）：mcp → workflow（`runtime_reads` 读取
//! 用例与 `ResourceExecutionHost` 端口）、mcp → ai（`resolve_query_limit`）、
//! mcp → settings（`ExecutionSettings` 的结果上限）。错误文案逐字保留
//! 宿主历史行为（`tool_help::tool_error` 的包装由宿主完成，本层只返回
//! 业务 `String`）。

use crate::ai::application::db_tools::resolve_query_limit;
use crate::execution::domain::permission::{self, McpPermissionMode};
use crate::mcp::application::describe::format_table_description;
use crate::settings::ExecutionSettings;
use crate::workflow::application::resource_execution::ResourceExecutionHost;
use crate::workflow::application::runtime_reads;
use datazen_driver_api::TableSchema;

/// `list_databases`：列出连接服务器上的全部数据库。
pub async fn list_databases(
    host: &dyn ResourceExecutionHost,
    connection_id: &str,
) -> Result<String, String> {
    let output = runtime_reads::read_target_command(
        host,
        connection_id,
        "list_databases",
        serde_json::json!({}),
    )
    .await?;
    serde_json::to_string_pretty(&datazen_driver_api::parse_databases_from_command(&output))
        .map_err(|error| error.to_string())
}

/// `list_tables`：列出数据库的表（名称、类型与行数）。
pub async fn list_tables(
    host: &dyn ResourceExecutionHost,
    connection_id: &str,
    database: Option<String>,
    schema: Option<String>,
) -> Result<String, String> {
    let output = runtime_reads::read_target_command(
        host,
        connection_id,
        "list_tables",
        serde_json::json!({ "database": database, "schema": schema }),
    )
    .await?;
    serde_json::to_string_pretty(&datazen_driver_api::parse_tables_from_command(&output))
        .map_err(|error| error.to_string())
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
) -> Result<String, String> {
    let limit = limit.unwrap_or(20) as usize;
    let output = runtime_reads::read_target_command(
        host,
        connection_id,
        "list_tables",
        serde_json::json!({ "database": database, "schema": schema }),
    )
    .await?;
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
    .map_err(|error| error.to_string())
}

/// `query`：执行 SQL 并返回 JSON 结果。
///
/// 行数上限取 `resolve_query_limit`（默认 100，上限 50000）与执行设置的
/// `apply_result_limit` 的交；SQL 先过权限档位分级（stdio 模式与 GUI 同
/// 一条守卫，不是旁路）。
pub async fn query(
    host: &dyn ResourceExecutionHost,
    connection_id: &str,
    sql: &str,
    limit: Option<u32>,
    mode: McpPermissionMode,
    settings: &ExecutionSettings,
) -> Result<String, String> {
    permission::check_sql_allowed(sql, mode)?;
    let mut command_input = serde_json::json!({
        "sql": sql,
        "limit": resolve_query_limit(limit),
    });
    settings.apply_result_limit(&mut command_input);
    let output =
        runtime_reads::read_target_command(host, connection_id, "query", command_input).await?;
    serde_json::to_string_pretty(&output).map_err(|error| error.to_string())
}

/// `get_schema`：表的完整结构（列、类型、主键、外键与索引），包一层数组。
pub async fn get_schema(
    host: &dyn ResourceExecutionHost,
    connection_id: &str,
    database: Option<String>,
    schema: Option<String>,
    table: &str,
) -> Result<String, String> {
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
    .await?;
    serde_json::to_string_pretty(&vec![output["schema"].clone()])
        .map_err(|error| error.to_string())
}

/// `explain_query`：取 SQL 的执行计划（EXPLAIN）。
pub async fn explain_query(
    host: &dyn ResourceExecutionHost,
    connection_id: &str,
    sql: &str,
) -> Result<String, String> {
    let output = runtime_reads::read_target_command(
        host,
        connection_id,
        "explain",
        serde_json::json!({ "sql": sql }),
    )
    .await?;
    serde_json::to_string_pretty(&output).map_err(|error| error.to_string())
}

/// `describe_table`：表的人类可读描述（`format_table_description`）。
pub async fn describe_table(
    host: &dyn ResourceExecutionHost,
    connection_id: &str,
    database: Option<String>,
    schema: Option<String>,
    table: &str,
) -> Result<String, String> {
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
    .await?;
    let schema = serde_json::from_value::<TableSchema>(output["schema"].clone())
        .map_err(|error| error.to_string())?;
    Ok(format_table_description(table, &schema))
}

#[cfg(test)]
mod tests {
    //! 行为钉子集中在 `workflow::application::runtime_reads_tests`（端口伪造
    //! 与文案）；本层的增量是纯装配（JSON 组装与格式化），由宿主侧
    //! golden 契约（`mcp_external_contract.json`）与既有集成旅程覆盖。
}
