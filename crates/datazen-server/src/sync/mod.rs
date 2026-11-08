//! Data Sync 域的服务端服务层（W8 刀3e-3；HTTP 路由见
//! `http/domains/sync.rs`）。
//!
//! 桌面对照（`datazen-desktop/src/commands/sync/`）：本刀先点亮**资产与
//! 读面**——任务/档案 CRUD 胶水、配对分类、迁移运行历史读面、sync job
//! 读回/取消。执行面（inspect / compare / prepare / apply / execute /
//! SQL 预览 / 复核 / 冲突检查 / 恢复核验）随刀3e-4 落地，届时
//! [`jobs::SyncJobs`] 的受理工序与 `DataSyncHost` 的服务端实现在此模块
//! 续建。
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
//!   消息表——durable 记录只落安全原因码，真消息由宿主侧补；3e-4 的
//!   执行宿主是唯一生产者，本刀先立仓储）。
//!
//! 隔离纪律：任务/档案/运行历史按 [`OrganizationScope`] 隔离（CM-05：
//! 越界读空、写只落本边界；INV-01：组织来自认证链事实）；job 读回对越
//! 组织行天然落空（JobHost 按 ctx 组织过滤）。

pub mod assets;
pub mod jobs;
pub mod runtime_state;

pub use jobs::SyncJobs;
pub use runtime_state::SyncRuntimeState;
