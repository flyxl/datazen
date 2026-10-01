//! [`FakeHarness`] 的自测（fake-runtime-fixtures.md §4.3、§9.1、§9.2、§9.3）。
//!
//! `cm73.rs` 的模块注释点名了本文件：「反例由 `tests.rs` 里显式造出来，
//! 不靠『相信正例不会出错』」。所以本文件的一半内容是**判负**用例：
//! §9.2 的三条反例命令、以及 §9.3「宿主把旧句柄搬到恢复资源上复用」。
//!
//! 全程**不 sleep**：CM-73 的先后关系由 [`Barrier`] + journal `seq` 表达（§6 L357）。
//!
//! **panic 约定**（docs/development/panic-policy.md）：本文件每个 `expect` / `panic!`
//! 的消息都写明「哪条不变量被打破才算失败」。命令网关返回 `Err` 属于**被测语义**
//! 时用 `match` + `panic!` 显式失败（例如 §9.2 的判负命令）；夹具基线路径本身
//! 不允许失败，此时才用 `expect`。

use serde_json::json;

use super::{EvictionRaceOutcome, EvictionRaceReport, FakeHarness, GatewayError};
use crate::connection::error::ProviderError;
use crate::connection::execution::{EffectOutcome, ExecutionErrorCode, SessionCommand};
use crate::connection::port::BudgetClass;
use crate::connection::session::HandleKind;
use crate::connection::testing::fake_resource::{FakeResourceProvider, FaultKind, ResourceOp};
use crate::connection::testing::fixtures::{self, NS_A_KEY, NS_B_KEY};
use crate::connection::testing::harness::fixture_target;
use crate::connection::testing::journal::{HandleAction, JournalEntry, ResourceEvent};
use crate::connection::types::{HandleId, JobId, OrganizationId, OwnerRef, WorkerId};
use datazen_driver_api::command::CommandResult;

/// §8.1：夹具目标取自 `fixtures`，用例里不写硬编码命名空间字面量。
fn harness_for(namespace_key: &str) -> FakeHarness {
    FakeHarness::from_provider(
        FakeResourceProvider::new(WorkerId::new("w1"), fixture_target(namespace_key))
            .with_execution_identity(fixtures::IDENTITY_SHARED),
    )
}

/// §8.1 `PROFILE_P` 归属：一个 job owner。
fn owner() -> OwnerRef {
    OwnerRef::Job {
        organization_id: OrganizationId::new(fixtures::ORG_A),
        job_id: JobId::new("job_org-alpha_0001"),
        stage_id: "job:job_org-alpha_0001/stage:1".to_owned(),
    }
}

/// 从 `CommandResult.data` 里取第一条会话句柄的 id。
///
/// §9.1 的 output schema 已经声明了 `sessionHandles[].handleId`，所以这里
/// 只做形状检查；形状漂移用 `expect` 报出来是刻意的 —— 它意味着命令表变了。
fn first_handle_id(result: &CommandResult) -> String {
    result
        .data
        .pointer("/sessionHandles/0/handleId")
        .and_then(|value| value.as_str())
        .unwrap_or_else(|| {
            panic!(
                "命令输出里没有 sessionHandles[0].handleId，实际是 {}",
                result.data
            )
        })
        .to_owned()
}

// ---------------------------------------------------------------------------
// §9.1 正例：会话句柄命令走命令定义 + 入参校验的同一条路径
// ---------------------------------------------------------------------------

/// §9.1：`begin → prepare → open cursor → commit → close` 全程登记与注销成对，
/// 收尾后 §4.3 的 I1–I8 与 §5.3 的变化点断言必须全部收口。
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

    // 提交 / 关闭都要**带上句柄 id** —— §9.1 要求它们走同一条 `handleId` 入参。
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
    // 摘掉 active session，I1/I2/I4/I5/I6 才能全部收口。
    harness
        .close(&acquired.handle)
        .expect("close_resource 必须成功");

    // §4.3 I1–I8 + §5.3 变化点全收口。
    if let Err(violations) = harness.assert_no_leak() {
        panic!("正常会话旅程不应留下泄漏：{violations}");
    }
}

/// §9.1 的入参校验确实生效：`rows` 标了 `required`，漏掉必须被
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
// §9.2 反例：三条命令的存在意义就是被判负
// ---------------------------------------------------------------------------

/// §9.2 登记与注销的**正例半**：`begin` 之后登记册必须能看到句柄。
///
/// 反例半（句柄造了但**不进**登记册 ⇒ 台账只有 `orphaned` ⇒ I7）在
/// `fake_resource/tests.rs::an_orphaned_handle_...` 里，判据是 I7 而不是本文件，
/// 两半分开放，避免「用 I5 判 I7 的错」。
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

    // 未关闭前 I5 必然不成立 —— 这条断言防止下面的 I5 收口恒真。
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

