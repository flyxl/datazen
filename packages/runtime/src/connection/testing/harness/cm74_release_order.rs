//! 「释放顺序」的定点用例。
//!
//! 从 `tests.rs` 拆出，**只搬运**：用例名、断言表达式、期望值一字未改。
//!
//! 共用的三个帮助函数（`harness_for` / `owner` / `first_handle_id`）归 [`super`]：
//! 它们被本文件与 `tests.rs` 同时调用，归属理由见 `mod.rs` 里那段注释。

use serde_json::json;

use super::{first_handle_id, harness_for, owner};
use crate::connection::execution::SessionCommand;
use crate::connection::port::{BudgetClass, CloseResourceRequest};
use crate::connection::testing::fixtures::NS_A_KEY;
use crate::connection::testing::journal::{HandleAction, JournalEntry, ResourceEvent};

// ---------------------------------------------------------------------------
// 释放顺序（竞态用例编排表；淘汰是宿主行为）
// ---------------------------------------------------------------------------

/// 「淘汰 + 句柄登记并存」：journal 顺序必须是
/// `handle closed` → `resource Closed` → `permit -1`。
///
/// 这里钉的是**次序**而不是「三件事都发生过」：三件都发生但次序错了，宿主就会在
/// 句柄还挂着的时候先释放预算占用，permit 收支与登记册收口一起破，
/// 而任何「都发生过」式的断言都照样是绿的。
///
/// 钉的是**宿主关闭路径**（[`FakeHarness::close`]）。驱动直连路径的句柄注销曾被
/// `reclaim_registered_handles_on_close` 兜在归池判定**之后**——那样挪过来会让
/// 「句柄非空 → 关闭而非归池」被一个已经注销干净的 `registered_handles == 0` 骗过去，
/// 所以那条路径一度退化成 `resource Closed → permit -1 → handle closed`。
///
/// **该理由经统一裁定更新**：`close_resource` 现在在**同一个临界区**里先取
/// 注销前快照、再注销、最后判归池，两条路径产出**同一条**顺序。
/// 本用例的函数名与 `vec![...]` 断言一字未动，另由
/// `the_driver_direct_close_releases_in_the_same_cm74_order` 钉住直连路径。
#[test]
fn closing_a_resource_that_still_holds_a_handle_releases_in_the_cm74_order() {
    let harness = harness_for(NS_A_KEY);
    let acquired = harness
        .acquire(owner(), "pol-3", BudgetClass::Session)
        .expect("acquire 必须成功");
    // 故意**不**回滚：句柄仍登记在册，资源带着它进关闭路径（这才是要钉的形状）。
    let begun = harness
        .invoke(
            SessionCommand::BeginSessionTransaction,
            &acquired.handle,
            json!({}),
        )
        .expect("begin 必须成功");
    let handle_id = first_handle_id(&begun);
    let resource_id = acquired.handle.resource_id.clone();

    harness
        .close(&acquired.handle)
        .expect("close 必须成功：带着登记句柄关闭是合法路径");

    // 按台账顺序把这三步摘出来（journal entries 是按 seq 追加的，遍历序即发生序）。
    let mut steps: Vec<&str> = Vec::new();
    for entry in harness.journal().entries() {
        match entry {
            JournalEntry::Handle {
                handle_id: owner_id,
                action: HandleAction::Closed,
                ..
            } if *owner_id == handle_id => steps.push("handle closed"),
            JournalEntry::Resource {
                resource_id: rid,
                event: ResourceEvent::Closed,
                ..
            } if rid.as_ref() == resource_id.as_ref() => steps.push("resource Closed"),
            JournalEntry::Permit { delta: -1, .. } => steps.push("permit -1"),
            _ => {}
        }
    }
    assert_eq!(
        steps,
        vec!["handle closed", "resource Closed", "permit -1"],
        "要求的释放顺序是 `handle closed` → `resource Closed` → `permit -1`；\
         句柄还挂着就归还预算占用，I1 与 I5 一起破。实际次序是 {steps:?}"
    );

    harness
        .assert_no_leak()
        .unwrap_or_else(|violations| panic!("顺序成立也不许留下泄漏：{violations}"));
}

