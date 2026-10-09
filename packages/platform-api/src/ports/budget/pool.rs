//! driver pool 的**物理**预算：PoolKey、租约、归池与释放。
//!
//! 对应 `connection-management.md` §9.4（归池前检查）、§9.6（PoolKey）、§9.3（占用与释放），
//! 以及 `shared-boundaries-and-ports.md:212` 的 `resource` 端口行：
//! acquire/cleanup/quarantine、隧道引用、归池前宿主检查；**不导出**驱动 `Clean` 判定，
//! **不导出**物理连接句柄。
//!
//! 契约层只给形状与规则判定，**不给记账实现**——P3（`platform-development-plan.md:111`）
//! 才写 actor 与池。`physical: ResourceId` 只是账目，句柄留在 adapter 侧（见上面的「不导出」）。

use async_trait::async_trait;

use crate::context::OwnerRef;
use crate::error::PortError;
use crate::id::{
    ConfigRevision, ConnectionId, Counter, CredentialRevision, DriverResourceKey,
    ExecutionIdentityKey, LeaseId, NetworkRouteRevision, OrganizationId, PolicyIsolationKey,
    ResourceId, Timestamp,
};
use crate::ports::budget::coordinator::{DrainScope, DrainStatus};
use crate::ports::budget::dimension::{
    BudgetDimension, BudgetUsage, PhysicalOccupancy, QuotaKey, QuotaVerdict, ResourceClass,
};

/// 池键（§9.6）。
///
/// 八个分量与 §9.6 的表格一一对应，**一个不多一个不少**：
///
/// | 分量 | 生产方 |
/// | --- | --- |
/// | `organization_id` | [`RequestContext`](crate::context::RequestContext) |
/// | `connection_id` | 同上（配置身份） |
/// | `config_revision` | [`ProfileRepository`](crate::ports::ProfileRepository) CAS |
/// | `credential_revision` | [`SecretProvider`](crate::ports::SecretProvider) 的不透明版本 |
/// | `execution_identity_key` | [`IdentityResolver`](crate::ports::IdentityResolver) |
/// | `policy_isolation_key` | [`PolicyService`](crate::ports::PolicyService) |
/// | `driver_resource_key` | 驱动 `describeResource` 对 CanonicalTarget / 初始化基线的规范化结果 |
/// | `network_route_revision` | [`NetworkProvider`](crate::ports::NetworkProvider) |
///
/// 任何一个分量变化都产生**新 key**：旧 idle 停发并关闭，额度另行申请。
/// 这正是「不跨池复用」的实现方式——没有 key 就没有池，没有池就不会把旧 socket 递给别人。
///
/// 三个 `opaque_id` 分量的 `Debug` 是脱敏的，所以派生的 [`PoolKey`] 的 `Debug` 也自动脱敏：
/// `{:?}` 进日志不会带出身份键、策略键或资源键。
///
/// **禁止**用连接串、密码散列或随手拼的字符串代替这个结构（§9.6 原文）。它必须可等值比较，
/// 因此每个分量都是独立 newtype，且不提供跨类型的 `From`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolKey {
    pub organization_id: OrganizationId,
    pub connection_id: ConnectionId,
    pub config_revision: ConfigRevision,
    pub credential_revision: CredentialRevision,
    pub execution_identity_key: ExecutionIdentityKey,
    pub policy_isolation_key: PolicyIsolationKey,
    /// 数据库绑定资源按 database 分 key（§9.6），因此同一连接的不同 database 天然落到不同池。
    pub driver_resource_key: DriverResourceKey,
    pub network_route_revision: NetworkRouteRevision,
}

/// 一次物理连接申请。
///
/// `class` 与 `dimension` 由**用例层**从 `RequestContext` / `OwnerRef` 推导，
/// 不是 IPC 参数（§9.5：分类由服务端用例决定，客户端不能自报优先级）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolAcquireRequest {
    pub pool_key: PoolKey,
    /// 这一批连接属于哪一类。
    pub class: ResourceClass,
    /// 主体维度（主体 / 数据库服务 / worker / job）。组织层由
    /// [`BudgetDimension::capped_by`] 自动补上。
    pub dimension: BudgetDimension,
    /// 归属。`OwnerRef::Job` 带 `stage_id`，与 §10.1「Job 资源 owner 是 jobId/stageId」一致。
    pub owner: OwnerRef,
    /// 申请的**物理连接条数**。
    ///
    /// §9.5：「Job 的多端申请必须一次预留全部许可或失败释放，不持有半边无限等待」——
    /// 所以它是一次性的批量数量，没有「先给 1 条剩下的排队」这种中间态。
    pub requested_physical: u32,
    /// 等待上限（毫秒）。`None` 走 §9.2 的 10 秒默认；期限到了就是
    /// [`QuotaDenial::WaitTimeout`](crate::ports::budget::QuotaDenial::WaitTimeout)。
    pub acquire_timeout_ms: Option<u64>,
}

