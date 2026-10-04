//! 隧道引用计数台账：把 platform-api **已冻结**的 `NetworkProvider` 隧道共享契约落成实现。
//!
//! 上游契约在 [`datazen_platform_api::ports::network`]（冻结，不得修改）：
//!
//! * [`TunnelSpec`] = 「如何到端点」，`route_ref` × `via_host` × `via_port`，
//!   **全等即同版本隧道**；
//! * [`TunnelBinding`] 的 `ref_count` 明文「仅供观测，**不用于判断能否释放**
//!   （释放以 `release_tunnel` 的调用为准）」；
//! * `release_tunnel` 明文「引用计数归零才真正拆除；**重复释放是幂等的**」。
//!
//! # 唯一计数铁律
//!
//! 全系统**只能有一个**引用计数器，就是 [`TunnelLedger`] 的 `refs`。
//! [`TunnelTransport`] **不带**任何计数字段 —— 它只负责开/关真实隧道。
//! 双重记账会让 CM-28「隧道不多减引用」与 CM-27「许可归零」同时失效，
//! 而且两边各自看起来都对，极难排查。所以这里用类型系统把第二份账**在编译期消灭**：
//! 物理接缝 [`TunnelTransport`] 的方法全部 `&self`（可放进 `Arc<dyn TunnelTransport>`），
//! 因此它**不可能**持有需要 `&mut self` 的内部计数。
//! 代数不变量由 `tunnel::harness` 的 `single_counter_algebra_holds` 钉住。
//!
//! # 失败传播（CM-32 第三条断言）
//!
//! 隧道是**共享**资源：它中途死亡时受影响的不是某一个持有者，而是**全部登记过的
//! 依赖方**。所以每条 entry 带一份 `dependents` 清单，失败时全量枚举并交出。

use std::sync::Arc;

use datazen_platform_api::ports::network::{TunnelBinding, TunnelSpec};
use indexmap::IndexSet;

use super::error::TunnelError;
use super::transport::{TunnelHandle, TunnelTransport};
use crate::connection::types::LeaseId;

/// 一条隧道在其生命周期里的状态。
///
/// 状态机（CM-32 只用到 `Establishing`/`Live`/`Closing`/`Unconfirmed`/`Failed`
/// 这一条主干，其余变体是**完整性护栏**，防止任何路径把一条没人引用的隧道漏掉）：
///
/// ```text
///            open 成功
///  (无 entry) ──────▶ Live ──refs 归零──▶ Closing ──close 成功──▶ (无 entry)
///                        │                  │
///      引用它时报错 ◀─────┘                  └── close 失败 ──▶ Unconfirmed
///                        │
///                        └── report_failure ──▶ Failed（向依赖方传播，绝不静默）
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TunnelState {
    /// 台账里还没有这条隧道，或它已被彻底拆除（引用归零且 `close` 成功）。
    Absent,
    /// 正在建立。注意 `acquire` 全程持 `&mut self` 且同步调 `open`，
    /// 因此本状态不会被别的调用方观察到 —— 它只是把「可能失败」这件事在类型上标出来。
    Establishing,
    /// 已建立，可以服务流量。唯一的正常计数状态。
    Live,
    /// 引用已归零、`close` 已发出，等待确认。**期间不得再次 `close`**。
    Closing,
    /// `close` 失败，拆除结果不明（CM-73 的同类纪律：结果不明 ⇒ 不静默丢弃，
    /// 也不允许换一个「新的」把旧的蒙混过去）。
    Unconfirmed,
    /// 隧道中途死亡（上游断等）。引用**不减**，等待各持有方自己走归还流程。
    Failed,
}

impl TunnelState {
    /// 稳定字面码，用于日志与断言（不随文案改写而漂移）。
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Absent => "absent",
            Self::Establishing => "establishing",
            Self::Live => "live",
            Self::Closing => "closing",
            Self::Unconfirmed => "unconfirmed",
            Self::Failed => "failed",
        }
    }

    /// 还能否接受新的引用。`Closing`/`Unconfirmed` 不可再发引用：
    /// 这正是 CM-28「隧道不多减引用」必须防住的前置状态。
    pub const fn accepts_reference(&self) -> bool {
        matches!(self, Self::Live)
    }
}

