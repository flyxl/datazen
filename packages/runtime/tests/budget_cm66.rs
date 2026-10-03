//! CM-66：**逻辑** session 与**物理**连接两套额度分开记账。
//!
//! 夹具（§9.5 CM-66 复现步骤）：单用户逻辑上限 20、已连接编辑器上限 2、
//! 单组织逻辑上限 30、物理连接总上限 4。
//! 步骤：开 20 个 `New` 再开第 21 个 → 并发连 3 个 → 关掉一个已连接的再试 → 另一个用户撞组织上限。
//!
//! 断言里的上限值一律从配置读回来，期望的计数一律是**精确值**（不是「≥」也不是「没变」）。

mod support;

use datazen_platform_api::id::{ConnectionId, DbSessionId, OrganizationId, PrincipalId};
use datazen_platform_api::ports::budget::{PhysicalOccupancy, ResourceClass};

use datazen_runtime::budget::ledger::SessionScope;
use datazen_runtime::budget::{BudgetConfig, BudgetLedger, DenialReason, PermitRecord, SlotKind};

use support::{claim, config, count_class, granted, ok};

const ORG: &str = "org-acme";
const CONN: &str = "conn-pg-main";

fn org() -> OrganizationId {
    OrganizationId::new(ORG)
}

fn conn() -> ConnectionId {
    ConnectionId::new(CONN)
}

/// §9.5 CM-66 的复现夹具。
fn cm66_config() -> BudgetConfig {
    config(8, [1, 1, 1, 0])
        .with_per_user_logical_sessions(20)
        .with_per_org_logical_sessions(30)
        .with_per_user_connected_editors(2)
        .with_physical_connections(4)
}

fn ledger() -> BudgetLedger {
    BudgetLedger::new(cm66_config())
}

/// 开 `count` 个逻辑 session，全是 `New`。
fn open_many(
    ledger: &mut BudgetLedger,
    organization_id: &OrganizationId,
    principal: &PrincipalId,
    connection_id: &ConnectionId,
    count: u32,
) -> Vec<DbSessionId> {
    (0..count)
        .map(|_| {
            ok(
                ledger.open_session(organization_id, principal, connection_id),
                "open_session",
            )
        })
        .collect()
}

fn denied_open(
    ledger: &mut BudgetLedger,
    organization_id: &OrganizationId,
    principal: &PrincipalId,
    connection_id: &ConnectionId,
) -> DenialReason {
    match ledger.open_session(organization_id, principal, connection_id) {
        Ok(ticket) => panic!("会话已满，却开出了 {ticket:?}"),
        Err(reason) => reason,
    }
}

fn denied_attach(ledger: &mut BudgetLedger, ticket: &DbSessionId) -> DenialReason {
    match ledger.attach_physical(ticket, PhysicalOccupancy::InUse) {
        Ok(()) => panic!("连接已满，却把 {ticket:?} 接上了物理连接"),
        Err(reason) => reason,
    }
}

/// 取一条 `PhysicalOccupancy`。
fn occupancy(ledger: &BudgetLedger, ticket: &DbSessionId) -> Option<PhysicalOccupancy> {
    match ledger.session(ticket) {
        Some(record) => record.occupancy,
        None => panic!("{ticket:?} 必须有会话记录"),
    }
}

// ---------------------------------------------------------------------------------------------
// New 只算逻辑，不算物理
// ---------------------------------------------------------------------------------------------

#[test]
fn cm66_new_sessions_count_logical_but_not_physical() {
    let config = cm66_config();
    let (org, conn) = (org(), conn());
    let user_a = PrincipalId::new("user-a");
    let mut ledger = ledger();
    ledger.ensure_service(&conn);

    let tickets = open_many(
        &mut ledger,
        &org,
        &user_a,
        &conn,
        config.per_user_logical_sessions,
    );

    // 20 个 `New`：逻辑计满，物理一个都不占。
    assert_eq!(
        ledger.user_logical(&user_a),
        config.per_user_logical_sessions
    );
    assert_eq!(ledger.org_logical(&org), config.per_user_logical_sessions);
    assert_eq!(
        ledger.physical(&conn),
        0,
        "`New` 没有 socket，不得占物理额度"
    );
    assert_eq!(
        ledger.user_connected(&conn, &user_a),
        0,
        "`New` 不算已连接编辑器"
    );
    for ticket in &tickets {
        assert_eq!(occupancy(&ledger, ticket), None, "`New` 的占用状态必须为空");
    }

    // 第 21 个：报单用户逻辑上限，且**顺带把物理/编辑器计数也验一遍没被动过**。
    match denied_open(&mut ledger, &org, &user_a, &conn) {
        DenialReason::SessionQuotaExceeded { scope, cap, used } => {
            assert_eq!(scope, SessionScope::PerUser);
            assert_eq!(cap, config.per_user_logical_sessions);
            assert_eq!(used, config.per_user_logical_sessions);
        }
        other => panic!("逻辑会话超限必须报 SessionQuotaExceeded(PerUser)，实际 {other:?}"),
    }
    assert_eq!(
        ledger.user_logical(&user_a),
        config.per_user_logical_sessions
    );
    assert_eq!(ledger.physical(&conn), 0);
    assert_eq!(ledger.org_logical(&org), config.per_user_logical_sessions);
}

