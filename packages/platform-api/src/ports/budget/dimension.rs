//! 预算维度与额度：**谁**在占、按**哪一类**占、占**多少**。
//!
//! 依据 `connection-management.md` §9.3（预算会计）、§9.5（资源类别）。
//! 这里只有类型与纯函数，**没有任何记账实现**——记账与公平队列是 P3
//! （`platform-development-plan.md:111`）的退出门槛。
//!
//! ## 维度 × 类别 = 一个额度格
//!
//! §9.3 说预算覆盖「组织、用户、数据库服务、worker、Job」五个维度；§9.5 说每个数据库服务的
//! 预算再拆成 control/interactive/metadata/job 四个类别。两者正交，交叉点就是一个**额度格**：
//! [`QuotaKey`]。计数、排队、加权、公平全部以额度格为最小单位。
//!
//! ## 一条物理连接要在几层同时扣账
//!
//! [`BudgetDimension::capped_by`] 给出夹住关系：主体发起的连接同时占用它所在组织的额度。
//! 组织额度是**最外层封顶**，所以它在链首；只有它自己不封顶别人。

use crate::id::{ConnectionId, Counter, JobId, OrganizationId, PrincipalId, WorkerId};

/// 预算维度：额度算在谁头上（§9.3 的五个）。
///
/// 四个非组织维度都带 `organization_id`，因此 [`BudgetDimension::capped_by`] 能把它们
/// 一致地夹在组织额度之下。`DatabaseService` 用 `ConnectionId` 标识——桌面与服务端形态下，
/// 一个「数据库服务」就是一个已保存的连接配置（host + port + 类型），它同时是
/// §9.5 里「每个数据库服务的预算」的单位。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BudgetDimension {
    /// 组织。**最外层封顶**，其余维度都被它夹住。
    Organization { organization_id: OrganizationId },
    /// 自然人 / 服务主体。
    Principal {
        organization_id: OrganizationId,
        principal_id: PrincipalId,
    },
    /// 一个数据库服务（本形态下一个已保存连接配置）。
    DatabaseService {
        organization_id: OrganizationId,
        connection_id: ConnectionId,
    },
    /// 一个集群 worker 节点。§9.3：多 worker 走节点额度或全局许可，二选一。
    Worker {
        organization_id: OrganizationId,
        worker_id: WorkerId,
    },
    /// 一个任务。**不含阶段**：§10.1 的「Job 资源 owner 是 jobId/stageId」中 stage 属于归属
    /// （[`crate::context::OwnerRef::Job`]），而 §9.5 要求多端申请一次预留全部许可，
    /// 所以计数按 job 汇总，不按阶段切分。
    Job {
        organization_id: OrganizationId,
        job_id: JobId,
    },
}

impl BudgetDimension {
    /// 本维度所属的组织；组织维度返回自身。
    pub fn organization_id(&self) -> OrganizationId {
        match self {
            Self::Organization { organization_id }
            | Self::Principal {
                organization_id, ..
            }
            | Self::DatabaseService {
                organization_id, ..
            }
            | Self::Worker {
                organization_id, ..
            }
            | Self::Job {
                organization_id, ..
            } => organization_id.clone(),
        }
    }

    /// 夹住本维度的全部层级，**由外到内**，末位是本维度自身。
    ///
    /// 一条物理连接要在返回的每一层同时扣账。返回 `Vec` 而不是「一条链」是因为
    /// worker / job 在单机形态下不一定再被主体维度夹一层，形状是数据依赖的。
    pub fn capped_by(&self) -> Vec<BudgetDimension> {
        let mut chain = Vec::with_capacity(2);
        if !matches!(self, Self::Organization { .. }) {
            chain.push(Self::Organization {
                organization_id: self.organization_id(),
            });
        }
        chain.push(self.clone());
        chain
    }
}

