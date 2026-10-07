//! `CANCEL_POLL_INTERVAL` 的可核查性。
//!
//! 这个常量此前**从未被任何断言约束过**，同时又被两侧测试隐式依赖：
//! `job_cancel_watch.rs` 的 `QUIET_WINDOW`（十处"读次数增长 / 不增长"断言全部建立在
//! "窗口至少跨过一个轮询周期"这个前提上）过去写死 400ms，
//! `data-transfer` 侧 `kernel_cancel.rs` 的 `WATCHER_SLACK` 写死 1200ms，
//! 宿主 `src-tauri/src/commands/sync/jobs_cancel_contract.rs` 写死 "10 个周期 ≈ 500ms"。
//! 这些窗口一旦与真源脱钩，它们会**静默退化成空转**——间隔被调大之后，窗口里一次轮询
//! 都落不下，"读次数不再增长"自动成立，一个根本没在跑的看门狗和一个正常工作的看门狗
//! 给出完全相同的结论。
//!
//! 这里补两条互补的检查：
//!
//! 1. `poll_interval_keeps_the_host_cancel_contract_settle_window_honest`
//!    —— **跨模块契约**。上界不是"我觉得 50ms 挺好"，而是宿主取消契约**已经写死的**
//!    前提：放行闸门前要等 10 个周期落定，整体预算 `SETTLE = 500ms`。间隔一旦超过
//!    `500ms / 10`，宿主的等待就不再覆盖 10 个周期，它那句"令牌必然已被置位"的保证
//!    随之失效。这条断言的判据完全来自 `src-tauri` 侧的既有常量，不是把真源的字面量
//!    抄一份换个地方断言。
//! 2. `the_watchdog_actually_polls_at_cancel_poll_interval`
//!    —— **实测量**。把阶段 hold 住，在一段由真源推导的窗口里数 `cancel_poll` 的读次数，
//!    断言它落在名义速率的四分之一到六分之一以外… 即：断言"实测速率"既不塌到 0（轮询停摆）、
//!    也不涨成自旋（间隔退化成 0）。这条是 `CANCEL_POLL_INTERVAL` 第一次真的被**测**出来，
//!    而不是被假定。
//!
//! 两条各有分工，缺一条都有洞：只留第 1 条，间隔在带内被调（例如 50ms → 60ms）无人察觉；
//! 只留第 2 条，间隔被调到 5s 时窗口会自己撑到 200s，实测量永远"通过"。

use super::*;
use datazen_platform_api::dto::job::JobState;

/// 宿主取消契约 `jobs_cancel_contract.rs` 的 `SETTLE`：放行闸门前的落定预算。
///
/// 来源（只读证据，本轨无权修改）：
/// `src-tauri/src/commands/sync/jobs_cancel_contract.rs`
///     const SETTLE: Duration = Duration::from_millis(500);
const HOST_SETTLE: Duration = Duration::from_millis(500);

/// 同一处注释里写死的周期数：「The test waits 10 polls before opening the gate」。
const HOST_SETTLE_POLLS: u32 = 10;

/// 观测窗口的周期数：40 个名义周期（50ms 下即 2s）。
///
/// 40 这个数买的是"负载抖动下仍然站得住"：别的轨在并发编译同一批 crate，
/// 机器很挤，单次 `sleep` 迟到几十毫秒是常事。周期数越多，相对抖动越小。
const MEASURE_PERIODS: u64 = 40;

/// 窗口的绝对上限，只为在**变异运行**里限幅——若有人把间隔调到 3s，
/// 不加上限会让本用例静静跑满 120s。
///
/// 它不影响有效性：名义周期数 `expected` 是拿窗口除以 `CANCEL_POLL_INTERVAL`
/// 独立算出来的，被夹断之后区间两端同步收窄，断言依然成立；真正把越界的间隔
/// 挑出来的是上面那条同步的契约断言，它会先红并直接点名原因。
const MEASURE_WINDOW_CAP: Duration = Duration::from_secs(2);

// ---------------------------------------------------------------- 1：跨模块契约

