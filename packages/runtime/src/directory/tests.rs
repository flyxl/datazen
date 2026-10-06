//! 模块内单元测试：只测**纯逻辑**（投影、期限、ID 生成、条目状态机）。
//!
//! 端到端的场景测试（CM-63 / CM-71 / 原子替换 / 无落盘）在 `tests/` 下，
//! 那里注入的是同一套 [`DirectoryClock`]，但走的是完整目录。
//!
//! 断言原则：每一条都必须在「实现被删掉」之后**失败**。因此这里不写
//! 「调用不 panic」这类恒真断言，只写能区分对错的精确值比较。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use datazen_platform_api::context::OwnerRef;
use datazen_platform_api::id::{
    ClientInstanceId, ConnectionId, DbSessionId, ExecutionId, OrganizationId, PrincipalId,
    RuntimeEpoch, Timestamp, WorkerId,
};
use datazen_platform_api::ports::session_directory::{InvalidationReason, SessionOwner};

use super::entry::{Adjudication, ClosureReason, DirectoryEntry, RouteRejection, RoutingState};
use super::id::{
    DbSessionIdGenerator, SessionIdEntropy, ID_ENTROPY_BYTES, MAX_ID_GENERATION_ATTEMPTS,
};
use super::ttl::{Deadline, DeadlineKind, DeadlineSet};
use super::{project_millis_to_timestamp, InMemorySessionDirectory, MonoInstant};

// ------------------------------------------------------------------ 测试替身

/// 固定投影的时钟：`project` 与 `now` 解耦，测试可以单独动其中一个。
struct FixedClock {
    base_millis: std::sync::atomic::AtomicI64,
}

impl FixedClock {
    fn new(origin_millis: i64) -> Self {
        Self {
            base_millis: std::sync::atomic::AtomicI64::new(origin_millis),
        }
    }

    fn shift_projection(&self, delta_millis: i64) {
        let current = self.base_millis.load(Ordering::Relaxed);
        self.base_millis
            .store(current + delta_millis, Ordering::Relaxed);
    }
}

impl super::DirectoryClock for FixedClock {
    fn now(&self) -> MonoInstant {
        MonoInstant::ZERO
    }

    fn project(&self, at: MonoInstant) -> Timestamp {
        project_millis_to_timestamp(
            self.base_millis.load(Ordering::Relaxed) + (at.as_nanos() / 1_000_000) as i64,
        )
    }
}

/// 恒定熵源：每次都返回同一串字节，用来**强制碰撞**。
struct ConstantEntropy {
    byte: u8,
}

impl SessionIdEntropy for ConstantEntropy {
    fn fill(&self, out: &mut [u8]) {
        for slot in out.iter_mut() {
            *slot = self.byte;
        }
    }
}

/// 递增熵源：每次调用换一串字节，模拟「不碰撞的正常路径」。
struct CountingEntropy {
    counter: AtomicU64,
}

impl SessionIdEntropy for CountingEntropy {
    fn fill(&self, out: &mut [u8]) {
        let value = self.counter.fetch_add(1, Ordering::Relaxed);
        for (index, slot) in out.iter_mut().enumerate() {
            *slot = ((value >> (index % 8 * 8)) & 0xff) as u8 ^ 0xa5;
        }
    }
}

/// 交替熵源：奇数次调用给 0x00，偶数次给 0xff。
/// 用来制造「第一次撞上、第二次不撞」的确定性场景。
struct AlternatingEntropy {
    counter: AtomicU64,
}

impl SessionIdEntropy for AlternatingEntropy {
    fn fill(&self, out: &mut [u8]) {
        let byte = if self
            .counter
            .fetch_add(1, Ordering::Relaxed)
            .is_multiple_of(2)
        {
            0x00
        } else {
            0xff
        };
        for slot in out.iter_mut() {
            *slot = byte;
        }
    }
}

fn owner(db_session_id: &str, runtime_epoch: &str) -> SessionOwner {
    SessionOwner {
        db_session_id: DbSessionId::new(db_session_id),
        organization_id: OrganizationId::new("org-1"),
        principal_id: PrincipalId::new("principal-1"),
        connection_id: ConnectionId::new("conn-1"),
        owner: OwnerRef::ClientSession {
            client_instance_id: ClientInstanceId::new("client-1"),
            purpose: "mcp".to_owned(),
        },
        worker_id: WorkerId::new("worker-1"),
        runtime_epoch: RuntimeEpoch::new(runtime_epoch),
        resource_epoch: 0,
        last_business_activity: Timestamp::new("2026-01-01T00:00:00.000Z"),
    }
}

