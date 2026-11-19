//! T133: `ai/streamRegistry.ts` — the 12 React unit cases plus the event-shape,
//! clock-injection and no-pruning invariants the TS source documents.
//!
//! All state is observed through the registry's public `get` / `gate` results
//! and the events captured by real `subscribe` listeners. Nothing here reads
//! the implementation's private fields.

use std::cell::RefCell;
use std::rc::Rc;

use crate::ai::stream_registry::{
    ApplyChunkInput, ApplyChunkResult, StreamDropReason, StreamEvent, StreamPhase, StreamRegistry,
    StreamTerminalKind,
};

const CONV: &str = "conv-1";

fn registry() -> StreamRegistry {
    StreamRegistry::new()
}

fn result(accepted: bool, reason: Option<StreamDropReason>, phase: Option<StreamPhase>, generation: Option<u64>, terminal_kind: Option<StreamTerminalKind>) -> ApplyChunkResult {
    ApplyChunkResult {
        accepted,
        drop_reason: reason,
        phase,
        generation,
        terminal_kind,
    }
}

// ---- React lifecycle cases -------------------------------------------------------------------

#[test]
fn walks_starting_streaming_terminal_done_step_by_step() {
    let mut r = registry();
    let started = r.begin(CONV, "req-1");
    assert_eq!(started.phase, StreamPhase::Starting);
    assert_eq!(started.generation, 1);
    assert_eq!(started.terminal_kind, None);
    assert_eq!(started.cancel_issued_at_ms, None);

    // First non-terminal chunk enters streaming.
    assert_eq!(
        r.apply_chunk("req-1", ApplyChunkInput::content()),
        result(true, None, Some(StreamPhase::Streaming), Some(1), None)
    );
    assert_eq!(r.get("req-1").map(|e| e.phase), Some(StreamPhase::Streaming));

    // Subsequent chunks stay streaming.
    assert_eq!(
        r.apply_chunk("req-1", ApplyChunkInput::content()).phase,
        Some(StreamPhase::Streaming)
    );

    // Terminal chunk leaves the machine exactly once.
    assert_eq!(
        r.apply_chunk("req-1", ApplyChunkInput::done()),
        result(true, None, Some(StreamPhase::Terminal), Some(1), Some(StreamTerminalKind::Done))
    );
    assert_eq!(r.get("req-1").map(|e| e.phase), Some(StreamPhase::Terminal));
}

#[test]
fn terminal_idempotent_second_terminal_chunk_is_dropped() {
    let mut r = registry();
    r.begin(CONV, "req-1");
    assert!(r.apply_chunk("req-1", ApplyChunkInput::done()).accepted);

    let again = r.apply_chunk("req-1", ApplyChunkInput::done());
    assert_eq!(again.drop_reason, Some(StreamDropReason::Terminal));
    assert_eq!(
        again,
        result(false, Some(StreamDropReason::Terminal), Some(StreamPhase::Terminal), Some(1), Some(StreamTerminalKind::Done))
    );
    assert_eq!(r.get("req-1").and_then(|e| e.terminal_kind), Some(StreamTerminalKind::Done));
}

#[test]
fn done_and_cancelled_is_terminal_cancelled_not_done() {
    let mut r = registry();
    r.begin(CONV, "req-1");
    r.apply_chunk("req-1", ApplyChunkInput::content());

    let terminal = r.apply_chunk("req-1", ApplyChunkInput::cancelled());
    assert!(terminal.accepted);
    assert_eq!(terminal.phase, Some(StreamPhase::Terminal));
    assert_eq!(terminal.terminal_kind, Some(StreamTerminalKind::Cancelled));
}

