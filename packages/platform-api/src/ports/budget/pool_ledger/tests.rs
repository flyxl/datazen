//! [`InMemoryDriverPoolBudget`] 的记账行为测试。
//!
//! 拆成独立文件是为了守住单文件 800 行上限；测试与实现在同一个模块树里，
//! 因此 `use super::*` 拿到的就是实现本身，没有第二份契约。

use super::*;
use crate::context::OwnerRef;
use crate::id::{
    ClientInstanceId, ConfigRevision, ConnectionId, CredentialRevision, DriverResourceKey,
    EditorSessionId, ExecutionIdentityKey, LeaseId, NetworkRouteRevision, OrganizationId,
    PolicyIsolationKey, PrincipalId, WorkerId,
};

const AT: &str = "2026-01-01T00:00:00Z";
const LATER: &str = "2026-01-01T00:05:00Z";

fn fixed_clock() -> Arc<dyn Fn() -> Timestamp + Send + Sync> {
    Arc::new(|| Timestamp::new(AT))
}

fn organization() -> OrganizationId {
    OrganizationId::new("org-1")
}

fn pool_key() -> PoolKey {
    PoolKey {
        organization_id: organization(),
        connection_id: ConnectionId::new("conn-1"),
        config_revision: ConfigRevision::new(1),
        credential_revision: CredentialRevision::new(1),
        execution_identity_key: ExecutionIdentityKey::new("identity-1"),
        policy_isolation_key: PolicyIsolationKey::new("policy-1"),
        driver_resource_key: DriverResourceKey::new("db-1"),
        network_route_revision: NetworkRouteRevision::new(1),
    }
}

fn organization_dimension() -> BudgetDimension {
    BudgetDimension::Organization {
        organization_id: organization(),
    }
}

fn service_dimension() -> BudgetDimension {
    BudgetDimension::DatabaseService {
        organization_id: organization(),
        connection_id: ConnectionId::new("conn-1"),
    }
}

fn quota(total: u32) -> ServiceQuota {
    ServiceQuota::new(total, [0; 4]).expect("测试额度合法：保留之和不超过总额")
}

/// 组织 20 / 连接 20：额度充足。
fn roomy_pool() -> InMemoryDriverPoolBudget {
    pool_with(&[
        (organization_dimension(), quota(20)),
        (service_dimension(), quota(20)),
    ])
}

/// 组织 1 / 连接 20：组织层先见底。
fn org_capped_pool() -> InMemoryDriverPoolBudget {
    pool_with(&[
        (organization_dimension(), quota(1)),
        (service_dimension(), quota(20)),
    ])
}

fn pool_with(quotas: &[(BudgetDimension, ServiceQuota)]) -> InMemoryDriverPoolBudget {
    InMemoryDriverPoolBudget::new(PoolBudgetConfig::new(quotas.to_vec()), fixed_clock())
}

fn request_for(dimension: BudgetDimension, class: ResourceClass, count: u32) -> PoolAcquireRequest {
    PoolAcquireRequest {
        pool_key: pool_key(),
        class,
        dimension,
        owner: OwnerRef::Editor {
            client_instance_id: ClientInstanceId::new("client-1"),
            editor_session_id: EditorSessionId::new("editor-1"),
        },
        requested_physical: count,
        acquire_timeout_ms: None,
    }
}

fn request(count: u32) -> PoolAcquireRequest {
    request_for(service_dimension(), ResourceClass::Interactive, count)
}

async fn granted<P: DriverPoolBudgetPort + ?Sized>(
    port: &P,
    request: &PoolAcquireRequest,
) -> PoolLease {
    match port.acquire_physical(request).await {
        Ok(PoolAcquisition::Granted(lease)) => lease,
        Ok(PoolAcquisition::Denied(denial)) => {
            panic!("额度充足却没拿到连接：{denial:?}")
        }
        Err(error) => panic!("额度不足是返回值而不是错误：{error:?}"),
    }
}

async fn denied<P: DriverPoolBudgetPort + ?Sized>(
    port: &P,
    request: &PoolAcquireRequest,
) -> QuotaDenial {
    match port.acquire_physical(request).await {
        Ok(PoolAcquisition::Denied(denial)) => denial,
        Ok(PoolAcquisition::Granted(lease)) => panic!("不该发出来的租约：{lease:?}"),
        Err(error) => panic!("业务拒绝必须走返回值：{error:?}"),
    }
}

