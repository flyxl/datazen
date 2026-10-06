//! `closeResource` 的 F11 定点用例（fake-runtime-fixtures.md §4.1 F11、§5.3 规则 2/3/6）。
//!
//! 从 `close_cases.rs` 拆出来的理由不是「凑文件数」，而是 §9.4(b) 的归池判据要求**成对**的
//! 用例（正例 + 三个判负 + 凭证两道关口）已经占满一个文件，叠加 F11 这 7 条会顶破
//! AGENTS.md 的单文件规模线。两个文件的分工：
//! - 本文件钉 **F11 的完整后果**：注入未确认之后，回执、台账、permit、状态机
//!   四处各自是什么，以及重试 / 隔离两条后续路径；
//! - [`super::close_cases`] 钉**判据的输入来源**（实测 vs 宿主声明）与凭证校验的两个
//!   `Err` 出口。
//!
//! 共享的帮助函数都在 [`super`]（`close_request` / `resource_event_names` / `pooled_count`
//! / `closed_count`，归属理由见 `mod.rs` 末尾那段注释）。
//!
//! 全程**不 sleep**；断言一律打在台账与 `assert_no_leak` 的返回值上，不看内存布局。

use super::{
    acquire, close_request, provider, resource_event_names, FakeResourceState, FaultKind,
    ResourceOp,
};
use crate::connection::error::ProviderError;
use crate::connection::execution::EffectOutcome;
use crate::connection::port::{CloseOutcome, ResourceRelease, TransactionOperation};
use crate::connection::session::{HandleKind, SessionState};
use crate::connection::testing::journal::{HandleAction, JournalEntry, ResourceEvent};
use crate::connection::types::HandleId;

// ---------------------------------------------------------------------------
// §4.1 F11：关闭未确认 —— 这条提前返回分支此前**执行 0 次**
// ---------------------------------------------------------------------------

/// F11 的**注入证据**：同一份构造、只差脚本里那一次注入，两条路径必须给出**不同**结论。
///
/// 这条用例存在的全部理由是「判据强度」：基线给出 `Confirmed`，注入给出 `Pending` +
/// `Unknown` + 状态停在 `Closing`。若 `close_resource` 里 F11 那段提前返回整块删掉，
/// 注入会变成一次普通的确认关闭，本用例的 `assert_ne!` 当场转红 —— 也就是说
/// **这条分支被删掉时这里必然点红**，而不是「全绿但少覆盖一条分支」。
#[test]
fn f11_an_injected_close_unconfirmed_is_never_reported_as_a_confirmed_release() {
    let baseline = provider();
    let base_res = acquire(&baseline).expect("基线 acquire 必须成功");
    let base = baseline
        .close_resource(&close_request(&baseline, &base_res))
        .expect("基线 close 必须成功");

    let provider = provider();
    provider.script().once(
        ResourceOp::Close,
        FaultKind::CloseUnconfirmed {
            reason: "F11 关闭握手超时",
        },
    );
    let acquired = acquire(&provider).expect("注入不应影响 acquire");
    let receipt = provider
        .close_resource(&close_request(&provider, &acquired))
        .expect("F11 是**正常返回值**，不是错误：未证实关闭也要交出一个结论");

    // §5.1：未证实关闭不能伪称预算已回收。
    assert_eq!(
        receipt.resource_release,
        ResourceRelease::Pending,
        "F11 必须报 Pending，实际 {:?}",
        receipt.resource_release
    );
    assert_eq!(receipt.effect_outcome, EffectOutcome::Unknown);
    assert_eq!(receipt.state, SessionState::Closing);
    assert_eq!(receipt.db_session_id, base_res.session.handle.db_session_id);
    assert_ne!(
        receipt.resource_release, base.resource_release,
        "注入前后必须给出**不同**结论，否则这条用例恒真（也说明 F11 分支没被执行）"
    );
    assert_ne!(receipt.effect_outcome, base.effect_outcome);
    // 脚本额度必须被**这一次** closeResource 消费掉，不留下「注入了但没人碰」。
    assert_eq!(
        provider.script().pending(ResourceOp::Close),
        0,
        "注入的 F11 必须被这次 closeResource 消费"
    );
}

