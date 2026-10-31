//! MCP：权限模式、连接白名单与表描述的 Provider-neutral 业务规则
//! （D6 item 4 切片一，2026-10-26）。
//!
//! 词源：`datazen::mcp` 三文件逐字迁移。分层：
//! * `domain`——权限模式词汇与 SQL 分级（permission：`McpPermissionMode`
//!   是持久化在应用设置里的 wire 词汇，check_sql_allowed/check_tool_call/
//!   资源与工具可见性是纯规则）与连接白名单（allowlist：空表即全拒绝的
//!   安全默认）；
//! * `application`——表描述格式化（describe：MCP `describe_table` 工具
//!   输出的人类可读文案）。只读命令用例（M4）位于
//!   `workflow::application::runtime_reads`（它吃 ResourceExecutionHost
//!   端口与 mcp 的权限词汇，故落在 workflow 侧），表结构上下文装配位于
//!   `ai::application::schema_context`。
//!
//! 边界（07-stages D6 工作项 4）：业务分发（server/handler/tools/resources）
//! 与认证/stdio/HTTP 登录/事件传输留宿主（src-tauri::mcp）；模块只持有
//! 规则与词汇。SQL 分级复用 `execution::domain::sql_guard` 的机制内核，
//! 不另立一份判据。

pub mod application;
pub mod domain;

// 旧扁平路径镜像（modules 垫片与直接消费者兼容）。
pub use application::describe;
pub use domain::allowlist;
pub use domain::permission;
