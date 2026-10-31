//! 只读命令在持久化连接 profile 上的执行用例（M4，跨形态收敛）。
//!
//! 「在持久化连接 profile 上执行只读元数据命令」此前只有桌面直连实现
//! （宿主 `RuntimeCommandSource::read_target_command`，Wapp 只读宿主出身，
//! 后被 MCP 的 `runtime_schema.rs` 复用），而 AI 侧早已走
//! [`ResourceExecutionHost`] 端口（`ai::application::schema_context`）。
//! 同一能力、两条适配，违反跨形态一致性原则。本模块把这条读取路径
//! 用例化进共享层：逻辑只此一份，桌面 `RuntimeCommandSource` 回归其合法
//! 角色——端口的桌面实现之一，由宿主装配注入。
//!
//! 归属说明：该用例吃 [`ResourceExecutionHost`] 端口（workflow 域）与
//! mcp 的权限词汇（`McpPermissionMode`，`workflow` feature 已依赖 `mcp`，
//! 方向无环），故落在 workflow 的应用层；调用方是 Wapp、MCP 与未来的
//! web 形态。按 token 预算装配表结构上下文的姊妹用例（list_tables 迭代
//! + DDL 压缩）在 `ai::application::schema_context`——它依赖 ai 的
//! budget/format_compact_ddl，而 `ai` feature 已依赖 `workflow`。
//!
//! 错误文案契约：MCP 元数据工具可达路径的文本逐字保留桌面版行为
//! （profile 失败走 `WorkflowError::Display`、禁用连接
//! "Connection profile is disabled"、未知命令 "Unsupported command: …"）。
//! 唯一改写的是访问级别拒绝文案：桌面原文 "Wapps may invoke read-only
//! driver commands" 把形态词（Wapps）钉进了共享语义，此处改为形态中立
//! 表述；该文案在 MCP 元数据工具的输入域内不可达（它们只提交 Read 级
//! 命令），改写不改变任何 MCP 工具的外部文本。

use crate::execution::domain::target::ExecutionTarget;
use crate::execution::domain::permission::McpPermissionMode;
use crate::schema::application::metadata;
use crate::workflow::domain::command::WorkflowCommandStep;
use crate::workflow::domain::error::WorkflowError;
use datazen_driver_api::{
    check_command_access, CommandAccessLevel, CommandCatalog, ConnectionConfig, DatabaseDriver,
    LegacyCommandCatalog,
};
use std::sync::Arc;

use super::resource_execution::{self, PreparedExecution, ResourceExecutionHost};

/// 每次读取的 step id。
///
/// id 只用于日志与审计标注执行，从不到达数据库。桌面实现的历史值
/// "wapp-command" 是 Wapp 出身的痕迹；共享用例用形态中立的名称。
const READ_STEP_ID: &str = "read-only-command";

/// 访问级别高于 Read 的命令在这条路径上的拒绝文案。
const READ_ONLY_REFUSAL: &str = "Only read-only driver commands are permitted on this path";

/// 任何执行宿主的只读视图。
///
/// 这条路径的身份是「元数据读取」：`query` / `explain` 是 Read 级命令，
/// 但它们携带的 SQL 本身可以写。视图在 profile 上强制
/// [`ConnectionConfig::read_only`]，使共享 SQL 守卫在语句层拒绝一切写
/// 动词。桌面实现的历史形态（`WappReadOnlyHost`）由此成为共享语义的
/// 一部分，而不是某个宿主的私有包装。
struct ReadOnlyForcedHost<'a>(&'a dyn ResourceExecutionHost);

#[async_trait::async_trait]
impl ResourceExecutionHost for ReadOnlyForcedHost<'_> {
    async fn profile(&self, id: &str) -> Result<(ConnectionConfig, u64, bool), WorkflowError> {
        let (mut config, revision, enabled) = self.0.profile(id).await?;
        config.read_only = true;
        Ok((config, revision, enabled))
    }

    async fn driver(
        &self,
        config: &ConnectionConfig,
    ) -> Result<Arc<dyn DatabaseDriver>, WorkflowError> {
        self.0.driver(config).await
    }

    async fn safe_mode(&self) -> bool {
        self.0.safe_mode().await
    }

    async fn prepare(
        &self,
        connection_id: &str,
        revision: u64,
        command: &str,
        target: ExecutionTarget,
    ) -> Result<PreparedExecution, WorkflowError> {
        self.0
            .prepare(connection_id, revision, command, target)
            .await
    }
}

/// 宿主错误的调用方文案。
///
/// `ConnectionFailed` 保留宿主自己的措辞（桌面适配器一直是
/// "Connection profile unavailable"）；其余变体保留各自的 Display 形式。
/// 这与 `ai::application::schema_context` 的 `host_message` 同一先例。
fn host_message(error: WorkflowError) -> String {
    match error {
        WorkflowError::ConnectionFailed { message, .. } => message,
        other => other.to_string(),
    }
}

/// 在持久化连接 profile 上执行一条只读元数据命令。
///
/// 该用例不解析任何编辑器会话：宿主端口负责校验 profile、配置版本与
/// 规范化目标，然后以独立资源执行命令，后端生成请求身份。
/// 命令必须声明为 Read 访问级，且语句经只读视图受 SQL 守卫约束。
pub async fn read_target_command(
    host: &dyn ResourceExecutionHost,
    connection_id: &str,
    command: &str,
    input: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let (config, _, enabled) = host
        .profile(connection_id)
        .await
        .map_err(|error| error.to_string())?;
    if !enabled {
        return Err("Connection profile is disabled".into());
    }
    let driver = host
        .driver(&config)
        .await
        .map_err(|error| error.to_string())?;
    let catalog = LegacyCommandCatalog::new(driver.as_ref());
    let definition = catalog
        .command_definitions()
        .into_iter()
        .chain(datazen_driver_api::schema_catalog_command_definitions())
        .chain(metadata::command_definitions())
        .find(|definition| definition.id == command)
        .ok_or_else(|| format!("Unsupported command: {command}"))?;
    check_command_access(&definition, CommandAccessLevel::Read)
        .map_err(|_| READ_ONLY_REFUSAL.to_owned())?;
    let step = WorkflowCommandStep::new(
        READ_STEP_ID,
        command,
        Some(connection_id.to_owned()),
        input,
    );
    resource_execution::execute(
        &ReadOnlyForcedHost(host),
        &step,
        connection_id,
        Some(McpPermissionMode::SafeWrite),
        None,
        false,
    )
    .await
    .map(|result| result.data)
    .map_err(|error| error.to_string())
}

/// 解析持久化连接 profile 配置的驱动。
///
/// 需要执行命令的调用方必须走 [`read_target_command`]，让端口完成
/// 权威的 profile、版本与目标校验；本助手只为渲染提示词等驱动元数据
/// 服务。
pub async fn driver_for_connection(
    host: &dyn ResourceExecutionHost,
    connection_id: &str,
) -> Result<Arc<dyn DatabaseDriver>, String> {
    let (config, _, enabled) = host.profile(connection_id).await.map_err(host_message)?;
    if !enabled {
        return Err("Connection profile is disabled".into());
    }
    host.driver(&config).await.map_err(host_message)
}

#[cfg(test)]
#[path = "runtime_reads_tests.rs"]
mod tests;
