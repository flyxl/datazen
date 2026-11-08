//! Workflow 域的服务端宿主件（W8 刀3c-2）。
//!
//! 分层：词汇与规则在共享层 `datazen::workflow`（模型/执行器/调度器/
//! registry/授权单派生）；本目录只提供 server 形态的宿主绑定：
//!
//! - [`host`]——`WorkflowHost` 的 server 实现（`WorkflowExecutor` 的
//!   执行宿主：连接名/命令执行/SQL 方言/AI 补全）；
//! - [`schedule_grant`]——`WorkflowScheduleGrantReader` 的 server 实现
//!   （派生授权单：定义 YAML + 连接 profile 版本的纯函数）；
//! - [`scheduler`]——核心调度器的 server 接线（`WorkflowScheduleHost`
//!   的 server 实现 + spawn/停车，镜像 `execution::idle_reclaimer` 的
//!   装配形态）。

pub mod host;
pub mod schedule_grant;
pub mod scheduler;
