//! T049: `packages/frontend/src/ai/grantState.ts` port — branch/transition
//! coverage with a fake clock and host-driven effect delivery. Every test
//! asserts the emitted [`GrantEffect`]s and the resulting snapshot, mirroring
//! one concrete branch of the React state machine.

use std::cell::RefCell;
use std::rc::Rc;

use datazen_ui::ai::grant_state::*;

/// Build a receipt like the host's `ai_grant_editor_session` response.
fn receipt(id: &str, expires_at_ms: i64) -> AiGrantReceipt {
    AiGrantReceipt {
        grant_id: id.to_owned(),
        connection_id: "conn-1".to_owned(),
        database: Some("main".to_owned()),
        config_revision: 7,
        expires_at_ms,
    }
}

fn test_input() -> GrantInput {
    GrantInput {
        connection_id: "conn-1".to_owned(),
        database: Some("main".to_owned()),
        editor_session_id: "editor-1".to_owned(),
    }
}

/// A machine plus the fake clock its injected `now` closure reads.
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

/// Drive a fresh machine from `independent` to `granted` with the standard
/// receipt (`ttl = 2000`, touch at 1000, expiry at 2000 from the start time).
fn grant_ok(state: &mut GrantState) -> AiGrantReceipt {
    let result = receipt("g-1", 3_000);
    let start = state.grant(test_input());
    assert_eq!(
        start.effects,
        vec![GrantEffect::CallGrant {
            input: test_input(),
            token: 1
        }]
    );
    let effects = state.on_grant_result(1, Ok(result.clone()));
    assert_eq!(
        effects,
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
    result
}

// --- Entry: independent / duplicate / granted / disposed --------------------

#[test]
fn starts_in_independent_with_no_receipt_deadline_or_error() {
    let (state, _) = machine(1_000);
    assert_eq!(state.snapshot(), &GrantSnapshot::independent());
    assert!(!state.is_granted());
}

#[test]
fn grant_from_independent_enters_granting_and_emits_the_call() {
    let (mut state, _) = machine(1_000);
    let step = state.grant(test_input());
    assert_eq!(
        step.effects,
        vec![GrantEffect::CallGrant {
            input: test_input(),
            token: 1
        }]
    );
    assert!(!step.joined_in_flight);
    assert!(step.receipt.is_none());
    assert_eq!(
        state.snapshot(),
        &GrantSnapshot {
            phase: GrantPhase::Granting,
            receipt: None,
            expires_at_ms: None,
            error: None,
        }
    );
}

#[test]
fn a_duplicate_grant_joins_the_in_flight_request_without_a_new_call() {
    let (mut state, _) = machine(1_000);
    state.grant(test_input());
    let joined = state.grant(test_input());
    assert!(joined.joined_in_flight);
    assert!(joined.effects.is_empty());
    assert!(joined.receipt.is_none());
    // The token was not advanced: the original request still resolves it.
    let _ = state.on_grant_result(1, Ok(receipt("g-1", 3_000)));
    assert!(state.is_granted());
}

#[test]
fn grant_while_granted_returns_the_existing_receipt_without_effects() {
    let (mut state, _) = machine(1_000);
    let expected = grant_ok(&mut state);
    let step = state.grant(test_input());
    assert_eq!(step.receipt, Some(expected));
    assert!(step.effects.is_empty());
    assert!(!step.joined_in_flight);
    assert_eq!(state.snapshot().phase, GrantPhase::Granted);
}

#[test]
fn grant_after_dispose_is_a_noop() {
    let (mut state, _) = machine(1_000);
    state.grant(test_input());
    state.dispose();
    let step = state.grant(test_input());
    assert!(step.receipt.is_none());
    assert!(step.effects.is_empty());
    assert!(!step.joined_in_flight);
}

// --- Grant receipt / denial / races -----------------------------------------

#[test]
fn a_successful_grant_uses_ttl_over_two_for_touch_and_ttl_for_expiry() {
    let (mut state, _) = machine(1_000);
    state.grant(test_input());
    let effects = state.on_grant_result(1, Ok(receipt("g-1", 3_000)));
    assert_eq!(
        effects,
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
    let snapshot = state.snapshot();
    assert_eq!(snapshot.phase, GrantPhase::Granted);
    assert_eq!(snapshot.receipt.as_ref().map(|r| r.grant_id.as_str()), Some("g-1"));
    assert_eq!(snapshot.expires_at_ms, Some(3_000));
    assert_eq!(snapshot.error, None);
    assert!(state.is_granted());
}

#[test]
fn a_grant_expired_on_arrival_never_enters_granted() {
    let (mut state, _) = machine(5_000);
    state.grant(test_input());
    let effects = state.on_grant_result(1, Ok(receipt("g-late", 3_000)));
    // ttl clamps to 0 -> remaining <= 0 -> expireLocally immediately.
    assert!(effects.is_empty());
    assert_eq!(state.snapshot().phase, GrantPhase::Independent);
    assert!(state.snapshot().receipt.is_none());
    assert!(!state.is_granted());
}

#[test]
fn a_denied_grant_returns_to_independent_with_the_error_message() {
    let (mut state, _) = machine(1_000);
    state.grant(test_input());
    let effects = state.on_grant_result(1, Err("network down".to_owned()));
    assert!(effects.is_empty());
    assert_eq!(
        state.snapshot(),
        &GrantSnapshot {
            phase: GrantPhase::Independent,
            receipt: None,
            expires_at_ms: None,
            error: Some("network down".to_owned()),
        }
    );
}

#[test]
fn a_grant_response_after_revoke_is_released_not_applied() {
    let (mut state, _) = machine(1_000);
    state.grant(test_input()); // token 1
    let revoke_effects = state.revoke(); // token 2, no receipt to release
    assert!(revoke_effects.is_empty());

    let effects = state.on_grant_result(1, Ok(receipt("g-orphan", 3_000)));
    assert_eq!(
        effects,
        vec![GrantEffect::CallRevoke {
            grant_id: "g-orphan".to_owned()
        }]
    );
    assert_eq!(state.snapshot().phase, GrantPhase::Revoked);
    assert!(state.snapshot().receipt.is_none());
    assert!(!state.is_granted());
}

#[test]
fn a_grant_error_after_revoke_is_ignored() {
    let (mut state, _) = machine(1_000);
    state.grant(test_input());
    state.revoke();
    let effects = state.on_grant_result(1, Err("late failure".to_owned()));
    assert!(effects.is_empty());
    assert_eq!(state.snapshot().error, None);
    assert_eq!(state.snapshot().phase, GrantPhase::Revoked);
}

#[test]
fn a_grant_response_after_dispose_is_released_without_a_state_change() {
    let (mut state, _) = machine(1_000);
    state.grant(test_input()); // phase Granting
    let dispose_effects = state.dispose();
    assert!(dispose_effects.is_empty()); // no receipt yet

    let effects = state.on_grant_result(1, Ok(receipt("g-orphan", 3_000)));
    assert_eq!(
        effects,
        vec![GrantEffect::CallRevoke {
            grant_id: "g-orphan".to_owned()
        }]
    );
    // React dispose() does not update `current`; the phase stays `granting`.
    assert_eq!(state.snapshot().phase, GrantPhase::Granting);
    assert!(state.snapshot().receipt.is_none());
}

// --- Revoke / dispose --------------------------------------------------------

#[test]
fn revoke_from_granted_clears_both_timers_and_releases_the_receipt() {
    let (mut state, _) = machine(1_000);
    grant_ok(&mut state);
    let effects = state.revoke();
    assert_eq!(
        effects,
        vec![
            GrantEffect::ClearTimer { handle: 1 },
            GrantEffect::ClearTimer { handle: 2 },
            GrantEffect::CallRevoke {
                grant_id: "g-1".to_owned()
            },
        ]
    );
    assert_eq!(
        state.snapshot(),
        &GrantSnapshot {
            phase: GrantPhase::Revoked,
            receipt: None,
            expires_at_ms: None,
            error: None,
        }
    );
}

#[test]
fn revoke_while_granting_invalidates_the_request_without_a_receipt_release() {
    let (mut state, _) = machine(1_000);
    state.grant(test_input()); // token 1
    let effects = state.revoke();
    assert!(effects.is_empty());
    assert_eq!(state.snapshot().phase, GrantPhase::Revoked);
    // A new grant increments the token again (TS `++grantToken`): revoke took
    // it to 2, the next grant takes it to 3 — so the old request stays stale.
    let step = state.grant(test_input());
    assert_eq!(
        step.effects,
        vec![GrantEffect::CallGrant {
            input: test_input(),
            token: 3
        }]
    );
}

#[test]
fn revoke_from_independent_is_clean() {
    let (mut state, _) = machine(1_000);
    let effects = state.revoke();
    assert!(effects.is_empty());
    assert_eq!(state.snapshot().phase, GrantPhase::Revoked);
    assert!(state.snapshot().receipt.is_none());
}

#[test]
fn a_failed_quiet_revoke_warns_and_a_successful_one_is_silent() {
    let (mut state, _) = machine(1_000);
    let effects = state.on_revoke_result("g-1", Err("boom".to_owned()));
    assert_eq!(
        effects,
        vec![GrantEffect::Warn {
            message: "failed to revoke editor grant g-1".to_owned(),
            error: Some("boom".to_owned()),
        }]
    );
    assert!(state.on_revoke_result("g-1", Ok(())).is_empty());
}

#[test]
fn dispose_is_idempotent_stops_timers_and_keeps_the_last_snapshot() {
    let (mut state, _) = machine(1_000);
    grant_ok(&mut state);
    let effects = state.dispose();
    assert_eq!(
        effects,
        vec![
            GrantEffect::ClearTimer { handle: 1 },
            GrantEffect::ClearTimer { handle: 2 },
            GrantEffect::CallRevoke {
                grant_id: "g-1".to_owned()
            },
        ]
    );
    assert!(state.dispose().is_empty());
    // React dispose() clears listeners/timers but does not rewrite `current`.
    assert_eq!(state.snapshot().phase, GrantPhase::Granted);
    assert!(state.is_granted());
}

// --- Touch scheduling / firing ----------------------------------------------

#[test]
fn the_touch_timer_calls_touch_while_the_grant_is_still_live() {
    let (mut state, _) = machine(1_000);
    grant_ok(&mut state);
    let effects = state.fire_touch_timer(1);
    assert_eq!(
        effects,
        vec![GrantEffect::CallTouch {
            grant_id: "g-1".to_owned()
        }]
    );
    assert_eq!(state.snapshot().phase, GrantPhase::Granted);
    assert!(state.is_granted());
}

#[test]
fn a_touch_fired_at_the_deadline_expires_locally_and_clears_expiry() {
    let (mut state, clock) = machine(1_000);
    grant_ok(&mut state);
    *clock.borrow_mut() = 3_000; // now >= expiresAtMs
    let effects = state.fire_touch_timer(1);
    assert_eq!(effects, vec![GrantEffect::ClearTimer { handle: 2 }]);
    assert_eq!(state.snapshot().phase, GrantPhase::Independent);
    assert!(state.snapshot().receipt.is_none());
    assert!(!state.is_granted());
}

#[test]
fn a_successful_touch_extends_the_deadline_and_rearms_both_timers() {
    let (mut state, clock) = machine(1_000);
    grant_ok(&mut state);
    *clock.borrow_mut() = 1_500;
    let fired = state.fire_touch_timer(1);
    assert_eq!(
        fired,
        vec![GrantEffect::CallTouch {
            grant_id: "g-1".to_owned()
        }]
    );
    let effects = state.on_touch_result("g-1", Ok(true));
    // The still-running expiry timer is cancelled before the new pair is set.
    assert_eq!(
        effects,
        vec![
            GrantEffect::ClearTimer { handle: 2 },
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
    assert_eq!(state.snapshot().expires_at_ms, Some(3_500)); // now(1500) + ttl(2000)
    assert_eq!(state.snapshot().phase, GrantPhase::Granted);
}

#[test]
fn a_rejected_touch_moves_to_revoked_with_touch_rejected_error() {
    let (mut state, _) = machine(1_000);
    grant_ok(&mut state);
    state.fire_touch_timer(1);
    let effects = state.on_touch_result("g-1", Ok(false));
    assert_eq!(effects, vec![GrantEffect::ClearTimer { handle: 2 }]);
    assert_eq!(state.snapshot().phase, GrantPhase::Revoked);
    assert_eq!(state.snapshot().error.as_deref(), Some("touch rejected"));
    assert!(state.snapshot().receipt.is_none());
}

#[test]
fn a_failed_touch_moves_to_revoked_with_its_own_message() {
    let (mut state, _) = machine(1_000);
    grant_ok(&mut state);
    state.fire_touch_timer(1);
    let effects = state.on_touch_result("g-1", Err("socket closed".to_owned()));
    assert_eq!(effects, vec![GrantEffect::ClearTimer { handle: 2 }]);
    assert_eq!(state.snapshot().phase, GrantPhase::Revoked);
    assert_eq!(state.snapshot().error.as_deref(), Some("socket closed"));
}

#[test]
fn revoke_during_a_touch_await_wins_over_every_touch_outcome() {
    let (mut state, _) = machine(1_000);
    grant_ok(&mut state);
    state.fire_touch_timer(1); // touch in flight
    let revoke_effects = state.revoke();
    assert_eq!(
        revoke_effects,
        vec![
            GrantEffect::ClearTimer { handle: 2 },
            GrantEffect::CallRevoke {
                grant_id: "g-1".to_owned()
            },
        ]
    );
    // Both a success and an error are ignored once the receipt is gone.
    assert!(state.on_touch_result("g-1", Ok(true)).is_empty());
    assert!(state.on_touch_result("g-1", Err("late".to_owned())).is_empty());
    assert_eq!(state.snapshot().phase, GrantPhase::Revoked);
    assert_eq!(state.snapshot().error, None);
}

#[test]
fn the_expiry_timer_expires_locally_and_clears_the_touch_timer() {
    let (mut state, _) = machine(1_000);
    grant_ok(&mut state);
    let effects = state.fire_expiry_timer(2);
    assert_eq!(effects, vec![GrantEffect::ClearTimer { handle: 1 }]);
    assert_eq!(state.snapshot().phase, GrantPhase::Independent);
    assert!(state.snapshot().receipt.is_none());
    assert!(!state.is_granted());
}

#[test]
fn stale_timer_handles_are_ignored() {
    let (mut state, _) = machine(1_000);
    grant_ok(&mut state);
    assert!(state.fire_touch_timer(99).is_empty());
    assert!(state.fire_expiry_timer(99).is_empty());
    assert_eq!(state.snapshot().phase, GrantPhase::Granted);
    // The live handle still works after the stale ones were rejected.
    assert_eq!(
        state.fire_touch_timer(1),
        vec![GrantEffect::CallTouch {
            grant_id: "g-1".to_owned()
        }]
    );
}

#[test]
fn a_touch_outcome_without_a_granted_receipt_is_a_noop() {
    let (mut state, _) = machine(1_000);
    // Independent: no receipt, not granted.
    assert!(state.on_touch_result("g-1", Ok(true)).is_empty());
    assert!(state.on_touch_result("g-1", Ok(false)).is_empty());
    assert!(state.on_touch_result("g-1", Err("x".to_owned())).is_empty());
    assert_eq!(state.snapshot().phase, GrantPhase::Independent);
}

// --- Configurable timing -----------------------------------------------------

#[test]
fn a_custom_touch_ratio_changes_the_touch_delay_only() {
    let (mut state, _) = machine_with_ratio(1_000, 0.25);
    state.grant(test_input());
    let effects = state.on_grant_result(1, Ok(receipt("g-1", 3_000)));
    assert_eq!(
        effects,
        vec![
            GrantEffect::SetTouchTimer {
                handle: 1,
                delay_ms: 500
            },
            GrantEffect::SetExpiryTimer {
                handle: 2,
                delay_ms: 2_000
            },
        ]
    );
}

#[test]
fn the_touch_delay_is_never_less_than_one_millisecond() {
    let (mut state, _) = machine_with_ratio(1_000, 0.1);
    state.grant(test_input());
    // ttl = 2ms; floor(2 * 0.1) = 0 -> clamped to 1.
    let effects = state.on_grant_result(1, Ok(receipt("g-1", 1_002)));
    assert_eq!(
        effects,
        vec![
            GrantEffect::SetTouchTimer {
                handle: 1,
                delay_ms: 1
            },
            GrantEffect::SetExpiryTimer {
                handle: 2,
                delay_ms: 2
            },
        ]
    );
}

#[test]
fn the_grant_input_including_a_null_database_is_passed_through_verbatim() {
    let (mut state, _) = machine(1_000);
    let input = GrantInput {
        connection_id: "c-null".to_owned(),
        database: None,
        editor_session_id: "e-null".to_owned(),
    };
    let step = state.grant(input.clone());
    assert_eq!(
        step.effects,
        vec![GrantEffect::CallGrant { input, token: 1 }]
    );
}

// --- Observer facility -------------------------------------------------------

#[test]
fn listeners_see_each_update_and_unsubscribe_stops_notifications() {
    let (mut state, _) = machine(1_000);
    let seen = Rc::new(RefCell::new(Vec::<GrantSnapshot>::new()));
    let recorder = seen.clone();
    let id = state.subscribe(Box::new(move |snapshot| recorder.borrow_mut().push(snapshot.clone())));

    state.grant(test_input());
    assert_eq!(seen.borrow().len(), 1);
    assert_eq!(seen.borrow()[0].phase, GrantPhase::Granting);

    let _ = state.on_grant_result(1, Ok(receipt("g-1", 3_000)));
    assert_eq!(seen.borrow().len(), 2);
    assert_eq!(seen.borrow()[1].phase, GrantPhase::Granted);

    state.unsubscribe(id);
    state.revoke();
    assert_eq!(seen.borrow().len(), 2);
}
