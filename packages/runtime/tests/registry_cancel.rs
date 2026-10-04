//! §7.6 取消 / CM-24 取消绑定 —— 登记表公开入口的集成面。
//!
//! 这一支只从 [`SessionRegistry`] 的公开方法进入，不触碰 actor 私有面：
//! 私有面已经由 `src/registry/actor/tests/` 覆盖，两边重复只会让断言互相掩护。
//! 这里要证明的是**从调用方看得到的那部分**：回执的形状、伪造绑定被挡在
//! 后端之前、取消不排队、以及「不支持」是正常返回而不是失败。
//!
//! | 用例 | 分支 | 权威 |
//! | --- | --- | --- |
//! | 正常取消回执形状自洽 | 后端取消成功 | §7.6 回执三字段 |
//! | 伪造 cancelHandle 零后端调用 | 绑定逐字比对 | CM-24 / §3.2 L143 |
//! | 冻结端口自带 `None` 也走同一条校验 | 端口不是旁路 | §7.6 |
//! | 取消先于句柄公布先受理后兑现 | 排队 + `deliver_published` | §7.6 步骤 3 |
//! | driver 无独立控制路径回 `unsupported` | 正常返回 | §7.6 步骤 4 |
//! | 后端取消失败不得塌缩成已取消 | `Err` | §7.6 |
//! | 从未绑定的执行当场拒绝 | `unboundExecution` | CM-24 |
//! | 已终结的执行终态优先 | `alreadyFinished` | §7.6 步骤 5 |
//! | 旧世代句柄连后端都碰不到 | epoch 先判 | §12.1 |

#![allow(dead_code)]
mod registry_fixtures;

use std::sync::Arc;

use datazen_runtime::connection::{ExecutionId, ExecutionState, RuntimeError, SessionView};
use datazen_runtime::registry::{CancelDisposition, SessionPort, SessionRegistry};

use registry_fixtures::{
    as_backend, db_session_id, execute_request, execution_id, handle_of, register_ready, settle,
    stale_handle, wait_until, BackendPlan, Gate, Script, ScriptedBackend, SESSION_LIMIT,
};

/// 夹具公布的 cancelHandle 字面量。调用方声称的绑定必须与它**逐字**一致。
const CANCEL_HANDLE: &str = "ch_exec_7_1";

/// 登记表持有锁，不是 `Clone`，所以并发用例只能把它 `Arc` 起来再 `tokio::spawn`。
fn registry_with(backend: &Arc<ScriptedBackend>) -> Arc<SessionRegistry> {
    Arc::new(SessionRegistry::new(as_backend(backend), SESSION_LIMIT))
}

/// 登记一个会话，并把它的执行推到**句柄已绑定、执行仍未结束**这一步。
///
/// `Script::TwoStage` 是唯一会公布 cancelHandle 的脚本，所以取消这一支的
/// 成功路径只能用它。推进方式是「放一道闸门 → 等 `published_calls` 变成 1」，
/// 而不是 `settle()` 几次：调度让步的次数和后端走到哪一步没有任何关系。
async fn published_execution(
    plan: BackendPlan,
) -> (
    Arc<ScriptedBackend>,
    Gate,
    Arc<SessionRegistry>,
    SessionView,
    ExecutionId,
) {
    let backend = ScriptedBackend::new(plan.with_script(Script::TwoStage)).await;
    let gate = backend.gate();
    let registry = registry_with(&backend);
    let view = register_ready(&registry, db_session_id()).await;
    let handle = handle_of(&view);
    let epoch = registry
        .epoch_of(&db_session_id())
        .expect("在册会话必有世代")
        .get();
    let in_flight = ExecutionId::new(format!("exec_{epoch}_1"));
    let flying = {
        let registry = Arc::clone(&registry);
        async move { registry.submit_execution(execute_request(&handle, 1)).await }
    };
    tokio::spawn(flying);
    assert!(
        wait_until(|| backend.execute_calls() == 1).await,
        "执行必须先真的下发到后端，否则测的不是取消"
    );
    gate.send(()).expect("闸门接收端仍在运行");
    assert!(
        wait_until(|| backend.published_calls() == 1).await,
        "两道闸门之间必须已完成 cancelHandle 公布"
    );
    (backend, gate, registry, view, in_flight)
}

fn release(gate: &Gate, n: usize) {
    for _ in 0..n {
        gate.send(()).expect("闸门接收端在测试结束时仍在运行");
    }
}

/// 取消失败时唯一允许的错误形状：绑定对不上或后端拒绝，都必须是 `CancelFailed`，
/// 且带一个**可判读的原因字面量**。写成别的变体说明调用方无法区分「没绑上」
/// 和「驱动拒绝了」。
fn assert_cancel_failed(error: &RuntimeError, reason: &str) {
    match error {
        RuntimeError::CancelFailed(actual) => assert_eq!(
            *actual, reason,
            "取消失败必须报出可判读的原因，而不是笼统的失败"
        ),
        other => panic!("期望 CancelFailed({reason})，实际得到 {other:?}"),
    }
}

