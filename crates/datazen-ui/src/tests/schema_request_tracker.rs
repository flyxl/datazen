//! T421: the schema request tracker — session barriers survive while the
//! session lives, slot barriers go stale when a new request replaces
//! them, capture_slot binds the standing request, and invalidate/reset
//! retire every barrier.

use crate::schema_request_tracker::SchemaRequestTracker;

#[test]
fn a_session_barrier_lives_until_invalidate() {
    let mut tracker = SchemaRequestTracker::default();
    let barrier = tracker.capture("s1");
    assert!(tracker.session_alive(&barrier));
    // A second capture of the same session agrees (the same slot map).
    let again = tracker.capture("s1");
    assert!(tracker.session_alive(&again));
    // Another session's capture does not disturb it.
    let _other = tracker.capture("s2");
    assert!(tracker.session_alive(&barrier));
    // Invalidate retires every barrier of that session.
    tracker.invalidate("s1");
    assert!(!tracker.session_alive(&barrier));
    assert!(!tracker.session_alive(&again));
    assert!(
        tracker.session_alive(&_other),
        "the other session's barrier is untouched"
    );
}

#[test]
fn a_new_generation_after_invalidate_is_a_new_session() {
    let mut tracker = SchemaRequestTracker::default();
    let first = tracker.capture("s1");
    tracker.invalidate("s1");
    let second = tracker.capture("s1");
    assert!(!tracker.session_alive(&first), "the old barrier is stale");
    assert!(tracker.session_alive(&second), "the new generation is live");
    assert_ne!(first, second, "the barriers are distinct identities");
}

#[test]
fn begin_replaces_the_slot_request_and_retires_the_old_barrier() {
    let mut tracker = SchemaRequestTracker::default();
    let old = tracker.begin("s1", "columns");
    assert!(tracker.slot_alive(&old));
    let new = tracker.begin("s1", "columns");
    assert!(!tracker.slot_alive(&old), "a newer request retires the old one");
    assert!(tracker.slot_alive(&new));
    // A different slot is independent.
    let other = tracker.begin("s1", "indexes");
    assert!(tracker.slot_alive(&new), "the columns slot is untouched");
    assert!(tracker.slot_alive(&other));
}

#[test]
fn capture_slot_binds_the_standing_request() {
    let mut tracker = SchemaRequestTracker::default();
    let first = tracker.begin("s1", "columns");
    // A second caller binds the same request: both barriers stay live.
    let bound = tracker.capture_slot("s1", "columns");
    assert!(tracker.slot_alive(&first));
    assert!(tracker.slot_alive(&bound));
    // A new request retires both.
    let replaced = tracker.begin("s1", "columns");
    assert!(!tracker.slot_alive(&first));
    assert!(!tracker.slot_alive(&bound));
    assert!(tracker.slot_alive(&replaced));
}

#[test]
fn capture_slot_on_an_empty_slot_begins_one() {
    let mut tracker = SchemaRequestTracker::default();
    let began = tracker.capture_slot("s1", "columns");
    assert!(tracker.slot_alive(&began), "the empty slot began a request");
    // And a later capture binds to it.
    let bound = tracker.capture_slot("s1", "columns");
    assert!(tracker.slot_alive(&bound));
    assert_eq!(bound, began, "the bind sees the same request identity");
}

#[test]
fn invalidate_retires_the_slot_barriers_too() {
    let mut tracker = SchemaRequestTracker::default();
    let barrier = tracker.begin("s1", "columns");
    tracker.invalidate("s1");
    assert!(!tracker.slot_alive(&barrier), "the session went, the slot with it");
    // A fresh session with the same id mints fresh slots.
    let fresh = tracker.capture_slot("s1", "columns");
    assert!(tracker.slot_alive(&fresh));
    assert_ne!(fresh, barrier);
}

#[test]
fn reset_retires_everything() {
    let mut tracker = SchemaRequestTracker::default();
    let a = tracker.begin("s1", "columns");
    let b = tracker.begin("s2", "indexes");
    let session_only = tracker.capture("s3");
    tracker.reset();
    assert!(!tracker.slot_alive(&a));
    assert!(!tracker.slot_alive(&b));
    assert!(!tracker.session_alive(&session_only));
    // Everything works again after the reset.
    let fresh = tracker.begin("s1", "columns");
    assert!(tracker.slot_alive(&fresh));
}

#[test]
fn two_sessions_do_not_share_slots() {
    let mut tracker = SchemaRequestTracker::default();
    let a = tracker.begin("s1", "columns");
    let b = tracker.begin("s2", "columns");
    assert!(tracker.slot_alive(&a));
    assert!(tracker.slot_alive(&b));
    tracker.invalidate("s1");
    assert!(!tracker.slot_alive(&a));
    assert!(tracker.slot_alive(&b), "the other session's slot survives");
}
