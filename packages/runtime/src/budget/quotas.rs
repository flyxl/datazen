//! 预算口径的**配置**。
//!
//! 这里的每个数字都是字段而不是散落在调度逻辑里的字面量：
//!
//! - **额度低于保留总和则拒绝配置；所有保留都在总额度内**——由 [`BudgetConfig::validate`]
//!   在**装配期**拒绝，不允许运行到一半才发现额度非法。
//! - acquire 等待上限必须是配置：端口测试
//!   `acquire_timeout_is_configuration_not_a_hardcoded_constant`
//!   （`packages/platform-api/src/ports/budget/coordinator.rs`）守住端口侧，本模块在实现侧
//!   用同一个字段名 [`BudgetConfig::acquire_timeout_ms`] 把它变成可注入的参数：
//!   调用方不改逻辑就能改期限，逻辑里也不需要出现「10 秒」这种魔法值。

use datazen_platform_api::ports::budget::{ResourceClass, ServiceQuota};

/// 可调默认值。集中成常量，让「配置」有唯一来源。
pub mod defaults {
    /// 团队版：每个数据库服务的总额度。
    pub const TEAM_SERVICE_TOTAL: u32 = 20;
    /// 团队版桌面端：每个数据库服务的总额度。
    pub const DESKTOP_SERVICE_TOTAL: u32 = 16;
    /// 团队版保留额，按 [`ResourceClass::index`] 定位：control 2 / interactive 2 / metadata 1。
    pub const TEAM_RESERVED: [u32; 4] = [2, 2, 1, 0];
    /// 桌面端保留额：control 1 / interactive 2 / metadata 1。
    pub const DESKTOP_RESERVED: [u32; 4] = [1, 2, 1, 0];

    /// 单个用户的逻辑 session 上限。`New` 状态也算——它没有 socket。
    pub const PER_USER_LOGICAL_SESSIONS: u32 = 100;
    /// 单个组织（租户）的逻辑 session 上限。
    pub const PER_ORG_LOGICAL_SESSIONS: u32 = 1000;
    /// 单个用户同时已连接的编辑器数。它**不替代**物理额度。
    pub const PER_USER_CONNECTED_EDITORS: u32 = 5;

    /// 每类每服务的等待队列上限（32）。满了就是 `QueueFull`，绝不静默阻塞。
    pub const QUEUE_CAP_PER_CLASS_PER_SERVICE: u32 = 32;
    /// 单个用户的等待队列上限（32）。
    pub const QUEUE_CAP_PER_USER: u32 = 32;

    /// acquire 等待上限的默认配置值（10s，可取消）。
    ///
    /// 这是**默认值**不是硬编码：调用方可以用 [`BudgetConfig::with_acquire_timeout_ms`] 覆盖，
    /// 也可以由 [`BudgetRequest::acquire_timeout_ms`](datazen_platform_api::ports::budget::BudgetRequest)
    /// 逐次覆盖。
    pub const ACQUIRE_TIMEOUT_MS: u64 = 10_000;

    /// 节点额度租约的默认租期（「续期失败即视为失联」，租期长度本身是可调项）。
    pub const NODE_LEASE_TTL_MS: u64 = 30_000;

    /// 单个 Job 的最大并发阶段数。
    pub const JOB_MAX_CONCURRENT_STAGES: u32 = 4;
}

/// 拒绝一份不合法的预算配置。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BudgetConfigError {
    /// 总额度低于保留之和：「额度低于保留总和则拒绝配置」。
    #[error("保留额度之和 {reserved_sum} 超过总额度 {total}（额度低于保留总和则拒绝配置）")]
    ReservedExceedsTotal { total: u32, reserved_sum: u32 },
    /// 某个上限被配成了 0：那不是「不限制」，而是「谁都不许用」。
    #[error("{field} 必须为正，收到 {value}")]
    MustBePositive { field: &'static str, value: u32 },
}

/// 一个 DB 服务的预算口径。复制到整个账本，账本内部不再读全局配置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BudgetConfig {
    /// 总额度 / 保留额 / 共享额。`reserved` 已由 `ServiceQuota::new` 校验过。
    pub service_quota: ServiceQuota,
    /// 该服务上同时存在的**物理**连接上限（occupancy 集合算它）。
    pub service_physical_connections: u32,
    /// 单用户的逻辑 session 上限。`New` 也计入。
    pub per_user_logical_sessions: u32,
    /// 单组织的逻辑 session 上限。
    pub per_org_logical_sessions: u32,
    /// 单用户同时已连接的编辑器数（不替代物理额度）。
    pub per_user_connected_editors: u32,
    /// 每类每服务的队列上限。
    pub queue_cap_per_class_per_service: u32,
    /// 单用户的队列上限。
    pub queue_cap_per_user: u32,
    /// acquire 等待上限（毫秒）。**配置项**。
    pub acquire_timeout_ms: u64,
    /// 节点租约租期（毫秒）。失联判定依赖它。
    pub node_lease_ttl_ms: u64,
    /// 单 Job 最大并发阶段数。
    pub job_max_concurrent_stages: u32,
}

impl BudgetConfig {
    /// 以给定服务额度构造，其余字段取 [`defaults`]。
    pub fn new(service_quota: ServiceQuota) -> Self {
        Self {
            service_quota,
            service_physical_connections: service_quota.total,
            per_user_logical_sessions: defaults::PER_USER_LOGICAL_SESSIONS,
            per_org_logical_sessions: defaults::PER_ORG_LOGICAL_SESSIONS,
            per_user_connected_editors: defaults::PER_USER_CONNECTED_EDITORS,
            queue_cap_per_class_per_service: defaults::QUEUE_CAP_PER_CLASS_PER_SERVICE,
            queue_cap_per_user: defaults::QUEUE_CAP_PER_USER,
            acquire_timeout_ms: defaults::ACQUIRE_TIMEOUT_MS,
            node_lease_ttl_ms: defaults::NODE_LEASE_TTL_MS,
            job_max_concurrent_stages: defaults::JOB_MAX_CONCURRENT_STAGES,
        }
    }

