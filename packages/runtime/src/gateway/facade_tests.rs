//! `ExecutionGateway` 门面的受理 / 下发 / 权限 / 计时单测。
//!
//! 单独成文件的原因：`mod.rs` 本身已接近单文件上限（§4 的 800 行），
//! 门面测试塞回去会顶破。测试模块与被测模块同 crate，但**只**用公开 API 驱动——
//! 走 `pub(crate)` 捷径会让重构失去编译期护栏。
//!
//! 替身与夹具在 `super::facade_support`。

use super::facade_support::*;

// ─────────────────────────────── 受理（§3.1） ───────────────────────────────

#[tokio::test]
async fn a_first_acceptance_is_queued_and_not_a_result() {
    let h = ready_harness();
    let acceptance = match h.gateway.accept(&principal(), request(REVISION)).await {
        Ok(value) => value,
        Err(_) => panic!("就绪会话应当受理"),
    };
    assert!(
        acceptance.is_queued_not_finished(),
        "受理回执的 state 必须恒为 Queued，不能是终态"
    );
    assert_eq!(acceptance.receipt.state, ExecutionState::Queued);
    assert!(!acceptance.is_replay());
    // 受理阶段只读会话，绝不碰驱动。
    assert_eq!(h.port.execute_calls(), 0);
    assert_eq!(h.port.session_view_calls(), 1);
}

#[tokio::test]
async fn a_resend_with_the_same_key_returns_the_same_execution_id_and_writes_once() {
    let h = ready_harness();
    let first = match h.gateway.accept(&principal(), request(REVISION)).await {
        Ok(value) => value,
        Err(_) => panic!("首次受理应当成功"),
    };
    let second = match h.gateway.accept(&principal(), request(REVISION)).await {
        Ok(value) => value,
        Err(_) => panic!("同键重发应当回放"),
    };
    assert_eq!(first.execution_id(), second.execution_id());
    assert!(second.is_replay());
    assert!(!first.is_replay());
    assert_eq!(
        second.receipt.state,
        ExecutionState::Queued,
        "回放出来的仍然是受理回执，不是结果"
    );
    assert_eq!(h.store.write_count(), 1, "同键重发绝不允许第二次写记录");
    assert_eq!(h.gateway.execution_count().await, 1);
}

#[tokio::test]
async fn a_resend_survives_a_context_revision_that_moved_on() {
    let h = ready_harness();
    let first = match h.gateway.accept(&principal(), request(REVISION)).await {
        Ok(value) => value,
        Err(_) => panic!("首次受理应当成功"),
    };
    // 会话上下文已经前进到 9：重发仍必须回放，不能被乐观闸门挡住。
    h.port
        .set_session_view(ready_view(db_session(), Counter::new(1), Counter::new(9)));
    let replay = match h.gateway.accept(&principal(), request(REVISION)).await {
        Ok(value) => value,
        Err(_) => panic!("重发必须回放"),
    };
    assert_eq!(replay.execution_id(), first.execution_id());
    assert!(replay.is_replay());
    assert_eq!(h.store.write_count(), 1);
}

#[tokio::test]
async fn the_same_key_with_a_different_request_is_a_conflict() {
    let h = ready_harness();
    let _ = accept(&h, request(REVISION)).await;
    let other = ExecutionRequest::new(
        session_handle(),
        Counter::new(REVISION),
        CommandCall {
            command: "query".to_owned(),
            input: serde_json::json!({ "sql": "delete from t" }),
        },
        IDEMPOTENCY_KEY,
        source(),
    );
    let error = match h.gateway.accept(&principal(), other).await {
        Ok(_) => panic!("同一个 key 复用到另一个命令必须报冲突"),
        Err(error) => error,
    };
    match error {
        GatewayError::IdempotencyConflict { key, .. } => assert_eq!(key, IDEMPOTENCY_KEY),
        other => panic!("期望 IdempotencyConflict，实际 {other:?}"),
    }
    assert_eq!(h.gateway.execution_count().await, 1);
}

