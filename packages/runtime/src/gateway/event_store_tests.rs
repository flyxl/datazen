//! `EventStore` 的单元测试。
//!
//! 从 `events.rs` 里拆出来单独成文件：事件水位是本轨道行数最多的模块，
//! 把测试留在同一文件会顶破 800 行上限。
//!
//! 这里只用 `events` 的**公开**接口（`pub` 方法与 `pub` 类型），因此它测的
//! 是事件水位本身的行为，而不是任何测试专用旁路。

use crate::connection::{ContextConfidence, Counter, DbSessionId, ExecutionId, ExecutionState};
use crate::gateway::events::{EventDisposition, EventStore, ExecutionEvent, ExecutionEventKind};
use crate::gateway::provenance::{ExecutionSource, SourceKind};

fn event(sequence: u64, kind: ExecutionEventKind) -> ExecutionEvent {
    ExecutionEvent {
        execution_id: ExecutionId::new("exe_unit"),
        db_session_id: DbSessionId::new("dbse_a"),
        runtime_epoch: Counter::new(5),
        sequence: Counter::new(sequence),
        context_revision: Counter::new(sequence),
        kind,
        declared_source: None,
    }
}

fn bound_store() -> EventStore {
    let mut store = EventStore::new();
    store.bind(DbSessionId::new("dbse_a"), Counter::new(5));
    store
}

#[test]
fn unbound_store_refuses_everything() {
    let mut store = EventStore::new();
    assert!(!store.is_bound());
    assert_eq!(
        store.apply(&event(
            1,
            ExecutionEventKind::Terminal {
                state: ExecutionState::Succeeded,
            }
        )),
        EventDisposition::Unbound
    );
}

#[test]
fn an_event_from_another_session_never_touches_state() {
    let mut store = bound_store();
    store.apply(&event(
        1,
        ExecutionEventKind::StateChanged {
            state: ExecutionState::Running,
        },
    ));
    let mut foreign = event(
        2,
        ExecutionEventKind::Terminal {
            state: ExecutionState::Failed,
        },
    );
    foreign.db_session_id = DbSessionId::new("dbse_OLD");
    assert_eq!(
        store.apply(&foreign),
        EventDisposition::IgnoredStaleSession {
            bound: DbSessionId::new("dbse_a")
        }
    );
    assert_eq!(store.execution_state(), ExecutionState::Running);
    assert_eq!(store.last_sequence().map(Counter::get), Some(1));
}

#[test]
fn an_older_epoch_never_touches_state() {
    let mut store = bound_store();
    store.apply(&event(
        1,
        ExecutionEventKind::StateChanged {
            state: ExecutionState::Running,
        },
    ));
    let mut stale = event(
        2,
        ExecutionEventKind::Terminal {
            state: ExecutionState::Failed,
        },
    );
    stale.runtime_epoch = Counter::new(4);
    assert_eq!(
        store.apply(&stale),
        EventDisposition::IgnoredStaleEpoch {
            bound: Counter::new(5)
        }
    );
    assert_eq!(store.execution_state(), ExecutionState::Running);
    assert_eq!(store.last_sequence().map(Counter::get), Some(1));
}

#[test]
fn a_newer_epoch_never_touches_state() {
    let mut store = bound_store();
    store.apply(&event(
        1,
        ExecutionEventKind::StateChanged {
            state: ExecutionState::Running,
        },
    ));
    let mut foreign = event(
        2,
        ExecutionEventKind::Terminal {
            state: ExecutionState::Succeeded,
        },
    );
    foreign.runtime_epoch = Counter::new(9);
    assert_eq!(
        store.apply(&foreign),
        EventDisposition::IgnoredForeignEpoch {
            bound: Counter::new(5)
        }
    );
    assert_eq!(store.execution_state(), ExecutionState::Running);
}

#[test]
fn out_of_order_arrival_is_rejected_without_rewinding_the_watermark() {
    let mut store = bound_store();
    store.apply(&event(
        10,
        ExecutionEventKind::StateChanged {
            state: ExecutionState::Running,
        },
    ));
    let late = event(
        4,
        ExecutionEventKind::Terminal {
            state: ExecutionState::Failed,
        },
    );
    assert_eq!(
        store.apply(&late),
        EventDisposition::IgnoredOutOfOrder {
            sequence: Counter::new(4),
            watermark: Counter::new(10)
        }
    );
    assert_eq!(store.execution_state(), ExecutionState::Running);
    assert_eq!(store.last_sequence().map(Counter::get), Some(10));
}