#[test]
fn late_chunks_after_generation_change_are_dropped_as_stale() {
    let mut r = registry();
    r.begin(CONV, "req-1");
    r.apply_chunk("req-1", ApplyChunkInput::content());

    let second = r.begin(CONV, "req-2");
    assert_eq!(second.generation, 2);
    assert_eq!(r.conversation_generation(CONV), 2);
    assert_eq!(r.get("req-1").map(|e| e.phase), Some(StreamPhase::Terminal));
    assert_eq!(r.get("req-1").and_then(|e| e.terminal_kind), Some(StreamTerminalKind::Cancelled));

    let late = r.apply_chunk("req-1", ApplyChunkInput::content());
    assert!(!late.accepted);
    assert_eq!(late.drop_reason, Some(StreamDropReason::StaleGeneration));
    // A stale drop keeps the old entry's visible state (React's gate shape).
    assert_eq!(late.phase, Some(StreamPhase::Terminal));
    assert_eq!(late.generation, Some(1));
    assert_eq!(late.terminal_kind, Some(StreamTerminalKind::Cancelled));

    // The new run is unaffected and starts in its own starting phase.
    assert!(r.apply_chunk("req-2", ApplyChunkInput::content()).accepted);
    assert_eq!(r.get("req-2").map(|e| e.phase), Some(StreamPhase::Streaming));
}

#[test]
fn generations_are_monotonic_and_independent_per_conversation() {
    let mut r = registry();
    r.begin(CONV, "r1");
    r.begin(CONV, "r2");
    r.begin(CONV, "r3");
    assert_eq!(r.conversation_generation(CONV), 3);
    assert_eq!(r.get("r3").map(|e| e.generation), Some(3));

    r.begin("conv-2", "other");
    assert_eq!(r.conversation_generation("conv-2"), 1);
    assert_eq!(r.conversation_generation(CONV), 3);
    assert_eq!(r.get("other").map(|e| e.generation), Some(1));
}

#[test]
fn cancel_drops_late_content_but_accepts_the_terminal_chunk() {
    let mut r = registry();
    r.begin(CONV, "req-1");
    r.apply_chunk("req-1", ApplyChunkInput::content());

    assert!(r.mark_cancelling_at("req-1", 1234));
    let cancelling = r.get("req-1").unwrap_or_else(|| panic!("entry mounted"));
    assert_eq!(cancelling.phase, StreamPhase::Cancelling);
    assert_eq!(cancelling.cancel_issued_at_ms, Some(1234));

    let late = r.apply_chunk("req-1", ApplyChunkInput::content());
    assert!(!late.accepted);
    assert_eq!(late.drop_reason, Some(StreamDropReason::CancelIssued));
    assert_eq!(late.phase, Some(StreamPhase::Cancelling));
    assert_eq!(r.get("req-1").map(|e| e.phase), Some(StreamPhase::Cancelling));

    let terminal = r.apply_chunk("req-1", ApplyChunkInput::cancelled());
    assert!(terminal.accepted);
    assert_eq!(terminal.terminal_kind, Some(StreamTerminalKind::Cancelled));

    // The fallback settle after a real terminal chunk is a no-op.
    assert!(!r.settle("req-1", StreamTerminalKind::Cancelled));
}

#[test]
fn normal_done_is_accepted_while_cancelling() {
    let mut r = registry();
    r.begin(CONV, "req-1");
    r.apply_chunk("req-1", ApplyChunkInput::content());
    r.mark_cancelling("req-1");
    assert_eq!(
        r.apply_chunk("req-1", ApplyChunkInput::done()).terminal_kind,
        Some(StreamTerminalKind::Done)
    );
    assert_eq!(r.get("req-1").and_then(|e| e.terminal_kind), Some(StreamTerminalKind::Done));
}

#[test]
fn out_of_band_error_settles_and_rejects_later_chunks() {
    let mut r = registry();
    r.begin(CONV, "req-1");
    r.apply_chunk("req-1", ApplyChunkInput::content());

    let error = r.apply_error("req-1");
    assert!(error.accepted);
    assert_eq!(error.terminal_kind, Some(StreamTerminalKind::Error));
    assert_eq!(r.apply_error("req-1").drop_reason, Some(StreamDropReason::Terminal));
    assert_eq!(
        r.apply_chunk("req-1", ApplyChunkInput::done()).drop_reason,
        Some(StreamDropReason::Terminal)
    );
}

