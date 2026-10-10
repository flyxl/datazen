//! 宿主侧归还裁决：**能力闸 + 双条件**判定 + 清理报告 + 注入式物理端口。
//!
//! ## 边界一在这里落地
//!
//! 驱动的 `Clean` 以 [`DriverCleanVerdict`] 进入本文件，它是一个**结论**（`bool`），
//! 刻意不携带任何判定依据：宿主看不到 `Clean` 是怎么算出来的，也没有在本模块重算它的入口。
//! 宿主只做**自己的**检查（[`HostConditionSnapshot`]）。两侧**都**通过才允许回池 ——
//! 形状对齐 `platform-api` 的 `PoolReturnVerdict::decide`。
//!
//! ## 能力闸：在双闸**之前**
//!
//! 归池准入还要读一项驱动侧事实：`ResetForReuse`（`connection::capability`，宿主侧那份）。
//! 它在**能力声明阶段**就确定了，比运行时 `Clean` 更根本 —— 驱动说没有 verified
//! reset-to-baseline 路径时，归池候选当场作废，连那次注定失败的复位往返都不必跑
//! （§9.4「reset 不支持…直接关闭」）。`Verified` 时本闸完全静默，双闸一步都不少。
//!
//! 闸门**位置**：它坐在 [`HostConditionSnapshot::first_blocker`] 这同一条报障通道里，
//! 排在宿主四项**之后**。理由是报障纪律而不是判定强度 ——「具体状态优先于静态事实」，
//! 与 `FixedSessionNotPoolable` 完全同构（`fixed_session` 也在那条链的末尾）。

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::connection::capability::ResetForReuse;
use crate::connection::port::CancelDisposition;
use crate::connection::{ExecutionId, HandleId, HandleKind, LeaseId, ResourceId, SessionHandleRef};

use crate::resource::lease::{LeasePurpose, LeaseRecord, LeaseRequest, LeaseState};
use crate::resource::tunnel_wiring::TunnelDisposition;
use crate::resource::ResourceError;

/// 驱动自报的 `Clean` 结论。
///
/// **只有一个私有 bool 字段**是刻意的：判定依据属于驱动，宿主拿到结论就够了。
/// 想在这里加「按事务状态 / 按连接串推 Clean」之类的字段等于把驱动判定搬进宿主，
/// 边界一立刻破。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DriverCleanVerdict {
    clean: bool,
}

impl DriverCleanVerdict {
    /// 驱动报告：完整结果已消费、复位已完成、可以安全复用。
    pub const fn reported_clean() -> Self {
        Self { clean: true }
    }

    /// 驱动报告：不干净（结果未消费完 / 复位失败 / 协议未排空…）。**判定依据由驱动掌握。**
    pub const fn reported_unclean() -> Self {
        Self { clean: false }
    }

    pub const fn is_clean(self) -> bool {
        self.clean
    }
}

/// 未完成的协议负债。
///
/// 「协议未排空」是三个独立阻断项之一：驱动报 `Clean` 也**不代表**宿主的
/// 协议已经排空，这两件事由不同的人负责。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolDebt {
    pub protocol_id: String,
    /// 债务的定性说明（协议名 / 订阅 / 流），只用于记账与日志。
    pub detail: &'static str,
}

impl ProtocolDebt {
    pub fn new(protocol_id: impl Into<String>, detail: &'static str) -> Self {
        Self {
            protocol_id: protocol_id.into(),
            detail,
        }
    }
}

