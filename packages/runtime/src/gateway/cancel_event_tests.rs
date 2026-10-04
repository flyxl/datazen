//! `ExecutionGateway` 的取消三态与事件交付单测（§3.2 / CM-55）。
//!
//! 与 `facade_tests.rs` 分开是因为单文件 800 行上限，不是语义上的分组理由。
//! 替身与夹具在 `super::facade_support`。

use super::facade_support::*;

// ─────────────────────────── 取消三态（§3.2 / D-01） ───────────────────────────

#[tokio::test]
async fn a_driver_without_precise_cancel_is_unsupported_and_never_touches_the_port() {
    let h = ready_harness();
    let id = accept(&h, request(REVISION)).await;
    let outcome = match cancel_with(&h, &id, PreciseCancel::Unsupported).await {
        Ok(outcome) => outcome,
        Err(error) => panic!("不支持精确取消时也要给出明确处置，实际 {error:?}"),
    };
    assert_eq!(outcome.disposition, CancelDisposition::Unsupported);
    assert_eq!(outcome.disposition.as_str(), "unsupported");
    assert!(
        !outcome.disposition.is_requested(),
        "unsupported 绝不能对外呈现成「已下发取消」"
    );
    assert_eq!(
        h.port.cancel_calls(),
        0,
        "驱动不支持精确取消时碰它，就是一次会误伤同会话其他执行的真调用"
    );
}

#[tokio::test]
async fn cancelling_an_execution_the_gateway_never_accepted_is_a_cancel_failure() {
    let h = ready_harness();
    let error = match h
        .gateway
        .cancel(
            &principal(),
            CancelRequest::new(
                CancelBinding::new(execution_id("exe_never_accepted"), session_handle()),
                PreciseCancel::Supported,
            ),
        )
        .await
    {
        Ok(outcome) => panic!("未受理的执行不得报取消成功，实际 {outcome:?}"),
        Err(error) => error,
    };
    assert_eq!(
        error,
        GatewayError::Runtime(cancel_failed(UNKNOWN_EXECUTION_REASON)),
        "未受理的执行必须是 CancelFailed，而不是一个凭空的成功回执"
    );
    assert_eq!(h.port.cancel_calls(), 0);
}

#[tokio::test]
async fn a_driver_cancel_failure_is_propagated_and_never_downgraded_to_cancelled() {
    let port = Arc::new(
        FakePort::new(ready())
            .with_cancel(Err(RuntimeError::CancelFailed("driverPathUnreachable"))),
    );
    let h = harness(port.clone());
    let id = accept(&h, request(REVISION)).await;
    let error = match cancel_with(&h, &id, PreciseCancel::Supported).await {
        Ok(outcome) => panic!("取消失败绝不能降级成已取消，实际 {outcome:?}"),
        Err(error) => error,
    };
    assert_eq!(
        error,
        GatewayError::Runtime(RuntimeError::CancelFailed("driverPathUnreachable"))
    );
    assert_eq!(port.cancel_calls(), 1);
}

#[tokio::test]
async fn a_live_execution_reports_requested_without_claiming_a_terminal_state() {
    let port = Arc::new(FakePort::new(ready()).with_cancel(Ok(ExecutionState::CancelRequested)));
    let h = harness(port.clone());
    let id = accept(&h, request(REVISION)).await;
    let outcome = match cancel_with(&h, &id, PreciseCancel::Supported).await {
        Ok(outcome) => outcome,
        Err(error) => panic!("精确取消应被受理，实际 {error:?}"),
    };
    assert_eq!(outcome.disposition, CancelDisposition::Requested);
    assert_eq!(outcome.disposition.as_str(), "requested");
    assert!(outcome.disposition.is_requested());
    assert_eq!(outcome.state, ExecutionState::CancelRequested);
    assert_eq!(outcome.execution_id, id);
    assert_eq!(port.cancel_calls(), 1);
    assert!(
        !outcome.is_terminal(),
        "取消是异步的：requested 不等于已经 Cancelled"
    );
}

#[tokio::test]
async fn an_execution_already_in_a_terminal_state_reports_already_finished() {
    let port = Arc::new(FakePort::new(ready()).with_cancel(Ok(ExecutionState::Succeeded)));
    let h = harness(port.clone());
    let id = accept(&h, request(REVISION)).await;
    let outcome = match cancel_with(&h, &id, PreciseCancel::Supported).await {
        Ok(outcome) => outcome,
        Err(error) => panic!("终态执行取消应回 alreadyFinished，实际 {error:?}"),
    };
    assert_eq!(outcome.disposition, CancelDisposition::AlreadyFinished);
    assert_eq!(outcome.disposition.as_str(), "alreadyFinished");
    assert!(!outcome.disposition.is_requested());
    assert_eq!(outcome.state, ExecutionState::Succeeded);
}

