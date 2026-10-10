//! [`FakeHarness`] 的自测。
//!
//! `cm73.rs` 的模块注释点名了本文件：「反例由 `tests.rs` 里显式造出来，
//! 不靠『相信正例不会出错』」。所以本文件的一半内容是**判负**用例：
//! 三条反例命令、以及「宿主把旧句柄搬到恢复资源上复用」。
//!
//! 全程**不 sleep**：先后关系由 [`Barrier`] + journal `seq` 表达。
//!
//! **panic 约定**：本文件每个 `expect` / `panic!`
//! 的消息都写明「哪条不变量被打破才算失败」。命令网关返回 `Err` 属于**被测语义**
//! 时用 `match` + `panic!` 显式失败（例如用错资源/过期 epoch 的判负命令）；夹具基线路径本身
//! 不允许失败，此时才用 `expect`。

use serde_json::json;

use super::{
    first_handle_id, harness_for, owner, EvictionRaceOutcome, EvictionRaceReport, GatewayError,
};
use crate::connection::error::ProviderError;
use crate::connection::execution::{EffectOutcome, ExecutionErrorCode, SessionCommand};
use crate::connection::port::BudgetClass;
use crate::connection::session::{HandleKind, TransactionState};
use crate::connection::testing::fake_resource::{FaultKind, ResourceOp};
use crate::connection::testing::fixtures::{NS_A_KEY, NS_B_KEY};
use crate::connection::testing::harness::fixture_target;
use crate::connection::testing::journal::{HandleAction, JournalEntry, ResourceEvent};
use crate::connection::types::HandleId;

// ---------------------------------------------------------------------------
// 正例：会话句柄命令走命令定义 + 入参校验的同一条路径
// ---------------------------------------------------------------------------

/// `begin → prepare → open cursor → commit → close` 全程登记与注销成对，
/// 收尾后泄漏不变量与变化点断言必须全部收口。
#[test]
fn a_normal_session_journey_leaves_no_leak_and_balanced_change_points() {
    let harness = harness_for(NS_A_KEY);
    let acquired = harness
        .acquire(owner(), "pol-1", BudgetClass::Session)
        .expect("夹具自带的 acquire 必须成功");

    let begun = harness
        .invoke(
            SessionCommand::BeginSessionTransaction,
            &acquired.handle,
            json!({}),
        )
        .expect("begin_session_transaction 必须成功");
    let handle_id = first_handle_id(&begun);

    harness
        .invoke(
            SessionCommand::PrepareServerStatement,
            &acquired.handle,
            json!({ "name": "ps_demo" }),
        )
        .expect("prepare_server_statement 必须成功");
    // 游标句柄由 `open_session_cursor` **自己**铸造，与事务句柄是两个不同的 id
    // （`next_handle_id` 每次都从新的 executionId 派生）。关闭必须关**开出来的那一个**：
    // 拿事务句柄去关游标，事务在 commit 时已经注销，必然是 `SessionNotFound`。
    let opened = harness
        .invoke(
            SessionCommand::OpenSessionCursor,
            &acquired.handle,
            json!({ "rows": 2 }),
        )
        .expect("open_session_cursor 必须成功");
    let cursor_handle_id = first_handle_id(&opened);

    // 提交 / 关闭都要**带上句柄 id** —— 它们走同一条 `handleId` 入参。
    harness
        .invoke(
            SessionCommand::CommitSessionTransaction,
            &acquired.handle,
            json!({ "handleId": handle_id }),
        )
        .expect("commit_session_transaction 必须成功");
    harness
        .invoke(
            SessionCommand::CloseSessionCursor,
            &acquired.handle,
            json!({ "handleId": cursor_handle_id }),
        )
        .expect("close_session_cursor 必须成功");

    // 旅程终点是**关闭资源**：归还 permit、注销残留的 prepared statement 句柄、
    // 摘掉 active session，台账各口才能全部收口。
    harness
        .close(&acquired.handle)
        .expect("close_resource 必须成功");

    // 泄漏不变量 + 变化点全收口。
    if let Err(violations) = harness.assert_no_leak() {
        panic!("正常会话旅程不应留下泄漏：{violations}");
    }
}

