//! `FakeResourceProvider` 的自测。
//!
//! 这些测试刻意**不看内存布局**：断言一律打在 journal 台账、permit 台账与
//! `assert_no_leak` 的返回值上。所以将来把假提供方换成任何别的实现，只要台账
//! 语义不变，断言就仍然成立。
//!
//! 全程**不 sleep**：顺序由 `seq` 表达，时间由 [`FakeClock`] 推进（禁止用
//! sleep 猜顺序）。
//!
//! 本文件由 `mod.rs` 以 `#[cfg(test)] mod tests;` 声明 —— 因此本文件的 `super`
//! 就是 `fake_resource` 本身，`use super::*` 直接拿到本模块的类型。
//!
//! **panic 约定**：本文件每个 `expect` / `panic!`
//! 的消息都写明「哪条不变量被打破才算失败」，而不是「出错了」。夹具自带的基线路径
//! 不允许 panic；只有**夹具没按设计造出目标状态**（例如注入的故障没生效、句柄没造出来）
//! 才 panic，那种情况继续断言下去只会得到一个由假状态推出的假结论。

use std::time::Duration;

use serde_json::json;

use super::{
    acquire, close_and_release, owner, pool_key, provider, AcquiredResource, FakeResourceProvider,
    FaultKind, ResourceOp,
};
use crate::connection::error::ProviderError;
use crate::connection::execution::{EffectOutcome, ExecutionErrorCode};
use crate::connection::port::{
    AcquireResourceRequest, BudgetClass, ExecuteOnResourceRequest, ResetDiscardReason,
    ResetOutcome, ResetResourceRequest, TransactionOperation,
};
use crate::connection::testing::clock::FakeClock;
use crate::connection::testing::journal::{JournalEntry, ResourceEvent};
use crate::connection::types::{Counter, DbSessionId, ExecutionId, HandleId};

// ---------------------------------------------------------------------------
// 创建与归池必须记账
// ---------------------------------------------------------------------------

/// 每次 `resourceId` 创建 → permit 余额 +1 **且** `live_resources` 同步 +1；
/// 归池前置条件满足才允许归还。两条都在同一条台账轨迹上验。
#[test]
fn creating_a_resource_takes_one_permit_and_closing_it_gives_it_back() {
    let provider = provider();
    let acquired = acquire(&provider).expect("假提供方自带的 acquire 必须成功");

    // 创建时：permit 台账 +1，live_resources 同步 +1。
    assert_eq!(
        provider.journal().permits_issued(),
        1,
        "每次资源创建必须记一笔 permit 发放"
    );
    assert_eq!(
        provider.live_resources(),
        vec![acquired.resource_id.clone()]
    );

    close_and_release(&provider, &acquired).expect("归池必须成功");

    // 没有残留占用预算的资源。
    assert!(
        provider.live_resources().is_empty(),
        "归池后不得仍有资源占用预算（I2）"
    );
    // 发放与归还是配平的。
    assert_eq!(
        provider.journal().assert().ledger_violations(),
        Vec::<String>::new(),
        "permit 台账必须配平（I1/I6）"
    );
}

// ---------------------------------------------------------------------------
// 变化点断言在注入故障下仍然可表达（ops.rs 的模块级引用点名了本用例）
// ---------------------------------------------------------------------------

/// `ops.rs` 的模块注释点名了本用例：变化点断言在**注入故障**的轨迹上仍然成立。
///
/// 覆盖「错误码与效果结论独立」两个注入点，并顺带证明超时与不可判定的强制关系：
/// 不可判定的 `errorCode` ⇒ `effectOutcome` 必须是 `unknown`。
#[test]
fn the_change_point_rules_stay_expressible_under_injected_faults() {
    // --- 语句派发就失败，且失败**可判定**（SqlError）→ 作用域未开始 ---
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
            .expect("注入了失败但仍然必须交出终态记录");

        // `errorCode` 与 `effectOutcome` 独立取值。
        assert_eq!(completion.error_code, Some(ExecutionErrorCode::SqlError));
        // 语句没送出去 ⇒ 作用域未开始，**不是** unknown。
        assert_eq!(completion.effect_outcome, EffectOutcome::NotStarted);
    }

    // --- 结果传输失败，且不可判定（Timeout）⇒ 必须 unknown ---
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
            .expect("注入了失败但仍然必须交出终态记录");

        assert_eq!(completion.error_code, Some(ExecutionErrorCode::Timeout));
        // 超时 ⇒ 作用域不可判定 ⇒ 只能是 unknown，
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
// 预算占满窗口挂在假时钟上，不 sleep
// ---------------------------------------------------------------------------