#[tokio::test]
async fn an_unreadable_idempotency_record_asks_for_verification_and_writes_nothing() {
    let port = Arc::new(FakePort::new(ready()));
    let store = Arc::new(UnreadableStore {
        write_calls: AtomicU64::new(0),
    });
    let gateway = ExecutionGateway::new(
        port.clone(),
        Arc::new(AlwaysAllow),
        store.clone(),
        FixedClock::shared(),
    );
    let error = match gateway.accept(&principal(), request(REVISION)).await {
        Ok(_) => panic!("不可读时绝不能判成 Miss 然后受理"),
        Err(error) => error,
    };
    match error {
        GatewayError::IdempotencyVerificationRequired { message } => {
            assert_eq!(message, "storeUnavailable");
        }
        other => panic!("期望 IdempotencyVerificationRequired，实际 {other:?}"),
    }
    assert_eq!(
        store.write_calls.load(Ordering::SeqCst),
        0,
        "不可读路径上绝不允许写库"
    );
    assert_eq!(gateway.execution_count().await, 0);
    assert_eq!(port.execute_calls(), 0);
}

#[tokio::test]
async fn a_stale_context_revision_is_refused_at_acceptance_and_writes_nothing() {
    let h = ready_harness();
    let error = match h.gateway.accept(&principal(), request(REVISION - 1)).await {
        Ok(_) => panic!("过期 revision 必须在受理阶段被拒"),
        Err(error) => error,
    };
    match error {
        GatewayError::Runtime(RuntimeError::ContextRevisionMismatch { expected, actual }) => {
            assert_eq!(expected, REVISION - 1);
            assert_eq!(
                actual, REVISION,
                "必须携带服务端实际值，调用方才能重读后重发"
            );
        }
        other => panic!("期望 ContextRevisionMismatch，实际 {other:?}"),
    }
    assert_eq!(h.store.write_count(), 0);
    assert_eq!(h.gateway.execution_count().await, 0);
}

#[tokio::test]
async fn an_empty_command_is_refused_before_the_port_is_touched() {
    let h = ready_harness();
    let empty = ExecutionRequest::new(
        session_handle(),
        Counter::new(REVISION),
        CommandCall {
            command: String::new(),
            input: serde_json::Value::Null,
        },
        IDEMPOTENCY_KEY,
        source(),
    );
    let error = match h.gateway.accept(&principal(), empty).await {
        Ok(_) => panic!("空命令必须被拒"),
        Err(error) => error,
    };
    match error {
        GatewayError::InvalidRequest { reason } => assert_eq!(reason, "commandRequired"),
        other => panic!("期望 InvalidRequest，实际 {other:?}"),
    }
    assert_eq!(h.port.session_view_calls(), 0, "参数校验先于任何端口调用");
    assert_eq!(h.store.write_count(), 0);
}

// ───────────────────── 拒绝不得降级成「已受理」（§3.1） ─────────────────────

#[tokio::test]
async fn an_unknown_session_is_not_downgraded_to_accepted() {
    let port = Arc::new(
        FakePort::new(ready())
            .with_session_view_error(RuntimeError::UnknownSession(SESSION.to_owned())),
    );
    let h = harness(port);
    let error = match h.gateway.accept(&principal(), request(REVISION)).await {
        Ok(_) => panic!("未知会话绝不能判成已受理"),
        Err(error) => error,
    };
    assert_eq!(
        error,
        GatewayError::Runtime(RuntimeError::UnknownSession(SESSION.to_owned()))
    );
    assert_eq!(h.store.write_count(), 0);
}

#[tokio::test]
async fn a_closed_session_is_not_downgraded_to_accepted() {
    // 端口对终态会话**返回 Ok**：网关必须自己看 state。
    let port = Arc::new(FakePort::new(view(
        SESSION,
        1,
        REVISION,
        SessionState::Closed,
    )));
    let h = harness(port);
    let error = match h.gateway.accept(&principal(), request(REVISION)).await {
        Ok(_) => panic!("已关闭会话绝不能判成已受理"),
        Err(error) => error,
    };
    assert_eq!(
        error,
        GatewayError::Runtime(RuntimeError::SessionClosed(SESSION.to_owned()))
    );
    assert_eq!(h.store.write_count(), 0);
}