async fn physical_used<P: DriverPoolBudgetPort + ?Sized>(
    port: &P,
    dimension: &BudgetDimension,
    class: ResourceClass,
) -> u32 {
    let key = QuotaKey::new(dimension.clone(), class);
    let usage = port.usage(&key).await.expect("额度查询不是错误");
    assert_eq!(
        usage.logical,
        Counter::ZERO,
        "池端口只记物理额度，逻辑许可归 BudgetCoordinator"
    );
    u32::try_from(usage.physical.get()).unwrap_or(u32::MAX)
}

async fn stats<P: DriverPoolBudgetPort + ?Sized>(port: &P, key: &PoolKey) -> PoolStats {
    port.pool_stats(key).await.expect("池快照存在")
}

#[tokio::test]
async fn every_capped_layer_is_charged_and_returning_keeps_the_charge() {
    let pool = roomy_pool();
    let lease = granted(&pool, &request(1)).await;
    assert_eq!(lease.occupancy, PhysicalOccupancy::InUse);
    assert_eq!(
        physical_used(&pool, &organization_dimension(), ResourceClass::Interactive).await,
        1,
        "组织层必须记账"
    );
    assert_eq!(
        physical_used(&pool, &service_dimension(), ResourceClass::Interactive).await,
        1
    );

    let verdict = pool
        .return_to_pool(&lease, PoolReturnCheck::new(true, true))
        .await
        .expect("归池不是错误");
    assert_eq!(verdict, PoolReturnVerdict::AdmittedToIdlePool);
    assert_eq!(
        physical_used(&pool, &organization_dimension(), ResourceClass::Interactive).await,
        1,
        "归池不释放预算"
    );
    assert_eq!(
        physical_used(&pool, &service_dimension(), ResourceClass::Interactive).await,
        1
    );

    let snapshot = stats(&pool, &pool_key()).await;
    assert_eq!(snapshot.physical, Counter::new(1));
    assert_eq!(snapshot.idle, Counter::new(1));
    assert_eq!(snapshot.usage.physical, Counter::new(1));
    assert_eq!(snapshot.class, ResourceClass::Interactive, "归池不改变类别");
}

#[tokio::test]
async fn the_outermost_exhausted_layer_is_the_denial_reported() {
    let pool = org_capped_pool();
    let _first = granted(&pool, &request(1)).await;
    let denial = denied(&pool, &request(1)).await;
    assert_eq!(
        denial,
        QuotaDenial::Insufficient {
            key: QuotaKey::new(organization_dimension(), ResourceClass::Interactive),
            requested: 1,
            available: 0,
        }
    );
    assert_eq!(
        denial.key(),
        Some(&QuotaKey::new(
            organization_dimension(),
            ResourceClass::Interactive
        ))
    );
}

#[tokio::test]
async fn a_refused_acquire_charges_nothing_on_any_layer() {
    let pool = org_capped_pool();
    let _first = granted(&pool, &request(1)).await;
    let before_org =
        physical_used(&pool, &organization_dimension(), ResourceClass::Interactive).await;
    let before_service =
        physical_used(&pool, &service_dimension(), ResourceClass::Interactive).await;
    let _refused = denied(&pool, &request(2)).await;
    assert_eq!(
        physical_used(&pool, &organization_dimension(), ResourceClass::Interactive).await,
        before_org,
        "失败的整批申请不能留下半个记账"
    );
    assert_eq!(
        physical_used(&pool, &service_dimension(), ResourceClass::Interactive).await,
        before_service,
        "组织层拒绝时，连接层不能先扣再退"
    );
}

#[tokio::test]
async fn the_dimension_total_caps_every_class_at_once() {
    let pool = pool_with(&[
        (organization_dimension(), quota(1)),
        (service_dimension(), quota(1)),
    ]);
    let _interactive = granted(&pool, &request(1)).await;
    let denial = denied(
        &pool,
        &request_for(service_dimension(), ResourceClass::Job, 1),
    )
    .await;
    assert_eq!(
        denial,
        QuotaDenial::Insufficient {
            key: QuotaKey::new(organization_dimension(), ResourceClass::Job),
            requested: 1,
            available: 0,
        },
        "Job 这一格自己还剩 1，是组织总额把它压成 0"
    );
    assert_eq!(
        physical_used(&pool, &organization_dimension(), ResourceClass::Job).await,
        0
    );
    assert_eq!(
        physical_used(&pool, &service_dimension(), ResourceClass::Job).await,
        0
    );
    assert_eq!(
        physical_used(&pool, &organization_dimension(), ResourceClass::Interactive).await,
        1,
        "别人的连接不该被这笔被拒的申请挤掉"
    );
}

