//! actor 的取消用例：§7.6 / CM-22 / CM-23 / CM-24 与 §4.5 CM-72 审计纪律。
//!
//! 取消走的是**控制旁路**，所以这些用例里取消命令一律经 `ctrl_tx` 直接投递，
//! 不经过 `exec_tx` 的执行队列——否则「取消不被执行排队挡住」这件事本身就是
//! 被自己测出来的，测不出旁路是否真的存在。

use super::super::{AuditKind, CancelDisposition, RuntimeError};
use super::*;
use crate::connection::ExecutionState;
use crate::registry::CancelReceipt;

/// T4 / CM-22：取消请求打在**句柄尚未发布**的窗口里。
///
/// 这是最容易做假的一格：此时根本没有 `cancelHandle` 可递给驱动，一个诚实的
/// 实现只能先把意图记在 actor 上，等句柄真发布出来再补发——而**不能**谎称已经取消。
/// 证据是两侧的数字：此刻后端 cancel 调用数是 0；闸门放行、句柄发布之后是 1。
#[tokio::test(start_paused = true)]
async fn 句柄未发布时的取消先记意图发布后补发一次() {
    let backend = FakeBackend::new(FakeOutcome::default().with_script(Script::TwoStage)).await;
    let (actor, view, mut audit) = spawn_ready(backend.clone()).await;
    let handle = handle_of(&view);

    // 塞进执行命令并停住：还没过第一个闸门，句柄没发布。
    let (tx, _reply) = tokio::sync::oneshot::channel();
    actor
        .exec_tx
        .send(ExecCommand::Execute {
            request: execute_request(&handle, 1),
            reply: tx,
        })
        .expect("执行队列必须还开着");
    wait_until(|| backend.execute_calls() == 1).await;

    let receipt = actor
        .control(|reply| cancel_command(handle.clone(), execution_id(1), None, reply))
        .await
        .expect("发布前的取消必须正常返回，不能是错误");
    assert_eq!(
        receipt,
        CancelReceipt {
            execution_id: execution_id(1),
            disposition: CancelDisposition::Requested,
            state: ExecutionState::CancelRequested,
        }
    );
    assert_eq!(
        backend.cancel_calls(),
        0,
        "句柄还没发布，此刻没有任何东西可以递给驱动"
    );

    // 放行：执行走到发布点，actor 在同一轮里把攒下的意图补发出去。
    backend.gate().send(()).expect("闸门接收端必须还在");
    wait_until(|| backend.cancel_calls() == 1).await;
    let canceled = backend.traces().await.canceled;
    assert_eq!(canceled.len(), 1);
    assert_eq!(
        canceled[0].cancel_handle, "ch_exec_7_1",
        "补发必须用驱动自己发布的那个句柄，不是宿主编的"
    );

    let entries = drain_audit(&mut audit);
    let resolved = single_of_kind(&entries, AuditKind::CancelResolved).expect("必须有取消审计");
    assert_eq!(resolved.outcome, Outcome::Succeeded);
    assert_eq!(resolved.disposition, Some("requested"));
}

/// T5 / CM-22：句柄已发布时取消必须**立刻**下发，而且用的是登记绑定。
#[tokio::test(start_paused = true)]
async fn 已发布句柄的取消立刻下发且用的是登记绑定() {
    let backend = FakeBackend::new(FakeOutcome::default().with_script(Script::TwoStage)).await;
    let (actor, view, _audit) = spawn_ready(backend.clone()).await;
    let handle = handle_of(&view);

    let (tx, _reply) = tokio::sync::oneshot::channel();
    actor
        .exec_tx
        .send(ExecCommand::Execute {
            request: execute_request(&handle, 1),
            reply: tx,
        })
        .expect("执行队列必须还开着");
    wait_until(|| backend.execute_calls() == 1).await;
    // 过第一道闸门：驱动此刻发布 cancelHandle。
    backend.gate().send(()).expect("闸门接收端必须还在");
    for _ in 0..16 {
        settle().await;
    }

    let receipt = actor
        .control(|reply| cancel_command(handle.clone(), execution_id(1), None, reply))
        .await
        .expect("取消必须正常返回");
    assert_eq!(receipt.disposition, CancelDisposition::Requested);
    assert_eq!(receipt.state, ExecutionState::CancelRequested);
    assert_eq!(backend.cancel_calls(), 1);
    let canceled = backend.traces().await.canceled;
    assert_eq!(canceled[0].cancel_handle, "ch_exec_7_1");
    assert_eq!(canceled[0].resource_id, "res_7");
}