/// F11 的**台账后果**：§5.3 规则 3 —— `CloseUnconfirmed` 时 permit 余额**不变**，
/// 且这条路径上一次归池都不许发生。
///
/// 这一条钉的是「资源留在预算占用里」这件事**必须留下证据**：只记 `CloseUnconfirmed`
/// 而不写 `Closed`、不写 `ReturnedToPool`、不写那条 `-1`。任何一条多写出来都是错的：
/// - 多写 `Closed` ⇒ §5.3 规则 2 会让读者以为 permit 已归还；
/// - 多写 `ReturnedToPool` ⇒ §9.4 的归池前置根本没判过。
#[test]
fn f11_close_unconfirmed_keeps_the_permit_and_records_exactly_one_event() {
    let provider = provider();
    provider.script().once(
        ResourceOp::Close,
        FaultKind::CloseUnconfirmed {
            reason: "F11 未确认",
        },
    );
    let acquired = acquire(&provider).expect("acquire 必须成功");
    provider
        .close_resource(&close_request(&provider, &acquired))
        .expect("F11 必须返回结论而不是错误");

    let events: Vec<&'static str> = resource_event_names(&provider, &acquired.resource_id);
    assert_eq!(
        events,
        vec!["Created", "OpeningReady", "CloseUnconfirmed"],
        "F11 的台账形状是固定的：停在未确认，不写 Closed / ReturnedToPool。实际 {events:?}"
    );
    // §5.3 规则 3：余额不变 —— permit 还在手上。
    assert_eq!(
        provider.journal().permit_balance(),
        1,
        "未确认关闭不得归还 permit"
    );
    assert_eq!(provider.journal().permits_returned(), 0);
    // 资源仍占用预算（I2 因此**不该**收口 —— 这正是 F11 要暴露的状态）。
    assert_eq!(
        provider.live_resources(),
        vec![acquired.resource_id.clone()],
        "未确认关闭的资源必须仍在预算占用集合里"
    );
    assert_eq!(
        provider.journal().permit_balance(),
        1,
        "规则 3 的「余额不变」在 permit_balance 上必须看得见"
    );
    // 台账本身在「live 非空 + 余额 1」这个形状上是干净的：变化点规则不许误报。
    assert_eq!(
        provider.journal().assert().change_point_violations(),
        Vec::<String>::new(),
        "F11 形状下 §5.3 变化点断言必须仍然成立"
    );
    // 但 I1（收支相抵）在这个形状上**必然**不成立 —— 这条断言防止「I1 检查恒真」。
    let ledger = provider.journal().assert().ledger_violations();
    assert!(
        ledger.iter().any(|v| v.contains("I1")),
        "未确认关闭时 I1 必须判负（permit 未归还），实际 {ledger:?}"
    );
}

