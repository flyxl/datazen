//! §9.3 CM-73 的**真实线程**版：空闲驱逐线程 vs 持有事务句柄的线程。
//!
//! `cm73.rs` 的编排是**单线程行为驱动**的（顺序由 journal 的 `seq` 表达），
//! 它能证明「语义上先回滚注销、再归还资源」这条顺序被记下来了，
//! 但它**证明不了**并发：如果所有步骤本来就串行，谁先谁后无关紧要。
//! 本文件补的就是这一点 —— 两个真实 OS 线程，顺序是**可断言、可复现**的。
//!
//! ## 交错顺序怎么被钉死的
//!
//! 全程**没有 sleep**。顺序由两样东西承担，都不是「等一会儿看起来像」：
//!
//! 1. **阻塞生效**用 [`Barrier::wait_for_waiters`] 断言：它等到
//!    `waiter_count(HOLDER_PARKED) == 1`，也就是持有线程**确实卡在**
//!    `wait_released` 里面。只看 `arrive` 证明不了这个 —— 到达之后线程完全可能
//!    一路跑完，于是「它还停着」就退化成撞运气（这正是只靠 `arrive` 编排时的实测）。
//! 2. **谁先谁后**用 `Barrier` 的 `seq` 断言：五个点共用一个原子计数器，
//!    `parked < eviction-precheck < eviction-done < release < resumed`。
//!    计数器是单线程递增的，所以这个不等式与线程调度无关，是逻辑必然。
//!    （§6.4 纪律：顺序断言只认 journal `seq` 或本夹具的 `Barrier`。）
//!
//! `release` 全程**只有编排线程会发**，所以持有线程不可能提前解除阻塞；
//! 「驱逐落在窗口内」因此是推出来的，不是等出来的。
//!
//! ## 判负从哪来
//!
//! 光断言「驱逐时拒绝了归池」不够 —— 夹具也可能永远拒绝。同一个用例里
//! 第二个真实线程对空闲资源走**同一条**驱逐入口，事件里必须有 `ReturnedToPool`；
//! 文件末尾的 `an_idle_resource_with_no_transaction_still_returns_to_the_pool`
//! 再把完整事件序列钉死。两者之差才是竞态的影响。

use serde_json::json;

use super::{FakeHarness, GatewayError};
use crate::connection::error::ProviderError;
use crate::connection::execution::SessionCommand;
use crate::connection::port::{BudgetClass, CloseReceipt};
use crate::connection::session::SessionState;
use crate::connection::testing::barrier::BarrierToken;
use crate::connection::testing::fake_resource::FakeResourceProvider;
use crate::connection::testing::fixtures::{self, NS_A_KEY};
use crate::connection::testing::harness::fixture_target;
use crate::connection::testing::journal::{JournalEntry, ResourceEvent};
use crate::connection::types::{HandleId, JobId, OrganizationId, OwnerRef, ResourceId, WorkerId};

/// 持有事务的线程停在哪一个 barrier tag 上。
const HOLDER_PARKED: &str = "cm73-real-thread-holder-parked";
/// 驱逐线程动资源**之前**的 tag。
const EVICTION_PRECHECK: &str = "cm73-real-thread-eviction-precheck";
/// 驱逐线程动资源**之后**的 tag。
const EVICTION_DONE: &str = "cm73-real-thread-eviction-done";
/// 编排线程发 `release` 之前的 tag。
const RELEASE_POINT: &str = "cm73-real-thread-release-point";
/// 持有线程解除阻塞、提交之后的 tag。
const HOLDER_RESUMED: &str = "cm73-real-thread-holder-resumed";

/// §8.1：夹具目标取自 `fixtures`，用例里不写硬编码命名空间字面量。
fn harness_for(namespace_key: &str) -> FakeHarness {
    FakeHarness::from_provider(
        FakeResourceProvider::new(WorkerId::new("w1"), fixture_target(namespace_key))
            .with_execution_identity(fixtures::IDENTITY_SHARED),
    )
}

/// §8.1 `PROFILE_P` 归属：一个 job owner。
fn owner() -> OwnerRef {
    OwnerRef::Job {
        organization_id: OrganizationId::new(fixtures::ORG_A),
        job_id: JobId::new("job_org-alpha_0001"),
        stage_id: "job:job_org-alpha_0001/stage:1".to_owned(),
    }
}