#[tokio::test]
async fn a_lost_session_is_not_downgraded_to_accepted() {
    let port = Arc::new(FakePort::new(view(
        SESSION,
        1,
        REVISION,
        SessionState::Lost,
    )));
    let h = harness(port);
    let error = match h.gateway.accept(&principal(), request(REVISION)).await {
        Ok(_) => panic!("已丢失会话绝不能判成已受理"),
        Err(error) => error,
    };
    assert_eq!(
        error,
        GatewayError::Runtime(RuntimeError::SessionLost(SESSION.to_owned()))
    );
    assert_eq!(h.store.write_count(), 0);
}

#[tokio::test]
async fn a_quarantined_session_is_not_downgraded_to_accepted() {
    let port = Arc::new(
        FakePort::new(ready())
            .with_execute_error(RuntimeError::SessionQuarantined("cleanupUnconfirmed")),
    );
    let h = harness(port);
    let id = accept(&h, request(REVISION)).await;
    let error = match h.gateway.dispatch(&principal(), &id).await {
        Ok(_) => panic!("隔离中的会话绝不能下发成功"),
        Err(error) => error,
    };
    assert_eq!(
        error,
        GatewayError::Runtime(RuntimeError::SessionQuarantined("cleanupUnconfirmed"))
    );
    assert_eq!(
        h.gateway.execution(&id).await.map(|r| r.last_state()),
        Some(ExecutionState::Queued),
        "失败路径不许顺手推进状态"
    );
}

#[tokio::test]
async fn a_budget_exhausted_session_is_not_downgraded_to_accepted() {
    let port = Arc::new(
        FakePort::new(ready()).with_execute_error(RuntimeError::BudgetExhausted("queueFull")),
    );
    let h = harness(port);
    let id = accept(&h, request(REVISION)).await;
    let error = match h.gateway.dispatch(&principal(), &id).await {
        Ok(_) => panic!("预算耗尽绝不能下发成功"),
        Err(error) => error,
    };
    assert_eq!(
        error,
        GatewayError::Runtime(RuntimeError::BudgetExhausted("queueFull"))
    );
}

#[tokio::test]
async fn a_session_lost_between_accept_and_dispatch_is_not_downgraded() {
    let h = ready_harness();
    let id = accept(&h, request(REVISION)).await;
    h.port
        .set_session_view(view(SESSION, 1, REVISION, SessionState::Lost));
    let error = match h.gateway.dispatch(&principal(), &id).await {
        Ok(_) => panic!("会话丢失后绝不能下发"),
        Err(error) => error,
    };
    assert_eq!(
        error,
        GatewayError::Runtime(RuntimeError::SessionLost(SESSION.to_owned()))
    );
    assert_eq!(h.port.execute_calls(), 0);
    // 闸门没过 = 请求没出网关：占位必须还回去，否则这条执行自称已下发、
    // 调用方再也重试不了。
    assert!(!record_of(&h, &id).await.is_dispatched());
}

#[tokio::test]
async fn a_driver_failure_keeps_the_dispatch_reservation() {
    let port = Arc::new(
        FakePort::new(ready()).with_execute_error(RuntimeError::SessionLost(SESSION.to_owned())),
    );
    let h = harness_with(
        port.clone(),
        Arc::new(AlwaysAllow),
        InMemoryIdempotencyStore::shared(),
    );
    let id = accept(&h, request(REVISION)).await;
    let error = match h.gateway.dispatch(&principal(), &id).await {
        Ok(_) => panic!("驱动失败时不可能拿到回执"),
        Err(error) => error,
    };
    assert_eq!(
        error,
        GatewayError::Runtime(RuntimeError::SessionLost(SESSION.to_owned()))
    );
    assert!(
        record_of(&h, &id).await.is_dispatched(),
        "已经交给驱动的执行不许被当成还能重发"
    );
}

