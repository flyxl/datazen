//! §6.5 句柄登记的对称性：**登记进来的必须摘得掉，摘不动的必须挡住归还**。
//!
//! Task A 的验收点之一是「登记与释放对称，异常路径同样可释放」。句柄登记表就是
//! 「可释放」这三个字最容易漏掉的地方：登记是加法，摘除也必须是加法的另一半，
//! 而且关不掉句柄时归还必须**失败**，不能悄悄放行。
//!
//! 这些用例把登记表当成一台**状态机 + 一个拒绝闸**来测：正常路径（登记 → 关闭 →
//! 摘除 → 归池）、每一条非法登记/摘除、句柄关不掉时的完整回滚与可重试性，以及
//! 「句柄关闭必须排在物理复位/关闭之前」的顺序保证。

mod resource_support;
mod support;

use std::sync::Arc;

use datazen_modules::execution::runtime::connection::{HandleId, SessionHandleRef};
use datazen_modules::execution::runtime::resource::{
    CleanupDisposition, DriverCleanVerdict, LeaseState, NoHandleCloser, ResourceError,
    NO_HANDLE_CLOSER,
};

use resource_support::{
    clear_host, counters, fixed_request, fixture, handle, main_request, ScriptedTransport,
};
use support::ok;

/// 什么都关得掉的 closer：这些用例只关心登记簿，不关心驱动的复位裁决。
fn accept_all() -> impl FnMut(&SessionHandleRef) -> Result<(), ResourceError> {
    |_handle| Ok(())
}

/// 记录每个句柄被关闭的那一刻，物理层已经做过多少次 reset / close。
///
/// 顺序是这里唯一要证的命题，计次查不出来 —— 只有「关句柄的那一刻物理还没动过」
/// 才说明关句柄排在物理释放之前（§6.5 的前置条件）。
#[derive(Default)]
struct ClearTrace {
    seen: Vec<(HandleId, usize, usize)>,
}

impl ClearTrace {
    fn observe(
        &mut self,
        handle_id: HandleId,
        transport: &ScriptedTransport,
    ) -> Result<(), ResourceError> {
        self.seen.push((
            handle_id,
            transport.count("reset"),
            transport.count("close"),
        ));
        Ok(())
    }

    fn ids(&self) -> Vec<String> {
        self.seen
            .iter()
            .map(|(id, _, _)| id.as_str().to_owned())
            .collect()
    }
}

// ===========================================================================
// 1. 对称路径：登记 → 关闭 → 摘除 → 归池
// ===========================================================================

#[test]
fn a_registered_handle_is_closed_and_deregistered_before_the_lease_may_park() {
    let mut fixture = fixture();
    let record = ok(fixture.manager.acquire(&main_request()), "acquire");
    ok(
        fixture
            .manager
            .register_handle(&record.lease_id, handle("h-1", &record.resource_id)),
        "register h-1",
    );
    assert_eq!(fixture.manager.handles().len(), 1);

    let released = ok(
        fixture.manager.release(
            &record.lease_id,
            &clear_host(),
            &DriverCleanVerdict::reported_clean(),
            &mut accept_all(),
        ),
        "a closable handle must not block the return",
    );

    assert_eq!(
        released.handles.closed_handles, 1,
        "the handle was closed on the original resource"
    );
    assert_eq!(released.handles.deregistered.len(), 1);
    assert_eq!(released.handles.deregistered[0].as_str(), "h-1");
    assert!(
        released.handles.is_settled(),
        "closing without deregistering would leave a ghost handle in the registry"
    );
    assert!(released.handles.residual_dropped.is_empty());
    assert!(
        released.pool.park,
        "and once the registry is clean the lease may go back to the idle pool"
    );
    assert_eq!(
        fixture.manager.handles().len(),
        0,
        "registration and deregistration are two halves of one operation"
    );
    assert!(!fixture
        .manager
        .handles()
        .is_registered(&record.lease_id, &HandleId::new("h-1")));
    // 归池不核销额度：空闲连接仍然占着物理槽位（§9.3）。
    assert!(!released.budget_settled);
    assert_eq!(counters(&fixture.budget), (1, 1, 0, 0));
}

