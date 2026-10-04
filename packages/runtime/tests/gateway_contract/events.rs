//! §3.3 事件投递：容忍陈旧 epoch / 异会话 / 乱序 / 空洞 / 大计数，
//! 陈旧事件既不覆盖状态也不重复计行。

use super::{other_request, record};
use crate::gateway_fixtures as fx;
use datazen_runtime::connection::{Counter, DbSessionId, ExecutionId, ExecutionState};
use datazen_runtime::gateway::{EventDisposition, ExecutionEventKind};
// ───────────────── F §3.3 事件投递 ─────────────────

#[tokio::test(start_paused = true)]
async fn an_event_from_a_stale_epoch_is_ignored() {
    let h = fx::ready_harness();
    let id = fx::accept(&h, fx::request(fx::REVISION)).await;
    let mut event = fx::event(&id, 1, fx::terminal(ExecutionState::Succeeded));
    event.runtime_epoch = Counter::new(0);

    assert_eq!(
        h.gateway.apply_event(event).await,
        EventDisposition::IgnoredStaleEpoch {
            bound: Counter::new(1)
        }
    );
    let record = record(&h, &id).await;
    assert_eq!(record.observed_sequence(), None);
    assert_eq!(record.last_state(), ExecutionState::Queued);
}

#[tokio::test(start_paused = true)]
async fn an_event_from_another_session_is_ignored() {
    let h = fx::ready_harness();
    let id = fx::accept(&h, fx::request(fx::REVISION)).await;
    let mut event = fx::event(&id, 1, fx::terminal(ExecutionState::Succeeded));
    event.db_session_id = DbSessionId::new("db_session_other");

    assert!(matches!(
        h.gateway.apply_event(event).await,
        EventDisposition::IgnoredStaleSession { .. }
    ));
    assert_eq!(record(&h, &id).await.last_state(), ExecutionState::Queued);
}

#[tokio::test(start_paused = true)]
async fn an_out_of_order_event_never_moves_the_watermark_backwards() {
    let h = fx::ready_harness();
    let id = fx::accept(&h, fx::request(fx::REVISION)).await;
    for seq in 1..=3 {
        let chunk = ExecutionEventKind::ResultChunk {
            chunk_index: Counter::new(seq),
            rows: seq,
        };
        assert_eq!(
            h.gateway.apply_event(fx::event(&id, seq, chunk)).await,
            EventDisposition::Applied
        );
    }

    let stale = ExecutionEventKind::ResultChunk {
        chunk_index: Counter::new(2),
        rows: 2,
    };
    let disposition = h.gateway.apply_event(fx::event(&id, 2, stale)).await;
    assert_eq!(
        disposition,
        EventDisposition::IgnoredOutOfOrder {
            sequence: Counter::new(2),
            watermark: Counter::new(3),
        }
    );
    let record = record(&h, &id).await;
    assert_eq!(record.observed_sequence(), Some(Counter::new(3)));
    assert_eq!(record.observed_row_count(), 6);
}

#[tokio::test(start_paused = true)]
async fn a_sequence_gap_requires_recovery_without_advancing_state() {
    let h = fx::ready_harness();
    let id = fx::accept(&h, fx::request(fx::REVISION)).await;
    let first = ExecutionEventKind::ResultChunk {
        chunk_index: Counter::new(0),
        rows: 5,
    };
    h.gateway.apply_event(fx::event(&id, 1, first)).await;

    let jumped = ExecutionEventKind::ResultChunk {
        chunk_index: Counter::new(3),
        rows: 7,
    };
    assert_eq!(
        h.gateway.apply_event(fx::event(&id, 4, jumped)).await,
        EventDisposition::Gap {
            expected: Counter::new(2),
            received: Counter::new(4),
        }
    );
    let record = record(&h, &id).await;
    assert!(record.needs_recovery());
    assert_eq!(record.resubscribe_from(), Some(Counter::new(1)));
    assert_eq!(record.observed_sequence(), Some(Counter::new(1)));
    assert_eq!(record.observed_row_count(), 5);
}