/// 某个资源在台账上的资源级事件名（按 `seq`）。
fn resource_events(harness: &FakeHarness, resource_id: &ResourceId) -> Vec<&'static str> {
    harness
        .journal()
        .entries()
        .iter()
        .filter_map(|entry| match entry {
            JournalEntry::Resource {
                resource_id: owner_id,
                event,
                ..
            } if owner_id == resource_id => Some(event.as_str()),
            _ => None,
        })
        .collect()
}

/// 持有事务的线程交回来的东西：它拿到的会话句柄、提交结果与两个顺序点。
struct HolderReport {
    /// `BeginSessionTransactionHold` 交给它的那一个会话句柄。
    handle_id: HandleId,
    /// `Ok(理由)` = 被拒（竞态的正确结局）；`Err(意外)` = 提交居然成功了。
    commit: Result<String, String>,
    /// 停住这个顺序点。
    parked: BarrierToken,
    /// 解除阻塞、提交之后的顺序点。
    resumed: BarrierToken,
}

/// 断言 `earlier` 确实排在 `later` 之前，并把两个 tag 一起报出来。
///
/// 判据是 `Barrier` 的单一原子 `seq`，所以它与线程调度无关：
/// 顺序不成立时必然失败，而不是「大概率失败」。
fn assert_ordered(earlier: &BarrierToken, later: &BarrierToken, what: &str) {
    assert!(
        earlier.seq < later.seq,
        "{what}：{}({}) 必须排在 {}({}) 之前，实际 seq 是 {} 与 {}",
        earlier.tag,
        earlier.seq,
        later.tag,
        later.seq,
        earlier.seq,
        later.seq
    );
}

/// 夹具本身必须能被两个线程共享，否则「真实线程竞态」根本无从谈起。
///
/// 这里同时证明两件事：写线程的 `arrive`/`release` 对读线程可见，
/// 且两个线程操作的**是同一个** `Barrier`（读线程等到的计数来自写线程那次 arrive）。
#[test]
fn the_fake_harness_can_be_driven_from_two_threads_at_once() {
    let harness = harness_for(NS_A_KEY);
    const SHARED: &str = "cm73-sync";
    let seen = std::thread::scope(|scope| {
        let writer = scope.spawn(|| {
            harness.barrier().arrive(SHARED);
            harness.barrier().release(SHARED);
        });
        let reader = scope.spawn(|| {
            // 有界等待写线程那次 arrive；等不到就是并发没成立，不是本用例可以忽略的失败。
            harness.barrier().wait_for_count(SHARED, 1);
            harness.barrier().arrival_count(SHARED)
        });
        writer.join().expect("写线程 panic 了");
        reader.join().expect("读线程 panic 了")
    });
    assert_eq!(
        seen, 1,
        "读线程必须看到写线程那一次 arrive（共享的是同一个 Barrier）；实际看到 {seen}"
    );
}

