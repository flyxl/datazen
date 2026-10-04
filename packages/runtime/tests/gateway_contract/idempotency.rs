//! §3.3 / CM-54·55·56：同一幂等键重发必须回到同一个 `executionId`，落库恰好一次，
//! 读不到旧记录必须要求核验，绝不自动换键重试。

use std::sync::Arc;

use super::{err, record};
use crate::gateway_fixtures as fx;
use datazen_runtime::connection::{OrganizationId, PrincipalId, SessionState};
use datazen_runtime::gateway::{AcceptanceDisposition, AlwaysAllow, GatewayError, SourceKind};
// ───────────────── B §3.3 幂等 ─────────────────

#[tokio::test(start_paused = true)]
async fn a_resend_returns_the_same_execution_id_and_writes_once() {
    let (h, store) = fx::counted_harness();
    let first = h
        .gateway
        .accept(&fx::principal(), fx::request(fx::REVISION))
        .await
        .expect("应当受理");
    let second = h
        .gateway
        .accept(&fx::principal(), fx::request(fx::REVISION))
        .await
        .expect("应当重发");

    assert_eq!(first.execution_id(), second.execution_id());
    assert!(second.is_replay());
    // DB 写入恰好一次，重发不得换新键、不得二次落库。
    assert_eq!(store.write_count(), 1);
    assert_eq!(store.len(), 1);
    assert_eq!(h.gateway.execution_count().await, 1);
    assert_eq!(h.port.execute_calls(), 0);
}

#[tokio::test(start_paused = true)]
async fn a_resend_keeps_the_first_accepted_timestamp() {
    let h = fx::ready_harness();
    fx::accept(&h, fx::request(fx::REVISION)).await;
    h.clock.advance(9_000);
    let replay = h
        .gateway
        .accept(&fx::principal(), fx::request(fx::REVISION))
        .await
        .expect("应当重发");

    match replay.disposition {
        AcceptanceDisposition::Replayed {
            first_seen_at_nanos,
        } => assert_eq!(first_seen_at_nanos, 0),
        other => panic!("期望重发，实际 {other:?}"),
    }
}

#[tokio::test(start_paused = true)]
async fn a_resend_survives_a_moved_context_revision() {
    let h = fx::ready_harness();
    fx::accept(&h, fx::request(fx::REVISION)).await;
    // 服务端上下文已推进：查重在前，所以这仍然是一次重发而不是并发冲突。
    h.port.set_view(Ok(fx::view(
        fx::SESSION,
        1,
        fx::REVISION + 1,
        SessionState::Ready,
    )));

    let replay = h
        .gateway
        .accept(&fx::principal(), fx::request(fx::REVISION))
        .await
        .expect("重发不应被乐观闸拦住");
    assert!(replay.is_replay());
    assert_eq!(h.gateway.execution_count().await, 1);
}

#[tokio::test(start_paused = true)]
async fn the_same_key_with_a_different_call_is_a_conflict() {
    let h = fx::ready_harness();
    fx::accept(&h, fx::request(fx::REVISION)).await;

    let mut second = fx::request(fx::REVISION);
    second.call.input = serde_json::json!({ "sql": "select 2" });
    let error = err(h.gateway.accept(&fx::principal(), second).await);
    match error {
        GatewayError::IdempotencyConflict { key, .. } => assert_eq!(key, fx::IDEMPOTENCY_KEY),
        other => panic!("期望幂等冲突，实际 {other:?}"),
    }
    assert_eq!(h.gateway.execution_count().await, 1);
}

#[tokio::test(start_paused = true)]
async fn an_unreadable_record_requires_verification_and_writes_nothing() {
    let store = fx::UnreadableStore::shared();
    let h = fx::harness_with(
        Arc::new(fx::RecordingPort::new(fx::ready_view())),
        Arc::new(AlwaysAllow),
        store.clone(),
    );

    let error = err(h
        .gateway
        .accept(&fx::principal(), fx::request(fx::REVISION))
        .await);
    match error {
        GatewayError::IdempotencyVerificationRequired { .. } => {}
        other => panic!("期望要求核验，实际 {other:?}"),
    }
    // 读不到就不能写：既不能凭猜测受理，也不能自动换键重试。
    assert_eq!(store.write_calls(), 0);
    assert_eq!(h.gateway.execution_count().await, 0);
}