#[test]
fn cm66_closing_one_logical_session_reopens_exactly_one_slot() {
    let config = cm66_config();
    let (org, conn) = (org(), conn());
    let user_a = PrincipalId::new("user-a");
    let mut ledger = ledger();
    ledger.ensure_service(&conn);

    let tickets = open_many(
        &mut ledger,
        &org,
        &user_a,
        &conn,
        config.per_user_logical_sessions,
    );
    ok(
        ledger.close_session(&tickets[0]),
        "close one logical session",
    );
    assert_eq!(
        ledger.user_logical(&user_a),
        config.per_user_logical_sessions - 1,
        "关一个必须只放出一个名额"
    );
    assert_eq!(
        ledger.org_logical(&org),
        config.per_user_logical_sessions - 1
    );

    // 补一个回到满；再多一个仍然超限。
    ok(
        ledger.open_session(&org, &user_a, &conn),
        "reopen exactly one slot",
    );
    assert_eq!(
        ledger.user_logical(&user_a),
        config.per_user_logical_sessions
    );
    assert!(
        matches!(
            denied_open(&mut ledger, &org, &user_a, &conn),
            DenialReason::SessionQuotaExceeded {
                scope: SessionScope::PerUser,
                used: 20,
                ..
            }
        ),
        "补回一个之后仍必须精确停在上限"
    );
}

// ---------------------------------------------------------------------------------------------
// 已连接编辑器（物理）上限：每用户 2
// ---------------------------------------------------------------------------------------------

#[test]
fn cm66_connected_editor_cap_is_two_per_user() {
    let config = cm66_config();
    let (org, conn) = (org(), conn());
    let user_a = PrincipalId::new("user-a");
    let mut ledger = ledger();
    ledger.ensure_service(&conn);

    let tickets = open_many(&mut ledger, &org, &user_a, &conn, 3);
    for ticket in &tickets[..config.per_user_connected_editors as usize] {
        ok(
            ledger.attach_physical(ticket, PhysicalOccupancy::InUse),
            "attach within the editor cap",
        );
    }
    assert_eq!(ledger.user_connected(&conn, &user_a), 2);
    assert_eq!(ledger.physical(&conn), 2);

    // 第 3 个编辑器：报单用户已连接上限，失败的那次不得留下任何物理占用。
    match denied_attach(&mut ledger, &tickets[2]) {
        DenialReason::ConnectedEditorsExhausted { principal, cap } => {
            assert_eq!(principal, user_a);
            assert_eq!(cap, config.per_user_connected_editors);
        }
        other => panic!("编辑器超限必须报 ConnectedEditorsExhausted，实际 {other:?}"),
    }
    assert_eq!(
        occupancy(&ledger, &tickets[2]),
        None,
        "被拒的连接不得留下占用状态"
    );
    assert_eq!(ledger.physical(&conn), 2, "被拒的连接不得占用物理额度");
    assert_eq!(ledger.user_connected(&conn, &user_a), 2);

    // 关掉一个已连接的编辑器 ⇒ 恰好放行一个名额，重试成功。
    ok(
        ledger.close_session(&tickets[0]),
        "close a connected editor",
    );
    assert_eq!(ledger.user_connected(&conn, &user_a), 1);
    assert_eq!(ledger.physical(&conn), 1);
    ok(
        ledger.attach_physical(&tickets[2], PhysicalOccupancy::InUse),
        "retry after freeing one editor slot",
    );
    assert_eq!(ledger.user_connected(&conn, &user_a), 2);
    assert_eq!(ledger.physical(&conn), 2);
}