/// 台账里的一条隧道：一个 [`TunnelSpec`] ⇒ 一条真实隧道 + 一份权威计数。
///
/// `refs` 是**全系统唯一**的隧道引用计数。`TunnelBinding.ref_count` 只是
/// 交给调用方的观测快照，**任何**释放判断都不得读它（见模块头「唯一计数铁律」）。
#[derive(Debug, Clone)]
struct TunnelEntry {
    /// 共享身份。查找**一律**用它的 `PartialEq`（=`==`），因为端口契约说的
    /// 「同一 `TunnelSpec` 共享引用计数」就是 `==`。冻结上游没有给它派生
    /// `Hash`/`Ord`，所以既不 `HashMap` 也不 `BTreeMap` —— 那都得另造一个键类型，
    /// 而另造的键一旦与 spec 漂移，就会静默地允许两条隧道互相复用对方的引用。
    /// 台账里同时存在的隧道只有个位数，线性查找的成本可以忽略。
    spec: TunnelSpec,
    state: TunnelState,
    /// 权威引用计数。归零即触发且仅触发一次 `close`。
    refs: u32,
    /// 依赖这条隧道的租约。隧道失败时**全量**枚举（共享资源的传播面就是它）。
    dependents: IndexSet<LeaseId>,
}

/// 一次 `acquire` 的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TunnelLease {
    /// 交给调用方的绑定句柄。`ref_count` 字段仅供观测。
    pub binding: TunnelBinding,
    /// 本次是否**新建**了隧道（false = 复用了一条已有的）。
    ///
    /// CM-32 第一条断言「第一步不关闭隧道」就是靠它与
    /// [`TunnelLedger::release`] 的 `closed` 一起钉的：第二个持有者拿到 `false`，
    /// 关掉它之后 `closed` 必须是 `false`。
    pub opened: bool,
    /// 本次新建时拿到的物理句柄；复用已有隧道时为 `None`。
    ///
    /// 刻意**不**存进台账 entry：台账只管「这条 spec 有几个引用」，
    /// 物理句柄是单次 `acquire` 的产物，不是共享状态。
    pub tunnel_handle: Option<TunnelHandle>,
}

/// 一次 `release` 的结果 —— **计数型证据**，供调用方与测试观察。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TunnelRelease {
    /// 本次是否**真的**发起了 `close`（引用归零且状态允许）。
    pub closed: bool,
    /// 释放后的权威计数。重复释放时它保持不变，**绝不为负**。
    pub refs: u32,
    /// 释放后的状态。`close` 失败时是 [`TunnelState::Unconfirmed`]。
    pub state: TunnelState,
}

/// 隧道引用计数台账。
///
/// 单线程（`&mut self`），与 `resource::ResourceManager` 同一纪律：
/// 并发由调用方的 actor 串行化，本层只裁决计数与状态。
pub struct TunnelLedger {
    transport: Arc<dyn TunnelTransport>,
    entries: Vec<TunnelEntry>,
    /// 已发出的 `close` 次数。**只是观测计数器，不是引用计数** ——
    /// 它不参与任何释放判断，只供测试读出代数不变量。
    close_calls: u64,
}

impl TunnelLedger {
    pub fn new(transport: Arc<dyn TunnelTransport>) -> Self {
        Self {
            transport,
            entries: Vec::new(),
            close_calls: 0,
        }
    }

    /// 共享身份的唯一定义点：`TunnelSpec` 全等。
    #[allow(dead_code)]
    fn position(&self, spec: &TunnelSpec) -> Option<usize> {
        self.entries.iter().position(|entry| entry.spec == *spec)
    }