/// 轮询间隔必须仍然让宿主取消契约的落定窗口覆盖得了它声明的周期数。
///
/// 判据写成 `间隔 × 周期数 ≤ 预算` 而不是 `间隔 ≤ 50ms`：前者把宿主那三个数字
/// （10 个周期、500ms 预算）当成契约的输入，间隔是这轮除法的**结果**。宿主哪天改了
/// `SETTLE`，这条断言会跟着重算，而不是继续守着一个人类拍过的毫秒数。
#[test]
fn poll_interval_keeps_the_host_cancel_contract_settle_window_honest() {
    assert!(
        CANCEL_POLL_INTERVAL * HOST_SETTLE_POLLS <= HOST_SETTLE,
        "轮询间隔 {CANCEL_POLL_INTERVAL:?} 乘以宿主取消契约声明的 {HOST_SETTLE_POLLS} 个周期 \
         已经超出它的 {HOST_SETTLE:?} 落定预算（每周期上限 {:?}）：\
         宿主那条测试就不再保证「闸门放行前令牌必然已置位」，\
         而它自己看不见间隔变了——这个上界就是为守住那句话存在的。",
        HOST_SETTLE / HOST_SETTLE_POLLS,
    );
}

// ---------------------------------------------------------------- 2：实测量

/// 把阶段 hold 住不动，实测看守者的轮询速率，并断言它由 `CANCEL_POLL_INTERVAL` 决定。
///
/// 为什么必须"阶段仍在跑"时采样：阶段一返回，`run_stage_watched` 立刻 abort + await
/// 看守者，读次数冻结是必然的，后面所有"读次数不再增长"的断言都会变成空转。
///
/// 为什么上下界都留：`upper` 杀的是间隔退化成 0——`sleep(0)` 不再让出，
/// 看门狗变成全局互斥锁上的自旋，把写取消意图的 `request_cancel` 饿死，
/// 也就是饿死它自己要读的那一位；`lower` 杀的是轮询停摆（间隔被改成一个大到
/// 窗口里落不下一次的值，或者看门狗压根没跑起来）。
#[tokio::test]
async fn the_watchdog_actually_polls_at_cancel_poll_interval() {
    assert!(
        !CANCEL_POLL_INTERVAL.is_zero(),
        "轮询间隔为 0：看门狗退化成全局互斥锁上的自旋，\
         会把落取消意图的 `request_cancel` 饿死——正是它要等的那一位写入"
    );

    let mut rig = rig("job-cw-pi", vec![("s1", Plan::Held)]).await;
    let run = rig.start();

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

    let measure_window = periods(MEASURE_PERIODS).min(MEASURE_WINDOW_CAP);
    let before = rig.repo.get_calls();
    tokio::time::sleep(measure_window).await;
    let observed = (rig.repo.get_calls() - before) as u64;

    // 放行阶段，让 dispatch 正常收尾（它会 abort + await 看守者）。
    rig.release.notify_waiters();
    let result = run.await.expect("join").expect("run");
    assert_eq!(result.state, JobState::Succeeded);

    // 此刻读次数必须比入口时多：证明观测窗口里看门狗真的在轮询，
    // 而不是"读次数冻结"这条断言从一个从不轮询的计数器上取到了真。
    assert!(
        rig.repo.get_calls() > entered,
        "整段观测窗口里读次数一个字都没长：下面的速率断言将建立在空转之上"
    );

    // 名义速率由真源算出：窗口 ÷ 间隔。上下界都从它派生，断言里没有一个手写毫秒数。
    let expected = (measure_window.as_nanos() / CANCEL_POLL_INTERVAL.as_nanos()) as u64;
    let lower = expected / 4;
    let upper = expected + expected / 5;

    assert!(
        observed >= lower,
        "实测轮询 {observed} 次 / 观测窗口 {measure_window:?}，\
         按 {CANCEL_POLL_INTERVAL:?} 的名义速率（{expected} 个周期）应当不少于 {lower} 次：\
         看门狗的实际速率低于它自称的间隔"
    );
    assert!(
        observed <= upper,
        "实测轮询 {observed} 次 / 观测窗口 {measure_window:?}，\
         按 {CANCEL_POLL_INTERVAL:?} 的名义速率（{expected} 个周期）应当不超过 {upper} 次：\
         看门狗轮询得比自称的密（间隔趋近 0 时会变成互斥锁上的自旋）"
    );
}
