//! T049 independent audit — coverage the coder's branch suite does not pin.
//!
//! Focus: the receipt-id race guard under *re-grant* (not just revoke), exact
//! ttl / touch-delay boundary values, expiry winning over an in-flight touch,
//! and observer/dispose semantics. Every case drives the machine only through
//! its public API; nothing here mutates product source.

use std::cell::RefCell;
use std::rc::Rc;

use crate::ai::grant_state::*;

fn receipt(id: &str, expires_at_ms: i64) -> AiGrantReceipt {
    AiGrantReceipt {
        grant_id: id.to_owned(),
        connection_id: "conn-audit".to_owned(),
        database: None,
        config_revision: 1,
        expires_at_ms,
    }
}

fn input() -> GrantInput {
    GrantInput {
        connection_id: "conn-audit".to_owned(),
        database: None,
        editor_session_id: "editor-audit".to_owned(),
    }
}

fn machine(now_ms: i64) -> (GrantState, Rc<RefCell<i64>>) {
    machine_with_ratio(now_ms, 0.5)
}

fn machine_with_ratio(now_ms: i64, ratio: f64) -> (GrantState, Rc<RefCell<i64>>) {
    let clock = Rc::new(RefCell::new(now_ms));
    let reader = clock.clone();
    let state = GrantState::new(GrantConfig {
        touch_interval_ratio: ratio,
        now: Box::new(move || *reader.borrow()),
    });
    (state, clock)
}

/// Drive independent -> granted with `g-1`, expires at now+2000.
fn grant_g1(state: &mut GrantState) {
    assert_eq!(
        state.grant(input()).effects,
        vec![GrantEffect::CallGrant {
            input: input(),
            token: 1
        }]
    );
    assert_eq!(
        state.on_grant_result(1, Ok(receipt("g-1", 3_000))),
        vec![
            GrantEffect::SetTouchTimer {
                handle: 1,
                delay_ms: 1_000
            },
            GrantEffect::SetExpiryTimer {
                handle: 2,
                delay_ms: 2_000
            },
        ]
    );
}

// --- Receipt-id guard under re-grant ----------------------------------------

#[test]
fn a_stale_touch_success_after_regrant_cannot_mutate_the_new_grant() {
    let (mut state, _) = machine(1_000);
    grant_g1(&mut state); // g-1, timers 1/2
    state.fire_touch_timer(1); // CallTouch(g-1) now in flight
    state.revoke(); // token 2, release g-1, cancel timer 2
    assert_eq!(
        state.grant(input()).effects,
        vec![GrantEffect::CallGrant {
            input: input(),
            token: 3
        }]
    );
    assert_eq!(
        state.on_grant_result(3, Ok(receipt("g-2", 3_000))),
        vec![
            GrantEffect::SetTouchTimer {
                handle: 3,
                delay_ms: 1_000
            },
            GrantEffect::SetExpiryTimer {
                handle: 4,
                delay_ms: 2_000
            },
        ]
    );
    let before = state.snapshot().clone();
    assert_eq!(before.receipt.as_ref().map(|r| r.grant_id.as_str()), Some("g-2"));

    // A late result of g-1's touch carries the OLD grant id. The receipt-id
    // guard must reject it: no new timers and no deadline mutation on g-2.
    let effects = state.on_touch_result("g-1", Ok(true));
    assert!(effects.is_empty(), "stale touch re-armed timers: {:?}", effects);
    assert_eq!(state.snapshot(), &before);
}

#[test]
fn a_stale_touch_failure_after_regrant_cannot_revoke_the_new_grant() {
    let (mut state, _) = machine(1_000);
    grant_g1(&mut state);
    state.fire_touch_timer(1);
    state.revoke();
    state.grant(input());
    state.on_grant_result(3, Ok(receipt("g-2", 3_000)));
    let before = state.snapshot().clone();

    let effects = state.on_touch_result("g-1", Err("late touch error".to_owned()));
    assert!(effects.is_empty(), "stale touch failure mutated state: {:?}", effects);
    assert_eq!(state.snapshot(), &before);
    assert_eq!(state.snapshot().phase, GrantPhase::Granted);
}