/// §9.3 真实线程版：驱逐线程与持有事务句柄的线程真并发，可控顺序，行为有差。
///
/// 断言五件事：
/// 1. **阻塞生效** —— 持有线程确实卡在 `wait_released` 里（`wait_for_waiters`），
///    驱逐**前后**各查一次；这排除了「它一路跑完了，只是慢」。
/// 2. **交错顺序**成立 —— 五个 barrier tag 的 `seq` 严格递增。
/// 3. **行为差异**成立 —— 竞态窗口里的驱逐**拒绝归池**（台账里只有 `Closed`，
///    没有 `ReturnedToPool`），同一条驱逐入口对空闲资源则照常归池。
/// 4. **竞态后果**成立 —— 持有线程解除阻塞后的提交被拒（资源已关闭）。
/// 5. 收尾 I1–I8 与变化点断言全部收口。
#[test]
fn a_real_eviction_thread_meets_a_real_thread_holding_a_transaction() {
    let harness = harness_for(NS_A_KEY);

    let acquired = harness
        .acquire(owner(), "pol-real-thread", BudgetClass::Session)
        .expect("R1 必须能取到：取资源是本用例的前置，不是被测语义");
    let racing_handle = acquired.handle.clone();
    let racing_resource = acquired.resource_id.clone();

    // 对照资源（无事务）在进线程作用域之前就取好：spawn 闭包只能借用作用域外已存在的数据。
    let idle = harness
        .acquire(owner(), "pol-real-thread", BudgetClass::Session)
        .expect("对照资源必须能取到：取资源是前置，不是被测语义");
    let idle_handle = idle.handle.clone();
    let idle_resource = idle.resource_id.clone();

    let (receipt, report, precheck, done, release_point) = std::thread::scope(|scope| {
        // ---- 线程 A：持有事务句柄的线程 ----
        let holder = scope.spawn(|| -> Result<HolderReport, String> {
            let opened = harness
                .invoke(
                    SessionCommand::BeginSessionTransactionHold,
                    &racing_handle,
                    json!({ "holdMs": 5 }),
                )
                .map_err(|err| format!("begin 失败：{err:?}"))?;
            let handle_id = HandleId::new(
                opened
                    .data
                    .pointer("/sessionHandles/0/handleId")
                    .and_then(|value| value.as_str())
                    .ok_or_else(|| format!("begin 没有交出会话句柄，实际是 {}", opened.data))?
                    .to_owned(),
            );
            let parked = harness.barrier().arrive(HOLDER_PARKED);
            harness.barrier().wait_released(HOLDER_PARKED);

            // 解除阻塞后提交：驱逐已经发生，这次提交必须被拒。
            let commit = harness.invoke(
                SessionCommand::CommitSessionTransaction,
                &racing_handle,
                json!({ "handleId": handle_id.as_str() }),
            );
            let reported = match commit {
                Err(GatewayError::Provider(ProviderError::SessionLost(message))) => Ok(message),
                other => Err(format!("{other:?}")),
            };
            let resumed = harness.barrier().arrive(HOLDER_RESUMED);
            Ok(HolderReport {
                handle_id,
                commit: reported,
                parked,
                resumed,
            })
        });

        // ---- 编排线程：确认 A 真的停住了，才允许驱逐发生 ----
        // 阻塞生效的证明（不是「等一会儿看看」）：等 waiter_count 到 1。
        harness.barrier().wait_for_waiters(HOLDER_PARKED, 1);
        let parked = harness.barrier().arrival_count(HOLDER_PARKED);
        assert_eq!(parked, 1, "持有线程必须恰好 arrive 过一次");

        // 停在窗口里时的快照：事务开着、句柄登记着 —— 这正是 §9.3 竞态的现场。
        let snapshot = harness.provider().resource(&racing_resource);
        assert!(
            snapshot
                .as_ref()
                .is_some_and(|state| state.has_open_transaction()),
            "驱逐窗口内事务必须仍然是开的，否则这条用例根本没覆盖到竞态"
        );
        assert_eq!(
            snapshot.as_ref().map(|state| state.registered_handles()),
            Some(1),
            "驱逐窗口内必须恰好有一个登记中的句柄"
        );

        // ---- 线程 B：驱逐线程，在 A 阻塞期间真的跑 ----
        let evictor = scope.spawn(|| {
            let precheck = harness.barrier().arrive(EVICTION_PRECHECK);
            let closed = harness.evict_idle_resource(&racing_handle);
            let done = harness.barrier().arrive(EVICTION_DONE);
            (precheck, done, closed)
        });
        let (precheck, done, close) = evictor.join().expect("驱逐线程 panic 了，竞态就没测到");
        let receipt = close.expect("驱逐必须成功：驱动路径失败不是本用例的被测语义");

        // 驱逐跑完了，A 必须**仍然**堵在 wait_released 里：release 还没发。
        assert_eq!(
            harness.barrier().waiter_count(HOLDER_PARKED),
            1,
            "驱逐期间持有线程必须还阻塞着（release 此刻尚未发出），实际停着的线程数是 {}",
            harness.barrier().waiter_count(HOLDER_PARKED)
        );

        // 行为差异：竞态窗口里的驱逐不得把资源归池（§5.3 规则 2 前置）。
        let events = resource_events(&harness, &racing_resource);
        assert!(
            events.contains(&"Closed"),
            "驱逐必须留下 Closed 事件，实际事件序列是 {events:?}"
        );
        assert!(
            !events.contains(&"ReturnedToPool"),
            "持有线程还挂着事务句柄时不得归池（这是 §9.3 要钉住的行为），实际事件序列是 {events:?}"
        );

        // ---- 对照：同一驱逐入口，在另一条真实线程上跑无事务资源 ----
        let control = scope.spawn(|| harness.evict_idle_resource(&idle_handle));
        control
            .join()
            .expect("对照驱逐线程 panic 了")
            .expect("对照驱逐必须成功");
        let idle_events = resource_events(&harness, &idle_resource);
        assert!(
            idle_events.contains(&"ReturnedToPool"),
            "无事务的资源走同一条驱逐路径必须归池，否则上面的拒绝说明不了是竞态造成的，实际事件序列是 {idle_events:?}"
        );

        // ---- 放开 A，让它带着已关闭的资源继续 ----
        let release_point = harness.barrier().arrive(RELEASE_POINT);
        harness.barrier().release(HOLDER_PARKED);
        let report = holder
            .join()
            .expect("持有线程 panic 了")
            .unwrap_or_else(|err| panic!("持有事务的线程没跑通：{err}"));
        (receipt, report, precheck, done, release_point)
    });

    assert_eq!(
        receipt.state,
        SessionState::Closed,
        "驱逐后资源必须处于 Closed"
    );
    // 顺序不等式：全靠 `Barrier` 的单一原子 `seq`，与线程调度无关。
    assert_ordered(&report.parked, &precheck, "持有线程停住");
    assert_ordered(&precheck, &done, "驱逐线程进入");
    assert_ordered(&done, &release_point, "驱逐线程完成");
    assert_ordered(&release_point, &report.resumed, "编排线程放行");
    assert_eq!(
        [
            report.parked.tag.as_str(),
            precheck.tag.as_str(),
            done.tag.as_str(),
            release_point.tag.as_str(),
            report.resumed.tag.as_str()
        ],
        [
            HOLDER_PARKED,
            EVICTION_PRECHECK,
            EVICTION_DONE,
            RELEASE_POINT,
            HOLDER_RESUMED
        ],
        "五个顺序点必须分别是停住/进入/完成/放行/恢复"
    );
    let registered = harness.registered_handle_ids_on(&racing_resource);
    assert!(
        registered.contains(&report.handle_id),
        "持有线程拿到的会话句柄 {} 必须在台账上登记过（竞态窗口的现场），实际登记的是 {registered:?}",
        report.handle_id.as_str()
    );
    // 持有线程回报的是「被拒理由」；Err 分支才是意外（提交居然成功了）。
    let rejection = report.commit.unwrap_or_else(|unexpected| {
        panic!(
            "被驱逐后拿旧句柄提交必须被拒；实际提交结果是 {unexpected}，句柄 {}",
            report.handle_id.as_str()
        )
    });
    assert!(
        rejection.contains("已关闭"),
        "拒绝理由必须说明资源已关闭，实际是「{rejection}」"
    );

    // 竞态不得留下任何泄漏。句柄复用的判负要用**恢复资源**来判：R1 是被关掉的那一个，
    // 它在竞态窗口里本来就登记着句柄（上面已断言），拿它当判负对象是判错了对象。
    // 竞态过后再取一次资源：拿到的（来自池子恢复的）资源上不得带任何句柄登记。
    let after_race = harness
        .acquire(owner(), "pol-after-race", BudgetClass::Session)
        .expect("竞态后取资源是判负的前置，不是被测语义");
    harness
        .assert_no_handle_reuse(&after_race.resource_id)
        .unwrap_or_else(|reason| panic!("§9.3 判负（正例不该触发）：{reason}"));
    harness
        .close(&after_race.handle)
        .expect("判负用的资源必须能正常关掉，否则泄漏检查会把它算进去");
    if let Err(violations) = harness.assert_no_leak() {
        panic!("真实线程竞态收尾不得留下泄漏：{violations}");
    }
}

