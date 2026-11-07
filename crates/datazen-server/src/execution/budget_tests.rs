//! `ServerBudgetCoordinator` 行为钉子：可取消等待、整体失败释放、幂等核销、
//! 排空拒绝与节点租约失联的端口记账规则。

use std::time::Duration;

use datazen::execution::application::budget::{
    BudgetCoordinator, BudgetPurpose, BudgetRequest, DrainScope, ReleaseOutcome,
};
use datazen::shared::error::PortError;
use datazen::shared::id::{ConnectionId, OrganizationId, WorkerId};

use super::budget::ServerBudgetCoordinator;

fn request(organization: &str, connection: &str) -> BudgetRequest {
    BudgetRequest::new(
        OrganizationId::new(organization),
        ConnectionId::new(connection),
        BudgetPurpose::UserInteractive,
    )
}

fn coordinator(node_limit: u64, organization_limit: u64) -> ServerBudgetCoordinator {
    ServerBudgetCoordinator::new(
        node_limit,
        organization_limit,
        OrganizationId::new("org-node"),
    )
}

#[tokio::test]
async fn try_acquire_grants_and_release_frees_the_slot() {
    let budget = coordinator(1, 1);
    let permit = budget
        .try_acquire(request("org-a", "conn-1"))
        .await
        .expect("try_acquire")
        .expect("capacity available");
    assert_eq!(permit.organization_id.as_str(), "org-a");
    assert_eq!(permit.connection_id.as_str(), "conn-1");

    // 满了：立即 None，不等待。
    let refused = budget
        .try_acquire(request("org-b", "conn-2"))
        .await
        .expect("try_acquire");
    assert!(refused.is_none(), "node limit reached → Ok(None)");

    budget
        .release(&permit, ReleaseOutcome::Consumed)
        .await
        .expect("release");
    let again = budget
        .try_acquire(request("org-b", "conn-2"))
        .await
        .expect("try_acquire");
    assert!(again.is_some(), "released slot is reusable");
}

#[tokio::test]
async fn acquire_times_out_with_provider_timeout() {
    let budget = ServerBudgetCoordinator::with_configuration(
        1,
        1,
        OrganizationId::new("org-node"),
        // 超时是构造参数（测试用 50ms 验证可取消等待路径）。
        Duration::from_millis(50),
        Duration::from_secs(60),
    );
    let permit = budget
        .try_acquire(request("org-a", "conn-1"))
        .await
        .expect("first acquire")
        .expect("granted");
    let error = budget
        .acquire(request("org-a", "conn-2"))
        .await
        .expect_err("second acquire must time out");
    assert!(matches!(error, PortError::ProviderTimeout(_)), "{error:?}");
    budget
        .release(&permit, ReleaseOutcome::Unused)
        .await
        .expect("release");
}

#[tokio::test]
async fn release_is_idempotent_and_does_not_double_free() {
    let budget = coordinator(1, 1);
    let permit = budget
        .try_acquire(request("org-a", "conn-1"))
        .await
        .expect("acquire")
        .expect("granted");
    budget
        .release(&permit, ReleaseOutcome::Consumed)
        .await
        .expect("release");
    budget
        .release(&permit, ReleaseOutcome::Consumed)
        .await
        .expect("repeat release");

    let snapshot = budget.snapshot().await.expect("snapshot");
    assert_eq!(
        snapshot.in_use.get(),
        0,
        "重复归还不得把额度释放成负数再膨胀"
    );
    assert_eq!(
        snapshot.granted_total.get(),
        1,
        "granted_total 只计一次发放"
    );

    // 释放后槽位恰好空出一个（没有因重复归还而多出）。
    let extra = budget
        .try_acquire(request("org-a", "conn-2"))
        .await
        .expect("acquire after idempotent release");
    assert!(extra.is_some());
    let overflow = budget
        .try_acquire(request("org-a", "conn-3"))
        .await
        .expect("acquire");
    assert!(overflow.is_none(), "槽位没有被重复归还凭空放大");
}

#[tokio::test]
async fn reserve_many_is_all_or_nothing() {
    let budget = coordinator(2, 2);
    // 两端都能拿到：整体成功。
    let set = budget
        .reserve_many(&[request("org-a", "conn-1"), request("org-a", "conn-2")])
        .await
        .expect("reserve both");
    assert_eq!(set.len(), 2);

    // 第三端拿不到：整体失败，且之前预留的不留半边（先归还这组再失败）。
    let error = budget
        .reserve_many(&[request("org-b", "conn-3"), request("org-b", "conn-4")])
        .await
        .expect_err("full node refuses the whole set");
    assert!(matches!(error, PortError::QuotaExceeded(_)), "{error:?}");

    let snapshot = budget.snapshot().await.expect("snapshot");
    assert_eq!(
        snapshot.in_use.get(),
        2,
        "失败的 reserve_many 不得留下半边占用"
    );
    assert_eq!(snapshot.granted_total.get(), 2);
}