#[test]
fn a_replayed_sequence_is_a_duplicate_not_a_second_application() {
    let mut store = bound_store();
    let first = event(
        1,
        ExecutionEventKind::ResultChunk {
            chunk_index: Counter::new(0),
            rows: 10,
        },
    );
    assert!(store.apply(&first).mutated_state());
    assert_eq!(
        store.apply(&first),
        EventDisposition::IgnoredDuplicate {
            sequence: Counter::new(1)
        }
    );
    assert_eq!(store.row_count(), 10);
}

#[test]
fn a_repeated_chunk_index_does_not_duplicate_rows() {
    let mut store = bound_store();
    let chunk = ExecutionEventKind::ResultChunk {
        chunk_index: Counter::new(7),
        rows: 5,
    };
    store.apply(&event(1, chunk.clone()));
    store.apply(&event(2, chunk.clone()));
    // 同一个 chunk_index 再来一次，序号是新的但内容重复。
    assert_eq!(
        store.apply(&event(3, chunk)),
        EventDisposition::IgnoredDuplicateChunk {
            chunk_index: Counter::new(7)
        }
    );
    assert_eq!(store.row_count(), 5, "重复分片不得让行数再涨");
    assert_eq!(
        store.last_sequence().map(Counter::get),
        Some(3),
        "重复分片的序号仍被消费，否则下一帧会被误判成缺口"
    );
}

#[test]
fn a_gap_never_advances_state_and_asks_for_recovery() {
    let mut store = bound_store();
    store.apply(&event(
        1,
        ExecutionEventKind::StateChanged {
            state: ExecutionState::Running,
        },
    ));
    let gap = event(
        3,
        ExecutionEventKind::Terminal {
            state: ExecutionState::Succeeded,
        },
    );
    let disposition = store.apply(&gap);
    assert_eq!(
        disposition,
        EventDisposition::Gap {
            expected: Counter::new(2),
            received: Counter::new(3)
        }
    );
    assert!(disposition.needs_recovery(), "缺口必须要求读快照或补订阅");
    assert!(store.needs_recovery());
    assert_eq!(store.resubscribe_from(), Some(Counter::new(1)));
    assert_eq!(
        store.execution_state(),
        ExecutionState::Running,
        "缺口期间不得把终态应用上去"
    );
    assert_eq!(store.last_sequence().map(Counter::get), Some(1));
}

#[test]
fn an_ordinary_terminal_needs_no_recovery() {
    let mut store = bound_store();
    let disposition = store.apply(&event(
        1,
        ExecutionEventKind::Terminal {
            state: ExecutionState::Succeeded,
        },
    ));
    assert!(!disposition.needs_recovery());
    assert!(!store.needs_recovery());
    assert_eq!(store.execution_state(), ExecutionState::Succeeded);
}

#[test]
fn a_snapshot_clears_the_gap_and_realigns_the_context_revision() {
    let mut store = bound_store();
    store.apply(&event(
        1,
        ExecutionEventKind::ResultChunk {
            chunk_index: Counter::new(0),
            rows: 5,
        },
    ));
    store.apply(&event(
        2,
        ExecutionEventKind::StateChanged {
            state: ExecutionState::Running,
        },
    ));
    store.apply(&event(
        9,
        ExecutionEventKind::Terminal {
            state: ExecutionState::Succeeded,
        },
    ));
    assert!(store.needs_recovery());

    let view = crate::gateway::testing_support::ready_view(
        DbSessionId::new("dbse_a"),
        Counter::new(5),
        Counter::new(42),
    );
    store.recover_from_snapshot(&view);

    assert!(!store.needs_recovery());
    assert_eq!(store.context_revision(), Counter::new(42));
    // 快照只补缺口：缺口之前积累的事实必须原样留下（这条断言曾经写的是
    // `Queued`——那正是重建水位会把已推进的状态机倒回去）。
    assert_eq!(store.execution_state(), ExecutionState::Running);
    assert_eq!(store.last_sequence().map(Counter::get), Some(2));
    assert_eq!(store.row_count(), 5);
    // 缺口之前已应用过的序号在恢复后仍然「见过」，重投必须被继续识别为重复。
    assert_eq!(
        store.apply(&event(
            2,
            ExecutionEventKind::ResultChunk {
                chunk_index: Counter::new(1),
                rows: 9,
            },
        )),
        EventDisposition::IgnoredDuplicate {
            sequence: Counter::new(2)
        }
    );
    assert_eq!(store.row_count(), 5);
}