/// T6 / CM-24：驱动不支持取消时，宿主返回的是**正常结果**而不是错误，
/// 而且执行状态**保持 Running**。
///
/// 两条都是反直觉的，所以单独一格：把 `Unsupported` 塞进 `Err` 会让上层把
/// 「驱动不支持」误当成「取消失败」；把状态改写成 `CancelRequested` 则是凭空
/// 捏造了一个没发生过的状态转移。
#[tokio::test(start_paused = true)]
async fn 驱动不支持取消时正常返回不支持且状态不动() {
    let backend = FakeBackend::new(
        FakeOutcome::default()
            .with_script(Script::Silent)
            .driver_without_cancel(),
    )
    .await;
    let (actor, view, mut audit) = spawn_ready(backend.clone()).await;
    let handle = handle_of(&view);

    let (tx, _reply) = tokio::sync::oneshot::channel();
    actor
        .exec_tx
        .send(ExecCommand::Execute {
            request: execute_request(&handle, 1),
            reply: tx,
        })
        .expect("执行队列必须还开着");
    wait_until(|| backend.execute_calls() == 1).await;

    let receipt = actor
        .control(|reply| cancel_command(handle.clone(), execution_id(1), None, reply))
        .await
        .expect("不支持取消是正常结果，不是错误");
    assert_eq!(
        receipt,
        CancelReceipt {
            execution_id: execution_id(1),
            disposition: CancelDisposition::Unsupported,
            state: ExecutionState::Running,
        },
        "不支持取消时执行仍在跑，状态不能被改写成 CancelRequested"
    );
    assert_eq!(backend.cancel_calls(), 0);

    let entries = drain_audit(&mut audit);
    let resolved = single_of_kind(&entries, AuditKind::CancelResolved).expect("必须有取消审计");
    assert_eq!(resolved.disposition, Some("unsupported"));
    assert_eq!(resolved.execution_state, Some("running"));
}

/// T7 / CM-24：伪造绑定必须被拒，而且**不得**顺手把新执行取消了。
///
/// 上一次执行留下的 `cancelHandle` 拿着去取消新的执行，是这条 CM 的原型事故。
/// 断言两件事：回执是 `Err`，以及驱动的 cancel 调用数仍是 0。
#[tokio::test(start_paused = true)]
async fn 拿旧句柄取消新执行被拒且没有下发到驱动() {
    let backend = FakeBackend::new(FakeOutcome::default().with_script(Script::TwoStage)).await;
    let (actor, view, _audit) = spawn_ready(backend.clone()).await;
    let handle = handle_of(&view);

    let (tx, _reply) = tokio::sync::oneshot::channel();
    actor
        .exec_tx
        .send(ExecCommand::Execute {
            request: execute_request(&handle, 1),
            reply: tx,
        })
        .expect("执行队列必须还开着");
    wait_until(|| backend.execute_calls() == 1).await;
    backend.gate().send(()).expect("闸门接收端必须还在");
    for _ in 0..16 {
        settle().await;
    }

    let forged = actor
        .control(|reply| {
            cancel_command(
                handle.clone(),
                execution_id(1),
                Some("ch_from_a_previous_run".to_owned()),
                reply,
            )
        })
        .await;
    assert_eq!(
        forged,
        Err(RuntimeError::CancelFailed("cancelBindingMismatch"))
    );
    assert_eq!(backend.cancel_calls(), 0, "伪造句柄绝不能落到驱动上");

    // 不带调用方句柄的那条路（trait 那条）此时才应该成功。
    let bound = actor
        .control(|reply| cancel_command(handle.clone(), execution_id(1), None, reply))
        .await
        .expect("走登记绑定必须成功");
    assert_eq!(bound.disposition, CancelDisposition::Requested);
    assert_eq!(backend.cancel_calls(), 1);

    // 从没绑定过的执行 id 不许蒙混过关。
    let unknown = actor
        .control(|reply| cancel_command(handle, execution_id(99), None, reply))
        .await;
    assert_eq!(unknown, Err(RuntimeError::CancelFailed("unboundExecution")));
}

/// T8 / CM-23：执行已经终结后再取消，回执是 `alreadyFinished`，
/// 并且**逐字**带回当时的终态，而不是笼统的 `cancelled`。
#[tokio::test(start_paused = true)]
async fn 已终结执行的取消回执是已完结并逐字带回终态() {
    let backend = FakeBackend::new(FakeOutcome::default()).await;
    let (actor, view, mut audit) = spawn_ready(backend.clone()).await;
    let handle = handle_of(&view);

    let receipt = actor
        .exec(|reply| ExecCommand::Execute {
            request: execute_request(&handle, 1),
            reply,
        })
        .await
        .expect("执行必须成功");
    assert_eq!(receipt.state, ExecutionState::Succeeded);

    let late = actor
        .control(|reply| cancel_command(handle, receipt.execution_id.clone(), None, reply))
        .await
        .expect("已终结不等于错误");
    assert_eq!(
        late,
        CancelReceipt {
            execution_id: receipt.execution_id,
            disposition: CancelDisposition::AlreadyFinished,
            state: ExecutionState::Succeeded,
        }
    );
    assert_eq!(
        backend.cancel_calls(),
        0,
        "没有在飞行的东西，不该再去打扰驱动"
    );

    let entries = drain_audit(&mut audit);
    let resolved = single_of_kind(&entries, AuditKind::CancelResolved).expect("必须有取消审计");
    assert_eq!(resolved.disposition, Some("alreadyFinished"));
    assert_eq!(resolved.execution_state, Some("succeeded"));
}

