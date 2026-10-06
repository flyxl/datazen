//! **第二格**：`CleanupDisposition::Quarantined` 下隧道引用与物理预算的
//! **归属配对**（登记表第 **(3)** 格）。
//!
//! # 判据是哪一个布尔
//!
//! 判据就是 `resource/cleanup.rs` 里那一个布尔：
//!
//! ```text
//! CleanupDisposition::releases_physical_budget() == (disposition == Closed)
//! ```
//!
//! 它在失败矩阵里被反复要求（「不能确认则隔离并核验」、恢复决策表的 `cleanup 未确认`
//! 行），而 `cleanup.rs` 只兑现了**物理预算那一半**：隔离时预算
//! 继续占着，可隧道引用的归属没人管。本文件用**同一个布尔**驱动两侧，把配对钉死：
//!
//! | 处置          | 物理预算 | 隧道引用 |
//! |---------------|----------|----------|
//! | `ReturnedToPool` | 仍占用 | 保留 |
//! | `Quarantined`    | 仍占用 | 保留 |
//! | `Closed`         | 此刻核销 | 归还 |
//!
//! 归零**只能**经由 `TunnelLedger::return_resource` → 私有 `drain`，资源侧不存第二个计数
//! （见 `../tunnel_wiring_contract.rs::last_cell_refuses_to_keep_a_second_tally`）。

mod tunnel_arch_support;

use tunnel_arch_support::{request, tunnel_spec, wired};

use datazen_runtime::connection::{
    Counter, ExecutionId, HandleId, HandleKind, ResourceId, SessionHandleRef,
};
use datazen_runtime::resource::{
    CleanupDisposition, CleanupReport, DriverCleanVerdict, HostConditionSnapshot, TunnelDisposition,
};

/// **第三格的判据，压成一行**：报告自带的两半必须逐字相等。
///
/// 这一句是本文件的全部意义 —— `CleanupReport` 里同时有物理预算那半和隧道引用那半，
/// 而它们来自同一个布尔 `releases_physical_budget()`。任何一侧单独漂移（例如有人
/// 给 `Quarantined` 也归还引用）都会让这一行变红。
fn assert_paired(report: &CleanupReport) {
    assert_eq!(
        report.tunnel.pairs_with_budget_release(),
        report.physical_budget_released,
        "隧道引用与物理预算必须同拍：预算不核销 ⇒ 引用保留；预算核销 ⇒ 引用归还"
    );
}

/// 预算核销 ⇒ 引用必须落到 0，`Released` 那半不能是假话。
fn assert_released(report: &CleanupReport) {
    assert_paired(report);
    assert!(
        matches!(
            report.tunnel,
            TunnelDisposition::Released {
                tunnel_closed: true
            }
        ),
        "预算已经核销，隧道就必须真的关掉；报告却记着 {:?}",
        report.tunnel
    );
}

/// 预算未核销 ⇒ 引用必须还在占用，处置是 `Retained` 而不是 `NoReference`
/// ——`NoReference` 说的是「这条连接本来就没走隧道」，那是直连租约的事。
fn assert_still_held(report: &CleanupReport) {
    assert_paired(report);
    assert!(
        matches!(report.tunnel, TunnelDisposition::Retained),
        "预算还占着，隧道引用就还占着；报告却记着 {:?}",
        report.tunnel
    );
}

/// 四项宿主条件全清 + 驱动 Clean ⇒ 短操作租约回池，物理预算**不**释放。
fn clean_host() -> HostConditionSnapshot {
    HostConditionSnapshot::default()
}

/// 活动执行未结束 ⇒ 隔离。
///
/// `cleanup.rs` 的判据是 `record.active_execution.is_some()`，不是快照字段本身：
/// 快照的 `active_executions` 只负责**拒绝复用**，真正决定「不许静默关闭」的是租约
/// 上挂着的那次执行。两者都要真，缺一个就退化成普通 `Closed`。
fn quarantining_host(execution_id: &ExecutionId) -> HostConditionSnapshot {
    HostConditionSnapshot {
        active_executions: vec![execution_id.clone()],
        ..HostConditionSnapshot::default()
    }
}

// ============================================================ 回池：预算不动，引用也不动