/// 统一后的**驱动直连**路径（不经宿主网关，直接 `close_resource`），顺序必须与上面那条宿主路径
/// **逐字相同**。统一之前它退化成 `resource Closed → permit -1 → handle closed`：句柄注销当时被
/// `reclaim_registered_handles_on_close` 兜到了 permit 归还之后。
#[test]
fn the_driver_direct_close_releases_in_the_same_cm74_order() {
    let harness = harness_for(NS_A_KEY);
    let acquired = harness
        .acquire(owner(), "pol-4", BudgetClass::Session)
        .expect("acquire 必须成功");
    // 故意**不**回滚、不走宿主：句柄仍登记在册，资源带着它进 close_resource。
    let begun = harness
        .invoke(
            SessionCommand::BeginSessionTransaction,
            &acquired.handle,
            json!({}),
        )
        .expect("begin 必须成功");
    let handle_id = first_handle_id(&begun);
    let resource_id = acquired.handle.resource_id.clone();
    harness
        .provider()
        .close_resource(&CloseResourceRequest {
            handle: acquired.handle.clone(),
            registered_handles: harness.provider().registered_handles(&acquired.resource_id),
            protocol_drained: true,
        })
        .expect("带着登记句柄直接 close_resource 必须成功");

    // journal entries 按 seq 追加，遍历序即发生序。
    let mut steps: Vec<&str> = Vec::new();
    for entry in harness.journal().entries() {
        match entry {
            JournalEntry::Handle {
                handle_id: owner_id,
                action: HandleAction::Closed,
                ..
            } if *owner_id == handle_id => steps.push("handle closed"),
            JournalEntry::Resource {
                resource_id: rid,
                event: ResourceEvent::Closed,
                ..
            } if rid.as_ref() == resource_id.as_ref() => steps.push("resource Closed"),
            JournalEntry::Permit { delta: -1, .. } => steps.push("permit -1"),
            _ => {}
        }
    }
    assert_eq!(
        steps,
        vec!["handle closed", "resource Closed", "permit -1"],
        "要求两条关闭路径产出**同一条**顺序；句柄还挂着就归还预算占用，\
         I1 与 I5 一起破。直连路径实际次序是 {steps:?}"
    );

    harness
        .assert_no_leak()
        .unwrap_or_else(|violations| panic!("直连路径顺序成立也不许留下泄漏：{violations}"));
}

/// 定点钉子（直连路径）：驱动报 `Clean`（`protocol_drained = true`）而资源关闭前仍挂着
/// 登记句柄 —— 此时**必须关闭，不得归池**。
///
/// 钉的是 `ReturnedToPool` 的**出现次数**，不是「落了 `Closed`」：一个既记 `Closed` 又记
/// `ReturnedToPool` 的坏实现照样能过后者。计数 0 也把「driver 报 `Clean` 但句柄非空」的注入版
/// （`fake_resource/tests.rs::f10_a_clean_reset_...`）钉在机制层：判据读注销**前**的快照，
/// 一旦改成读注销之后的余量，这里立刻变成 1。
#[test]
fn a_resource_still_holding_a_handle_is_never_returned_to_the_pool() {
    let harness = harness_for(NS_A_KEY);
    let acquired = harness
        .acquire(owner(), "pol-5", BudgetClass::Session)
        .expect("acquire 必须成功");
    harness
        .invoke(
            SessionCommand::BeginSessionTransaction,
            &acquired.handle,
            json!({}),
        )
        .expect("begin 必须成功");
    let resource_id = acquired.handle.resource_id.clone();
    let declared = harness.provider().registered_handles(&acquired.resource_id);
    assert!(
        declared > 0,
        "本用例的前提就是关闭前挂着登记句柄，实际 declared={declared}"
    );
    harness
        .provider()
        .close_resource(&CloseResourceRequest {
            handle: acquired.handle.clone(),
            registered_handles: declared,
            protocol_drained: true,
        })
        .expect("直连关闭必须成功");

    let (mut pooled, mut closed) = (0usize, 0usize);
    for entry in harness.journal().entries() {
        if let JournalEntry::Resource {
            resource_id: rid,
            event,
            ..
        } = entry
        {
            if rid.as_ref() != resource_id.as_ref() {
                continue;
            }
            match event {
                ResourceEvent::ReturnedToPool { .. } => pooled += 1,
                ResourceEvent::Closed => closed += 1,
                _ => {}
            }
        }
    }
    assert_eq!(
        pooled, 0,
        "驱动报 Clean 但宿主仍有已登记句柄时必须**关闭而非归池**；\
         归池判据必须读关闭**前**的句柄快照，不能读注销之后的余量"
    );
    assert_eq!(closed, 1, "资源必须恰好记一次 Closed");
    harness
        .assert_no_leak()
        .unwrap_or_else(|violations| panic!("F10 不许留下泄漏：{violations}"));
}