#[test]
fn a_handle_that_cannot_be_closed_blocks_the_return_and_stays_retryable() {
    let mut fixture = fixture();
    let record = ok(fixture.manager.acquire(&main_request()), "acquire");
    let handle_ref = handle("h-1", &record.resource_id);
    ok(
        fixture
            .manager
            .register_handle(&record.lease_id, handle_ref.clone()),
        "register h-1",
    );

    // `NoHandleCloser` 什么都关不掉：归还必须失败，租约必须一动不动。
    let error = match fixture.manager.release(
        &record.lease_id,
        &clear_host(),
        &DriverCleanVerdict::reported_clean(),
        &mut NoHandleCloser,
    ) {
        Ok(_) => panic!("a handle that cannot be closed must block the return"),
        Err(error) => error,
    };
    match &error {
        ResourceError::TransportRefused(reason) => {
            assert_eq!(*reason, NO_HANDLE_CLOSER)
        }
        other => panic!("expected a refused handle close, got {other:?}"),
    }
    assert_eq!(error.reason(), "resourceTransportRefused");
    assert_eq!(
        fixture
            .manager
            .resources()
            .lease(&record.lease_id)
            .map(|lease| lease.state),
        Some(LeaseState::InUse),
        "the lease keeps its state so the caller can retry"
    );
    assert!(
        fixture
            .manager
            .handles()
            .is_registered(&record.lease_id, &handle_ref.handle_id),
        "the registration survives the refusal — that is what makes a retry possible"
    );
    assert_eq!(fixture.manager.resources().idle_lease_count(), 0);
    assert_eq!(counters(&fixture.budget), (1, 1, 0, 0));

    // 出口：换一个关得掉的 closer，同一条租约原样走完 —— 被拒不是死锁。
    let released = ok(
        fixture.manager.release(
            &record.lease_id,
            &clear_host(),
            &DriverCleanVerdict::reported_clean(),
            &mut accept_all(),
        ),
        "retry with a closer that works",
    );
    assert!(released.handles.is_settled());
    assert!(released.pool.park);
    assert_eq!(fixture.manager.handles().len(), 0);
}

#[test]
fn handle_closes_happen_before_any_physical_reset_or_close() {
    let mut fixture = fixture();
    // 固定会话一定走物理关闭，所以「物理动作还没发生」是可观测的强断言，
    // 而不是一条恒真的 `count == 0`。
    let record = ok(
        fixture.manager.acquire(&fixed_request()),
        "acquire fixed session",
    );
    ok(
        fixture
            .manager
            .register_handle(&record.lease_id, handle("h-1", &record.resource_id)),
        "register h-1",
    );
    ok(
        fixture
            .manager
            .register_handle(&record.lease_id, handle("h-2", &record.resource_id)),
        "register h-2",
    );

    let transport: Arc<ScriptedTransport> = Arc::clone(&fixture.transport);
    let mut trace = ClearTrace::default();
    let released = ok(
        fixture.manager.release(
            &record.lease_id,
            &clear_host(),
            &DriverCleanVerdict::reported_clean(),
            &mut |handle_ref: &SessionHandleRef| {
                trace.observe(handle_ref.handle_id.clone(), &transport)
            },
        ),
        "release with an observable closer",
    );
    assert_eq!(released.cleanup.disposition, CleanupDisposition::Closed);
    assert_eq!(
        fixture.transport.count("close"),
        1,
        "the physical close really did happen — otherwise the order proof is vacuous"
    );

    assert_eq!(
        trace.ids(),
        vec!["h-1".to_owned(), "h-2".to_owned()],
        "handles are closed in registration order"
    );
    for (handle_id, resets, closes) in &trace.seen {
        assert_eq!(
            (*resets, *closes),
            (0, 0),
            "handle {handle_id} was closed after a physical reset/close had already run"
        );
    }
}

// ===========================================================================
// 2. 登记与摘除的拒绝闸
// ===========================================================================

#[test]
fn registering_the_same_handle_twice_is_refused() {
    let mut fixture = fixture();
    let record = ok(fixture.manager.acquire(&main_request()), "acquire");
    ok(
        fixture
            .manager
            .register_handle(&record.lease_id, handle("h-1", &record.resource_id)),
        "first registration",
    );

    let error = match fixture
        .manager
        .register_handle(&record.lease_id, handle("h-1", &record.resource_id))
    {
        Ok(()) => panic!("two owners of one handle would leave the close order undefined"),
        Err(error) => error,
    };
    match &error {
        ResourceError::HandleAlreadyRegistered(handle_id) => assert_eq!(*handle_id, "h-1"),
        other => panic!("expected a duplicate registration, got {other:?}"),
    }
    assert_eq!(
        fixture.manager.handles().len(),
        1,
        "a refused registration does not add a second entry"
    );
}

