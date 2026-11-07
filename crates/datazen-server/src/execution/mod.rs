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
//!   端口的 server 实现（无状态 MCP 目标命令七工具与 driverGateway 过渡域
//!   的执行入口；消费者：mcp 门控的 `TargetMachine` 与恒开的
//!   `driver_gateway` 域——模块本身不再门控，`TargetResolver` 在 datazen
//!   `application` feature 后）。
//! - [`target_runner`]：W8-a P2 —— `table_browsing::api::TargetCommandRunner`
//!   端口的 server 实现（tableData 读半边的执行面：目标解析、提交、轮询、
//!   产物分块收集；消费者：tableData 域路由）。
//! - [`ephemeral`]：W8-b ② —— driverType 直取（无连接执行）的内存
//!   ephemeral 会话注册表（CSPRNG 句柄 + 归属盖章 + TTL；消费者：
//!   driverGateway 域的 driverType 直取分支）。
//! - [`table_columns`]：W8 刀1 —— `table_editing::TableColumnSource` 端口的
//!   server 实现（变更二端点的命令面列源备料件：`get_table_schema` 经
//!   runner target 分支现查；组件按请求构造，不进全局装配图）。
//! - [`live_source`]：W8 刀2 —— `LiveConnectionSource` 出向端口的 server
//!   实现（四读端点的活句柄面：反查物理资源 → 权威 driver type → 同实例
//!   驱动 Arc → 借出句柄；经租约源 `with_live_source` 随实例装配）。

pub mod budget;
pub mod budget_bridge;
pub mod driver_exec;
pub mod ephemeral;
pub mod history_recorder;
pub mod identity;
pub mod idle_reclaimer;
pub mod lease;
pub mod lease_use_cases;
pub mod live_source;
pub mod network_provider;
pub mod policy_adapter;
pub mod publisher;
pub mod resource_host;
pub mod secret_provider;
pub mod session_directory;
pub mod submission_token;
pub mod table_columns;
pub mod target_runner;

#[cfg(test)]
mod budget_tests;