fn handle(db_session_id: &str, runtime_epoch: &str) -> super::SessionHandle {
    super::SessionHandle {
        db_session_id: DbSessionId::new(db_session_id),
        runtime_epoch: RuntimeEpoch::new(runtime_epoch),
    }
}

// ------------------------------------------------------------------ UTC 投影

#[test]
fn utc_projection_is_epoch_anchored_and_fixed_width() {
    assert_eq!(
        project_millis_to_timestamp(0).as_str(),
        "1970-01-01T00:00:00.000Z"
    );
    // 已知刻度：2026-01-01T00:00:00Z == 1767225600000 ms。
    assert_eq!(
        project_millis_to_timestamp(1_767_225_600_000).as_str(),
        "2026-01-01T00:00:00.000Z"
    );
    // 闰日与世纪闰年都要对，否则投影在长会话上会漂。
    assert_eq!(
        project_millis_to_timestamp(1_709_164_800_123).as_str(),
        "2024-02-29T00:00:00.123Z"
    );
    assert_eq!(
        project_millis_to_timestamp(951_782_400_000).as_str(),
        "2000-02-29T00:00:00.000Z"
    );
}

#[test]
fn utc_projection_is_lexicographically_ordered() {
    // `Timestamp` 是字符串序比较的 ID 类型：宽度不固定就会让「哪个更早」判错。
    let earlier = project_millis_to_timestamp(1_767_225_600_000);
    let later = project_millis_to_timestamp(1_767_225_600_001);
    assert_eq!(earlier.as_str().len(), later.as_str().len());
    assert!(earlier < later, "{earlier:?} 应当早于 {later:?}");
    assert!(later > earlier);
}

#[test]
fn negative_millis_render_before_epoch_without_panicking() {
    let before = project_millis_to_timestamp(-1);
    assert_eq!(before.as_str(), "1969-12-31T23:59:59.999Z");
    assert!(before < project_millis_to_timestamp(0));
}

// ------------------------------------------------------------------ 期限

#[test]
fn earliest_deadline_wins_over_armed_order() {
    let mut set = DeadlineSet::new();
    set.arm(Deadline::new(
        DeadlineKind::SessionIdle,
        MonoInstant::from_nanos(900),
    ));
    set.arm(Deadline::new(
        DeadlineKind::TransactionIdle,
        MonoInstant::from_nanos(300),
    ));
    set.arm(Deadline::new(
        DeadlineKind::DisconnectGrace,
        MonoInstant::from_nanos(700),
    ));

    let earliest = set.earliest().expect("三个期限都在，必有最早者");
    assert_eq!(earliest.kind(), DeadlineKind::TransactionIdle);
    assert_eq!(earliest.at().as_nanos(), 300);
    assert_eq!(set.len(), 3);
}

#[test]
fn same_instant_deadlines_tie_break_to_a_unique_kind() {
    let mut set = DeadlineSet::new();
    set.arm(Deadline::new(
        DeadlineKind::SessionIdle,
        MonoInstant::from_nanos(500),
    ));
    set.arm(Deadline::new(
        DeadlineKind::DisconnectGrace,
        MonoInstant::from_nanos(500),
    ));

    let earliest = set.earliest().expect("同刻也有最早者");
    assert_eq!(earliest.kind(), DeadlineKind::DisconnectGrace);
    assert_eq!(
        set.due_kind(MonoInstant::from_nanos(500)),
        Some(DeadlineKind::DisconnectGrace)
    );
}

#[test]
fn re_arming_same_kind_overrides_instead_of_stacking() {
    let mut set = DeadlineSet::new();
    set.arm(Deadline::new(
        DeadlineKind::TransactionIdle,
        MonoInstant::from_nanos(100),
    ));
    set.arm(Deadline::new(
        DeadlineKind::TransactionIdle,
        MonoInstant::from_nanos(800),
    ));
    assert_eq!(set.len(), 1, "同类期限只能有一个");
    assert_eq!(
        set.get(DeadlineKind::TransactionIdle).map(Deadline::at),
        Some(MonoInstant::from_nanos(800))
    );
    assert!(set.cancel(DeadlineKind::TransactionIdle));
    assert!(
        !set.cancel(DeadlineKind::TransactionIdle),
        "重复摘除不是错误"
    );
    assert!(set.is_empty());
}