/// F11 与 `CloseOutcome::CloseUnconfirmed` 的**关系**（§4.1 核实结论）：
/// **两条不同路径、同一个结论口径** —— 前者是注入原因，后者是从状态机派生的结论。
///
/// 这条用例把结论钉成可执行事实，而不是留在注释里：注入 F11 之后，资源侧的
/// `close_outcome()` 必须报 `CloseUnconfirmed`（与回执 `Pending` 同源），
/// 而**基线**关闭之后同一个方法必须报 `Closed`（与回执 `Confirmed` 同源）。
/// 两者由 `prepare_close` 一次性交出（`CloseAttempt::close_outcome`），
/// `ops.rs` 只映射、不再第二次判 `state == Closed`。
#[test]
fn f11_and_the_derived_close_outcome_never_disagree_with_the_receipt() {
    // 基线：确认关闭 ⇒ 状态机与回执都说「Closed / Confirmed」。
    let baseline = provider();
    let clean = acquire(&baseline).expect("acquire 必须成功");
    let clean_receipt = baseline
        .close_resource(&close_request(&baseline, &clean))
        .expect("基线 close 必须成功");
    assert_eq!(clean_receipt.resource_release, ResourceRelease::Confirmed);
    assert_eq!(
        baseline
            .resource(&clean.resource_id)
            .expect("资源必须还在")
            .close_outcome(),
        CloseOutcome::Closed,
        "确认关闭之后，状态机派生的结论必须与回执一致"
    );

    // 注入：未确认 ⇒ 状态机与回执都说「CloseUnconfirmed / Pending」。
    let provider = provider();
    provider.script().once(
        ResourceOp::Close,
        FaultKind::CloseUnconfirmed {
            reason: "F11 一致性",
        },
    );
    let acquired = acquire(&provider).expect("acquire 必须成功");
    let receipt = provider
        .close_resource(&close_request(&provider, &acquired))
        .expect("F11 必须返回结论");
    assert_eq!(receipt.resource_release, ResourceRelease::Pending);
    let slot = provider
        .resource(&acquired.resource_id)
        .expect("资源必须还在");
    assert_eq!(
        slot.close_outcome(),
        CloseOutcome::CloseUnconfirmed,
        "未确认关闭之后，状态机派生的结论必须与回执一致"
    );
    // 「两条路径」的证据：CloseOutcome 这一支**不是**F11 专属的 —— 隔离（F8）走的是
    // 完全不同的代码路径（`transaction.rs` 的 quarantine），派生结论同样是未确认。
    // 所以回执的口径必须取自状态机，而不是「这次有没有注入」。
    let quarantined = super::provider();
    let acquired_q = acquire(&quarantined).expect("acquire 必须成功");
    quarantined
        .transaction_operation(&acquired_q.handle, TransactionOperation::Begin)
        .expect("begin 必须成功");
    quarantined.script().once(
        ResourceOp::Transaction,
        FaultKind::RollbackFailed {
            reason: "F8 隔离".to_owned(),
        },
    );
    let _ = quarantined.transaction_operation(
        &acquired_q.handle,
        TransactionOperation::Rollback(HandleId::new("hl-none")),
    );
    assert_eq!(
        quarantined
            .resource(&acquired_q.resource_id)
            .expect("资源必须还在")
            .close_outcome(),
        CloseOutcome::CloseUnconfirmed,
        "隔离资源（无 F11 注入）也必须派生出「未确认关闭」：两条路径同一个口径"
    );
}

/// F11 的**状态形状**：资源停在 `Closing`、`protocol_drained = false`，
/// 且句柄**不得**被注销 —— 未确认的关闭不能伪称「句柄随资源一起死了」。
///
/// 后半句是这条用例的独立判据：旧实现把注销留在提前返回**之后**（本来就不会执行到），
/// 重构成 `prepare_close` 时如果顺手把 unconfirmed 分支也去 clear 句柄，本用例当场转红。
#[test]
fn f11_leaves_the_resource_closing_with_an_undrained_protocol_and_live_handles() {
    let provider = provider();
    provider.script().once(
        ResourceOp::Close,
        FaultKind::CloseUnconfirmed {
            reason: "F11 状态形状",
        },
    );
    let acquired = acquire(&provider).expect("acquire 必须成功");
    // 资源上挂一个真实登记句柄（§6.5），这才是「关闭未确认」的现场。
    let opened = provider
        .transaction_operation(&acquired.handle, TransactionOperation::Begin)
        .expect("begin 必须成功");
    let _ = opened;
    provider
        .register_handle(
            &acquired.resource_id,
            HandleKind::Cursor,
            HandleId::new("hl_f11_live"),
            None,
        )
        .expect("登记必须成功");
    assert_eq!(provider.registered_handles(&acquired.resource_id), 1);

    let receipt = provider
        .close_resource(&close_request(&provider, &acquired))
        .expect("F11 必须返回结论");
    assert_eq!(receipt.state, SessionState::Closing);

    let slot = provider
        .resource(&acquired.resource_id)
        .expect("资源必须还在");
    assert_eq!(
        slot.state,
        FakeResourceState::Closing,
        "F11 之后资源必须停在 Closing，不能伪称 Closed"
    );
    assert!(
        !slot.protocol_drained,
        "F11 之后排空证据必须撤回（归池前置之一不成立）"
    );
    assert_eq!(
        provider.registered_handles(&acquired.resource_id),
        1,
        "F11 **不得**注销句柄：未确认的关闭没资格宣称句柄已随资源死亡"
    );
    assert!(
        !slot.can_return_to_pool(),
        "§9.4：未确认关闭的资源前置条件必然不满足"
    );
    // permit 也没还（同 §5.3 规则 3），所以这张资源还在占用预算。
    assert_eq!(provider.journal().permit_balance(), 1);
    assert!(slot.accounting.occupied, "记账锚点必须仍标记为占用");
}

