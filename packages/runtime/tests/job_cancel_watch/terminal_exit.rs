//! D-B：`watch_cancel_request` 的 `terminal => return` 早退分支。
//!
//! 这条分支的意思是：Job 已经终结，还挂在它上面的轮询**没有任何东西可等**（取消意图
//! 永远不会被翻转），继续读仓储只是白白持有锁、每个 `CANCEL_POLL_INTERVAL` 撞一次全局 `Mutex`。
//!
//! 它此前没有任何断言覆盖：把 `repository.rs` 的 `cancel_poll` 里
//! `terminal: row.record.view.state.is_terminal()` 改成 `false`，全部用例依旧全绿。
//! 这不是正确性缺陷（`CancelWatch` 的 `Drop` 在两条路径上都归还在飞计数，计数仍然归零），
//! 而是一条**清理路径分支没有断言**。回归后的表现是：一个已经结束的 Job，它的阶段还在跑，
//! 看护者会一直轮询到阶段返回被 abort 为止。
//!
//! 上面那句「依旧全绿」有对照实验支撑，不是推断：在本文件落地之前的提交 `e16b43415`
//! 上做同一个 `terminal: false` 变异，独立 detached worktree + 全新
//! `CARGO_TARGET_DIR`，`cargo test -p datazen-runtime --test job_cancel_watch` 得
//! `test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.92s`，
//! EXIT=0；加上本文件后同一变异变成 `1 failed`，红在下面 `get_calls()` 冻结那条 `assert_eq!` 上。

use super::*;

// ---------------------------------------------------------------- D-B：终态即退出

/// Job 在阶段仍在执行时进入终态 → 看守者**自己**停止轮询，而不是被 abort 回收。
///
/// 判据必须是「阶段还阻塞着的时候读次数就冻结了」：阶段一返回，`run_stage_watched`
/// 立刻 abort + await 看守者，读次数冻结是必然的，那条断言什么都没证明。所以观测窗口
/// 全部落在阶段仍被 hold 的这段时间里。
#[tokio::test]
async fn watcher_stops_polling_on_its_own_when_the_job_is_already_terminal() {
    let job_id = "job-cw-9";
    let mut rig = rig(job_id, vec![("s1", Plan::Held)]).await;
    let run = rig.start();

    // 前提：注入终态之前，看护者必须在真的轮询。否则后面「读次数冻结」这条断言
    // 可以靠一个从不轮询的计数器空转通过。
    let entered = match rig.next().await {
        Msg::Entered {
            stage,
            watchers_at_entry,
            get_calls_at_entry,
            ..
        } => {
            assert_eq!(stage, "s1");
            assert_eq!(watchers_at_entry, 1, "阶段执行期间必须有一个看守者在飞");
            get_calls_at_entry
        }
        other => panic!("期望进入 s1，得到 {other:?}"),
    };
    tokio::time::sleep(QUIET_WINDOW).await;
    assert!(
        rig.repo.get_calls() > entered,
        "注入终态前看护者必须真的在轮询（跨过 {QUIET_WINDOW:?} 读次数必须增长），\
         否则下面『停止轮询』测的是一个从不轮询的计数器"
    );
    assert_eq!(
        rig.runtime.active_cancel_watchers(),
        1,
        "在飞计数覆盖的是 dispatch 手里的句柄：阶段没返回，它就该一直是 1"
    );

    // 采样点必须在**强制之前**。若先强制再采样，看护者有可能恰好在两者之间轮询到
    // 终态并退出，于是「强制之后还读到过」这条断言变成与调度时序赛跑的 flake——
    // 它要证明的是「读到终态才停」，不是「强制那一刻正好没被读到」。
    // 放在强制之前采样则无竞态：无论那一次轮询落在强制之前还是之后，
    // 跨过一个 QUIET_WINDOW 至少会有一次 +1（窗口由 `CANCEL_POLL_INTERVAL` 推导，
    // 恒等于 8 个周期，所以"至少一次"与间隔调没调无关）。
    let before_force = rig.repo.get_calls();
    // 把 Job 强制写进终态（测试缝，见 repository.rs 的 force_terminal_for_test），
    // 阶段继续 hold 不动。
    rig.repo
        .force_terminal_for_test(&ctx(), &rig.job_id, JobState::Succeeded)
        .expect("强制 Job 进入终态");
    tokio::time::sleep(QUIET_WINDOW).await;
    assert!(
        rig.repo.get_calls() > before_force,
        "强制终结后看护者应当至少再读到一次终态，才谈得上『读到就退出』"
    );

    // 关键断言：阶段还在跑，读次数却冻结了。这只可能是 `terminal => return` 早退，
    // 因为 abort 回收只发生在阶段返回之后。
    let frozen = rig.repo.get_calls();
    tokio::time::sleep(QUIET_WINDOW).await;
    assert_eq!(
        rig.repo.get_calls(),
        frozen,
        "Job 已经终结时看守者必须自己退出轮询，不得继续读一个已经结束的 Job \
         （阶段仍在执行，这条冻结不可能是 abort 造成的）"
    );
    assert_eq!(
        rig.runtime.active_cancel_watchers(),
        1,
        "退出的是轮询，不是任务：句柄仍由 dispatch 持有，要等阶段返回才归还"
    );

    // 放行阶段：此后的收尾与「没提前退出」时完全一样——早退不改变 Job 的结局。
    rig.release.notify_one();
    match rig.next().await {
        Msg::Finished {
            observed_cancel, ..
        } => assert!(
            !observed_cancel,
            "Job 已经终结不等于取消：令牌不得被翻转，effects 仍按正常完成收敛"
        ),
        other => panic!("期望阶段结束事件，得到 {other:?}"),
    }
    let result = run.await.expect("join").expect("run");
    assert_eq!(
        result.state,
        JobState::Succeeded,
        "提前退出只是不再空转，阶段结果仍照常收敛"
    );
    assert_eq!(result.effect_outcome, EffectOutcome::Completed);
    assert_eq!(
        rig.runtime.active_cancel_watchers(),
        0,
        "阶段返回后 in-flight 计数必须归零"
    );

    let settled = rig.repo.get_calls();
    tokio::time::sleep(QUIET_WINDOW).await;
    assert_eq!(
        rig.repo.get_calls(),
        settled,
        "dispatch 返回后不得还有任务继续读仓储"
    );
}
