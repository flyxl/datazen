//! 预算端口迁移垫片（D3，2026-10-26）。
//!
//! 预算端口家族（`BudgetCoordinator` / `DriverPoolBudgetPort` /
//! `ControlResourcePort` / `ClusterNodeBudgetPort` 与配套词汇、进程内参考账本
//! `pool_ledger`）的唯一定义自 D3 起在
//! `datazen-modules::execution::application::budget`（§5.3：Job 预算协调经注入
//! 契约使用同一账本）。本模块只再导出，旧 import 路径不变；D8 退役本 crate
//! 时一并删除。
//!
//! 刻意用**命名**再导出而非 glob：`ports/inventory` 的自检测试从源码文本里
//! 抽取 `pub use datazen_modules::…` 的名字来核对 PORT_TRAITS（glob 在文本层
//! 无法枚举，会静默漏掉整个家族）；与 artifact.rs 等其余垫片同一口径。

pub use datazen_modules::execution::application::budget::{
    BudgetCoordinator, BudgetDimension, BudgetPermit, BudgetPermitSet, BudgetPurpose,
    BudgetRequest, BudgetSnapshot, BudgetUsage, ClusterNodeBudgetPort, ControlBudgetHeadroom,
    ControlResourceKind, ControlResourceLease, ControlResourcePort, ControlResourceRequest,
    DrainScope, DrainStatus, DriverPoolBudgetPort, InMemoryDriverPoolBudget, NodeBudgetMode,
    NodeBudgetState, NodeCharge, NodeLease, NodeQuota, PhysicalOccupancy, PoolAcquireRequest,
    PoolAcquisition, PoolBudgetConfig, PoolKey, PoolLease, PoolReturnCheck, PoolReturnVerdict,
    PoolStats, QuarantineReason, QuotaConfigError, QuotaDenial, QuotaKey, QuotaVerdict,
    ReleaseDisposition, ReleaseOutcome, ReleaseReceipt, ReleaseSource, ResourceClass, ServiceQuota,
};
