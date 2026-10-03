//! 预算门禁（CM-65 / CM-66）测试夹具。
//!
//! 两条纪律：
//!
//! 1. **没有真实等待**：所有时间都走 [`TestClock`]，账本入口接受 `now_ms`，
//!    所以整条调度链是同步的——没有 `sleep`、没有定时器竞态、没有偶发失败。
//! 2. **断言里不写死数字**：期望值一律从配置或账本里读回来再比，
//!    这样改额度配置时测试不会说谎，也不会被绕过。

// 两个门禁二进制共用本文件，但各自只用其中一部分，所以整文件豁免 dead_code。
#![allow(dead_code)]

use std::fmt::Debug;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use datazen_platform_api::id::{ConnectionId, OrganizationId, PrincipalId};
use datazen_platform_api::ports::budget::coordinator::BudgetRequest;
use datazen_platform_api::ports::budget::{ResourceClass, ServiceQuota};

use datazen_runtime::budget::{
    AdmitOutcome, BudgetClaim, BudgetClock, BudgetConfig, BudgetLedger, DenialReason,
    InProcessBudgetCoordinator, PermitRecord, PrincipalResolver, SlotKind,
};

/// 确定性时钟：`now_ms` 只在被测试代码显式 `advance` / `set` 时才前进。
pub struct TestClock {
    now_ms: AtomicU64,
}

impl TestClock {
    /// 从 0ms 起的共享时钟。
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            now_ms: AtomicU64::new(0),
        })
    }

    /// 前进若干毫秒。
    pub fn advance(&self, delta_ms: u64) {
        self.now_ms.fetch_add(delta_ms, Ordering::SeqCst);
    }

    /// 直接跳到某个绝对时刻。
    pub fn set(&self, value_ms: u64) {
        self.now_ms.store(value_ms, Ordering::SeqCst);
    }
}

impl BudgetClock for TestClock {
    fn now_ms(&self) -> u64 {
        self.now_ms.load(Ordering::SeqCst)
    }
}

/// 按 `connectionId` 映射主体的解析器。
///
/// 端口的 `BudgetRequest` 里**没有** `principal_id` 字段（这是「调用方不能自报身份」的
/// 结构性保证），所以多主体场景只能靠服务端注入的解析器恢复。
pub struct FixedPrincipals {
    by_connection: Vec<(String, PrincipalId)>,
}

impl FixedPrincipals {
    pub fn new(by_connection: Vec<(String, PrincipalId)>) -> Arc<Self> {
        Arc::new(Self { by_connection })
    }
}

impl PrincipalResolver for FixedPrincipals {
    fn resolve(&self, request: &BudgetRequest) -> PrincipalId {
        for (connection_id, principal) in &self.by_connection {
            if request.connection_id.as_str() == connection_id {
                return principal.clone();
            }
        }
        // 兜底：与 `OrganizationPrincipal` 同口径，把整个组织视为一个主体。
        PrincipalId::new(request.organization_id.as_str())
    }
}

/// 构造一份合法服务额度；不合法直接 panic（测试夹具不该产出非法额度）。
pub fn quota(total: u32, reserved: [u32; 4]) -> ServiceQuota {
    match ServiceQuota::new(total, reserved) {
        Ok(value) => value,
        Err(error) => panic!("ServiceQuota::new({total}, {reserved:?}) must be valid: {error:?}"),
    }
}

/// 构造一份已校验的预算配置。
pub fn config(total: u32, reserved: [u32; 4]) -> BudgetConfig {
    let config = BudgetConfig::new(quota(total, reserved));
    match config.validate() {
        Ok(()) => config,
        Err(error) => panic!("test fixture config must validate: {error}"),
    }
}

/// 单名额申请。
pub fn claim(
    organization_id: &OrganizationId,
    connection_id: &ConnectionId,
    principal: &str,
    class: ResourceClass,
) -> BudgetClaim {
    BudgetClaim::single(
        organization_id.clone(),
        connection_id.clone(),
        PrincipalId::new(principal),
        class,
    )
}

/// 断言这次准入是放行，并取出 permit 记录。
pub fn granted(outcome: AdmitOutcome) -> PermitRecord {
    match outcome {
        AdmitOutcome::Granted(record) => record,
        other => panic!("expected Granted, got {other:?}"),
    }
}

/// 断言这次准入是「额度暂时不够」（可排队 / 可重试）。
pub fn busy(outcome: AdmitOutcome) -> DenialReason {
    match outcome {
        AdmitOutcome::Busy(reason) => reason,
        other => panic!("expected Busy, got {other:?}"),
    }
}