/// §9.2 `commit_session_transaction` 注入 F9：结果**不可判定**。
///
/// 硬规则有三条（doc :472、:774）：
/// 1. `effectOutcome` 必须是 `unknown`，绝不能是 `completed`；
/// 2. `errorCode` 取**实际成因**（这里是 `protocolError`），不是笼统的 `unknown`；
/// 3. **不自动再执行**，也**不返回** `TransactionResolutionRequired`。
///
/// 第 3 条的判据是台账：`transactionOperation` 上只排了一个 F9，脚本额度用尽后
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
    let injected = injected.expect("F9 注入的是**不可判定**，命令本身仍返回 completion");

    assert_eq!(
        injected
            .data
            .pointer("/effectOutcome")
            .and_then(|v| v.as_str()),
        Some(EffectOutcome::Unknown.as_str()),
        "F9 之后 effectOutcome 必须是 unknown（§3.2）"
    );
    assert_eq!(
        injected.data.pointer("/errorCode").and_then(|v| v.as_str()),
        Some(ExecutionErrorCode::ProtocolError.as_str()),
        "errorCode 必须取实际成因 protocolError，而不是笼统值（§9.2）"
    );
    assert_eq!(
        harness.script().pending(ResourceOp::Transaction),
        0,
        "F9 额度只应被这一次提交消耗掉；剩余额度就意味着存在自动重放"
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

/// §9.2 `rollback_session_transaction` 注入 F10：回滚失败。
///
/// 断言形状（doc :473）：资源进 `Quarantined`、**预算占用保留**、错误是
/// `ProviderError::RollbackFailed`。台账上必须能看到那条 `Quarantined` 事件，
/// 且 `permit_balance` 仍然是 1（§5.3 规则 3）。
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
        other => panic!("F10 注入的必须是 RollbackFailed，实际是 {other:?}"),
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
        "隔离后预算占用必须保留（§9.2 / §5.3 规则 3）"
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

/// §9.2 反例二：提交时携带**陈旧**的 `runtimeEpoch`。
///
/// 判负靠 `ApiError::code`：`ProviderError::RuntimeEpochMismatch` —— §3.1 规定
/// 「每次操作都要校验 `resourceId` + `runtimeEpoch` + owner」，epoch 对不上就是
/// `RuntimeEpochMismatch`（不是更泛的 `SessionLost`：后者留给「资源/会话根本不在了」，
/// 两者在 §9.2 的两条反例里必须能区分开）。
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
            // §9.2：判负要落在 `ApiError.code` 上，不能只看是不是 Err。
            assert_eq!(
                error.api_code(),
                Some(crate::connection::error::ApiErrorCode::RuntimeEpochMismatch),
                "陈旧 epoch 的拒绝码必须是 RuntimeEpochMismatch"
            );
        }
        other => panic!("陈旧 epoch 的提交必须被拒，实际是 {other:?}"),
    }

    // 判负**不改变账本**：事务句柄仍在登记册里，归池前置不成立（§9.2）。
    harness
        .close(&acquired.handle)
        .expect("收尾 close_resource 必须成功");
}

/// §9.2 反例三：拿 A 资源的句柄去动 B 资源。
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
// §9.3 CM-73：空闲驱逐 vs 会话事务
// ---------------------------------------------------------------------------

/// §9.3 正例：五步竞态跑完，R2 上**不得**出现任何句柄登记，
/// 恢复用的 `dbSessionId` 必须是**新的**，收尾后 I1–I8 全成立。
#[test]
fn the_eviction_race_recovers_on_a_fresh_resource_without_reusing_handles() {
    let harness = harness_for(NS_A_KEY);
    let report: EvictionRaceReport = harness
        .script_hold_for_eviction_then_begin_commit(owner(), "pol-1", 5)
        .expect("§9.3 编排必须跑通");

    // 停住那一刻开的事务必须落在 R1 上（不是新资源）。
    let race_handles = harness.registered_handle_ids_on(&report.pre_close_resource_id);
    assert_eq!(
        race_handles.len(),
        1,
        "停住那一刻的事务句柄必须登记在被驱逐的资源 R1 上"
    );
    assert_ne!(report.recovery_resource_id, report.pre_close_resource_id);

    // §9.3 第 5 步：恢复出来的 dbSessionId 必须是新的。
    assert_ne!(
        report.recovery_db_session_id, report.pre_close_db_session_id,
        "恢复路径必须换一个新的 dbSessionId，不得复用旧会话 id"
    );

    // 核心判负：R2 上不得有句柄登记。
    assert_eq!(report.outcome, EvictionRaceOutcome::HandlesNotReused);
    assert_eq!(report.registered_on_recovery, 0);
    harness
        .assert_no_handle_reuse(&report.recovery_resource_id)
        .unwrap_or_else(|reason| panic!("§9.3 判负（正例不该触发）：{reason}"));

    // 收尾：R2 是**仍然开着**的恢复资源，不关它 I2（live 资源）/ I4（active session）/
    // I1+I6（permit 收支）就不可能收口 —— R1 已在第 4 步归还 permit，差的正好是 R2 那一张。
    harness
        .close(&report.recovery_handle)
        .expect("收尾必须把恢复资源 R2 也走一遍正常关闭");

    if let Err(violations) = harness.assert_no_leak() {
        panic!("§9.3 收尾不得留下泄漏：{violations}");
    }
}

/// §9.3 反例：宿主作弊 —— 把旧句柄在恢复资源 R2 上补登记一次。
///
/// 这条用例的**唯一**目的是让 `assert_no_handle_reuse` 判负：只验正例
/// 的话，「没有登记」和「断言根本没跑」是分不开的。
#[test]
fn a_host_that_re_registers_the_old_handle_on_the_recovery_resource_is_caught() {
    let harness = harness_for(NS_A_KEY);
    let report = harness
        .script_hold_for_eviction_then_begin_commit(owner(), "pol-1", 5)
        .expect("§9.3 编排必须跑通");

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
        .expect_err("§9.3 判负：复用旧句柄必须被抓出来");
    assert!(
        rejection.contains("不得复用旧句柄"),
        "判负信息必须说清是复用了旧句柄，实际是：{rejection}"
    );
}

// ---------------------------------------------------------------------------
// §4.3：未登记句柄的显式收口
// ---------------------------------------------------------------------------

/// §4.3 I5/I7：`Close` 之后登记册必须空，孤立句柄必须空。
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
// §8.1：命名空间隔离（NS_A / NS_B 是两个独立夹具世界）
// ---------------------------------------------------------------------------

/// §8.1：两个命名空间是两个独立夹具资源，互不串台。
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
