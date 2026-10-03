//! 宿主侧归还裁决：**双条件**判定 + 清理报告 + 注入式物理端口。
//!
//! ## 边界一在这里落地
//!
//! 驱动的 `Clean` 以 [`DriverCleanVerdict`] 进入本文件，它是一个**结论**（`bool`），
//! 刻意不携带任何判定依据：宿主看不到 `Clean` 是怎么算出来的，也没有在本模块重算它的入口。
//! 宿主只做**自己的**检查（[`HostConditionSnapshot`]）。两侧**都**通过才允许回池 ——
//! 形状对齐 `platform-api` 的 `PoolReturnVerdict::decide`（CM-69）。

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::connection::port::CancelDisposition;
use crate::connection::{ExecutionId, HandleId, HandleKind, LeaseId, ResourceId, SessionHandleRef};

use crate::resource::lease::{LeasePurpose, LeaseRecord, LeaseRequest, LeaseState};
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

/// 未完成的协议负债（§7.5 步骤 5 / §13 放弃消费策略）。
///
/// 「协议未排空」是 CM-69 的三个独立阻断项之一：驱动报 `Clean` 也**不代表**宿主的
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
/// 四个字段正是 CM-69 点名的三个独立场景加一个归属禁用：
/// (a) `active_executions` 非空；(b) `outstanding_protocol` 非空；
/// (c) `unreleased_handles` 非空；(d) `owner_disabled`。
/// **任意一项非空即拒绝复用**，三者互不替代 —— 驱动报 `Clean` 不会让其中任何一项消失。
///
/// 第五个字段 `fixed_session` 不是「故障」，而是**用途**声明：§7.5 步骤 5 规定用户自开的
/// 任意 SQL 会话关闭时直接关物理资源。这类会话即使四项检查全清也不进空闲池。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostConditionSnapshot {
    pub active_executions: Vec<ExecutionId>,
    pub outstanding_protocol: Vec<ProtocolDebt>,
    pub unreleased_handles: Vec<SessionHandleRef>,
    pub owner_disabled: Option<String>,
    /// 固定会话：不参与归还池（§7.5 步骤 5）。
    pub fixed_session: bool,
}

impl HostConditionSnapshot {
    /// CM-69 的四项宿主条件全满足（注意：这**不代表**可以回池，驱动那一侧还没看，
    /// 用途也可能根本不允许归还）。
    pub const fn is_clear(&self) -> bool {
        self.active_executions.is_empty()
            && self.outstanding_protocol.is_empty()
            && self.unreleased_handles.is_empty()
            && self.owner_disabled.is_none()
    }

    /// 宿主侧认为「这条租约有资格进空闲池」：四项检查全清，且用途允许归还。
    pub const fn admits_to_pool(&self) -> bool {
        self.is_clear() && !self.fixed_session
    }

    /// 按固定顺序取**第一个**阻断项，保证裁决结果稳定可复现：
    /// 活动执行 → 未完成协议 → 未释放句柄 → 归属被禁用。
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
                self.unreleased_handles.first().map(|handle| {
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
        for handle in &self.unreleased_handles {
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

    /// 标记为固定会话（§7.5 步骤 5：用户自开的会话关闭时直接关物理资源）。
    pub const fn as_fixed_session(mut self) -> Self {
        self.fixed_session = true;
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
    /// 固定会话不参与归还（§7.5 步骤 5）。这不是故障，是用途。
    FixedSessionNotPoolable,
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
        }
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
    /// 驱动过了才轮到宿主四项，逐项独立判定，任一不满足立刻拒绝。
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
    /// 只有真正关闭才释放物理预算（§9.2：回池不释放）。
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
        // 固定会话**结构上**就不归池（§7.5）：这条不变量来自租约自身的用途，
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
        // 还挂着未释放会话级句柄（事务/游标/服务端预备对象）**不在此列**：CM-73 的目标
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
    /// CM-73：回收时是否已在**原资源**上执行过复位（回滚事务并解除映射）。
    /// 为真 ⇒ `Close` 事件排在 `Reset` 之后，这是可审计的行为证据。
    pub session_reset_performed: bool,
    pub physical_budget_released: bool,
    pub decided_at_nanos: u64,
}

impl CleanupReport {
    pub fn from_plan(
        record: &LeaseRecord,
        host: &HostConditionSnapshot,
        plan: CleanupPlan,
        now_nanos: u64,
        session_reset_performed: bool,
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
                host.unreleased_handles
                    .iter()
                    .filter(|handle| !handle.closed)
                    .map(|handle| handle.handle_id.clone())
                    .collect()
            },
            session_reset_performed,
            physical_budget_released,
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

/// CM-39：禁用/删除时对**已经在跑**的执行，记录它的**真实**取消处置。
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

/// 宿主注入的**物理端口**：本模块唯一的驱动侧出入口。
///
/// 为什么自己定义而不是复用 `connection/port.rs`：`connection/port.rs` 冻结的是 DTO
/// 与 `BudgetPort`，**并没有**资源操作端口；而本模块不得反向依赖尚未存在的
/// `registry/**`（Wave 2 接缝，冻结）。所有方法只收发**账目标识**，不收发句柄。
pub trait PhysicalTransport: Send + Sync + 'static {
    /// 建立一条新的物理连接，返回**账目标识**。
    fn open(&self, request: &LeaseRequest) -> Result<ResourceId, ResourceError>;
    /// 关闭物理连接。返回 `Err` 表示关闭握手没确认，调用方必须转隔离而不是放行。
    fn close(&self, resource: &ResourceId) -> Result<(), ResourceError>;
    /// 复位（§9.4）。短租约归池前必跑；驱动判定 `Clean` 时已经包含它。
    fn reset(&self, resource: &ResourceId) -> Result<(), ResourceError>;
    /// 请求精确取消一次运行中的执行，返回**真实处置**。
    fn cancel(
        &self,
        resource: &ResourceId,
        execution: &ExecutionId,
    ) -> Result<CancelDisposition, ResourceError>;
}

/// 便于把 [`PhysicalTransport`] 塞进 `ResourceManager`。
pub type SharedTransport = Arc<dyn PhysicalTransport>;
