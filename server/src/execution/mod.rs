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

pub mod budget;
pub mod identity;
pub mod network_provider;
pub mod secret_provider;
pub mod session_directory;
pub mod submission_token;

#[cfg(test)]
mod budget_tests;
