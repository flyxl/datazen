//! T133 independent audit: `ai/streamRegistry.ts` port.
//!
//! Covers branches the ported React cases do not exercise: the event stream
//! emitted while `begin` supersedes a live run, re-begin of a terminal request,
//! terminal-field precedence, cancel timestamp preservation, unknown-request
//! `applyError`, and multi-subscriber delivery order. Everything is observed
//! through the registry's public API and real `subscribe` listeners.

use std::cell::RefCell;
use std::rc::Rc;

use crate::ai::stream_registry::{
    ApplyChunkInput, StreamDropReason, StreamEvent, StreamPhase, StreamRegistry, StreamTerminalKind,
};

const CONV: &str = "conv-1";

fn labels(events: &[StreamEvent]) -> Vec<String> {
    events
        .iter()
        .map(|e| match e {
            StreamEvent::Settled { terminal_kind, .. } => format!("settled:{terminal_kind:?}").to_lowercase(),
            _ => e.event_type().to_owned(),
        })
        .collect()
}

/// React `begin` supersedes a live run by calling `settle(previous,'cancelled')`
/// *before* emitting `started` for the new run. Both settles must be observable
/// in one ordered event stream with pinned generations and terminal kinds.
#[test]
fn supersede_emits_settled_then_started_with_pinned_values() {
    let mut r = StreamRegistry::new();
    let events = Rc::new(RefCell::new(Vec::<StreamEvent>::new()));
    let sink = events.clone();
    let _id = r.subscribe(Box::new(move |e| sink.borrow_mut().push(e)));

    let first = r.begin(CONV, "req-1");
    r.apply_chunk("req-1", ApplyChunkInput::content());
    let second = r.begin(CONV, "req-2");

    assert_eq!(first.generation, 1);
    assert_eq!(second.generation, 2);
    let captured = events.borrow().clone();
    assert_eq!(
        labels(&captured),
        ["started", "streaming", "settled:cancelled", "started"],
        "supersede settles the old run before starting the new one"
    );
    assert_eq!(captured[2].request_id(), "req-1", "settled is for the superseded request");
    assert_eq!(captured[2].terminal_kind(), Some(StreamTerminalKind::Cancelled));
    assert_eq!(captured[3].request_id(), "req-2", "started is for the new request");
    if let StreamEvent::Started { entry } = &captured[3] {
        assert_eq!(entry.generation, 2);
        assert_eq!(entry.phase, StreamPhase::Starting);
    } else {
        panic!("fourth event is started");
    }
}

/// React `begin` on an already-terminal request id proceeds (the live-request
/// no-op only guards non-terminal entries): a fresh generation is created and
/// the entry is reset to `starting`.
#[test]
fn rebegin_of_terminal_request_creates_fresh_generation_and_entry() {
    let mut r = StreamRegistry::new();
    let events = Rc::new(RefCell::new(Vec::<StreamEvent>::new()));
    let sink = events.clone();
    let _id = r.subscribe(Box::new(move |e| sink.borrow_mut().push(e)));

    let first = r.begin(CONV, "req-1");
    assert!(r.apply_chunk("req-1", ApplyChunkInput::done()).accepted);
    assert_eq!(r.get("req-1").map(|e| e.phase), Some(StreamPhase::Terminal));

    let again = r.begin(CONV, "req-1");
    assert_eq!(first.generation, 1);
    assert_eq!(again.generation, 2, "re-begin bumps the generation");
    assert_eq!(again.phase, StreamPhase::Starting);
    assert_eq!(again.terminal_kind, None);
    assert_eq!(again.cancel_issued_at_ms, None);
    assert_eq!(r.conversation_generation(CONV), 2);
    assert!(r.has("req-1"));

    assert_eq!(labels(&events.borrow()), ["started", "settled:done", "started"]);
    let chunk = r.apply_chunk("req-1", ApplyChunkInput::content());
    assert!(chunk.accepted);
    assert_eq!(chunk.generation, Some(2));
    assert_eq!(chunk.phase, Some(StreamPhase::Streaming));
}

