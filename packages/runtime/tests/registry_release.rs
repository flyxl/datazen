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
//! | 归池时 driver 报 Clean 但宿主仍有登记句柄不得算归池成功 | §9.4 步骤 2(b) | §9.4 |
//! | driver 逐批少报且明说还有剩余 | §9.4 步骤 1 注销时机 | §9.4 |
//! | driver 两批确认数正好对平但宿主账未清空 | §9.4 步骤 2(b) 独占 | §9.4 |

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

    // 墓碑本身（`Lost` + `TransactionState::Unknown`）在 actor 内部断言，见
    // `src/registry/actor/tests/release.rs` 的 T10。集成面读不到它，因为
    // §9.4 四步跑完之后登记表已经把这一行摘掉了——和**成功**关闭之后一样：
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
/// * `SessionLost` 是 §9.4 四步**跑完**之后写出来的：句柄已终结、物理资源已关闭、
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
        "§9.4 已经把物理资源关掉了，额度必须原样归还"
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

/// CM-74 归池路径上的宿主检查：driver 报 Clean 但宿主仍有登记句柄时，**不得**算归池成功。
///
/// 这条比上面那个 helper 级断言强一档，也不重复 `宿主登记数与后端确认数对不上时墓碑不得报已关闭`
/// （那条走的是 `close_registered`）。区别在于**出口的形状**：
///
/// * `close_registered` 拿到 `Err` 就自己兜底，R-01 明确要求它注销行、归还额度；
/// * `evict_idle_at` 拿到 `Err` 曾是一句 `else { continue }`——**吞掉**，不 `forget`、
///   不还额度、也不进返回值。可 §9.4 四步此时**已经跑完**：终结请求发过了、物理资源
///   关掉了、绑定作废了。
///
/// 于是留下的是一个「物理资源已死、行还在表里、额度还扣着」的僵尸会话。本用例曾经
/// **刻画**那个现状（三条【现状】断言）；现状已闭合，所以这里钉的是**修好之后**的形状，
/// 判据一个字没减：CM-74 `:1324`「driver 返回 Clean 时若宿主仍有已登记句柄，宿主检查
/// 必须失败（§9.4）」，加上 R-01「已关掉就得摘行还额度」。
///
/// 承重断言五条，缺一不可：
/// 1. 不出现在归池结果里（宿主检查否决的是「算成功」）；
/// 2. §9.4「任一失败都关闭」：物理资源**照关**；
/// 3. 审计写 `Undecided`，不是 `Succeeded`；
/// 4. 行摘除、额度归还，且归还的额度**能再发出去**（R-01：不摘就是永久卡死）；
/// 5. `CloseResource.registered_handles == 2`：宿主把「自己账上还没被确认的那两个」
///    如实带进关闭请求。FU2 之前取样用的是 `drain_handles`（**取走即注销**），这个数
///    **恒为 0**，第 3 步的请求形状因此替缺陷作证；现在它是这条判据在后端请求形状上的
///    可观测面，也是本用例能区分「宿主真的自己数了一遍」与「检查恒真」的地方。
#[tokio::test(start_paused = true)]
async fn 归池时driver报clean但宿主仍有登记句柄不得算归池成功() {
    // 宿主登记 2 个句柄，driver 只确认终结 1 个——它对已交出的句柄没有可见性，
    // 所以这个 Clean 是**空口**的：§9.4「driver 对已交出的句柄没有可见性，
    // 其 Clean 不构成事务终结的证据」。
    let backend = ScriptedBackend::new(
        BackendPlan::default()
            .handles(vec![handle_ref("h1", "res_a"), handle_ref("h2", "res_a")])
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

    // §9.4「任一失败都关闭」：不可判定也必须真的关掉物理资源。
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
        "§9.4 已跑完、关闭已发出 ⇒ 行必须摘除：留着就是一行查得到、能用、却指向已关资源的僵尸登记"
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

/// FU2：driver **逐批少报并明说还有剩余**时，宿主账必须留痕并判失。
///
/// 与上一条的区别是**少报的形状**。上面那条 `finalizes(1, 0)` 里 driver 报
/// `remaining == 0`（自称收干净）而确认数只有 1，失败来自判据 (a)「累计确认数 ≠ 登记数」。
/// 本例把两个句柄分到**两个资源**、逐批回报 `(1, 0)` 与 `(0, 1)`：
/// 第一批整批确认 ⇒ 注销；第二批 driver **明说那一个还活着** ⇒ 不注销。
/// 于是宿主账上剩的这一个既没被确认、也带着 `remaining > 0` 的原话，
/// 判据 (b)「宿主自己数一遍」在这里**独自**起作用——(a) 与 (b) 谁都不是冗余的。
///
/// 为什么必须有这一条：如果注销时机退回「取样即注销」，第二批的 `remaining == 1`
/// 会被账上的空洞盖过去，判据 (b) 又变回恒真式，而 `handlesStillOpen` 与
/// `registeredHandlesRemained` 这两道出口就都只能靠 driver 的口头数字触发。
#[tokio::test(start_paused = true)]
async fn driver逐批少报且明说还有剩余时宿主账必须留痕并判失() {
    let backend = ScriptedBackend::new(
        BackendPlan::default()
            .handles(vec![handle_ref("h1", "res_a"), handle_ref("h2", "res_b")])
            .finalize_sequence(&[(1, 0), (0, 1)]),
    )
    .await;
    let registry = registry_with(&backend);
    let handle = session_with_handles(&registry).await;

    let error = registry
        .close_registered(&handle, close_any())
        .await
        .expect_err("第二批句柄明确还活着 ⇒ 不得算干净收尾");
    match error {
        RuntimeError::SessionLost(cause) => assert_eq!(
            cause, "handlesStillOpen",
            "理由必须来自 driver 自己报的剩余数，而不是被总数对不上抢在前面"
        ),
        other => panic!("期望 SessionLost(handlesStillOpen)，实际 {other:?}"),
    }

    let traces = backend.traces().await;
    assert_eq!(
        traces.finalized.len(),
        2,
        "两个资源两批终结请求，一批都不能少"
    );
    assert_eq!(
        traces.closed_with[0].registered_handles, 1,
        "driver 报剩余的那一个必须还挂在宿主账上，并带进关闭请求：确认掉的注销、没确认的留着"
    );
    assert_eq!(backend.close_calls(), 1, "§9.4 任一失败都关闭");
    assert_eq!(
        registry.remaining_quota(),
        SESSION_LIMIT,
        "R-01：关闭已发出 ⇒ 摘行还额度"
    );
}

/// §9.4 判据 (b) 的**专属**用例：累计确认数正好对平，宿主账却没清空。
///
/// 上一条让判据 (a) 失败（累计确认 1 ≠ 登记 2），这一条把 (a) 摆平：两批各报
/// `(2, 0)` 与 `(0, 0)`，累计确认 `2 + 0 = 2` **正好等于**登记数 2。
/// driver 两批都自称收干净（`remaining == 0`），判据 (a) 于是通过。
///
/// 宿主账上却还挂着两个句柄：第一批只交出去 1 个却报确认 2 个，**多报**，
/// 逐批对不上就不注销；第二批报确认 0 个，**少报**，同样不注销。
/// 于是只有判据 (b)「释放后宿主自己数一遍」能叫停它——理由是
/// `registeredHandlesRemained`，而不是 driver 自己的 `handlesStillOpen`
/// （本例两批 `remaining` 都是 0，那句话 driver 根本没说过）。
///
/// 为什么这条必须独立成例：(a) 与 (b) 都不是对方的冗余。少报会让 (a) 先响，
/// 本例则构造出**只有 (b) 会响**的形状。反过来，若把 (b) 删掉（只留总数比较），
/// 本例会绿——多报与少报相抵，宿主账上其实还留着两个活句柄，归池却判成功。
#[tokio::test(start_paused = true)]
async fn driver两批确认数正好对平但宿主账未清空时仍判不可知() {
    let backend = ScriptedBackend::new(
        BackendPlan::default()
            .handles(vec![handle_ref("h1", "res_a"), handle_ref("h2", "res_b")])
            // 第一批（res_a，1 个句柄）报确认 2 个：**多报**；
            // 第二批（res_b，1 个句柄）报确认 0 个：**少报**。
            // 累计 2 == 登记 2 ⇒ 判据 (a) 通过；两批 remaining 都是 0 ⇒ driver
            // 从头到尾没说过「还有句柄活着」。
            .finalize_sequence(&[(2, 0), (0, 0)]),
    )
    .await;
    let registry = registry_with(&backend);
    let handle = session_with_handles(&registry).await;

    let error = registry
        .close_registered(&handle, close_any())
        .await
        .expect_err("累计确认数对得上，但宿主账上仍有活句柄 ⇒ 不得算干净收尾");
    match error {
        RuntimeError::SessionLost(cause) => assert_eq!(
            cause, "registeredHandlesRemained",
            "理由必须来自宿主自己数账，不能借 driver 的 `remaining`（本例恒为 0）"
        ),
        other => panic!("期望 SessionLost(registeredHandlesRemained)，实际 {other:?}"),
    }

    let traces = backend.traces().await;
    assert_eq!(
        traces.finalized.len(),
        2,
        "两个资源两批终结请求，一批都不能少"
    );
    assert_eq!(
        traces.closed_with[0].registered_handles, 2,
        "多报/少报相抵时宿主账一个都不能注销，两个句柄必须如实带进关闭请求"
    );
    assert_eq!(backend.close_calls(), 1, "§9.4 任一失败都关闭");
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
            .handles(vec![handle_ref("h1", "res_a"), handle_ref("h2", "res_a")])
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