/// 一次申请的判定：拿到租约，或拿到一条可读的拒绝原因。
pub type PoolAcquisition = QuotaVerdict<PoolLease>;

/// 一次**物理**连接的租约。
///
/// 刻意**没有** `db_session_id` 字段：租的是 socket，会话是逻辑视图，
/// 二者的绑定属于 P3 的 SessionRegistry（`platform-development-plan.md:109`）。
/// 把 `DbSessionId` 塞进来会让一个租约看起来能同时代表多条会话，
/// 而 §9.5 恰恰要求「活动事务、游标不抢占」——那是会话状态，不是租约字段能表达的。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolLease {
    pub lease_id: LeaseId,
    pub pool_key: PoolKey,
    /// 这条连接当初按哪一类申请的。归池后**保留原类别**（§9.5），所以它不会被改写。
    pub class: ResourceClass,
    pub dimension: BudgetDimension,
    /// 物理连接的账目标识。**不是**句柄，句柄不导出。
    pub physical: ResourceId,
    /// 本租约一次占用的物理条数（§9.5 的批量预留）。
    pub physical_count: u32,
    /// 当前占用状态。只会是 `Opening / InUse / Cleaning / IdlePooled / Quarantined`；
    /// `ControlSocket` 只出现在 [`ControlResourceLease`](crate::ports::budget::ControlResourceLease)。
    pub occupancy: PhysicalOccupancy,
    pub acquired_at: Timestamp,
}

impl PoolLease {
    /// 本租约要扣账的**全部**额度格，由外到内（组织 → 维度 → 类别）。
    ///
    /// 一次申请在每一层同时扣账，漏掉任一层都会让最外层封顶失效。
    pub fn quota_keys(&self) -> Vec<QuotaKey> {
        self.dimension
            .capped_by()
            .iter()
            .map(|dimension| QuotaKey::new(dimension.clone(), self.class))
            .collect()
    }
}

/// 不可复用的原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuarantineReason {
    /// 宿主侧检查未通过：仍有活动事务、活动游标或订阅者。
    HostCheckFailed,
    /// 驱动侧判定不干净（判定本身由 adapter 持有，本端口不导出驱动 `Clean` 谓词）。
    DriverUnclean,
    /// 隧道断了，socket 还在但没有意义。
    TunnelLost,
    /// 身份键轮换，池的隔离键已失效。
    IdentityRotated,
    /// 策略隔离键变化。
    PolicyChanged,
    /// 健康检查失败。
    HealthCheckFailed,
}

/// 归池前的**双重检查**结果（§9.4 / CM-06）。
///
/// 两个事实分别由宿主侧与驱动侧得出，作为**结果**传进来。端口不负责也不应该知道
/// 驱动如何判定 `Clean`（`shared-boundaries-and-ports.md:212`），所以这里没有驱动句柄、
/// 没有判定方法，只有一个布尔。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoolReturnCheck {
    /// 宿主侧：没有活动事务 / 游标 / 订阅者。
    pub host_clean: bool,
    /// 驱动侧：驱动自己认为连接可复用。
    pub driver_clean: bool,
}

impl PoolReturnCheck {
    pub const fn new(host_clean: bool, driver_clean: bool) -> Self {
        Self {
            host_clean,
            driver_clean,
        }
    }
}

/// 归池判定。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoolReturnVerdict {
    /// 可以进 idle pool。**仍然占用物理额度**（§9.3），等 TTL / LRU 真正 close 才释放。
    AdmittedToIdlePool,
    /// 必须清理关闭，因此会释放物理额度。
    CleanupRequired { reason: QuarantineReason },
}