// ─────────────────────────── 权限与乐观闸门（§3.4） ───────────────────────────

#[tokio::test]
async fn a_permission_denied_at_entry_is_not_downgraded_to_accepted() {
    let port = Arc::new(FakePort::new(ready()));
    let h = harness_with(
        port.clone(),
        AlwaysDeny::shared("alwaysDeny"),
        InMemoryIdempotencyStore::shared(),
    );
    let error = match h.gateway.accept(&principal(), request(REVISION)).await {
        Ok(_) => panic!("入口拒绝绝不能判成已受理"),
        Err(error) => error,
    };
    match error {
        GatewayError::PermissionDenied { action, reason } => {
            assert_eq!(action.as_str(), "execute", "被拒的必须是执行动作");
            assert_eq!(reason, "alwaysDeny");
        }
        other => panic!("期望 PermissionDenied，实际 {other:?}"),
    }
    assert_eq!(h.store.write_count(), 0);
    assert_eq!(port.execute_calls(), 0);
}

#[tokio::test]
async fn a_permission_revoked_between_accept_and_dispatch_blocks_execution() {
    let port = Arc::new(FakePort::new(ready()));
    let authorizer = Arc::new(FlippingAuthorizer {
        allow_execute: AtomicU64::new(1),
    });
    let h = harness_with(
        port.clone(),
        authorizer.clone(),
        InMemoryIdempotencyStore::shared(),
    );
    let id = accept(&h, request(REVISION)).await;
    // 受理之后权限被撤销。
    authorizer.allow_execute.store(0, Ordering::SeqCst);
    let error = match h.gateway.dispatch(&principal(), &id).await {
        Ok(_) => panic!("权限被撤销后绝不能下发"),
        Err(error) => error,
    };
    match error {
        GatewayError::PermissionDenied { action, reason } => {
            assert_eq!(action.as_str(), "execute", "被拒的必须是执行动作");
            assert_eq!(reason, "permissionRevoked");
        }
        other => panic!("期望 PermissionDenied，实际 {other:?}"),
    }
    assert_eq!(port.execute_calls(), 0, "授权失败时驱动绝不能被调用");
}

#[tokio::test]
async fn a_context_revision_change_between_accept_and_dispatch_is_refused_before_execution() {
    let h = ready_harness();
    let id = accept(&h, request(REVISION)).await;
    // 受理之后上下文前进了一格。
    h.port.set_session_view(ready_view(
        db_session(),
        Counter::new(1),
        Counter::new(REVISION + 1),
    ));
    let error = match h.gateway.dispatch(&principal(), &id).await {
        Ok(_) => panic!("revision 变化后绝不能真正执行"),
        Err(error) => error,
    };
    match error {
        GatewayError::Runtime(RuntimeError::ContextRevisionMismatch { expected, actual }) => {
            assert_eq!(expected, REVISION);
            assert_eq!(actual, REVISION + 1);
        }
        other => panic!("期望 ContextRevisionMismatch，实际 {other:?}"),
    }
    assert_eq!(
        h.port.execute_calls(),
        0,
        "闸门必须拦在驱动调用之前，而不是事后报错"
    );
}

#[tokio::test]
async fn dispatch_sends_the_request_frozen_at_acceptance() {
    let h = ready_harness();
    let id = accept(&h, request(REVISION)).await;
    let receipt = match h.gateway.dispatch(&principal(), &id).await {
        Ok(value) => value,
        Err(_) => panic!("下发应当成功"),
    };
    assert_eq!(receipt.state, ExecutionState::Queued);
    assert_eq!(h.port.execute_calls(), 1);
    let sent = h.port.executed_requests();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].idempotency_key, IDEMPOTENCY_KEY);
    assert_eq!(sent[0].expected_context_revision, Counter::new(REVISION));
    assert_eq!(sent[0].handle, session_handle());
    assert_eq!(sent[0].call, call());
}