#[tokio::test]
async fn a_binding_on_a_different_epoch_is_refused_before_the_port_is_touched() {
    let port = Arc::new(FakePort::new(ready()).with_cancel(Ok(ExecutionState::CancelRequested)));
    let h = harness(port.clone());
    let id = accept(&h, request(REVISION)).await;
    let error = match h
        .gateway
        .cancel(
            &principal(),
            CancelRequest::new(
                // 同一个 dbSessionId、不同的 runtimeEpoch：旧的执行已被换掉。
                CancelBinding::new(id.clone(), handle(SESSION, 2)),
                PreciseCancel::Supported,
            ),
        )
        .await
    {
        Ok(outcome) => panic!("绑定不一致绝不能取消成功，实际 {outcome:?}"),
        Err(error) => error,
    };
    assert_eq!(
        error,
        GatewayError::Runtime(cancel_failed(cancel_binding_reason::EPOCH_MISMATCH))
    );
    assert_eq!(port.cancel_calls(), 0, "绑定不一致必须在触达端口之前拒掉");
}

#[tokio::test]
async fn a_binding_on_a_different_resource_binding_is_refused_before_the_port() {
    let port = Arc::new(FakePort::new(ready()).with_cancel(Ok(ExecutionState::CancelRequested)));
    let h = harness(port.clone());
    let bound = request(REVISION).with_resource_binding(ResourceId::new("rsc-accepted"));
    let id = accept(&h, bound).await;
    let error = match h
        .gateway
        .cancel(
            &principal(),
            CancelRequest::new(
                CancelBinding::new(id.clone(), session_handle())
                    .with_resource_binding(ResourceId::new("rsc-other")),
                PreciseCancel::Supported,
            ),
        )
        .await
    {
        Ok(outcome) => panic!("资源绑定不一致绝不能取消成功，实际 {outcome:?}"),
        Err(error) => error,
    };
    assert_eq!(
        error,
        GatewayError::Runtime(cancel_failed(
            cancel_binding_reason::RESOURCE_BINDING_MISMATCH
        ))
    );
    assert_eq!(
        port.cancel_calls(),
        0,
        "资源绑定不一致必须在触达端口之前拒掉"
    );
}

#[tokio::test]
async fn cancel_is_authorized_at_its_own_entry_point() {
    let port = Arc::new(FakePort::new(ready()).with_cancel(Ok(ExecutionState::CancelRequested)));
    // 受理放行、取消拒绝：证明取消不是「受理授权过就一路放行」。
    let h = harness_with(
        port.clone(),
        Arc::new(DenyCancel),
        InMemoryIdempotencyStore::shared(),
    );
    let id = accept(&h, request(REVISION)).await;
    let error = match cancel_with(&h, &id, PreciseCancel::Supported).await {
        Ok(outcome) => panic!("取消被拒时绝不能报成功，实际 {outcome:?}"),
        Err(error) => error,
    };
    match error {
        GatewayError::PermissionDenied { action, reason } => {
            assert_eq!(action.as_str(), "cancel", "被拒的必须是取消动作");
            assert_eq!(reason, "cancelForbidden");
        }
        other => panic!("期望 PermissionDenied，实际 {other:?}"),
    }
    assert_eq!(port.cancel_calls(), 0, "取消授权失败时驱动绝不能被调用");
}

// ─────────────────────────── 事件交付（CM-55） ───────────────────────────

#[tokio::test]
async fn an_event_carrying_another_session_is_ignored_and_changes_nothing() {
    let h = ready_harness();
    let id = accept(&h, request(REVISION)).await;
    let mut incoming = event(
        &id,
        1,
        ExecutionEventKind::Terminal {
            state: ExecutionState::Succeeded,
        },
    );
    incoming.db_session_id = DbSessionId::new("dbse-other");
    assert_eq!(
        h.gateway.apply_event(incoming).await,
        EventDisposition::IgnoredStaleSession {
            bound: db_session()
        }
    );
    assert_eq!(
        record_of(&h, &id).await.observed_sequence(),
        None,
        "别的会话的帧不许推进这条执行"
    );
}