#[tokio::test]
async fn a_batch_lease_is_charged_and_released_as_one_block() {
    let pool = roomy_pool();
    let lease = granted(&pool, &request(4)).await;
    assert_eq!(lease.physical_count, 4);
    assert_eq!(lease.quota_keys().len(), 2, "组织 + 连接两层");
    assert_eq!(
        physical_used(&pool, &service_dimension(), ResourceClass::Interactive).await,
        4
    );
    assert_eq!(stats(&pool, &pool_key()).await.physical, Counter::new(4));

    let receipt = pool
        .release_physical(&lease, ReleaseSource::SessionClosed)
        .await
        .expect("释放不是错误");
    assert_eq!(receipt.disposition, ReleaseDisposition::Closed);
    assert_eq!(receipt.released_physical, Counter::new(4));
    assert_eq!(receipt.remaining_idle, Counter::ZERO);
    assert_eq!(receipt.source, ReleaseSource::SessionClosed);
    assert_eq!(receipt.at, Timestamp::new(AT));
    assert_eq!(
        physical_used(&pool, &organization_dimension(), ResourceClass::Interactive).await,
        0,
        "两层一起归零"
    );
    assert_eq!(
        physical_used(&pool, &service_dimension(), ResourceClass::Interactive).await,
        0
    );
}

#[tokio::test]
async fn closing_a_lease_releases_exactly_once() {
    let pool = roomy_pool();
    let lease = granted(&pool, &request(1)).await;
    pool.release_physical(&lease, ReleaseSource::SessionClosed)
        .await
        .expect("释放不是错误");
    assert!(
        matches!(
            pool.release_physical(&lease, ReleaseSource::SessionClosed)
                .await,
            Err(PortError::NotFound(_))
        ),
        "同一条租约不能释放两次，第二次绝不能凭空造一张回执"
    );
    assert_eq!(
        physical_used(&pool, &service_dimension(), ResourceClass::Interactive).await,
        0,
        "重复释放不能把额度退成负数或翻倍"
    );
}

#[tokio::test]
async fn a_failed_return_marks_the_lease_for_cleanup_and_keeps_the_charge() {
    let pool = roomy_pool();
    let lease = granted(&pool, &request(1)).await;
    let verdict = pool
        .return_to_pool(&lease, PoolReturnCheck::new(true, false))
        .await
        .expect("归池不是错误");
    assert_eq!(
        verdict,
        PoolReturnVerdict::CleanupRequired {
            reason: QuarantineReason::DriverUnclean
        }
    );
    let snapshot = stats(&pool, &pool_key()).await;
    assert_eq!(snapshot.idle, Counter::ZERO, "不干净的连接不进 idle");
    assert_eq!(snapshot.physical, Counter::new(1));
    assert_eq!(
        physical_used(&pool, &service_dimension(), ResourceClass::Interactive).await,
        1,
        "清理前额度仍然被占着"
    );

    let receipt = pool
        .release_physical(&lease, ReleaseSource::CleanupAfterReturn)
        .await
        .expect("清理释放不是错误");
    assert_eq!(receipt.disposition, ReleaseDisposition::Closed);
    assert_eq!(receipt.released_physical, Counter::new(1));
    assert_eq!(
        physical_used(&pool, &service_dimension(), ResourceClass::Interactive).await,
        0
    );
}

#[tokio::test]
async fn an_idle_lease_is_reused_without_charging_twice() {
    let pool = roomy_pool();
    let first = granted(&pool, &request(1)).await;
    pool.return_to_pool(&first, PoolReturnCheck::new(true, true))
        .await
        .expect("归池不是错误");

    let second = granted(&pool, &request(1)).await;
    assert_eq!(first.physical, second.physical, "复用的是同一条物理连接");
    assert_ne!(
        first.lease_id, second.lease_id,
        "租约必须换新，否则两次释放会互相踩"
    );
    assert_eq!(
        physical_used(&pool, &organization_dimension(), ResourceClass::Interactive).await,
        1,
        "复用不得二次记账"
    );
    assert_eq!(
        physical_used(&pool, &service_dimension(), ResourceClass::Interactive).await,
        1
    );
    let snapshot = stats(&pool, &pool_key()).await;
    assert_eq!(snapshot.physical, Counter::new(1), "池里还是那一条");
    assert_eq!(snapshot.idle, Counter::ZERO, "它已经交出去了");
}

