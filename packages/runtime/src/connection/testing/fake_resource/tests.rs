//! `FakeResourceProvider` 的自测（fake-runtime-fixtures.md §3、§4、§5.3、§8、§13）。
//!
//! 这些测试刻意**不看内存布局**：断言一律打在 journal 台账、permit 台账与
//! `assert_no_leak` 的返回值上。所以将来把假提供方换成任何别的实现，只要台账
//! 语义不变，断言就仍然成立。
//!
//! 全程**不 sleep**：顺序由 `seq` 表达，时间由 [`FakeClock`] 推进（§6 L357 禁止用
//! sleep 猜顺序）。
//!
//! 本文件由 `mod.rs` 以 `#[cfg(test)] mod tests;` 声明 —— 因此本文件的 `super`
//! 就是 `fake_resource` 本身，`use super::*` 直接拿到 §3.1 的类型。
//!
//! **panic 约定**（docs/development/panic-policy.md）：本文件每个 `expect` / `panic!`
//! 的消息都写明「哪条不变量被打破才算失败」，而不是「出错了」。夹具自带的基线路径
//! 不允许 panic；只有**夹具没按设计造出目标状态**（例如注入的故障没生效、句柄没造出来）
//! 才 panic，那种情况继续断言下去只会得到一个由假状态推出的假结论。

use std::time::Duration;

use serde_json::json;

use super::{AcquiredResource, FakeResourceProvider, FaultKind, ResourceOp};
use crate::connection::error::ProviderError;
use crate::connection::execution::{EffectOutcome, ExecutionErrorCode};
use crate::connection::port::{
    AcquireResourceRequest, BudgetClass, CloseResourceRequest, ExecuteOnResourceRequest,
};
use crate::connection::testing::clock::FakeClock;
use crate::connection::testing::fixtures::{self, NS_A_KEY, PROFILE_P};
use crate::connection::testing::harness::fixture_target;
use crate::connection::testing::journal::JournalEntry;
use crate::connection::types::{
    ConnectionId, Counter, ExecutionId, HandleId, JobId, NamespaceTarget, OrganizationId, OwnerRef,
    PoolKeyFingerprint, PoolKeyInputs, WorkerId,
};

/// §8.1：夹具目标一律取自 `fixtures`，用例里不写硬编码字面量。
fn target() -> crate::connection::types::ExecutionTarget {
    fixture_target(NS_A_KEY)
}

/// §8.1 的 `PROFILE_P` 归属：一个 job owner。`owner` 是 `OwnerRef`，不是裸 id。
fn owner() -> OwnerRef {
    OwnerRef::Job {
        organization_id: OrganizationId::new(fixtures::ORG_A),
        job_id: JobId::new("job_org-alpha_0001"),
        stage_id: "job:job_org-alpha_0001/stage:1".to_owned(),
    }
}

/// §8.1 `PoolKeyInputs` 派生。夹具走和
/// [`FakeHarness::pool_key`](crate::connection::testing::harness::FakeHarness::pool_key)
/// 完全一样的算法，
/// 否则「同池复用」的正例根本不成立（CM-05 / CM-67 的换 key 判据会失效）。
fn pool_key(provider: &FakeResourceProvider, policy_isolation_key: &str) -> PoolKeyFingerprint {
    PoolKeyFingerprint::derive(&PoolKeyInputs {
        connection_id: ConnectionId::new(PROFILE_P),
        config_revision: provider.config_revision(),
        driver_id: super::PROVIDER_ID.to_owned(),
        namespace: NamespaceTarget {
            database: provider.target().namespace.database.clone(),
            catalog: String::new(),
            schema: String::new(),
            path: String::new(),
        },
        execution_identity_key: provider.execution_identity().to_owned(),
        policy_isolation_key: policy_isolation_key.to_owned(),
    })
}

/// 默认假提供方：假时钟与 journal 共享同一个 `FakeClock`。
fn provider() -> FakeResourceProvider {
    FakeResourceProvider::new(WorkerId::new("w1"), target())
        .with_execution_identity(fixtures::IDENTITY_SHARED)
}

/// 走一次 `acquire`。
fn acquire(provider: &FakeResourceProvider) -> Result<AcquiredResource, ProviderError> {
    provider.acquire(&AcquireResourceRequest {
        descriptor: provider.descriptor(),
        pool_key: pool_key(provider, "pol-1"),
        budget_class: BudgetClass::Session,
        owner: owner(),
        db_session_id: provider.ids().next_db_session_id(),
    })
}