// ---------------------------------------------------------------------------------------------
// 组织逻辑上限跨用户生效
// ---------------------------------------------------------------------------------------------

#[test]
fn cm66_organization_logical_cap_spans_users() {
    let config = cm66_config();
    let (org, conn) = (org(), conn());
    let user_a = PrincipalId::new("user-a");
    let user_b = PrincipalId::new("user-b");
    let mut ledger = ledger();
    ledger.ensure_service(&conn);

    // 用户 A 一个人就占满 20（还没到组织上限 30）。
    open_many(&mut ledger, &org, &user_a, &conn, 20);
    assert_eq!(ledger.user_logical(&user_a), 20);
    assert_eq!(ledger.org_logical(&org), 20);

    // 用户 B 还能开 10 个，正好顶到组织上限 30。
    open_many(&mut ledger, &org, &user_b, &conn, 10);
    assert_eq!(ledger.user_logical(&user_b), 10);
    assert_eq!(ledger.org_logical(&org), config.per_org_logical_sessions,);

    // B 的第 11 个撞的是**组织**上限，不是 B 的单用户上限。
    match denied_open(&mut ledger, &org, &user_b, &conn) {
        DenialReason::SessionQuotaExceeded { scope, cap, used } => {
            assert_eq!(scope, SessionScope::PerOrganization);
            assert_eq!(cap, config.per_org_logical_sessions);
            assert_eq!(used, config.per_org_logical_sessions);
        }
        other => panic!("组织逻辑超限必须报 SessionQuotaExceeded(PerOrganization)，实际 {other:?}"),
    }
    assert_eq!(ledger.org_logical(&org), config.per_org_logical_sessions);
    assert_eq!(
        ledger.user_logical(&user_b),
        10,
        "组织上限不得改写单用户计数"
    );

    // A 那边仍按单用户上限被拒（单用户先查）。
    assert!(
        matches!(
            denied_open(&mut ledger, &org, &user_a, &conn),
            DenialReason::SessionQuotaExceeded {
                scope: SessionScope::PerUser,
                ..
            }
        ),
        "单用户上限必须先于组织上限被判定"
    );
}

// ---------------------------------------------------------------------------------------------
// 物理连接额度：原子占用、失败归还
// ---------------------------------------------------------------------------------------------

#[test]
fn cm66_physical_connection_slots_are_reserved_atomically_and_reclaimed_on_failure() {
    let config = cm66_config();
    let (org, conn) = (org(), conn());
    let user_a = PrincipalId::new("user-a");
    let user_b = PrincipalId::new("user-b");
    let user_c = PrincipalId::new("user-c");
    let mut ledger = ledger();
    ledger.ensure_service(&conn);

    // 两个用户各开 2 个编辑器（每用户编辑器上限 2），正好填满物理总上限 4。
    let mut held: Vec<DbSessionId> = open_many(&mut ledger, &org, &user_a, &conn, 2);
    held.extend(open_many(&mut ledger, &org, &user_b, &conn, 2));
    for ticket in &held {
        ok(
            ledger.attach_physical(ticket, PhysicalOccupancy::InUse),
            "attach within the physical cap",
        );
    }
    assert_eq!(ledger.physical(&conn), config.service_physical_connections);

    // 第五个连接：整体失败。
    let fifth = ok(
        ledger.open_session(&org, &user_c, &conn),
        "open a session for user-c",
    );
    match denied_attach(&mut ledger, &fifth) {
        DenialReason::PhysicalExhausted { connection_id, cap } => {
            assert_eq!(connection_id, conn);
            assert_eq!(cap, config.service_physical_connections);
        }
        other => panic!("物理连接超限必须报 PhysicalExhausted，实际 {other:?}"),
    }
    assert_eq!(
        ledger.physical(&conn),
        config.service_physical_connections,
        "失败的连接不得占用物理额度"
    );
    assert_eq!(occupancy(&ledger, &fifth), None);
    assert_eq!(ledger.user_connected(&conn, &user_c), 0);

    // 关掉一个已连接的 ⇒ 恰好腾出一个，重试成功并精确填满。
    ok(ledger.close_session(&held[0]), "close one connected editor");
    assert_eq!(
        ledger.physical(&conn),
        config.service_physical_connections - 1
    );
    ok(
        ledger.attach_physical(&fifth, PhysicalOccupancy::InUse),
        "retry after freeing one physical slot",
    );
    assert_eq!(ledger.physical(&conn), config.service_physical_connections);
    assert_eq!(
        ledger.user_connected(&conn, &user_c),
        1,
        "腾出的名额必须精确转交给新用户"
    );
}