#[tokio::test]
async fn an_idle_lease_is_never_handed_to_another_class_or_dimension() {
    let principal_dimension = BudgetDimension::Principal {
        organization_id: organization(),
        principal_id: PrincipalId::new("principal-1"),
    };
    let pool = pool_with(&[
        (organization_dimension(), quota(20)),
        (service_dimension(), quota(20)),
        (principal_dimension.clone(), quota(20)),
    ]);
    let first = granted(&pool, &request(1)).await;
    pool.return_to_pool(&first, PoolReturnCheck::new(true, true))
        .await
        .expect("归池不是错误");

    let job = granted(
        &pool,
        &request_for(service_dimension(), ResourceClass::Job, 1),
    )
    .await;
    assert_ne!(job.physical, first.physical, "换类别就得另借");
    assert_eq!(
        physical_used(&pool, &organization_dimension(), ResourceClass::Interactive).await,
        1,
        "旧 idle 仍按原类别占着额度（§9.5）"
    );
    assert_eq!(
        physical_used(&pool, &organization_dimension(), ResourceClass::Job).await,
        1
    );

    let metadata = granted(
        &pool,
        &request_for(principal_dimension, ResourceClass::Metadata, 1),
    )
    .await;
    assert_ne!(metadata.physical, first.physical, "换维度就得另借");
    assert_eq!(
        physical_used(&pool, &organization_dimension(), ResourceClass::Metadata).await,
        1
    );
}

#[tokio::test]
async fn an_idle_lease_is_not_handed_out_for_a_different_batch_size() {
    let pool = roomy_pool();
    let first = granted(&pool, &request(3)).await;
    pool.return_to_pool(&first, PoolReturnCheck::new(true, true))
        .await
        .expect("归池不是错误");
    let second = granted(&pool, &request(1)).await;
    assert_ne!(first.physical, second.physical, "批量预留不能被拆开发");
    assert_eq!(
        physical_used(&pool, &service_dimension(), ResourceClass::Interactive).await,
        4,
        "3 + 1 而不是替换"
    );
}

#[tokio::test]
async fn quarantine_keeps_holding_the_budget_until_the_lease_is_closed() {
    let pool = roomy_pool();
    let lease = granted(&pool, &request(1)).await;
    let quarantined = pool
        .quarantine(&lease, QuarantineReason::TunnelLost)
        .await
        .expect("隔离不是错误");
    assert_eq!(quarantined.occupancy, PhysicalOccupancy::Quarantined);
    assert_eq!(
        quarantined.lease_id, lease.lease_id,
        "隔离是标记，不是换租约"
    );
    assert_eq!(
        physical_used(&pool, &organization_dimension(), ResourceClass::Interactive).await,
        1,
        "隔离不释放额度"
    );
    let snapshot = stats(&pool, &pool_key()).await;
    assert_eq!(snapshot.idle, Counter::ZERO);
    assert_eq!(snapshot.physical, Counter::new(1));

    let receipt = pool
        .release_physical(&quarantined, ReleaseSource::SessionClosed)
        .await
        .expect("close 不是错误");
    assert_eq!(receipt.released_physical, Counter::new(1));
    assert_eq!(
        physical_used(&pool, &organization_dimension(), ResourceClass::Interactive).await,
        0
    );
}

#[tokio::test]
async fn pool_stats_fades_out_with_the_pool() {
    let pool = roomy_pool();
    assert!(
        matches!(
            pool.pool_stats(&pool_key()).await,
            Err(PortError::NotFound(_))
        ),
        "没有池就没有快照"
    );
    let lease = granted(&pool, &request(1)).await;
    let snapshot = stats(&pool, &pool_key()).await;
    assert_eq!(snapshot.pool_key, pool_key());
    assert_eq!(snapshot.physical, Counter::new(1));
    assert_eq!(snapshot.idle, Counter::ZERO);
    assert_eq!(snapshot.idle_ttl_ms, 60_000, "§9.6 的起点");
    assert_eq!(snapshot.max_empty_entries, 32, "§9.6 的起点");

    pool.release_physical(&lease, ReleaseSource::SessionClosed)
        .await
        .expect("释放不是错误");
    assert!(
        matches!(
            pool.pool_stats(&pool_key()).await,
            Err(PortError::NotFound(_))
        ),
        "池空了就不留空条目"
    );

    let rotated = PoolKey {
        credential_revision: CredentialRevision::new(2),
        ..pool_key()
    };
    assert!(
        matches!(pool.pool_stats(&rotated).await, Err(PortError::NotFound(_))),
        "池键轮换产生新池，旧键不得复用（§9.6）"
    );
}