/// 宿主侧的复用前置条件快照。
///
/// 四个字段正是三个独立场景加一个归属禁用：
/// (a) `active_executions` 非空；(b) `outstanding_protocol` 非空；
/// (c) `unreleased_handles` 里有**仍未终结**的句柄；(d) `owner_disabled`。
/// **任意一项成立即拒绝复用**，三者互不替代 —— 驱动报 `Clean` 不会让其中任何一项消失。
///
/// 字段 (c) 的读法只有一处（[`Self::outstanding_handles`]）：`closed = true` 的句柄
/// **不算**阻断项。字段名仍叫 `unreleased_handles` 是因为它是调用方交进来的原始
/// 声明（含已关闭的那些），过滤在读取侧做，不在写入侧做 —— 写入侧过滤会让
/// 「调用方报过但已关闭」这件事在快照里消失，而报表要能看出它曾经存在过。
///
/// 第五个字段 `fixed_session` 不是「故障」，而是**用途**声明：用户自开的
/// 任意 SQL 会话关闭时直接关物理资源。这类会话即使四项检查全清也不进空闲池。
///
/// 第六个字段 `reset_for_reuse` 同理不是「故障」，而是**能力**声明：驱动在能力
/// 声明阶段说了有没有 verified reset-to-baseline 路径。说没有 ⇒ 归池候选当场作废，
/// 连那次注定失败的复位往返都不必跑（§9.4「reset 不支持…直接关闭」）。
///
/// 后两者都被算作 `HostConditionSnapshot`（而非另开端口）的理由：它们与四项检查
/// **同向**（都是「宿主侧已知 ⇒ 拒绝复用」），且都不携带判定依据 —— 与
/// `FixedSessionNotPoolable` 完全同构。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostConditionSnapshot {
    pub active_executions: Vec<ExecutionId>,
    pub outstanding_protocol: Vec<ProtocolDebt>,
    pub unreleased_handles: Vec<SessionHandleRef>,
    pub owner_disabled: Option<String>,
    /// 固定会话：不参与归还池。
    pub fixed_session: bool,
    /// 归池复用的复位能力声明。`Unsupported` ⇒ 不进归池候选，直接关闭。
    ///
    /// 默认 `Verified`（与 `CapabilitySnapshot::defaults()` 对齐）：它让「调用方
    /// 没有供给能力」时的行为与本次改动之前**逐字节相同**（仍跑一次复位再发现不能
    /// 归池）。换句话说，忘记供给的失败模式不会比改动前更糟，只会退回旧行为。
    pub reset_for_reuse: ResetForReuse,
}

impl HostConditionSnapshot {
    /// **仍未终结**的会话级句柄：`closed = true` 的不算。
    ///
    /// 这是本快照对「句柄」这一项的**唯一**口径，`is_clear` / `first_blocker` /
    /// `all_blockers` / `ResourceManager::release` 的 `needs_reset` 四处一律经它读，
    /// 不再各自 `.is_empty()` 或 `iter().any(|h| !h.closed)`。
    ///
    /// 为什么必须过滤 `closed`：`SessionHandleRef.closed` 是「已自行关闭、登记还挂着」
    /// 的终态标志（`lease_manager.rs` 的 `deregister_handle` 文档把这称为登记与摘除的
    /// 「对称另一半」；`ExecutionCompletion` 也正是这样上报已完结的句柄）。宿主把它
    /// 提交/回滚掉之后，它就不再是一项未了义务 —— 再拿它挡归池，租约会因为一条
    /// **死**句柄永远进不了空闲池，池化收益整体失效且无任何测试覆盖过这种输入。
    ///
    /// 与 [`CleanupReport::outstanding_handles`] 的过滤条件逐字一致，两份口径因此
    /// 不可能再对同一个快照给出「不清」与「没有未了句柄」两种答案。
    pub fn outstanding_handles(&self) -> impl Iterator<Item = &SessionHandleRef> {
        self.unreleased_handles
            .iter()
            .filter(|handle| !handle.closed)
    }

    /// 四项宿主条件全满足（注意：这**不代表**可以回池，驱动那一侧还没看，
    /// 用途也可能根本不允许归还）。
    ///
    /// 句柄那一项读 [`Self::outstanding_handles`]，不是 `unreleased_handles.is_empty()`
    /// —— 后者会把已关闭的句柄也算成阻断项，见那边的理由。
    pub fn is_clear(&self) -> bool {
        self.active_executions.is_empty()
            && self.outstanding_protocol.is_empty()
            && self.outstanding_handles().next().is_none()
            && self.owner_disabled.is_none()
    }

