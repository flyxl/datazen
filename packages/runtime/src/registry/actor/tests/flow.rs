//! actor 的执行流用例：会话内串行 / 会话间并行、切库竞态、登记失败、旧 epoch。
//!
//! 这里刻意**不走** `SessionActor::exec/control` 的便捷封装：那两个封装会
//! `.await` 回执，而在「飞行中」的场景里回执本来就不该到。用例改为直接往
//! 私有的 `exec_tx` / `ctrl_tx` 里塞命令，握住 `oneshot::Receiver`，
//! 用 `try_recv()` 断言「还没被回答」——这才是排队语义的正面证据。
//!
//! 私有通道可达的原因：actor 是本用例模块的**祖先**模块，Rust 的私有性是
//! 「对后代可见」，因此 `actor::tests::flow` 能拿到 `SessionActor` 的私有字段。

use super::super::{AuditKind, Counter, ExecCommand, RuntimeError, SessionHandle, SessionState};
use super::*;

/// 同一会话的第二条执行**必须**排到第一条结束之后。
///
/// 证据有三段：后端只被调用了一次、第二条的回执还没到（`try_recv` 是
/// `Empty` 而不是 `Closed`）、闸门放开后两条按 host 铸造的递增 id 先后落地。
#[tokio::test(start_paused = true)]
async fn 会话内严格串行第二条执行排到第一条之后() {
    let backend = FakeBackend::new(FakeOutcome::default().with_script(Script::Silent)).await;
    let (actor, view, _audit) = spawn_ready(backend.clone()).await;
    let handle = handle_of(&view);

    // 第一条：直接塞进私有队列，自己握住回执。
    let (tx1, mut rx1) = tokio::sync::oneshot::channel();
    actor
        .exec_tx
        .send(ExecCommand::Execute {
            request: execute_request(&handle, 1),
            reply: tx1,
        })
        .expect("执行队列必须还开着");
    wait_until(|| backend.execute_calls() == 1).await;

    // 第二条：飞行中塞进去，必须被推迟。
    let (tx2, mut rx2) = tokio::sync::oneshot::channel();
    actor
        .exec_tx
        .send(ExecCommand::Execute {
            request: execute_request(&handle, 1),
            reply: tx2,
        })
        .expect("执行队列必须还开着");
    for _ in 0..8 {
        settle().await;
    }

    assert_eq!(backend.execute_calls(), 1, "飞行中不得并发发起第二条执行");
    assert_eq!(
        rx1.try_recv(),
        Err(tokio::sync::oneshot::error::TryRecvError::Empty),
        "第一条还在闸门里，回执不该提前到"
    );
    assert_eq!(
        rx2.try_recv(),
        Err(tokio::sync::oneshot::error::TryRecvError::Empty),
        "第二条被推迟，回执不该提前到（也不该是 Closed）"
    );

    // 飞行中只读投影仍可插空回答——它是观测面，不是执行面。
    let mid = actor
        .exec(|reply| ExecCommand::View {
            handle: handle.clone(),
            reply,
        })
        .await
        .expect("飞行中 View 必须立刻答");
    assert_eq!(mid.state, SessionState::Executing);
    assert_eq!(backend.execute_calls(), 1);

    // 放开闸门：两条依次落地，id 单调递增即严格有序。
    for _ in 0..4 {
        backend.gate().send(()).expect("闸门接收端必须还在");
    }
    let first = rx1.await.expect("回执通道必须送达").expect("执行必须成功");
    let second = rx2.await.expect("回执通道必须送达").expect("执行必须成功");
    assert_eq!(first.execution_id.as_str(), format!("{EXEC_PREFIX}1"));
    assert_eq!(second.execution_id.as_str(), format!("{EXEC_PREFIX}2"));
}