#[tokio::test]
async fn pool_limits_are_reported_verbatim() {
    let pool = InMemoryDriverPoolBudget::new(
        PoolBudgetConfig::new(vec![
            (organization_dimension(), quota(4)),
            (service_dimension(), quota(4)),
        ])
        .with_pool_limits(1_500, 4),
        fixed_clock(),
    );
    let lease = granted(&pool, &request(1)).await;
    let snapshot = stats(&pool, &pool_key()).await;
    assert_eq!(snapshot.idle_ttl_ms, 1_500);
    assert_eq!(snapshot.max_empty_entries, 4);
    pool.release_physical(&lease, ReleaseSource::IdleEvicted)
        .await
        .expect("驱逐不是错误");
}

#[tokio::test]
async fn usage_reports_physical_only_and_pool_stats_mirrors_it() {
    let pool = roomy_pool();
    let _lease = granted(&pool, &request(2)).await;
    let key = QuotaKey::new(service_dimension(), ResourceClass::Interactive);
    let usage = pool.usage(&key).await.expect("额度查询不是错误");
    assert_eq!(usage.physical, Counter::new(2));
    assert_eq!(usage.logical, Counter::ZERO);
    assert_eq!(usage.remaining_physical(5), 3);
    let snapshot = stats(&pool, &pool_key()).await;
    assert_eq!(snapshot.usage, usage);
    assert_eq!(snapshot.physical, Counter::new(2));
    assert_eq!(snapshot.usage.physical, snapshot.physical);
}

#[tokio::test]
async fn a_control_class_request_is_refused_before_it_can_take_a_driver_slot() {
    let pool = roomy_pool();
    let error = pool
        .acquire_physical(&request_for(service_dimension(), ResourceClass::Control, 1))
        .await
        .expect_err("control socket 不走 driver pool");
    assert!(matches!(error, PortError::QuotaExceeded(_)), "{error:?}");
    assert_eq!(
        physical_used(&pool, &service_dimension(), ResourceClass::Interactive).await,
        0,
        "被拒的申请不能留下记账"
    );
}

#[tokio::test]
async fn a_zero_physical_request_is_refused() {
    let pool = roomy_pool();
    let error = pool
        .acquire_physical(&request(0))
        .await
        .expect_err("0 条连接不是一次发放");
    assert!(matches!(error, PortError::QuotaExceeded(_)), "{error:?}");
    assert!(
        matches!(
            pool.pool_stats(&pool_key()).await,
            Err(PortError::NotFound(_))
        ),
        "被拒的申请不能留下池记录"
    );
}

#[tokio::test]
async fn an_unconfigured_dimension_is_denied_rather_than_metered_freely() {
    let pool = pool_with(&[(service_dimension(), quota(20))]);
    let denial = denied(&pool, &request(1)).await;
    assert_eq!(
        denial,
        QuotaDenial::Insufficient {
            key: QuotaKey::new(organization_dimension(), ResourceClass::Interactive),
            requested: 1,
            available: 0,
        },
        "没配封顶的维度一律拒绝（fail closed）"
    );
    assert!(matches!(
        pool.pool_stats(&pool_key()).await,
        Err(PortError::NotFound(_))
    ));
}

#[tokio::test]
async fn draining_reports_what_is_still_outstanding() {
    let pool = roomy_pool();
    let held = granted(&pool, &request(1)).await;
    let pooled = granted(&pool, &request(1)).await;
    pool.return_to_pool(&pooled, PoolReturnCheck::new(true, true))
        .await
        .expect("归池不是错误");

    let status = pool
        .drain_pool(&pool_key(), DrainScope::Organization(organization()))
        .await
        .expect("drain 不是错误");
    assert_eq!(status.scope, DrainScope::Organization(organization()));
    assert_eq!(
        status.outstanding, 1,
        "idle 已经是可关闭的存货，不算 outstanding"
    );
    assert!(
        status.draining,
        "`draining` 是存量派生值：有待关闭的租约就还没排空完"
    );

    pool.release_physical(&held, ReleaseSource::AdminDrain)
        .await
        .expect("排空不是错误");
    let status = pool
        .drain_pool(&pool_key(), DrainScope::Organization(organization()))
        .await
        .expect("drain 不是错误");
    assert_eq!(status.outstanding, 0, "在用的那条已经关闭");
    assert!(!status.draining, "没有待关闭存量就是排空完了");
    assert_eq!(
        stats(&pool, &pool_key()).await.idle,
        Counter::new(1),
        "那条 idle 还在池里等复用，池条目不该消失"
    );

    pool.release_physical(&pooled, ReleaseSource::AdminDrain)
        .await
        .expect("关闭 idle 不是错误");
    assert!(
        matches!(
            pool.pool_stats(&pool_key()).await,
            Err(PortError::NotFound(_))
        ),
        "池里一条租约都不剩时，条目随之消失"
    );
    assert!(matches!(
        pool.drain_pool(&pool_key(), DrainScope::Organization(organization()))
            .await,
        Err(PortError::NotFound(_))
    ));
}

