//! Stage panic converges to Failed + Unknown and reaps the watcher before return.

use super::*;
use datazen_platform_api::dto::job::JobState;

// ---------------------------------------------------------------- 展开路径的存活

/// 阶段内 panic：看守者不会泄漏，而且 panic 的阶段**绝不能**被报成成功。
#[tokio::test]
async fn panic_inside_a_stage_still_converges_the_cancel_watchdog() {
    let mut rig = rig("job-cw-panic", vec![("s1", Plan::PanicInStage)]).await;
    let run = rig.start();

    // 前提：panic 之前看门狗必须真的在飞，且还没有被取消。
    // 不确认"阶段真的开跑了"，就分不清测的是"展开路径"还是"panic 发生在阶段之前"。
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

    let result = run
        .await
        .expect("dispatch task survives")
        .expect("fault converges");
    assert_eq!(result.state, JobState::Failed);
    assert_eq!(result.effect_outcome, EffectOutcome::Unknown);
    assert_eq!(
        result.error.as_deref(),
        Some("handlerStageFailedOrPanicked")
    );

    // 计数已经归零——但这条断言本身**不能**证明看门狗结束了（见文件头）。
    // 它只保证"句柄没有被泄漏地丢弃"这一层，两条一起断言才构成完整的存活证据。
    assert_eq!(
        rig.runtime.active_cancel_watchers(),
        0,
        "阶段 panic 展开后，看门者的在飞计数必须归还"
    );

    // 真正的判据：读次数**最终不再增长**。
    //
    // 这里依赖 `#[tokio::test]` 的单线程 flavor：panic 与 Drop 都发生在同一个线程上，
    // 顺序完全确定（Drop 先 abort，再 fetch_sub），所以采样窗口里不可能再多落一次轮询。
    // 换成多线程 flavor 时这条会变脆——它测的是"调度器最终丢弃了任务"，不是"立刻丢弃"。
    let before = rig.repo.get_calls();
    tokio::time::sleep(QUIET_WINDOW).await;
    assert_eq!(
        rig.repo.get_calls(),
        before,
        "阶段 panic 之后跨过 {QUIET_WINDOW:?} 读次数仍在增长：\
         `CancelWatch::drop` 的 abort 兜底没有生效，看门者在无限轮询并一直持有 repo / ctx。\
         注意本条断言不能只看 `active_cancel_watchers()`——计数在 Drop 里同步归还，\
         任务泄漏时它照样是 0。"
    );

    // 安全性属性：panic 的阶段绝不能被投影成 Succeeded。
    let record = rig
        .repo
        .get(&ctx(), rig.job_id.clone())
        .await
        .expect("读取 Job 记录");
    assert_eq!(
        record.view.state,
        JobState::Failed,
        "panic 展开的阶段绝不能被报成成功"
    );
}