    /// 引用一份隧道：同 [`TunnelSpec`] 全等则复用同一条隧道并把计数 +1。
    ///
    /// * 已有 `Live` entry ⇒ 计数 +1，**不开**隧道；
    /// * 无 entry ⇒ `transport.open`，成功则建 entry（计数 1），失败则**不落账**。
    ///
    /// 返回 `Ok(None)` 表示「**这条资源不经隧道**」，两种情形：
    ///
    /// 1. 传入 `None`（宿主 `TunnelKind::None` 直连）；
    /// 2. `spec.via_port == 0` —— 没有跳板端口就没有隧道可建；
    /// 3. `transport.open` 自报这条 spec 不需要隧道（`Ok(None)`）。
    ///
    /// 它们不是「一条空的隧道」，因此**不进台账、不占 entry、不产生任何引用计数**，
    /// 也**不会**在 `release` 时凭空 `close` 一条隧道。
    pub fn acquire(
        &mut self,
        spec: Option<&TunnelSpec>,
        dependent: &LeaseId,
    ) -> Result<Option<TunnelLease>, TunnelError> {
        let Some(spec) = spec else {
            return Ok(None);
        };
        if spec.via_port == 0 {
            return Ok(None);
        }

        if let Some(index) = self.position(spec) {
            let entry = &mut self.entries[index];
            if !entry.state.accepts_reference() {
                // 入口条件不满足就说清楚，绝不把调用方塞进一条正在拆除或已失败的隧道。
                return Err(TunnelError::NotShareable {
                    spec: spec.clone(),
                    state: entry.state,
                });
            }
            // 同一条租约重复引用同一条隧道 ⇒ 当场拒绝。放行会让计数比依赖清单多 1，
            // 一次归还只减 1，剩下的引用**永远没人还**，隧道泄漏。
            if entry.dependents.contains(dependent) {
                return Err(TunnelError::AlreadyHeld { spec: spec.clone() });
            }
            entry.refs += 1;
            let snapshot = entry.refs;
            entry.dependents.insert(dependent.clone());
            return Ok(Some(TunnelLease {
                binding: TunnelBinding {
                    spec: spec.clone(),
                    ref_count: snapshot,
                },
                opened: false,
                tunnel_handle: None,
            }));
        }

        // 建立失败 ⇒ **不落账**：引用不增、entry 不建，调用方拿到 Err 而不是
        // 一条指向死端口的隧道句柄。
        let Some(handle) = self.transport.open(spec)? else {
            // 物理接缝自报「这条 spec 不需要隧道」：同直连，不留任何账。
            return Ok(None);
        };
        let entry = TunnelEntry {
            spec: spec.clone(),
            state: TunnelState::Live,
            refs: 1,
            dependents: IndexSet::from([dependent.clone()]),
        };
        let binding = TunnelBinding {
            spec: spec.clone(),
            ref_count: entry.refs,
        };
        self.entries.push(entry);
        Ok(Some(TunnelLease {
            binding,
            opened: true,
            tunnel_handle: Some(handle),
        }))
    }

    /// 释放一次引用。**引用归零才关闭；重复释放是幂等的。**
    ///
    /// * 计数 0→1 时**不**关闭（CM-32 第一条断言）；
    /// * 计数 1→0 时发起**恰好一次** `close`，成功即清 entry；
    /// * 已知 entry 的重复释放 ⇒ 幂等返回，**不二次 close**；
    /// * `Failed` 的 entry 仍走同一条归零路径（只是不再发新引用）；
    /// * `close` 失败 ⇒ 转 [`TunnelState::Unconfirmed`]，entry **保留**
    ///   （结果不明不静默丢弃）。
    pub fn release(&mut self, spec: &TunnelSpec) -> TunnelRelease {
        let Some(index) = self.position(spec) else {
            // 没有这条隧道的释放：幂等（幂等指的是「不改变任何状态」，
            // 对账的职责在调用方 —— 释放一个从未引用过的 spec 是调用方的 bug，
            // 但它**不应该**在这里 panic，也不应该凭空 close 一条隧道）。
            return TunnelRelease {
                closed: false,
                refs: 0,
                state: TunnelState::Absent,
            };
        };
        self.drain(index)
    }

    /// **一条资源归还 ⇒ 恰好一次释放。**（CM-32 与 CM-27 的接线点）
    ///
    /// 这是资源生命周期侧唯一该调用的归还入口：给定租约，摘掉它对隧道的那份引用，
    /// 并走与 [`Self::release`] **完全同一条**归零路径（同一个 `drain`），因此
    /// 「最后一份引用才关闭」「重复释放幂等」在这里同样成立，不可能被绕过。
    ///
    /// * 该租约**没有**依赖任何隧道（直连、或早就不经隧道）⇒ `None`，零副作用；
    /// * 同一条租约**重复归还** ⇒ 第二次起 `None` —— 一次归还只对应一次释放，
    ///   不会因为多调一次就多减一次、也不会二次 `close`；
    /// * 归还时隧道**仍在被别的资源共用** ⇒ `closed == false`，隧道照常服务它们；
    /// * 它是最后一份引用 ⇒ 恰好一次 `close`。
    pub fn return_resource(&mut self, dependent: &LeaseId) -> Option<TunnelRelease> {
        let index = self
            .entries
            .iter()
            .position(|entry| entry.dependents.contains(dependent))?;
        // 先摘依赖，再归零：资源已经走了，无论隧道状态如何它都不再是依赖方。
        self.entries[index].dependents.shift_remove(dependent);
        Some(self.drain(index))
    }