#[test]
fn returned_to_pool_keeps_the_budget_and_keeps_the_tunnel_reference() {
    let (mut manager, _t, tunnel, connection_id) = wired();
    let spec = tunnel_spec();
    let lease = manager
        .acquire(&request(&connection_id, "appdb").via_tunnel(spec.clone()))
        .expect("the tunnel stage succeeded");

    let report = manager
        .release(
            &lease.lease_id,
            clean_host(),
            DriverCleanVerdict::reported_clean(),
        )
        .expect("a clean lease returns to the idle pool");

    assert_eq!(report.disposition, CleanupDisposition::ReturnedToPool);
    assert_still_held(&report);
    assert!(
        !report.physical_budget_released,
        "a pooled connection still occupies its physical budget"
    );
    assert_eq!(
        manager.tunnel_refs(&spec),
        Some(1),
        "归属配对：预算没核销，隧道引用就一并留着"
    );
    assert_eq!(tunnel.closed(), 0);
}

// ============================================================ 隔离：预算不动，引用也不动

#[test]
fn quarantined_keeps_the_budget_and_keeps_the_tunnel_reference() {
    let (mut manager, _t, tunnel, connection_id) = wired();
    let spec = tunnel_spec();
    let lease = manager
        .acquire(&request(&connection_id, "appdb").via_tunnel(spec.clone()))
        .expect("the tunnel stage succeeded");
    let execution = ExecutionId::new("exec-arch-1");
    manager
        .begin_execution(&lease.lease_id, execution.clone())
        .expect("the execution is still running");

    let report = manager
        .release(
            &lease.lease_id,
            quarantining_host(&execution),
            DriverCleanVerdict::reported_clean(),
        )
        .expect("a lease with an active execution is quarantined, not refused");

    assert_eq!(report.disposition, CleanupDisposition::Quarantined);
    assert_still_held(&report);
    assert!(
        !report.physical_budget_released,
        "an unconfirmed close keeps the budget occupied (§9.3 / §10.1.1)"
    );
    assert_eq!(
        manager.tunnel_refs(&spec),
        Some(1),
        "归属配对的核心一格：隔离中的连接仍占着隧道，引用不得先还"
    );
    assert_eq!(
        manager.live_tunnels(),
        1,
        "the tunnel is still live because a live connection may still use it"
    );
    assert_eq!(tunnel.closed(), 0);
}

/// 隔离不是终点：运维核验后一次**被确认的**强制关闭，预算核销、引用同拍归还，
/// 而且只归还一次（CM-28「重复响应一致」）。
#[test]
fn quarantined_released_by_a_later_confirmed_close_gives_the_reference_back_exactly_once() {
    let (mut manager, _t, tunnel, connection_id) = wired();
    let spec = tunnel_spec();
    let lease = manager
        .acquire(&request(&connection_id, "appdb").via_tunnel(spec.clone()))
        .expect("the tunnel stage succeeded");
    let execution = ExecutionId::new("exec-arch-2");
    manager
        .begin_execution(&lease.lease_id, execution.clone())
        .expect("the execution is still running");
    manager
        .release(
            &lease.lease_id,
            quarantining_host(&execution),
            DriverCleanVerdict::reported_clean(),
        )
        .expect("quarantine is a disposition, not an error");
    assert_eq!(manager.tunnel_refs(&spec), Some(1));

    // 运维核验后强制关闭：这次关闭被确认。
    let report = manager
        .retire(&lease.lease_id)
        .expect("the confirmed close releases the budget");
    assert_released(&report);

    assert_eq!(
        manager.tunnel_refs(&spec),
        None,
        "确认关闭 ⇒ 预算核销 ⇒ 引用同刻归还"
    );
    assert_eq!(
        (manager.tunnel_close_calls(), tunnel.closed()),
        (1, 1),
        "the reference reaching zero is the **only** thing that closes the tunnel"
    );

    // 再关一次不重复释放（CM-28「重复响应一致」）。
    assert!(
        manager.retire(&lease.lease_id).is_err(),
        "the lease is gone, so a second retire has nothing to close"
    );
    assert_eq!(manager.tunnel_close_calls(), 1);
}

/// 配对的对偶：关闭**没能确认**时，两侧一起不动；排障后重试成功才一起动。
#[test]
fn retire_with_an_unconfirmed_close_keeps_the_budget_and_keeps_the_tunnel_reference() {
    let (mut manager, transport, tunnel, connection_id) = wired();
    let spec = tunnel_spec();
    let lease = manager
        .acquire(&request(&connection_id, "appdb").via_tunnel(spec.clone()))
        .expect("the tunnel stage succeeded");
    transport.fail_close();

    assert!(
        manager.retire(&lease.lease_id).is_err(),
        "an unconfirmed close is reported, never swallowed"
    );
    assert_eq!(
        manager.tunnel_refs(&spec),
        Some(1),
        "the close was not confirmed, so the reference stays paired with the held budget"
    );
    assert_eq!(tunnel.closed(), 0);

    // 排障之后传输端口恢复，再关一次：这次确认 ⇒ 两侧一起释放。
    transport.succeed_close();
    let report = manager
        .retire(&lease.lease_id)
        .expect("the retry confirms the close");
    assert_released(&report);
    assert_eq!(manager.tunnel_refs(&spec), None);
    assert_eq!(manager.tunnel_close_calls(), 1);
}