/// 资源类别：这条物理连接被用来做什么（§9.5）。
///
/// 四个取值是**封闭集合**，没有 `Unknown` 也没有「其它」——§9.5 要求分类由服务端用例决定，
/// 客户端不能自报，因此开放集合会让自报成为可能。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ResourceClass {
    /// 取消与健康恢复。§9.5：**不用于普通 SQL**。
    Control,
    /// 固定编辑器与 TablePanel。
    Interactive,
    /// 元数据读取与补全。
    Metadata,
    /// 迁移 / 导出 / 同步 / 传输等阶段。
    Job,
}

impl ResourceClass {
    /// 全部类别，顺序即 [`ResourceClass::index`] 的顺序。
    pub const ALL: [ResourceClass; 4] =
        [Self::Control, Self::Interactive, Self::Metadata, Self::Job];

    /// 在定长数组中的下标。
    pub const fn index(self) -> usize {
        match self {
            Self::Control => 0,
            Self::Interactive => 1,
            Self::Metadata => 2,
            Self::Job => 3,
        }
    }

    /// 是否为控制类。
    pub const fn is_control(self) -> bool {
        matches!(self, Self::Control)
    }

    /// 是否允许动用共享额度。
    ///
    /// §9.5 的共享额度按 interactive:metadata:job = 4:2:1 加权轮转，**control 不在其中**——
    /// 控制连接只能吃自己那一份保留额度，因此满池时它不会去抢普通 SQL 的位置。
    pub const fn may_join_shared(self) -> bool {
        !matches!(self, Self::Control)
    }

    /// 共享额度里的相对权重；`None` = 该类不参与共享轮转。
    ///
    /// 权重是**轮转**的输入，不是静态切分：§9.5 讲的是加权轮转且空队列跳过，
    /// 所以把 15 静态切成 7/4/3 会把轮转语义做错。真正的轮转调度是 P3 的活，这里只固定权重值。
    pub const fn shared_weight(self) -> Option<u32> {
        match self {
            Self::Control => None,
            Self::Interactive => Some(4),
            Self::Metadata => Some(2),
            Self::Job => Some(1),
        }
    }

    /// 首版保留额度是否允许被其他类别借用。§9.5：「首版保留不可被其他类借用」。
    ///
    /// 恒为 `false`。写成方法而不是注释，是为了让「不可借用」成为**可执行的断言**：
    /// 单测遍历 [`ResourceClass::ALL`] 逐个钉死，任何一次放宽都会红。
    pub const fn may_borrow_reserved(self) -> bool {
        false
    }
}

/// 一个额度格：维度 × 类别。
///
/// 等值比较即额度比较。它是 `Hash + Ord`，所以实现方可以直接用它做 map key，
/// 而**不需要**拼字符串——§9.6 明令「PoolKey 使用类型化结构等值比较，不能用随意字符串拼接」。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct QuotaKey {
    pub dimension: BudgetDimension,
    pub class: ResourceClass,
}

impl QuotaKey {
    pub const fn new(dimension: BudgetDimension, class: ResourceClass) -> Self {
        Self { dimension, class }
    }
}

/// 预算配置被拒绝的原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuotaConfigError {
    /// 总额度低于各保留之和。§9.5：「额度低于保留总和则拒绝配置；所有保留都在总额度内」。
    ReservedExceedsTotal { total: u32, reserved_sum: u32 },
}

/// 一个数据库服务的预算配置（§9.5）。
///
/// §9.2 的默认值：团队单数据库服务额度 20（control 保留 2 / metadata 保留 1 /
/// interactive 保留 2，共享 15）；桌面总目标连接 16（保留 1/1/2，共享 12）。
/// 这些是**配置起点而不是契约常量**，因此这里只提供形状与校验，不硬编码默认值——
/// 默认值由实现方从配置读入，见 [`ServiceQuota::new`] 的单测。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServiceQuota {
    /// 总额度。
    pub total: u32,
    /// 按 [`ResourceClass::index`] 定位的保留额度。
    pub reserved: [u32; 4],
    /// 非保留部分，即共享额度。仅 interactive / metadata / job 可用。
    pub shared: u32,
}

