//! §4.5 CM-72 审计条目 / D-02 出口折叠 / CM-58 额度查询 —— 登记表的可观测面。
//!
//! 这一支只断言三件事，且都只从公开导出进入：
//!
//! 1. **审计条目不带凭据**（CM-72）：结构里没有敏感字段，能力位只记版本号；
//!    取消条目与额度条目都不许写 `effectOutcome`（取消被受理不等于数据已回滚）。
//! 2. **对外错误码由唯一折叠函数产出**（D-02）：会话层与 provider 层两个事实空间
//!    每一个变体都有确定结论；`hostRejected` 这类宿主缺陷**不上线路**；
//!    `RuntimeEpochMismatch` 刻意改判成 `sessionNotFound`（见 `epoch.rs` 的注释）。
//! 3. **审计的两种读法语义不同**（`drain_audit` 增量 / `audit_log` 累积）：
//!    只读投影会顺手把审计队列抽干，这一点必须钉住，否则所有「存在性」断言都会随机失败。
//!
//! | 用例 | 分支 | 权威 |
//! | --- | --- | --- |
//! | 会话层九个变体逐一有确定结论 | `fold_exit(ExitFact::Session)` | D-02 |
//! | 陈旧世代改判成会话未找到 | `ExitFact::Provider` 特例 | D-02 |
//! | 宿主拒绝对外不上线路 | `NotOnTheWire` | §13 出口投影 |
//! | 取消回执线格式键集固定三项 | `CancelReceipt` serde | §7.6 / D-01 |
//! | 审计条目只带版本不带连接串 | `CapabilityVersions::is_version_only` | CM-72 |
//! | 登记成功只留一条带能力版本的条目 | `SessionRegistered` | §4.5 |
//! | 执行终态是唯一带效果判定的条目 | `ExecutionCompleted` | CM-72 正交 |
//! | 取消成功不改写数据效果 | `CancelResolved` 无 `effectOutcome` | CM-72 正交 |
//! | 收干只给增量而累积账保留全部 | `drain_audit` vs `audit_log` | §4.5 |
//! | 额度只由登记表账本回答 | `remaining_quota` / `stale_quota_for` | CM-58 |

#![allow(dead_code)]
mod registry_fixtures;

use std::sync::Arc;

use datazen_runtime::connection::error::ApiErrorCode;
use datazen_runtime::connection::{
    ExecutionErrorCode, ExecutionId, ExecutionState, ProviderError, RuntimeError,
};
use datazen_runtime::registry::{
    fold_exit, AuditKind, CancelDisposition, CancelReceipt, CapabilityVersions, ExitFact,
    ExitProjection, Outcome, RegistryAuditEntry, SessionPort, SessionRegistry,
};

use registry_fixtures::{
    as_backend, close_any, db_session_id, execute_request, handle_of, open_request, register_ready,
    wait_until, worker_id, BackendPlan, Script, ScriptedBackend, SESSION_LIMIT,
};

fn registry_with(backend: &Arc<ScriptedBackend>) -> Arc<SessionRegistry> {
    Arc::new(SessionRegistry::new(as_backend(backend), SESSION_LIMIT))
}

/// 折叠后的对外码字面量。`None` 表示「不上线路」，调用方拿不到可判读的码。
fn wire_literal(projection: ExitProjection) -> Option<&'static str> {
    projection.code().map(|code| code.as_str())
}

