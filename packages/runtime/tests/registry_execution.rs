//! registry 集成测试：actor 邮箱（串行执行队列 + 只读投影插空）、执行登记、执行终态审计。
//!
//! # 用例 → 分支
//!
//! | 用例 | 分支 |
//! | --- | --- |
//! | `同一会话的执行串行_第二条排队不下发` | `handle_exec_while_flying` → `deferred` |
//! | `跨会话执行并行_峰值并发为二` | 两个独立 actor 各自 `start_execution`；串行是**会话内**性质 |
//! | `执行飞行中只读投影插空回答` | `handle_exec_while_flying` 的 `View` 分支（控制通道旁路） |
//! | `句柄先登记终态后交付` | `apply_completion` 中 register 先于 reply |
//! | `执行失败不伪装成功且会话回到就绪` | `Ok(Err(cause))` → `Outcome::Undecided` |
//! | `排队中的执行不得绕过世代校验` | `drain_deferred` → `admit_execution`（切库竞态） |
//!
//! # 为什么必须在这一层测
//!
//! 串行性不是「每个 actor 自己加锁」，而是**队列位置**的性质：
//! 一条执行命令在飞行期间到达，不得被立即处理，也不得被拒绝，
//! 只能排在后面按 FIFO 放出来。crate 内单测能看见这条，
//! 但看不见「物理层同时在跑几条」的峰值——那只能从后端替身数出来。
//!
//! 并发窗口一律用**显式闸门**制造，不用时间推进：暂停时钟下
//! `tokio::time::timeout` 会被运行时自动推进骗过去，
//! 所以就绪判据只能用 `yield_now` 轮询或 `oneshot::try_recv`。

#![allow(dead_code)]

mod registry_fixtures;

use std::sync::Arc;

use datazen_runtime::connection::{ExecutionState, RuntimeError, SessionState};
use datazen_runtime::registry::{AuditKind, Outcome, SessionPort, SessionRegistry};

use registry_fixtures::{
    as_backend, ask_until, db_session_id, execute_request, handle_of, handle_ref,
    other_db_session_id, register_ready, settle, wait_until, BackendPlan, Gate, Script,
    ScriptedBackend, SESSION_LIMIT,
};

/// 停在闸门上的后端替身：本组用例的并发窗口**只**由显式闸门制造。
///
/// 世代固定为 1：这一组只测排队，不测世代推进（那是 `registry_lifecycle` 的事）。
/// 让后端推进世代的话，队尾那条会先撞上 `ContextRevisionMismatch`，
/// 串行性的断言就会被世代校验的噪声盖掉。
async fn gated_backend() -> (Arc<ScriptedBackend>, Gate) {
    let backend = ScriptedBackend::new(
        BackendPlan::default()
            .with_script(Script::Silent)
            .context_revision(1),
    )
    .await;
    let gate = backend.gate();
    (backend, gate)
}

/// 放行 `n` 条执行的闸门。闸门是无界的，发送端不会挂。
fn release(gate: &Gate, n: usize) {
    for _ in 0..n {
        gate.send(()).expect("闸门接收端在测试结束时仍在运行");
    }
}

#[tokio::test(start_paused = true)]
async fn 同一会话的执行串行_第二条排队不下发() {
    let (backend, gate) = gated_backend().await;
    let registry = Arc::new(SessionRegistry::new(as_backend(&backend), SESSION_LIMIT));
    let view = register_ready(&registry, db_session_id()).await;
    let handle = handle_of(&view);

    let first = {
        let registry = Arc::clone(&registry);
        let handle = handle.clone();
        tokio::spawn(async move { registry.submit_execution(execute_request(&handle, 1)).await })
    };
    assert!(
        wait_until(|| backend.execute_calls() == 1).await,
        "第一条执行必须已经下发到后端并停在闸门上"
    );

    // 第二条在飞行期间到达：必须排队，不能被立即下发，也不能被拒。
    let second = {
        let registry = Arc::clone(&registry);
        let handle = handle.clone();
        tokio::spawn(async move { registry.submit_execution(execute_request(&handle, 1)).await })
    };
    settle().await;
    settle().await;
    assert_eq!(
        backend.execute_calls(),
        1,
        "飞行中的会话不得并发下发第二条执行"
    );

    release(&gate, 2);
    let first = first
        .await
        .expect("任务必须结束")
        .expect("第一条执行必须成功");
    assert!(
        wait_until(|| backend.execute_calls() == 2).await,
        "第一条落定之后，队尾的第二条才允许被放行"
    );
    release(&gate, 2);
    let second = second
        .await
        .expect("任务必须结束")
        .expect("第二条执行必须成功");

    assert_ne!(
        first.execution_id, second.execution_id,
        "同会话的两次执行必须是两个不同的 executionId"
    );
    assert_eq!(
        backend.peak_live(),
        1,
        "同一会话在任何时刻只允许一条执行在飞"
    );
}

