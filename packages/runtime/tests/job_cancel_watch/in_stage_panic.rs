//! 阶段 handler panic 时，runtime 将 JoinError 映射为 Failed/Unknown，
//! 并等待取消看守者停止。本测试同时核对 Job 终态和看守者没有继续轮询。

use super::*;
use datazen_platform_api::dto::execution::EffectOutcome;
use datazen_platform_api::dto::job::JobState;

// ---------------------------------------------------------------- 阶段 panic 的收敛

/// 阶段内 panic：看守者不会泄漏，而且 panic 的阶段**绝不能**被报成成功。
#[tokio::test]
async fn panic_inside_a_stage_still_converges_the_cancel_watchdog() {
    let mut rig = rig("job-cw-panic", vec![("s1", Plan::PanicInStage)]).await;
    let run = rig.start();

    // 前提：panic 之前看门狗必须真的在飞，且还没有被取消。
    match rig.next().await {
        Msg::Entered {
            stage,
            cancel_at_entry,
            watchers_at_entry,
            ..
        } => {
            assert_eq!(stage, "s1", "必须落在唯一的阶段 s1 上");
            assert!(!cancel_at_entry, "本用例不注入取消，令牌必须还是干净的");
            assert_eq!(watchers_at_entry, 1, "阶段执行期间必须有一个看守者在飞");
        }
        other => panic!("期望进入 s1，得到 {other:?}"),
    }

    // runtime 将 handler task 的 panic 转成可读的失败结果，不让 Job 悬在 Running。
    let result = run
        .await
        .expect("runtime task must finish")
        .expect("stage panic must be represented as a JobResult");
    assert_eq!(result.state, JobState::Failed);
    assert_eq!(result.effect_outcome, EffectOutcome::Unknown);
    assert_eq!(result.error.as_deref(), Some("handlerPanickedUnknown"));

    // 计数归零且轮询读数停止增长，验证取消看守者已经被回收。
    assert_eq!(
        rig.runtime.active_cancel_watchers(),
        0,
        "阶段 panic 展开后，看门者的在飞计数必须归还"
    );

    // 真正的判据：读次数**最终不再增长**。
    //
    // 阶段返回后 runtime 会 abort 并 await watcher；静默窗口额外验证读操作不会继续增长。
    let before = rig.repo.get_calls();
    tokio::time::sleep(QUIET_WINDOW).await;
    assert_eq!(
        rig.repo.get_calls(),
        before,
        "阶段 panic 之后跨过 {QUIET_WINDOW:?} 读次数仍在增长：取消看守者没有停止轮询。"
    );

    // Job 记录必须与 runtime 返回值一致，panic 阶段绝不能被投影成 Succeeded。
    let record = rig
        .repo
        .get(&ctx(), rig.job_id.clone())
        .await
        .expect("读取 Job 记录");
    assert_eq!(record.view.state, JobState::Failed);
    assert_eq!(record.view.effect_outcome, Some(EffectOutcome::Unknown));
}