/// 注入的预算占满窗口挂在**假单调时钟**上。窗口内申请必须被拒，
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
// 句柄必须登记，孤立句柄要被台账抓到
// ---------------------------------------------------------------------------

/// 造出来的句柄必须经 `register_handle` 登记，否则台账抓不到它的归属。
/// `orphan_handle` 故意造一个「只登记了 orphaned 事件、没进登记册」的句柄
/// ⇒ 孤立句柄不变式 **不成立**（这里是**故意**制造泄漏，验的是断言能抓到它，
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
            "夹具自检：造句柄但不登记",
        )
        .expect("孤立句柄必须造得出来");

    // 孤立句柄必须为空 —— 这里**不成立**，否则断言就是恒真的。
    let violations = provider.journal().assert().leak_invariant_violations();
    assert!(
        violations
            .iter()
            .any(|text| text.contains("orphan_handles 非空")),
        "孤立句柄必须让 I7 判负，实际违规列表：{violations:?}"
    );

    // 收口路径是**关闭资源**而不是 `close_handle`：孤立句柄从没进过登记册，
    // `close_handle` 只能报 `SessionNotFound`（造句柄 ≠ 登记句柄）。
    // `close_resource` 末尾会 `recover_orphans_on_close`，孤立句柄由此收口（预算收支一并收口）。
    close_and_release(&provider, &acquired).expect("关闭资源必须成功");
    assert_eq!(
        provider.journal().assert().leak_invariant_violations(),
        Vec::<String>::new(),
        "资源关闭、孤儿句柄被回收后不得再有残留违规"
    );
}

// ---------------------------------------------------------------------------
// epoch 与 journal seq 单调
// ---------------------------------------------------------------------------

/// 单调 `seq` 来自单个原子计数器，因此台账顺序**不依赖时钟**。
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
    close_and_release(&provider, &first).expect("归池必须成功");
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
// id 形态可预测
// ---------------------------------------------------------------------------

/// `dbSessionId` 形如 `dbs_<workerId>_<seq:04>`，`resourceId` 形如
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

/// `FakeClock` 只前进不后退，且与真实墙钟无关 —— 这是整套夹具
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
// 池键隔离（共享执行身份、不同隔离键）
// ---------------------------------------------------------------------------