#[tokio::test]
async fn organization_limit_is_enforced_independently_of_the_node_limit() {
    // 节点上限 10、每组织上限 1：第二个组织仍可用，第一个组织被拒。
    let budget = coordinator(10, 1);
    let first = budget
        .try_acquire(request("org-a", "conn-1"))
        .await
        .expect("acquire")
        .expect("granted");
    let org_refused = budget
        .try_acquire(request("org-a", "conn-2"))
        .await
        .expect("acquire");
    assert!(org_refused.is_none(), "组织级额度独立生效");
    let other_org = budget
        .try_acquire(request("org-b", "conn-3"))
        .await
        .expect("acquire");
    assert!(other_org.is_some(), "别的组织不受 org-a 额度影响");
    budget
        .release(&first, ReleaseOutcome::Unused)
        .await
        .expect("release");
}

#[tokio::test]
async fn drain_refuses_new_permits_for_the_matching_scope() {
    let budget = coordinator(4, 4);
    let held = budget
        .try_acquire(request("org-a", "conn-1"))
        .await
        .expect("acquire")
        .expect("granted");

    let status = budget
        .drain(DrainScope::Organization(OrganizationId::new("org-a")))
        .await
        .expect("drain");
    assert!(status.draining);
    assert_eq!(status.outstanding, 1, "outstanding 报告该范围内的在飞许可");

    let refused = budget.try_acquire(request("org-a", "conn-2")).await;
    assert!(
        matches!(refused, Err(PortError::QuotaExceeded(_))),
        "排空范围内不再发放新许可，{refused:?}"
    );
    // 范围外照常发放。
    let outside = budget
        .try_acquire(request("org-b", "conn-9"))
        .await
        .expect("acquire");
    assert!(outside.is_some(), "组织范围排空不波及别的组织");

    // 节点范围排空扣住一切（v1 许可不带 worker 归属 → 保守失败关闭）。
    budget
        .drain(DrainScope::Node(WorkerId::new("worker-1")))
        .await
        .expect("node drain");
    let node_wide = budget.try_acquire(request("org-b", "conn-10")).await;
    assert!(matches!(node_wide, Err(PortError::QuotaExceeded(_))));

    budget
        .release(&held, ReleaseOutcome::Consumed)
        .await
        .expect("release");
}

#[tokio::test]
async fn renew_node_lease_extends_and_unknown_leases_fail() {
    let budget = coordinator(4, 4);
    let lease = budget.register_node(WorkerId::new("worker-1"));
    let renewed = budget.renew_node_lease(&lease).await.expect("renew");
    assert_eq!(renewed.lease_id, lease.lease_id);
    assert_eq!(renewed.worker_id, lease.worker_id);

    let unknown = datazen::execution::application::budget::NodeLease {
        lease_id: datazen::shared::id::LeaseId::new("no-such-lease"),
        worker_id: WorkerId::new("worker-x"),
        expires_at: renewed.expires_at.clone(),
    };
    let error = budget
        .renew_node_lease(&unknown)
        .await
        .expect_err("unknown lease");
    assert!(
        matches!(error, PortError::NotFound(_)),
        "未知租约 = 失联，{error:?}"
    );
}

#[tokio::test]
async fn snapshot_reports_node_totals_under_the_reporting_scope() {
    let budget = coordinator(4, 4);
    let permit = budget
        .try_acquire(request("org-a", "conn-1"))
        .await
        .expect("acquire")
        .expect("granted");
    let snapshot = budget.snapshot().await.expect("snapshot");
    assert_eq!(
        snapshot.organization_id.as_str(),
        "org-node",
        "快照带构造时给定的报告标签"
    );
    assert_eq!(snapshot.in_use.get(), 1);
    assert_eq!(snapshot.granted_total.get(), 1);
    budget
        .release(&permit, ReleaseOutcome::Consumed)
        .await
        .expect("release");
    let after = budget.snapshot().await.expect("snapshot");
    assert_eq!(after.in_use.get(), 0);
    assert_eq!(after.granted_total.get(), 1, "granted_total 是累计发放量");
}