#[test]
fn an_old_touch_timer_handle_is_ignored_after_regrant() {
    let (mut state, _) = machine(1_000);
    grant_g1(&mut state);
    state.revoke();
    state.grant(input());
    state.on_grant_result(3, Ok(receipt("g-2", 3_000)));
    // The g-1 handles were cancelled; a queued fire of handle 1 must be a noop.
    assert!(state.fire_touch_timer(1).is_empty());
    assert!(state.fire_expiry_timer(2).is_empty());
    assert_eq!(state.snapshot().receipt.as_ref().map(|r| r.grant_id.as_str()), Some("g-2"));
    // g-2's live timer still fires.
    assert_eq!(
        state.fire_touch_timer(3),
        vec![GrantEffect::CallTouch {
            grant_id: "g-2".to_owned()
        }]
    );
}

#[test]
fn a_stale_grant_success_after_regrant_releases_orphan_and_keeps_the_new_grant() {
    let (mut state, _) = machine(1_000);
    // token 1 in flight
    assert_eq!(
        state.grant(input()).effects,
        vec![GrantEffect::CallGrant {
            input: input(),
            token: 1
        }]
    );
    state.revoke(); // token 2
    state.grant(input()); // token 3
    state.on_grant_result(3, Ok(receipt("g-current", 3_000)));
    let before = state.snapshot().clone();

    // token-1 response arrives late: must release g-orphan, not touch g-current.
    let effects = state.on_grant_result(1, Ok(receipt("g-orphan", 9_000)));
    assert_eq!(
        effects,
        vec![GrantEffect::CallRevoke {
            grant_id: "g-orphan".to_owned()
        }]
    );
    assert_eq!(state.snapshot(), &before);
}

// --- Expiry vs in-flight touch -----------------------------------------------

#[test]
fn local_expiry_during_an_in_flight_touch_wins_over_a_late_success() {
    let (mut state, clock) = machine(1_000);
    grant_g1(&mut state);
    state.fire_touch_timer(1); // touch in flight, expiry timer 2 still armed
    *clock.borrow_mut() = 3_000; // the local deadline passes
    // fire_expiry_timer clears its own handle; expire_locally then cancels the
    // (already nulled) touch handle — no stray ClearTimer is emitted.
    let expired = state.fire_expiry_timer(2);
    assert!(expired.is_empty());
    assert_eq!(state.snapshot().phase, GrantPhase::Independent);
    assert!(state.snapshot().receipt.is_none());

    // A late success of the in-flight touch must NOT resurrect the grant.
    let effects = state.on_touch_result("g-1", Ok(true));
    assert!(effects.is_empty(), "late touch resurrected: {:?}", effects);
    assert_eq!(state.snapshot().phase, GrantPhase::Independent);
    assert!(!state.is_granted());
}

#[test]
fn local_expiry_cancels_the_touch_timer_so_a_queued_fire_is_ignored() {
    let (mut state, _) = machine(1_000);
    grant_g1(&mut state);
    let effects = state.fire_expiry_timer(2);
    assert_eq!(effects, vec![GrantEffect::ClearTimer { handle: 1 }]);
    assert!(state.fire_touch_timer(1).is_empty());
    assert_eq!(state.snapshot().phase, GrantPhase::Independent);
}

// --- Exact timing boundaries --------------------------------------------------

#[test]
fn ttl_of_exactly_zero_at_arrival_expires_immediately_with_no_timers() {
    let (mut state, _) = machine(1_000);
    state.grant(input());
    // expires_at_ms == now -> ttl clamps to 0 -> remaining 0 -> expireLocally.
    let effects = state.on_grant_result(1, Ok(receipt("g-zero", 1_000)));
    assert!(effects.is_empty());
    assert_eq!(state.snapshot().phase, GrantPhase::Independent);
    assert!(!state.is_granted());
}

#[test]
fn ttl_of_one_millisecond_sets_both_timers_to_one() {
    let (mut state, _) = machine(1_000);
    state.grant(input());
    // remaining = 1; floor(1 * 0.5) = 0 -> clamped to 1; expiry = 1.
    let effects = state.on_grant_result(1, Ok(receipt("g-1ms", 1_001)));
    assert_eq!(
        effects,
        vec![
            GrantEffect::SetTouchTimer {
                handle: 1,
                delay_ms: 1
            },
            GrantEffect::SetExpiryTimer {
                handle: 2,
                delay_ms: 1
            },
        ]
    );
    assert!(state.is_granted());
}

#[test]
fn a_zero_ratio_still_clamps_the_touch_delay_to_one_millisecond() {
    let (mut state, _) = machine_with_ratio(1_000, 0.0);
    state.grant(input());
    // remaining = 2000; floor(2000 * 0) = 0 -> clamped to 1; expiry = 2000.
    let effects = state.on_grant_result(1, Ok(receipt("g-1", 3_000)));
    assert_eq!(
        effects,
        vec![
            GrantEffect::SetTouchTimer {
                handle: 1,
                delay_ms: 1
            },
            GrantEffect::SetExpiryTimer {
                handle: 2,
                delay_ms: 2_000
            },
        ]
    );
}