    /// 宿主侧认为「这条租约有资格进空闲池」：四项检查全清，且用途允许归还。
    ///
    /// 不再是 `const fn`：句柄那一项经 [`Self::outstanding_handles`] 读，而它返回
    /// 迭代器。已核实全仓没有任何调用点依赖 const（`admits_to_pool` 本身无生产
    /// 调用方，`is_clear` 只被本方法与两处文档提及）。
    pub fn admits_to_pool(&self) -> bool {
        self.is_clear() && !self.fixed_session
    }

    /// 按固定顺序取**第一个**阻断项，保证裁决结果稳定可复现：
    /// 活动执行 → 未完成协议 → 未释放句柄 → 归属被禁用 → 固定会话 → 复位能力未声明。
    ///
    /// 复位能力排在**最后**是同一定序原则的延伸：它不覆盖任何一条宿主条件的阻断，
    /// 因为那些是「这一侧**当前**的状态」，而能力是「驱动侧**从开始**就如此」的静态事实。
    /// 排最后意味着**先报更具体、更可操作的那一条**。
    pub fn first_blocker(&self) -> Option<ReturnReason> {
        self.active_executions
            .first()
            .map(|execution_id| ReturnReason::ActiveExecution {
                execution_id: execution_id.clone(),
            })
            .or_else(|| {
                self.outstanding_protocol
                    .first()
                    .map(|debt| ReturnReason::OutstandingProtocol {
                        protocol_id: debt.protocol_id.clone(),
                        detail: debt.detail,
                    })
            })
            .or_else(|| {
                self.outstanding_handles().next().map(|handle| {
                    ReturnReason::UnreleasedSessionHandle {
                        handle_id: handle.handle_id.clone(),
                        kind: handle.kind,
                    }
                })
            })
            .or_else(|| {
                self.owner_disabled
                    .as_ref()
                    .map(|owner_key| ReturnReason::OwnerDisabled {
                        owner_key: owner_key.clone(),
                    })
            })
            .or_else(|| {
                self.fixed_session
                    .then_some(ReturnReason::FixedSessionNotPoolable)
            })
            .or_else(|| self.reset_blocker())
    }

    /// 复位能力阻断项：驱动**没有 verified baseline** 时不产生阻断（归池候选直接作废），
    /// 否则本条不参与，交给双闸自己判。
    fn reset_blocker(&self) -> Option<ReturnReason> {
        (!self.reset_for_reuse.admits_reuse()).then_some(ReturnReason::DriverResetUnsupported)
    }

    /// 全部阻断项，用于报告（不是只报第一个）。
    pub fn all_blockers(&self) -> Vec<ReturnReason> {
        let mut blockers = Vec::new();
        for execution_id in &self.active_executions {
            blockers.push(ReturnReason::ActiveExecution {
                execution_id: execution_id.clone(),
            });
        }
        for debt in &self.outstanding_protocol {
            blockers.push(ReturnReason::OutstandingProtocol {
                protocol_id: debt.protocol_id.clone(),
                detail: debt.detail,
            });
        }
        for handle in self.outstanding_handles() {
            blockers.push(ReturnReason::UnreleasedSessionHandle {
                handle_id: handle.handle_id.clone(),
                kind: handle.kind,
            });
        }
        if let Some(owner_key) = &self.owner_disabled {
            blockers.push(ReturnReason::OwnerDisabled {
                owner_key: owner_key.clone(),
            });
        }
        if self.fixed_session {
            blockers.push(ReturnReason::FixedSessionNotPoolable);
        }
        if let Some(reason) = self.reset_blocker() {
            blockers.push(reason);
        }
        blockers
    }

    pub fn with_active_execution(mut self, execution_id: ExecutionId) -> Self {
        self.active_executions.push(execution_id);
        self
    }

    pub fn with_outstanding_protocol(mut self, debt: ProtocolDebt) -> Self {
        self.outstanding_protocol.push(debt);
        self
    }