/// 隔离掉竞态因素后，单看驱逐入口：空闲资源**确实**会被归还到池子。
///
/// 这条用例单独存在，是为了让上一条里的「拒绝归池」有可比的对照 ——
/// 否则「夹具永远不归池」和「竞态导致不归池」在断言上长得一模一样。
#[test]
fn an_idle_resource_with_no_transaction_still_returns_to_the_pool() {
    let harness = harness_for(NS_A_KEY);
    let idle = harness
        .acquire(owner(), "pol-control", BudgetClass::Session)
        .expect("取资源是前置，不是被测语义");
    let receipt: CloseReceipt = harness
        .evict_idle_resource(&idle.handle)
        .expect("空闲资源必须驱逐成功");

    assert_eq!(receipt.state, SessionState::Closed);
    let events = resource_events(&harness, &idle.resource_id);
    assert_eq!(
        events,
        vec!["Created", "OpeningReady", "ReturnedToPool", "Closed"],
        "空闲驱逐的事件序列是固定的；变了就说明对照失效"
    );
    // 明确排除另一条事件，防止「台账里混进别的资源事件」被这条用例漏过去。
    assert!(
        !events.contains(&ResourceEvent::Quarantined.as_str()),
        "空闲驱逐不得把资源隔离"
    );
    if let Err(violations) = harness.assert_no_leak() {
        panic!("空闲驱逐收尾不得留下泄漏：{violations}");
    }
}