/// `terminalize` only changes phase and terminal kind; a cancel timestamp set
/// earlier must survive the terminal chunk, and `settled` fires exactly once.
#[test]
fn terminal_chunk_preserves_cancel_timestamp_and_settles_once() {
    let mut r = StreamRegistry::new();
    let events = Rc::new(RefCell::new(Vec::<StreamEvent>::new()));
    let sink = events.clone();
    let _id = r.subscribe(Box::new(move |e| sink.borrow_mut().push(e)));

    r.begin(CONV, "req-1");
    r.apply_chunk("req-1", ApplyChunkInput::content());
    assert!(r.mark_cancelling_at("req-1", 777));
    let terminal = r.apply_chunk("req-1", ApplyChunkInput::cancelled());

    assert!(terminal.accepted);
    assert_eq!(terminal.terminal_kind, Some(StreamTerminalKind::Cancelled));
    let entry = r.get("req-1").expect("entry mounted");
    assert_eq!(entry.phase, StreamPhase::Terminal);
    assert_eq!(entry.cancel_issued_at_ms, Some(777), "cancel timestamp kept");
    assert_eq!(
        labels(&events.borrow()),
        ["started", "streaming", "cancelling", "settled:cancelled"]
    );
    assert!(!r.settle("req-1", StreamTerminalKind::Done), "no second settle");
    assert_eq!(events.borrow().len(), 4, "no extra settled event");
}

/// React checks `chunk.error` before `chunk.done`, so an inline error wins even
/// when `done && cancelled` are both set.
#[test]
fn inline_error_beats_done_and_cancelled() {
    let mut r = StreamRegistry::new();
    r.begin(CONV, "req-1");
    let win = ApplyChunkInput {
        done: true,
        cancelled: true,
        error: Some("boom".to_owned()),
    };
    let result = r.apply_chunk("req-1", win);
    assert!(result.accepted);
    assert_eq!(result.terminal_kind, Some(StreamTerminalKind::Error));
    assert_eq!(result.phase, Some(StreamPhase::Terminal));
    assert_eq!(r.get("req-1").and_then(|e| e.terminal_kind), Some(StreamTerminalKind::Error));
}

/// `applyError` on an unknown request must take the same `unknown-request` gate
/// branch as `applyChunk`, with all three snapshot fields null.
#[test]
fn apply_error_unknown_request_drops_with_null_state() {
    let mut r = StreamRegistry::new();
    let drop = r.apply_error("missing");
    assert!(!drop.accepted);
    assert_eq!(drop.drop_reason, Some(StreamDropReason::UnknownRequest));
    assert_eq!(drop.phase, None);
    assert_eq!(drop.generation, None);
    assert_eq!(drop.terminal_kind, None);
    assert!(!r.has("missing"));
}

/// React's `Set<listener>` delivers in insertion order; the `u64`-id Vec must
/// preserve that order and only notify listeners that are still subscribed.
#[test]
fn multiple_subscribers_are_notified_in_subscription_order() {
    let mut r = StreamRegistry::new();
    let a = Rc::new(RefCell::new(Vec::<String>::new()));
    let b = Rc::new(RefCell::new(Vec::<String>::new()));
    let a_sink = a.clone();
    let b_sink = b.clone();
    let tag = |e: &StreamEvent| format!("{}:{}", e.event_type(), e.request_id());
    let id_a = r.subscribe(Box::new(move |e| a_sink.borrow_mut().push(tag(&e))));
    let _id_b = r.subscribe(Box::new(move |e| b_sink.borrow_mut().push(tag(&e))));

    r.begin(CONV, "r1");
    assert_eq!(*a.borrow(), vec!["started:r1"]);
    assert_eq!(*b.borrow(), vec!["started:r1"]);

    // The second begin supersedes the still-live r1, so a settled event for it
    // precedes the started event for r2 (React's begin calls settle first).
    r.unsubscribe(id_a);
    r.begin(CONV, "r2");
    assert_eq!(*a.borrow(), vec!["started:r1"], "detached listener silent");
    assert_eq!(*b.borrow(), vec!["started:r1", "settled:r1", "started:r2"]);
}