#[test]
fn cm66_returning_to_the_idle_pool_does_not_release_physical_budget() {
    let config = cm66_config();
    let (org, conn) = (org(), conn());
    let user_a = PrincipalId::new("user-a");
    let user_b = PrincipalId::new("user-b");
    let mut ledger = ledger();
    ledger.ensure_service(&conn);

    let mut held: Vec<DbSessionId> = open_many(&mut ledger, &org, &user_a, &conn, 2);
    held.extend(open_many(&mut ledger, &org, &user_b, &conn, 2));
    for ticket in &held {
        ok(
            ledger.attach_physical(ticket, PhysicalOccupancy::InUse),
            "attach",
        );
    }
    assert_eq!(ledger.physical(&conn), config.service_physical_connections);

    // 回落空闲池：socket 还活着，**不释放**物理额度（§9.3）。
    ok(
        ledger.transition(&held[0], PhysicalOccupancy::IdlePooled),
        "transition to idle pool",
    );
    assert_eq!(
        occupancy(&ledger, &held[0]),
        Some(PhysicalOccupancy::IdlePooled)
    );
    assert_eq!(
        ledger.physical(&conn),
        config.service_physical_connections,
        "回落到空闲池不得释放物理额度"
    );

    // 额度仍然满：新来的连接还是撞 PhysicalExhausted。
    let extra = ok(
        ledger.open_session(&org, &PrincipalId::new("user-c"), &conn),
        "open a session for user-c",
    );
    assert!(
        matches!(
            denied_attach(&mut ledger, &extra),
            DenialReason::PhysicalExhausted { .. }
        ),
        "空闲池里的 socket 仍然占着预算"
    );
    assert_eq!(ledger.physical(&conn), config.service_physical_connections);

    // 只有真正 close 才释放，且只释放一个。
    ok(ledger.close_session(&held[0]), "close the pooled session");
    assert_eq!(
        ledger.physical(&conn),
        config.service_physical_connections - 1,
        "close 只释放这一个物理额度"
    );
    assert_eq!(ledger.user_connected(&conn, &user_a), 1);
}

// ---------------------------------------------------------------------------------------------
// 快照计数与逻辑/物理分离
// ---------------------------------------------------------------------------------------------

#[test]
fn cm66_snapshot_counts_permits_not_logical_sessions() {
    let config = cm66_config();
    let (org, conn) = (org(), conn());
    let user_a = PrincipalId::new("user-a");
    let mut ledger = ledger();
    ledger.ensure_service(&conn);

    // 20 个逻辑 session 在手，但一个 permit 都没有。
    open_many(
        &mut ledger,
        &org,
        &user_a,
        &conn,
        config.per_user_logical_sessions,
    );
    let empty = ledger.snapshot(&org);
    assert_eq!(empty.in_use.get(), 0, "逻辑 session 不得被算进在手 permit");

    let records: Vec<PermitRecord> = (0..2)
        .map(|_| {
            granted(ledger.try_admit(&claim(&org, &conn, "user-a", ResourceClass::Interactive), 0))
        })
        .collect();
    assert_eq!(count_class(&records, ResourceClass::Interactive), 2);
    assert_eq!(records[0].slot, SlotKind::Reserved);
    assert_eq!(records[1].slot, SlotKind::Shared);

    let busy = ledger.snapshot(&org);
    assert_eq!(
        busy.in_use.get(),
        2,
        "在手 permit 数必须与逻辑 session 数无关"
    );
    assert_eq!(
        ledger.user_logical(&user_a),
        config.per_user_logical_sessions,
        "发放 permit 不得改写逻辑计数"
    );

    // 关掉一个逻辑 session 也不影响在手 permit（两者是独立账本）。
    let ticket = ok(
        ledger.open_session(&org, &PrincipalId::new("user-c"), &conn),
        "open one more for user-c",
    );
    ok(ledger.close_session(&ticket), "close it again");
    assert_eq!(ledger.snapshot(&org).in_use.get(), 2);
    assert_eq!(ledger.permits().count(), 2);
}