/// 会话层错误空间的期望结论。全部九个变体都必须在表里：
/// 少写一个，这个用例就退化成「有写到的都对了」，新加的变体反而无人管。
///
/// 建成函数而不是 `const`：表里有 `String` 载荷，`to_owned` 不是 const fn。
fn session_facts() -> Vec<(&'static str, RuntimeError, Option<&'static str>)> {
    vec![
        (
            "unknownSession",
            RuntimeError::UnknownSession("dbs_x".to_owned()),
            Some("sessionNotFound"),
        ),
        (
            "sessionClosed",
            RuntimeError::SessionClosed("dbs_x".to_owned()),
            Some("sessionNotFound"),
        ),
        (
            "sessionLost",
            RuntimeError::SessionLost("dbs_x".to_owned()),
            Some("sessionLost"),
        ),
        (
            "sessionQuarantined",
            RuntimeError::SessionQuarantined("quarantined"),
            Some("sessionLost"),
        ),
        (
            "contextRevisionMismatch",
            RuntimeError::ContextRevisionMismatch {
                expected: 1,
                actual: 2,
            },
            Some("contextConflict"),
        ),
        (
            "budgetExhausted",
            RuntimeError::BudgetExhausted("budget"),
            Some("resourceBusy"),
        ),
        (
            "closeRejected",
            RuntimeError::CloseRejected("transactionInProgress"),
            Some("transactionResolutionRequired"),
        ),
        // 取消失败与不变量破裂都是**宿主内部**结论：叫得出来的原因已经在
        // `CancelFailed(..)` / `InvariantBroken(..)` 的载荷里，给一个对外码
        // 等于让调用方以为那是可以照着处理的错误。
        (
            "cancelFailed",
            RuntimeError::CancelFailed("cancelBindingMismatch"),
            None,
        ),
        (
            "invariantBroken",
            RuntimeError::InvariantBroken("executionAlreadyInFlight"),
            None,
        ),
    ]
}

#[test]
fn 会话层九个变体逐一有确定结论() {
    assert_eq!(
        session_facts().len(),
        9,
        "会话层变体共九个，表里少了说明折叠表有分支没被这个用例盯住"
    );
    for (name, error, expected) in &session_facts() {
        let projection = fold_exit(ExitFact::Session(error));
        match expected {
            Some(literal) => assert_eq!(
                wire_literal(projection),
                Some(*literal),
                "{name} 的对外码必须唯一确定"
            ),
            None => {
                assert!(
                    matches!(projection, ExitProjection::NotOnTheWire { .. }),
                    "{name} 是宿主内部结论，不得编一个对外码出去，实际 {projection:?}"
                );
                assert!(!projection.is_on_the_wire());
                assert!(projection.code().is_none());
            }
        }
    }
}

#[test]
fn 陈旧世代经_provider_面折叠成会话未找到() {
    let mismatch = ProviderError::RuntimeEpochMismatch("epoch 6 != 7".to_owned());
    // provider 面自己把它报成「世代错配」……
    assert_eq!(
        mismatch.api_code().map(|code| code.as_str()),
        Some("runtimeEpochMismatch"),
        "provider 面的原始分类应当是世代错配，断言它是为了让下面的改判有意义"
    );
    // ……但出口折叠刻意改判：调用方拿到的句柄已经过期，
    // 能做的动作只有「重读会话」，「世代错配」会让它去比对一个它已经不该持有的世代。
    let projection = fold_exit(ExitFact::Provider(&mismatch));
    assert_eq!(wire_literal(projection), Some("sessionNotFound"));
    assert_eq!(
        projection.code(),
        Some(ApiErrorCode::SessionNotFound),
        "刻意偏差必须钉成字面量，改判回去会静默改变 §12.1 的语义"
    );
}

/// 全部对外码。审计里出现的宿主原字一旦落进这张表，就意味着它会被原样放上线。
fn all_api_codes() -> Vec<ApiErrorCode> {
    vec![
        ApiErrorCode::InvalidArgument,
        ApiErrorCode::TargetRequired,
        ApiErrorCode::TargetConflict,
        ApiErrorCode::TargetUnsupported,
        ApiErrorCode::SessionNotFound,
        ApiErrorCode::SessionLost,
        ApiErrorCode::RuntimeEpochMismatch,
        ApiErrorCode::ContextConflict,
        ApiErrorCode::PermissionDenied,
        ApiErrorCode::ResourceBusy,
        ApiErrorCode::QueueFull,
        ApiErrorCode::TransactionResolutionRequired,
        ApiErrorCode::CapabilityUnsupported,
        ApiErrorCode::UnsupportedPlan,
        ApiErrorCode::EndpointOverlap,
        ApiErrorCode::SessionQuotaExceeded,
        ApiErrorCode::IdempotencyExpired,
        ApiErrorCode::RollbackFailed,
        ApiErrorCode::CleanupFailed,
        ApiErrorCode::OutcomeUnknown,
    ]
}

