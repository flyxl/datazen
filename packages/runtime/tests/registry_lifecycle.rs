//! registry 集成测试：§4.1 登记前置、§4.2 会话面投影、§4.5 世代、CM-58 额度账。
//!
//! # 用例 → 分支 → 权威
//!
//! | 用例 | 分支 | 权威 |
//! | --- | --- | --- |
//! | `登记成功只投影会话面` | `open_and_publish` Ok | §4.1「登记不得早于打开成功」 |
//! | `同一_id_二次登记当场拒绝且不留痕` | `write_table().insert` 冲突 | §4.1 失败不留痕 |
//! | `打开失败不留痕不占额度` | `open_and_publish` Err → `quota.release()` | §4.1 |
//! | `额度用尽拒绝新登记` | `QuotaLedger::try_reserve` | CM-58 |
//! | `未登记句柄一律 UnknownSession` | `locate` 落空 | CM-71 精确匹配 |
//! | `旧世代句柄立即失效` | `check_epoch` | §4.5 / INV-07 |
//!
//! # 这些用例为什么值钱
//!
//! §4.1 那句「任何一步失败都**不留痕**」是纯负面断言：证它需要看额度账和表位，
//! 而不是看返回值。crate 内的 actor 单测看不到登记表与额度账，
//! 所以这层事实**只能**在集成层断言——本文件就是它的落点。

#![allow(dead_code)]

mod registry_fixtures;

use datazen_runtime::connection::{
    AttachmentState, DbSessionId, RuntimeError, SessionHandle, SessionState,
};
use datazen_runtime::registry::{AuditKind, Outcome, SessionPort, SessionRegistry};

use registry_fixtures::{
    db_session_id, execute_request, handle_of, open_request, other_db_session_id, register_ready,
    stale_handle, BackendPlan, ScriptedBackend, SESSION_LIMIT,
};
use std::sync::Arc;

fn registry_with(backend: Arc<ScriptedBackend>, limit: usize) -> SessionRegistry {
    SessionRegistry::new(backend, limit)
}

#[tokio::test(start_paused = true)]
async fn 登记成功只投影会话面且表位额度同步可见() {
    let backend = ScriptedBackend::defaults().await;
    let registry = registry_with(Arc::clone(&backend), SESSION_LIMIT);

    assert!(
        !registry.is_registered(&db_session_id()),
        "登记之前 is_registered 必须是 false（§4.1：登记不得先于打开成功）"
    );

    let view = register_ready(&registry, db_session_id()).await;

    assert!(registry.is_registered(&db_session_id()));
    assert_eq!(registry.registered_ids(), vec![db_session_id()]);
    assert_eq!(backend.open_calls(), 1, "一次登记只打开一次物理资源");
    assert_eq!(
        registry.remaining_quota(),
        SESSION_LIMIT - 1,
        "占用一格额度且只占一格"
    );

    // 投影里只能有会话面：connectionId / owner / initialTarget / observedContext，
    // 没有任何物理资源标识——`resource_id` 不在 `SessionView` 的字段表里，
    // 这条是**形状**约束，由编译期保证（后端若试着返回它，接口就通不过）。
    assert_eq!(view.state, SessionState::Ready);
    // `attachment_state` 由**附着**侧（SessionDirectory）拥有，不归登记表管：
    // 登记成功的那一刻不存在任何附着关系，投影上必须是 Detached 而不是 Attached。
    // 这条钉住「登记表不去伪造它无权决定的事实」。
    assert_eq!(view.attachment_state, AttachmentState::Detached);
    assert_eq!(view.connection_id.as_str(), "conn_1");
    assert_eq!(
        view.context_revision.get(),
        1,
        "打开成功后立刻处于第 1 个上下文世代（此后每个执行终态按后端回报推进）"
    );
    assert!(view.active_execution_id.is_none());
    assert_eq!(
        view.observed_context.effective_identity, "tester",
        "observed_context 必须是**打开后真实观测到**的，不是配置期望值"
    );

    let audit = registry.audit_log();
    let registered: Vec<_> = audit
        .iter()
        .filter(|e| e.kind == AuditKind::SessionRegistered)
        .collect();
    assert_eq!(registered.len(), 1, "一次登记一条 SessionRegistered 审计");
    assert_eq!(registered[0].outcome, Outcome::Succeeded);
    assert_eq!(
        registered[0]
            .capability_versions
            .as_ref()
            .expect("登记成功的条目必须带能力版本")
            .contract,
        "1.0.0",
        "CM-72：非敏感能力版本必须进审计"
    );
    assert_eq!(
        registered[0]
            .capability_versions
            .as_ref()
            .expect("登记成功的条目必须带能力版本")
            .driver_api,
        "2.3.1"
    );
}

#[tokio::test(start_paused = true)]
async fn 同一_id_二次登记当场拒绝且不留痕() {
    let backend = ScriptedBackend::defaults().await;
    let registry = registry_with(Arc::clone(&backend), SESSION_LIMIT);
    register_ready(&registry, db_session_id()).await;

    let again = registry
        .register_session(open_request(db_session_id()))
        .await;
    assert!(
        matches!(
            again,
            Err(RuntimeError::InvariantBroken("duplicateDbSessionId"))
        ),
        "同一 dbSessionId 的第二次登记必须当场拒绝，得到 {again:?}"
    );

    assert_eq!(
        registry.registered_ids().len(),
        1,
        "失败的那次不得覆盖或叠加表项"
    );
    assert_eq!(
        registry.remaining_quota(),
        SESSION_LIMIT - 1,
        "失败的那次不得留下已占用但无人持有的额度（§4.1 不留痕）"
    );
}