#[test]
fn unknown_requests_are_dropped_with_null_state() {
    let mut r = registry();
    let dropped = r.apply_chunk("missing", ApplyChunkInput::done());
    assert!(!dropped.accepted);
    assert_eq!(dropped.drop_reason, Some(StreamDropReason::UnknownRequest));
    assert_eq!(dropped.phase, None);
    assert_eq!(dropped.generation, None);
    assert_eq!(dropped.terminal_kind, None);
    assert!(!r.has("missing"));
    assert!(!r.mark_cancelling("missing"));
    assert!(!r.settle("missing", StreamTerminalKind::Cancelled));
}

#[test]
fn force_settles_a_stuck_cancelling_run() {
    let mut r = registry();
    r.begin(CONV, "req-1");
    r.apply_chunk("req-1", ApplyChunkInput::content());
    r.mark_cancelling("req-1");
    assert_eq!(r.get("req-1").map(|e| e.phase), Some(StreamPhase::Cancelling));

    assert!(r.settle("req-1", StreamTerminalKind::Cancelled));
    assert_eq!(r.get("req-1").map(|e| e.phase), Some(StreamPhase::Terminal));
    assert_eq!(r.get("req-1").and_then(|e| e.terminal_kind), Some(StreamTerminalKind::Cancelled));
    assert_eq!(
        r.apply_chunk("req-1", ApplyChunkInput::cancelled()).drop_reason,
        Some(StreamDropReason::Terminal)
    );
}

#[test]
fn events_are_emitted_in_order_and_unsubscribe_stops_them() {
    let mut r = registry();
    let events = Rc::new(RefCell::new(Vec::<StreamEvent>::new()));
    let sink = events.clone();
    let id = r.subscribe(Box::new(move |event| sink.borrow_mut().push(event)));

    r.begin(CONV, "req-1");
    r.apply_chunk("req-1", ApplyChunkInput::content());
    r.mark_cancelling("req-1");
    r.apply_chunk("req-1", ApplyChunkInput::cancelled());

    let labels: Vec<String> = events
        .borrow()
        .iter()
        .map(|event| match event.terminal_kind() {
            Some(kind) => format!("settled:{kind:?}").to_lowercase(),
            None => event.event_type().to_owned(),
        })
        .collect();
    assert_eq!(labels, ["started", "streaming", "cancelling", "settled:cancelled"]);

    r.unsubscribe(id);
    r.begin(CONV, "req-2");
    assert_eq!(events.borrow().len(), 4, "no events after unsubscribe");
}

#[test]
fn repeated_begin_of_a_live_request_is_a_noop() {
    let mut r = registry();
    let first = r.begin(CONV, "req-1");
    r.apply_chunk("req-1", ApplyChunkInput::content());

    let events = Rc::new(RefCell::new(0usize));
    let counter = events.clone();
    let _id = r.subscribe(Box::new(move |_| *counter.borrow_mut() += 1));

    let duplicate = r.begin(CONV, "req-1");
    assert_eq!(duplicate.generation, first.generation);
    assert_eq!(duplicate.phase, StreamPhase::Streaming);
    assert_eq!(r.conversation_generation(CONV), 1);
    assert_eq!(*events.borrow(), 0, "duplicate begin emits nothing");
}

// ---- Additional invariants from the TS source / docs ------------------------------------------