    /// 团队版口径：总额度 20，保留 control 2 / interactive 2 / metadata 1，共享 15。
    pub fn team_default() -> Self {
        Self::from_parts(defaults::TEAM_SERVICE_TOTAL, defaults::TEAM_RESERVED)
    }

    /// 桌面端口径：总额度 16，保留 control 1 / interactive 2 / metadata 1，共享 12。
    pub fn desktop_default() -> Self {
        Self::from_parts(defaults::DESKTOP_SERVICE_TOTAL, defaults::DESKTOP_RESERVED)
    }

    /// 用总额度与保留额直接构造。
    ///
    /// `total < reserved_sum` 时**不**在这里返回错误，而是把总额度抬到保留之和，
    /// 让 [`BudgetConfig::validate`] 报出原始的 [`BudgetConfigError::ReservedExceedsTotal`]，
    /// 而不是让调用方拿到一个已经被悄悄改写过的额度。
    pub fn from_parts(total: u32, reserved: [u32; 4]) -> Self {
        let reserved_sum = reserved
            .iter()
            .fold(0u32, |acc, value| acc.saturating_add(*value));
        let effective_total = if reserved_sum > total {
            reserved_sum
        } else {
            total
        };
        // `effective_total >= reserved_sum` 由上面的分支保证，构造必然成功。
        let quota = match ServiceQuota::new(effective_total, reserved) {
            Ok(quota) => quota,
            Err(_) => ServiceQuota {
                total: reserved_sum,
                reserved,
                shared: 0,
            },
        };
        Self::new(quota)
    }

    /// 覆盖物理连接上限。
    pub fn with_physical_connections(mut self, value: u32) -> Self {
        self.service_physical_connections = value;
        self
    }

    /// 覆盖单用户逻辑 session 上限。
    pub fn with_per_user_logical_sessions(mut self, value: u32) -> Self {
        self.per_user_logical_sessions = value;
        self
    }

    /// 覆盖单组织逻辑 session 上限。
    pub fn with_per_org_logical_sessions(mut self, value: u32) -> Self {
        self.per_org_logical_sessions = value;
        self
    }

    /// 覆盖单用户已连接编辑器数。
    pub fn with_per_user_connected_editors(mut self, value: u32) -> Self {
        self.per_user_connected_editors = value;
        self
    }

    /// 覆盖每类每服务的队列上限。
    pub fn with_queue_cap_per_class_per_service(mut self, value: u32) -> Self {
        self.queue_cap_per_class_per_service = value;
        self
    }

    /// 覆盖单用户队列上限。
    pub fn with_queue_cap_per_user(mut self, value: u32) -> Self {
        self.queue_cap_per_user = value;
        self
    }

    /// 覆盖 acquire 等待上限（毫秒）。
    pub fn with_acquire_timeout_ms(mut self, value: u64) -> Self {
        self.acquire_timeout_ms = value;
        self
    }

    /// 覆盖节点租约租期（毫秒）。
    pub fn with_node_lease_ttl_ms(mut self, value: u64) -> Self {
        self.node_lease_ttl_ms = value;
        self
    }

    /// 校验配置。装配期调用，失败即拒绝启动，不允许非法额度进入账本。
    pub fn validate(&self) -> Result<(), BudgetConfigError> {
        let reserved_sum = self
            .service_quota
            .reserved
            .iter()
            .fold(0u32, |acc, value| acc.saturating_add(*value));
        if reserved_sum > self.service_quota.total {
            return Err(BudgetConfigError::ReservedExceedsTotal {
                total: self.service_quota.total,
                reserved_sum,
            });
        }
        if self.service_quota.shared != self.service_quota.total - reserved_sum {
            return Err(BudgetConfigError::ReservedExceedsTotal {
                total: self.service_quota.total,
                reserved_sum: self.service_quota.total - self.service_quota.shared,
            });
        }
        for (field, value) in [
            (
                "service_physical_connections",
                self.service_physical_connections,
            ),
            ("per_user_logical_sessions", self.per_user_logical_sessions),
            ("per_org_logical_sessions", self.per_org_logical_sessions),
            (
                "per_user_connected_editors",
                self.per_user_connected_editors,
            ),
            (
                "queue_cap_per_class_per_service",
                self.queue_cap_per_class_per_service,
            ),
            ("queue_cap_per_user", self.queue_cap_per_user),
            ("job_max_concurrent_stages", self.job_max_concurrent_stages),
        ] {
            if value == 0 {
                return Err(BudgetConfigError::MustBePositive { field, value });
            }
        }
        if self.acquire_timeout_ms == 0 {
            return Err(BudgetConfigError::MustBePositive {
                field: "acquire_timeout_ms",
                value: 0,
            });
        }
        if self.node_lease_ttl_ms == 0 {
            return Err(BudgetConfigError::MustBePositive {
                field: "node_lease_ttl_ms",
                value: 0,
            });
        }
        Ok(())
    }

    /// 某类在**不借用他人保留**的前提下还能吃下的名额上界。
    ///
    /// 「首版保留不可被其他类借用」因此是这个值而不是 `total`：
    /// 共享部分按权重轮转，分到多少是调度结果，不是承诺。
    pub fn capacity_of(&self, class: ResourceClass) -> u32 {
        self.service_quota.capacity_of(class)
    }
}