#[tokio::test(start_paused = true)]
async fn a_snapshot_realigns_the_watermark_after_a_gap() {
    let h = fx::ready_harness();
    let id = fx::accept(&h, fx::request(fx::REVISION)).await;
    h.gateway
        .apply_event(fx::event(&id, 1, fx::terminal(ExecutionState::Running)))
        .await;
    let jumped = fx::terminal(ExecutionState::Succeeded);
    h.gateway.apply_event(fx::event(&id, 4, jumped)).await;

    assert!(
        h.gateway
            .recover_from_snapshot(&id, &fx::ready_view())
            .await
    );
    let restored = record(&h, &id).await;
    assert!(!restored.needs_recovery());
    assert_eq!(restored.resubscribe_from(), None);

    let resumed = fx::event(&id, 2, fx::terminal(ExecutionState::Succeeded));
    assert_eq!(
        h.gateway.apply_event(resumed).await,
        EventDisposition::Applied
    );
    assert_eq!(
        record(&h, &id).await.last_state(),
        ExecutionState::Succeeded
    );
}

#[tokio::test(start_paused = true)]
async fn large_counters_are_exact_and_duplicates_do_not_double_count() {
    let h = fx::ready_harness();
    let id = fx::accept(&h, fx::request(fx::REVISION)).await;
    let huge = 9_007_199_254_740_993_u64;

    let mut first = fx::event(
        &id,
        1,
        ExecutionEventKind::ResultChunk {
            chunk_index: Counter::new(huge),
            rows: 7,
        },
    );
    first.sequence = Counter::new(huge);
    assert_eq!(
        h.gateway.apply_event(first.clone()).await,
        EventDisposition::Applied
    );

    // 同一 chunk_index 换个 sequence 也不许重复计入。
    let mut again = first;
    again.sequence = Counter::new(huge + 1);
    assert_eq!(
        h.gateway.apply_event(again).await,
        EventDisposition::IgnoredDuplicateChunk {
            chunk_index: Counter::new(huge)
        }
    );
    assert_eq!(record(&h, &id).await.observed_row_count(), 7);
    // 序号被消费（水位跟到 huge + 1），行数不被重复计入。
    assert_eq!(
        record(&h, &id).await.observed_sequence(),
        Some(Counter::new(huge + 1))
    );
}

#[tokio::test(start_paused = true)]
async fn an_event_for_an_unknown_execution_is_unbound() {
    let h = fx::ready_harness();
    let event = fx::event(
        &ExecutionId::new("exe_contract_none"),
        1,
        fx::terminal(ExecutionState::Succeeded),
    );
    assert_eq!(
        h.gateway.apply_event(event).await,
        EventDisposition::Unbound
    );
}

#[tokio::test(start_paused = true)]
async fn two_executions_in_one_session_keep_independent_watermarks() {
    let h = fx::ready_harness();
    let first = fx::accept(&h, fx::request(fx::REVISION)).await;
    let second = fx::accept(&h, other_request()).await;
    assert_ne!(first, second);

    let chunk = ExecutionEventKind::ResultChunk {
        chunk_index: Counter::new(0),
        rows: 3,
    };
    h.gateway.apply_event(fx::event(&first, 1, chunk)).await;
    let tail = ExecutionEventKind::ResultChunk {
        chunk_index: Counter::new(0),
        rows: 9,
    };
    h.gateway.apply_event(fx::event(&second, 7, tail)).await;

    assert_eq!(
        record(&h, &first).await.observed_sequence(),
        Some(Counter::new(1))
    );
    assert_eq!(
        record(&h, &second).await.observed_sequence(),
        Some(Counter::new(7))
    );
}

#[tokio::test(start_paused = true)]
async fn g1_snapshot_recovery_must_not_regress_a_terminal_state() {
    let h = fx::ready_harness();
    let id = fx::accept(&h, fx::request(fx::REVISION)).await;
    h.gateway
        .dispatch(&fx::principal(), &id)
        .await
        .expect("应当下发到驱动");
    assert_eq!(
        h.gateway
            .apply_event(fx::event(&id, 1, fx::terminal(ExecutionState::Succeeded)))
            .await,
        EventDisposition::Applied
    );

    // 制造缺口：序号跳跃意味着中间帧丢失，必须先读快照才能继续。
    let jumped = h
        .gateway
        .apply_event(fx::event(&id, 4, fx::terminal(ExecutionState::Succeeded)))
        .await;
    assert!(
        matches!(jumped, EventDisposition::Gap { .. }),
        "期望缺口，实际 {jumped:?}"
    );
    assert!(record(&h, &id).await.needs_recovery());
    assert!(
        h.gateway
            .recover_from_snapshot(&id, &fx::ready_view())
            .await
    );

    // 快照只对齐空洞，**不得**把已经终态的执行打回排队中。
    let restored = record(&h, &id).await;
    assert!(!restored.needs_recovery());
    assert_eq!(restored.last_state(), ExecutionState::Succeeded);
    assert_eq!(restored.observed_sequence(), Some(Counter::new(1)));
}