impl ServiceQuota {
    /// 校验并构造。`total` 低于保留之和时拒绝配置（§9.5）。
    pub fn new(total: u32, reserved: [u32; 4]) -> Result<Self, QuotaConfigError> {
        let reserved_sum = reserved
            .iter()
            .fold(0u32, |acc, value| acc.saturating_add(*value));
        if reserved_sum > total {
            return Err(QuotaConfigError::ReservedExceedsTotal {
                total,
                reserved_sum,
            });
        }
        Ok(Self {
            total,
            reserved,
            shared: total - reserved_sum,
        })
    }

    /// 某类的保留额度。
    pub fn reserved_of(&self, class: ResourceClass) -> u32 {
        self.reserved[class.index()]
    }

    /// 某类的**上限**：保留额度，加上它被允许动用的共享额度。
    ///
    /// 这是封顶值，不是承诺分到的值——共享部分按权重轮转，实际分到多少由 P3 的调度决定。
    pub fn capacity_of(&self, class: ResourceClass) -> u32 {
        let shared = if class.may_join_shared() {
            self.shared
        } else {
            0
        };
        self.reserved_of(class).saturating_add(shared)
    }
}

/// 物理连接所处的占用状态（§9.3 的占用集合）。
///
/// 六个取值合起来就是「占用额度」的**全集**：`Opening`、`InUse`、`Cleaning`、`idle pool`、
/// `Quarantined`，以及取消/健康恢复用的 **control sockets**。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PhysicalOccupancy {
    /// 物理连接已建立，正在跑初始化基线。尚不可归池。
    Opening,
    /// 正被使用者持有。
    InUse,
    /// 归池前检查未通过，正在关闭。
    Cleaning,
    /// 已归池。§9.3：「归池不释放物理预算」。
    IdlePooled,
    /// 判定不可复用但尚未 close。
    Quarantined,
    /// 取消 / 健康恢复专用连接（§9.3 的 control sockets）。
    ControlSocket,
}

impl PhysicalOccupancy {
    pub const ALL: [PhysicalOccupancy; 6] = [
        Self::Opening,
        Self::InUse,
        Self::Cleaning,
        Self::IdlePooled,
        Self::Quarantined,
        Self::ControlSocket,
    ];

    /// 是否仍占用物理额度。**六个取值全部为 `true`**——这是 §9.3 的规则本身：
    /// 「归池不释放物理预算，只有实际 close 才释放」。
    ///
    /// 写成恒真的方法不是废话：它把「idle pool 也占着」这条规则变成一条可被单测遍历钉死的
    /// 断言，而不是散落在实现里的一句注释。额度释放的唯一出口是
    /// [`crate::ports::budget::ReleaseDisposition::releases_physical_budget`]。
    pub const fn counts_against_budget(self) -> bool {
        true
    }

    /// 是否仍可被新使用者直接复用（只有 `InUse` 之外都不算；实际归池还要过 §9.4 双重检查）。
    pub const fn is_live(self) -> bool {
        matches!(self, Self::InUse)
    }
}

/// 某个额度格或整条链上的用量。
///
/// `physical` 与 `logical` **分开计数**：§9.2 的「每用户/每组织逻辑 session 100/1000」与
/// 「桌面总目标连接 16」是两套额度，前者数会话归属，后者数 socket，合并成一个数字
/// 就再也查不出是谁在占。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BudgetUsage {
    /// 已占用的物理连接数。
    pub physical: Counter,
    /// 已占用的逻辑 session 数。
    pub logical: Counter,
}

impl BudgetUsage {
    pub const fn new(physical: Counter, logical: Counter) -> Self {
        Self { physical, logical }
    }