    pub fn with_unreleased_handle(mut self, handle: SessionHandleRef) -> Self {
        self.unreleased_handles.push(handle);
        self
    }

    pub fn with_disabled_owner(mut self, owner_key: impl Into<String>) -> Self {
        self.owner_disabled = Some(owner_key.into());
        self
    }

    /// 标记为固定会话（用户自开的会话关闭时直接关物理资源）。
    pub const fn as_fixed_session(mut self) -> Self {
        self.fixed_session = true;
        self
    }

    /// 标记复位能力（驱动能力声明阶段的结论）。
    pub fn with_reset_for_reuse(mut self, reset_for_reuse: ResetForReuse) -> Self {
        self.reset_for_reuse = reset_for_reuse;
        self
    }
}

/// 一次归还裁决的**第一个**失败理由。
///
/// 变体集合刻意与 `platform-api` 的 `QuarantineReason` 语义对齐，
/// 但**不共用那个类型**：那个属于冻结的上游预算端口，宿主资源层不依赖它的变体集。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReturnReason {
    /// 驱动报告不干净。依据由驱动掌握，宿主只转发结论。
    DriverUnclean,
    ActiveExecution {
        execution_id: ExecutionId,
    },
    OutstandingProtocol {
        protocol_id: String,
        detail: &'static str,
    },
    UnreleasedSessionHandle {
        handle_id: HandleId,
        kind: HandleKind,
    },
    OwnerDisabled {
        owner_key: String,
    },
    /// 固定会话不参与归还。这不是故障，是用途。
    FixedSessionNotPoolable,
    /// **归池前的复位没有被确认**。
    ///
    /// 这不是「宿主四项条件之一没过」——四项全清才轮得到归池；也不是驱动重算：
    /// 它是 §9.4「reset 不支持、失败或超时直接关闭」在宿主侧**实测**到的物理事实。
    /// 归池裁决因此作废，落点是关闭（`ResourceManager::refuse_park_after_failed_reset`）。
    ///
    /// `transport` 装的是 [`ResourceError::reason`] 那个稳定字面码，只用于记账与日志。
    HostResetUnconfirmed {
        transport: &'static str,
    },
    /// 驱动**明确**回答「无法证明已复位」（[`ResetDisposition::Discard`]）。
    ///
    /// 这与 [`Self::HostResetUnconfirmed`] 是**两件事**，不许合并：这里是驱动给了
    /// 确定的答复，只是答复是「不能复用」；那里是宿主压根没拿到答复（`Err`）。
    /// 把两者压成一条路径，就等于把「知道不能复用」谎报成「不知道」。
    ///
    /// **这是正常的、预期内的结果，不是故障。** 首版 15/15 path 驱动都落这一格
    /// （全部声明 `ResetForReuse::Unsupported`），所以它按 `info` 记账，
    /// **不得**记成 warn —— 否则真正的告警会被每一条正常归还淹没。
    DriverDiscardedReset,
    /// **驱动在能力声明阶段就说了不复位**，因此这条租约根本不进归池候选。
    ///
    /// 与 [`Self::HostResetUnconfirmed`] 是同一件事（§9.4「reset 不支持…直接关闭」）的
    /// **两个不同证据等级**：那条是宿主**实测**到复位端口失败，本条是驱动**声明**过
    /// 「没有 verified baseline」——能力快照一出来就是确定事实，不必等一次注定失败的往返。
    ///
    /// 它仍然**不是故障**：`FixedSessionNotPoolable` 已经示范过「不归池」可以表达成用途。
    /// 15 个 path 驱动全部声明 `ResetForReuse::Unsupported`（驱动侧 `Default` 亦是
    /// `Unsupported`，失败安全），首版池为空是**正确行为**，这里让它显式、可观测。
    DriverResetUnsupported,
}