#[test]
fn 宿主原字不是任何对外码() {
    // 审计逐字记的是 ExecutionErrorCode 的写法……
    assert_eq!(ExecutionErrorCode::HostRejected.as_str(), "hostRejected");
    for code in all_api_codes() {
        assert_ne!(
            code.as_str(),
            "hostRejected",
            "宿主原字混进了对外码表，它就会被原样放上线"
        );
    }
    // ……而 provider 面那一支（冻结层 connection/error.rs:136）**有意**把它归成
    // invalidArgument：调用方据此重读参数后重发。这条刻意偏差不是缺陷，
    // 与 `RuntimeEpochMismatch → sessionNotFound` 是同一族刻意决定，见 epoch.rs 模块文档。
    let rejected = ProviderError::HostRejected("executeRejected".to_owned());
    let projection = fold_exit(ExitFact::Provider(&rejected));
    assert_eq!(wire_literal(projection), Some("invalidArgument"));
    assert_eq!(
        projection,
        ExitProjection::Code(ApiErrorCode::InvalidArgument)
    );
}

#[test]
fn 取消回执的线格式键集固定为三项() {
    let receipt = CancelReceipt::normalize(
        ExecutionId::new("exec_1_1".to_owned()),
        ExecutionState::Running,
        true,
    );
    let json = serde_json::to_value(&receipt).expect("回执必须可序列化");
    let object = json.as_object().expect("回执必须是 JSON 对象");
    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec!["disposition", "executionId", "state"],
        "回执的线格式键集就是这三个，多一个就意味着审计或凭据混进了返回体"
    );
    assert_eq!(receipt.disposition, CancelDisposition::Requested);
    assert_eq!(receipt.state, ExecutionState::Running);
}

#[test]
fn 审计条目只带版本不带连接凭据() {
    let entry = RegistryAuditEntry::registered(
        db_session_id(),
        1,
        CapabilityVersions::new("1.0.0", "2.3.1"),
    );
    let json = serde_json::to_string(&entry).expect("审计条目必须可序列化");
    assert!(
        !json.contains("://"),
        "审计条目里出现了 URL，等于把连接串写进了审计：{json}"
    );
    let versions = entry
        .capability_versions
        .as_ref()
        .expect("登记条目必须带能力版本");
    assert!(versions.is_version_only());
    // 反例钉死判据本身：把 DSN 塞进「能力版本」必须被抓住，
    // 否则这个宽松判据等于没有判据。
    let leaked = CapabilityVersions::new("postgres://user:pw@host/db", "2.3.1");
    assert!(
        !leaked.is_version_only(),
        "像 DSN 的能力版本说明调用方塞了连接串，宽松判据会让它进审计"
    );
}

#[tokio::test(start_paused = true)]
async fn 宿主拒绝原字进审计且不占额度() {
    let backend = ScriptedBackend::new(BackendPlan::default().open_fails()).await;
    let registry = registry_with(&backend);
    assert!(
        registry
            .register_session(open_request(db_session_id()))
            .await
            .is_err(),
        "物理打开失败不得登记成功"
    );
    assert_eq!(
        registry.remaining_quota(),
        SESSION_LIMIT,
        "打开失败必须把占掉的额度退回，不得留下永久占用"
    );

    let entries = registry.audit_log();
    assert_eq!(
        entries.len(),
        1,
        "打开失败只留一条审计，实际 {:?}",
        entries.iter().map(|e| e.kind).collect::<Vec<_>>()
    );
    let entry = &entries[0];
    assert_eq!(entry.kind, AuditKind::SessionRegistrationFailed);
    assert_eq!(entry.outcome, Outcome::Rejected);
    assert_eq!(entry.db_session_id, db_session_id());
    assert_eq!(
        entry.runtime_epoch, 1,
        "世代在打开之前就已发放，审计里必须记下"
    );
    assert_eq!(
        entry.error_code,
        Some("hostRejected"),
        "审计必须逐字记宿主的原判定，不能就地改写成对外码"
    );
    assert_eq!(
        entry.capability_versions, None,
        "物理资源没开出来，没有能力版本可记"
    );
    assert_eq!(entry.handle_count, 0);
    assert_eq!(entry.effect_outcome, None);
}

