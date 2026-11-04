//! WR-D4 执行接线：六端口的 server 适配器（team-server §2.3 的组装单元；
//! composition root 在 `bootstrap.rs` 统一装配，本模块不隐式构造全局单例）。
//!
//! - [`secret_provider`]：`SecretProvider` 端口 + 版本化秘密材料存储
//!   （`ServerSecretVault`，HMAC-SHA256 KDF 派生钥的 sealed box；明文永不入库）。
//!   同时向 `storage` 的连接/隧道仓储复用净化与密封原语。
//! - [`session_directory`]：`SessionDirectory` 单进程内存实现（v1 形态）。
//! - [`budget`]：`BudgetCoordinator` 节点/组织额度（内存计数器 + 配置注入限额）。
//! - [`submission_token`]：`SubmissionTokenIssuer` 服务端签名密钥 + `keyVersion`
//!   轮换（HMAC-SHA256）。
//! - [`network_provider`]：`NetworkProvider` 出站白名单 + 内存路由表 +
//!   隧道引用计数（fail-closed：空白名单全拒）。
//! - [`identity`]：`IdentityResolver` 数据库侧服务身份 + 委托（CM-06 的
//!   `execution_service_identity` 表承接）。
//! - [`driver_exec`]：WR-F7 驱动执行服务端化 —— `SessionBackend` 端口的
//!   server v1 实现（进程内直连驱动，无 stdio 子进程）。
//! - [`resource_host`]：WR-F7 目标元数据切片 —— `ResourceExecutionHost`
//!   端口的 server 实现（无状态 MCP 目标命令七工具的执行入口；mcp
//!   feature 门控——`TargetResolver` 在 datazen `application` feature 后）。

pub mod budget;
pub mod budget_bridge;
pub mod driver_exec;
pub mod identity;
pub mod lease;
pub mod lease_use_cases;
pub mod network_provider;
pub mod policy_adapter;
pub mod publisher;
#[cfg(feature = "mcp")]
pub mod resource_host;
pub mod secret_provider;
pub mod session_directory;
pub mod submission_token;

#[cfg(test)]
mod budget_tests;