/// **不同会话**必须真的并发，不能被一把全局锁串起来。
///
/// 这里不是「两个 actor 各自跑」，而是同一个后端上量出峰值并发数：
/// 如果实现退化成一把握住所有会话的大锁，`peak_live()` 只会是 1。
#[tokio::test(start_paused = true)]
async fn 不同会话之间真的并发() {
    let backend = FakeBackend::new(FakeOutcome::default().with_script(Script::Silent)).await;
    let (left, left_view, _a) = spawn_ready_for(backend.clone(), db_session_id()).await;
    let (right, right_view, _b) = spawn_ready_for(backend.clone(), other_db_session_id()).await;

    for (actor, view) in [(&left, &left_view), (&right, &right_view)] {
        actor
            .exec_tx
            .send(ExecCommand::Execute {
                request: execute_request(&handle_of(view), 1),
                reply: tokio::sync::oneshot::channel().0,
            })
            .expect("执行队列必须还开着");
    }

    // 两条都停在各自的闸门上：先证明它们**同时**活着。
    wait_until(|| backend.peak_live() == 2).await;
    assert_eq!(backend.execute_calls(), 2);
    assert_eq!(
        backend.peak_live(),
        2,
        "两个会话的执行必须真正重叠，峰值并发数才是 2"
    );

    for _ in 0..4 {
        backend.gate().send(()).expect("闸门接收端必须还在");
    }
    for _ in 0..8 {
        settle().await;
    }
}

/// 切库竞态：请求带着**旧修订**打过来，必须被拒，
/// 而不是在新目标上执行——配置不得用来找替代会话。
#[tokio::test(start_paused = true)]
async fn 带着旧上下文修订的执行被拒而不是跑在新目标上() {
    let backend = FakeBackend::new(FakeOutcome::default()).await;
    let (actor, view, _audit) = spawn_ready(backend.clone()).await;
    assert_eq!(view.context_revision, Counter(1));

    let stale = actor
        .exec(|reply| ExecCommand::Execute {
            request: execute_request(&handle_of(&view), 0),
            reply,
        })
        .await;

    assert_eq!(
        stale,
        Err(RuntimeError::ContextRevisionMismatch {
            expected: 0,
            actual: 1
        })
    );
    assert_eq!(backend.execute_calls(), 0, "旧修订的执行不许到达后端");
}

/// 一个 ID 只登记一个归属者。带着**旧 runtimeEpoch** 的句柄
/// 是上一代会话的遗物，它既不能投影也不能执行。
#[tokio::test(start_paused = true)]
async fn 旧运行纪元的句柄既不能投影也不能执行() {
    let backend = FakeBackend::new(FakeOutcome::default()).await;
    let (actor, view, _audit) = spawn_ready(backend.clone()).await;

    let view_only = actor
        .exec(|reply| ExecCommand::View {
            handle: stale_handle(&view),
            reply,
        })
        .await;
    assert_eq!(
        view_only,
        Err(RuntimeError::UnknownSession(db_session_id().to_string()))
    );

    let execute = actor
        .exec(|reply| ExecCommand::Execute {
            request: execute_request(&stale_handle(&view), 1),
            reply,
        })
        .await;
    assert_eq!(
        execute,
        Err(RuntimeError::UnknownSession(db_session_id().to_string()))
    );

    let register = actor
        .exec(|reply| ExecCommand::RegisterHandles {
            handle: stale_handle(&view),
            handles: vec![handle_ref("h_1", "res_7")],
            reply,
        })
        .await;
    assert_eq!(
        register,
        Err(RuntimeError::UnknownSession(db_session_id().to_string()))
    );

    assert_eq!(backend.execute_calls(), 0);
    assert_eq!(backend.finalize_calls(), 0);
    assert_eq!(backend.close_calls(), 0);
}

