//! ②：阶段内 panic 展开时，取消看守者的存活。
//!
//! `runtime.rs` 的 `run_stage_watched` 对这条路径有**明文承诺**（本轨不改它，只让它可核查）：
//!
//! > handler panic 展开走的是 [`CancelWatch`] 的 `Drop` 兜底，那条路径只 abort 不 await，
//! > 任务真正结束的时机由调度器决定——因此这里承诺的只是「不会一直跑下去」，
//! > 不是「返回时已结束」。
//!
//! 这句承诺此前**没有任何断言覆盖**：全仓库没有一处驱动 panic 的 Job 阶段
//! （`packages/runtime/src/job/` 下也没有 `catch_unwind`），所以"不会一直跑下去"
//! 这半句一直只是注释。
//!
//! ## 为什么计数归零**证明不了**这件事
//!
//! [`CancelWatch`] 的 `Drop` 里两件事是分开的：
//!
//! ```text
//! handle.take() → handle.abort()   // 终止任务
//! self.live.fetch_sub(1, SeqCst)   // 归还计数（**同步**，与任务是否真的结束无关）
//! ```
//!
//! `fetch_sub` 在 `Drop` 里同步发生，**不 await**。把 `abort()` 那行删掉，看门狗任务会
//! 永远轮询下去，而 `active_cancel_watchers()` **照样返回 0**——计数只描述"句柄被丢弃了"，
//! 不描述"任务结束了"。所以任何只断言计数归零的用例，在"看门狗泄漏"这个缺陷上都是绿的。
//!
//! 真正能区分的是**读次数**：泄漏的看门照会永远以 `CANCEL_POLL_INTERVAL` 的节奏增长
//! `cancel_poll` 的读计数，而被 abort 的不会。断言"读次数最终停止增长"，
//! 对 `{间隔 × N 增长}` 与 `{恒定}` 是两个不同的结论。
//!
//! ## 为什么断言的是"最终"而不是"立刻"
//!
//! 承诺的原话是「不会一直跑下去」，不是「返回时已结束」。展开路径只 abort 不 await，
//! 任务真正被调度器丢弃的时刻不确定。所以这里跨过 [`QUIET_WINDOW`] 再采样读次数，
//! 断言的是**有界时间内收敛**，而不是"展开返回的瞬间就已经收敛"。后者会是一条
//! 比承诺更强的断言，红了也只是在测承诺里没有的东西。
//!
//! ## 另一个被顺手记下的事实：Job 没有收敛到任何终态
//!
//! `dispatch` 里没有 `catch_unwind`，panic 发生在 `run_stage_watched` 的 `.await` 处，
//! 于是 L228 之后的收尾（checkpoint 落库、`set_effect_outcome`、终态 CAS）**一行都没跑到**，
//! Job 记录停在 `Running`。本用例只断言"**绝不**被报成 Succeeded"这一条安全性属性，
//! 刻意不去把"`Running` 悬空"钉成期望值——那是一条待裁定的缺陷，不是契约。
//! 详见本轨 `progress.md` 的登记项。

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

    // 展开路径上 `run` 不会返回任何 JobResult：panic 直接穿过 dispatch。
    let joined = run.await;
    assert!(
        joined.is_err(),
        "阶段 panic 必须把派发任务带崩；若这里拿到了 Ok，说明 panic 被某处吞掉了，\
         那是一条与本用例不同的语义，需要单独裁定"
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
    assert_ne!(
        record.view.state,
        JobState::Succeeded,
        "panic 展开的阶段绝不能被报成成功"
    );
}
