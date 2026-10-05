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

pub mod application;
pub mod budget;
pub mod connection;
/// 会话目录端口（`SessionDirectory`）的单进程实现：owner / runtimeEpoch / TTL 路由。
/// 只存路由信息，不存连接、不存凭据、不落盘。
pub mod directory;
pub mod gateway;
/// CM-60 门禁开销的测量口径（分位数算法 + 逐请求求和）。
pub mod latency;
/// 会话登记表接缝：`SessionPort` 的**唯一**契约面，Wave 2 的 registry 实现它、gateway 消费它。
pub mod registry;
/// 宿主资源台账与生命周期裁决（§3.2 模块表第 4 行）：物理连接表、`PoolKey` 索引、
/// 租约状态机、归池前的宿主侧检查、候选替换提交闸门。不做驱动 `Clean` 判定，不导出物理句柄。
pub mod resource;
/// 隧道共享与引用计数（CM-32）：把 platform-api **已冻结**的
/// `NetworkProvider::{ensure_tunnel, release_tunnel}` 契约落成实现方。
/// 共享身份是 `TunnelSpec` 全等值（全系统**唯一**一份引用计数就在 `TunnelLedger`）；
/// 物理开法是注入式接缝 —— 本模块不认识 SSH / 代理 / WebSocket 任何一种。
pub mod tunnel;

/// 夹具门控开关的单一事实源。
///
/// `connection::testing` 模块的可见性完全由此常量决定：编译 `src-tauri`（未开
/// `test-harness`）时该模块不存在，因此生产路径在类型层面就看不到夹具。
#[cfg(any(test, feature = "test-harness"))]
pub use connection::testing;
