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
//! ## 类型系统**挡不住**这件事（这条边界说清楚，不谎报成语言保证）
//!
//! 本 trait 的方法全部取 `&self`。这**不等于**「实现方无法维护计数」：
//! `&self` 只排除 `&mut self`，而 `Mutex` / `Cell` 是**内部可变性**，
//! 根本不需要 `&mut self`；`TunnelTransport: Send + Sync` 之下，
//! 放一个 `Mutex<usize>` 完全合法。本模块的夹具就是活证据：
//! `RecordingTunnelTransport` 有 `journal: Mutex<Vec<TunnelEvent>>`，crate 外宿主侧的
//! `HostTunnelTransport` 有 `events: Mutex<Vec<&'static str>>` ——
//! 「全 `&self` 所以装不下第二个计数」这句话按这两行即可证伪。
//!
//! 因此真正的约束是**结构**上的：「端口不许自存一份账，台账的计数只有一个读取面，
//! 而且台账的声明里只许有一份计数」。它由 `tunnel::single_counter_audit` 落成机械闸门
//! （随 `--lib` 跑，因而进 CI），不是靠评审。下面三条是闸门各条规则的口径。
//!
//! 1. **端口只有一份事实、一个折法。** 实现本 trait 的结构体**不允许**持有任何
//!    整数标量字段（含 `Mutex<usize>` / `Cell<isize>` / `Atomic*` /
//!    `struct CloseTally(Mutex<usize>)` 这类本地新类型壳，闸门递归展开类型名），
//!    且该文件里「事件 → 账」的**纯**折函数必须**恰好一个**；所有返回整数的
//!    `&self` 观测方法都必须是这个折法的投影。今天三份端口的样子：
//!    `RecordingTunnelTransport` 只有 `journal`，`HostTunnelTransport` 只有 `events`，
//!    `RecordingTunnelPort` 只有 `events`；读数全部走各自的 `tallies` / `tally`。
//!    （落在闸门 R4 + R5。）
//! 2. **台账的计数只有一个读取面，且声明里只有一份计数。** `TunnelEntry::refs` 只许被
//!    四个注册方法（`established` / `refs` / `add_reference` / `take_reference`）点访问，
//!    归零路径 `drain()` **按值**从 `take_reference` 取数，且释放结果里的数只许来自
//!    那个返回值（R1 + R2 + R2+）。留着 `refs` 却在旁边挂一份逐笔相同的镜像账
//!    （`shadow_refs: u32`）绕过的是**名字**而不是铁律，由 R7 按声明杀掉：
//!    `TunnelEntry` 的整数字段必须**恰好一个**且名为 `refs`。
//!    `drain()` 另外读的 `TunnelEntry::state` 是**终态守卫**，只保证 `Closing` /
//!    `Unconfirmed` 不再发第二次 close —— 它不参与计数、不增减，因此不是第二本账。
//!    `TunnelBinding` 只在 `acquire` 处被快照出去，此后不再回流。
//! 3. **观测账不得参与判断，代数另有钉子。** 台账自己的 `teardown_calls`
//!    （对外 `close_calls()`）是**观测**字段，闸门禁止它出现在任何比较 / 条件里（R3）。
//!    代数不变量由 `tunnel::journey_single_counter::single_counter_algebra_holds`
//!    钉住，三条各自独立：`open` 次数恒为 **1**（一条 spec 只开一条物理隧道，
//!    与 acquire 次数无关）；剩余计数 == acquire 次数 − release 次数；
//!    `close` 次数 ∈ {0, 1} —— 归零那一次为 1，重复释放不再增加。
//!    另有两条值层面的不变量：`every_port_reading_is_the_projection_of_one_journal_fold`
//!    沿整条旅程把端口每个读数与「现场从 journal 数出来的原始账」逐步对账；
//!    `the_ledger_and_the_port_tallies_agree_only_because_both_count_the_same_drain`
//!    绕过台账直接从接缝发一次 close，实证两本账的相等来自构造而非巧合。
//!
//! ## CM-32-FU1 的反例现在会怎样
//!
//! CM-32 repair round 1 实证过：给 `RecordingTunnelTransport` 加一个
//! `close_tally: Mutex<usize>` 并把 `close_calls()` 改成读它，**当年编译通过、
//! 全轨测试全绿** —— 第二本账就此伪装成第一本账。如今同一次改动被两条**独立**路径杀掉：
//! R4 按字段类型点名 `close_tally: Mutex<usize>`，R5 点名「`close_calls` 不再调用唯一
//! 折函数」。台账那一侧的同类伪装（新写一个函数直读 `entry.refs`、让 `drain()` 自己做
//! 减法、另挂一份镜像账、或把观测账读进判断条件）由 R1 / R2 / R7 / R3 分别杀掉。
//! 每条规则另带一条把**植入变异**（含上面那段原始反例逐字复现）喂给扫描器自己的
//! kill test，外加 R6 断言登记表与模块表一致 —— 把闸门裁小这件事本身就会转红。
//!
//! 边界要说明白：这是**测试期的结构闸门**，不是类型系统保证。它挡得住自然写出来的
//! 第二本账，挡不住蓄意对抗（`unsafe` 指针转义、跨 crate 静态、`proc-macro` 生成）；
//! `&self` + 内部可变性装得下任意计数这一点，任何扫描都消不掉。

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
/// `Mutex`/`Cell` 不需要 `&mut self`。实现方因此**装得下**一份自存的账，
/// 只是不许装：那份约束由 `tunnel::single_counter_audit` 机械执行，见模块头。
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