impl ReturnReason {
    pub const fn reason_code(&self) -> &'static str {
        match self {
            Self::DriverUnclean => "driverUnclean",
            Self::ActiveExecution { .. } => "hostActiveExecution",
            Self::OutstandingProtocol { .. } => "hostOutstandingProtocol",
            Self::UnreleasedSessionHandle { .. } => "hostUnreleasedSessionHandle",
            Self::OwnerDisabled { .. } => "hostOwnerDisabled",
            Self::FixedSessionNotPoolable => "hostFixedSessionNotPoolable",
            Self::HostResetUnconfirmed { .. } => "hostResetUnconfirmed",
            Self::DriverDiscardedReset => "driverDiscardedReset",
            Self::DriverResetUnsupported => "driverResetUnsupported",
        }
    }

    /// 这条阻断项是不是**故障**。
    ///
    /// 分发依据是「这是不是一次预期内的正常答复」，而不是文案好看：
    /// [`Self::DriverDiscardedReset`] 是驱动按契约给出的正常判决（首版全部驱动都这样），
    /// 记成 warn 会让真正的告警淹在常态里。其余变体都是异常或宿主主动拒绝。
    pub const fn is_fault(&self) -> bool {
        !matches!(self, Self::DriverDiscardedReset)
    }
}

/// 归还裁决结论。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReturnDecision {
    /// 两侧都过：驱动报 `Clean`，宿主四类条件全空。**只有这一种允许回池。**
    Returned,
    /// 任何一侧没过：拒绝复用，转关闭或隔离。
    CleanupRequired(ReturnReason),
}

impl ReturnDecision {
    /// 双条件裁决。
    ///
    /// 顺序与 `PoolReturnVerdict::decide` 一致：**先看驱动**，驱动不干净就直接以
    /// `DriverUnclean` 收口 —— 因为在那种情况下再报「活动执行」是误导，根因在驱动侧。
    /// 驱动过了才轮到宿主侧，逐项独立判定，任一不满足立刻拒绝。
    ///
    /// 复位能力**不**在这里特判，它经 [`Self::first_blocker`] 同一条通道进来，排在
    /// 宿主条件之后（见那边的定序理由）。这样「具体状态优先于静态事实」的报障纪律
    /// 对能力同样成立：有活动执行时报的是活动执行，不会让能力把根因盖掉。
    pub fn decide(driver: DriverCleanVerdict, host: &HostConditionSnapshot) -> Self {
        if !driver.is_clean() {
            return Self::CleanupRequired(ReturnReason::DriverUnclean);
        }
        match host.first_blocker() {
            Some(reason) => Self::CleanupRequired(reason),
            None => Self::Returned,
        }
    }

    pub const fn admits_to_idle_pool(&self) -> bool {
        matches!(self, Self::Returned)
    }

    pub fn reason(&self) -> Option<&ReturnReason> {
        match self {
            Self::Returned => None,
            Self::CleanupRequired(reason) => Some(reason),
        }
    }
}

/// 清理后的处置。与 `ReleaseDisposition` 同义：**回池不释放物理预算**，因为连接还活着。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CleanupDisposition {
    /// 回到空闲池，等待同代请求再次签发；物理预算**仍被占用**。
    ReturnedToPool,
    /// 物理关闭已确认；物理预算**在此刻释放**。
    Closed,
    /// 隔离中：既不复用也未关闭，物理预算**继续占用**直到排障后强制关闭。
    Quarantined,
}

impl CleanupDisposition {
    /// 只有真正关闭才释放物理预算（回池不释放）。
    pub const fn releases_physical_budget(self) -> bool {
        matches!(self, Self::Closed)
    }
}

/// 裁决 + 派生出的状态跃迁。决策是纯函数，副作用由 `ResourceManager` 执行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanupPlan {
    pub decision: ReturnDecision,
    pub disposition: CleanupDisposition,
    /// 裁决后租约要进入的状态。
    pub next_state: LeaseState,
    pub blockers: Vec<ReturnReason>,
}

