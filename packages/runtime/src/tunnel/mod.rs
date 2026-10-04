//! 隧道共享与引用计数（CM-32）。
//!
//! # 本模块解决什么
//!
//! **两个 session / Job 共用同版本隧道**，关掉其中一个时**不得**把隧道一起关掉；
//! 只有**最后一个引用**释放才真正拆除；隧道失败要传播给**全部**依赖资源。
//!
//! # 边界（重要）
//!
//! * 本模块**不是** tunnel 的发明者。词汇表与端口签名在
//!   `datazen_platform_api::ports::network`（**冻结上游，不得修改**）：
//!   [`TunnelSpec`] = 共享身份，[`TunnelBinding`] = 观测句柄，
//!   `NetworkProvider::{ensure_tunnel, release_tunnel}` = 契约。
//!   本模块是那份契约的一个**实现方**。
//! * 本模块**不认识** SSH / HTTP 代理 / WebSocket 任何一种隧道。宿主枚举
//!   （`driver-api::tunnel_types::TunnelKind` 与 `src-tauri::tunnel::Tunnel`）是
//!   **宿主实现细节**，决定「用哪种开法」，**不参与共享判定**，也不泄漏进来。
//! * 桌面 `NetworkProvider` 实现按 `shared-boundaries-and-ports.md:590` 属**接缝期**，
//!   不在本轨范围内（见 `progress.md` 的剩余项清单）。
//!
//! # 「同版本隧道」= `TunnelSpec` 全等值
//!
//! 它**不是** [`PoolKeyGeneration`]，也**不是** `CacheRevision`：
//!
//! | 概念 | 回答什么 | 与隧道的关系 |
//! | --- | --- | --- |
//! | `PoolKeyGeneration` | 哪条物理连接可以被谁复用 | **无隧道字段** |
//! | `CacheRevision` | 慢结果能不能回填 | 无关（缓存代次） |
//! | **`TunnelSpec`** | **哪条隧道被多少持有者共用** | **就是它** |
//!
//! 共享键比 `PoolKeyGeneration` **更细**：同一 `network_route_revision` 内，
//! 两条 `TunnelSpec` 不同的连接**仍各开各的隧道**（上游 `network.rs` 已有测试锁定）。
//! 反向的安全性不必在此重写 —— 路由轮换后 `ResourceManager::current_pool_key`
//! 已用 `PoolKeyRotated` 拒绝旧代申请，跨代隧道本就不会被共享。
//!
//! # 归还接线点：`return_resource`
//!
//! 资源生命周期侧**只有一条**归还入口 [`TunnelLedger::return_resource`]：
//! 给出租约 ⇒ 恰好一次释放，走与 `release` **同一条**归零路径。
//! 它不是「可选接线」—— 台账刻意**不提供**「只摘依赖、不减引用」的口子，
//! 因为那样的口子必然泄漏一个谁都不会再还的引用。
//!
//! # 唯一计数铁律
//!
//! 全系统**只有一份**隧道引用计数：`TunnelLedger` 的 `refs`。
//! `TunnelBinding.ref_count` 是观测快照，**任何**释放判断都不得读它。
//!
//! 这条铁律**不由类型系统保证**，而由字段审计保证：`&self` 方法并不排除
//! `Mutex`/`Cell` 内部可变性（两个夹具本身就在用 `Mutex`），编译器挡不住
//! 第二份账。真正的约束是「释放决策只读 `refs`，[`TunnelTransport`] 的返回
//! 值不携带计数」。反证与已知名洞见 [`transport`] 模块头。
//!
//! [`PoolKeyGeneration`]: crate::resource::PoolKeyGeneration

mod error;
mod ledger;
mod transport;

pub use error::TunnelError;
pub use ledger::{TunnelLease, TunnelLedger, TunnelRelease, TunnelState};
pub use transport::{TunnelFault, TunnelHandle, TunnelTransport};

/// 测试替身：记录式物理端口 + 可注入故障的隧道旅程。与 `resource::harness` 同一形态。
#[cfg(test)]
mod harness;

/// 三个注入点的失败传播旅程（CM-32 第三条断言）。
#[cfg(test)]
mod journey_failure;
/// 资源归还接线：`一次归还 = 恰好一次 release`。
#[cfg(test)]
mod journey_return;
/// CM-32 主旅程：两个 session 共用隧道（第一条、第二条断言）。
#[cfg(test)]
mod journey_sharing;
/// 唯一计数铁律的代数不变量。
#[cfg(test)]
mod journey_single_counter;
