//! §9.4 释放顺序 / §6.5 句柄登记与绑定 / §12 CM-58 陈旧额度 —— 登记表集成面。
//!
//! 释放这一段的价值全在**顺序**上，而顺序只能从「后端收到的调用序列」读出来，
//! 不能从返回值读：四个步骤都做完，返回值只是一个终态。因此这里的断言集中在
//! 两处：每个 `FinalizeHandles` 落在**登记时那个资源**上，每个 `CloseResource`
//! 落在**当前物理资源**上；以及登记数与后端确认数对不上时，墓碑不得写成「已关闭」。
//!
//! | 用例 | 分支 | 权威 |
//! | --- | --- | --- |
//! | 句柄先在原资源上终结再关物理资源 | 四步顺序 | §9.4 步骤 1 → 3 |
//! | 跨资源句柄按登记资源分批终结 | 按 resource_id 分组 | §6.5 / §9.4 |
//! | 回滚关闭把 disposition 写成 Rollback | 模式决定处置 | §9.4 步骤 1 |
//! | 有活句柄要求无事务关闭当场拒绝 | 派发前拒绝 | §7.4 |
//! | 无登记句柄时直接关资源 | 无需终结 | §9.4 步骤 2 |
//! | 登记数对不上时墓碑不得报已关闭 | 不可判定 → `Lost` | §9.4 / CM-73 |
//! | 空闲到期才驱逐并归还额度 | 注入时钟 | §6.4 |
//! | 未设空闲期限的会话永不被驱逐 | 无期限即不驱逐 | §6.4 |
//! | 租约失效作废后额度扣住不外发 | CM-58 两段式 | §12 / CM-58 |
//! | 归池前检查有登记句柄一律不放行 | §9.4 步骤 2 | §9.4 |

#![allow(dead_code)]
mod registry_fixtures;

use std::sync::Arc;

use datazen_runtime::connection::{
    CloseMode, RuntimeError, SessionHandle, SessionState, TransactionState, WorkerId,
};
use datazen_runtime::registry::backend::{ready_to_return_to_pool, HandleDisposition};
use datazen_runtime::registry::{
    AuditKind, Outcome, RegistryAuditEntry, SessionPort, SessionRegistry,
};

use registry_fixtures::{
    as_backend, close_any, db_session_id, execute_request, handle_of, handle_ref, open_request,
    other_db_session_id, register_ready, worker_id, BackendPlan, ScriptedBackend, IDLE_DEADLINE_MS,
    SESSION_LIMIT,
};

fn registry_with(backend: &Arc<ScriptedBackend>) -> Arc<SessionRegistry> {
    Arc::new(SessionRegistry::new(as_backend(backend), SESSION_LIMIT))
}

/// 造一个登记好的会话并跑完一次带句柄的执行，返回会话句柄。
///
/// 句柄必须先被**登记**才谈得上在关闭时被终结，所以每个释放用例都从这里起步，
/// 避免出现「关闭时根本没有句柄可终结」的空跑。
/// 累积日志里某类条目的快照。
///
/// **不能用 `drain_audit()` 做存在性断言**：它只返回「上一次收干之后新到的」条目，
/// 而 `is_registered` / `session_view` / `cancel_execution` / `registered_ids`
/// 每一次调用都会顺手把审计队列 pump 进累积日志（registry.rs 的实现如此），
/// 于是夹在中间的投影调用会把断言要看的那条吃掉——症状是「明明发了审计，计数却是 0」。
fn entries_of(registry: &SessionRegistry, kind: AuditKind) -> Vec<RegistryAuditEntry> {
    registry
        .audit_log()
        .into_iter()
        .filter(|entry| entry.kind == kind)
        .collect()
}

async fn session_with_handles(registry: &Arc<SessionRegistry>) -> SessionHandle {
    let view = register_ready(registry, db_session_id()).await;
    let handle = handle_of(&view);
    registry
        .submit_execution(execute_request(&handle, 1))
        .await
        .expect("带句柄的执行必须完成");
    handle
}