/// 断言这次准入是终态拒绝（排队也没用）。
pub fn denied(outcome: AdmitOutcome) -> DenialReason {
    match outcome {
        AdmitOutcome::Denied(reason) => reason,
        other => panic!("expected Denied, got {other:?}"),
    }
}

/// `Result::unwrap` 的显式版：失败信息带上上下文，不引入裸 `unwrap()`。
pub fn ok<T, E: Debug>(result: Result<T, E>, context: &str) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("{context}: unexpected error {error:?}"),
    }
}

/// 按类别数一数这次放行里各类各占几个。
pub fn count_class(records: &[PermitRecord], class: ResourceClass) -> usize {
    records
        .iter()
        .filter(|record| record.class == class)
        .count()
}

/// 门禁统一的组织 id。
pub const ORG: &str = "org-acme";

/// 门禁统一的连接 id（= 每个 `DatabaseService` 维度的配额主体）。
pub const CONN: &str = "conn-pg-main";

/// 排队等待者常用的、足够长的期限：只为「不死在断言里」，不参与调度判定。
pub const DEADLINE_MS: u64 = 60_000;

pub fn org() -> OrganizationId {
    OrganizationId::new(ORG)
}

pub fn conn() -> ConnectionId {
    ConnectionId::new(CONN)
}

/// 把共享池**精确**填满（`quota(8, [1,1,1,0])` ⇒ `shared = 8 - 3 = 5`）。
///
/// 槽位分配必须是：interactive 1 保留 + 2 共享、metadata 1 保留 + 1 共享、job 2 共享，
/// 共 7 份在手 permit、共享水位 5。第 4 个 job 请求必须是 `Busy`——顺带证明
/// 「总量 8 = 保留 3 + 共享 5」是硬边界而不是注释里的说法。
pub fn fill_shared_pool(ledger: &mut BudgetLedger, org: &OrganizationId, conn: &ConnectionId) {
    // interactive：1 个保留 + 2 个共享
    assert_eq!(
        granted(ledger.try_admit(&claim(org, conn, "user-a", ResourceClass::Interactive), 0)).slot,
        SlotKind::Reserved
    );
    for _ in 0..2 {
        assert_eq!(
            granted(ledger.try_admit(&claim(org, conn, "user-a", ResourceClass::Interactive), 0))
                .slot,
            SlotKind::Shared
        );
    }
    // metadata：1 个保留 + 1 个共享
    assert_eq!(
        granted(ledger.try_admit(&claim(org, conn, "user-a", ResourceClass::Metadata), 0)).slot,
        SlotKind::Reserved
    );
    assert_eq!(
        granted(ledger.try_admit(&claim(org, conn, "user-a", ResourceClass::Metadata), 0)).slot,
        SlotKind::Shared
    );
    // job：只剩 2 个共享槽，填满共享池。
    for _ in 0..2 {
        assert_eq!(
            granted(ledger.try_admit(&claim(org, conn, "user-a", ResourceClass::Job), 0)).slot,
            SlotKind::Shared
        );
    }
    // 第 4 个 job 请求必须被挡下：7 份在手 = 2 保留 + 5 共享，共享池已经见底。
    match busy(ledger.try_admit(&claim(org, conn, "user-a", ResourceClass::Job), 0)) {
        DenialReason::Exhausted { class, .. } => assert_eq!(class, ResourceClass::Job),
        other => panic!("共享池满时 job 必须报 Exhausted，实际 {other:?}"),
    }
    assert_eq!(ledger.shared_used(conn), 5);
    assert_eq!(ledger.permits().count(), 7);
}

/// 同样的填池动作，但走 `InProcessBudgetCoordinator::admit`：端口只有
/// `UserInteractive` / `BackgroundJob` 两种 purpose，`admit` 才是能排出控制/元数据类
/// 的入口，异步门禁用它铺满共享池（结果同样是 7 份 permit、共享水位 5）。
pub fn fill_shared_pool_via_coordinator(
    coordinator: &InProcessBudgetCoordinator,
    org: &OrganizationId,
    conn: &ConnectionId,
) {
    for (class, want) in [
        (ResourceClass::Interactive, 3),
        (ResourceClass::Metadata, 2),
        (ResourceClass::Job, 2),
    ] {
        for _ in 0..want {
            granted(coordinator.admit(&claim(org, conn, "user-a", class)));
        }
    }
}