#[tokio::test(start_paused = true)]
async fn 正常取消回执形状自洽且绑定逐字命中() {
    let (backend, gate, registry, view, execution) =
        published_execution(BackendPlan::default()).await;
    let handle = handle_of(&view);

    let receipt = registry
        .cancel_execution_bound(&handle, &execution, CANCEL_HANDLE)
        .await
        .expect("绑定逐字命中时取消必须成功");

    assert_eq!(
        receipt.execution_id, execution,
        "回执必须回填被取消的那一次执行"
    );
    assert_eq!(receipt.disposition, CancelDisposition::Requested);
    assert_eq!(receipt.state, ExecutionState::CancelRequested);
    assert!(receipt.is_coherent(), "三字段回执必须自洽");
    assert!(receipt.is_requested());
    assert_eq!(backend.cancel_calls(), 1, "取消必须真的下发到后端");

    let traces = backend.traces().await;
    assert_eq!(traces.canceled.len(), 1);
    assert_eq!(traces.canceled[0].cancel_handle, CANCEL_HANDLE);

    release(&gate, 1);
    settle().await;
}

#[tokio::test(start_paused = true)]
async fn 伪造取消绑定当场拒绝且零后端调用() {
    let (backend, gate, registry, view, execution) =
        published_execution(BackendPlan::default()).await;
    let handle = handle_of(&view);

    let error = registry
        .cancel_execution_bound(&handle, &execution, "ch_forged")
        .await
        .expect_err("伪造的 cancelHandle 必须被拒");
    assert_cancel_failed(&error, "cancelBindingMismatch");

    assert_eq!(
        backend.cancel_calls(),
        0,
        "伪造者不得在后端留下任何取消痕迹"
    );
    assert!(
        backend.traces().await.canceled.is_empty(),
        "伪造的取消连请求都不该构造出来"
    );

    release(&gate, 1);
    settle().await;
}

#[tokio::test(start_paused = true)]
async fn 冻结端口自带空绑定也走同一条校验() {
    let (backend, gate, registry, view, execution) =
        published_execution(BackendPlan::default()).await;
    let handle = handle_of(&view);
    // 走**冻结的那个 trait 对象**，而不是具体类型的方法解析：
    // 端口形状（返回 `ExecutionState`）本身就是本用例要钉的东西。
    let port: Arc<dyn SessionPort> = registry.clone();

    // 冻结的 `SessionPort::cancel_execution` 没有 `cancelHandle` 字段，
    // 自带 `None` 不是「免检票」：它仍然要用 actor 登记的绑定去核对绑定表。
    let state = port
        .cancel_execution(&handle, &execution)
        .await
        .expect("端口路径同样校验通过");
    assert_eq!(
        state,
        ExecutionState::CancelRequested,
        "冻结端口形状固定为 Result<ExecutionState, RuntimeError>"
    );
    assert_eq!(backend.cancel_calls(), 1, "端口路径同样要真的下发");

    // D-01 的落点：同一个 registry 的具名入口仍然交出 §7.6 的三字段回执。
    // 端口投影不是另算一遍，两条路径对同一次取消必须说同一件事。
    let receipt = registry
        .cancel_registered(&handle, &execution)
        .await
        .expect("门面路径同样校验通过");
    assert_eq!(receipt.disposition, CancelDisposition::Requested);
    assert_eq!(receipt.execution_id, execution);
    assert_eq!(receipt.state, state, "端口投影与门面回执必须描述同一次取消");
    assert_eq!(backend.cancel_calls(), 2, "门面路径同样要真的下发");

    release(&gate, 1);
    settle().await;
}

#[tokio::test(start_paused = true)]
async fn 取消先到先受理_句柄公布后必须兑现() {
    let backend = ScriptedBackend::new(BackendPlan::default().with_script(Script::TwoStage)).await;
    let gate = backend.gate();
    let registry = registry_with(&backend);
    let view = register_ready(&registry, db_session_id()).await;
    let handle = handle_of(&view);
    let epoch = registry
        .epoch_of(&db_session_id())
        .expect("在册会话必有世代")
        .get();
    let execution = ExecutionId::new(format!("exec_{epoch}_1"));

    let task = {
        let registry = Arc::clone(&registry);
        let in_caller = handle.clone();
        tokio::spawn(async move {
            registry
                .submit_execution(execute_request(&in_caller, 1))
                .await
        })
    };
    assert!(
        wait_until(|| backend.execute_calls() == 1).await,
        "执行必须先真的下发到后端"
    );

    // 此刻句柄还没公布：取消不能被打成失败，也不能凭空写成「已取消」。
    let receipt = registry
        .cancel_execution_bound(&handle, &execution, CANCEL_HANDLE)
        .await
        .expect("句柄未公布时取消仍算已受理");
    assert_eq!(receipt.disposition, CancelDisposition::Requested);
    assert_eq!(receipt.state, ExecutionState::CancelRequested);
    assert_eq!(
        backend.cancel_calls(),
        0,
        "还没有可用的绑定，此刻不该有任何后端取消"
    );

    // 句柄一公布，先前那张回执就必须兑现：否则它是一张空头支票。
    gate.send(()).expect("闸门接收端仍在运行");
    assert!(
        wait_until(|| backend.published_calls() == 1).await,
        "两道闸门之间必须已完成 cancelHandle 公布"
    );
    assert!(
        wait_until(|| backend.cancel_calls() == 1).await,
        "先前受理的取消必须在句柄公布后被补送"
    );
    assert_eq!(
        backend.traces().await.canceled[0].cancel_handle,
        CANCEL_HANDLE,
        "补送必须用 actor 登记的绑定，不是调用方声称的那个"
    );

    release(&gate, 1);
    let _ = task.await.expect("执行任务不得 panic");
}