#[test]
fn inline_error_chunk_settles_error_and_empty_error_is_falsy() {
    let mut r = registry();
    r.begin(CONV, "req-1");

    // React `if (chunk.error)` treats "" as falsy: the chunk stays non-terminal.
    let empty = r.apply_chunk("req-1", ApplyChunkInput::error(""));
    assert!(empty.accepted);
    assert_eq!(empty.phase, Some(StreamPhase::Streaming));

    let failed = r.apply_chunk("req-1", ApplyChunkInput::error("boom"));
    assert!(failed.accepted);
    assert_eq!(failed.terminal_kind, Some(StreamTerminalKind::Error));
    assert_eq!(r.get("req-1").map(|e| e.phase), Some(StreamPhase::Terminal));
}

#[test]
fn events_carry_the_full_entry_snapshot() {
    let mut r = registry();
    let events = Rc::new(RefCell::new(Vec::<StreamEvent>::new()));
    let sink = events.clone();
    let _id = r.subscribe(Box::new(move |event| sink.borrow_mut().push(event)));

    r.begin(CONV, "req-1");
    r.apply_chunk("req-1", ApplyChunkInput::content());
    r.mark_cancelling_at("req-1", 99);
    r.apply_chunk("req-1", ApplyChunkInput::cancelled());

    let captured = events.borrow();
    assert_eq!(captured.len(), 4);
    for event in captured.iter() {
        assert_eq!(event.request_id(), "req-1");
        match event {
            StreamEvent::Started { entry } => {
                assert_eq!(entry.phase, StreamPhase::Starting);
                assert_eq!(entry.conversation_id, CONV);
                assert_eq!(entry.generation, 1);
                assert_eq!(entry.cancel_issued_at_ms, None);
            }
            StreamEvent::Streaming { entry } => assert_eq!(entry.phase, StreamPhase::Streaming),
            StreamEvent::Cancelling { entry } => {
                assert_eq!(entry.phase, StreamPhase::Cancelling);
                assert_eq!(entry.cancel_issued_at_ms, Some(99));
            }
            StreamEvent::Settled { entry, terminal_kind } => {
                assert_eq!(entry.phase, StreamPhase::Terminal);
                assert_eq!(*terminal_kind, StreamTerminalKind::Cancelled);
                assert_eq!(entry.terminal_kind, Some(StreamTerminalKind::Cancelled));
            }
        }
    }
}

#[test]
fn there_is_no_pruning_or_cap_on_entries() {
    let mut r = registry();
    r.begin(CONV, "keep-me");
    for i in 0..64 {
        r.begin(CONV, &format!("run-{i}"));
    }
    // React never deletes entries or generations: the first one is still there
    // and the generation series kept climbing.
    assert!(r.has("keep-me"));
    assert_eq!(r.get("keep-me").map(|e| e.generation), Some(1));
    assert_eq!(r.conversation_generation(CONV), 65);
}

#[test]
fn mark_cancelling_uses_the_injected_clock_and_generation_gate() {
    let mut r = StreamRegistry::with_now(|| 4242);
    r.begin(CONV, "req-1");
    assert!(r.mark_cancelling("req-1"));
    assert_eq!(r.get("req-1").and_then(|e| e.cancel_issued_at_ms), Some(4242));

    // Once the generation moves on, the old run cannot be marked any more.
    r.begin(CONV, "req-2");
    assert!(!r.mark_cancelling("req-1"));
    // The first mark emits; repeating on the current cancelling run is still
    // true (React's early return) but emits no second event.
    assert!(r.mark_cancelling("req-2"));
    let events = Rc::new(RefCell::new(0usize));
    let counter = events.clone();
    let _id = r.subscribe(Box::new(move |_| *counter.borrow_mut() += 1));
    assert!(r.mark_cancelling("req-2"));
    assert_eq!(*events.borrow(), 0);
}

#[test]
fn drop_reason_strings_match_the_react_union() {
    assert_eq!(StreamDropReason::UnknownRequest.as_str(), "unknown-request");
    assert_eq!(StreamDropReason::StaleGeneration.as_str(), "stale-generation");
    assert_eq!(StreamDropReason::CancelIssued.as_str(), "cancel-issued");
    assert_eq!(StreamDropReason::Terminal.as_str(), "terminal");
}