#[tokio::test]
async fn a_stale_epoch_event_is_ignored_and_leaves_the_record_alone() {
    let h = ready_harness();
    let id = accept(&h, request(REVISION)).await;
    let mut incoming = event(
        &id,
        1,
        ExecutionEventKind::Terminal {
            state: ExecutionState::Failed,
        },
    );
    // 驱动重启后复用旧 executionId、把 epoch 推到更小：这类帧一律丢弃。
    incoming.runtime_epoch = Counter::new(0);
    assert_eq!(
        h.gateway.apply_event(incoming).await,
        EventDisposition::IgnoredStaleEpoch {
            bound: Counter::new(1)
        }
    );
    let record = record_of(&h, &id).await;
    assert_eq!(record.last_state(), ExecutionState::Queued);
    assert_eq!(record.observed_sequence(), None);
}

#[tokio::test]
async fn an_event_from_a_later_epoch_is_ignored_even_with_the_right_execution_id() {
    let h = ready_harness();
    let id = accept(&h, request(REVISION)).await;
    let mut incoming = event(
        &id,
        1,
        ExecutionEventKind::Terminal {
            state: ExecutionState::Succeeded,
        },
    );
    // 同一个 executionId、来自更新的 epoch：本网关不认这个执行，绝不能顺手吃掉。
    incoming.runtime_epoch = Counter::new(9);
    assert_eq!(
        h.gateway.apply_event(incoming).await,
        EventDisposition::IgnoredForeignEpoch {
            bound: Counter::new(1)
        }
    );
    let record = record_of(&h, &id).await;
    assert_eq!(
        record.last_state(),
        ExecutionState::Queued,
        "更新的 epoch 同样不得改写本执行"
    );
    assert_eq!(record.observed_sequence(), None);
}

#[tokio::test]
async fn a_terminal_event_updates_the_recorded_state_but_never_the_source() {
    let h = ready_harness();
    let id = accept(&h, request(REVISION)).await;
    assert_eq!(
        h.gateway
            .apply_event(event(
                &id,
                1,
                ExecutionEventKind::Terminal {
                    state: ExecutionState::Succeeded
                }
            ))
            .await,
        EventDisposition::Applied
    );
    // 事件自称来自后台作业：必须被丢弃，来源仍以受理请求为准（CM-61）。
    let mut rogue = event(
        &id,
        2,
        ExecutionEventKind::StateChanged {
            state: ExecutionState::Failed,
        },
    );
    rogue.declared_source = Some(ExecutionSource::new(
        SourceKind::Job,
        "job-rogue",
        None,
        None,
    ));
    let _ = h.gateway.apply_event(rogue).await;
    let record = record_of(&h, &id).await;
    assert_eq!(record.source(), &source());
    // 帧本身照常生效（它在序号上是合法的）；被丢掉的只有它自称的来源。
    assert_eq!(
        record.last_state(),
        ExecutionState::Failed,
        "来源声明被丢弃，但这条事件本身该有的状态迁移仍然生效"
    );
    assert_eq!(record.declared_source_events_ignored(), 1);
}

#[tokio::test]
async fn a_sequence_gap_asks_for_recovery_and_stops_moving_the_watermark() {
    let h = ready_harness();
    let id = accept(&h, request(REVISION)).await;
    assert_eq!(
        h.gateway
            .apply_event(event(
                &id,
                1,
                ExecutionEventKind::StateChanged {
                    state: ExecutionState::Running
                }
            ))
            .await,
        EventDisposition::Applied
    );
    let disposition = h
        .gateway
        .apply_event(event(
            &id,
            5,
            ExecutionEventKind::Terminal {
                state: ExecutionState::Succeeded,
            },
        ))
        .await;
    match disposition {
        EventDisposition::Gap { expected, received } => {
            assert_eq!(expected.get(), 2);
            assert_eq!(received.get(), 5);
        }
        other => panic!("期望 Gap，实际 {other:?}"),
    }
    let record = record_of(&h, &id).await;
    assert!(record.needs_recovery());
    // 契约：补订阅起点 = 缺口前最后一个已应用序号（不是缺口本身那个序号）。
    assert_eq!(
        record.resubscribe_from(),
        Some(Counter::new(1)),
        "补订阅起点必须是缺口前最后一个已应用序号"
    );
    assert_eq!(
        record.observed_sequence(),
        Some(Counter::new(1)),
        "空洞之后的序号不得前移水位线"
    );
    assert_eq!(record.last_state(), ExecutionState::Running);
}

