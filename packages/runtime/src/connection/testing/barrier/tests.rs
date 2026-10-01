//! `Barrier` / `DrainBarrier` / sink 的单测。
//!
//! 依赖方向同父模块。§6.4：这里允许用「观察某个状态位没有变化」来断言**阻塞**语义
//! （负向断言），不允许用裸 `sleep` 推导顺序。

use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::*;
use crate::connection::execution::{ExecutionErrorCode, ResultSink, SinkWrite, TruncationReason};
use crate::connection::testing::clock::FakeClock;
use crate::connection::types::{Counter, ExecutionId};

#[test]
fn arrival_order_comes_from_seq_not_from_timing() {
    let barrier = Barrier::new();
    let first = barrier.arrive("session-created");
    let second = barrier.arrive("eviction-close-precheck");
    assert!(first.seq < second.seq, "先到达的 seq 必须更小");
    barrier.assert_arrived_before("session-created", "eviction-close-precheck");
    let arrivals = barrier.order();
    let order: Vec<&str> = arrivals.iter().map(|a| a.tag.as_str()).collect();
    assert_eq!(order, vec!["session-created", "eviction-close-precheck"]);
}

#[test]
fn concurrent_arrivals_still_produce_a_deterministic_sequence() {
    let barrier = Barrier::new();
    let mut handles = Vec::new();
    for _ in 0..4 {
        let barrier = barrier.clone();
        handles.push(std::thread::spawn(move || barrier.arrive("racing")));
    }
    for handle in handles {
        handle.join().expect("线程必须正常结束");
    }
    barrier.wait_for_count("racing", 4);
    assert_eq!(barrier.arrival_count("racing"), 4);
    let seqs: Vec<u64> = barrier.order().into_iter().map(|a| a.seq).collect();
    let mut sorted = seqs.clone();
    sorted.sort_unstable();
    assert_eq!(seqs, sorted, "seq 必须唯一且单调，顺序可断言");
    sorted.dedup();
    assert_eq!(sorted.len(), 4, "并发到达不得产生重复 seq");
}

#[test]
fn wait_for_blocks_until_the_tag_is_reached() {
    let barrier = Barrier::new();
    let worker = {
        let barrier = barrier.clone();
        std::thread::spawn(move || {
            // `wait_for` 等的是「tag 已被到达」；必须先 arrive 才可能有人观察到。
            barrier.arrive("hold-point");
            barrier.wait_for("hold-point");
            barrier.release("hold-point");
        })
    };
    barrier.wait_for("hold-point");
    barrier.wait_released("hold-point");
    assert!(barrier.is_released("hold-point"));
    worker.join().expect("线程必须正常结束");
}

#[test]
fn wait_released_really_blocks_instead_of_racing_the_releaser() {
    // 上一个用例在语义上是**竞态**：主线程先 `wait_for` 到货，子线程随后
    // `release`，两者之间没有任何「阻塞」保证。把 `wait_released` 的循环掏成
    // 空操作，`assert!(is_released)` 仍有相当概率在子线程 `release` 之后才被
    // 求值，于是掏空了照样全绿。这里改为**负向断言**。
    let barrier = Barrier::new();
    let (gate_tx, gate_rx) = mpsc::channel::<()>();
    let (started_tx, started_rx) = mpsc::channel::<()>();
    let (returned_tx, returned_rx) = mpsc::channel::<()>();

    let holder = {
        let barrier = barrier.clone();
        std::thread::spawn(move || {
            barrier.arrive("hold-point");
            // 只有主线程放行才 `release`；主线程 panic 时 sender 被丢弃，
            // `recv` 返回 `Err`，本线程随之退出，不会变成永远空转的孤儿。
            let _ = gate_rx.recv();
            barrier.release("hold-point");
        })
    };
    let waiter = {
        let barrier = barrier.clone();
        std::thread::spawn(move || {
            let _ = started_tx.send(());
            barrier.wait_released("hold-point");
            let _ = returned_tx.send(());
        })
    };

    // 先确认等待线程真的跑起来了，否则下面的负向断言是空转的。
    started_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("等待线程没有启动");
    barrier.wait_for("hold-point");

    // 负向断言：`release` 之前 `wait_released` **不得**返回。
    // §6.4 允许的是「观察状态位没有变化」，这里正是如此；不依赖 sleep 推导顺序。
    let window = Instant::now();
    while window.elapsed() < Duration::from_millis(200) {
        assert!(
            returned_rx.try_recv().is_err(),
            "`wait_released` 在同线程 `release` 之前就返回了 —— 阻塞语义已被架空"
        );
        std::thread::yield_now();
    }

    let _ = gate_tx.send(());
    holder.join().expect("放行线程必须正常结束");
    returned_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("`release` 之后 `wait_released` 仍未返回 —— 实现死锁或永远阻塞");
    waiter.join().expect("等待线程必须正常结束");
    assert!(barrier.is_released("hold-point"));
}