    /// 距离 `total` 还剩多少物理额度。已超发时返回 0，不回绕。
    pub fn remaining_physical(&self, total: u32) -> u32 {
        let used = u32::try_from(self.physical.get()).unwrap_or(u32::MAX);
        total.saturating_sub(used)
    }
}

/// 哪一个队列满了（§9.5）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueKind {
    /// 每类每服务一个队列。
    ClassPerService { class: ResourceClass },
    /// 每用户一个队列。
    Principal,
}

/// 排队准入结果。§9.5：「每类每服务排队上限 32、每用户 32；任一满返回 QueueFull」。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueAdmission {
    /// 准予排队，`position` 为 0 表示立即可发放。
    Admitted { position: u32 },
    /// 该队列已满。
    Full { queue: QueueKind },
}

/// §9.2/§9.5 的排队上限（默认值，实现方可配）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueueLimits {
    /// 每类每服务。默认 32。
    pub per_class_per_service: u32,
    /// 每用户。默认 32。
    pub per_principal: u32,
}

impl QueueLimits {
    pub const fn new(per_class_per_service: u32, per_principal: u32) -> Self {
        Self {
            per_class_per_service,
            per_principal,
        }
    }

    /// §9.2 的默认排队上限。
    pub const DEFAULTS: QueueLimits = QueueLimits::new(32, 32);

    /// 判定一次入队。**两类同时检查，任一满即拒绝**（§9.5）。
    pub const fn admit(
        &self,
        class: ResourceClass,
        class_waiting: u32,
        principal_waiting: u32,
    ) -> QueueAdmission {
        if class_waiting >= self.per_class_per_service {
            return QueueAdmission::Full {
                queue: QueueKind::ClassPerService { class },
            };
        }
        if principal_waiting >= self.per_principal {
            return QueueAdmission::Full {
                queue: QueueKind::Principal,
            };
        }
        QueueAdmission::Admitted {
            position: principal_waiting,
        }
    }
}

/// 额度**不足**这个事实。
///
/// 它是**返回值**而不是错误：§4.1 规定业务拒绝不在端口层抛出。用例层把每种原因翻译成不同的
/// `ApiError`（`QuotaExceeded` / `ResourceBusy`），端口层不能替它决定。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuotaDenial {
    /// 额度格已装不下。`requested` 与 `available` 都是物理连接条数。
    Insufficient {
        key: QuotaKey,
        requested: u32,
        available: u32,
    },
    /// 队列已满（§9.5 的 QueueFull）。
    QueueFull { admission: QueueAdmission },
    /// 等待超过申请方给的期限。§9.2 默认 10 秒，翻译为 `ResourceBusy`。
    WaitTimeout {
        key: QuotaKey,
        waited_ms: u64,
        limit_ms: u64,
    },
    /// 节点租约失联：停止发放新资源（§9.3）。
    NodeLost { worker_id: WorkerId },
}

impl QuotaDenial {
    /// 受影响的那个额度格；节点失联没有额度格，返回 `None`。
    pub fn key(&self) -> Option<&QuotaKey> {
        match self {
            Self::Insufficient { key, .. } | Self::WaitTimeout { key, .. } => Some(key),
            Self::QueueFull { .. } | Self::NodeLost { .. } => None,
        }
    }

    /// 是否可以稍后重试。额度不足与队列满可以，等超时与节点失联在本契约里不行——
    /// 前者是调用方给的期限到了，后者是协调权已经丢了（P3 另开一条恢复路径，不在本端口内重试）。
    pub const fn is_retryable(&self) -> bool {
        matches!(self, Self::Insufficient { .. } | Self::QueueFull { .. })
    }
}

/// 申请额度的判定：要么拿到东西，要么拿到一个**可读的拒绝原因**。
///
/// 三个端口（driver pool / 控制资源 / 集群节点）共用这一个判定泛型，因此
/// 「为什么没拿到连接」在三条路径上是同一套词汇，用例层只需要一处翻译。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuotaVerdict<T> {
    Granted(T),
    Denied(QuotaDenial),
}