#[test]
fn no_deadline_projects_to_none_and_never_expires() {
    let clock = FixedClock::new(0);
    let set = DeadlineSet::new();
    assert_eq!(set.expiration_projection(&clock), None);
    assert!(!set.is_due(MonoInstant::from_nanos(u128::MAX)));
}

#[test]
fn due_decision_ignores_the_wall_clock_projection() {
    let clock = FixedClock::new(1_767_225_600_000);
    let mut set = DeadlineSet::new();
    set.arm(Deadline::new(
        DeadlineKind::SessionIdle,
        MonoInstant::from_nanos(1_000),
    ));

    // 把 UTC 投影整体前移一年：到期判定**不能**跟着变。
    clock.shift_projection(365 * 86_400_000);
    assert!(
        !set.is_due(MonoInstant::from_nanos(999)),
        "墙钟前移不应让未到点的期限变成已到期"
    );
    assert!(set.is_due(MonoInstant::from_nanos(1_000)));
}

// ------------------------------------------------------------------ ID 生成

#[test]
fn generated_session_id_carries_128_random_bits() {
    assert_eq!(ID_ENTROPY_BYTES * 8, 128);
    let generator = DbSessionIdGenerator::new();
    let id = generator.generate();
    let raw = id.as_str().strip_prefix("dbs_").expect("前缀固定为 dbs_");
    assert_eq!(raw.len(), 32, "128 位 = 32 位十六进制");
    assert!(raw.chars().all(|c| c.is_ascii_hexdigit()));

    let other = generator.generate();
    assert_ne!(id, other, "OS 熵源必须给出不同 ID");
}

#[test]
fn generated_candidate_is_checked_against_taken_ids() {
    let generator = DbSessionIdGenerator::with_entropy(Arc::new(CountingEntropy {
        counter: AtomicU64::new(0),
    }));
    let first = generator.generate();
    let checks = AtomicU64::new(0);
    let id = generator
        .generate_distinct(
            |candidate| {
                checks.fetch_add(1, Ordering::Relaxed);
                candidate == &first
            },
            MAX_ID_GENERATION_ATTEMPTS,
        )
        .expect("递增熵不会撞上刚生成的那个");
    assert_eq!(checks.load(Ordering::Relaxed), 1, "去重判定必须真的被问过");
    assert_ne!(id, first);
}

#[test]
fn collision_triggers_regeneration_instead_of_reuse() {
    let generator = DbSessionIdGenerator::with_entropy(Arc::new(AlternatingEntropy {
        counter: AtomicU64::new(0),
    }));
    // 熵源只在 A / B 之间摆动，目录里已经占了 A：第一次必须被判碰撞，第二次才通过。
    let occupied = generator.generate();
    let id = generator
        .generate_distinct(
            |candidate| candidate == &occupied,
            MAX_ID_GENERATION_ATTEMPTS,
        )
        .expect("第二次取值与已占用者不同");
    assert_ne!(id, occupied, "撞上就换一个，绝不复用被占用的 ID");
    assert_eq!(generator.attempts(), 2, "恰好重试了一轮");
}

#[test]
fn exhausted_collision_budget_fails_instead_of_returning_a_taken_id() {
    let generator = DbSessionIdGenerator::with_entropy(Arc::new(ConstantEntropy { byte: 0x3c }));
    let err = generator
        .generate_distinct(|_| true, MAX_ID_GENERATION_ATTEMPTS)
        .expect_err("永远被占用时必须失败");
    assert!(
        matches!(
            err,
            datazen_platform_api::error::PortError::BackendUnavailable(_)
        ),
        "耗尽重试只能落到 BackendUnavailable，实得 {err:?}"
    );
    assert_eq!(
        generator.attempts(),
        MAX_ID_GENERATION_ATTEMPTS as u64,
        "必须真的重试满预算，而不是一次就放弃"
    );
}

#[test]
fn worker_restart_always_yields_a_new_runtime_epoch() {
    let directory = InMemorySessionDirectory::new();
    let first = directory.start_worker_epoch(&WorkerId::new("worker-1"));
    let second = directory.start_worker_epoch(&WorkerId::new("worker-1"));
    assert_ne!(first, second, "每次 worker 启动都必须换 epoch");
    assert!(first.as_str().starts_with("rte_"));
}

// ------------------------------------------------------------------ 条目状态机