/// 入参校验确实生效：`rows` 标了 `required`，漏掉必须被
/// `validate_command_input` 拒在网关层，而不是流到驱动。
#[test]
fn a_command_missing_a_required_field_is_rejected_at_the_gateway() {
    let harness = harness_for(NS_A_KEY);
    let acquired = harness
        .acquire(owner(), "pol-1", BudgetClass::Session)
        .expect("acquire 必须成功");

    let result = harness.invoke(
        SessionCommand::OpenSessionCursor,
        &acquired.handle,
        json!({}),
    );

    match result {
        Err(GatewayError::InvalidInput(_)) => {}
        other => panic!("缺少 required 字段必须在网关层被判负，实际是 {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 反例：三条命令的存在意义就是被判负
// ---------------------------------------------------------------------------

/// 登记与注销的**正例半**：`begin` 之后登记册必须能看到句柄。
///
/// 反例半（句柄造了但**不进**登记册 ⇒ 台账只有 `orphaned` ⇒ 孤立句柄不变式不成立）在
/// `fake_resource/tests.rs::an_orphaned_handle_...` 里，判据是登记册而不是本文件，
/// 两半分开放，避免「用登记册判孤立句柄的错」。
#[test]
fn a_registered_handle_is_visible_in_the_registry() {
    let harness = harness_for(NS_A_KEY);
    let acquired = harness
        .acquire(owner(), "pol-1", BudgetClass::Session)
        .expect("acquire 必须成功");

    harness
        .invoke(
            SessionCommand::BeginSessionTransaction,
            &acquired.handle,
            json!({}),
        )
        .expect("begin 必须成功");
    let registered = harness.registered_handle_ids_on(&acquired.resource_id);
    assert_eq!(registered.len(), 1, "正常 begin 必须恰好登记一个句柄");
    assert!(
        harness.provider().registered_handles(&acquired.resource_id) > 0,
        "登记册里必须能看到这个句柄"
    );

    // 未关闭前登记册必然不收口 —— 这条断言防止下面的收口断言恒真。
    assert!(
        !harness.journal().handle_registry().is_empty(),
        "句柄还开着，登记册就不该是空的"
    );
    harness
        .close(&acquired.handle)
        .expect("收尾 close_resource 必须成功");
    assert!(
        harness.journal().handle_registry().is_empty(),
        "关资源之后登记册必须空（I5）"
    );
}

/// `commit_session_transaction` 注入不可判定故障：结果必须**不可判定**。
///
/// 硬规则有三条：
/// 1. `effectOutcome` 必须是 `unknown`，绝不能是 `completed`；
/// 2. `errorCode` 取**实际成因**（这里是 `protocolError`），不是笼统的 `unknown`；
/// 3. **不自动再执行**，也**不返回** `TransactionResolutionRequired`。
///
/// 第 3 条的判据是台账：`transactionOperation` 上只排了一个回滚失败，脚本额度用尽后
/// 第二次提交就是基线路径，所以「有没有被自动重放」由 `script.pending` 与
/// 事务状态机共同表达 —— 这里断言 `pending == 0`（不重放、额度已被这一次用掉）。
#[test]
fn a_commit_injected_with_commit_unknown_is_never_completed_and_is_not_replayed() {
    let harness = harness_for(NS_A_KEY);
    let acquired = harness
        .acquire(owner(), "pol-1", BudgetClass::Session)
        .expect("acquire 必须成功");
    let begun = harness
        .invoke(
            SessionCommand::BeginSessionTransaction,
            &acquired.handle,
            json!({}),
        )
        .expect("begin 必须成功");
    let handle_id = first_handle_id(&begun);

    harness.script().once(
        ResourceOp::Transaction,
        FaultKind::CommitUnknown {
            code: "protocolError",
        },
    );
    let injected = harness.invoke(
        SessionCommand::CommitSessionTransaction,
        &acquired.handle,
        json!({ "handleId": handle_id }),
    );
    let injected = injected.expect("F8 注入的是**不可判定**，命令本身仍返回 completion");

    assert_eq!(
        injected
            .data
            .pointer("/effectOutcome")
            .and_then(|v| v.as_str()),
        Some(EffectOutcome::Unknown.as_str()),
        "注入不可判定故障后 effectOutcome 必须是 unknown"
    );
    assert_eq!(
        injected.data.pointer("/errorCode").and_then(|v| v.as_str()),
        Some(ExecutionErrorCode::ProtocolError.as_str()),
        "errorCode 必须取实际成因 protocolError，而不是笼统值"
    );
    assert_eq!(
        harness.script().pending(ResourceOp::Transaction),
        0,
        "F8 额度只应被这一次提交消耗掉；剩余额度就意味着存在自动重放"
    );
    assert!(
        !injected
            .data
            .to_string()
            .contains("TransactionResolutionRequired"),
        "不可判定不得回退成 TransactionResolutionRequired：{}",
        injected.data
    );

    // 不可判定不改变登记册：句柄最终归属未知，归池前置不成立。
    assert!(
        !harness.journal().handle_registry().is_empty(),
        "不可判定的提交不得顺手注销句柄"
    );
    harness
        .close(&acquired.handle)
        .expect("收尾 close_resource 必须成功");
}

/// `rollback_session_transaction` 遇到注入的回滚失败。
///
/// 断言形状：资源进 `Quarantined`、**预算占用保留**、错误是
/// `ProviderError::RollbackFailed`。台账上必须能看到那条 `Quarantined` 事件，
/// 且 `permit_balance` 仍然是 1。
#[test]
fn a_rollback_injected_with_rollback_failed_quarantines_and_keeps_the_budget() {
    let harness = harness_for(NS_A_KEY);
    let acquired = harness
        .acquire(owner(), "pol-1", BudgetClass::Session)
        .expect("acquire 必须成功");
    let begun = harness
        .invoke(
            SessionCommand::BeginSessionTransaction,
            &acquired.handle,
            json!({}),
        )
        .expect("begin 必须成功");
    let handle_id = first_handle_id(&begun);
    assert_eq!(
        harness.journal().permit_balance(),
        1,
        "acquire 之后 permit 余额必须是 1"
    );

    harness.script().once(
        ResourceOp::Transaction,
        FaultKind::RollbackFailed {
            reason: "回滚失败：连接已断".to_owned(),
        },
    );
    let result = harness.invoke(
        SessionCommand::RollbackSessionTransaction,
        &acquired.handle,
        json!({ "handleId": handle_id }),
    );
    match result {
        Err(GatewayError::Provider(ProviderError::RollbackFailed(reason))) => {
            assert_eq!(reason, "回滚失败：连接已断", "原因必须原样透出，不被吞掉");
        }
        other => panic!("F8 注入的必须是 RollbackFailed，实际是 {other:?}"),
    }

    // 台账上必须有 Quarantined，且预算**没有**归还。
    let quarantined = harness.journal().entries().iter().any(|entry| {
        matches!(
            entry,
            JournalEntry::Resource {
                event: ResourceEvent::Quarantined,
                ..
            }
        )
    });
    assert!(quarantined, "回滚失败必须留下 Quarantined 事件");
    assert_eq!(
        harness.journal().permit_balance(),
        1,
        "隔离后预算占用必须保留"
    );
    assert!(
        harness.journal().live_resources().is_empty(),
        "隔离的资源不可再被 acquire"
    );
    // 变化点规则在「live 空 + 余额 1」这个形状上必须仍然干净（见 journal 那条同名用例）。
    assert_eq!(
        harness.journal().assert().change_point_violations(),
        Vec::<String>::new(),
        "隔离形状下规则 4 应当对得平"
    );

    harness
        .close(&acquired.handle)
        .expect("收尾 close_resource 必须成功");
    if let Err(violations) = harness.assert_no_leak() {
        panic!("隔离收尾不得留下泄漏：{violations}");
    }
}

/// 反例二：提交时携带**陈旧**的 `runtimeEpoch`。
///
/// 判负靠 `ApiError::code`：`ProviderError::RuntimeEpochMismatch` —— 规定
/// 「每次操作都要校验 `resourceId` + `runtimeEpoch` + owner」，epoch 对不上就是
/// `RuntimeEpochMismatch`（不是更泛的 `SessionLost`：后者留给「资源/会话根本不在了」，
/// 两者在两条反例里必须能区分开）。
#[test]
fn a_commit_carrying_a_stale_runtime_epoch_is_rejected() {
    let harness = harness_for(NS_A_KEY);
    let acquired = harness
        .acquire(owner(), "pol-1", BudgetClass::Session)
        .expect("acquire 必须成功");

    let begun = harness
        .invoke(
            SessionCommand::BeginSessionTransaction,
            &acquired.handle,
            json!({}),
        )
        .expect("begin 必须成功");
    let handle_id = first_handle_id(&begun);

    // 真实 epoch 至少 +1，保证它一定「陈旧」。
    let stale = acquired.handle.runtime_epoch.get() + 1;
    let result = harness.invoke(
        SessionCommand::CommitWithStaleHandle,
        &acquired.handle,
        json!({ "handleId": handle_id, "runtimeEpoch": stale }),
    );

    match result {
        Err(error @ GatewayError::Provider(ProviderError::RuntimeEpochMismatch(_))) => {
            // 判负要落在 `ApiError.code` 上，不能只看是不是 Err。
            assert_eq!(
                error.api_code(),
                Some(crate::connection::error::ApiErrorCode::RuntimeEpochMismatch),
                "陈旧 epoch 的拒绝码必须是 RuntimeEpochMismatch"
            );
        }
        other => panic!("陈旧 epoch 的提交必须被拒，实际是 {other:?}"),
    }

    // 判负**不改变账本**：事务句柄仍在登记册里，归池前置不成立。
    harness
        .close(&acquired.handle)
        .expect("收尾 close_resource 必须成功");
}

/// 反例三：拿 A 资源的句柄去动 B 资源。
///
/// 判负靠 `FakeResourceProvider::resolve` 的资源归属检查 —— 句柄里带了
/// `resourceId`，跨资源使用时对不上。
#[test]
fn a_handle_from_another_resource_is_rejected() {
    let harness = harness_for(NS_A_KEY);
    let first = harness
        .acquire(owner(), "pol-1", BudgetClass::Session)
        .expect("第一张资源必须申请成功");
    let second = harness
        .acquire(owner(), "pol-1", BudgetClass::Session)
        .expect("第二张资源必须申请成功");
    assert_ne!(
        first.resource_id, second.resource_id,
        "两张资源必须不同，否则这条反例不成立"
    );

    let begun = harness
        .invoke(
            SessionCommand::BeginSessionTransaction,
            &first.handle,
            json!({}),
        )
        .expect("begin 必须成功");
    let handle_id = first_handle_id(&begun);

    // 拿着第一张资源的句柄，第二张资源找不到它 ⇒ 必须判负。
    let result = harness.invoke(
        SessionCommand::HandleFromOtherResource,
        &second.handle,
        json!({ "handleId": handle_id, "resourceId": first.resource_id.as_str() }),
    );

    assert!(
        result.is_err(),
        "跨资源使用句柄必须被拒，实际返回了 {:?}",
        result.map(|value| value.data)
    );
}

// ---------------------------------------------------------------------------
// 反例四：未登记句柄（CM-74 断言 4）
// ---------------------------------------------------------------------------

/// CM-74 断言 4：「未登记句柄被拒绝返回」——**提交**与**回滚**两条终结路径
/// 都必须在触达后端之前被 `ensure_registered` 拦下。
///
/// 这条用例存在的理由是上面那条 `a_handle_from_another_resource_is_rejected`
/// **顶不掉它**（2026-06 曾据此判过「已覆盖」，属于误判）：那条走
/// `SessionCommand::HandleFromOtherResource`，命中的是 `session_cmds.rs` 里
/// **另一套** match，判负点是资源归属检查，**根本不经过** `ensure_registered`。
/// 把 `ensure_registered` 整个删成放行，那条照样绿。
///
/// 断言分两层，缺一不可：
/// 1. **返回值**：`ProviderError::SessionNotFound` + `api_code == SessionNotFound`。
///    光有这一层只能证明「报了个错」，证明不了后端没动手。
/// 2. **后端零动作**：三个互相独立的探针 ——
///    (a) 预先注入的事务故障额度**没被消费**（`transaction_operation` 是
///    `ResourceOp::Transaction` 故障的唯一消费点）；(b) 资源上的
///    `transaction_state` 仍是反例 begin 留下的 `Active`（真跑了 Commit/Rollback
///    会被打回 `None`）；(c) 登记册仍无该句柄、孤儿表仍有它、台账没多出
///    `HandleAction::Closed`，回滚那半程还没多出 `ResourceEvent::Quarantined`
///    （回滚真跑了 F8 `RollbackFailed` 就会隔离资源并保留预算占用）。
///
/// 只测 (1) 的退化盲区：某次重构让 `ensure_registered` **报错之后仍继续调后端**，
/// 返回值照样是 `SessionNotFound`，只有 (a)(b) 能抓住。
#[test]
fn an_unregistered_handle_is_refused_by_both_commit_and_rollback_without_touching_the_backend() {
    let harness = harness_for(NS_A_KEY);
    let acquired = harness
        .acquire(owner(), "pol-1", BudgetClass::Session)
        .expect("acquire 必须成功");

    // 反例命令：句柄**铸出来**了（后续两步的前提），但**不进登记册**。
    let begun = harness
        .invoke(
            SessionCommand::BeginSessionTransactionUnregistered,
            &acquired.handle,
            json!({}),
        )
        .expect("反例 begin 必须成功：句柄已铸造、已记 orphaned，只是没登记");
    let handle_id = first_handle_id(&begun);

    // 前提自证：没登记这条不变量必须先成立，否则测的就不是断言 4。
    assert!(
        !harness.journal().handle_registry().contains_key(&handle_id),
        "前提：句柄 {handle_id} 必须不在登记册里"
    );
    assert_eq!(
        harness
            .journal()
            .orphan_handles()
            .iter()
            .map(|record| record.handle_id.as_str())
            .collect::<Vec<_>>(),
        vec![handle_id.as_str()],
        "前提：反例命令必须把句柄 {handle_id} 记成 orphaned"
    );

    // ---- 后端零动作（一）：提交 ----
    // 注入一次 F8 提交故障。真跑到后端的话，这份额度必被消耗掉。
    harness.script().once(
        ResourceOp::Transaction,
        FaultKind::CommitUnknown {
            code: "protocolError",
        },
    );
    let commit = harness.invoke(
        SessionCommand::CommitSessionTransaction,
        &acquired.handle,
        json!({ "handleId": handle_id }),
    );
    match commit {
        Err(error @ GatewayError::Provider(ProviderError::SessionNotFound(_))) => {
            assert_eq!(
                error.api_code(),
                Some(crate::connection::error::ApiErrorCode::SessionNotFound),
                "未登记句柄的提交必须报 SessionNotFound，而不是别的错误码"
            );
        }
        other => panic!("未登记句柄的提交必须被拒，实际是 {other:?}"),
    }
    assert_eq!(
        harness.script().pending(ResourceOp::Transaction),
        1,
        "后端零动作：提交被拒就不得消费注入的事务故障额度，剩余 {} 份说明后端被触达了",
        harness.script().pending(ResourceOp::Transaction)
    );
    assert_eq!(
        harness
            .provider()
            .resource(&acquired.resource_id)
            .expect("刚 acquire 的资源必然还在")
            .transaction_state,
        TransactionState::Active,
        "后端零动作：事务状态必须仍是反例 begin 留下的 Active，被打到 None 就说明 Commit 真的跑了"
    );

    // 上一份注入额度在提交这一路没被消费，这里换一份回滚故障重新起判。
    harness.script().clear();

    // ---- 后端零动作（二）：回滚 ----
    harness.script().once(
        ResourceOp::Transaction,
        FaultKind::RollbackFailed {
            reason: "回滚失败：连接已断".to_owned(),
        },
    );
    let rollback = harness.invoke(
        SessionCommand::RollbackSessionTransaction,
        &acquired.handle,
        json!({ "handleId": handle_id }),
    );
    match rollback {
        Err(error @ GatewayError::Provider(ProviderError::SessionNotFound(_))) => {
            assert_eq!(
                error.api_code(),
                Some(crate::connection::error::ApiErrorCode::SessionNotFound),
                "未登记句柄的回滚必须报 SessionNotFound，而不是别的错误码"
            );
        }
        other => panic!("未登记句柄的回滚必须被拒，实际是 {other:?}"),
    }
    assert_eq!(
        harness.script().pending(ResourceOp::Transaction),
        1,
        "后端零动作：回滚被拒就不得消费注入的事务故障额度，剩余 {} 份说明后端被触达了",
        harness.script().pending(ResourceOp::Transaction)
    );
    assert_eq!(
        harness
            .provider()
            .resource(&acquired.resource_id)
            .expect("刚 acquire 的资源必然还在")
            .transaction_state,
        TransactionState::Active,
        "后端零动作：事务状态必须仍是 Active，被打到 None 就说明 Rollback 真的跑了"
    );
    assert!(
        !harness.journal().entries().iter().any(|entry| matches!(
            entry,
            JournalEntry::Resource {
                event: ResourceEvent::Quarantined,
                ..
            }
        )),
        "后端零动作：回滚真跑了注入的 RollbackFailed 就该隔离资源并留下 Quarantined 台账；不得出现"
    );

    // ---- 后端零动作（三）：登记册 / 台账全程未变 ----
    // 句柄从头到尾没进过登记册，所以「关闭后不许有残留登记」这件事必须成立。
    assert!(
        !harness.journal().handle_registry().contains_key(&handle_id),
        "后端零动作：提交/回滚都不得让未登记句柄混进登记册"
    );
    assert!(
        !harness.journal().entries().iter().any(
            |entry| matches!(entry, JournalEntry::Handle { handle_id: id, action: HandleAction::Closed, .. } if id == &handle_id)
        ),
        "后端零动作：句柄从未登记，台账上不得出现它的 Closed 注销事件"
    );

    harness.script().clear();
    harness
        .close(&acquired.handle)
        .expect("收尾 close_resource 必须成功");
    // 孤儿句柄必须在关资源时被回收（I7），否则上面「零动作」只是把泄漏推后了。
    if let Err(violations) = harness.assert_no_leak() {
        panic!("未登记句柄这条反例的收尾泄漏不变量不成立：{violations}");
    }
}

// ---------------------------------------------------------------------------
// 空闲驱逐 vs 会话事务
// ---------------------------------------------------------------------------

/// 正例：五步竞态跑完，R2 上**不得**出现任何句柄登记，
/// 恢复用的 `dbSessionId` 必须是**新的**，收尾后全部不变式都成立。
#[test]
fn the_eviction_race_recovers_on_a_fresh_resource_without_reusing_handles() {
    let harness = harness_for(NS_A_KEY);
    let report: EvictionRaceReport = harness
        .script_hold_for_eviction_then_begin_commit(owner(), "pol-1", 5)
        .expect("五步编排必须跑通");

    // 停住那一刻开的事务必须落在 R1 上（不是新资源）。
    let race_handles = harness.registered_handle_ids_on(&report.pre_close_resource_id);
    assert_eq!(
        race_handles.len(),
        1,
        "停住那一刻的事务句柄必须登记在被驱逐的资源 R1 上"
    );
    assert_ne!(report.recovery_resource_id, report.pre_close_resource_id);

    // 第 5 步：恢复出来的 dbSessionId 必须是新的。
    assert_ne!(
        report.recovery_db_session_id, report.pre_close_db_session_id,
        "恢复路径必须换一个新的 dbSessionId，不得复用旧会话 id"
    );

    // 核心判负：R2 上不得有句柄登记。
    assert_eq!(report.outcome, EvictionRaceOutcome::HandlesNotReused);
    assert_eq!(report.registered_on_recovery, 0);
    harness
        .assert_no_handle_reuse(&report.recovery_resource_id)
        .unwrap_or_else(|reason| panic!("判负（正例不该触发）：{reason}"));

    // 收尾：R2 是**仍然开着**的恢复资源，不关它 live 资源 / active session /
    // permit 收支就不可能收口 —— R1 已在第 4 步归还 permit，差的正好是 R2 那一张。
    harness
        .close(&report.recovery_handle)
        .expect("收尾必须把恢复资源 R2 也走一遍正常关闭");

    if let Err(violations) = harness.assert_no_leak() {
        panic!("收尾不得留下泄漏：{violations}");
    }
}

/// 反例：宿主作弊 —— 把旧句柄在恢复资源 R2 上补登记一次。
///
/// 这条用例的**唯一**目的是让 `assert_no_handle_reuse` 判负：只验正例
/// 的话，「没有登记」和「断言根本没跑」是分不开的。
#[test]
fn a_host_that_re_registers_the_old_handle_on_the_recovery_resource_is_caught() {
    let harness = harness_for(NS_A_KEY);
    let report = harness
        .script_hold_for_eviction_then_begin_commit(owner(), "pol-1", 5)
        .expect("五步编排必须跑通");

    // 作弊：把 R1 的旧句柄在 R2 上再登记一次（`register_handle` 是公开的）。
    let cheater = harness
        .provider()
        .register_handle(
            &report.recovery_resource_id,
            HandleKind::Transaction,
            HandleId::new(report.race_handle_id.as_str()),
            None,
        )
        .expect("register 必须成功");
    assert_eq!(
        cheater.resource_id, report.recovery_resource_id,
        "作弊登记必须落在 R2 上"
    );

    // 判负：R2 上出现了 1 条登记。
    assert_eq!(
        harness
            .registered_handle_ids_on(&report.recovery_resource_id)
            .len(),
        1,
        "作弊必须在台账上留下痕迹，否则这条反例是恒真的"
    );
    let rejection = harness
        .assert_no_handle_reuse(&report.recovery_resource_id)
        .expect_err("判负：复用旧句柄必须被抓出来");
    assert!(
        rejection.contains("不得复用旧句柄"),
        "判负信息必须说清是复用了旧句柄，实际是：{rejection}"
    );
}

// ---------------------------------------------------------------------------
// 未登记句柄的显式收口
// ---------------------------------------------------------------------------

/// `Close` 之后登记册必须空，孤立句柄必须空。
/// 用命令级路径跑一遍，验的是「关句柄」这条写路径真的回写登记册。
#[test]
fn closing_a_session_handle_empties_the_registry() {
    let harness = harness_for(NS_A_KEY);
    let acquired = harness
        .acquire(owner(), "pol-2", BudgetClass::Session)
        .expect("acquire 必须成功");
    let begun = harness
        .invoke(
            SessionCommand::BeginSessionTransaction,
            &acquired.handle,
            json!({}),
        )
        .expect("begin 必须成功");
    let handle_id = first_handle_id(&begun);

    assert!(
        !harness.journal().handle_registry().is_empty(),
        "begin 之后登记册必须非空"
    );

    harness
        .invoke(
            SessionCommand::RollbackSessionTransaction,
            &acquired.handle,
            json!({ "handleId": handle_id }),
        )
        .expect("rollback 必须成功");

    // 回滚之后句柄注销：登记册里没有仍开启的句柄。
    let still_open: Vec<_> = harness.journal().open_handles();
    assert!(
        still_open.is_empty(),
        "回滚之后不得仍有开启的句柄，实际还有 {still_open:?}"
    );
    // 台账上必须成对：一次 Registered、一次 Closed。
    let actions: Vec<HandleAction> = harness
        .journal()
        .entries()
        .iter()
        .filter_map(|entry| match entry {
            JournalEntry::Handle {
                action,
                handle_id: owner_id,
                ..
            } if *owner_id == handle_id => Some(*action),
            _ => None,
        })
        .collect();
    assert_eq!(
        actions,
        vec![HandleAction::Registered, HandleAction::Closed],
        "句柄在台账上必须恰好经历一次登记与一次注销"
    );
}

// ---------------------------------------------------------------------------
// 命名空间隔离（NS_A / NS_B 是两个独立夹具世界）
// ---------------------------------------------------------------------------

/// 两个命名空间是两个独立夹具资源，互不串台。
/// 夹具世界本身要能分别造出目标不同的两个 `FakeHarness`。
#[test]
fn the_two_fixture_namespaces_derive_distinct_targets() {
    let a = fixture_target(NS_A_KEY);
    let b = fixture_target(NS_B_KEY);
    assert_ne!(
        a.namespace, b.namespace,
        "两个夹具命名空间必须给出不同的执行目标"
    );
    assert_eq!(
        a.connection_id, b.connection_id,
        "两个命名空间共用 PROFILE_P 连接"
    );
}