/// F11 之后**不得**留下「第二次关闭就万事大吉」的误导：重试路径必须真的把资源关掉，
/// 并且台账上「未确认那一次」与「确认那一次」两条事件都读得出来、顺序正确。
///
/// 这是任务点名要的那条「后续（隔离/重试路径）不会被这条状态误导」：
/// - permit 归还必须**恰好一次**（`Accounting::release()` 的至多一次保证）；
/// - 重试走的是同一条 CM-74 顺序，句柄在关闭前注销；
/// - `CloseUnconfirmed` 排在 `Closed` **之前**（顺序判据，不是「都发生过」）。
#[test]
fn f11_retry_confirms_the_close_and_returns_the_permit_exactly_once() {
    let provider = provider();
    provider.script().once(
        ResourceOp::Close,
        FaultKind::CloseUnconfirmed {
            reason: "F11 重试"
        },
    );
    let acquired = acquire(&provider).expect("acquire 必须成功");
    provider
        .register_handle(
            &acquired.resource_id,
            HandleKind::Cursor,
            HandleId::new("hl_f11_retry"),
            None,
        )
        .expect("登记必须成功");

    let first = provider
        .close_resource(&close_request(&provider, &acquired))
        .expect("第一次关闭（未确认）必须返回结论");
    assert_eq!(first.resource_release, ResourceRelease::Pending);

    // 宿主按 §6.5 先把句柄注销掉，再重试关闭 —— 这才是「不被误导」的完整旅程。
    provider
        .close_handle(
            &acquired.resource_id,
            &HandleId::new("hl_f11_retry"),
            "重试前注销",
        )
        .expect("注销必须成功");
    let second = provider
        .close_resource(&close_request(&provider, &acquired))
        .expect("第二次关闭必须成功");
    assert_eq!(second.resource_release, ResourceRelease::Confirmed);
    assert_eq!(second.state, SessionState::Closed);

    let events: Vec<&'static str> = resource_event_names(&provider, &acquired.resource_id);
    assert_eq!(
        events,
        vec!["Created", "OpeningReady", "CloseUnconfirmed", "Closed"],
        "未确认与确认两次关闭都必须在台账上留痕且顺序正确。实际 {events:?}"
    );
    // permit 收支：+1 之后只有**一笔** -1（幂等关闭不二次归还，§4.3 I1）。
    assert_eq!(provider.journal().permit_balance(), 0);
    assert_eq!(provider.journal().permits_returned(), 1);
    let deltas: Vec<i32> = provider
        .journal()
        .entries()
        .iter()
        .filter_map(|entry| match entry {
            JournalEntry::Permit { delta, .. } => Some(*delta),
            _ => None,
        })
        .collect();
    assert_eq!(deltas, vec![1, -1], "permit 必须一笔 +1、恰好一笔 -1");
    // 句柄注销排在资源 Closed 之前（CM-74 顺序在重试路径上同样成立）。
    let handle_closed_seq = provider
        .journal()
        .entries()
        .iter()
        .find_map(|entry| match entry {
            JournalEntry::Handle {
                handle_id,
                action: HandleAction::Closed,
                seq,
                ..
            } if handle_id == "hl_f11_retry" => Some(*seq),
            _ => None,
        })
        .expect("重试路径必须注销那个句柄（§5.3 规则 6）");
    let closed_seq = provider
        .journal()
        .entries()
        .iter()
        .find_map(|entry| match entry {
            JournalEntry::Resource {
                resource_id,
                event: ResourceEvent::Closed,
                seq,
                ..
            } if resource_id == &acquired.resource_id => Some(*seq),
            _ => None,
        })
        .expect("第二次关闭必须记 Closed");
    assert!(
        handle_closed_seq < closed_seq,
        "CM-74：句柄注销必须排在资源关闭之前（{} vs {}）",
        handle_closed_seq,
        closed_seq
    );
    // 重试收口之后不得留下任何泄漏。
    assert!(
        provider.live_resources().is_empty(),
        "重试成功之后不得残留预算占用（I2）"
    );
    assert!(
        provider
            .journal()
            .assert()
            .leak_invariant_violations()
            .is_empty(),
        "重试之后 I1–I8 必须收口"
    );
    assert_eq!(
        provider
            .resource(&acquired.resource_id)
            .expect("资源必须还在")
            .close_outcome(),
        CloseOutcome::Closed,
        "派生结论必须跟着最终状态走，不能停在「未确认」"
    );
}