#[tokio::test(start_paused = true)]
async fn 句柄先在登记时的资源上终结再关当前物理资源() {
    // 句柄登记在 `res_legacy` 上，会话当前的物理资源是 `res_1`：
    // 两者不同正是 §9.4「为什么 1 必须在 3 之前」的可观测形态——
    // 终结请求带着登记时的资源，关闭请求带着当前资源。
    let backend = ScriptedBackend::new(
        BackendPlan::default()
            .handles(vec![
                handle_ref("h1", "res_legacy"),
                handle_ref("h2", "res_legacy"),
            ])
            .finalizes(2, 0),
    )
    .await;
    let registry = registry_with(&backend);
    let handle = session_with_handles(&registry).await;

    registry
        .close_registered(&handle, close_any())
        .await
        .expect("两个句柄都被干净终结，关闭必须成功");

    assert_eq!(backend.finalize_calls(), 1, "同一资源上的句柄必须合成一批");
    assert_eq!(backend.close_calls(), 1);

    let traces = backend.traces().await;
    assert_eq!(
        traces.finalized[0].resource_id, "res_legacy",
        "终结必须落在句柄登记时的那个资源上"
    );
    assert_eq!(traces.finalized[0].handles.len(), 2);
    assert_eq!(
        traces.closed_with[0].resource_id, "res_1",
        "关闭必须落在当前物理资源上"
    );
    assert_eq!(
        traces.closed_with[0].registered_handles, 0,
        "关闭请求里的句柄数是**释放后**的账：句柄已在第 1 步清空"
    );

    assert!(!registry.is_registered(&db_session_id()), "关闭后必须注销");
    assert_eq!(
        registry.remaining_quota(),
        SESSION_LIMIT,
        "关闭成功必须归还额度"
    );
}

#[tokio::test(start_paused = true)]
async fn 跨资源句柄按登记资源分批终结不得跨资源混合() {
    let backend = ScriptedBackend::new(
        BackendPlan::default()
            .handles(vec![
                handle_ref("h1", "res_a"),
                handle_ref("h2", "res_b"),
                handle_ref("h3", "res_a"),
            ])
            // 3 个句柄分两批：确认数必须**逐批不同**（2 + 1 = 3），
            // §9.4 比的是累计确认数与宿主登记数，固定回报表达不了这个形状。
            .finalize_sequence(&[(2, 0), (1, 0)]),
    )
    .await;
    let registry = registry_with(&backend);
    let handle = session_with_handles(&registry).await;

    registry
        .close_registered(&handle, close_any())
        .await
        .expect("三个句柄都被确认终结");

    assert_eq!(
        backend.finalize_calls(),
        2,
        "同一资源上的句柄必须合并，不能一个一批"
    );
    let traces = backend.traces().await;
    assert_eq!(traces.finalized[0].resource_id, "res_a");
    assert_eq!(traces.finalized[0].handles.len(), 2);
    assert_eq!(traces.finalized[1].resource_id, "res_b");
    assert_eq!(
        traces.finalized[1].handles.len(),
        1,
        "批内不得混入别的资源的句柄：那就是把旧句柄提交到新会话"
    );
}

#[tokio::test(start_paused = true)]
async fn 回滚关闭把终结处置写成回滚() {
    let backend = ScriptedBackend::new(
        BackendPlan::default()
            .handles(vec![handle_ref("h1", "res_a")])
            .finalizes(1, 0),
    )
    .await;
    let registry = registry_with(&backend);
    let handle = session_with_handles(&registry).await;

    registry
        .close_registered(&handle, CloseMode::RollbackAndClose)
        .await
        .expect("关闭必须成功");

    assert_eq!(
        backend.traces().await.finalized[0].disposition,
        HandleDisposition::Rollback,
        "回滚关闭不得把句柄提交出去"
    );
}

