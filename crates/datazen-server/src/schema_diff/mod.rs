//! Schema Diff 域的服务端服务层（W8 刀3b-2；HTTP 路由见
//! `http/domains/schema_diff.rs`）。
//!
//! 桌面对照（`datazen-desktop/src/commands/schema_diff/`）：业务编排早已
//! 下沉共享层 `datazen::migration::schema_diff::api`（计划准备/对比/部署
//! 执行）与 `…::job`（JobRuntime 契约 + `SchemaDiffHandler`）；桌面宿主
//! 只剩「会话→端点解析、recovery 目标/指纹、迁移历史记账、JobRuntime
//! 接线」四块传输胶水。本模块是同一胶水的服务端投影：
//!
//! - [`endpoints`] — 会话→端点解析（活句柄借出 + 档案 config 回读，
//!   桌面 `services::migration_endpoint` 的 server 对应物）+ 端点身份
//!   （`checked_identify`，EndpointRef 的 service_key 来源）+ 能力快照哈希。
//! - [`recovery`] — 部署恢复的目标对象派生与目录指纹（桌面
//!   `commands/schema_diff/job/recovery.rs` 的会话绑定编排，纯投影复用
//!   共享层 `job::recovery_targets`）。
//! - [`history`] — 迁移运行历史两段落账 + profile 引用校验（桌面
//!   `commands/history.rs` 的 server 对应物，存储 = v12
//!   `migration_run_history`）。
//! - [`backend`] — [`SchemaDiffJobBackend`] 的服务端实现（五方法）+
//!   sync 适配器/对比预览两个端口的注册表转发实现。
//! - [`jobs`] — JobRuntime 接线（受理→派发→读回/取消/恢复核验，
//!   桌面 `commands/schema_diff/job/desktop.rs` 的 server 对应物）。
//!
//! 隔离纪律：所有会话可见性走目录闸（越组织/未知 → 404 固定文案，
//! sessions 域防枚举同款）；profile/reviewed 计划按
//! [`OrganizationScope`](datazen::shared::scope::OrganizationScope) 隔离
//! （INV-01：组织来自认证链事实，不来自请求体）。

pub mod backend;
pub mod endpoints;
pub mod history;
pub mod jobs;
pub mod recovery;

pub use backend::{ServerComparisonPreviewHost, ServerSchemaDiffAdapters, ServerSchemaDiffBackend};
pub use jobs::{
    SchemaDiffJobAccepted, SchemaDiffJobDetails, SchemaDiffJobs, SchemaDiffPrepareEnvelope,
};