#[tokio::test]
async fn recovering_from_a_snapshot_clears_the_gap_and_realigns_state() {
    let h = ready_harness();
    let id = accept(&h, request(REVISION)).await;
    // 先收到 1，再直接来 9：中间那截丢了，才是真缺口。
    // （首帧永远是基线，没有前序序号可比，因此不产生缺口。）
    assert_eq!(
        h.gateway
            .apply_event(event(
                &id,
                1,
                ExecutionEventKind::StateChanged {
                    state: ExecutionState::Running,
                },
            ))
            .await,
        EventDisposition::Applied
    );
    assert_eq!(
        h.gateway
            .apply_event(event(
                &id,
                9,
                ExecutionEventKind::StateChanged {
                    state: ExecutionState::Running,
                },
            ))
            .await,
        EventDisposition::Gap {
            expected: Counter::new(2),
            received: Counter::new(9),
        }
    );
    assert!(record_of(&h, &id).await.needs_recovery());

    let snapshot = ready_view(db_session(), Counter::new(1), Counter::new(11));
    assert!(h.gateway.recover_from_snapshot(&id, &snapshot).await);
    let record = record_of(&h, &id).await;
    assert!(!record.needs_recovery());
    assert_eq!(record.resubscribe_from(), None);
    assert_eq!(record.observed_context_revision(), Counter::new(11));
}

#[tokio::test]
async fn a_duplicated_chunk_does_not_double_count_rows() {
    let h = ready_harness();
    let id = accept(&h, request(REVISION)).await;
    let chunk = ExecutionEventKind::ResultChunk {
        chunk_index: Counter::new(3),
        rows: 10,
    };
    assert_eq!(
        h.gateway.apply_event(event(&id, 1, chunk.clone())).await,
        EventDisposition::Applied
    );
    // 重发同一块（换了序号，模拟驱动重投）。
    assert_eq!(
        h.gateway.apply_event(event(&id, 2, chunk)).await,
        EventDisposition::IgnoredDuplicateChunk {
            chunk_index: Counter::new(3)
        }
    );
    assert_eq!(record_of(&h, &id).await.observed_row_count(), 10);
    assert_eq!(h.gateway.execution_count().await, 1);
}

#[tokio::test]
async fn a_huge_counter_survives_the_round_trip_exactly() {
    const HUGE: u64 = 9_007_199_254_740_993;
    let h = ready_harness();
    let id = accept(&h, request(REVISION)).await;
    let mut incoming = event(
        &id,
        1,
        ExecutionEventKind::ResultChunk {
            chunk_index: Counter::new(1),
            rows: HUGE - 1,
        },
    );
    incoming.context_revision = Counter::new(HUGE);
    assert_eq!(
        h.gateway.apply_event(incoming).await,
        EventDisposition::Applied
    );
    let record = record_of(&h, &id).await;
    assert_eq!(record.observed_context_revision().get(), HUGE);
    assert_eq!(record.observed_row_count(), HUGE - 1);
}

#[tokio::test]
async fn an_unbound_gateway_reports_no_execution() {
    let h = ready_harness();
    assert!(h
        .gateway
        .execution(&execution_id("exe-absent"))
        .await
        .is_none());
    assert_eq!(h.gateway.execution_count().await, 0);
    assert!(
        !h.gateway
            .recover_from_snapshot(&execution_id("exe-absent"), &ready())
            .await
    );
    // 网关没受理过的执行，事件同样投递不进来。
    assert_eq!(
        h.gateway
            .apply_event(event(
                &execution_id("exe-absent"),
                1,
                ExecutionEventKind::Terminal {
                    state: ExecutionState::Succeeded,
                },
            ))
            .await,
        EventDisposition::Unbound
    );
    assert_eq!(h.gateway.execution_count().await, 0);
}

/// 「水位线拒绝改绑」这条不变式属于 `EventStore`；这里补一条门面级别的可见性证明：
/// 同一网关内两次受理拿到的是两条独立水位线，互不串台。
#[tokio::test]
async fn two_executions_keep_independent_event_watermarks() {
    let h = ready_harness();
    let first = accept(&h, request(REVISION)).await;
    let second = accept(
        &h,
        ExecutionRequest::new(
            session_handle(),
            Counter::new(REVISION),
            CommandCall {
                command: "explain".to_owned(),
                input: serde_json::json!({ "sql": "select 2" }),
            },
            "idem-key-second",
            source(),
        ),
    )
    .await;
    assert_ne!(first, second);
    assert_eq!(
        h.gateway
            .apply_event(event(
                &first,
                4,
                ExecutionEventKind::StateChanged {
                    state: ExecutionState::Running
                }
            ))
            .await,
        EventDisposition::Applied
    );
    // 事件归属只看它自带的 executionId：指向 first，就绝不能顺带动到 second。
    assert_eq!(
        record_of(&h, &first).await.observed_sequence(),
        Some(Counter::new(4))
    );
    assert_eq!(
        record_of(&h, &second).await.observed_sequence(),
        None,
        "同会话的另一条执行不得被这条事件顺带更新"
    );
}