impl PoolReturnVerdict {
    /// 双重检查取**与**：任一不干净就清理。
    ///
    /// §9.5：「不能先记账转移仍存活的 socket」——归池是记账转移，
    /// 只要宿主或驱动任何一方认为连接还带着状态，转移就是错的。
    pub const fn decide(check: PoolReturnCheck) -> Self {
        if check.driver_clean {
            if check.host_clean {
                return Self::AdmittedToIdlePool;
            }
            return Self::CleanupRequired {
                reason: QuarantineReason::HostCheckFailed,
            };
        }
        Self::CleanupRequired {
            reason: QuarantineReason::DriverUnclean,
        }
    }

    /// 是否准许进 idle pool。准许时**不**释放物理额度。
    pub const fn admits_to_idle_pool(&self) -> bool {
        matches!(self, Self::AdmittedToIdlePool)
    }
}

/// 物理连接为什么消失。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseSource {
    /// 持有者关闭。
    SessionClosed,
    /// idle TTL / LRU 淘汰。
    IdleEvicted,
    /// 池键分量变化，旧 idle 停发并关闭（§9.6）。
    PoolKeyRotated,
    /// 管理员 drain。
    AdminDrain,
    /// 归池前检查未通过后的清理。
    CleanupAfterReturn,
    /// 进程退出。
    Shutdown,
}

/// 物理额度被释放的**方式**。只有两种，没有第三种。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseDisposition {
    /// 真的 close 了。§9.3：「只有实际 close 才释放」。
    Closed,
    /// 只是回到 idle pool，socket 还活着。**不释放**。
    ReturnedToPool,
}

impl ReleaseDisposition {
    /// 是否真的释放了物理额度。
    ///
    /// §9.5：「idle pool 保留原类别与实际预算，可按 LRU 关闭**并在确认后**释放额度，
    /// 再向其他类别发放」——「关闭并确认后」是唯一的释放时刻，
    /// 所以「归池」在任何实现里都不能被当成释放。
    pub const fn releases_physical_budget(&self) -> bool {
        matches!(self, Self::Closed)
    }
}

/// 释放回执。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseReceipt {
    pub disposition: ReleaseDisposition,
    pub source: ReleaseSource,
    /// 本次实际释放的物理额度条数。归池时**必须**为 0。
    pub released_physical: Counter,
    /// 释放后该池内仍处于 idle 的连接数。
    pub remaining_idle: Counter,
    pub at: Timestamp,
}

impl ReleaseReceipt {
    /// 由「方式 + 条数」推出回执。释放条数由规则决定，不由调用方填写——
    /// 这样「归池却释放了 1 条」在类型层就构造不出来。
    pub fn for_disposition(
        disposition: ReleaseDisposition,
        source: ReleaseSource,
        held: u32,
        remaining_idle: Counter,
        at: Timestamp,
    ) -> Self {
        let released_physical = if disposition.releases_physical_budget() {
            Counter::new(u64::from(held))
        } else {
            Counter::ZERO
        };
        Self {
            disposition,
            source,
            released_physical,
            remaining_idle,
            at,
        }
    }
}

/// 一个池的账面快照。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolStats {
    pub pool_key: PoolKey,
    /// 池的类别。归池不改变它（§9.5）。
    pub class: ResourceClass,
    /// 池内物理连接数（含 idle）。
    pub physical: Counter,
    /// 其中 idle 的条数。
    pub idle: Counter,
    /// 在该额度格上的物理 / 逻辑用量。
    pub usage: BudgetUsage,
    /// idle TTL（毫秒）。§9.6 默认 60 秒。
    pub idle_ttl_ms: u64,
    /// 空 pool 元数据条数上限。§9.6 默认 32。
    pub max_empty_entries: u32,
}