    /// 归零路径：减一份引用；**归零才关闭**，且恰好一次。`release` 与
    /// `return_resource` 共用它，两条归还入口不可能有第二种语义。
    fn drain(&mut self, index: usize) -> TunnelRelease {
        let spec = self.entries[index].spec.clone();
        let entry = &mut self.entries[index];

        // `Closing` / `Unconfirmed`：close 已经发出或结果不明，**不得再发第二次**。
        // `Failed` 不在此列 —— 隧道在中途死了不等于它已经被拆除，既有持有方的
        // 归还仍要走完整条归零路径，否则 entry 会永远留在台账里漏掉。
        if matches!(entry.state, TunnelState::Closing | TunnelState::Unconfirmed) {
            return TunnelRelease {
                closed: false,
                refs: entry.refs,
                state: entry.state,
            };
        }

        debug_assert!(
            entry.refs > 0,
            "a live tunnel entry must hold at least one reference"
        );
        // 饱和减法：refs 是 u32，先判零再减，保证任何路径都减不到 0 以下。
        entry.refs = entry.refs.saturating_sub(1);

        if entry.refs > 0 {
            return TunnelRelease {
                closed: false,
                refs: entry.refs,
                state: entry.state,
            };
        }

        // 计数归零 ⇒ 关闭，且**恰好一次**。
        entry.state = TunnelState::Closing;
        match self.transport.close(&spec) {
            Ok(()) => {
                self.close_calls += 1;
                self.entries.remove(index);
                TunnelRelease {
                    closed: true,
                    refs: 0,
                    state: TunnelState::Absent,
                }
            }
            Err(_) => {
                // 结果不明：保留 entry 并标记，绝不静默丢弃。
                entry.state = TunnelState::Unconfirmed;
                TunnelRelease {
                    closed: false,
                    refs: 0,
                    state: TunnelState::Unconfirmed,
                }
            }
        }
    }

    /// 隧道中途死亡。返回**全部**依赖租约，供调用方逐个隔离。
    ///
    /// 引用计数**不减** —— 各持有方还没归还，账要等它们自己走归还流程；
    /// 在这里减引用正是 CM-28「隧道不多减引用」要防的错误。
    pub fn report_failure(&mut self, spec: &TunnelSpec) -> Vec<LeaseId> {
        let Some(index) = self.position(spec) else {
            return Vec::new();
        };
        let entry = &mut self.entries[index];
        entry.state = TunnelState::Failed;
        entry.dependents.iter().cloned().collect()
    }

    /// 依赖一条**不同规格**隧道的租约，不得出现在这条隧道的依赖清单里。
    pub fn depends_on(&self, spec: &TunnelSpec, dependent: &LeaseId) -> bool {
        self.position(spec)
            .is_some_and(|index| self.entries[index].dependents.contains(dependent))
    }

    /// 权威计数快照。仅供观测，**不得**用于判断能否释放。
    pub fn ref_count(&self, spec: &TunnelSpec) -> Option<u32> {
        self.position(spec).map(|index| self.entries[index].refs)
    }

    /// 当前状态快照。
    pub fn state(&self, spec: &TunnelSpec) -> TunnelState {
        self.position(spec)
            .map_or(TunnelState::Absent, |index| self.entries[index].state)
    }

    /// 台账里还没被彻底拆除的隧道条数。
    pub fn live_tunnels(&self) -> usize {
        self.entries.len()
    }

    /// 累计 `close` 调用次数。**观测用**，不参与任何释放判断。
    pub fn close_calls(&self) -> u64 {
        self.close_calls
    }
}