#[test]
fn a_ratio_of_one_schedules_touch_at_the_full_ttl_without_clamping() {
    let (mut state, _) = machine_with_ratio(1_000, 1.0);
    state.grant(input());
    let effects = state.on_grant_result(1, Ok(receipt("g-1", 3_000)));
    assert_eq!(
        effects,
        vec![
            GrantEffect::SetTouchTimer {
                handle: 1,
                delay_ms: 2_000
            },
            GrantEffect::SetExpiryTimer {
                handle: 2,
                delay_ms: 2_000
            },
        ]
    );
}

#[test]
fn an_exact_integer_floor_is_not_bumped_by_the_one_millisecond_clamp() {
    let (mut state, _) = machine_with_ratio(1_000, 0.25);
    state.grant(input());
    // remaining = 4; floor(4 * 0.25) = 1 exactly -> must stay 1 (not 2).
    let effects = state.on_grant_result(1, Ok(receipt("g-1", 1_004)));
    assert_eq!(
        effects,
        vec![
            GrantEffect::SetTouchTimer {
                handle: 1,
                delay_ms: 1
            },
            GrantEffect::SetExpiryTimer {
                handle: 2,
                delay_ms: 4
            },
        ]
    );
}

// --- Observer / dispose semantics --------------------------------------------

#[test]
fn two_listeners_are_both_notified_and_unsubscribing_one_leaves_the_other() {
    let (mut state, _) = machine(1_000);
    let a = Rc::new(RefCell::new(Vec::<GrantPhase>::new()));
    let b = Rc::new(RefCell::new(Vec::<GrantPhase>::new()));
    let ra = a.clone();
    let rb = b.clone();
    let id_a = state.subscribe(Box::new(move |s| ra.borrow_mut().push(s.phase)));
    state.subscribe(Box::new(move |s| rb.borrow_mut().push(s.phase)));

    state.grant(input());
    assert_eq!(*a.borrow(), vec![GrantPhase::Granting]);
    assert_eq!(*b.borrow(), vec![GrantPhase::Granting]);

    state.unsubscribe(id_a);
    state.revoke();
    assert_eq!(*a.borrow(), vec![GrantPhase::Granting]);
    assert_eq!(*b.borrow(), vec![GrantPhase::Granting, GrantPhase::Revoked]);
}

#[test]
fn dispose_clears_previous_listeners_but_a_later_subscriber_still_sees_updates() {
    let (mut state, _) = machine(1_000);
    let old = Rc::new(RefCell::new(Vec::<GrantPhase>::new()));
    let ro = old.clone();
    state.subscribe(Box::new(move |s| ro.borrow_mut().push(s.phase)));
    grant_g1(&mut state);
    assert_eq!(*old.borrow(), vec![GrantPhase::Granting, GrantPhase::Granted]);

    state.dispose();
    let fresh = Rc::new(RefCell::new(Vec::<GrantPhase>::new()));
    let rf = fresh.clone();
    state.subscribe(Box::new(move |s| rf.borrow_mut().push(s.phase)));
    // revoke() is not gated on `disposed` (faithful to the TS source); it still
    // updates state. The pre-dispose listener must not see it; the new one does.
    let _ = state.revoke();
    assert_eq!(*old.borrow(), vec![GrantPhase::Granting, GrantPhase::Granted]);
    assert_eq!(*fresh.borrow(), vec![GrantPhase::Revoked]);
}

#[test]
fn a_failed_release_of_an_orphan_grant_warns_with_that_orphans_id() {
    let (mut state, _) = machine(1_000);
    state.grant(input()); // token 1
    state.revoke(); // token 2
    let release = state.on_grant_result(1, Ok(receipt("g-orphan", 3_000)));
    assert_eq!(
        release,
        vec![GrantEffect::CallRevoke {
            grant_id: "g-orphan".to_owned()
        }]
    );
    let effects = state.on_revoke_result("g-orphan", Err("permission denied".to_owned()));
    assert_eq!(
        effects,
        vec![GrantEffect::Warn {
            message: "failed to revoke editor grant g-orphan".to_owned(),
            error: Some("permission denied".to_owned()),
        }]
    );
}