/// T9 / CM-22：取消本身失败**不得**降级成「已取消」。
///
/// 这条最危险——把驱动的报错吞掉、改写成 `requested` 回执，上层就会对着一个
/// 还在跑的语句显示「已取消」。所以既断言 `Err`，也断言审计结论是 `rejected`。
#[tokio::test(start_paused = true)]
async fn 取消失败如实报错且审计结论是否决() {
    let backend = FakeBackend::new(
        FakeOutcome::default()
            .with_script(Script::TwoStage)
            .cancel_fails(),
    )
    .await;
    let (actor, view, mut audit) = spawn_ready(backend.clone()).await;
    let handle = handle_of(&view);

    let (tx, _reply) = tokio::sync::oneshot::channel();
    actor
        .exec_tx
        .send(ExecCommand::Execute {
            request: execute_request(&handle, 1),
            reply: tx,
        })
        .expect("执行队列必须还开着");
    wait_until(|| backend.execute_calls() == 1).await;
    backend.gate().send(()).expect("闸门接收端必须还在");
    for _ in 0..16 {
        settle().await;
    }

    let failed = actor
        .control(|reply| cancel_command(handle, execution_id(1), None, reply))
        .await;
    assert_eq!(
        failed,
        Err(RuntimeError::CancelFailed("cancelRequestFailed"))
    );
    assert_eq!(backend.cancel_calls(), 1);

    let entries = drain_audit(&mut audit);
    let resolved = single_of_kind(&entries, AuditKind::CancelResolved).expect("必须有取消审计");
    assert_eq!(resolved.outcome, Outcome::Rejected);
    assert_eq!(resolved.error_code, Some("hostRejected"));
    assert_eq!(
        resolved.effect_outcome, None,
        "取消结论不得被请求本身的成功覆盖掉（CM-72）"
    );
}

/// T18 / CM-72：取消审计里只准出现**非敏感的能力版本**，不得出现句柄或凭据。
///
/// 另外把三种 disposition 的线上字面量钉死：它们必须与执行终态那套字面量
/// **不相交**，否则两套词汇混在一起就再也分不清「控制结论」和「执行结论」。
#[tokio::test(start_paused = true)]
async fn 取消审计只带能力版本且处置字面量与执行状态不相交() {
    const DISPOSITIONS: [&str; 3] = ["requested", "unsupported", "alreadyFinished"];
    const EXECUTION_STATES: [&str; 6] = [
        "queued",
        "running",
        "cancelRequested",
        "succeeded",
        "failed",
        "cancelled",
    ];
    for disposition in DISPOSITIONS {
        assert!(
            !EXECUTION_STATES.contains(&disposition),
            "处置 {disposition} 与执行状态字面量撞了，两套词汇会混在一起"
        );
    }

    let backend = FakeBackend::new(FakeOutcome::default().with_script(Script::TwoStage)).await;
    let (actor, view, mut audit) = spawn_ready(backend.clone()).await;
    let handle = handle_of(&view);
    let (tx, _reply) = tokio::sync::oneshot::channel();
    actor
        .exec_tx
        .send(ExecCommand::Execute {
            request: execute_request(&handle, 1),
            reply: tx,
        })
        .expect("执行队列必须还开着");
    wait_until(|| backend.execute_calls() == 1).await;
    backend.gate().send(()).expect("闸门接收端必须还在");
    for _ in 0..16 {
        settle().await;
    }
    actor
        .control(|reply| cancel_command(handle, execution_id(1), None, reply))
        .await
        .expect("取消必须成功");

    let entries = drain_audit(&mut audit);
    let resolved = single_of_kind(&entries, AuditKind::CancelResolved).expect("必须有取消审计");
    let capabilities = resolved
        .capability_versions
        .as_ref()
        .expect("取消审计必须带能力版本，否则事后无从判断是哪个驱动的行为");
    assert!(capabilities.is_version_only());
    assert_eq!(capabilities.contract, "1.0.0");
    assert_eq!(capabilities.driver_api, "2.3.1");
    assert_eq!(resolved.handle_count, 0, "控制请求不改变句柄计数");
}