#[tokio::test(start_paused = true)]
async fn 有活句柄要求无事务关闭当场拒绝且零后端动作() {
    let backend = ScriptedBackend::new(
        BackendPlan::default()
            .handles(vec![handle_ref("h1", "res_a")])
            .finalizes(1, 0),
    )
    .await;
    let registry = registry_with(&backend);
    let handle = session_with_handles(&registry).await;

    let error = registry
        .close_registered(&handle, CloseMode::RequireNoTransaction)
        .await
        .expect_err("有活句柄时要求无事务关闭必须被拒");
    match error {
        RuntimeError::CloseRejected(reason) => {
            assert_eq!(reason, "transactionInProgress")
        }
        other => panic!("期望 CloseRejected(transactionInProgress)，实际 {other:?}"),
    }

    assert_eq!(
        backend.finalize_calls(),
        0,
        "派发前拒绝不得留下任何半途动作"
    );
    assert_eq!(backend.close_calls(), 0);
    assert!(
        registry.is_registered(&db_session_id()),
        "被拒的关闭不得注销会话"
    );
    assert_eq!(
        registry.remaining_quota(),
        SESSION_LIMIT - 1,
        "被拒的关闭不得归还额度"
    );
}

#[tokio::test(start_paused = true)]
async fn 无登记句柄时直接关资源不经终结() {
    let backend = ScriptedBackend::new(BackendPlan::default()).await;
    let registry = registry_with(&backend);
    let view = register_ready(&registry, db_session_id()).await;
    let handle = handle_of(&view);

    registry
        .close_registered(&handle, close_any())
        .await
        .expect("没有句柄就没有不可判定的来源");

    assert_eq!(backend.finalize_calls(), 0, "没有句柄就没有可终结的批次");
    assert_eq!(backend.close_calls(), 1);
    assert!(!registry.is_registered(&db_session_id()));
    assert_eq!(registry.remaining_quota(), SESSION_LIMIT);

    let closed = entries_of(&registry, AuditKind::SessionClosed);
    assert_eq!(closed.len(), 1, "正常关闭必须留一条审计");
    assert_eq!(closed[0].outcome, Outcome::Succeeded);
    assert_eq!(closed[0].handle_count, 0);
    assert_eq!(
        closed[0].effect_outcome, None,
        "释放条目不是一次执行的效果判定"
    );
}

#[tokio::test(start_paused = true)]
async fn 宿主登记数与后端确认数对不上时墓碑不得报已关闭() {
    // 宿主登记 2 个句柄，后端只确认终结 1 个：剩下那一个的命运已经不可知。
    let backend = ScriptedBackend::new(
        BackendPlan::default()
            .handles(vec![handle_ref("h1", "res_a"), handle_ref("h2", "res_a")])
            .finalizes(1, 0),
    )
    .await;
    let registry = registry_with(&backend);
    let handle = session_with_handles(&registry).await;

    let error = registry
        .close_registered(&handle, close_any())
        .await
        .expect_err("对不上的账不得报关闭成功");
    match error {
        RuntimeError::SessionLost(cause) => {
            assert_eq!(cause, "registeredHandlesRemained")
        }
        other => panic!("期望 SessionLost(registeredHandlesRemained)，实际 {other:?}"),
    }

    assert_eq!(
        backend.close_calls(),
        1,
        "§9.4「任一失败都关闭」：不可判定也必须真的关掉物理资源"
    );

    let view = registry.session_view(&handle).await.expect("墓碑仍可投影");
    assert_eq!(
        view.state,
        SessionState::Lost,
        "有一步不可判定，墓碑就不得写成 Closed——那等于替一条不可知的链背书"
    );
    assert_eq!(
        view.observed_context.transaction_state,
        TransactionState::Unknown,
        "不可判定时必须停止报 Active（CM-73）"
    );

    let undecided = entries_of(&registry, AuditKind::SessionClosed);
    assert_eq!(undecided.len(), 1);
    assert_eq!(undecided[0].outcome, Outcome::Undecided);
    assert_eq!(undecided[0].error_code, Some("resourceLost"));
    assert_eq!(undecided[0].effect_outcome, Some("unknown"));
}