#[test]
fn a_blocked_path_only_continues_after_release() {
    // §9.3：淘汰在关闭前置检查处挂起；此时发起 begin，落点必须在同一资源上。
    let barrier = Barrier::new();
    barrier.arrive("eviction-close-precheck");
    assert!(!barrier.is_released("eviction-close-precheck"));
    barrier.release("eviction-close-precheck");
    assert!(barrier.is_released("eviction-close-precheck"));
    // 放行之后才到达的续点：顺序断言必须建立在**两个**真实到达之上。
    barrier.arrive("eviction-released");
    barrier.assert_arrived_before("eviction-close-precheck", "eviction-released");
}

#[test]
fn write_without_consumer_blocks_then_truncates_after_the_drain_deadline() {
    // §6.2 竞态表 CM-64：无消费者 → 写入等待 → FakeClock 推进 10s → 截断。
    let clock = FakeClock::new();
    let drain = DrainBarrier::new(clock.clone(), DrainLimits::lowered_for_tests());
    drain.set_no_consumer(true);
    let execution = ExecutionId::new("exe_dbs_w1_0001_0001");

    assert_eq!(drain.write(&execution, 0, 1024), SinkWrite::Blocked);
    assert_eq!(drain.write(&execution, 1, 1024), SinkWrite::Blocked);
    assert!(
        !drain.drain_deadline_elapsed(),
        "未推进时钟前期限不得视为到期"
    );
    assert_eq!(drain.truncation_of(&execution), None);

    clock.advance(Duration::from_secs(10));
    assert!(drain.drain_deadline_elapsed());
    assert_eq!(
        drain.write(&execution, 2, 1024),
        SinkWrite::Truncated(TruncationReason::NoConsumerDrainDeadline)
    );
    assert_eq!(
        drain.truncation_of(&execution),
        Some(TruncationReason::NoConsumerDrainDeadline)
    );
    assert_eq!(drain.produced_bytes(), 0, "无消费者时不得计为已产出");
}

#[test]
fn a_consumer_restores_acceptance_without_truncation() {
    let clock = FakeClock::new();
    let drain = DrainBarrier::new(clock.clone(), DrainLimits::lowered_for_tests());
    let execution = ExecutionId::new("exe_dbs_w1_0001_0001");
    drain.set_no_consumer(true);
    assert_eq!(drain.write(&execution, 0, 512), SinkWrite::Blocked);
    drain.set_no_consumer(false);
    assert_eq!(drain.write(&execution, 1, 512), SinkWrite::Accepted);
    clock.advance(Duration::from_secs(600));
    assert_eq!(
        drain.truncation_of(&execution),
        None,
        "消费者在期限内接管不得产生截断"
    );
    assert_eq!(drain.produced_bytes(), 512);
    assert_eq!(drain.produced_events(), 1);
}