#[tokio::test(start_paused = true)]
async fn a_failed_persistence_leaves_no_execution_record() {
    let store = fx::WriteFailingStore::shared();
    let h = fx::harness_with(
        Arc::new(fx::RecordingPort::new(fx::ready_view())),
        Arc::new(AlwaysAllow),
        store.clone(),
    );

    let error = err(h
        .gateway
        .accept(&fx::principal(), fx::request(fx::REVISION))
        .await);
    match error {
        GatewayError::IdempotencyPersistFailed { .. } => {}
        other => panic!("期望落库失败，实际 {other:?}"),
    }
    assert_eq!(store.write_calls(), 1);
    assert_eq!(h.gateway.execution_count().await, 0);
}

#[tokio::test(start_paused = true)]
async fn g4_the_same_key_with_a_different_source_is_a_conflict_not_a_replay() {
    let h = fx::ready_harness();
    let id = fx::accept(&h, fx::request(fx::REVISION)).await;

    // CM-61：来源参与指纹。同一幂等键、同一命令，只换来源 ⇒ 冲突而不是重发，
    // 否则后台任务的来源会被静默记成首次那条（编辑器）的来源。
    let mut second = fx::request(fx::REVISION);
    second.source = fx::background_source();
    let error = err(h.gateway.accept(&fx::principal(), second).await);
    match error {
        GatewayError::IdempotencyConflict { key, .. } => assert_eq!(key, fx::IDEMPOTENCY_KEY),
        other => panic!("期望幂等冲突，实际 {other:?}"),
    }

    // 冲突不新建执行、不下发驱动，也不得改写首条记录里已冻结的来源。
    assert_eq!(h.gateway.execution_count().await, 1);
    assert_eq!(h.port.execute_calls(), 0);
    assert_eq!(record(&h, &id).await.source(), &fx::source());
}

#[tokio::test(start_paused = true)]
async fn a_replay_receipt_reports_the_source_frozen_at_first_acceptance() {
    // CM-61 的落盘面是**受理回执上的 source**，不是账本里的记录：调用方拿回执就知道
    // 「这条执行是哪条链路发起的」。所以重发回执必须同样带上来源，且这个来源只能是
    // 首次受理时冻结的那一个——重发方可以是一个完全不同的调用点。
    let h = fx::ready_harness();
    // 同一幂等键 + 同一命令 + 同一来源 ⇒ 第二次是重发。
    let first = h
        .gateway
        .accept(&fx::principal(), fx::request(fx::REVISION))
        .await
        .expect("应当受理");
    let replay = h
        .gateway
        .accept(&fx::principal(), fx::request(fx::REVISION))
        .await
        .expect("应当重发");

    // 处置面：必须是 `Replayed`，且带的是**首次**受理时刻。
    assert_eq!(
        replay.disposition,
        AcceptanceDisposition::Replayed {
            first_seen_at_nanos: 0
        }
    );

    // ── 下面每条断言都直接读**回执**上的 `source` 字段，不经账本记录中转。
    // 整体相等（`ExecutionSource` 是 PartialEq 的完整值，四字段一起比）。
    assert_eq!(replay.source, fx::source());
    // 与首次受理回执上冻结的来源逐字段相等：两次受理是同一请求，来源必须一致。
    assert_eq!(replay.source, first.source);
    // 逐字段钉到字面量：伪造一个 `kind` 相同的来源骗不过 `source_id` 与两个 id。
    assert_eq!(replay.source.kind(), SourceKind::Editor);
    assert_eq!(replay.source.source_id(), "edt-contract");
    assert_eq!(
        replay.source.organization_id(),
        Some(&OrganizationId::new("org-contract"))
    );
    assert_eq!(
        replay.source.principal_id(),
        Some(&PrincipalId::new("principal-contract"))
    );

    // 回执的 executionId 必须是首次那一个（不是重发时新生成的）。
    assert_eq!(replay.execution_id(), first.execution_id());
    // 重发不新建执行、不下发驱动。
    assert_eq!(h.gateway.execution_count().await, 1);
    assert_eq!(h.port.execute_calls(), 0);
}
