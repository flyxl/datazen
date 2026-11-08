//! Data Sync 域的服务端服务层（W8 刀3e-3 / 刀3e-4a；HTTP 路由见
//! `http/domains/sync.rs`）。
//!
//! 桌面对照（`datazen-desktop/src/commands/sync/`）：刀3e-3 点亮**资产与
//! 读面**（任务/档案 CRUD 胶水、配对分类、迁移运行历史读面、sync job
//! 读回/取消）；刀3e-4a 续建**执行面服务层**（受理管线与执行宿主——九
//! 方法 inspect / compare / prepare / apply / execute / SQL 预览 / 复核 /
//! 冲突检查 / 恢复核验，HTTP 路由随刀3e-4b 点亮）。
//!
//! 词汇唯一定义在共享层 `datazen::migration::data_sync`（`StoredSyncTask`
//! / `SyncProfile` / `DataSyncPairingView`）；迁移运行历史词汇在
//! `datazen::history`（Data Sync / Data Transfer / Schema Diff 三类运行
//! 共用，`operation` 区分）。本模块只做服务端胶水：
//!
//! - [`assets`] —— 档案保存的域校验（`validate` + 连接在场性核对 +
//!   `updated_at` 推进，桌面 `save_sync_profile` 同链）。
//! - [`jobs`] —— sync job 的读回/取消（kind 过滤 + 失败消息覆写 + 计划
//!   预览解析，桌面 `jobs.rs` 读命令的 server 对应物）。
//! - [`runtime_state`] —— 进程级运行态（复核投影挂起集合 + 宿主侧失败
//!   消息表——durable 记录只落安全原因码，真消息由宿主侧补；执行宿主是
//!   唯一生产者）。
//! - [`host`] —— 执行宿主的服务端端口适配（共享层 [`RuntimeDataSyncHost`]
//!   的连接档案回读 / 工件选区注册表 / 失败消息落点三端口 + 组装入口；
//!   桌面 `host::desktop_execution_host` 的服务端对偶）。
//! - [`sessions`] —— 专用会话与端点身份（受理时开 job 自有会话并证明与
//!   用户会话同一物理端点；桌面 `job_api.rs` 的 `own_sessions` /
//!   `release_sessions` + `migration_endpoint` 的开关专用会话）。
//! - [`execution`] —— 执行面受理管线（九方法服务层 + durable Job 受理/
//!   派发收尾 + 迁移运行历史两段落账；桌面 `inspect.rs` / `apply.rs` /
//!   `exec.rs` / `job_api.rs` 的服务端投影）。
//!
//! 隔离纪律：任务/档案/运行历史按 [`OrganizationScope`] 隔离（CM-05：
//! 越界读空、写只落本边界；INV-01：组织来自认证链事实）；job 读写对越
//! 组织行天然落空（JobHost 按 ctx 组织过滤）。

pub mod assets;
pub mod execution;
pub mod host;
pub mod jobs;
pub mod runtime_state;
pub mod sessions;

pub use execution::{
    SyncExecution, SyncExecutionError, SyncInspectRequest, SyncPrepareRequest, SyncRecoveryRequest,
};
pub use jobs::SyncJobs;
pub use runtime_state::SyncRuntimeState;
