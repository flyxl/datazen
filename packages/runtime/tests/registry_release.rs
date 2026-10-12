//! 释放顺序 / 句柄登记与绑定 / 陈旧额度 —— 登记表集成面。
//!
//! 释放这一段的价值全在**顺序**上，而顺序只能从「后端收到的调用序列」读出来，
//! 不能从返回值读：四个步骤都做完，返回值只是一个终态。因此这里的断言集中在
//! 两处：每个 `FinalizeHandles` 落在**登记时那个资源**上，每个 `CloseResource`
//! 落在**当前物理资源**上；以及登记数与后端确认数对不上时，墓碑不得写成「已关闭」。
//!
//! | 用例 | 分支 |
//! | --- | --- |
//! | 句柄先在原资源上终结再关物理资源 | 四步顺序 |
//! | 同资源的多个句柄合并为一个终结批次 | 按 resource_id 分组 |
//! | 回滚关闭把 disposition 写成 Rollback | 模式决定处置 |
//! | 有活句柄要求无事务关闭当场拒绝 | 派发前拒绝 |
//! | 无登记句柄时直接关资源 | 无需终结 |
//! | 登记数对不上时墓碑不得报已关闭 | 不可判定 → `Lost` |
//! | 空闲到期才驱逐并归还额度 | 注入时钟 |
//! | 未设空闲期限的会话永不被驱逐 | 无期限即不驱逐 |
//! | 租约失效作废后额度扣住不外发 | 两段式 |
//! | 归池前的登记句柄检查一律不放行 | 归池前 |
//! | 归池时 driver 报 Clean 但宿主仍有登记句柄不得算归池成功 | 归池时 |
//! | driver 报告仍有句柄时宿主账保留整批 | 逐批注销时机 |
//! | driver 多报终结数时宿主账未清空 | 登记数不可被多报抵消 |

#![allow(dead_code)]
mod registry_fixtures;

use std::sync::Arc;