#[test]
fn prepared_candidate_is_not_routable_until_published() {
    let mut entry = DirectoryEntry::prepared(owner("dbs_p", "rte_1"), MonoInstant::ZERO);
    assert_eq!(entry.state(), RoutingState::CommitBarrier);
    assert!(!entry.is_routable(), "候选在提交前不可路由");
    assert!(entry.route(&handle("dbs_p", "rte_1")).is_err());

    entry.publish();
    assert_eq!(entry.state(), RoutingState::Routable);
    assert!(entry.route(&handle("dbs_p", "rte_1")).is_ok());
}

#[test]
fn stale_epoch_is_rejected_with_a_distinct_rejection() {
    let entry = DirectoryEntry::new(owner("dbs_s", "rte_2"), MonoInstant::ZERO);
    let rejection = entry
        .route(&handle("dbs_s", "rte_1"))
        .expect_err("上一代 epoch 的句柄必须显式失败");
    assert!(matches!(rejection, RouteRejection::StaleEpoch { .. }));
    assert!(entry.route(&handle("dbs_s", "rte_2")).is_ok());
}

#[test]
fn adjudication_moves_to_closing_with_the_due_kind() {
    let clock = FixedClock::new(1_767_225_600_000);
    let mut entry = DirectoryEntry::new(owner("dbs_a", "rte_1"), MonoInstant::ZERO);
    entry.attach();
    entry
        .arm(
            DeadlineKind::TransactionIdle,
            MonoInstant::from_nanos(5_000),
        )
        .expect("可路由条目可以挂期限");

    assert_eq!(
        entry.adjudicate(MonoInstant::from_nanos(4_999)),
        Adjudication::Pending
    );
    assert!(entry.is_routable());
    assert!(entry.is_attachable());

    let adjudicated = entry.adjudicate(MonoInstant::from_nanos(5_000));
    assert_eq!(
        adjudicated,
        Adjudication::Expired {
            kind: DeadlineKind::TransactionIdle
        }
    );
    assert_eq!(entry.state(), RoutingState::Closing);
    assert!(!entry.attached(), "进入关闭中即视为已摘除");
    assert!(!entry.is_attachable(), "Closing/expired 不可再挂载");
    assert!(!entry.is_routable());
    assert_eq!(
        entry.closure().and_then(ClosureReason::expiration),
        Some(DeadlineKind::TransactionIdle)
    );
    assert_eq!(
        entry.last_adjudicated_at(),
        Some(MonoInstant::from_nanos(5_000))
    );
    assert!(entry.observe(&clock).expires_at.is_some());
}

#[test]
fn repeated_adjudication_is_idempotent_and_keeps_first_reason() {
    let mut entry = DirectoryEntry::new(owner("dbs_a", "rte_1"), MonoInstant::ZERO);
    entry
        .arm(DeadlineKind::SessionIdle, MonoInstant::from_nanos(10))
        .expect("可路由条目可以挂期限");
    assert!(entry.adjudicate(MonoInstant::from_nanos(10)).is_expired());
    let first = entry.closure().cloned();

    // 再裁决一次不得换理由，也不得把状态往回搬：Closing 不再参与到期裁决。
    assert!(!entry.adjudicate(MonoInstant::from_nanos(20)).is_expired());
    assert_eq!(entry.closure().cloned(), first);
    assert_eq!(entry.state(), RoutingState::Closing);
    assert_eq!(
        entry.last_adjudicated_at(),
        Some(MonoInstant::from_nanos(10)),
        "裁决时刻不得被后续调用覆盖"
    );
}

#[test]
fn barrier_blocks_new_execution_while_old_is_draining() {
    let old_handle = handle("dbs_old", "rte_1");
    let mut old_entry = DirectoryEntry::new(owner("dbs_old", "rte_1"), MonoInstant::ZERO);
    old_entry
        .begin_execution(ExecutionId::new("exec-1"))
        .expect("空闲会话可以开始执行");
    old_entry.hold_commit_barrier(Some(handle("dbs_new", "rte_1")));

    assert!(!old_entry.is_routable(), "屏障期间旧条目不可路由");
    assert!(old_entry.has_active_execution(), "活跃执行仍在排空中");
    assert_eq!(
        old_entry.closure().cloned(),
        Some(ClosureReason::ReplacedBy(handle("dbs_new", "rte_1")))
    );

    old_entry.end_execution(&ExecutionId::new("exec-1"));
    old_entry.close(ClosureReason::ReplacedBy(handle("dbs_new", "rte_1")));
    assert_eq!(old_entry.state(), RoutingState::Closed);
    assert!(matches!(
        old_entry.route(&old_handle),
        Err(RouteRejection::Closed { .. })
    ));
}