#[test]
fn deregistering_a_handle_that_was_never_registered_is_refused() {
    let mut fixture = fixture();
    let record = ok(fixture.manager.acquire(&main_request()), "acquire");
    ok(
        fixture
            .manager
            .register_handle(&record.lease_id, handle("h-1", &record.resource_id)),
        "register h-1",
    );

    let error = match fixture
        .manager
        .deregister_handle(&record.lease_id, &HandleId::new("h-404"))
    {
        Ok(_) => panic!("a silent success would hide 'this handle is still on the session'"),
        Err(error) => error,
    };
    match &error {
        ResourceError::HandleNotRegistered(handle_id) => assert_eq!(*handle_id, "h-404"),
        other => panic!("expected an unknown handle, got {other:?}"),
    }
    assert_eq!(
        fixture.manager.handles().len(),
        1,
        "h-1 is untouched by the refused deregistration"
    );

    // 出口：真的摘一次，登记簿立刻空。
    let removed = ok(
        fixture
            .manager
            .deregister_handle(&record.lease_id, &HandleId::new("h-1")),
        "deregister the real handle",
    );
    assert_eq!(removed.handle_id.as_str(), "h-1");
    assert!(fixture.manager.handles().is_empty());
}

#[test]
fn a_handle_may_only_be_registered_against_the_resource_that_issued_it() {
    let mut fixture = fixture();
    let request = main_request();
    let first = ok(fixture.manager.acquire(&request), "first acquire");
    let second = ok(fixture.manager.acquire(&request), "second acquire");
    assert_ne!(first.resource_id, second.resource_id);

    let error = match fixture
        .manager
        .register_handle(&first.lease_id, handle("h-1", &second.resource_id))
    {
        Ok(()) => panic!("a handle borrowed from another resource must not be accepted"),
        Err(error) => error,
    };
    match &error {
        ResourceError::HandleResourceMismatch(handle_id) => assert_eq!(*handle_id, "h-1"),
        other => panic!("expected a resource mismatch, got {other:?}"),
    }
    assert!(fixture.manager.handles().is_empty());
}

#[test]
fn a_lease_that_cannot_be_issued_refuses_new_handle_registrations() {
    let mut fixture = fixture();
    let record = ok(
        fixture.manager.acquire(&fixed_request()),
        "acquire fixed session",
    );
    fixture.transport.refuse_close();
    ok(
        fixture.manager.release(
            &record.lease_id,
            &clear_host(),
            &DriverCleanVerdict::reported_clean(),
            &mut NoHandleCloser,
        ),
        "quarantine the lease",
    );
    assert_eq!(
        fixture
            .manager
            .resources()
            .lease(&record.lease_id)
            .map(|lease| lease.state),
        Some(LeaseState::Quarantined)
    );

    // §6.2：隔离态不可再获取；句柄登记是获取的前置动作，所以同样必须被拒。
    let error = match fixture
        .manager
        .register_handle(&record.lease_id, handle("h-1", &record.resource_id))
    {
        Ok(()) => panic!("a quarantined lease must not accept new handles"),
        Err(error) => error,
    };
    match error {
        ResourceError::IllegalTransition { from, to } => {
            assert_eq!(from, LeaseState::Quarantined);
            assert_eq!(to, LeaseState::InUse);
        }
        other => panic!("expected an illegal transition, got {other:?}"),
    }

    // 退休之后租约根本不存在了 ⇒ 未知名，而不是非法迁移。
    fixture.transport.accept_close();
    ok(fixture.manager.retire(&record.lease_id), "retire");
    match fixture
        .manager
        .register_handle(&record.lease_id, handle("h-2", &record.resource_id))
    {
        Ok(()) => panic!("a retired lease must not accept handles"),
        Err(ResourceError::UnknownResource(lease_id)) => {
            assert_eq!(lease_id, record.lease_id.as_str())
        }
        Err(other) => panic!("expected an unknown resource, got {other:?}"),
    }
}

// ===========================================================================
// 3. 强制退休：残留登记被丢弃，绝不复用旧句柄
// ===========================================================================

#[test]
fn retiring_a_lease_drops_residual_registrations_instead_of_reusing_them() {
    let mut fixture = fixture();
    let record = ok(
        fixture.manager.acquire(&fixed_request()),
        "acquire fixed session",
    );
    ok(
        fixture
            .manager
            .register_handle(&record.lease_id, handle("h-1", &record.resource_id)),
        "register h-1",
    );

    let report = ok(fixture.manager.retire(&record.lease_id), "force retire");

    assert_eq!(report.disposition, CleanupDisposition::Closed);
    assert_eq!(
        fixture.manager.handles().len(),
        0,
        "§6.5: an old handle is never reused, so the registration is dropped"
    );
    assert_eq!(
        fixture.manager.resources().lease_count(),
        0,
        "and the lease itself is gone"
    );
    assert_eq!(counters(&fixture.budget), (0, 1, 1, 0));
}