/// 按 §5.3 的归池前置条件关掉一张资源。
fn close(
    provider: &FakeResourceProvider,
    acquired: &AcquiredResource,
) -> Result<(), ProviderError> {
    provider.close_resource(&CloseResourceRequest {
        handle: acquired.handle.clone(),
        registered_handles: provider.registered_handles(&acquired.resource_id),
        protocol_drained: true,
    })?;
    Ok(())
}

// ---------------------------------------------------------------------------
// §5.3 规则 1 / §4.3 I1、I2、I6：创建与归池必须记账
// ---------------------------------------------------------------------------

/// §5.3 规则 1：每次 `resourceId` 创建 → permit 余额 +1 **且** `live_resources` 同步 +1；
/// §5.3 规则 2：归池前置条件满足才允许归还。两条都在同一条台账轨迹上验。
#[test]
fn creating_a_resource_takes_one_permit_and_closing_it_gives_it_back() {
    let provider = provider();
    let acquired = acquire(&provider).expect("假提供方自带的 acquire 必须成功");

    // 规则 1：permit 台账 +1，live_resources 同步 +1。
    assert_eq!(
        provider.journal().permits_issued(),
        1,
        "每次资源创建必须记一笔 permit 发放"
    );
    assert_eq!(
        provider.live_resources(),
        vec![acquired.resource_id.clone()]
    );

    close(&provider, &acquired).expect("归池必须成功");

    // §4.3 I2：没有残留占用预算的资源。
    assert!(
        provider.live_resources().is_empty(),
        "归池后不得仍有资源占用预算（I2）"
    );
    // §4.3 I6：发放与归还是配平的。
    assert_eq!(
        provider.journal().assert().ledger_violations(),
        Vec::<String>::new(),
        "permit 台账必须配平（I1/I6）"
    );
}

// ---------------------------------------------------------------------------
// §5.3 变化点断言在注入故障下仍然可表达（ops.rs 的模块级引用点名了本用例）
// ---------------------------------------------------------------------------

/// `ops.rs` 的模块注释点名了本用例：变化点断言在**注入故障**的轨迹上仍然成立。
///
/// 覆盖 §4.1 F4/F5 的两个注入点，并顺带证明 §13 L774 的强制关系：
/// 不可判定的 `errorCode` ⇒ `effectOutcome` 必须是 `unknown`。
#[test]
fn the_change_point_rules_stay_expressible_under_injected_faults() {
    // --- F4：语句派发就失败，且失败**可判定**（SqlError）→ 作用域未开始 ---
    {
        let provider = provider();
        provider.script().once(
            ResourceOp::Execute,
            FaultKind::StatementDispatch { code: "SqlError" },
        );
        let acquired = acquire(&provider).expect("acquire 必须成功");

        let completion = provider
            .execute_on_resource(&ExecuteOnResourceRequest {
                handle: acquired.handle.clone(),
                execution_id: ExecutionId::new("exec-dispatch-fail"),
                command_id: "query".to_owned(),
                input: json!({ "sql": "SELECT 1" }),
            })
            .expect("注入了失败但仍然必须交出终态记录（§5.3 规则 8）");

        // §4.2 F4：`errorCode` 与 `effectOutcome` 独立取值。
        assert_eq!(completion.error_code, Some(ExecutionErrorCode::SqlError));
        // 语句没送出去 ⇒ 作用域未开始，**不是** unknown。
        assert_eq!(completion.effect_outcome, EffectOutcome::NotStarted);
    }

    // --- F5：结果传输失败，且不可判定（Timeout）⇒ 必须 unknown ---
    {
        let provider = provider();
        provider.script().once(
            ResourceOp::Execute,
            FaultKind::ResultTransport { code: "Timeout" },
        );
        let acquired = acquire(&provider).expect("acquire 必须成功");

        let completion = provider
            .execute_on_resource(&ExecuteOnResourceRequest {
                handle: acquired.handle.clone(),
                execution_id: ExecutionId::new("exec-transport-fail"),
                command_id: "query".to_owned(),
                input: json!({ "sql": "SELECT 1" }),
            })
            .expect("注入了失败但仍然必须交出终态记录（§5.3 规则 8）");

        assert_eq!(completion.error_code, Some(ExecutionErrorCode::Timeout));
        // CM-44 / CM-47：超时 ⇒ 作用域不可判定 ⇒ 只能是 unknown，
        // 禁止因为「看到超时」就回填 completed / rolledBack。
        assert_eq!(completion.effect_outcome, EffectOutcome::Unknown);
    }

    // --- 没有故障时是干净的 completed，且 errorCode 为空 ---
    {
        let provider = provider();
        let acquired = acquire(&provider).expect("acquire 必须成功");
        let completion = provider
            .execute_on_resource(&ExecuteOnResourceRequest {
                handle: acquired.handle.clone(),
                execution_id: ExecutionId::new("exec-clean"),
                command_id: "query".to_owned(),
                input: json!({ "sql": "SELECT 1" }),
            })
            .expect("无注入故障时必须成功");
        assert_eq!(
            completion.completion_status,
            crate::connection::port::CompletionStatus::Ok
        );
        assert_eq!(completion.effect_outcome, EffectOutcome::Completed);
        assert_eq!(completion.error_code, None);
    }
}