impl CleanupPlan {
    pub fn of(
        record: &LeaseRecord,
        host: &HostConditionSnapshot,
        driver: DriverCleanVerdict,
    ) -> Self {
        // 固定会话**结构上**就不归池：这条不变量来自租约自身的用途，
        // 不能依赖调用方记得把快照里的 `fixed_session` 打开 —— 否则一次忘记就等于
        // 把仍然持有着用户会话的连接塞回共享空闲池。
        let mut effective = host.clone();
        if record.purpose == LeasePurpose::FixedSession {
            effective.fixed_session = true;
        }

        let decision = ReturnDecision::decide(driver, &effective);
        let blockers = match decision.reason() {
            None => Vec::new(),
            Some(first) => {
                let mut all = vec![first.clone()];
                all.extend(effective.all_blockers().into_iter().filter(|r| r != first));
                all
            }
        };

        if decision.admits_to_idle_pool() {
            return Self {
                decision,
                disposition: CleanupDisposition::ReturnedToPool,
                next_state: LeaseState::Acquired,
                blockers,
            };
        }

        // 不复用时的落点：**能安全关闭就走关闭，不能就隔离。**
        //
        // 正在执行的连接、或归属已被禁用的连接，一律**不许静默关闭**：前者随时可能被
        // 驱动写回，后者连裁决权都没有，都留在隔离里等运维处置。
        //
        // 还挂着未释放会话级句柄（事务/游标/服务端预备对象）**不在此列**：目标
        // 行为恰恰是先在原资源上回滚并解除映射再关闭，回滚由 `ResourceManager::release`
        // 在 `CleanupDisposition::Closed` 分支上做；**回滚结果不明**才由它把处置改成
        // 隔离，不允许换个新资源把旧事务蒙混过去。
        let must_quarantine =
            record.active_execution.is_some() || effective.owner_disabled.is_some();

        Self {
            decision,
            disposition: if must_quarantine {
                CleanupDisposition::Quarantined
            } else {
                CleanupDisposition::Closed
            },
            next_state: if must_quarantine {
                LeaseState::Quarantined
            } else {
                LeaseState::Closing
            },
            blockers,
        }
    }
}

/// 一次清理裁决的完整回执（A1 公开类型）。
///
/// 它回答四个问题：**判了什么**、**落到哪个处置**、**被什么挡住**、
/// **物理预算有没有真的释放**。审计与 Job 历史靠它复原当时的判断依据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanupReport {
    pub resource_id: ResourceId,
    pub lease_id: LeaseId,
    pub decision: ReturnDecision,
    pub disposition: CleanupDisposition,
    pub blockers: Vec<ReturnReason>,
    /// 裁决时仍挂在会话上的句柄（已关闭的除外）。
    ///
    /// 复位成功后这里为空：映射已在原资源上解除，没有任何句柄还能提交。
    pub outstanding_handles: Vec<HandleId>,
    /// 回收时是否已在**原资源**上执行过复位（回滚事务并解除映射）。
    /// 为真 ⇒ `Close` 事件排在 `Reset` 之后，这是可审计的行为证据。
    pub session_reset_performed: bool,
    pub physical_budget_released: bool,
    /// 这次处置对**隧道引用**做了什么。
    ///
    /// 与 [`Self::physical_budget_released`] 是同一次判据的两半，放在一起才看得出配对：
    /// 不变式是 `tunnel.pairs_with_budget_release() == physical_budget_released`。
    /// 直连租约（没有 `tunnel_spec`）报 [`TunnelDisposition::NoReference`]，
    /// 它与 `physical_budget_released` 并不冲突 —— 「从没占过隧道」也是一种配对。
    pub tunnel: TunnelDisposition,
    pub decided_at_nanos: u64,
}

impl CleanupReport {
    pub fn from_plan(
        record: &LeaseRecord,
        host: &HostConditionSnapshot,
        plan: CleanupPlan,
        now_nanos: u64,
        session_reset_performed: bool,
        tunnel: TunnelDisposition,
    ) -> Self {
        let physical_budget_released = plan.disposition.releases_physical_budget();
        Self {
            resource_id: record.resource_id.clone(),
            lease_id: record.lease_id.clone(),
            decision: plan.decision,
            disposition: plan.disposition,
            blockers: plan.blockers,
            outstanding_handles: if session_reset_performed {
                Vec::new()
            } else {
                host.outstanding_handles()
                    .map(|handle| handle.handle_id.clone())
                    .collect()
            },
            session_reset_performed,
            physical_budget_released,
            tunnel,
            decided_at_nanos: now_nanos,
        }
    }