#[test]
fn a_stale_snapshot_never_lowers_the_observed_context_revision() {
    let mut store = bound_store();
    store.apply(&event(
        30,
        ExecutionEventKind::ContextObserved {
            confidence: ContextConfidence::Confirmed,
        },
    ));
    assert_eq!(store.context_revision(), Counter::new(30));

    store.recover_from_snapshot(&crate::gateway::testing_support::ready_view(
        DbSessionId::new("dbse_a"),
        Counter::new(5),
        Counter::new(11),
    ));
    assert_eq!(store.context_revision(), Counter::new(30));
}

#[test]
fn a_snapshot_keeps_a_terminal_state() {
    let mut store = bound_store();
    store.apply(&event(
        1,
        ExecutionEventKind::Terminal {
            state: ExecutionState::Succeeded,
        },
    ));
    store.recover_from_snapshot(&crate::gateway::testing_support::ready_view(
        DbSessionId::new("dbse_a"),
        Counter::new(5),
        Counter::new(42),
    ));

    assert_eq!(store.execution_state(), ExecutionState::Succeeded);
    assert_eq!(store.last_sequence().map(Counter::get), Some(1));
}

#[test]
fn a_snapshot_keeps_the_observed_row_count() {
    let mut store = bound_store();
    for (sequence, rows) in [(1_u64, 5_u64), (2, 7)] {
        store.apply(&event(
            sequence,
            ExecutionEventKind::ResultChunk {
                chunk_index: Counter::new(sequence),
                rows,
            },
        ));
    }
    store.recover_from_snapshot(&crate::gateway::testing_support::ready_view(
        DbSessionId::new("dbse_a"),
        Counter::new(5),
        Counter::new(42),
    ));

    assert_eq!(store.row_count(), 12);
}

#[test]
fn a_snapshot_keeps_the_chunk_dedupe_set() {
    let mut store = bound_store();
    store.apply(&event(
        1,
        ExecutionEventKind::ResultChunk {
            chunk_index: Counter::new(0),
            rows: 5,
        },
    ));
    store.recover_from_snapshot(&crate::gateway::testing_support::ready_view(
        DbSessionId::new("dbse_a"),
        Counter::new(5),
        Counter::new(42),
    ));

    assert_eq!(
        store.apply(&event(
            2,
            ExecutionEventKind::ResultChunk {
                chunk_index: Counter::new(0),
                rows: 5,
            },
        )),
        EventDisposition::IgnoredDuplicateChunk {
            chunk_index: Counter::new(0)
        }
    );
    assert_eq!(store.row_count(), 5);
}

#[test]
fn a_snapshot_never_rebinds_the_store() {
    let mut store = bound_store();
    store.apply(&event(
        1,
        ExecutionEventKind::StateChanged {
            state: ExecutionState::Running,
        },
    ));
    store.recover_from_snapshot(&crate::gateway::testing_support::ready_view(
        DbSessionId::new("dbse_OTHER"),
        Counter::new(77),
        Counter::new(42),
    ));

    assert_eq!(
        store.bound_session().map(|id| id.as_str().to_owned()),
        Some("dbse_a".to_owned())
    );
    assert_eq!(store.bound_epoch(), Some(Counter::new(5)));
    // 绑定没被快照带走，于是别的会话 / 别的 epoch 的帧仍旧被识别出来。
    let mut foreign = event(
        2,
        ExecutionEventKind::Terminal {
            state: ExecutionState::Failed,
        },
    );
    foreign.db_session_id = DbSessionId::new("dbse_OTHER");
    assert_eq!(
        store.apply(&foreign),
        EventDisposition::IgnoredStaleSession {
            bound: DbSessionId::new("dbse_a")
        }
    );
}

