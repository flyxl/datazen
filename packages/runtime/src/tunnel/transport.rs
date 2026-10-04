//! 隧道的物理接缝 —— 与 `resource::cleanup` 的 `PhysicalTransport` 同一形态：
//! runtime 不自己开隧道、不认识 SSH / HTTP 代理 / WebSocket 任何一种，
//! 只声明「需要有人能把一条隧道开起来、关掉」。
//!
//! # 这里为什么一个计数字段都没有
//!
//! 端口契约明写 `TunnelBinding.ref_count` 「仅供观测，不用于判断能否释放」。
//! 全系统的隧道引用计数**只允许一份**，就是 `TunnelLedger` 的 `refs`。
//! 若本 trait 的实现体再放一个 `refs`，两份账各自都「对」，
//! 但 CM-28「隧道不多减引用」和 CM-27「许可归零」会同时失效，且极难排查。
//!
//! ## 类型系统**挡不住**这件事（不要把纪律说成保证）
//!
//! 本 trait 的方法全部取 `&self`。这**不等于**「实现方无法维护计数」：
//! `&self` 只排除 `&mut self`，而 `Mutex` / `Cell` 是**内部可变性**，
//! 根本不需要 `&mut self`；`TunnelTransport: Send + Sync` 之下，
//! 放一个 `Mutex<usize>` 完全合法，能通过全部编译与测试。
//! 本模块的两个夹具就是活证据：`RecordingTunnelTransport` 有
//! `journal: Mutex<Vec<TunnelEvent>>`，crate 外宿主侧的
//! `HostTunnelTransport` 有 `events: Mutex<Vec<&'static str>>` ——
//! 「全 `&self` 所以装不下第二个计数」这句话按这两行即可证伪。
//!
//! 真实约束因此**不是**类型系统的，而是下面三条，靠字段审计与评审维持：
//!
//! 1. 经字段审计，本 trait 的三个返回值（`Option<TunnelHandle>`、`()`、
//!    `NetworkRouteRevision`）**都不携带任何计数** —— 宿主拿不到一份可以
//!    自己记着的账；`TunnelHandle(Arc<()>)` 是刻意不透明的单值。
//! 2. 释放决策（`ledger.rs` 的 `drain()`）读**两个**字段：`TunnelEntry::refs`
//!    （计数）与 `TunnelEntry::state`（终态守卫，只保证 `Closing` / `Unconfirmed`
//!    不再发第二次 close —— 它**不是**第二本账，不参与计数、不增减）。计数来源
//!    仍**只有一个**，即 `refs`；`TunnelBinding` 只在 `acquire` 处被快照出去，
//!    此后不再回流。
//! 3. 代数不变量由 `tunnel::journey_single_counter::single_counter_algebra_holds`
//!    钉住，三条各自独立：`open` 次数恒为 **1**（一条 spec 只开一条物理隧道，
//!    与 acquire 次数无关）；剩余计数 == acquire 次数 − release 次数；
//!    `close` 次数 ∈ {0, 1} —— 归零那一次为 1，重复释放不再增加。
//!
//! **已知名洞（登记于 CM-32 repair round 1，修复留待跟进轨）**：约束 (1) 当前
//! 由审计保证，编译器不保证它。已实证 —— 给 `RecordingTunnelTransport` 加一个
//! `close_tally: Mutex<usize>` 并让 `close_calls()` 改读它，**编译通过且全轨
//! 测试全绿**。两个候选修法（断言释放路径涉及的计数字段仅 `refs` 一个 /
//! 让 `drain` 按值从单个私有方法取计数而非直读结构体字段）本轮不裁决。

use std::sync::Arc;

use datazen_platform_api::id::{NetworkRouteRef, NetworkRouteRevision};
use datazen_platform_api::ports::network::TunnelSpec;

use super::error::TunnelError;

/// 一条**已打开**隧道的句柄。
///
/// 刻意不透明：宿主可能拿它去读本地端口，但端口号是**宿主实现细节**，
/// 不参与共享判定，也不泄漏进 runtime 的任何类型。
/// 共享身份只有 [`TunnelSpec`]。
#[derive(Clone, PartialEq, Eq)]
pub struct TunnelHandle(Arc<()>);

impl TunnelHandle {
    /// 由 [`TunnelTransport`] 的实现方铸造 —— 桌面侧 `NetworkProvider` 实现
    /// 住在本 crate 之外，所以构造权必须是 `pub`。
    ///
    /// 句柄本身不含任何共享语义：它**不参与**判定两条隧道是否同一条，
    /// 也不携带引用计数（见本文件模块头的「类型系统挡不住这件事」）。
    pub fn new() -> Self {
        Self(Arc::new(()))
    }
}

impl std::fmt::Debug for TunnelHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // 不打印内部指针：它对判定毫无帮助，还会在失败输出里制造噪音。
        f.write_str("TunnelHandle(<opaque>)")
    }
}

/// 隧道物理接缝。
///
/// 方法全部 `&self` 是为了让实现能放进 `Arc`，**不是**为了禁止内部计数 ——
/// `Mutex`/`Cell` 不需要 `&mut self`。不变量靠字段审计维持，见模块头。
pub trait TunnelTransport: Send + Sync + 'static {
    /// 建立一条隧道。返回 `None` 表示「这条 `spec` 其实不需要隧道」（等价于宿主
    /// `TunnelKind::None` 直连），此时**不产生**任何引用计数。
    fn open(&self, spec: &TunnelSpec) -> Result<Option<TunnelHandle>, TunnelError>;

    /// 关闭一条隧道。由台账在**引用归零时且仅此时**调用，且同一条 spec 最多一次。
    fn close(&self, spec: &TunnelSpec) -> Result<(), TunnelError>;

    /// 当前路由/隧道配置版本 —— 即 `PoolKey` 的 `networkRouteRevision` 分量
    /// （`connection-management.md:676`）。取不到就**不得**当它等于上一个已知值。
    fn revision(&self, route_ref: &NetworkRouteRef) -> Result<NetworkRouteRevision, TunnelError>;
}

/// 隧道失败种类。**只有两种**：`resource` 的 `DirectoryFault::{Unavailable, Rejected}`
/// 是同样形态的先例 —— 故障要可枚举，才能把失败注入钉死在测试里。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TunnelFault {
    /// 建立失败。台账据此**不落账**（不建 entry、不增引用）。
    Open,
    /// 拆除失败。台账据此把 entry 转 `Unconfirmed` 并**保留**，结果不明不静默丢弃。
    Close,
}
