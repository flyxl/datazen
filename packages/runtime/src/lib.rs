//! DataZen platform runtime kernel.
//!
//! `packages/runtime` 拥有 `DbSession`、`ResourceLease`、`Execution` 与 Job stage 的运行时状态，
//! 见 `docs/architecture/platform/shared-boundaries-and-ports.md` 与
//! `docs/architecture/platform/connection-management.md` §14。
//!
//! 层与边界的硬约束（`shared-boundaries-and-ports.md` F-02 / F-03）：
//! 本 crate **不得**依赖 `tauri*`，**不得**引入任何 HTTP 框架，**不得**引入任何 UI 运行时。
//! 因此这里只允许出现传输中立的类型与端口，不出现 socket、窗口、命令分发。
//!
//! 夹具（`connection::testing`）由 `#[cfg(any(test, feature = "test-harness"))]` 门控，
//! 生产构建默认不含夹具代码；`src-tauri` 的 `datazen` crate 也不依赖它。

pub mod budget;
pub mod connection;
/// CM-60 门禁开销的测量口径（分位数算法 + 逐请求求和）。
pub mod latency;
/// 会话登记表接缝：`SessionPort` 的**唯一**契约面，Wave 2 的 registry 实现它、gateway 消费它。
pub mod registry;

/// 夹具门控开关的单一事实源。
///
/// `connection::testing` 模块的可见性完全由此常量决定：编译 `src-tauri`（未开
/// `test-harness`）时该模块不存在，因此生产路径在类型层面就看不到夹具。
#[cfg(any(test, feature = "test-harness"))]
pub use connection::testing;