#[tokio::test(start_paused = true)]
async fn 跨会话执行并行_峰值并发为二() {
    let (backend, gate) = gated_backend().await;
    let registry = Arc::new(SessionRegistry::new(as_backend(&backend), SESSION_LIMIT));

    let first = register_ready(&registry, db_session_id()).await;
    let second = register_ready(&registry, other_db_session_id()).await;

    let one = {
        let registry = Arc::clone(&registry);
        let handle = handle_of(&first);
        async move { registry.submit_execution(execute_request(&handle, 1)).await }
    };
    let two = {
        let registry = Arc::clone(&registry);
        let handle = handle_of(&second);
        async move { registry.submit_execution(execute_request(&handle, 1)).await }
    };
    // 放闸门这一支必须和两个执行活在同一个 `join!` 里：否则它们会各自停在闸门上
    // 永不返回（暂停时钟下的两方会合死锁不会自行解除）。
    let releaser = async {
        assert!(
            wait_until(|| backend.execute_calls() == 2).await,
            "两个会话的执行必须同时在飞，否则本用例测不到并行"
        );
        release(&gate, 4);
    };

    let (left, right, ()) = tokio::join!(one, two, releaser);
    assert_eq!(
        backend.peak_live(),
        2,
        "串行是会话内性质，不得被提升成全局串行"
    );

    let left = left.expect("第一个会话的执行必须成功");
    let right = right.expect("第二个会话的执行必须成功");
    assert_ne!(
        left.execution_id, right.execution_id,
        "两个会话的执行 id 必须互相独立"
    );
    assert_eq!(
        left.execution_id.as_str(),
        format!(
            "exec_{}_1",
            registry
                .epoch_of(&db_session_id())
                .expect("在册会话必有世代")
                .get()
        ),
        "执行 id 必须带本会话自己的世代，跨会话不得串号"
    );
}

#[tokio::test(start_paused = true)]
async fn 执行飞行中只读投影插空回答() {
    let (backend, gate) = gated_backend().await;
    let registry = Arc::new(SessionRegistry::new(as_backend(&backend), SESSION_LIMIT));
    let view = register_ready(&registry, db_session_id()).await;
    let handle = handle_of(&view);

    let flying = {
        let registry = Arc::clone(&registry);
        let handle = handle.clone();
        tokio::spawn(async move { registry.submit_execution(execute_request(&handle, 1)).await })
    };
    assert!(
        wait_until(|| backend.execute_calls() == 1).await,
        "执行必须已经下发到后端并停在闸门上"
    );

    // 只读投影必须插空回答。若 `View` 也排队，这个 `.await` 会永远挂着，
    // 所以有界地轮询 `try_recv`：失败时得到的是一条断言，而不是一个挂死的进程。
    let (view_tx, mut view_rx) = tokio::sync::oneshot::channel();
    {
        let registry = Arc::clone(&registry);
        let handle = handle.clone();
        tokio::spawn(async move {
            let _ = view_tx.send(registry.session_view(&handle).await);
        });
    }
    let inflight = ask_until(&mut view_rx).await;
    assert!(
        inflight.is_some(),
        "执行飞行中只读投影必须插空回答，否则队列会把控制面一起堵死"
    );
    let inflight = inflight.expect("已就绪").expect("在册会话的视图必须可读");
    assert_eq!(inflight.state, SessionState::Executing);
    assert!(
        inflight.active_execution_id.is_some(),
        "飞行中的投影必须说得出当前执行 id，否则调用方无从发起取消"
    );

    release(&gate, 2);
    flying.await.expect("任务必须结束").expect("执行必须成功");
}