// ---------------------------------------------------------------------------
// §4.1 F2：预算占满窗口挂在假时钟上，不 sleep
// ---------------------------------------------------------------------------

/// F2：注入的预算占满窗口挂在**假单调时钟**上。窗口内申请必须被拒，
/// 推进假时钟越过窗口后同一申请必须成功 —— 全程没有一次 sleep。
#[test]
fn the_budget_busy_window_expires_on_the_fake_clock_not_on_sleep() {
    let provider = provider();
    provider.script().once(
        ResourceOp::Acquire,
        FaultKind::BudgetBusy {
            busy_for: Duration::from_secs(10),
            reason: "连接数达到上限",
        },
    );

    let rejected = acquire(&provider);
    match rejected {
        Err(ProviderError::ResourceBusy(reason)) => {
            assert_eq!(reason, "连接数达到上限", "拒绝原因必须原样透传给调用方");
        }
        other => panic!("预算占满时必须返回 ResourceBusy，实际是 {other:?}"),
    }

    // 假时钟推进 10s：窗口过期。
    provider.clock().advance(Duration::from_secs(10));
    acquire(&provider).expect("窗口过期后必须能申请到资源");
}

// ---------------------------------------------------------------------------
// §5.1 L416 / §5.3 规则 5：句柄必须登记，孤立句柄要被台账抓到
// ---------------------------------------------------------------------------

/// §5.1 L416：造出来的句柄必须经 `register_handle` 登记，否则台账抓不到它的归属。
/// `orphan_handle` 故意造一个「只登记了 orphaned 事件、没进登记册」的句柄
/// ⇒ §4.3 的 **I7** 不成立（这里是**故意**制造泄漏，验的是断言能抓到它，
/// 不是期望它通过）。
#[test]
fn an_orphan_handle_is_visible_to_the_leak_invariant() {
    let provider = provider();
    let acquired = acquire(&provider).expect("acquire 必须成功");

    provider
        .orphan_handle(
            &acquired.resource_id,
            crate::connection::session::HandleKind::Cursor,
            HandleId::new("hl_orphan_fixture"),
        )
        .expect("孤立句柄必须造得出来");

    // I7：orphan_handles 必须为空 —— 这里**不成立**，否则断言就是恒真的。
    let violations = provider.journal().assert().leak_invariant_violations();
    assert!(
        violations
            .iter()
            .any(|text| text.contains("orphan_handles 非空")),
        "孤立句柄必须让 I7 判负，实际违规列表：{violations:?}"
    );

    // 收口路径是**关闭资源**而不是 `close_handle`：孤立句柄从没进过登记册，
    // `close_handle` 只能报 `SessionNotFound`（§5.1「造句柄 ≠ 登记句柄」）。
    // `close_resource` 末尾会 `recover_orphans_on_close`，I7 由此收口（I1/I2/I6 一并收口）。
    close(&provider, &acquired).expect("关闭资源必须成功");
    assert_eq!(
        provider.journal().assert().leak_invariant_violations(),
        Vec::<String>::new(),
        "资源关闭、孤儿句柄被回收后不得再有残留违规"
    );
}

// ---------------------------------------------------------------------------
// §3.1 / §5.3：epoch 与 journal seq 单调
// ---------------------------------------------------------------------------