use datazen_runtime::connection::{CloseMode, RuntimeError, SessionHandle, SessionState, WorkerId};
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
    // 执行在 `res_1` 上签发两个句柄；释放必须先在该资源上终结它们，再关闭资源。
    let backend = ScriptedBackend::new(
        BackendPlan::default()
            .handles(vec![handle_ref("h1", "res_1"), handle_ref("h2", "res_1")])
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
        traces.finalized[0].resource_id, "res_1",
        "终结必须落在签发句柄的资源上"
    );
    assert_eq!(traces.finalized[0].handles.len(), 2);
    assert_eq!(
        traces.closed_with[0].resource_id, "res_1",
        "句柄终结后才关闭当前物理资源"
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
async fn 同资源的多个句柄合并为一个终结批次() {
    let backend = ScriptedBackend::new(
        BackendPlan::default()
            .handles(vec![
                handle_ref("h1", "res_1"),
                handle_ref("h2", "res_1"),
                handle_ref("h3", "res_1"),
            ])
            .finalizes(3, 0),
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
        1,
        "同一资源上的句柄必须合并为一批"
    );
    let traces = backend.traces().await;
    assert_eq!(traces.finalized[0].resource_id, "res_1");
    assert_eq!(traces.finalized[0].handles.len(), 3);
}

#[tokio::test(start_paused = true)]
async fn 回滚关闭把终结处置写成回滚() {
    let backend = ScriptedBackend::new(
        BackendPlan::default()
            .handles(vec![handle_ref("h1", "res_1")])
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
            .handles(vec![handle_ref("h1", "res_1")])
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
            .handles(vec![handle_ref("h1", "res_1"), handle_ref("h2", "res_1")])
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
        "「任一失败都关闭」：不可判定也必须真的关掉物理资源"
    );

    // 墓碑本身（`Lost` + `TransactionState::Unknown`）在 actor 内部断言，见
    // `src/registry/actor/tests/release.rs` 的 T10。集成面读不到它，因为
    // 四步跑完之后登记表已经把这一行摘掉了——和**成功**关闭之后一样：
    // 登记表只管活会话，墓碑归 actor。
    assert!(
        registry.session_view(&handle).await.is_err(),
        "四步已跑完 ⇒ 行必须摘除，否则额度永久卡死（R-01）"
    );

    let undecided = entries_of(&registry, AuditKind::SessionClosed);
    assert_eq!(undecided.len(), 1);
    assert_eq!(undecided[0].outcome, Outcome::Undecided);
    assert_eq!(undecided[0].error_code, Some("resourceLost"));
    assert_eq!(undecided[0].effect_outcome, Some("unknown"));
}

/// R-01：不可判定的关闭**已经关掉了**，登记表必须照样注销并归还额度。
///
/// 两种 `Err` 不是一回事：
///
/// * `CloseRejected` 是**派发前**的拒绝，物理资源原封不动 → 行与额度都留着
///   （见 `有活句柄要求无事务关闭当场拒绝且零后端动作`）；
/// * `SessionLost` 是四步**跑完**之后写出来的：句柄已终结、物理资源已关闭、
///   绑定已作废。不可判定的是某个句柄的命运，不是「有没有关掉」。
///
/// 这里要钉的正是「同一种失败形状」下的**两半账**：修复前额度被永久扣死
/// （`SESSION_LIMIT` 再也回不到满），修复过头又会凭空造额度。前者表现为额度再也回不来，
/// 所以本用例把「还能再登记一个会话」也一起断掉——只查 `remaining_quota` 的数字
/// 未必够，**证明这个额度真的能发出去**才算。
#[tokio::test(start_paused = true)]
async fn 不可判定的关闭仍必须注销并归还额度() {
    let backend = ScriptedBackend::new(
        BackendPlan::default()
            .handles(vec![handle_ref("h1", "res_1"), handle_ref("h2", "res_1")])
            .finalizes(1, 0),
    )
    .await;
    let registry = registry_with(&backend);
    let handle = session_with_handles(&registry).await;

    let error = registry
        .close_registered(&handle, close_any())
        .await
        .expect_err("对不上的账不得报关闭成功");
    assert!(
        matches!(error, RuntimeError::SessionLost(_)),
        "只有不可判定才走注销这条路，实际 {error:?}"
    );

    assert!(
        registry.registered_ids().is_empty(),
        "已关闭的会话不得继续留在登记表里"
    );
    assert!(
        !registry.is_registered(&db_session_id()),
        "在册判定必须同步失效，否则重连会撞上死行"
    );
    assert_eq!(
        registry.remaining_quota(),
        SESSION_LIMIT,
        "已经把物理资源关掉了，额度必须原样归还"
    );

    // 额度不只是数字：它得真的能再发一次登记。
    let reused = registry
        .register_session(open_request(other_db_session_id()))
        .await
        .expect("归还的额度必须能立刻发出去");
    assert_eq!(
        registry.remaining_quota(),
        SESSION_LIMIT - 1,
        "归还的额度只能被下一次真实登记消耗一次"
    );
    let _ = reused;
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

    // R-02：一次作废 = **一条** `SessionInvalidated`。修复前释放例程和
    // `invalidate_worker` 各发一条，字段逐字相同、只有自增 `id` 不同——
    // 按条数统计作废次数的调用方会把 1 次作废读成 2 次。
    let invalidated = entries_of(&registry, AuditKind::SessionInvalidated);
    assert_eq!(
        invalidated.len(),
        1,
        "同一次作废不得留下两条字段相同的审计，实际 {invalidated:#?}"
    );
    assert_eq!(
        invalidated[0].runtime_epoch,
        doomed.handle.runtime_epoch.get(),
        "作废条目必须挂在被作废的那个会话世代上"
    );

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

/// 归池路径上的宿主检查：driver 报 Clean 但宿主仍有登记句柄时，**不得**算归池成功。
///
/// 这条比上面那个 helper 级断言强一档，也不重复 `宿主登记数与后端确认数对不上时墓碑不得报已关闭`
/// （那条走的是 `close_registered`）。区别在于**出口的形状**：
///
/// * `close_registered` 拿到 `Err` 就自己兜底，R-01 明确要求它注销行、归还额度；
/// * `evict_idle_at` 拿到 `Err` 曾是一句 `else { continue }`——**吞掉**，不 `forget`、
///   不还额度、也不进返回值。可四步此时**已经跑完**：终结请求发过了、物理资源
///   关掉了、绑定作废了。
///
/// 于是留下的是一个「物理资源已死、行还在表里、额度还扣着」的僵尸会话。本用例曾经
/// **刻画**那个现状（三条【现状】断言）；现状已闭合，所以这里钉的是**修好之后**的形状，
/// 判据一个字没减：「driver 返回 Clean 时若宿主仍有已登记句柄，宿主检查必须失败」，
/// 加上 R-01「已关掉就得摘行还额度」。
///
/// 承重断言五条，缺一不可：
/// 1. 不出现在归池结果里（宿主检查否决的是「算成功」）；
/// 2. 「任一失败都关闭」：物理资源**照关**；
/// 3. 审计写 `Undecided`，不是 `Succeeded`；
/// 4. 行摘除、额度归还，且归还的额度**能再发出去**（R-01：不摘就是永久卡死）；
/// 5. `CloseResource.registered_handles == 2`：宿主把「自己账上还没被确认的那两个」
///    如实带进关闭请求。FU2 之前取样用的是 `drain_handles`（**取走即注销**），这个数
///    **恒为 0**，第 3 步的请求形状因此替缺陷作证；现在它是这条判据在后端请求形状上的
///    可观测面，也是本用例能区分「宿主真的自己数了一遍」与「检查恒真」的地方。
#[tokio::test(start_paused = true)]
async fn 归池时driver报clean但宿主仍有登记句柄不得算归池成功() {
    // 宿主登记 2 个句柄，driver 只确认终结 1 个——它对已交出的句柄没有可见性，
    // 所以这个 Clean 是**空口**的：「driver 对已交出的句柄没有可见性，
    // 其 Clean 不构成事务终结的证据」。
    let backend = ScriptedBackend::new(
        BackendPlan::default()
            .handles(vec![handle_ref("h1", "res_1"), handle_ref("h2", "res_1")])
            .finalizes(1, 0),
    )
    .await;
    let registry = registry_with(&backend);
    let handle = session_with_handles(&registry).await;
    assert_eq!(
        registry.remaining_quota(),
        SESSION_LIMIT - 1,
        "本用例的前提：驱逐之前这一格额度确实被会话占着"
    );

    // 注入时钟推到空闲期限之后，逼出驱逐。
    let evicted = registry.evict_idle_at(IDLE_DEADLINE_MS).await;

    assert!(
        evicted.is_empty(),
        "宿主检查失败 ⇒ 不得出现在归池结果里；实际带回了 {} 项",
        evicted.len()
    );

    // 「任一失败都关闭」：不可判定也必须真的关掉物理资源。
    assert_eq!(
        backend.close_calls(),
        1,
        "归池路径同样必须关闭物理资源——宿主检查否决的是「算成功」，不是「去关掉」"
    );

    // 审计写的是不可判定，不是成功。
    let entries = entries_of(&registry, AuditKind::SessionEvicted);
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].outcome,
        Outcome::Undecided,
        "登记数与 driver 确认数对不上 ⇒ 审计不得写 Succeeded"
    );

    // ↓↓↓ FU1 闭合的三条：不留僵尸行、额度归还、僵尸视图不再对外可查。
    assert!(
        !registry.is_registered(&db_session_id()),
        "已跑完、关闭已发出 ⇒ 行必须摘除：留着就是一行查得到、能用、却指向已关资源的僵尸登记"
    );
    assert_eq!(
        registry.remaining_quota(),
        SESSION_LIMIT,
        "额度必须归还，否则每驱逐失败一次就永久卡死一格（与 close_registered 的 R-01 同一个病）"
    );
    assert!(
        registry.session_view(&handle).await.is_err(),
        "已注销的会话不得继续对外可查、可发执行"
    );
    // 归还的额度得**真的**能再发出去：只查数字未必够。
    registry
        .register_session(open_request(other_db_session_id()))
        .await
        .expect("归还的额度必须能立刻发出去");
    assert_eq!(registry.remaining_quota(), SESSION_LIMIT - 1);

    // ↓↓↓ FU2 闭合的那条：关闭请求带的是宿主**释放后**的真实账。
    // driver 确认 1 个 / 宿主登记 2 个 ⇒ 这一批**整批**都不算确认（哪个被漏掉不可知），
    // 所以两个都还挂在账上，宿主检查据它们失败。
    let traces = backend.traces().await;
    assert_eq!(
        traces.closed_with[0].registered_handles, 2,
        "driver 少报 ⇒ 宿主账上那批未经确认的句柄必须仍然挂着，并如实带进关闭请求；\
         取样若是 drain（取走即注销），这个数恒为 0，判据就只能在纸面上成立"
    );
    assert_eq!(
        traces.finalized[0].handles.len(),
        2,
        "改成「确认后才注销」不得少发终结请求：第 1 步仍按登记时的资源带上全部句柄"
    );
}

