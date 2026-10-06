//! `closeResource` 的归池判据与凭证校验定点用例（fake-runtime-fixtures.md §4.1 F12/F13、
//! §5.3 规则 2/3/6、connection-management.md §6.5 / §9.4）。
//!
//! 拆出 `close_cases.rs` 的理由不是「凑文件数」，而是 §9.4(b) 这项缺陷的判据要求**成对**的
//! 用例：每个「拒绝」都必须有同构造的「放行」对照，否则「夹具永远拒绝归池」与「判据真的
//! 在判」在断言上长得一模一样（`cm73_threads.rs` 的对照用例是同一条纪律）。
//!
//! 与既有 CM-74 杀手用例的分工：
//! - `harness::cm74_release_order` 钉**释放顺序**（`handle closed` → `Closed` → `permit -1`）
//!   与「句柄非空时不得归池」；
//! - 本文件钉**判据的输入来源**（实测 vs 宿主声明）与**凭证校验的两个 `Err` 出口**。
//!   两组判负方向不同：句柄**多于**宿主声明 ⇒ 本文件的
//!   [`pooling_refuses_a_host_ledger_that_undercounts_the_registered_handles`]；
//!   宿主账本没清、走的是 `harness::close` ⇒ 那三条 CM-74 用例。
//!
//! F11 的那组用例在 [`super::close_unconfirmed`]。
//!
//! 共享的帮助函数都在 [`super`]（`close_request` / `resource_event_names` / `pooled_count`
//! / `closed_count`，归属理由见 `mod.rs` 末尾那段注释）。
//!
//! 全程**不 sleep**；断言一律打在台账与 `assert_no_leak` 的返回值上，不看内存布局。

use super::{
    acquire, close_and_release, close_request, closed_count, owner, pooled_count, provider,
    resource_event_names, FakeResourceState,
};
use crate::connection::error::ProviderError;
use crate::connection::execution::EffectOutcome;
use crate::connection::port::{CloseResourceRequest, ResourceHandle, ResourceRelease};
use crate::connection::session::{HandleKind, SessionState};
use crate::connection::testing::journal::{JournalEntry, ResourceEvent};
use crate::connection::types::{Counter, HandleId, ResourceId};

// ---------------------------------------------------------------------------
// §9.4(b)：归池判据必须**两份账都消费** —— 被调方实测的登记数与宿主声明，且两者一致
// ---------------------------------------------------------------------------

