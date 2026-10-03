//! CM-65 门禁 · 保留额度隔离。
//!
//! §9.5 的硬约束：**保留额度不可借用**（v1 `may_borrow_reserved()` 恒为 `false`），
//! 控制类额度只服务于取消/健康恢复、绝不进共享池；每个类先用光自己的保留槽，
//! 才允许去动共享池。这里是那个「不许借」的证据。

mod support;

use datazen_platform_api::ports::budget::ResourceClass;
use datazen_runtime::budget::{BudgetLedger, DenialReason, SlotKind};
use support::{busy, claim, config, conn, granted, org};

// ---------------------------------------------------------------------------------------------
// 保留额：控制类与 interactive 的保留槽都不许被侵占
// ---------------------------------------------------------------------------------------------

#[test]
fn cm65_control_reservation_is_never_borrowed_from_the_shared_pool() {
    let config = config(8, [1, 1, 1, 0]);
    let (org, conn) = (org(), conn());
    let mut ledger = BudgetLedger::new(config.clone());
    ledger.ensure_service(&conn);

    // 第一个控制请求走自己的保留额。
    let first = granted(ledger.try_admit(&claim(&org, &conn, "user-a", ResourceClass::Control), 0));
    assert_eq!(first.slot, SlotKind::Reserved);
    assert_eq!(
        ledger.shared_used(&conn),
        0,
        "控制类占的是保留槽，不得抬高共享池水位"
    );

    // 第二个控制请求没有保留额可拿：v1 保留**不可被借用**。
    match busy(ledger.try_admit(&claim(&org, &conn, "user-a", ResourceClass::Control), 0)) {
        DenialReason::Exhausted {
            connection_id,
            class,
            capacity,
        } => {
            assert_eq!(connection_id, conn);
            assert_eq!(class, ResourceClass::Control);
            assert_eq!(capacity, config.capacity_of(ResourceClass::Control));
        }
        other => panic!("控制类超额必须报 Exhausted，实际 {other:?}"),
    }

    // 共享池一个名额都没被控制类挪用。
    assert_eq!(ledger.shared_used(&conn), 0);
    assert_eq!(ledger.shared_headroom(&conn), config.service_quota.shared);
    assert_eq!(ledger.granted_total(), 1);
}

#[test]
fn cm65_interactive_uses_its_own_reservation_before_touching_shared() {
    let config = config(8, [1, 1, 1, 0]);
    let (org, conn) = (org(), conn());
    let mut ledger = BudgetLedger::new(config.clone());
    ledger.ensure_service(&conn);

    let first =
        granted(ledger.try_admit(&claim(&org, &conn, "user-a", ResourceClass::Interactive), 0));
    assert_eq!(first.slot, SlotKind::Reserved);
    assert_eq!(ledger.shared_used(&conn), 0);

    let second =
        granted(ledger.try_admit(&claim(&org, &conn, "user-a", ResourceClass::Interactive), 0));
    assert_eq!(second.slot, SlotKind::Shared);
    assert_eq!(ledger.shared_used(&conn), 1);

    // 对照组：共享池还有 4 个空位时，同一条请求**能**拿到名额——下面那次拒绝不是
    // 「实现没写」，而是共享池真的满了。
    let warm =
        granted(ledger.try_admit(&claim(&org, &conn, "user-a", ResourceClass::Interactive), 0));
    assert_eq!(warm.slot, SlotKind::Shared);
    assert_eq!(ledger.shared_used(&conn), 2);
    assert_eq!(ledger.granted_total(), 3);

    // 把剩余 3 个共享槽占满（job 没有保留额，只能吃共享）。
    for _ in 0..3 {
        granted(ledger.try_admit(&claim(&org, &conn, "user-c", ResourceClass::Job), 0));
    }
    assert_eq!(ledger.shared_used(&conn), config.service_quota.shared);

    // 保留额用完、共享也用满 ⇒ Busy，且共享水位**精确**停在填满时的值。
    match busy(ledger.try_admit(&claim(&org, &conn, "user-b", ResourceClass::Interactive), 0)) {
        DenialReason::Exhausted {
            class, capacity, ..
        } => {
            assert_eq!(class, ResourceClass::Interactive);
            assert_eq!(
                capacity,
                config.capacity_of(ResourceClass::Interactive),
                "拒绝时必须按本类自己的容量报错，而不是把共享池的总数塞给调用方"
            );
        }
        other => panic!("interactive 超额必须报 Exhausted，实际 {other:?}"),
    }
    assert_eq!(ledger.shared_used(&conn), config.service_quota.shared);
    assert_eq!(ledger.granted_total(), 6);
}