#[tokio::test(start_paused = true)]
async fn 打开失败不留痕不占额度不占表位() {
    let backend = ScriptedBackend::new(BackendPlan::default().open_fails()).await;
    let registry = registry_with(Arc::clone(&backend), SESSION_LIMIT);

    let result = registry
        .register_session(open_request(db_session_id()))
        .await;
    assert!(
        matches!(result, Err(RuntimeError::UnknownSession(_))),
        "打开失败必须不得返回成功，得到 {result:?}"
    );

    assert!(registry.registered_ids().is_empty(), "打开失败不得留下表项");
    assert!(!registry.is_registered(&db_session_id()));
    assert_eq!(
        registry.remaining_quota(),
        SESSION_LIMIT,
        "打开失败必须把额度退回去，否则额度会被幽灵会话吃掉"
    );
    assert!(
        registry
            .audit_log()
            .iter()
            .any(|e| e.kind == AuditKind::SessionRegistrationFailed),
        "打开失败必须留下 SessionRegistrationFailed 审计"
    );
}

#[tokio::test(start_paused = true)]
async fn 额度用尽拒绝新登记且不再打开物理资源() {
    let backend = ScriptedBackend::defaults().await;
    let registry = registry_with(Arc::clone(&backend), 1);
    register_ready(&registry, db_session_id()).await;

    let result = registry
        .register_session(open_request(other_db_session_id()))
        .await;
    assert!(
        matches!(result, Err(RuntimeError::BudgetExhausted(_))),
        "额度用尽必须以可重试的 BudgetExhausted 拒绝，得到 {result:?}"
    );

    assert_eq!(
        backend.open_calls(),
        1,
        "额度检查在打开之前：被拒的登记不得已经开了物理资源"
    );
    assert_eq!(registry.registered_ids().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn 未登记句柄一律_unknown_session_不定位到替代会话() {
    let backend = ScriptedBackend::defaults().await;
    let registry = registry_with(Arc::clone(&backend), SESSION_LIMIT);
    let view = register_ready(&registry, db_session_id()).await;

    // 同一配置（connectionId / owner）下、但从未登记的 dbSessionId。
    // CM-71：只有 dbSessionId + runtimeEpoch 精确匹配才算命中，配置侧字段不参与查找。
    let mut stranger = open_request(DbSessionId::new("dbs_stranger"));
    stranger.connection_id = view.connection_id.clone();
    stranger.owner = view.owner.clone();
    stranger.initial_target = view.initial_target.clone();
    let forged = SessionHandle {
        db_session_id: stranger.db_session_id.clone(),
        runtime_epoch: view.handle.runtime_epoch,
    };

    let viewed = registry.session_view(&forged).await;
    assert!(
        matches!(viewed, Err(RuntimeError::UnknownSession(_))),
        "未登记的句柄必须拿到 UnknownSession，而不是被配置字段拉到某个在册会话上，得到 {viewed:?}"
    );

    let executed = registry.submit_execution(execute_request(&forged, 0)).await;
    assert!(matches!(executed, Err(RuntimeError::UnknownSession(_))));

    let closed = registry
        .close_registered(&forged, registry_fixtures::close_any())
        .await;
    assert!(matches!(closed, Err(RuntimeError::UnknownSession(_))));
}

#[tokio::test(start_paused = true)]
async fn 旧世代句柄立即失效_登记表仍可定位() {
    let backend = ScriptedBackend::defaults().await;
    let registry = registry_with(Arc::clone(&backend), SESSION_LIMIT);
    let view = register_ready(&registry, db_session_id()).await;

    let epoch = registry
        .epoch_of(&db_session_id())
        .expect("在册会话必有世代号");
    assert_eq!(epoch.get(), 1, "登记表侧世代从 1 开始单调递增");

    // 登记表层仍能定位（这一层不读视图，正在执行的会话也不会被堵住）。
    assert!(registry.is_registered(&db_session_id()));

    let stale = stale_handle(&view);
    let viewed = registry.session_view(&stale).await;
    assert!(
        matches!(viewed, Err(RuntimeError::UnknownSession(_))),
        "资源替换后旧世代句柄必须立即失效（INV-07），得到 {viewed:?}"
    );

    // 失效只针对**句柄**，会话本身仍然在册且可读。
    let live = registry
        .session_view(&handle_of(&view))
        .await
        .expect("当前世代句柄仍然可读");
    assert_eq!(live.state, SessionState::Ready);
}

#[tokio::test(start_paused = true)]
async fn 执行完推进上下文世代_旧世代请求拿到明确错() {
    let backend = ScriptedBackend::defaults().await;
    let registry = registry_with(Arc::clone(&backend), SESSION_LIMIT);
    let view = register_ready(&registry, db_session_id()).await;
    let handle = handle_of(&view);

    let receipt = registry
        .submit_execution(execute_request(&handle, view.context_revision.get()))
        .await
        .expect("执行必须成功");
    let epoch = registry
        .epoch_of(&db_session_id())
        .expect("在册会话必有世代号")
        .get();
    assert!(
        receipt
            .execution_id
            .as_str()
            .starts_with(&format!("exec_{epoch}_")),
        "执行 id 由宿主铸造、并把该会话的 runtimeEpoch 嵌进去（§7.6 靠它发起取消），得到 {}",
        receipt.execution_id.as_str()
    );

    let after = registry.session_view(&handle).await.expect("读取视图");
    assert_eq!(
        after.context_revision.get(),
        2,
        "执行后的上下文世代必须推进，否则下一次执行的乐观并发形同虚设"
    );

    let stale = registry.submit_execution(execute_request(&handle, 0)).await;
    assert!(
        matches!(stale, Err(RuntimeError::ContextRevisionMismatch { .. })),
        "带旧世代的执行必须拿到 ContextRevisionMismatch，得到 {stale:?}"
    );
}