#[test]
fn huge_counters_are_stored_exactly_without_wrapping() {
    let mut store = bound_store();
    let big = u64::MAX - 1;
    let mut e = event(
        big,
        ExecutionEventKind::ContextObserved {
            confidence: ContextConfidence::Confirmed,
        },
    );
    e.context_revision = Counter::new(u64::MAX);
    assert!(store.apply(&e).mutated_state());
    assert_eq!(store.last_sequence().map(Counter::get), Some(big));
    assert_eq!(store.context_revision().get(), u64::MAX);

    // 水位已到 u64::MAX-1，下一帧 u64::MAX 必须精确命中而不是溢出成 0。
    let mut next = event(
        u64::MAX,
        ExecutionEventKind::Terminal {
            state: ExecutionState::Succeeded,
        },
    );
    next.context_revision = Counter::new(u64::MAX);
    assert!(store.apply(&next).mutated_state());
    assert_eq!(store.last_sequence().map(Counter::get), Some(u64::MAX));
    assert_eq!(store.execution_state(), ExecutionState::Succeeded);
}

#[test]
fn an_out_of_order_frame_cannot_roll_back_the_context_revision() {
    let mut store = bound_store();
    store.apply(&event(
        5,
        ExecutionEventKind::ContextObserved {
            confidence: ContextConfidence::Confirmed,
        },
    ));
    assert_eq!(store.context_revision(), Counter::new(5));

    let mut late = event(
        2,
        ExecutionEventKind::ContextObserved {
            confidence: ContextConfidence::Unknown,
        },
    );
    late.context_revision = Counter::new(1);
    store.apply(&late);
    assert_eq!(
        store.context_revision(),
        Counter::new(5),
        "迟到的帧不得把已观测的修订号拉回去"
    );
}

#[test]
fn a_declared_source_on_an_event_is_counted_and_dropped() {
    let mut store = bound_store();
    let mut e = event(
        1,
        ExecutionEventKind::StateChanged {
            state: ExecutionState::Running,
        },
    );
    e.declared_source = Some(ExecutionSource::new(SourceKind::Job, "job-9", None, None));
    assert!(store.apply(&e).mutated_state());
    assert_eq!(store.declared_source_events_ignored(), 1);
}

#[test]
fn rebinding_the_same_binding_is_idempotent_and_any_rebind_is_refused() {
    let mut store = EventStore::new();
    store.bind(DbSessionId::new("dbse_a"), Counter::new(5));
    // 完全相同的绑定重复一次：幂等，不清空水位。
    store.bind(DbSessionId::new("dbse_a"), Counter::new(5));
    store.apply(&event(
        1,
        ExecutionEventKind::StateChanged {
            state: ExecutionState::Running,
        },
    ));
    assert_eq!(store.last_sequence(), Some(Counter::new(1)));

    // 第二个执行换了会话：网关必须另起实例，这里保持旧绑定。
    store.bind(DbSessionId::new("dbse_b"), Counter::new(5));
    assert_eq!(store.bound_session(), Some(&DbSessionId::new("dbse_a")));
    assert_eq!(store.execution_state(), ExecutionState::Running);

    // 同一会话的新 epoch：绑定不可改写，否则旧执行的帧会落到新水位上。
    store.bind(DbSessionId::new("dbse_a"), Counter::new(6));
    assert_eq!(store.bound_epoch(), Some(Counter::new(5)));
    let mut from_new_epoch = event(
        2,
        ExecutionEventKind::StateChanged {
            state: ExecutionState::Succeeded,
        },
    );
    from_new_epoch.runtime_epoch = Counter::new(6);
    assert!(matches!(
        store.apply(&from_new_epoch),
        EventDisposition::IgnoredForeignEpoch { bound }
            if bound == Counter::new(5)
    ));
    assert_eq!(store.execution_state(), ExecutionState::Running);
    assert_eq!(store.last_sequence(), Some(Counter::new(1)));
}

#[test]
fn a_fresh_subscription_accepts_any_first_sequence() {
    let mut store = bound_store();
    assert_eq!(store.last_sequence(), None);
    assert!(store
        .apply(&event(
            1_000_000,
            ExecutionEventKind::StateChanged {
                state: ExecutionState::Running
            }
        ))
        .mutated_state());
    assert_eq!(store.last_sequence().map(Counter::get), Some(1_000_000));
}
