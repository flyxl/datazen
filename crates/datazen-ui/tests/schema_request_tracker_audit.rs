//! T421 independent audit (tester): fresh vectors for the request
//! tracker — interleaved sessions, slot churn across invalidates, the
//! bind-after-replace chain, and reset-vs-invalidate isolation.

use datazen_ui::schema_request_tracker::SchemaRequestTracker;

#[test]
fn audit_many_sessions_in_parallel() {
    let mut tracker = SchemaRequestTracker::default();
    let barriers: Vec<_> = (0..5)
        .map(|i| tracker.begin(&format!("s{i}"), "columns"))
        .collect();
    for barrier in &barriers {
        assert!(tracker.slot_alive(barrier));
    }
    tracker.invalidate("s2");
    assert!(!tracker.slot_alive(&barriers[2]));
    for (index, barrier) in barriers.iter().enumerate() {
        let alive = tracker.slot_alive(barrier);
        assert_eq!(alive, index != 2, "only s2 went");
    }
}

#[test]
fn audit_the_bind_chain_after_replacement() {
    let mut tracker = SchemaRequestTracker::default();
    let v1 = tracker.begin("s", "slot");
    let b1 = tracker.capture_slot("s", "slot");
    let v2 = tracker.begin("s", "slot"); // replaces
    let b2 = tracker.capture_slot("s", "slot"); // binds v2
    assert!(!tracker.slot_alive(&v1));
    assert!(!tracker.slot_alive(&b1), "the bind on the replaced request is stale");
    assert!(tracker.slot_alive(&v2));
    assert!(tracker.slot_alive(&b2));
    // A third replacement retires everything before it.
    let v3 = tracker.begin("s", "slot");
    assert!(!tracker.slot_alive(&v2));
    assert!(!tracker.slot_alive(&b2));
    assert!(tracker.slot_alive(&v3));
}

#[test]
fn audit_a_slot_barrier_spans_sessions_safely() {
    let mut tracker = SchemaRequestTracker::default();
    let a = tracker.begin("s1", "slot");
    // A different session's begin on the same slot name does not touch
    // the first.
    let _b = tracker.begin("s2", "slot");
    assert!(tracker.slot_alive(&a));
    // And invalidate of the second does not retire the first.
    tracker.invalidate("s2");
    assert!(tracker.slot_alive(&a));
}

#[test]
fn audit_reset_and_invalidate_are_both_total_for_their_scope() {
    let mut tracker = SchemaRequestTracker::default();
    let keep = tracker.begin("keep", "slot");
    tracker.invalidate("drop");
    assert!(tracker.slot_alive(&keep), "invalidate is scoped to one session");
    tracker.reset();
    assert!(!tracker.slot_alive(&keep), "reset is total");
}

#[test]
fn audit_capture_on_a_live_session_is_stable_across_calls() {
    let mut tracker = SchemaRequestTracker::default();
    let first = tracker.capture("s");
    // Any number of captures, begins and binds on the same session keep
    // the session identity stable.
    let _request = tracker.begin("s", "slot");
    let _bind = tracker.capture_slot("s", "slot");
    let second = tracker.capture("s");
    assert_eq!(first, second, "the session identity never mints again while alive");
    assert!(tracker.session_alive(&first));
}

#[test]
fn audit_an_empty_slot_begin_binds_like_begin() {
    let mut tracker = SchemaRequestTracker::default();
    let began = tracker.capture_slot("fresh", "empty");
    let direct = tracker.begin("fresh", "empty");
    // The begin replaced the capture_slot's request.
    assert!(!tracker.slot_alive(&began));
    assert!(tracker.slot_alive(&direct));
    let bound = tracker.capture_slot("fresh", "empty");
    assert_eq!(bound, direct, "the bind is the standing request");
}