#[tokio::test(start_paused = true)]
async fn g2_snapshot_recovery_must_not_zero_the_observed_row_count() {
    let h = fx::ready_harness();
    let id = fx::accept(&h, fx::request(fx::REVISION)).await;
    for (sequence, chunk, rows) in [(1_u64, 0_u64, 5_u64), (2, 1, 7)] {
        assert_eq!(
            h.gateway
                .apply_event(fx::event(
                    &id,
                    sequence,
                    ExecutionEventKind::ResultChunk {
                        chunk_index: Counter::new(chunk),
                        rows,
                    },
                ))
                .await,
            EventDisposition::Applied
        );
    }
    assert_eq!(record(&h, &id).await.observed_row_count(), 12);

    let jumped = h
        .gateway
        .apply_event(fx::event(&id, 6, fx::terminal(ExecutionState::Succeeded)))
        .await;
    assert!(
        matches!(jumped, EventDisposition::Gap { .. }),
        "期望缺口，实际 {jumped:?}"
    );
    assert!(
        h.gateway
            .recover_from_snapshot(&id, &fx::ready_view())
            .await
    );

    // 快照不带行数，清零等于把已经上报的结果从审计账上抹掉（CM-55）。
    let restored = record(&h, &id).await;
    assert_eq!(restored.observed_row_count(), 12);
    assert_eq!(restored.observed_sequence(), Some(Counter::new(2)));
}

#[tokio::test(start_paused = true)]
async fn g3_a_replayed_event_after_snapshot_recovery_must_not_double_count_rows() {
    let h = fx::ready_harness();
    let id = fx::accept(&h, fx::request(fx::REVISION)).await;
    let chunk = ExecutionEventKind::ResultChunk {
        chunk_index: Counter::new(0),
        rows: 5,
    };
    assert_eq!(
        h.gateway.apply_event(fx::event(&id, 1, chunk)).await,
        EventDisposition::Applied
    );

    let jumped = h
        .gateway
        .apply_event(fx::event(&id, 3, fx::terminal(ExecutionState::Succeeded)))
        .await;
    assert!(
        matches!(jumped, EventDisposition::Gap { .. }),
        "期望缺口，实际 {jumped:?}"
    );
    assert!(
        h.gateway
            .recover_from_snapshot(&id, &fx::ready_view())
            .await
    );

    // 重连后上游从空洞之前重放：序号已在水位里，必须判重复而不是再加一次行。
    let replayed = fx::event(
        &id,
        1,
        ExecutionEventKind::ResultChunk {
            chunk_index: Counter::new(0),
            rows: 5,
        },
    );
    assert_eq!(
        h.gateway.apply_event(replayed).await,
        EventDisposition::IgnoredDuplicate {
            sequence: Counter::new(1)
        }
    );
    let restored = record(&h, &id).await;
    assert_eq!(restored.observed_row_count(), 5);
    assert_eq!(restored.observed_sequence(), Some(Counter::new(1)));
}

#[tokio::test(start_paused = true)]
async fn g5_an_event_naming_another_execution_is_unbound_and_perturbs_nothing() {
    let h = fx::ready_harness();
    let id = fx::accept(&h, fx::request(fx::REVISION)).await;
    assert_eq!(
        h.gateway
            .apply_event(fx::event(
                &id,
                1,
                ExecutionEventKind::ResultChunk {
                    chunk_index: Counter::new(0),
                    rows: 5,
                },
            ))
            .await,
        EventDisposition::Applied
    );
    assert_eq!(
        h.gateway
            .apply_event(fx::event(&id, 2, fx::terminal(ExecutionState::Succeeded)))
            .await,
        EventDisposition::Applied
    );

    // 同会话里指向**别的**执行的帧：既不能绑到本执行，也不许碰它的水位。
    let stranger = ExecutionId::new("exe_contract_stranger");
    let foreign = fx::event(
        &stranger,
        7,
        ExecutionEventKind::ResultChunk {
            chunk_index: Counter::new(0),
            rows: 999,
        },
    );
    assert_eq!(
        h.gateway.apply_event(foreign).await,
        EventDisposition::Unbound
    );

    let untouched = record(&h, &id).await;
    assert_eq!(untouched.observed_row_count(), 5);
    assert_eq!(untouched.observed_sequence(), Some(Counter::new(2)));
    assert_eq!(untouched.last_state(), ExecutionState::Succeeded);
    assert_eq!(h.gateway.execution_count().await, 1);
    assert!(!untouched.needs_recovery());
}