#[tokio::test]
async fn a_node_scoped_drain_has_no_driver_pool_to_drain() {
    let pool = roomy_pool();
    let _lease = granted(&pool, &request(1)).await;
    assert!(
        matches!(
            pool.drain_pool(&pool_key(), DrainScope::Node(WorkerId::new("worker-1")))
                .await,
            Err(PortError::NotFound(_))
        ),
        "节点槽位归 ClusterNodeBudgetPort，不归池（§9.5）"
    );
}

#[tokio::test]
async fn each_grant_mints_its_own_lease_and_physical_ids() {
    let pool = roomy_pool();
    let first = granted(&pool, &request(1)).await;
    let second = granted(&pool, &request(1)).await;
    assert_ne!(first.lease_id, second.lease_id);
    assert_ne!(first.physical, second.physical);
    assert_eq!(first.lease_id.as_str(), "pool-lease-1");
    assert_eq!(second.lease_id.as_str(), "pool-lease-2");
    assert_eq!(first.physical.as_str(), "pool-physical-1");
    assert_eq!(second.physical.as_str(), "pool-physical-2");
}

#[tokio::test]
async fn an_unknown_lease_is_never_invented_into_a_receipt() {
    let pool = roomy_pool();
    let lease = granted(&pool, &request(1)).await;
    let forged = PoolLease {
        lease_id: LeaseId::new("pool-lease-404"),
        ..lease.clone()
    };
    assert!(matches!(
        pool.release_physical(&forged, ReleaseSource::SessionClosed)
            .await,
        Err(PortError::NotFound(_))
    ));
    assert!(matches!(
        pool.return_to_pool(&forged, PoolReturnCheck::new(true, true))
            .await,
        Err(PortError::NotFound(_))
    ));
    assert!(matches!(
        pool.quarantine(&forged, QuarantineReason::HealthCheckFailed)
            .await,
        Err(PortError::NotFound(_))
    ));
    assert_eq!(
        physical_used(&pool, &service_dimension(), ResourceClass::Interactive).await,
        1,
        "这些失败调用不该动过账"
    );
}

#[tokio::test]
async fn the_injected_clock_is_the_only_source_of_timestamps() {
    let cell = Arc::new(Mutex::new(String::from(AT)));
    let clock: Arc<dyn Fn() -> Timestamp + Send + Sync> = {
        let cell = Arc::clone(&cell);
        Arc::new(move || Timestamp::new(cell.lock().map(|v| v.clone()).unwrap_or_default()))
    };
    let pool = InMemoryDriverPoolBudget::new(
        PoolBudgetConfig::new(vec![
            (organization_dimension(), quota(20)),
            (service_dimension(), quota(20)),
        ]),
        Arc::clone(&clock),
    );
    let lease = granted(&pool, &request(1)).await;
    assert_eq!(lease.acquired_at, Timestamp::new(AT));

    {
        let mut tick = cell.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        *tick = String::from(LATER);
    }
    let receipt = pool
        .release_physical(&lease, ReleaseSource::IdleEvicted)
        .await
        .expect("释放不是错误");
    assert_eq!(receipt.at, Timestamp::new(LATER), "回执用注入的时钟");
}

#[tokio::test]
async fn the_ledger_is_reachable_behind_the_trait_object() {
    let pool: Arc<dyn DriverPoolBudgetPort> = Arc::new(roomy_pool());
    let lease = granted(&*pool, &request(2)).await;
    assert_eq!(lease.occupancy, PhysicalOccupancy::InUse);
    let receipt = pool
        .release_physical(&lease, ReleaseSource::Shutdown)
        .await
        .expect("释放不是错误");
    assert_eq!(receipt.released_physical, Counter::new(2));
    assert_eq!(
        physical_used(
            &*pool,
            &organization_dimension(),
            ResourceClass::Interactive
        )
        .await,
        0
    );
}