/// 对照**正例**：两份账一致（宿主按 §6.5 如实报 0）、协议已排空 ⇒ **必须**归池。
///
/// 这条用例存在的理由与 `cm73_threads::an_idle_resource_with_no_transaction_still_returns_to_the_pool`
/// 相同：没有它，下面那条「拒绝归池」的判负就分不清是判据在判还是夹具永远拒绝。
/// 它同时钉住 `ReturnedToPool` 记的是**实测**的那一个值（0）。
#[test]
fn a_clean_resource_with_matching_ledgers_returns_to_the_pool() {
    let provider = provider();
    let acquired = acquire(&provider).expect("acquire 必须成功");
    let declared = provider.registered_handles(&acquired.resource_id);
    assert_eq!(declared, 0, "本用例的前提就是干净的账");

    let receipt = provider
        .close_resource(&CloseResourceRequest {
            handle: acquired.handle.clone(),
            registered_handles: declared,
            protocol_drained: true,
        })
        .expect("关闭必须成功");
    assert_eq!(receipt.resource_release, ResourceRelease::Confirmed);

    let pooled: Vec<(bool, usize)> = provider
        .journal()
        .entries()
        .iter()
        .filter_map(|entry| match entry {
            JournalEntry::Resource {
                resource_id,
                event:
                    ResourceEvent::ReturnedToPool {
                        protocol_drained,
                        registered_handles,
                    },
                ..
            } if resource_id == &acquired.resource_id => {
                Some((*protocol_drained, *registered_handles))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        pooled,
        vec![(true, 0)],
        "干净资源必须恰好归池一次，且台账记的是判据实际消费的那两个输入"
    );
    assert!(
        provider
            .journal()
            .assert()
            .leak_invariant_violations()
            .is_empty(),
        "对照正例收尾 I1–I8 必须收口"
    );
}

/// §9.4(b) 的**核心判负**：宿主**少报**（声称 0，资源上其实还挂着登记句柄）⇒
/// 判据必须按**实测**拒绝归池。
///
/// 这条就是杀死「判据改读 `request.registered_handles`」那个变形的用例：
/// 变形之后这里会拿到宿主的 `0` 并伪称「宿主检查全部通过」而把带句柄的资源放回池子，
/// 台账立刻多出一条 `ReturnedToPool`，本用例当场转红。
///
/// 这个形状不是假想的：§9.4 之所以要求宿主检查 AND driver 的 Clean，就是因为
/// 「driver 说 Clean / 宿主账本说没事」而资源上仍有句柄是 §4.2 F10 的真实故障。
/// 与 `cm74_release_order::a_resource_still_holding_a_handle_is_never_returned_to_the_pool`
/// 的分工：那条钉的是**句柄多于声明**方向之外的「harness 先注销再关」形状并检查计数；
/// 这条钉的是**判据读哪一个数**——所以它必须在**没有**预先注销句柄的情况下，
/// 由调用方交出一份**偏低**的声明。
#[test]
fn pooling_refuses_a_host_ledger_that_undercounts_the_registered_handles() {
    let provider = provider();
    let acquired = acquire(&provider).expect("acquire 必须成功");
    provider
        .register_handle(
            &acquired.resource_id,
            HandleKind::Transaction,
            HandleId::new("hl_undercount"),
            None,
        )
        .expect("登记必须成功");
    assert_eq!(
        provider.registered_handles(&acquired.resource_id),
        1,
        "本用例的前提就是资源上挂着登记句柄"
    );

    // 宿主交出一份**偏低**的账：声称 0，实测 1。
    let receipt = provider
        .close_resource(&CloseResourceRequest {
            handle: acquired.handle.clone(),
            registered_handles: 0,
            protocol_drained: true,
        })
        .expect("带句柄关闭是合法路径（CM-74），只是不许归池");

    assert_eq!(
        pooled_count(&provider, &acquired.resource_id),
        0,
        "§9.4 / §4.2 F10：宿主账本少报时**不得**归池 —— 判据必须读被调方实测的那个值，\
         不能读调用方声称的那个"
    );
    assert_eq!(
        closed_count(&provider, &acquired.resource_id),
        1,
        "资源仍然必须被关掉（§9.4：任一失败都关闭）"
    );
    assert_eq!(
        receipt.resource_release,
        ResourceRelease::Confirmed,
        "关闭本身是确认的；被拒的是**归池**，不是关闭"
    );
    // 句柄仍然随关闭注销（CM-74 语义与声明偏低无关），所以 I5 收得了口。
    assert_eq!(provider.registered_handles(&acquired.resource_id), 0);
    assert!(
        provider
            .journal()
            .assert()
            .leak_invariant_violations()
            .is_empty(),
        "拒绝归池不是泄漏：permit 已归还、句柄已注销"
    );
}

/// §9.4(b) 的另一半：宿主**多报**（陈旧账本：声称 3，资源上根本没有句柄）时
/// **也不许**归池 —— §9.4 把归池前置写成 AND（「宿主检查全部通过」），而 §6.5 规定
/// 句柄只能经登记/注销改变，所以两份账分叉本身就说明前置不成立。
///
/// 这条用例把「两个输入都消费」钉成断言，因此判据被改成**只读实测**
/// （`measured_handles == 0 && protocol_drained`）时会当场转红 —— 那正是
/// 「记录一个量、消费另一个量」可以悄悄分叉的形状。
/// 注意关闭本身仍然成功、permit 仍然归还：分叉拒绝的是**归池**这条额外出路。
#[test]
fn pooling_refuses_a_stale_host_ledger_that_overcounts_the_registered_handles() {
    let provider = provider();
    let acquired = acquire(&provider).expect("acquire 必须成功");
    assert_eq!(provider.registered_handles(&acquired.resource_id), 0);

    let receipt = provider
        .close_resource(&CloseResourceRequest {
            handle: acquired.handle.clone(),
            registered_handles: 3,
            protocol_drained: true,
        })
        .expect("关闭必须成功");

    assert_eq!(
        pooled_count(&provider, &acquired.resource_id),
        0,
        "§9.4 的 AND：宿主的账与被调方实测分叉时前置不成立，不得归池（只能关闭）"
    );
    assert_eq!(closed_count(&provider, &acquired.resource_id), 1);
    assert_eq!(receipt.resource_release, ResourceRelease::Confirmed);
    assert_eq!(
        provider.journal().permit_balance(),
        0,
        "关闭仍然归还 permit"
    );
    assert!(
        provider
            .journal()
            .assert()
            .leak_invariant_violations()
            .is_empty(),
        "账本分叉的收尾不得泄漏"
    );
}

/// §9.4(b)：**声明为 0 但协议未排空**时不得归池 —— 判据的两个合取项都得活。
///
/// 单独存在是为了排除「归池被拒只是因为账本分叉」这种解释：这里两份账一致（0 == 0），
/// 被拒的原因只能是 `protocol_drained`。若把排空项从判据里删掉，本用例转红。
#[test]
fn pooling_refuses_an_undrained_protocol_even_when_both_ledgers_agree() {
    let provider = provider();
    let acquired = acquire(&provider).expect("acquire 必须成功");
    provider
        .close_resource(&CloseResourceRequest {
            handle: acquired.handle.clone(),
            registered_handles: 0,
            protocol_drained: false,
        })
        .expect("未排空的关闭仍然合法");
    assert_eq!(
        pooled_count(&provider, &acquired.resource_id),
        0,
        "§5.3 规则 2 前置：protocolDrained=false 时不得归池"
    );
    assert_eq!(closed_count(&provider, &acquired.resource_id), 1);
}

/// §9.4(b)：那段 `None` 分支**过去是可证明的死代码**，现在是真实的、被覆盖的分支。
///
/// 死代码的论证（自行核实过）：顶层 `resources` IndexMap 只在 `create_resource` 的
/// `insert` 出现、全目录没有任何 `remove` / `retain` / `clear` 作用在它身上，
/// 而旧实现在同一次调用里先用 `resolve()` 查过一遍这张表，所以后面的 `get_mut` 恒为 `Some`。
/// 重构之后**没有第二次查表**：`prepare_close` 就在 `get_mut` 拿到的那个 slot 上跑，
/// 于是 `None` 这一支变成「未知 resourceId 的关闭」的唯一入口，本用例就是它的覆盖。
///
/// 它必须**在写任何台账之前**失败：若判据先算了再说，就会留下一条来自不存在资源的
/// `Closed` 或 `-1`，守恒式（规则 4）当场失真。
#[test]
fn closing_an_unknown_resource_id_is_rejected_before_any_journal_write() {
    let provider = provider();
    let known = acquire(&provider).expect("acquire 必须成功");
    let seq_before = provider.journal().entries().len();

    // 造一个「从未签发过」的资源 id：§3.1 的 ResourceHandle 由提供方签发，
    // 这里用同样的构造口径伪造一个 id，模拟宿主拿着一张不属于本 provider 的凭证。
    let ghost = ResourceId::new("res_w1_9999");
    let forged = ResourceHandle::issue(&ghost, &Counter::new(1), &owner());
    let err = provider
        .close_resource(&CloseResourceRequest {
            handle: forged,
            registered_handles: 0,
            protocol_drained: true,
        })
        .expect_err("未知资源必须被拒：这是**可到达**的输入，不是夹具内部不变量");
    assert!(
        matches!(err, ProviderError::SessionLost(_)),
        "未知资源应报 SessionLost（§3.1 凭证校验），实际 {err:?}"
    );
    assert_eq!(
        provider.journal().entries().len(),
        seq_before,
        "拒绝必须发生在**任何**台账写入之前，否则规则 4 的守恒式会被一条幽灵事件打歪"
    );
    // 已知资源不受影响（这条断言排除「整个 provider 被这一笔搞坏」的误判）。
    assert_eq!(provider.journal().permit_balance(), 1);
    close_and_release(&provider, &known).expect("已知资源仍能正常关闭");
    assert!(
        provider
            .journal()
            .assert()
            .leak_invariant_violations()
            .is_empty(),
        "收尾 I1–I8 必须收口"
    );
}

/// §9.4(b)：凭证本身无效（epoch 不符）时，关闭必须死在**校验**而不是死在查表之后 ——
/// 并且不许留下任何台账。这条与上一条合起来覆盖 `prepare_close` 的两个 `Err` 出口。
#[test]
fn closing_with_a_stale_epoch_is_rejected_by_the_credential_gate_not_the_table() {
    let provider = provider();
    let acquired = acquire(&provider).expect("acquire 必须成功");
    let seq_before = provider.journal().entries().len();
    let stale = ResourceHandle {
        resource_id: acquired.resource_id.clone(),
        runtime_epoch: Counter::new(acquired.handle.runtime_epoch.get().saturating_add(7)),
        owner_token: acquired.handle.owner_token.clone(),
    };
    let err = provider
        .close_resource(&CloseResourceRequest {
            handle: stale,
            registered_handles: 0,
            protocol_drained: true,
        })
        .expect_err("epoch 不符必须被拒");
    assert!(
        matches!(err, ProviderError::RuntimeEpochMismatch(_)),
        "必须死在 epoch 门闸上（§3.1 三校验），实际 {err:?}"
    );
    assert_eq!(
        provider.journal().entries().len(),
        seq_before,
        "凭证校验失败不得写台账"
    );
    // 资源没被动过：状态、句柄、permit 全部原样。快照在读到 Err 之后取，
    // 所以「被拒的关闭推进了状态机」只能以**没有前进到 Closed** 判负，
    // 而不是硬编码 acquire 之后的具体状态（那是夹具的构造细节，不是被测语义）。
    let slot = provider
        .resource(&acquired.resource_id)
        .expect("资源必须还在");
    assert_ne!(
        slot.state,
        FakeResourceState::Closed,
        "被拒的关闭不得把资源推进到 Closed"
    );
    assert!(
        slot.accounting.occupied,
        "被拒的关闭不得归还 permit（记账锚点仍为占用）"
    );
    assert_eq!(provider.journal().permit_balance(), 1);
    close_and_release(&provider, &acquired).expect("收尾关闭必须成功");
}

/// F14：重复关闭是**幂等**的 —— 台账只记一次，回执照发（fixtures §3 契约表「幂等：首次 `Closed`，
/// 重复调用仍 `Closed`」/ 连接 §3 INV-10「lease 清理/关闭幂等，预算只释放一次」/ §7.5(6)
/// 「释放隧道引用、核销预算只执行一次」/ §7.5(7) 的 tombstone 供重复请求查询 / §9.3）。
///
/// 这条用例同时是 `freshly_closed` 的判别式：两次关闭的宿主声明都是 0，
/// 第二次如果照旧判据就会**再**记一条 `ReturnedToPool` 与一条 `Closed`。
/// 旧实现的实测形状正是如此（`ReturnedToPool=2 / Closed=2`，而 permit 只有一笔 `-1` ——
/// 那是 `Accounting::release()` 幂等兜住的，不是关闭路径自己的判据）。
///
/// 分界必须写清：**回执**（`state` / `resource_release` / `effect_outcome`）两次都发、
/// 两次都是 `Closed` / `Confirmed` —— 幂等管的是响应契约；**台账**（`Closed` 事件、permit 事件）
/// 只记一次 —— 幂等管的是「一次状态迁移记一笔」。把这两件事混为一谈，要么会误伤契约，
/// 要么会漏掉台账上的重复。
#[test]
fn closing_twice_is_idempotent_and_never_pools_or_refunds_twice() {
    let provider = provider();
    let acquired = acquire(&provider).expect("acquire 必须成功");
    let first = provider
        .close_resource(&close_request(&provider, &acquired))
        .expect("第一次关闭必须成功");
    let after_first = provider.journal().entries().len();
    let second = provider
        .close_resource(&close_request(&provider, &acquired))
        .expect("幂等：重复关闭仍然是成功（fixtures §3 契约表 / 连接 §3 INV-10）");

    // (1) 响应契约：两次回执完全一致，不因台账去重而变形。
    assert_eq!(first.resource_release, ResourceRelease::Confirmed);
    assert_eq!(second.resource_release, ResourceRelease::Confirmed);
    assert_eq!(first.state, SessionState::Closed);
    assert_eq!(second.state, SessionState::Closed);
    assert_eq!(second.effect_outcome, EffectOutcome::Completed);
    assert_eq!(
        provider.journal().entries().len(),
        after_first,
        "第二次关闭不得再写任何台账事件：`freshly_closed` 为假 ⇒ 归池判据不成立、且 `Closed` 也不再记"
    );

    // (2) 台账：归池一次、关闭一次 —— 这是 F14 的判负点。
    assert_eq!(
        pooled_count(&provider, &acquired.resource_id),
        1,
        "归池最多一次：资源只在**首次**关闭时归还给池子"
    );
    assert_eq!(
        closed_count(&provider, &acquired.resource_id),
        1,
        "`Closed` 只记一次：第二次调用没有产生新的状态迁移，台账就不该多出一条"
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
    assert_eq!(
        deltas,
        vec![1, -1],
        "permit 恰好一笔 +1 与一笔 -1：`Accounting::release()` 是至多一次的那一位"
    );
    assert_eq!(provider.journal().permit_balance(), 0);

    // (3) 台账里没有第二次归池带回来的资源事件：重复关闭真的什么都没记。
    let events_after = resource_event_names(&provider, &acquired.resource_id);
    assert_eq!(
        events_after,
        vec!["Created", "OpeningReady", "ReturnedToPool", "Closed"],
        "资源事件序列止步于首次关闭的那一对：重复调用不得再产生 `Closed`/`ReturnedToPool`"
    );
    assert!(
        provider
            .journal()
            .assert()
            .leak_invariant_violations()
            .is_empty(),
        "重复关闭收尾后 I1–I8 仍必须收口：幂等不是漏收的理由"
    );
    assert!(
        provider
            .journal()
            .assert()
            .change_point_violations()
            .is_empty(),
        "§5.3 变更点判负必须为空（第二次关闭不是一次新的资源变更）"
    );
}
