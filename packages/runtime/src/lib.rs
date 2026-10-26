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

// 机制内核已归 `datazen-kernel`（DDD 计划 P2）：域包要用这些机制，而它们与连接
// 资源运行时无关。这里的 `pub use` 保持 `datazen_runtime::job::…` 等既有路径不变。
pub use datazen_kernel::{budget, driver_registry, job, schema_cache, sql_guard};

// D3（2026-10-26）起，连接资源运行时机制的唯一定义在
// `datazen-modules::execution::runtime`。本 crate 的机制目录整体迁出，
// 以下再导出保持 `datazen_runtime::connection::…` 等既有路径（§5.5 单向
// 委托）；本 crate 保留 application（用例编排，D3-3c 拆分）、native_tool
// （依赖 Job 词汇，随 D5 jobs 迁移）与 bin/。D8 退役本 crate。
pub use datazen_modules::execution::runtime::{
    artifact, connection, directory, gateway, latency, registry, resource, tunnel,
};

// 用例编排（RuntimeConnectionUseCases）D3-3b 起随机制一同住在
// modules::execution::runtime::application（过渡期整块迁移保持 pub(crate) 语义；
// D3-3c 按 §5.3 拆分为 execution::application 编排与 domain 状态）。
pub use datazen_modules::execution::runtime::application;
pub mod native_tool;

/// 夹具门控开关的单一事实源。
///
/// `connection::testing` 模块的可见性完全由此常量决定：编译 `src-tauri`（未开
/// `test-harness`）时该模块不存在，因此生产路径在类型层面就看不到夹具。
#[cfg(any(test, feature = "test-harness"))]
pub use connection::testing;