#[test]
fn per_execution_byte_limit_truncates_with_its_own_reason() {
    // §6.2：每执行 8 MiB → 测试下调到 64 KiB；触顶后必须截断或按字节背压。
    let clock = FakeClock::new();
    let drain = DrainBarrier::new(clock, DrainLimits::lowered_for_tests());
    let execution = ExecutionId::new("exe_dbs_w1_0001_0002");
    assert_eq!(drain.write(&execution, 0, 32 * 1024), SinkWrite::Accepted);
    assert_eq!(drain.write(&execution, 1, 32 * 1024), SinkWrite::Accepted);
    assert_eq!(
        drain.write(&execution, 2, 1),
        SinkWrite::Truncated(TruncationReason::PerExecutionByteLimit)
    );
    assert_eq!(
        drain.truncation_of(&execution),
        Some(TruncationReason::PerExecutionByteLimit)
    );
    drain.finish_execution(&execution);
    assert_eq!(
        drain.write(&execution, 3, 1),
        SinkWrite::Accepted,
        "新执行应重新计量"
    );
}

#[test]
fn subscription_limits_are_counted_independently_of_the_execution_limit() {
    let clock = FakeClock::new();
    let limits = DrainLimits {
        events_per_subscription: 3,
        bytes_per_subscription: 1024 * 1024,
        ..DrainLimits::lowered_for_tests()
    };
    let drain = DrainBarrier::new(clock, limits);
    let a = ExecutionId::new("exe_dbs_w1_0001_0001");
    let b = ExecutionId::new("exe_dbs_w1_0001_0002");
    for index in 0..3 {
        assert_eq!(drain.write(&a, index, 8), SinkWrite::Accepted);
    }
    assert_eq!(
        drain.write(&b, 0, 8),
        SinkWrite::Truncated(TruncationReason::PerSubscriptionEventLimit)
    );
    assert_eq!(
        drain.truncation_of(&b),
        Some(TruncationReason::PerSubscriptionEventLimit)
    );
    assert_eq!(
        drain.truncation_of(&a),
        None,
        "订阅上限不得追溯到先前已接受的执行"
    );
}

#[test]
fn await_drain_wakes_when_the_clock_crosses_the_deadline() {
    let clock = FakeClock::new();
    let drain = DrainBarrier::new(clock.clone(), DrainLimits::design());
    drain.set_no_consumer(true);
    let execution = ExecutionId::new("exe_dbs_w1_0001_0001");
    assert_eq!(drain.write(&execution, 0, 64), SinkWrite::Blocked);

    let waiter = {
        let drain = drain.clone();
        let execution = execution.clone();
        std::thread::spawn(move || drain.await_drain(&execution))
    };
    clock.advance(Duration::from_secs(10));
    let reason = waiter.join().expect("线程必须正常结束");
    assert_eq!(reason, Some(TruncationReason::NoConsumerDrainDeadline));
}

#[test]
fn collecting_sink_counts_produced_bytes_and_terminal_state() {
    let mut sink = CollectingSink::default();
    let execution = ExecutionId::new("exe_dbs_w1_0001_0001");
    assert_eq!(
        sink.write(&execution, Counter::new(0), 100),
        SinkWrite::Accepted
    );
    assert_eq!(
        sink.write(&execution, Counter::new(1), 150),
        SinkWrite::Accepted
    );
    sink.complete(&execution, 250);
    assert_eq!(sink.produced_bytes(), 250);
    assert_eq!(sink.events(), 2);
    assert_eq!(sink.completed_at(), Some(250));
    assert_eq!(sink.failure(), None);
}

#[test]
fn failing_sink_reports_producer_write_failed() {
    // F5：`ResultSink` 写入失败必须带 `producerWriteFailed`，不得静默成功。
    let mut sink = FailingSink::default();
    let execution = ExecutionId::new("exe_dbs_w1_0001_0001");
    assert_eq!(
        sink.write(&execution, Counter::new(0), 10),
        SinkWrite::Truncated(TruncationReason::ProducerWriteFailed)
    );
    sink.fail(&execution, ExecutionErrorCode::ProtocolError, 0);
    assert_eq!(sink.code, Some(ExecutionErrorCode::ProtocolError));
}
