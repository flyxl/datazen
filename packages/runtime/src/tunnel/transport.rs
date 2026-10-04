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
//! 用类型系统把第二份账消灭在编译期：**所有方法都取 `&self`**，
//! 因此 `dyn TunnelTransport` 可放进 `Arc` 而拿不到 `&mut self`，
//! 结构上就**无法**维护任何内部引用计数。
//! 代数不变量（`close` 次数 == ensure 次数 − release 次数）由
//! `tunnel::harness::single_counter_algebra_holds` 钉住。

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
    /// 也不携带引用计数（见模块头的「唯一计数铁律」）。
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
/// 方法全部 `&self` —— 见模块头「这里为什么一个计数字段都没有」。
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