#[tokio::test(start_paused = true)]
async fn 驱动无独立控制路径回不支持且状态不许说谎() {
    let (backend, gate, registry, view, execution) =
        published_execution(BackendPlan::default().driver_without_cancel()).await;
    let handle = handle_of(&view);

    let receipt = registry
        .cancel_execution_bound(&handle, &execution, CANCEL_HANDLE)
        .await
        .expect("「不支持」是正常返回，不是 Err");

    assert_eq!(receipt.execution_id, execution);
    assert_eq!(receipt.disposition, CancelDisposition::Unsupported);
    assert_eq!(
        receipt.state,
        ExecutionState::Running,
        "驱动不支持取消时状态必须保持真实飞行态，写成 CancelRequested 是在骗调用方"
    );
    assert!(receipt.is_coherent());
    assert!(!receipt.is_requested());
    assert_eq!(backend.cancel_calls(), 0, "不支持的路径不该碰后端取消");

    release(&gate, 1);
    settle().await;
}

#[tokio::test(start_paused = true)]
async fn 后端取消失败不得塌缩成已取消() {
    let (backend, gate, registry, view, execution) =
        published_execution(BackendPlan::default().cancel_fails()).await;
    let handle = handle_of(&view);

    let error = registry
        .cancel_execution_bound(&handle, &execution, CANCEL_HANDLE)
        .await
        .expect_err("取消失败必须如实报错");
    assert_cancel_failed(&error, "cancelRequestFailed");
    assert_eq!(
        backend.cancel_calls(),
        1,
        "失败也要真的下发过：否则测的是「没发出去」，不是「驱动拒绝了」"
    );

    release(&gate, 1);
    settle().await;
}

#[tokio::test(start_paused = true)]
async fn 从未绑定的执行当场拒绝() {
    let backend = ScriptedBackend::new(BackendPlan::default()).await;
    let registry = registry_with(&backend);
    let view = register_ready(&registry, db_session_id()).await;
    let handle = handle_of(&view);

    let error = registry
        .cancel_execution_bound(&handle, &execution_id(99), CANCEL_HANDLE)
        .await
        .expect_err("没有绑定关系的执行不得被取消");
    assert_cancel_failed(&error, "unboundExecution");
    assert_eq!(backend.cancel_calls(), 0);
}

#[tokio::test(start_paused = true)]
async fn 已终结的执行终态优先不被改写() {
    let backend = ScriptedBackend::new(BackendPlan::default()).await;
    let registry = registry_with(&backend);
    let view = register_ready(&registry, db_session_id()).await;
    let handle = handle_of(&view);

    let finished = registry
        .submit_execution(execute_request(&handle, 1))
        .await
        .expect("即时脚本下执行立即完成");
    assert_eq!(finished.state, ExecutionState::Succeeded);

    let receipt = registry
        .cancel_execution_bound(&handle, &finished.execution_id, CANCEL_HANDLE)
        .await
        .expect("已终结的执行仍要正常回执，不是 Err");

    assert_eq!(receipt.execution_id, finished.execution_id);
    assert_eq!(
        receipt.disposition,
        CancelDisposition::AlreadyFinished,
        "终态不是被这次取消写出来的"
    );
    assert_eq!(receipt.state, ExecutionState::Succeeded, "终态必须原样回填");
    assert!(receipt.is_coherent());
    assert_eq!(backend.cancel_calls(), 0, "终态执行不该再惊动后端");
}

#[tokio::test(start_paused = true)]
async fn 旧世代句柄的取消连后端都碰不到() {
    let (backend, gate, registry, view, execution) =
        published_execution(BackendPlan::default()).await;
    let stale = stale_handle(&view);

    let error = registry
        .cancel_execution_bound(&stale, &execution, CANCEL_HANDLE)
        .await
        .expect_err("旧世代句柄必须当场失效");
    match error {
        RuntimeError::UnknownSession(_) => {}
        other => panic!("旧世代句柄应得 UnknownSession，实际 {other:?}"),
    }
    assert_eq!(
        backend.cancel_calls(),
        0,
        "世代校验排在取消路径最前，失效句柄不该产生任何后端调用"
    );

    release(&gate, 1);
    settle().await;
}
