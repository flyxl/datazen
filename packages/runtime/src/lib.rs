//! DataZen platform runtime kernel.
//!
//! `packages/runtime` 拥有 `DbSession`、`ResourceLease`、`Execution` 与 Job stage 的运行时状态。
//!
//! 层与边界的硬约束：
//! 本 crate **不得**依赖 `tauri*`，**不得**引入任何 HTTP 框架，**不得**引入任何 UI 运行时。
//! 因此这里只允许出现传输中立的类型与端口，不出现 socket、窗口、命令分发。
//!
//! 夹具（`connection::testing`）由 `#[cfg(any(test, feature = "test-harness"))]` 门控，
//! 生产构建默认不含夹具代码；`src-tauri` 的 `datazen` crate 也不依赖它。

// 机制内核已归 `datazen-modules`（DDD 计划 P2/D8）：这里的 `pub use` 保持
// `datazen_runtime::job::…` 等过渡路径，直到 runtime 本身退役。
pub use datazen_modules::execution::domain::sql_guard;
pub use datazen_modules::execution::runtime::{budget, driver_registry};
pub use datazen_modules::jobs as job;
pub use datazen_modules::schema as schema_cache;

// D3（2026-10-26）起，连接资源运行时机制的唯一定义在
// `datazen-modules::execution::runtime`。本 crate 的机制目录整体迁出，
// 以下再导出保持 `datazen_runtime::connection::…` 等既有路径（§5.5 单向
// 委托）；本 crate 保留 application（用例编排，D3-3c 拆分）、native_tool
// （依赖 Job 词汇，随 D5 jobs 迁移）与 bin/。D8 退役本 crate。
pub use datazen_modules::execution::runtime::{
    artifact, connection, directory, gateway, latency, registry, resource, tunnel,
};

// 用例编排（RuntimeConnectionUseCases）自 D3-3c 起住在
// modules::execution::application::orchestration（§5.3：请求编排归
// execution::application，不整块留在 runtime 目录）。这里按旧名再导出，
// `datazen_runtime::application::…` 既有路径不变。
pub use datazen_modules::execution::application::orchestration as application;
pub mod driver_mapping;
pub mod native_tool;

/// 夹具门控开关的单一事实源。
///
/// `connection::testing` 模块的可见性完全由此常量决定：编译 `src-tauri`（未开
/// `test-harness`）时该模块不存在，因此生产路径在类型层面就看不到夹具。
#[cfg(any(test, feature = "test-harness"))]
pub use connection::testing;
