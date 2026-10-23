//! 进程内预算与调度。
//!
//! 本模块实现 [`datazen_platform_api::ports::budget::coordinator::BudgetCoordinator`]，
//! 落在 `packages/runtime` 内部：单进程共享一份账本，不引入任何新的 crate 依赖。
//!
//! ## 为什么模块内部分成这几个文件
//!
//! | 文件 | 职责 | 关键约束 |
//! |------|------|----------|
//! | [`quotas`] | 配额与全部默认值 | 所有数字来自配置，没有写死的上限 |
//! | [`classes`] | 四类资源的槽位账 | 保留额不可借用；控制类永不进共享池 |
//! | [`queues`] | 同类轮转 + 共享加权轮询 | 按 id 取消；空队列跳过但不饿死 |
//! | [`records`] | 账本的值类型与计数器 | 只描述形状，自身不改任何计数 |
//! | [`ledger`] | **同步核心**：准入 / 记账 / 核销 | 槽位精确归还；批量全有或全无 |
//! | [`dispatch`] | 轮转放行引擎 | 先保留额后共享池；不抢占 |
//! | [`sessions`] | 逻辑 session 与物理连接额度 | 逻辑/物理分离；回空闲池**不**退物理额度 |
//! | [`coordinator`] | 端口适配层 | 锁绝不跨 `await`；取消 = future 丢弃 |
//!
//! 账本（[`ledger::BudgetLedger`]/[`dispatch`]/[`sessions`]）是纯同步的：**它自己不含任何时间源**，
//! 每个入口都显式收 `now_ms`。时间只有一处注入——[`coordinator::BudgetClock`]。
//! 于是「配额是配置」和「时间可替换」两件事都成立，而不需要任何真实等待。
//!
//! ## 硬约束落在哪
//!
//! 1. **四类分类由服务端决定**，调用方不能自报优先级 → [`coordinator::classify`]，
//!    端口的 `BudgetPurpose` 只有两值，映射也只由这一层做。
//! 2. **保留额 v1 不可借用**，控制类只服务取消/健康恢复 → `ResourceClass::may_borrow_reserved`
//!    恒 `false`；[`classes::ClassBucket`] 分别记 `Reserved`/`Shared` 两本账。
//! 3. **共享加权轮询 4:2:1** → [`queues::SharedRoundRobin`] 的平滑加权轮转（放行在 [`dispatch`]），
//!    非空队列在 `ceil(7/4)` 轮内必被服务，所以不饿死。
//! 4. **同类内按主体 FIFO 轮转** → [`queues::ClassQueue`] 的「泳道」结构。
//! 5. **队列上限（每类每服务 32 / 每用户 32）任一满即 `QueueFull`** → [`ledger::BudgetLedger::enqueue`]，
//!    **不静默阻塞**。
//! 6. **等待可取消、10 秒上限且是配置** → [`coordinator::InProcessBudgetCoordinator::acquire_with`]
//!    + `WaiterGuard::Drop` + `BudgetConfig::acquire_timeout_ms`。
//! 7. **不抢占**（已建立连接 / 活跃事务 / 游标）→ `PermitRecord::pinned` +
//!    [`sessions`] 的 `drain` 只标记、不吊销。
//! 8. **逻辑与物理额度分离**，`New` 只计逻辑 → [`records::SessionRecord`] 的 `occupancy: None`。
//! 9. **Job 多端点全有或全无** → [`ledger::BudgetLedger::try_admit_many`] 的回滚。
//! 10. **`drain` 按 `DrainScope` 生效**（节点 / 组织 / 连接）→ [`sessions`] 的 `drain`。
//! 11. **重复核销不二次记账** → `PermitRecord::permit_id` + [`ledger::ReleaseResult::AlreadyReleased`]。

pub mod classes;
pub mod coordinator;
pub mod dispatch;
pub mod ledger;
pub mod queues;
pub mod quotas;
pub mod records;
pub mod sessions;

pub use classes::{BudgetClaim, ClassBucket, ServiceClasses, SlotKind};
pub use coordinator::{
    classify, BudgetClock, InProcessBudgetCoordinator, MonotonicBudgetClock, OrganizationPrincipal,
    PrincipalResolver,
};
pub use ledger::{
    AdmitOutcome, BudgetLedger, DenialReason, PermitRecord, PumpReport, ReleaseResult,
};
pub use quotas::{BudgetConfig, BudgetConfigError};

// 端口侧的取值一律**转出**而不是重定义（协调者裁定 #1：跨轨道共享类型只 re-export）。
pub use datazen_platform_api::ports::budget::coordinator::{
    BudgetPermit, BudgetPermitSet, BudgetRequest, BudgetSnapshot, DrainScope, DrainStatus,
    NodeLease, ReleaseOutcome,
};
pub use datazen_platform_api::ports::budget::{
    BudgetDimension, BudgetPurpose, BudgetUsage, PhysicalOccupancy, QuotaDenial, QuotaKey,
    ResourceClass, ServiceQuota,
};