/// 登记失败**不得**返回成功，也不得留下半开的记录。
///
/// 三个断言缺一不可：回执是错误、投影不再是 `Ready`、审计里有
/// `sessionRegistrationFailed` 且结论是 `rejected`。
#[tokio::test(start_paused = true)]
async fn 打开失败不返回成功也不留下半开记录() {
    let backend = FakeBackend::new(FakeOutcome::default().open_fails()).await;
    let (tx, mut audit) = mpsc::unbounded_channel();
    let db = db_session_id();
    let actor = spawn_actor(
        open_request(db.clone()),
        RuntimeEpoch::new(EPOCH),
        backend.clone(),
        tx,
    );

    let opened = actor
        .exec(|reply| ExecCommand::Open {
            request: open_request(db.clone()),
            reply,
        })
        .await;
    assert_eq!(
        opened,
        Err(RuntimeError::UnknownSession(db.clone().to_string()))
    );
    assert_eq!(backend.open_calls(), 1);

    let projection = actor
        .exec(|reply| ExecCommand::View {
            handle: SessionHandle {
                db_session_id: db.clone(),
                runtime_epoch: Counter(EPOCH),
            },
            reply,
        })
        .await;
    assert_eq!(projection, Err(RuntimeError::SessionClosed(db.to_string())));

    let execute = actor
        .exec(|reply| ExecCommand::Execute {
            request: execute_request(
                &SessionHandle {
                    db_session_id: db.clone(),
                    runtime_epoch: Counter(EPOCH),
                },
                1,
            ),
            reply,
        })
        .await;
    assert_eq!(execute, Err(RuntimeError::SessionClosed(db.to_string())));
    assert_eq!(backend.execute_calls(), 0);

    let entries = drain_audit(&mut audit);
    assert_eq!(
        entries
            .iter()
            .filter(|entry| entry.kind == AuditKind::SessionRegistered)
            .count(),
        0,
        "打开失败不得同时留下成功登记的审计"
    );
    let failed =
        single_of_kind(&entries, AuditKind::SessionRegistrationFailed).expect("必须有失败审计");
    assert_eq!(failed.outcome, Outcome::Rejected);
    assert_eq!(failed.error_code, Some("hostRejected"));
}

/// actor 终止后不得重建任何句柄——句柄随物理资源一起没了。
///
/// 这里的做法是**丢掉最后一个** `SessionActor` 克隆，两个通道随之关闭，
/// actor 必须走 `finish()` 把物理资源收掉，而不是让它泄漏。
///
/// 后端剧本给的是 `.finalizes(1, 0)`：如实确认那一个登记句柄已终结。**这是
/// 本例后半段断言成立的前提**——`closed_with[0].registered_handles == 0` 现在
/// 读的是「确认终结之后宿主账上剩多少」，只有真的注销掉了才是 0。曾经它恒为 0
/// 是因为释放例程在发终结请求**之前**就把账 drain 空了（取样即注销），那条断言
/// 因此在「driver 少报、句柄仍活着的」场景下也照样绿——那正是
/// 「driver 报 Clean 而宿主仍有已登记句柄时宿主检查必须失败」从不被检验的原因。
#[tokio::test(start_paused = true)]
async fn actor_终止后收掉物理资源不再重建句柄() {
    let backend = FakeBackend::new(
        FakeOutcome::default()
            .with_script(Script::Immediate)
            .handles(vec![handle_ref("h_1", "res_7")])
            .finalizes(1, 0),
    )
    .await;
    let (actor, view, _audit) = spawn_ready(backend.clone()).await;

    // 先跑一次执行，让宿主登记一个事务句柄。
    actor
        .exec(|reply| ExecCommand::Execute {
            request: execute_request(&handle_of(&view), 1),
            reply,
        })
        .await
        .expect("执行必须成功");

    drop(actor);

    wait_until(|| backend.close_calls() == 1).await;
    let finalized = backend.traces().await.finalized;
    assert_eq!(finalized.len(), 1);
    assert_eq!(
        finalized[0].disposition,
        crate::registry::backend::HandleDisposition::Rollback,
        "actor 收尾也必须先回滚再关资源"
    );
    assert_eq!(
        finalized[0].handles.len(),
        1,
        "收尾时宿主登记的句柄必须被带上，不能随 actor 一起蒸发"
    );
    assert_eq!(backend.traces().await.closed_with[0].registered_handles, 0);
}