    pub const fn returned_to_pool(&self) -> bool {
        matches!(self.disposition, CleanupDisposition::ReturnedToPool)
    }

    pub const fn quarantined(&self) -> bool {
        matches!(self.disposition, CleanupDisposition::Quarantined)
    }
}

/// 禁用/删除时对**已经在跑**的执行，记录它的**真实**取消处置。
///
/// 把「取消失败」或「驱动不支持精确取消」谎报成「已取消」，会让审计与 Job 历史失真：
/// 调用者会以为结果集是完整的，实际可能是一个半截的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionCancellationRecord {
    pub execution_id: ExecutionId,
    pub resource_id: ResourceId,
    pub outcome: CancellationOutcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancellationOutcome {
    /// 精确取消已发出且被驱动接受。
    Cancelled { observed_at_nanos: u64 },
    /// 执行在取消到达之前已经终结：按已完成记账，**不得**当成已取消。
    AlreadyFinished { observed_at_nanos: u64 },
    /// 驱动不支持精确取消（F9：这是**正常返回值，不是异常**）。
    Unsupported { observed_at_nanos: u64 },
    /// 取消失败：物理资源必须进隔离，原始错误原样记账。
    Failed {
        observed_at_nanos: u64,
        reason: &'static str,
    },
}

impl CancellationOutcome {
    /// 把驱动侧处置 + 传输层错误翻译成**实测**结论。
    pub fn observed(disposition: Result<CancelDisposition, ResourceError>, now_nanos: u64) -> Self {
        match disposition {
            Ok(CancelDisposition::Requested) => Self::Cancelled {
                observed_at_nanos: now_nanos,
            },
            Ok(CancelDisposition::AlreadyFinished) => Self::AlreadyFinished {
                observed_at_nanos: now_nanos,
            },
            Ok(CancelDisposition::Unsupported) => Self::Unsupported {
                observed_at_nanos: now_nanos,
            },
            Err(error) => Self::Failed {
                observed_at_nanos: now_nanos,
                reason: error.reason(),
            },
        }
    }

    pub const fn cancelled(&self) -> bool {
        matches!(self, Self::Cancelled { .. })
    }
}

/// 一次复位的**驱动判决**，由 [`PhysicalTransport::reset`] 原样带回宿主。
///
/// 它回答的是「这条资源能不能证明自己回到了初始化基线」，而不是「复位这个动作有没有
/// 报错」——后者是 `Err`，是**第三种**结果，不属于本类型：
///
/// | 结果 | 含义 | 宿主落点 |
/// |---|---|---|
/// | `Ok(Clean)` | 已回到基线，可复用 | 允许归池（且宿主那四项还须全清） |
/// | `Ok(Discard)` | **无法证明**回到基线，不得再次签发 | 关闭并释放物理预算 |
/// | `Err(..)` | 握手从未确认，结果**未知** | 归池作废；落点见 [`crate::resource::ResourceManager::release`] |
///
/// ## 为什么本模块自己定一个，而不是复用 `driver-api` 的 `ResetDisposition`
///
/// 两者**形状相同**（都是 `Clean` / `Discard`），语义逐字对齐，但**刻意不共用符号**：
///
/// * `packages/driver-api/src/session.rs` 那个是**驱动协议的冻结面**。它的增删由
///   `PROTOCOL_VERSION` 与协议成员闸门治理（见 `docs/architecture/platform/
///   driver-capability-migration.md` §6），而本类型是宿主资源层的内部端口返回。
///   共用符号等于让驱动协议的任何一次演进顺着类型别名渗进 `resource/**` 的公开面。
/// * 本模块的边界声明要求它只收发**账目标识**与自有结论（见 `resource/mod.rs` 模块头），
///   而 `DriverCleanVerdict` 的同款处理已经确立了这条惯例：**结论进得来，判定依据进不来**。
/// * 宿主在此**不重算**驱动的判决，只做三态分派 —— 所以本类型只需要两个取值，
///   不需要把驱动侧的 `reason` 一并搬进来（`Error` 侧同样只带走一个稳定字面码）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetDisposition {
    /// 资源已回到初始化基线，可以再次签发。
    Clean,
    /// **无法证明**已回到基线 ⇒ 不得再次签发。
    ///
    /// **这是正常的、预期内的结果，不是异常。** 首版全部 path 驱动都走这一格：
    /// 15/15 声明 `ResetForReuse::Unsupported`，postgres / mysql / sqlite 的
    /// `reset_resource` 都返回 `Discard`，理由写在各自代码里 ——「no *proven*
    /// initialization protocol」。把常态记成 warn 级别的故障，会让真正的告警淹没在
    /// 每一条正常归还里。
    Discard,
}