/// driver pool 的物理预算端口。
///
/// 这是 §9.3「占用额度」的记账入口：**每一次 `driver.connect` 成功之前都必须先拿到
/// [`PoolAcquisition::Granted`]**，每一次真的 `disconnect` 之后都必须
/// [`release_physical`](Self::release_physical)。漏掉任一侧，物理额度就会漂。
///
/// 业务拒绝（额度不足 / 队列满 / 等待超时）以 [`QuotaVerdict::Denied`] **返回**，
/// 不抛 [`PortError`]（§4.1）。`Err` 只留给「预算后端自己都说不清」的情况，
/// 那时用 [`PortError::QuotaExceeded`]。
#[async_trait]
pub trait DriverPoolBudgetPort: Send + Sync + 'static {
    /// 申请物理连接。成功时租约已记账，失败时返回具体的拒绝原因。
    async fn acquire_physical(
        &self,
        request: &PoolAcquireRequest,
    ) -> Result<PoolAcquisition, PortError>;

    /// 连接真的 close 之后释放额度。归池请走 [`return_to_pool`](Self::return_to_pool)。
    async fn release_physical(
        &self,
        lease: &PoolLease,
        source: ReleaseSource,
    ) -> Result<ReleaseReceipt, PortError>;

    /// 归池。端口按 `check` 做双重检查并记录结果：准许则转 `IdlePooled`（不释放额度），
    /// 不准许则转 `Cleaning` 并要求调用方随后 close + [`release_physical`](Self::release_physical)。
    async fn return_to_pool(
        &self,
        lease: &PoolLease,
        check: PoolReturnCheck,
    ) -> Result<PoolReturnVerdict, PortError>;

    /// 标记不可复用。**不释放额度**（`Quarantined` 仍在 §9.3 的占用集合里），
    /// close 之后才由 [`release_physical`](Self::release_physical) 释放。
    async fn quarantine(
        &self,
        lease: &PoolLease,
        reason: QuarantineReason,
    ) -> Result<PoolLease, PortError>;

    /// 查一个额度格上的用量。
    async fn usage(&self, key: &QuotaKey) -> Result<BudgetUsage, PortError>;

    /// 查某个 key 的池快照。key 不存在时返回 `Err(PortError::NotFound)`。
    async fn pool_stats(&self, pool_key: &PoolKey) -> Result<PoolStats, PortError>;

    /// 对一个池执行 drain。词汇与 [`BudgetCoordinator`](crate::ports::BudgetCoordinator) 共用。
    async fn drain_pool(
        &self,
        pool_key: &PoolKey,
        scope: DrainScope,
    ) -> Result<DrainStatus, PortError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::{BlockId, ClientInstanceId, EditorSessionId, JobId, PrincipalId, StageId};

    fn key() -> PoolKey {
        PoolKey {
            organization_id: OrganizationId::new("org-1"),
            connection_id: ConnectionId::new("conn-1"),
            config_revision: ConfigRevision::new(3),
            credential_revision: CredentialRevision::new(2),
            execution_identity_key: ExecutionIdentityKey::new("exec-identity-raw"),
            policy_isolation_key: PolicyIsolationKey::new("policy-raw"),
            driver_resource_key: DriverResourceKey::new("postgres/appdb/canonical"),
            network_route_revision: NetworkRouteRevision::new(7),
        }
    }

    fn lease() -> PoolLease {
        PoolLease {
            lease_id: LeaseId::new("lease-1"),
            pool_key: key(),
            class: ResourceClass::Interactive,
            dimension: BudgetDimension::DatabaseService {
                organization_id: OrganizationId::new("org-1"),
                connection_id: ConnectionId::new("conn-1"),
            },
            physical: ResourceId::new("res-1"),
            physical_count: 1,
            occupancy: PhysicalOccupancy::InUse,
            acquired_at: Timestamp::new("2026-01-01T00:00:00Z"),
        }
    }

    #[test]
    fn pool_key_equality_covers_all_eight_components() {
        // §9.6 的表格有八行。少一行 = 两个不同资源共用一个池；多一行 = 同资源分裂成两个池。
        // 逐个分量改动都必须让 key 变得不相等。
        let base = key();
        let mutations: Vec<(&str, PoolKey)> = vec![
            (
                "organization_id",
                PoolKey {
                    organization_id: OrganizationId::new("org-2"),
                    ..base.clone()
                },
            ),
            (
                "connection_id",
                PoolKey {
                    connection_id: ConnectionId::new("conn-2"),
                    ..base.clone()
                },
            ),
            (
                "config_revision",
                PoolKey {
                    config_revision: ConfigRevision::new(4),
                    ..base.clone()
                },
            ),
            (
                "credential_revision",
                PoolKey {
                    credential_revision: CredentialRevision::new(3),
                    ..base.clone()
                },
            ),
            (
                "execution_identity_key",
                PoolKey {
                    execution_identity_key: ExecutionIdentityKey::new("other"),
                    ..base.clone()
                },
            ),
            (
                "policy_isolation_key",
                PoolKey {
                    policy_isolation_key: PolicyIsolationKey::new("other"),
                    ..base.clone()
                },
            ),
            (
                "driver_resource_key",
                PoolKey {
                    driver_resource_key: DriverResourceKey::new("postgres/otherdb/canonical"),
                    ..base.clone()
                },
            ),
            (
                "network_route_revision",
                PoolKey {
                    network_route_revision: NetworkRouteRevision::new(8),
                    ..base.clone()
                },
            ),
        ];
        assert_eq!(mutations.len(), 8, "§9.6 的池键必须正好八个分量");

        for (component, mutated) in mutations {
            assert_ne!(
                base, mutated,
                "改动 {component} 必须产生新池键，否则旧 socket 会被跨资源复用"
            );
        }
        assert_eq!(base, key(), "同样八个分量必然得到相等的键");
    }

    #[test]
    fn pool_key_debug_redacts_every_opaque_component() {
        // 身份键 / 策略键 / 资源键的内容是密钥材料，`{:?}` 进日志时必须全部脱敏。
        let rendered = format!("{:?}", key());
        for expected in [
            "ExecutionIdentityKey(<redacted>)",
            "PolicyIsolationKey(<redacted>)",
            "DriverResourceKey(<redacted>)",
        ] {
            assert!(
                rendered.contains(expected),
                "PoolKey 的 Debug 必须含 {expected}，实际为 {rendered}"
            );
        }
        for leaked in [
            "exec-identity-raw",
            "policy-raw",
            "postgres/appdb/canonical",
        ] {
            assert!(
                !rendered.contains(leaked),
                "PoolKey 的 Debug 泄漏了原始值 {leaked}"
            );
        }
    }

    #[test]
    fn pool_key_has_no_credential_or_connection_string_field() {
        // 池键按定义就要能进 map key、进缓存、进日志，所以它绝不能带任何凭据字段。
        // 这里按结构断言而不是按行为：多一个 `password: String` 字段，光靠单测跑不出来。
        let source = include_str!("pool.rs");
        let struct_block = source
            .split("pub struct PoolKey {")
            .nth(1)
            .and_then(|rest| rest.split("}").next())
            .unwrap_or_default();
        assert!(
            !struct_block.is_empty(),
            "必须能定位到 PoolKey 的字段块，结构断言否则是空跑"
        );
        for forbidden in [
            "password",
            "passwd",
            "secret",
            "connection_string",
            "dsn",
            "url",
        ] {
            assert!(
                !struct_block.to_lowercase().contains(forbidden),
                "PoolKey 不得含凭据字段 {forbidden}"
            );
        }
    }

    #[test]
    fn a_lease_charges_every_capped_layer() {
        // 数据库服务维度的租约必须同时扣组织额度与数据库服务额度，漏一层最外层封顶就失效。
        let keys = lease().quota_keys();
        assert_eq!(
            keys,
            vec![
                QuotaKey::new(
                    BudgetDimension::Organization {
                        organization_id: OrganizationId::new("org-1")
                    },
                    ResourceClass::Interactive,
                ),
                QuotaKey::new(
                    BudgetDimension::DatabaseService {
                        organization_id: OrganizationId::new("org-1"),
                        connection_id: ConnectionId::new("conn-1"),
                    },
                    ResourceClass::Interactive,
                ),
            ],
            "扣账链必须由外到内且包含自身"
        );
    }

    #[test]
    fn a_job_lease_is_charged_to_the_job_dimension_too() {
        // §9.5：「Job 另受组织最大并发阶段数（默认 4）及全局连接额度约束」——
        // 任务这一层是独立的封顶，不能只算组织。
        let mut job_lease = lease();
        job_lease.class = ResourceClass::Job;
        job_lease.dimension = BudgetDimension::Job {
            organization_id: OrganizationId::new("org-1"),
            job_id: JobId::new("job-1"),
        };
        assert_eq!(
            job_lease.quota_keys().len(),
            2,
            "任务租约应同时扣组织额度与任务额度"
        );
        assert_eq!(job_lease.quota_keys()[1].class, ResourceClass::Job);
    }

    #[test]
    fn pool_lease_does_not_carry_a_db_session_id() {
        // 租约租的是 socket，会话是逻辑视图。两者混在一个类型里，
        // 就会诱导实现方把「一条租约」当成「一个会话」，进而让 §9.5 的
        // 「活动事务、游标不抢占」变得无处表达。
        let source = include_str!("pool.rs");
        let struct_block = source
            .split("pub struct PoolLease {")
            .nth(1)
            .and_then(|rest| rest.split("}").next())
            .unwrap_or_default();
        assert!(!struct_block.is_empty(), "必须能定位到 PoolLease 的字段块");
        assert!(
            !struct_block.contains("DbSessionId"),
            "PoolLease 不得持有 DbSessionId，会话绑定属于 P3 的 SessionRegistry"
        );
    }

    #[test]
    fn returning_to_the_pool_never_releases_the_physical_budget() {
        // §9.3/§9.5 的核心规则：归池不释放，只有真 close 才释放。
        assert!(
            !ReleaseDisposition::ReturnedToPool.releases_physical_budget(),
            "归池时 socket 还活着，释放额度等于把名额送给别人"
        );
        assert!(ReleaseDisposition::Closed.releases_physical_budget());

        let receipt = ReleaseReceipt::for_disposition(
            ReleaseDisposition::ReturnedToPool,
            ReleaseSource::SessionClosed,
            1,
            Counter::new(4),
            Timestamp::new("2026-01-01T00:00:00Z"),
        );
        assert_eq!(
            receipt.released_physical,
            Counter::ZERO,
            "归池回执必须释放 0 条，释放条数由规则算出而不是调用方填"
        );
        assert_eq!(receipt.remaining_idle, Counter::new(4));

        let closed = ReleaseReceipt::for_disposition(
            ReleaseDisposition::Closed,
            ReleaseSource::IdleEvicted,
            1,
            Counter::new(3),
            Timestamp::new("2026-01-01T00:00:05Z"),
        );
        assert_eq!(
            closed.released_physical,
            Counter::new(1),
            "真的 close 才释放 1 条"
        );
    }

    #[test]
    fn pool_key_rotation_closes_and_therefore_releases() {
        // §9.6：池键分量变化时「旧 idle 停发并关闭」。关闭是 close，所以要释放额度。
        let receipt = ReleaseReceipt::for_disposition(
            ReleaseDisposition::Closed,
            ReleaseSource::PoolKeyRotated,
            1,
            Counter::ZERO,
            Timestamp::new("2026-01-01T00:00:10Z"),
        );
        assert_eq!(receipt.source, ReleaseSource::PoolKeyRotated);
        assert_eq!(
            receipt.released_physical,
            Counter::new(1),
            "池键轮转导致旧 idle 被关闭，该额度必须回到池子里"
        );
    }

    #[test]
    fn return_to_pool_requires_both_checks_to_pass() {
        // §9.4 / CM-06 双重检查；§9.5：「不能先记账转移仍存活的 socket」。
        let admitted = [PoolReturnCheck::new(true, true)];
        for check in admitted {
            assert_eq!(
                PoolReturnVerdict::decide(check),
                PoolReturnVerdict::AdmittedToIdlePool,
                "宿主与驱动都干净才能进 idle pool"
            );
            assert!(PoolReturnVerdict::decide(check).admits_to_idle_pool());
        }

        // 宿主不干净：活动事务 / 游标 / 订阅者还在。
        assert_eq!(
            PoolReturnVerdict::decide(PoolReturnCheck::new(false, true)),
            PoolReturnVerdict::CleanupRequired {
                reason: QuarantineReason::HostCheckFailed
            },
            "宿主不干净时必须清理，理由指向宿主侧"
        );

        // 驱动不干净：优先报驱动侧，理由要能指明是哪一边拦下的。
        assert_eq!(
            PoolReturnVerdict::decide(PoolReturnCheck::new(true, false)),
            PoolReturnVerdict::CleanupRequired {
                reason: QuarantineReason::DriverUnclean
            }
        );

        // 两边都不干净：仍然是确定性的单一结论。
        assert_eq!(
            PoolReturnVerdict::decide(PoolReturnCheck::new(false, false)),
            PoolReturnVerdict::CleanupRequired {
                reason: QuarantineReason::DriverUnclean
            }
        );
    }

    #[test]
    fn a_lease_starts_in_use_and_never_becomes_a_control_socket() {
        // 普通池租约不走 control 状态；控制连接有独立的租约类型。
        let current = lease();
        assert_eq!(current.occupancy, PhysicalOccupancy::InUse);
        assert!(current.occupancy.counts_against_budget());
        assert!(
            !matches!(current.occupancy, PhysicalOccupancy::ControlSocket),
            "普通池租约不得占用 control 状态"
        );
    }

    #[test]
    fn acquire_requests_batch_their_physical_count() {
        // §9.5：多端申请必须一次预留全部许可，没有「先给 1 条」的中间态。
        let request = PoolAcquireRequest {
            pool_key: key(),
            class: ResourceClass::Job,
            dimension: BudgetDimension::Job {
                organization_id: OrganizationId::new("org-1"),
                job_id: JobId::new("job-1"),
            },
            owner: OwnerRef::Job {
                job_id: JobId::new("job-1"),
                stage_id: StageId::new("stage-2"),
            },
            requested_physical: 4,
            acquire_timeout_ms: None,
        };
        assert_eq!(request.requested_physical, 4);
        assert_eq!(
            request.acquire_timeout_ms, None,
            "None 表示走 10 秒默认期限"
        );
        assert_eq!(
            request.owner,
            OwnerRef::Job {
                job_id: JobId::new("job-1"),
                stage_id: StageId::new("stage-2"),
            },
            "阶段归属要原样带上来：§10.1 的 Job owner 是 jobId/stageId"
        );
    }

    #[test]
    fn the_driver_pool_port_is_object_safe() {
        // 能编译出这个自由函数，就说明 trait 是对象安全的（§4.2：注入方是 Arc<dyn X>）。
        #[allow(dead_code)]
        fn round_trip(
            port: std::sync::Arc<dyn DriverPoolBudgetPort>,
        ) -> std::sync::Arc<dyn DriverPoolBudgetPort> {
            port
        }
    }

    #[test]
    fn the_budget_contract_ships_no_implementation() {
        // 契约层零实现是 P3 的退出门槛（H 层 CM-02～07）。这三个端口里一旦出现实现块，
        // 就等于把 P3 的活做掉了一半。被检查的指针由 join 拼出，本注释也不写全它。
        let sources = [
            ("pool.rs", include_str!("pool.rs"), "DriverPoolBudgetPort"),
            (
                "control.rs",
                include_str!("control.rs"),
                "ControlResourcePort",
            ),
            ("node.rs", include_str!("node.rs"), "ClusterNodeBudgetPort"),
        ];
        for (file, source, port) in sources {
            let needle = ["impl", port, "for"].join(" ");
            assert!(
                !source.contains(&needle),
                "{file} 出现了 `{needle}`，契约层必须零实现"
            );
        }
    }

    #[test]
    fn an_acquire_request_carries_the_owner_verbatim() {
        // 归属由 `OwnerRef` 表达（CM §4：不能允许用户声称任意 job owner），
        // 所以申请必须原样带上它，四种归属都要能填。
        for owner in [
            OwnerRef::Editor {
                client_instance_id: ClientInstanceId::new("ci-1"),
                editor_session_id: EditorSessionId::new("es-1"),
            },
            OwnerRef::Job {
                job_id: JobId::new("job-1"),
                stage_id: StageId::new("stage-1"),
            },
            OwnerRef::WorkflowBlock {
                job_id: JobId::new("job-1"),
                block_id: BlockId::new("block-1"),
            },
            OwnerRef::ClientSession {
                client_instance_id: ClientInstanceId::new("ci-2"),
                purpose: "mcp".to_string(),
            },
        ] {
            let request = PoolAcquireRequest {
                pool_key: key(),
                class: ResourceClass::Interactive,
                dimension: BudgetDimension::Principal {
                    organization_id: OrganizationId::new("org-1"),
                    principal_id: PrincipalId::new("u-7"),
                },
                owner: owner.clone(),
                requested_physical: 1,
                acquire_timeout_ms: Some(10_000),
            };
            assert_eq!(request.owner, owner, "归属必须原样传递，不能被预算层改写");
        }
    }
}