/// F11 的**隔离**后续：未确认关闭之后走隔离，预算占用同样保留（§5.3 规则 3 的两个变体
/// 在同一条资源上叠加时不许把 permit 提前归零）。
///
/// 这条用例防的是「F11 把状态钉成 Closing 之后，隔离路径误以为 permit 还没记过」——
/// 那会写出第二笔 `+1` 或者在守恒式里把这张资源算漏。
#[test]
fn f11_then_quarantine_still_keeps_exactly_one_permit_occupied() {
    let provider = provider();
    provider.script().once(
        ResourceOp::Close,
        FaultKind::CloseUnconfirmed {
            reason: "F11 后隔离",
        },
    );
    let acquired = acquire(&provider).expect("acquire 必须成功");
    provider
        .close_resource(&close_request(&provider, &acquired))
        .expect("F11 必须返回结论");
    assert_eq!(provider.journal().permit_balance(), 1);

    // 隔离（F8 回滚失败）：同一张资源上第二次「保留占用」的变化点。
    provider
        .transaction_operation(&acquired.handle, TransactionOperation::Begin)
        .expect("begin 必须成功");
    provider.script().once(
        ResourceOp::Transaction,
        FaultKind::RollbackFailed {
            reason: "隔离保留占用".to_owned(),
        },
    );
    let rollback = provider.transaction_operation(
        &acquired.handle,
        TransactionOperation::Rollback(HandleId::new("hl-none")),
    );
    assert!(
        matches!(rollback, Err(ProviderError::RollbackFailed(_))),
        "F8 注入必须报 RollbackFailed，实际 {rollback:?}"
    );

    assert_eq!(
        provider.journal().permit_balance(),
        1,
        "F11 + 隔离两次「保留占用」不得把 permit 提前归零"
    );
    let deltas: Vec<i32> = provider
        .journal()
        .entries()
        .iter()
        .filter_map(|entry| match entry {
            JournalEntry::Permit { delta, .. } => Some(*delta),
            _ => None,
        })
        .collect();
    assert_eq!(deltas, vec![1], "整本台账只有一笔 +1，没有任何归还");
    let events: Vec<&'static str> = resource_event_names(&provider, &acquired.resource_id);
    assert_eq!(
        events,
        vec!["Created", "OpeningReady", "CloseUnconfirmed", "Quarantined"],
        "F11 之后再隔离，两条规则 3 事件都必须留痕。实际 {events:?}"
    );
    // §5.3 规则 4 的守恒式在这个形状上必须对得平（分母是 permit 持有者，不是 live 集）。
    assert_eq!(
        provider.journal().assert().change_point_violations(),
        Vec::<String>::new(),
        "F11 + 隔离的形状下规则 4 必须仍然对得平"
    );
}

/// F11 不许把**注入额度**浪费在别的操作身上，也不许让第二张资源误受影响：
/// 故障只命中它自己那一次 `closeResource`（与 `script.rs` 的
/// `a_fault_only_fires_on_its_own_operation` 是**不同**层次的证明 —— 那条只验脚本表，
/// 这条验「fake provider 真的把这条分支走到了」）。
#[test]
fn f11_fires_once_on_its_own_resource_and_leaves_the_others_alone() {
    let provider = provider();
    provider.script().once(
        ResourceOp::Close,
        FaultKind::CloseUnconfirmed {
            reason: "F11 只命中一次",
        },
    );
    let first = acquire(&provider).expect("acquire 必须成功");
    let second = acquire(&provider).expect("acquire 必须成功");

    let r1 = provider
        .close_resource(&close_request(&provider, &first))
        .expect("第一次 close 必须返回结论");
    let r2 = provider
        .close_resource(&close_request(&provider, &second))
        .expect("第二次 close 必须返回结论");

    assert_eq!(
        r1.resource_release,
        ResourceRelease::Pending,
        "注入命中第一次"
    );
    assert_eq!(
        r2.resource_release,
        ResourceRelease::Confirmed,
        "注入只排了一次额度，第二次必须回到基线路径"
    );
    // 注入没命中时，判据与注销都照常走（不是「整个 provider 被 F11 污染」）。
    assert_eq!(
        provider.journal().permit_balance(),
        1,
        "只有第二张的 permit 回来了"
    );
    assert_eq!(provider.live_resources(), vec![first.resource_id.clone()]);
}