/// 执行身份相同、`policyIsolationKey` 不同 ⇒ 池键必须不同，
/// 于是「同池复用」的正例在夹具里根本不成立。
#[test]
fn a_different_policy_isolation_key_derives_a_different_pool_key() {
    let provider = provider();
    let first = pool_key(&provider, "pol-alpha");
    let second = pool_key(&provider, "pol-beta");
    assert_ne!(first, second, "policyIsolationKey 不同 ⇒ 池键必须不同");
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

// ---------------------------------------------------------------------------
// 重置归池：driver 报 `Clean` **不等于**宿主可以把资源放回池子
// ---------------------------------------------------------------------------

/// driver 报 `Clean` 但句柄非空 ⇒ 宿主判定事务未终结 ⇒ **关闭**而非归池。
///
/// 这条用例把「驱动说 Clean」和「宿主能归池」拆成两个独立判据，并要求二者在
/// 注入下**必然相反**：注入只改 `resetResource` 的返回值，资源状态机一个字节都不动。
/// 所以 `can_return_to_pool()` 仍是 `false`，随后的关闭也只能落 `Closed` +
/// 不写 `ReturnedToPool` —— 而基线（同构造、无注入）会当场给出 `Discard`。
#[test]
fn f10_a_clean_reset_does_not_license_returning_the_resource_to_the_pool() {
    // --- 基线：同样的构造，没有注入 ---
    let baseline = provider();
    let baseline_handle = dirty_resource(&baseline).expect("基线构造必须成功");
    let baseline_outcome = baseline
        .reset_resource(&ResetResourceRequest {
            handle: baseline_handle.handle.clone(),
        })
        .expect("基线 reset 不得报错");
    assert_eq!(
        baseline_outcome,
        ResetOutcome::Discard {
            reason: ResetDiscardReason::SessionStillExecuting
        },
        "基线看到未终结的事务必须 Discard，注入前后的差异全靠这一条兜底"
    );

    // --- 注入：driver 对同一份状态改报 `Clean` ---
    let provider = provider();
    provider
        .script()
        .once(ResourceOp::Reset, FaultKind::CleanButPreconditionUnmet);
    let acquired = dirty_resource(&provider).expect("注入构造必须成功");

    let outcome = provider
        .reset_resource(&ResetResourceRequest {
            handle: acquired.handle.clone(),
        })
        .expect("注入 CleanButPreconditionUnmet 仍然必须返回一个结论");
    assert_eq!(
        outcome,
        ResetOutcome::Clean,
        "F10 反例就是「driver 报 Clean」，断言不成立说明注入没生效"
    );
    assert_ne!(
        outcome, baseline_outcome,
        "注入前后必须给出**不同**结论，否则这条用例恒真"
    );

    // 关键：注入**没有**把状态弄脏 —— 前置条件真的不满足，宿主必须自己去查。
    let slot = provider
        .resource(&acquired.resource_id)
        .expect("资源必须还在");
    assert!(
        slot.has_open_transaction(),
        "resetForReuse 不许终结事务，断言不成立说明注入污染了状态机"
    );
    assert_eq!(slot.registered_handles(), 1, "句柄必须仍在登记册里");
    assert!(
        !slot.can_return_to_pool(),
        "宿主前置不满足时禁止归池 —— 这一条才是 F10 的落点"
    );

    // 宿主若信了 driver 的 `Clean` 就去归池，`ReturnedToPool` 会被记下来。
    // 正确实现必须走关闭，且关闭时不给 `allow_closed` 的把戏。
    let returned = provider
        .journal()
        .entries()
        .iter()
        .filter(|entry| {
            matches!(
                entry,
                JournalEntry::Resource {
                    event: ResourceEvent::ReturnedToPool { .. },
                    ..
                }
            )
        })
        .count();
    assert_eq!(
        returned, 0,
        "前置不满足时一次归池都不许发生，实际记了 {returned} 次"
    );

    close_and_release(&provider, &acquired).expect("关闭必须成功");
    let events: Vec<&'static str> = provider
        .journal()
        .entries()
        .iter()
        .filter_map(|entry| match entry {
            JournalEntry::Resource {
                resource_id, event, ..
            } if *resource_id == acquired.resource_id => Some(event.as_str()),
            _ => None,
        })
        .collect();
    assert!(
        events.contains(&"Closed"),
        "关闭必须记账，实际事件序列：{events:?}"
    );
    assert!(
        !events.contains(&"ReturnedToPool"),
        "句柄未清空时禁止归池 —— 只能关闭。实际事件序列：{events:?}"
    );
    // 关闭 ≠ 泄漏：permit 仍然要还回去。
    assert!(
        provider.live_resources().is_empty(),
        "关闭后不得残留预算占用（I2）"
    );
}

/// `Discard` 的原因由用例指定。基线**产不出** `TemporaryObjectsPresent`
/// —— 没有临时对象这件事，假侧无从观测。所以拿到这个原因本身就证明脚本生效。
#[test]
fn f10_a_scripted_discard_reason_is_carried_through_verbatim() {
    let provider = provider();
    provider.script().once(
        ResourceOp::Reset,
        FaultKind::ResetDiscard {
            reason: ResetDiscardReason::TemporaryObjectsPresent,
        },
    );
    let acquired = dirty_resource(&provider).expect("构造必须成功");

    let outcome = provider
        .reset_resource(&ResetResourceRequest {
            handle: acquired.handle.clone(),
        })
        .expect("注入 Discard 不得报错");
    assert_eq!(
        outcome,
        ResetOutcome::Discard {
            reason: ResetDiscardReason::TemporaryObjectsPresent
        },
        "脚本指定的 Discard 原因必须原样透传给宿主"
    );
}

/// `resetForReuse=unsupported` ⇒ 直接关闭。此时宿主拿到的是**错误**，
/// 绝不能把「reset 没在期限内返回」当成 `Clean`（那会把未终结的事务放回池子）。
#[test]
fn f10_a_timed_out_reset_is_an_error_and_never_a_clean_outcome() {
    let provider = provider();
    provider
        .script()
        .once(ResourceOp::Reset, FaultKind::ResetTimeout);
    let acquired = dirty_resource(&provider).expect("构造必须成功");

    match provider.reset_resource(&ResetResourceRequest {
        handle: acquired.handle.clone(),
    }) {
        Err(ProviderError::CleanupFailed(text)) => {
            assert!(
                text.contains("reset"),
                "错误消息必须说清是 reset 超时，实际是 {text}"
            );
        }
        other => panic!("reset 超时必须以错误收场，实际是 {other:?}"),
    }

    // 状态机没有被这次失败动过：事务仍未终结，宿主仍不能归池。
    let slot = provider
        .resource(&acquired.resource_id)
        .expect("资源必须还在");
    assert!(
        slot.has_open_transaction() && !slot.can_return_to_pool(),
        "reset 失败不得推进状态机"
    );
}

/// 造一张「reset 前置条件不满足」的资源：开了事务 + 登了一个句柄。
/// 两个「driver 报 `Clean` 但句柄非空」的用例共用它，避免同一段构造抄三遍。
fn dirty_resource(provider: &FakeResourceProvider) -> Result<AcquiredResource, ProviderError> {
    let acquired = acquire(provider)?;
    provider.transaction_operation(&acquired.handle, TransactionOperation::Begin)?;
    provider.register_handle(
        &acquired.resource_id,
        crate::connection::session::HandleKind::Cursor,
        HandleId::new("hl_reset_dirty"),
        None,
    )?;
    Ok(acquired)
}

// ---------------------------------------------------------------------------
// 句柄登记：造出来 ≠ 交出去；跨 epoch 复用必须死在 epoch 门闸上
// ---------------------------------------------------------------------------

/// runtime **拒绝**把句柄交给宿主，句柄只在 fake 侧标记 `orphaned`。
///
/// 可观察差异有三处，且都必须与基线相反：`acquire` 仍然成功、宿主登记册里
/// **没有**这个句柄、孤立句柄不变式不成立。关闭之后必须恢复成立。
#[test]
fn f12_a_handle_the_runtime_refuses_to_return_never_reaches_the_host_registry() {
    let provider = provider();
    provider.script().once(
        ResourceOp::Acquire,
        FaultKind::HandleNotReturned {
            reason: "runtime 拒绝把它交给宿主",
        },
    );

    let acquired = acquire(&provider).expect("句柄登记失败不是 acquire 的失败条件");
    assert_eq!(
        provider.registered_handles(&acquired.resource_id),
        0,
        "被拒绝交出的句柄不得进入宿主登记册"
    );
    let orphans = provider.journal().orphan_handles();
    assert_eq!(
        orphans.len(),
        1,
        "fake 侧必须把这件事记成 orphaned，实际：{orphans:?}"
    );
    assert!(
        provider
            .journal()
            .assert()
            .leak_invariant_violations()
            .iter()
            .any(|text| text.contains("orphan_handles 非空")),
        "I7 必须因这条 orphaned 判负"
    );

    close_and_release(&provider, &acquired).expect("关闭必须成功");
    assert!(
        provider.journal().orphan_handles().is_empty(),
        "关闭回收后孤立句柄不变式必须恢复成立"
    );
}

/// 第二行：跨 epoch 复用句柄。
///
/// 注入把句柄的 `runtimeEpoch` 改成陈旧值再送进 `executeOnResource`。`verify`
/// 的次序是 resourceId → epoch → owner，所以必须**死在 epoch 这一关**
/// （`RuntimeEpochMismatch`）；若死成 `SessionLost`，说明改错了字段，
/// 测到的就不是 epoch 门闸。
#[test]
fn f12_a_cross_epoch_handle_dies_on_the_epoch_gate_not_the_id_gate() {
    let provider = provider();
    let acquired = acquire(&provider).expect("acquire 必须成功");

    // 无注入时同一张资源上的执行是干净的 —— 断言不成立说明构造有问题。
    provider
        .execute_on_resource(&ExecuteOnResourceRequest {
            handle: acquired.handle.clone(),
            execution_id: ExecutionId::new("exec-cross-epoch-baseline"),
            command_id: "query".to_owned(),
            input: json!({ "sql": "SELECT 1" }),
        })
        .expect("无注入时执行必须成功");

    provider
        .script()
        .once(ResourceOp::Execute, FaultKind::CrossEpochHandleReuse);
    match provider.execute_on_resource(&ExecuteOnResourceRequest {
        handle: acquired.handle.clone(),
        execution_id: ExecutionId::new("exec-cross-epoch-stale"),
        command_id: "query".to_owned(),
        input: json!({ "sql": "SELECT 1" }),
    }) {
        Err(ProviderError::RuntimeEpochMismatch(text)) => {
            assert!(
                text.contains('1') && text.contains('0'),
                "错误消息必须报出两边的 epoch，实际是 {text}"
            );
        }
        other => {
            panic!("跨 epoch 复用必须死在 epoch 门闸上（RuntimeEpochMismatch），实际是 {other:?}")
        }
    }
}

// ---------------------------------------------------------------------------
// `force_collision` 让两个资源共用一个 epoch ⇒ epoch 门闸被击穿
// ---------------------------------------------------------------------------

/// `FakeIds::force_collision(RuntimeEpoch, k)` 把后续 `k` 次 epoch 发放改成**重复值**。
///
/// 先说清楚 epoch 是怎么发的（`ids.rs::next_runtime_epoch`）：计数器**按 dbSessionId 分桶**，
/// 每个新会话都从 1 开始。两张资源若开在不同会话上，counter 天然都是 1 —— 那种情况下
/// epoch 门闸本来就拦不住交叉句柄，跟撞车无关。所以这条用例把两张资源放进**同一个会话**：
/// counter 是 1 与 2，交叉句柄必然撞在 epoch 这一关。
///
/// 然后 `force_collision(RuntimeEpoch, 1)` 让第二张资源重新领到该作用域里发过的第一个值（1），
/// 于是两张资源共用 epoch。**可观察后果不是「放行」，而是「换了一关拦」**：基线上死在
/// `RuntimeEpochMismatch` 的同一张交叉句柄，撞车后越过 epoch 这一关，改由 `ownerToken`
/// 拒收 —— 因为 `ownerToken` 额外把 epoch 与 resourceId 编进了摘要（`port.rs::issue`）。
/// 断言写的是这个守卫切换，而不是「静默解析成另一张资源」：后者在当前签发规则下
/// 不可能发生，写成断言就是假绿。
#[test]
fn cm71_a_collided_epoch_lets_a_foreign_handle_through_the_epoch_gate() {
    // --- 基线：同一会话里两张资源，counter 1 与 2 ---
    let baseline = provider();
    let session = baseline.ids().next_db_session_id();
    let first = acquire_in_session(&baseline, &session).expect("第一张资源必须成功");
    let second = acquire_in_session(&baseline, &session).expect("第二张资源必须成功");
    assert_ne!(
        first.resource_id, second.resource_id,
        "两张必须是不同的资源，否则这条断言毫无意义"
    );
    assert_ne!(
        first.handle.runtime_epoch, second.handle.runtime_epoch,
        "同一会话里两张资源的 epoch 必须不同，否则这一条断言毫无意义"
    );

    // 把第一张的句柄改写成「指向第二张资源」—— 模拟 `handle_from_other_resource`。
    let mut crossed = first.handle.clone();
    crossed.resource_id = second.resource_id.clone();
    match baseline.resolve(&second.resource_id, &crossed, false) {
        Err(ProviderError::RuntimeEpochMismatch(_)) => {}
        other => panic!("基线必须死在 epoch 门闸上，实际是 {other:?}"),
    }

    // --- 撞车：第二张资源重新领到 counter 1 ---
    let collided = provider();
    let session = collided.ids().next_db_session_id();
    let first = acquire_in_session(&collided, &session).expect("第一张资源必须成功");
    collided.ids().force_collision(
        crate::connection::testing::ids::FakeIdScope::RuntimeEpoch,
        1,
    );
    let second = acquire_in_session(&collided, &session).expect("第二张资源必须成功");
    assert_eq!(
        first.handle.runtime_epoch, second.handle.runtime_epoch,
        "force_collision 必须真的让两张资源共用一个 epoch，否则下面这半条断言是假的"
    );

    let mut crossed = first.handle.clone();
    crossed.resource_id = second.resource_id.clone();
    // epoch 撞车之后，**换掉的守卫**是 epoch 那一关：它不再拒绝，剩下的拒绝来自
    // `ownerToken = fnv1a64(ownerHash | runtimeEpoch | resourceId)`（`port.rs::issue`），
    // 因为它把 epoch 和 resourceId 都编进了摘要。
    match collided.resolve(&second.resource_id, &crossed, false) {
        Err(ProviderError::SessionLost(text)) => {
            assert!(
                text.contains("ownerToken"),
                "撞车后必须死在 ownerToken 那一关，实际是 {text}"
            );
        }
        other => panic!(
            "epoch 撞车后应改由 ownerToken 拒绝，实际是 {other:?} \
             —— 若这里放行，说明 ownerToken 也不再绑定 epoch，三校验已名存实亡"
        ),
    }
}

/// 固定 `dbSessionId` 的一次 `acquire`。epoch 计数器按会话分桶（`ids.rs`），
/// 所以「两张资源的 epoch 是否不同」这个前提只能靠同会话构造。
fn acquire_in_session(
    provider: &FakeResourceProvider,
    db_session_id: &DbSessionId,
) -> Result<AcquiredResource, ProviderError> {
    provider.acquire(&AcquireResourceRequest {
        descriptor: provider.descriptor(),
        pool_key: pool_key(provider, "pol-1"),
        budget_class: BudgetClass::Session,
        owner: owner(),
        db_session_id: db_session_id.clone(),
    })
}