#[tokio::test]
async fn dispatching_an_unknown_execution_is_refused_without_touching_the_port() {
    let h = ready_harness();
    let error = match h
        .gateway
        .dispatch(&principal(), &execution_id("exe-absent"))
        .await
    {
        Ok(_) => panic!("不存在的执行绝不能被顺手受理"),
        Err(error) => error,
    };
    assert_eq!(
        error,
        GatewayError::InvalidRequest {
            reason: "unknownExecution"
        }
    );
    assert_eq!(h.port.execute_calls(), 0);
}

#[tokio::test]
async fn dispatching_twice_is_refused() {
    let h = ready_harness();
    let id = accept(&h, request(REVISION)).await;
    let _ = h.gateway.dispatch(&principal(), &id).await;
    let error = match h.gateway.dispatch(&principal(), &id).await {
        Ok(_) => panic!("同一次执行绝不能下发两次"),
        Err(error) => error,
    };
    assert_eq!(
        error,
        GatewayError::InvalidRequest {
            reason: "alreadyDispatched"
        }
    );
    assert_eq!(h.port.execute_calls(), 1);
}

// ─────────────────────────────── CM-60 计时 ───────────────────────────────

#[tokio::test]
async fn the_gateway_segment_excludes_the_driver_round_trip() {
    // 驱动往返 40ns：由替身在 `execute_in_session` 内部推进，
    // 于是「驱动耗时」与「网关耗时」在同一根时间轴上分得开。
    let port = Arc::new(FakePort::new(ready()).with_driver_nanos(40));
    let h = harness(port);
    let id = accept(&h, request(REVISION)).await;
    // 受理 → 下发之间推进 4ns：这是网关自己的活（授权重校验 + 闸门 + 下发调用）。
    h.clock.advance(4);
    let _ = h.gateway.dispatch(&principal(), &id).await;
    let samples = h.gateway.overhead_samples().await;
    assert_eq!(samples.len(), 1);
    let sample = samples.as_slice()[0];
    assert_eq!(sample.gateway_nanos, 4, "段①只包住授权/校验到下发的距离");
    // 段② 从驱动完成那一刻起算。冻结时钟下登记本身不耗时，算术
    // （标记完成后再推进 N 才取样）的证明放在 timing.rs 的单元测试里；
    // 这里要证明的是 40ns 的驱动往返既没混进段①，也没混进段②。
    assert_eq!(
        sample.registration_nanos, 0,
        "驱动往返落在两枚打点之间，两段都不得把它算进来"
    );
    assert_eq!(sample.total_nanos(), 4);
    assert_eq!(h.gateway.overhead_p95().await, Some(4));
}

#[tokio::test]
async fn a_thousand_times_slower_driver_does_not_change_the_gateway_segment() {
    let fast_port = Arc::new(FakePort::new(ready()).with_driver_nanos(1));
    let fast = harness(fast_port);
    let fast_id = accept(&fast, request(REVISION)).await;
    fast.clock.advance(6);
    let _ = fast.gateway.dispatch(&principal(), &fast_id).await;

    let slow_port = Arc::new(FakePort::new(ready()).with_driver_nanos(1_000));
    let slow = harness(slow_port);
    let slow_id = accept(&slow, request(REVISION)).await;
    slow.clock.advance(6);
    let _ = slow.gateway.dispatch(&principal(), &slow_id).await;

    assert_eq!(
        fast.gateway.overhead_samples().await.as_slice()[0].gateway_nanos,
        6
    );
    assert_eq!(
        slow.gateway.overhead_samples().await.as_slice()[0].gateway_nanos,
        6,
        "驱动慢 1000 倍，网关开销一个刻度也不能跟着涨"
    );
    assert_eq!(fast.gateway.overhead_p95().await, Some(6));
    assert_eq!(slow.gateway.overhead_p95().await, Some(6));
}

#[tokio::test]
async fn an_undispatched_execution_contributes_no_sample() {
    let h = ready_harness();
    let _ = accept(&h, request(REVISION)).await;
    assert!(
        h.gateway.overhead_samples().await.is_empty(),
        "只受理不执行的请求根本没有第二段，不该进样本仓库"
    );
    assert_eq!(h.gateway.overhead_p95().await, None);
}
