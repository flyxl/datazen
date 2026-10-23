//! DataZen 共享机制内核（mechanism kernel）。
//!
//! 本包收纳的是**与连接/会话无关**的机制：Job 运行时、多端预算、SQL 安全门、
//! Schema 元数据缓存、驱动登记表。它们此前住在 `packages/runtime`，与
//! `DbSession`/`ResourceLease`/`Execution` 这些**连接资源运行时**混在一起，
//! 导致领域包（`schema-diff` / `data-sync` / `data-transfer` / `feature-core`）
//! 想复用机制就必须依赖「连接运行时」整包——而那条边会让领域包被动拖上
//! 用例层与宿主词汇（DDD 计划 P2）。
//!
//! 层的方向：`kernel → platform-api / driver-api`，**不反向**。
//! 依赖本包的是 `runtime`（连接资源运行时）与领域包；`runtime` 通过
//! `pub use datazen_kernel::{...}` 保留 `datazen_runtime::job::…` 等既有路径，
//! 调用点零改动。
//!
//! 硬约束与 runtime 相同：不依赖 `tauri*`，不引入 HTTP 框架，不引入 UI 运行时。

pub mod budget;
pub mod driver_registry;
pub mod hash;
/// P5 JobRuntime 内核：接受/幂等 receipt/planId 唯一消费/claim fencing/handler 协议/
/// 多端原子预算/阶段调度与取消意图/效果结局聚合（data-migration-jobs.md 全文口径）。
pub mod job;
/// Schema 元数据缓存：列级 / 表级两层缓存与 schema 世代号失效闸门，
/// 外加 DDL 前缀判定 `sql_may_mutate_schema`。自 `src-tauri/src/cache/` 迁入。
pub mod schema_cache;
/// SQL 安全门网：注释剥离扫描、主关键字分类、参数绑定与 Safe Mode / 只读校验。
/// 宿主与未来的服务端入口共用这一份实现。
pub mod sql_guard;