impl<T> QuotaVerdict<T> {
    /// 判定为通过时取出结果值。**只读**：同一个判定可以先问有没有拿到、再问为什么没拿到
    /// （用例层要把拒绝原因翻译成 `ApiError`，需要读两次）。
    pub fn granted(&self) -> Option<&T> {
        match self {
            Self::Granted(value) => Some(value),
            Self::Denied(_) => None,
        }
    }

    /// 判定为拒绝时取出原因。`Denied` 侧的值不是 `Copy`（带 `QuotaKey`），
    /// 所以这里返回引用。
    pub fn denied(&self) -> Option<&QuotaDenial> {
        match self {
            Self::Granted(_) => None,
            Self::Denied(reason) => Some(reason),
        }
    }

    pub fn into_result(self) -> Result<T, QuotaDenial> {
        match self {
            Self::Granted(value) => Ok(value),
            Self::Denied(reason) => Err(reason),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn org() -> OrganizationId {
        OrganizationId::new("org-1")
    }

    fn principal_dim() -> BudgetDimension {
        BudgetDimension::Principal {
            organization_id: org(),
            principal_id: PrincipalId::new("u-7"),
        }
    }

    #[test]
    fn every_dimension_carries_its_organization() {
        // 夹住关系靠这个：非组织维度必须能报出组织，否则 capped_by 拼不出封顶层。
        for dimension in [
            BudgetDimension::Organization {
                organization_id: org(),
            },
            principal_dim(),
            BudgetDimension::DatabaseService {
                organization_id: org(),
                connection_id: ConnectionId::new("c-1"),
            },
            BudgetDimension::Worker {
                organization_id: org(),
                worker_id: WorkerId::new("w-1"),
            },
            BudgetDimension::Job {
                organization_id: org(),
                job_id: JobId::new("j-1"),
            },
        ] {
            assert_eq!(
                dimension.organization_id(),
                org(),
                "{dimension:?} 必须能报出组织，否则夹住链断在最外层"
            );
        }
    }

    #[test]
    fn non_organization_dimensions_are_capped_by_the_organization() {
        let chain = principal_dim().capped_by();
        assert_eq!(
            chain,
            vec![
                BudgetDimension::Organization {
                    organization_id: org()
                },
                principal_dim(),
            ],
            "主体发起的连接必须同时占组织额度，链首是封顶层，末位是自身"
        );

        // 组织维度不再被自己夹一层，否则会在封顶层上一次扣两次。
        let org_chain = BudgetDimension::Organization {
            organization_id: org(),
        }
        .capped_by();
        assert_eq!(org_chain.len(), 1, "组织维度只应有一条扣账链");
        assert_eq!(
            org_chain[0],
            BudgetDimension::Organization {
                organization_id: org()
            }
        );
    }

    #[test]
    fn control_never_joins_the_shared_pool() {
        // §9.5 的共享权重是 4:2:1，control 不在其中；它满池时不会去抢普通 SQL 的位置。
        assert!(
            !ResourceClass::Control.may_join_shared(),
            "控制类不能动用共享额度，否则满池时它会挤掉普通 SQL"
        );
        assert_eq!(ResourceClass::Control.shared_weight(), None);

        assert_eq!(ResourceClass::Interactive.shared_weight(), Some(4));
        assert_eq!(ResourceClass::Metadata.shared_weight(), Some(2));
        assert_eq!(ResourceClass::Job.shared_weight(), Some(1));
    }

    #[test]
    fn reserved_quota_is_not_borrowable_in_the_first_version() {
        // §9.5：「首版保留不可被其他类借用」。逐类钉死，任何一次放宽都会红。
        for class in ResourceClass::ALL {
            assert!(
                !class.may_borrow_reserved(),
                "{class:?} 在首版不允许借用其他类的保留额度"
            );
        }
    }

    #[test]
    fn every_occupancy_state_keeps_charging_the_physical_budget() {
        // §9.3：「归池不释放物理预算，只有实际 close 才释放」。
        // idle pool、正在清理、已隔离、控制连接全都还在占额度。
        for occupancy in PhysicalOccupancy::ALL {
            assert!(
                occupancy.counts_against_budget(),
                "{occupancy:?} 必须继续占用物理额度，否则归池等于放额度"
            );
        }
    }

    #[test]
    fn only_in_use_counts_as_live() {
        assert!(PhysicalOccupancy::InUse.is_live());
        for occupancy in [
            PhysicalOccupancy::Opening,
            PhysicalOccupancy::Cleaning,
            PhysicalOccupancy::IdlePooled,
            PhysicalOccupancy::Quarantined,
            PhysicalOccupancy::ControlSocket,
        ] {
            assert!(!occupancy.is_live(), "{occupancy:?} 不可直接复用");
        }
    }

    #[test]
    fn team_quota_shape_from_the_design_doc() {
        // §9.2/§9.5：团队单数据库服务额度 20，control 保留 2、metadata 保留 1、
        // interactive 保留 2，其余 15 为共享。
        let quota = ServiceQuota::new(
            20,
            [
                2, // control
                2, // interactive
                1, // metadata
                0, // job
            ],
        )
        .expect_ok();

        assert_eq!(quota.shared, 15, "20 - 5 应为共享 15");
        assert_eq!(quota.reserved_of(ResourceClass::Control), 2);
        assert_eq!(quota.reserved_of(ResourceClass::Interactive), 2);
        assert_eq!(quota.reserved_of(ResourceClass::Metadata), 1);
        // job 没有保留，封顶就是共享全部 15。
        assert_eq!(quota.capacity_of(ResourceClass::Job), 15);
        // control 的封顶只是它自己的保留 2，绝不含共享。
        assert_eq!(quota.capacity_of(ResourceClass::Control), 2);
    }

    #[test]
    fn desktop_quota_shape_from_the_design_doc() {
        // §9.2：桌面总目标连接 16，保留 1/1/2，共享 12。
        let quota = ServiceQuota::new(16, [1, 2, 1, 0]).expect_ok();
        assert_eq!(quota.shared, 12);
        assert_eq!(quota.reserved_of(ResourceClass::Interactive), 2);
        assert_eq!(quota.reserved_of(ResourceClass::Metadata), 1);
    }

    #[test]
    fn a_total_below_the_reserved_sum_is_rejected() {
        // §9.5：「额度低于保留总和则拒绝配置；所有保留都在总额度内」。
        let rejected = ServiceQuota::new(4, [2, 2, 1, 0]);
        assert_eq!(
            rejected,
            Err(QuotaConfigError::ReservedExceedsTotal {
                total: 4,
                reserved_sum: 5,
            }),
            "保留之和高于总额度时必须拒绝配置，不能悄悄截断"
        );

        // 相等是允许的：此时共享为 0，等于该服务只跑保留额度。
        let exact = ServiceQuota::new(5, [2, 2, 1, 0]).expect_ok();
        assert_eq!(exact.shared, 0, "保留之和等于总额度时共享为 0，而非被拒绝");
    }

    #[test]
    fn queue_admission_checks_both_queues() {
        let limits = QueueLimits::DEFAULTS;

        assert_eq!(
            limits.admit(ResourceClass::Interactive, 0, 0),
            QueueAdmission::Admitted { position: 0 },
            "两个队列都空时立即可发放"
        );

        assert_eq!(
            limits.admit(ResourceClass::Interactive, 32, 0),
            QueueAdmission::Full {
                queue: QueueKind::ClassPerService {
                    class: ResourceClass::Interactive
                }
            },
            "每类每服务队列满 32 即拒绝"
        );

        assert_eq!(
            limits.admit(ResourceClass::Job, 0, 32),
            QueueAdmission::Full {
                queue: QueueKind::Principal
            },
            "每用户队列满 32 即拒绝，与类别无关"
        );

        // 类别队列与用户队列互不影响：类别满时用户队列还空着，仍然按类别满拒绝。
        assert_eq!(
            limits.admit(ResourceClass::Metadata, 32, 31),
            QueueAdmission::Full {
                queue: QueueKind::ClassPerService {
                    class: ResourceClass::Metadata
                }
            }
        );
    }

    #[test]
    fn quota_key_compares_by_typed_structure() {
        let dimension = principal_dim();
        let key = QuotaKey::new(dimension.clone(), ResourceClass::Interactive);
        let same = QuotaKey::new(dimension.clone(), ResourceClass::Interactive);
        let other_class = QuotaKey::new(dimension.clone(), ResourceClass::Metadata);

        assert_eq!(key, same, "同维度同类必然相等");
        assert_ne!(key, other_class, "类别是额度格的一部分");
    }

    #[test]
    fn denials_separate_retryable_from_terminal() {
        let key = QuotaKey::new(principal_dim(), ResourceClass::Interactive);

        let insufficient = QuotaDenial::Insufficient {
            key: key.clone(),
            requested: 2,
            available: 1,
        };
        assert_eq!(insufficient.key(), Some(&key));
        assert!(insufficient.is_retryable(), "额度不足可以稍后重试");

        let queue_full = QuotaDenial::QueueFull {
            admission: QueueAdmission::Full {
                queue: QueueKind::Principal,
            },
        };
        assert_eq!(queue_full.key(), None, "队列满不针对某个具体额度格");
        assert!(queue_full.is_retryable());

        let timeout = QuotaDenial::WaitTimeout {
            key,
            waited_ms: 10_000,
            limit_ms: 10_000,
        };
        assert!(!timeout.is_retryable(), "期限已到，重试没有意义");

        let lost = QuotaDenial::NodeLost {
            worker_id: WorkerId::new("w-9"),
        };
        assert!(
            !lost.is_retryable(),
            "协调权已丢，重试只会继续发新连接，违反 §9.3"
        );
    }

    #[test]
    fn verdicts_split_granted_from_denied() {
        let granted: QuotaVerdict<u8> = QuotaVerdict::Granted(3);
        assert_eq!(granted.granted(), Some(&3));
        assert_eq!(granted.denied(), None);

        let denied: QuotaVerdict<u8> = QuotaVerdict::Denied(QuotaDenial::QueueFull {
            admission: QueueAdmission::Full {
                queue: QueueKind::Principal,
            },
        });
        assert_eq!(denied.granted(), None);
        assert!(denied.denied().is_some());
        assert!(denied.into_result().is_err());
    }

    #[test]
    fn physical_and_logical_usage_are_counted_separately() {
        // §9.2 的逻辑 session 上限（每用户 100 / 每组织 1000）与桌面总目标连接 16 是两套额度。
        let usage = BudgetUsage::new(Counter::new(16), Counter::new(3));
        assert_eq!(
            usage.remaining_physical(16),
            0,
            "16 条物理连接已用满 16 的总额度"
        );
        assert_eq!(
            usage.logical.get(),
            3,
            "逻辑 session 独立计数，不参与物理额度判断"
        );

        // 超发不回绕。
        let overspent = BudgetUsage::new(Counter::new(99), Counter::ZERO);
        assert_eq!(overspent.remaining_physical(16), 0);
    }

    /// 小工具：让「构造成功」在测试里读起来像断言一样明显。
    trait ExpectOk<T> {
        fn expect_ok(self) -> T;
    }

    impl<T, E: std::fmt::Debug> ExpectOk<T> for Result<T, E> {
        fn expect_ok(self) -> T {
            match self {
                Ok(value) => value,
                Err(error) => panic!("期望预算配置被接受，实际被拒：{error:?}"),
            }
        }
    }
}