#[test]
fn rolled_back_barrier_restores_the_old_entry() {
    let mut old_entry = DirectoryEntry::new(owner("dbs_old", "rte_1"), MonoInstant::ZERO);
    old_entry.hold_commit_barrier(Some(handle("dbs_new", "rte_1")));
    assert!(!old_entry.is_routable());

    assert!(old_entry.restore(), "restore 必须真的把状态搬回去");
    assert_eq!(old_entry.state(), RoutingState::Routable);
    assert!(old_entry.closure().is_none());
    assert!(old_entry.route(&handle("dbs_old", "rte_1")).is_ok());
}

#[test]
fn second_execution_cannot_hijack_a_busy_session() {
    let mut entry = DirectoryEntry::new(owner("dbs_b", "rte_1"), MonoInstant::ZERO);
    entry
        .begin_execution(ExecutionId::new("exec-1"))
        .expect("空闲会话可以开始执行");
    assert_eq!(
        entry.begin_execution(ExecutionId::new("exec-2")),
        Err(ExecutionId::new("exec-1")),
        "已有执行在跑时必须报出占用者而不是顶掉它"
    );
    assert!(!entry.end_execution(&ExecutionId::new("exec-2")));
    assert!(entry.end_execution(&ExecutionId::new("exec-1")));
    assert!(!entry.has_active_execution());
}

#[test]
fn attachment_flags_flip_without_touching_deadlines() {
    let mut entry = DirectoryEntry::new(owner("dbs_t", "rte_1"), MonoInstant::ZERO);
    entry
        .arm(DeadlineKind::SessionIdle, MonoInstant::from_nanos(4_000))
        .expect("可路由条目可以挂期限");
    let before = entry.deadlines().clone();

    assert!(!entry.attached());
    assert!(entry.attach());
    assert!(!entry.attach(), "重复 attach 不改变状态");
    assert!(entry.detach());
    assert!(!entry.detach(), "重复 detach 不改变状态");
    assert!(!entry.attached());
    assert_eq!(entry.deadlines(), &before, "挂载动作不得动期限");
}

#[test]
fn heartbeat_records_activity_but_not_expiry() {
    let mut entry = DirectoryEntry::new(owner("dbs_h", "rte_1"), MonoInstant::ZERO);
    entry
        .arm(DeadlineKind::SessionIdle, MonoInstant::from_nanos(1_000))
        .expect("可路由条目可以挂期限");
    let deadline_before = entry.deadlines().clone();

    entry.record_business_activity(Timestamp::new("2026-01-01T00:05:00.000Z"));

    assert_eq!(entry.deadlines(), &deadline_before);
    assert_eq!(
        entry.owner().last_business_activity.as_str(),
        "2026-01-01T00:05:00.000Z"
    );
    assert!(entry.adjudicate(MonoInstant::from_nanos(999)) == Adjudication::Pending);
    assert!(entry
        .adjudicate(MonoInstant::from_nanos(1_000))
        .is_expired());
}

#[test]
fn invalidation_reasons_stay_separately_readable() {
    for reason in [
        InvalidationReason::WorkerLost,
        InvalidationReason::ResourceLost,
        InvalidationReason::IdentityRotated,
        InvalidationReason::PolicyChanged,
    ] {
        let mut entry = DirectoryEntry::new(owner("dbs_i", "rte_1"), MonoInstant::ZERO);
        entry.close(ClosureReason::Invalidated(reason));
        assert_eq!(
            entry.closure().and_then(ClosureReason::invalidation),
            Some(reason)
        );
        assert_eq!(entry.closure().and_then(ClosureReason::expiration), None);
        assert!(!entry.is_routable());
    }
}

#[test]
fn expiry_projection_follows_the_monotonic_deadline_not_now() {
    let clock = FixedClock::new(1_767_225_600_000);
    let mut entry = DirectoryEntry::new(owner("dbs_p2", "rte_1"), MonoInstant::ZERO);
    entry
        .arm(
            DeadlineKind::DisconnectGrace,
            MonoInstant::from_nanos(Duration::from_secs(90).as_nanos()),
        )
        .expect("可路由条目可以挂期限");

    let observed = entry.observe(&clock);
    assert_eq!(
        observed.expires_in_nanos,
        Some(Duration::from_secs(90).as_nanos())
    );
    assert_eq!(
        observed.expires_at.map(|at| at.as_str().to_owned()),
        Some("2026-01-01T00:01:30.000Z".to_owned()),
        "expiresAt 是最早期限的 UTC 投影"
    );
}