#[tokio::test(start_paused = true)]
async fn 空闲到期才驱逐_未到期一律不动手() {
    let backend = ScriptedBackend::new(BackendPlan::default()).await;
    let registry = registry_with(&backend);
    let _view = register_ready(&registry, db_session_id()).await;

    assert!(
        registry
            .evict_idle_at(IDLE_DEADLINE_MS - 1)
            .await
            .is_empty(),
        "未到期的会话不得被驱逐"
    );
    assert_eq!(backend.close_calls(), 0);
    assert!(registry.is_registered(&db_session_id()));

    let evicted = registry.evict_idle_at(IDLE_DEADLINE_MS).await;
    assert_eq!(evicted.len(), 1, "到期即驱逐");
    assert_eq!(evicted[0].state, SessionState::Closed);
    assert_eq!(backend.close_calls(), 1);
    assert!(!registry.is_registered(&db_session_id()), "驱逐后必须注销");
    assert_eq!(
        registry.remaining_quota(),
        SESSION_LIMIT,
        "驱逐必须归还额度"
    );

    let kinds: Vec<AuditKind> = registry.audit_log().into_iter().map(|e| e.kind).collect();
    assert!(
        kinds.contains(&AuditKind::SessionEvicted),
        "驱逐与主动关闭是两件事，审计上不能混成一条：实际 {kinds:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn 未设空闲期限的会话永不被驱逐() {
    let backend = ScriptedBackend::new(BackendPlan::default()).await;
    let registry = registry_with(&backend);
    let mut request = open_request(db_session_id());
    request.idle_deadline_ms = None;
    registry
        .register_session(request)
        .await
        .expect("无空闲期限的登记必须成功");

    assert!(
        registry.evict_idle_at(u64::MAX).await.is_empty(),
        "没有期限就没有到期时刻，再大的时钟值也不该驱逐它"
    );
    assert_eq!(backend.close_calls(), 0);
    assert!(registry.is_registered(&db_session_id()));
}

#[tokio::test(start_paused = true)]
async fn 租约失效立即作废_额度扣住不外发_隔离确认后才归还() {
    let backend = ScriptedBackend::new(BackendPlan::default()).await;
    let registry = registry_with(&backend);
    let doomed = register_ready(&registry, db_session_id()).await;
    let mut bystander_request = open_request(other_db_session_id());
    bystander_request.worker_id = WorkerId::new("w_2");
    registry
        .register_session(bystander_request)
        .await
        .expect("别的 worker 的登记必须成功");
    let _ = doomed;

    assert_eq!(registry.remaining_quota(), SESSION_LIMIT - 2);

    let lost = registry.invalidate_worker(&worker_id()).await;
    assert_eq!(lost, vec![db_session_id()], "只作废失效租约名下的会话");
    assert!(!registry.is_registered(&db_session_id()));
    assert!(
        registry.is_registered(&other_db_session_id()),
        "别的 worker 不受牵连"
    );
    assert_eq!(
        registry.remaining_quota(),
        SESSION_LIMIT - 2,
        "作废不归还额度：这些物理连接还在别处活着"
    );
    assert_eq!(registry.stale_quota_for(&worker_id()), 1);
    assert_eq!(registry.stale_quota_for(&WorkerId::new("w_2")), 0);

    let held = entries_of(&registry, AuditKind::QuotaHeldStale);
    assert_eq!(held.len(), 1, "扣住陈旧额度必须留痕");
    assert_eq!(held[0].outcome, Outcome::Undecided);
    assert_eq!(
        held[0].runtime_epoch, 0,
        "额度条目是登记表级事实，没有单会话世代可写"
    );

    assert_eq!(
        registry.confirm_worker_quarantined(&worker_id()),
        1,
        "确认隔离后归还扣住的额度"
    );
    assert_eq!(registry.remaining_quota(), SESSION_LIMIT - 1);
    assert_eq!(registry.stale_quota_for(&worker_id()), 0);
    let released = entries_of(&registry, AuditKind::QuotaReleasedStale);
    assert_eq!(released.len(), 1);
    assert_eq!(released[0].outcome, Outcome::Succeeded);
}

#[test]
fn 归池前检查有登记句柄一律不放行() {
    assert!(ready_to_return_to_pool(0), "账上空了才谈得上归池");
    assert!(!ready_to_return_to_pool(1));
    assert!(!ready_to_return_to_pool(usize::MAX));
}