#[tokio::test(start_paused = true)]
async fn 登记成功只留一条带能力版本的条目() {
    let backend = ScriptedBackend::new(BackendPlan::default()).await;
    let registry = registry_with(&backend);
    register_ready(&registry, db_session_id()).await;

    let entries = registry.audit_log();
    assert_eq!(
        entries.len(),
        1,
        "一次成功登记只应留一条审计，实际留了 {:?}",
        entries.iter().map(|e| e.kind).collect::<Vec<_>>()
    );
    let entry = &entries[0];
    assert_eq!(entry.kind, AuditKind::SessionRegistered);
    assert_eq!(entry.outcome, Outcome::Succeeded);
    assert_eq!(entry.db_session_id, db_session_id());
    assert_eq!(
        entry.runtime_epoch, 1,
        "世代由登记表自己发放，第一个会话必然是 1"
    );
    assert_eq!(
        entry.runtime_epoch,
        registry
            .epoch_of(&db_session_id())
            .expect("在册会话必有世代")
            .get(),
        "审计里的世代必须与登记表对外报的世代逐字相同"
    );
    let versions = entry
        .capability_versions
        .as_ref()
        .expect("登记条目必须带能力版本");
    assert!(versions.is_version_only());
    assert_eq!(entry.handle_count, 0, "刚登记还没有任何句柄");
    assert_eq!(entry.error_code, None);
    assert_eq!(
        entry.effect_outcome, None,
        "登记不是一次执行，没有物理层效果可写"
    );
    assert_eq!(entry.execution_state, None);
    assert_eq!(entry.disposition, None);
}

#[tokio::test(start_paused = true)]
async fn 执行终态是唯一带效果判定的条目() {
    let backend = ScriptedBackend::new(BackendPlan::default()).await;
    let registry = registry_with(&backend);
    let view = register_ready(&registry, db_session_id()).await;

    let receipt = registry
        .submit_execution(execute_request(&handle_of(&view), 1))
        .await
        .expect("执行必须被受理");
    assert!(
        receipt.execution_id.to_string().starts_with("exec_1_"),
        "执行 id 必须带上登记表发放的世代，实际 {}",
        receipt.execution_id
    );
    assert!(
        wait_until(|| registry
            .audit_log()
            .iter()
            .any(|entry| entry.kind == AuditKind::ExecutionCompleted))
        .await,
        "执行结束后必须留下一条终态审计"
    );

    let entries = registry.audit_log();
    let completed: Vec<&RegistryAuditEntry> = entries
        .iter()
        .filter(|entry| entry.kind == AuditKind::ExecutionCompleted)
        .collect();
    assert_eq!(completed.len(), 1, "一次执行只留一条终态审计");
    let entry = completed[0];
    assert_eq!(entry.outcome, Outcome::Succeeded);
    assert_eq!(entry.execution_state, Some("succeeded"));
    assert_eq!(
        entry.effect_outcome,
        Some("completed"),
        "效果判定只能来自物理层，夹具这一支报 Completed"
    );
    // 能力版本由 `emit` 从当前物理资源统一填（audit 侧只有一处填法），
    // 所以执行终态也带一份；它必须与登记那条逐字相同，不能是另一次快照。
    let versions = entry
        .capability_versions
        .as_ref()
        .expect("执行终态条目同样带物理资源的能力版本");
    assert!(versions.is_version_only());
    assert_eq!(versions.contract, "1.0.0");
    assert_eq!(versions.driver_api, "2.3.1");
    // 非执行类条目一律不许带效果判定。
    for other in entries
        .iter()
        .filter(|e| e.kind != AuditKind::ExecutionCompleted)
    {
        assert_eq!(
            other.effect_outcome, None,
            "{:?} 不是执行终态，不许带 effectOutcome",
            other.kind
        );
    }
}