/// §5.2：单调 `seq` 来自单个原子计数器，因此台账顺序**不依赖时钟**。
/// 连续写入不得出现重复或倒退的 `seq`。
#[test]
fn journal_sequence_is_strictly_increasing_across_writes() {
    let provider = provider();
    let mut acquired = None;
    for index in 0..3 {
        let resource = acquire(&provider).expect("连续申请必须成功");
        if index == 0 {
            acquired = Some(resource);
        }
    }

    let seqs: Vec<u64> = provider
        .journal()
        .entries()
        .iter()
        .map(JournalEntry::seq)
        .collect();
    assert!(!seqs.is_empty(), "每一次写入都必须留下痕迹");
    assert!(
        seqs.windows(2).all(|pair| pair[0] < pair[1]),
        "seq 必须严格递增，实际前若干项：{:?}",
        &seqs[..seqs.len().min(8)]
    );

    // 关掉第一张，验证「创建 → 归池」这一对事件按顺序落在台账上。
    let first = acquired.expect("第一步必须拿到资源");
    close(&provider, &first).expect("归池必须成功");
    let order: Vec<&str> = provider
        .journal()
        .entries()
        .iter()
        .filter_map(|entry| match entry {
            JournalEntry::Resource { resource_id, .. } if *resource_id == first.resource_id => {
                Some(match entry {
                    JournalEntry::Resource {
                        event: crate::connection::testing::journal::ResourceEvent::Created,
                        ..
                    } => "created",
                    _ => "other",
                })
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        order.first().copied(),
        Some("created"),
        "Created 必须是该资源的第一条事件"
    );
}

// ---------------------------------------------------------------------------
// §8.2：id 形态可预测
// ---------------------------------------------------------------------------

/// §8.2：`dbSessionId` 形如 `dbs_<workerId>_<seq:04>`，`resourceId` 形如
/// `res_<workerId>_<seq:04>`。夹具里不用随机源，所以这两者可被直接断言。
#[test]
fn generated_ids_carry_the_worker_id_and_a_zero_padded_sequence() {
    let provider = provider();
    let acquired = acquire(&provider).expect("acquire 必须成功");

    assert_eq!(
        provider.ids().worker_id(),
        "w1",
        "id 必须带 worker 段，便于台账按前缀归组"
    );
    let db_session = acquired.session.handle.db_session_id.as_str();
    assert!(
        db_session.starts_with("dbs_w1_") && db_session.len() > "dbs_w1_".len(),
        "dbSessionId 必须形如 dbs_<worker>_<seq:04>，实际是 {db_session}"
    );
    let resource = acquired.resource_id.as_str();
    assert!(
        resource.starts_with("res_w1_") && resource.len() > "res_w1_".len(),
        "resourceId 必须形如 res_<worker>_<seq:04>，实际是 {resource}"
    );
}

// ---------------------------------------------------------------------------
// FakeClock 单独一档：假时钟是唯一的「时间」
// ---------------------------------------------------------------------------

/// §7：`FakeClock` 只前进不后退，且与真实墙钟无关 —— 这是整套夹具
/// 不用 sleep 的前提。
#[test]
fn the_fake_clock_only_moves_when_told_to() {
    let clock = FakeClock::new();
    let first = clock.monotonic();
    let second = clock.monotonic();
    assert_eq!(
        first, second,
        "不推进就不许变 —— 否则顺序断言会被调度噪声污染"
    );

    clock.advance(Duration::from_millis(250));
    assert!(clock.monotonic() > first, "推进后必须单调变大");
}

// ---------------------------------------------------------------------------
// 池键隔离（§8.1，CM-05 / CM-67）
// ---------------------------------------------------------------------------

/// §8.1：执行身份相同、`policyIsolationKey` 不同 ⇒ 池键必须不同，
/// 于是「同池复用」的正例在夹具里根本不成立。
#[test]
fn a_different_policy_isolation_key_derives_a_different_pool_key() {
    let provider = provider();
    let first = pool_key(&provider, "pol-alpha");
    let second = pool_key(&provider, "pol-beta");
    assert_ne!(
        first, second,
        "policyIsolationKey 不同 ⇒ 池键必须不同（CM-05 / CM-67）"
    );
    assert_eq!(
        first,
        pool_key(&provider, "pol-alpha"),
        "同一组输入必须派生出同一池键（幂等）"
    );
}

// ---------------------------------------------------------------------------
// 未使用导入的显式引用：让 `Counter` 等类型在本文件里保持可见
// ---------------------------------------------------------------------------

/// 夹具不变量：`counter` 必须单调。用一个最小夹具锁住这一点，
/// 避免 `Counter` 的导入在后续重构里被当成无用导入删掉。
#[test]
fn counter_is_monotonic() {
    let first = Counter::new(1);
    assert!(Counter::new(2) > first, "Counter 必须可比较且单调");
}
