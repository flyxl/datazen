//! 预算端口：**逻辑额度**（谁可以用、用多久）与**物理额度**（真的建了几条连接）两套契约。
//!
//! 对应 `connection-management.md` §9「预算与连接上限」。P2 阶段交付的是
//! **类型与 trait 本身**（`platform-development-plan.md:94`
//! 「所有实际建连的预算 port，包括 driver pool、集群节点和控制资源」），
//! 实际的跨进程记账、共享额度轮转与排队调度由 P3（`:111`）实现。
//! 本模块内唯一的实现体是 [`pool_ledger`]：driver pool 端口的**进程内**参考账本，
//! 用来把 §9.3 的记账规则钉在可测的代码上；它不是调度器，也还不是任何调用方的依赖。
//!
//! ## 四个 trait 的分工
//!
//! | trait | 管什么 | 不管什么 |
//! | --- | --- | --- |
//! | [`BudgetCoordinator`] | 逻辑预算许可：组织/连接维度、意图、drain | 不数物理连接、不认 PoolKey |
//! | [`DriverPoolBudgetPort`] | **物理连接**计数、PoolKey、归池/隔离/释放 | 不发逻辑许可 |
//! | [`ControlResourcePort`] | 取消与健康恢复用的控制连接（§9.5 保留类） | 不服务普通 SQL |
//! | [`ClusterNodeBudgetPort`] | 集群节点的物理槽位与失联状态 | 不替代上面的许可 |
//!
//! 一条**真实**的物理连接必须同时持有逻辑许可与物理记账，两者是同一笔账的两个视角：
//! 逻辑视角回答「这个用例被允许建立连接吗」，物理视角回答
//! 「这台机器/这个节点上现在真的有几条 socket」。任何一侧缺失都会让 §9.3 的
//! 「归池不释放物理预算」变成一句无法兑现的口号。
//!
//! ## 为什么 `BudgetCoordinator` 不被这三个 trait 取代
//!
//! `BudgetCoordinator`（P1 §6.4 第 10 行）的粒度是 `organization_id × connection_id ×
//! purpose`，它不知道 PoolKey，也不区分 control/interactive/metadata/job 四类。把它扩成
//! 「什么都能干的大 trait」会让无关实现方被迫实现无关方法，违反 §4.2 不合并 trait。
//! 因此这里**新增**三个窄 trait，而不是改写它——两套词汇各自成文件、各自带私有词汇表（§2.4）。
//!
//! ## 三条不可绕过的记账规则（已由本模块的类型与测试固定）
//!
//! 1. **归池不释放预算**（§9.3）：物理连接从 `InUse` 变成 `IdlePooled` 时，额度**仍然**被占；
//!    只有真正 `close` 才释放。规则落在 [`PhysicalOccupancy`] 的
//!    `counts_against_budget()` 与 [`ReleaseDisposition::releases_physical_budget()`]。
//! 2. **控制类只服务取消与健康恢复**（§9.5）：控制连接永远记在
//!    [`ResourceClass::Control`] 的**保留**额度上，不能记到共享额度里；
//!    [`ControlResourceLease`] 不对外暴露 `class` 字段，构造即锁定。
//! 3. **首版保留不可被其他类借用**（§9.5）：[`ResourceClass::may_borrow_reserved`]
//!    对四类一律返回 `false`。
//!
//! ## 分类由服务端决定
//!
//! §9.5 要求「分类由服务端用例决定，客户端不能自报优先级」。因此
//! [`ResourceClass`] 是**用例层**的取值：它出现在请求类型里，但请求本身由 adapter 从
//! `RequestContext`/`OwnerRef` 推导，而不是从 IPC 参数直传。

pub mod control;
pub mod coordinator;
pub mod dimension;
pub mod node;
pub mod pool;
pub mod pool_ledger;

pub use control::{
    ControlBudgetHeadroom, ControlResourceKind, ControlResourceLease, ControlResourcePort,
    ControlResourceRequest,
};
pub use coordinator::{
    BudgetCoordinator, BudgetPermit, BudgetPermitSet, BudgetPurpose, BudgetRequest, BudgetSnapshot,
    DrainScope, DrainStatus, NodeLease, ReleaseOutcome,
};
pub use dimension::{
    BudgetDimension, BudgetUsage, PhysicalOccupancy, QueueAdmission, QueueKind, QueueLimits,
    QuotaConfigError, QuotaDenial, QuotaKey, QuotaVerdict, ResourceClass, ServiceQuota,
};
pub use node::{ClusterNodeBudgetPort, NodeBudgetMode, NodeBudgetState, NodeCharge, NodeQuota};
pub use pool::{
    DriverPoolBudgetPort, PoolAcquireRequest, PoolAcquisition, PoolKey, PoolLease, PoolReturnCheck,
    PoolReturnVerdict, PoolStats, QuarantineReason, ReleaseDisposition, ReleaseReceipt,
    ReleaseSource,
};
pub use pool_ledger::{InMemoryDriverPoolBudget, PoolBudgetConfig};