/// driver 明确报告批次仍有一个句柄时，宿主必须保留整批登记项并判失。
/// 一次执行返回的句柄绑定到当前物理资源；未逐项确认整个批次前，不能只注销其中一部分。
#[tokio::test(start_paused = true)]
async fn driver报告仍有句柄时宿主账保留整批并判失() {
    let backend = ScriptedBackend::new(
        BackendPlan::default()
            .handles(vec![handle_ref("h1", "res_1"), handle_ref("h2", "res_1")])
            .finalizes(1, 1),
    )
    .await;
    let registry = registry_with(&backend);
    let handle = session_with_handles(&registry).await;

    let error = registry
        .close_registered(&handle, close_any())
        .await
        .expect_err("driver 报告仍有句柄 ⇒ 不得算干净收尾");
    match error {
        RuntimeError::SessionLost(cause) => assert_eq!(
            cause, "handlesStillOpen",
            "理由必须来自 driver 自己报的剩余数，而不是被总数对不上抢在前面"
        ),
        other => panic!("期望 SessionLost(handlesStillOpen)，实际 {other:?}"),
    }

    let traces = backend.traces().await;
    assert_eq!(traces.finalized.len(), 1, "同一资源的句柄只发一个终结批次");
    assert_eq!(
        traces.closed_with[0].registered_handles, 2,
        "批次未全部确认时宿主必须保留整批句柄，并带入关闭请求"
    );
    assert_eq!(backend.close_calls(), 1, "任一失败都关闭");
    assert_eq!(
        registry.remaining_quota(),
        SESSION_LIMIT,
        "R-01：关闭已发出 ⇒ 摘行还额度"
    );
}