#[tokio::test(start_paused = true)]
async fn 句柄先登记终态后交付() {
    let backend = ScriptedBackend::new(
        BackendPlan::default()
            .handles(vec![handle_ref("h_1", "res_1"), handle_ref("h_2", "res_1")]),
    )
    .await;
    let registry = SessionRegistry::new(backend, SESSION_LIMIT);
    let view = register_ready(&registry, db_session_id()).await;
    let handle = handle_of(&view);

    let receipt = registry
        .submit_execution(execute_request(&handle, view.context_revision.get()))
        .await
        .expect("执行必须成功");
    assert_eq!(receipt.state, ExecutionState::Succeeded);

    // 归池判定依据就写在这条终态条目上：句柄数必须在交付前就登记完。
    let completed: Vec<_> = registry
        .audit_log()
        .into_iter()
        .filter(|entry| entry.kind == AuditKind::ExecutionCompleted)
        .collect();
    assert_eq!(completed.len(), 1);
    assert_eq!(completed[0].execution_state, Some("succeeded"));
    assert_eq!(completed[0].effect_outcome, Some("completed"));
    assert_eq!(completed[0].outcome, Outcome::Succeeded);
    assert_eq!(
        completed[0].handle_count, 2,
        "终态条目必须带上该时刻 actor 内登记的句柄数，否则归池判定没有依据"
    );
}

#[tokio::test(start_paused = true)]
async fn 执行失败不伪装成功且会话回到就绪() {
    let backend = ScriptedBackend::new(BackendPlan::default().execute_fails()).await;
    let registry = SessionRegistry::new(as_backend(&backend), SESSION_LIMIT);
    let view = register_ready(&registry, db_session_id()).await;
    let handle = handle_of(&view);

    let result = registry
        .submit_execution(execute_request(&handle, view.context_revision.get()))
        .await;
    assert!(
        result.is_err(),
        "后端拒绝的执行绝不能返回成功回执，得到 {result:?}"
    );
    // 精确变体不在这里钉死：`provider_to_runtime` 的兜底臂还在协调者裁定中
    // （裁定 #4 针对的是 `fold_exit`，不是这个函数）。此处契约是
    // 「失败原样冒泡、效果不可判定、会话不被判死刑」。

    let completed: Vec<_> = registry
        .audit_log()
        .into_iter()
        .filter(|entry| entry.kind == AuditKind::ExecutionCompleted)
        .collect();
    assert_eq!(completed.len(), 1);
    assert_eq!(
        completed[0].outcome,
        Outcome::Undecided,
        "后端拒绝意味着效果不可判定，不得记成成功"
    );
    assert_eq!(completed[0].execution_state, Some("failed"));
    assert_eq!(completed[0].effect_outcome, Some("unknown"));

    let after = registry
        .session_view(&handle)
        .await
        .expect("失败的执行之后视图仍必须可读");
    assert_eq!(
        after.state,
        SessionState::Ready,
        "单次执行失败不得把会话打成不可用：调用方还要决定重试还是放弃"
    );
}

#[tokio::test(start_paused = true)]
async fn 排队中的执行不得绕过世代校验() {
    // 这里**必须**让后端把世代推进到 2（`BackendPlan` 的默认值）：排队的判据
    // 正是「轮到它时世代已经变了」。若把后端回报的世代钉成 1，
    // 前后都是 1，这条用例会永远绿——测到的是夹具的巧合，不是代码的性质。
    let backend = ScriptedBackend::new(BackendPlan::default().with_script(Script::Silent)).await;
    let gate = backend.gate();
    let registry = Arc::new(SessionRegistry::new(as_backend(&backend), SESSION_LIMIT));
    let view = register_ready(&registry, db_session_id()).await;
    let handle = handle_of(&view);

    let first = {
        let registry = Arc::clone(&registry);
        let handle = handle.clone();
        tokio::spawn(async move { registry.submit_execution(execute_request(&handle, 1)).await })
    };
    assert!(wait_until(|| backend.execute_calls() == 1).await);

    // 第二条带着**飞行前**读到的世代（1）入队。后端这一轮会把世代推进到 2，
    // 于是轮到它时必须被世代校验拦下——排队不等于豁免切库竞态。
    let second = {
        let registry = Arc::clone(&registry);
        let handle = handle.clone();
        tokio::spawn(async move { registry.submit_execution(execute_request(&handle, 1)).await })
    };
    settle().await;
    assert_eq!(
        backend.execute_calls(),
        1,
        "飞行中的会话不得并发下发第二条执行"
    );

    release(&gate, 2);
    first.await.expect("任务必须结束").expect("第一条必须成功");

    let second = second.await.expect("任务必须结束");
    assert!(
        matches!(second, Err(RuntimeError::ContextRevisionMismatch { .. })),
        "排队不等于豁免世代校验：切库竞态必须仍然拦得住，得到 {second:?}"
    );
    assert_eq!(backend.execute_calls(), 1, "被拒的第二条不得真的下发");
}