#[tokio::test(start_paused = true)]
async fn 取消成功不改写数据效果只留处置() {
    let backend = ScriptedBackend::new(BackendPlan::default().with_script(Script::TwoStage)).await;
    let gate = backend.gate();
    let registry = registry_with(&backend);
    let view = register_ready(&registry, db_session_id()).await;
    let handle = handle_of(&view);
    let in_flight = ExecutionId::new("exec_1_1".to_owned());

    let flying = {
        let registry = Arc::clone(&registry);
        let request = execute_request(&handle, 1);
        async move { registry.submit_execution(request).await }
    };
    tokio::spawn(flying);
    assert!(
        wait_until(|| backend.execute_calls() == 1).await,
        "执行必须先真的下发到后端"
    );
    gate.send(()).expect("闸门接收端仍在运行");
    assert!(
        wait_until(|| backend.published_calls() == 1).await,
        "两道闸门之间必须已完成 cancelHandle 公布"
    );

    let receipt = registry
        .cancel_execution(&handle, &in_flight)
        .await
        .expect("取消必须被受理");
    assert_eq!(receipt.disposition, CancelDisposition::Requested);

    let cancels: Vec<RegistryAuditEntry> = registry
        .audit_log()
        .into_iter()
        .filter(|entry| entry.kind == AuditKind::CancelResolved)
        .collect();
    assert_eq!(cancels.len(), 1, "一次取消只留一条审计");
    assert_eq!(cancels[0].disposition, Some("requested"));
    assert_eq!(cancels[0].execution_state, Some("cancelRequested"));
    assert_eq!(
        cancels[0].effect_outcome, None,
        "取消请求被受理不代表数据已回滚，写 effectOutcome 就是让宿主覆盖物理层判定（CM-72）"
    );
    assert_eq!(cancels[0].error_code, None);

    gate.send(()).expect("收尾：放掉第二道闸门让执行结束");
}

#[tokio::test(start_paused = true)]
async fn 收干审计只给增量而累积账保留全部() {
    let backend = ScriptedBackend::new(BackendPlan::default()).await;
    let registry = registry_with(&backend);
    let view = register_ready(&registry, db_session_id()).await;

    // `register_session` 自己在成功分支收尾时 pump 过一次，所以登记那条
    // **不在**增量窗口里——它已经在累积账里。这是「存在性断言必须用 audit_log」
    // 的第一条根因，第二条根因在下面那次只读投影。
    assert!(
        registry.drain_audit().is_empty(),
        "登记那条已被 register_session 的收尾 pump 收走"
    );
    assert_eq!(registry.audit_log().len(), 1);

    // 关闭之后**先不**收干，让条目留在队列里给只读投影吃掉。
    registry
        .close_registered(&handle_of(&view), close_any())
        .await
        .expect("关闭必须成功");
    assert!(
        !registry.is_registered(&db_session_id()),
        "关闭成功后必须从登记表里摘除"
    );
    assert!(
        registry.drain_audit().is_empty(),
        "只读投影顺手抽干了队列，所以增量窗口到这里已经空了"
    );
    let cumulative = registry.audit_log();
    assert_eq!(
        cumulative.len(),
        2,
        "抽干是传递不是丢弃：累积账仍然是登记 + 关闭两条"
    );
    assert_eq!(cumulative[0].kind, AuditKind::SessionRegistered);
    assert_eq!(cumulative[1].kind, AuditKind::SessionClosed);
    assert!(
        registry.drain_audit().is_empty(),
        "两种读法都已读过一遍，都不该再有增量"
    );
}

#[tokio::test(start_paused = true)]
async fn 额度只由登记表账本回答() {
    let backend = ScriptedBackend::new(BackendPlan::default()).await;
    let registry = registry_with(&backend);
    assert_eq!(registry.remaining_quota(), SESSION_LIMIT);

    register_ready(&registry, db_session_id()).await;
    assert_eq!(registry.remaining_quota(), SESSION_LIMIT - 1);

    // 同一 dbSessionId 二次登记当场拒绝，且不占额度。
    assert!(registry
        .register_session(open_request(db_session_id()))
        .await
        .is_err());
    assert_eq!(registry.remaining_quota(), SESSION_LIMIT - 1);
    assert_eq!(
        registry.stale_quota_for(&worker_id()),
        0,
        "被拒绝的登记不产生陈旧额度"
    );

    // 租约失效：作废不归还，额度扣住等隔离确认。
    let lost = registry.invalidate_worker(&worker_id()).await;
    assert_eq!(lost, vec![db_session_id()]);
    assert_eq!(registry.remaining_quota(), SESSION_LIMIT - 1);
    assert_eq!(registry.stale_quota_for(&worker_id()), 1);

    let released = registry.confirm_worker_quarantined(&worker_id());
    assert_eq!(released, 1);
    assert_eq!(registry.remaining_quota(), SESSION_LIMIT);
    assert_eq!(registry.stale_quota_for(&worker_id()), 0);
}