impl ResetDisposition {
    /// 只有 `Clean` 允许这条资源被再次签发。
    pub const fn admits_reuse(self) -> bool {
        matches!(self, Self::Clean)
    }

    /// 判决名，用于日志与指标（不随文案改写而漂移）。
    pub const fn disposition_code(self) -> &'static str {
        match self {
            Self::Clean => "clean",
            Self::Discard => "discard",
        }
    }
}

/// 宿主注入的**物理端口**：本模块唯一的驱动侧出入口。
///
/// 为什么自己定义而不是复用 `connection/port.rs`：`connection/port.rs` 冻结的是 DTO
/// 与 `BudgetPort`，**并没有**资源操作端口；而本模块不得反向依赖尚未存在的
/// `registry/**`（尚未落地的会话层接缝，冻结）。所有方法只收发**账目标识**，不收发句柄。
pub trait PhysicalTransport: Send + Sync + 'static {
    /// 建立一条新的物理连接，返回**账目标识**。
    fn open(&self, request: &LeaseRequest) -> Result<ResourceId, ResourceError>;
    /// 关闭物理连接。返回 `Err` 表示关闭握手没确认，调用方必须转隔离而不是放行。
    fn close(&self, resource: &ResourceId) -> Result<(), ResourceError>;
    /// 复位，返回**驱动判决**（不是「有没有报错」）。短租约归池前必跑。
    ///
    /// 三种结果互不重叠，调用方必须**分别**处理，不得把其中两种压成同一条路径：
    ///
    /// * `Ok(ResetDisposition::Clean)` —— 可复用，归池裁决继续；
    /// * `Ok(ResetDisposition::Discard)` —— 驱动**明确**回答「无法证明已复位」。
    ///   这是正常返回值（见 [`ResetDisposition::Discard`]），调用方必须**关闭**，
    ///   不得再签发；§9.4「reset 不支持、失败或超时直接关闭」、§7.3 第 6 步
    ///   「只有 Clean 才归池，其余关闭」。
    /// * `Err(..)` —— 握手从未确认，结果**未知**。与 `Discard` 的区别是「不知道」
    ///   与「知道不能复用」，两者不许互推。
    ///
    /// 返回 `Ok(())` 式的二值签名是**错的**：它把上面头两种压成同一个值，于是驱动
    /// 诚实地说「我复位不了」时，宿主看到的是成功。这正是本签名携带判决的原因。
    fn reset(&self, resource: &ResourceId) -> Result<ResetDisposition, ResourceError>;
    /// 请求精确取消一次运行中的执行，返回**真实处置**。
    fn cancel(
        &self,
        resource: &ResourceId,
        execution: &ExecutionId,
    ) -> Result<CancelDisposition, ResourceError>;
}

/// 便于把 [`PhysicalTransport`] 塞进 `ResourceManager`。
pub type SharedTransport = Arc<dyn PhysicalTransport>;