/// CM-73 的复位失败把处置从 `Closed` 升级成 `Quarantined`；此时引用**必须**跟着留。
///
/// 这一格最容易错：判据取的是**升级之后**的 `plan.disposition`。若结算读的是
/// `CleanupPlan::of` 的初值（`Closed`），就会出现「预算没核销、引用先还了」的错拍。
#[test]
fn a_failed_session_reset_escalates_to_quarantine_and_keeps_the_tunnel_reference() {
    let (mut manager, transport, tunnel, connection_id) = wired();
    let spec = tunnel_spec();
    let lease = manager
        .acquire(&request(&connection_id, "appdb").via_tunnel(spec.clone()))
        .expect("the tunnel stage succeeded");
    // 未释放的会话级句柄 ⇒ 关闭前必须先在原资源上复位；驱动报告 Clean、宿主无其他
    // 阻碍，所以 `CleanupPlan::of` 的初值是 `Closed`。
    let handle = SessionHandleRef::new(
        HandleId::new("handle-arch-1"),
        HandleKind::Transaction,
        ResourceId::new("res-handle-1"),
        Counter::new(1),
    );
    let host = HostConditionSnapshot::default().with_unreleased_handle(handle);
    transport.fail_reset();

    let report = manager
        .release(&lease.lease_id, host, DriverCleanVerdict::reported_clean())
        .expect("an unconfirmed reset is a disposition, not an error");

    assert_eq!(
        report.disposition,
        CleanupDisposition::Quarantined,
        "复位结果不明 ⇒ 处置从 Closed 升级成隔离"
    );
    assert!(!report.physical_budget_released, "预算不核销");
    assert_still_held(&report);
    assert_eq!(
        manager.tunnel_refs(&spec),
        Some(1),
        "预算没核销，隧道引用就不得先还 —— 配对判据读的是升级后的处置"
    );
    assert_eq!(manager.live_tunnels(), 1);
    assert_eq!(tunnel.closed(), 0);
}

/// `release` 自身走 `Closed` 分支时的配对：会话复位与物理关闭**都**被确认 ⇒
/// 预算核销、引用同拍归还。
///
/// 这一格是配对表的第三行在 `release` 路径上的**唯一**入口：`release` 要拿到
/// `Closed`，宿主条件必须既挡复用（未释放句柄）又不触发隔离（无执行中、无禁用归属）。
/// 没有它，上面两个隔离用例都只验「保留」这一侧，删除 `release` 路径上的结算
/// 不会有任何用例转红 —— 这一格就是补这个洞的。
#[test]
fn a_confirmed_reset_and_close_in_one_release_gives_the_reference_back_with_the_budget() {
    let (mut manager, transport, tunnel, connection_id) = wired();
    let spec = tunnel_spec();
    let lease = manager
        .acquire(&request(&connection_id, "appdb").via_tunnel(spec.clone()))
        .expect("the tunnel stage succeeded");
    let handle = SessionHandleRef::new(
        HandleId::new("handle-arch-2"),
        HandleKind::Cursor,
        ResourceId::new("res-handle-2"),
        Counter::new(1),
    );
    // 复位与关闭都不注入失败 ⇒ 两个确认都拿得到。
    let host = HostConditionSnapshot::default().with_unreleased_handle(handle);

    let report = manager
        .release(&lease.lease_id, host, DriverCleanVerdict::reported_clean())
        .expect("a confirmed reset and confirmed close release the budget");

    assert_eq!(
        report.disposition,
        CleanupDisposition::Closed,
        "宿主条件挡复用、又不触发隔离 ⇒ 处置是关闭"
    );
    assert!(
        report.physical_budget_released,
        "确认关闭 ⇒ 物理预算此刻核销"
    );
    assert_released(&report);
    assert!(
        report.session_reset_performed,
        "CM-73：复位排在关闭之前，这是可审计的顺序证据"
    );
    assert_eq!(
        manager.tunnel_refs(&spec),
        None,
        "配对表第三行：预算核销了，引用就同刻归还"
    );
    assert_eq!(
        (manager.tunnel_close_calls(), tunnel.closed()),
        (1, 1),
        "引用归零是关闭隧道的唯一事由，且只发生一次"
    );
    assert_eq!((transport.opened(), transport.closed()), (1, 1));
    assert_eq!(manager.lease_count(), 0, "预算核销之后租约行才被抹掉");
}