/// 驱动多报终结数时，宿主不得注销任何未被逐项确认的句柄，即使它声称
/// `remaining == 0`。宿主账必须保留整批句柄，关闭请求也必须带上真实计数。
#[tokio::test(start_paused = true)]
async fn driver多报终结数时宿主账未清空仍判不可知() {
    let backend = ScriptedBackend::new(
        BackendPlan::default()
            .handles(vec![handle_ref("h1", "res_1"), handle_ref("h2", "res_1")])
            .finalizes(3, 0),
    )
    .await;
    let registry = registry_with(&backend);
    let handle = session_with_handles(&registry).await;

    let error = registry
        .close_registered(&handle, close_any())
        .await
        .expect_err("驱动多报终结数，宿主账仍有句柄 ⇒ 不得算干净收尾");
    match error {
        RuntimeError::SessionLost(cause) => assert_eq!(
            cause, "registeredHandlesRemained",
            "宿主账上的句柄未全部得到逐项确认"
        ),
        other => panic!("期望 SessionLost(registeredHandlesRemained)，实际 {other:?}"),
    }

    let traces = backend.traces().await;
    assert_eq!(traces.finalized.len(), 1, "同一资源只发送一个终结批次");
    assert_eq!(
        traces.closed_with[0].registered_handles, 2,
        "多报不得注销句柄，关闭请求必须携带真实的宿主句柄数"
    );
    assert_eq!(backend.close_calls(), 1, "任一失败都关闭");
    assert_eq!(
        registry.remaining_quota(),
        SESSION_LIMIT,
        "R-01：关闭已发出 ⇒ 摘行还额度"
    );
}

/// 归池路径的对照组：driver 如实终结全部句柄 ⇒ 正常归池。
///
/// 没有这条，上面那条就只是「归池总是失败」也能变绿。对照组把差异钉在
/// **登记数与 driver 确认数是否一致**这一个变量上。
#[tokio::test(start_paused = true)]
async fn 归池时driver如实终结全部句柄则正常归还额度() {
    let backend = ScriptedBackend::new(
        BackendPlan::default()
            .handles(vec![handle_ref("h1", "res_1"), handle_ref("h2", "res_1")])
            .finalizes(2, 0),
    )
    .await;
    let registry = registry_with(&backend);
    let handle = session_with_handles(&registry).await;
    assert_eq!(registry.remaining_quota(), SESSION_LIMIT - 1);

    let evicted = registry.evict_idle_at(IDLE_DEADLINE_MS).await;

    assert_eq!(evicted.len(), 1, "如实终结 ⇒ 归池成功");
    assert_eq!(
        evicted[0].state,
        SessionState::Closed,
        "终态必须是 Closed，不是 Lost——lost 是上一条那条 driver 少报时的判定"
    );
    assert_eq!(backend.close_calls(), 1);
    assert!(
        !registry.is_registered(&db_session_id()),
        "归池成功 ⇒ 行必须摘除"
    );
    assert_eq!(
        registry.remaining_quota(),
        SESSION_LIMIT,
        "归池成功 ⇒ 额度必须归还"
    );
    assert!(registry.session_view(&handle).await.is_err());

    let entries = entries_of(&registry, AuditKind::SessionEvicted);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].outcome, Outcome::Succeeded);
}
