//! T034: React `lib/session/counters.ts` parity. Every assertion below pins a
//! behavior of one of the three source functions — the two error branches of
//! `counterValue`, the two-field identity of `sameHandle`, and the wire form of
//! `wireCounter`.

use std::cmp::Ordering;

use crate::session::counters::{
    counter_value, same_handle, wire_counter, CounterError, CounterInput, SessionHandle,
};

fn handle(db_session_id: &str, runtime_epoch: &str) -> SessionHandle {
    SessionHandle {
        db_session_id: db_session_id.to_owned(),
        runtime_epoch: runtime_epoch.to_owned(),
    }
}

#[test]
fn counter_value_accepts_decimal_strings_and_keeps_full_precision() {
    // The whole point of CM-01: a count above 2^53 must survive the trip.
    let big = counter_value(CounterInput::Text("9007199254740993".to_owned()))
        .expect("a decimal string is a valid counter encoding");
    assert_eq!(big.as_str(), "9007199254740993");

    let small = counter_value(CounterInput::Text("42".to_owned())).expect("42 is valid");
    assert_eq!(small.as_str(), "42");
    assert_eq!(wire_counter(&small), "42");
}

#[test]
fn counter_value_accepts_safe_numbers() {
    let zero = counter_value(CounterInput::Number(0.0)).expect("zero is a safe integer");
    assert_eq!(wire_counter(&zero), "0");

    let max = counter_value(CounterInput::Number(9_007_199_254_740_991.0))
        .expect("MAX_SAFE_INTEGER is representable");
    assert_eq!(max.as_str(), "9007199254740991");

    // JS `String(-0)` is "0", so -0 decodes to zero, not a negative counter.
    let negative_zero = counter_value(CounterInput::Number(-0.0)).expect("-0 is still zero");
    assert_eq!(wire_counter(&negative_zero), "0");
}

#[test]
fn counter_value_normalises_leading_zeros() {
    // BigInt("007") === 7n: leading zeros are digits but not part of the value.
    let seven = counter_value(CounterInput::Text("007".to_owned())).expect("leading zeros");
    assert_eq!(seven.as_str(), "7");
    let zero = counter_value(CounterInput::Text("000".to_owned())).expect("all zeros");
    assert_eq!(zero.as_str(), "0");
}

#[test]
fn counter_value_orders_numerically_not_lexicographically() {
    let nine = counter_value(CounterInput::Text("9".to_owned())).expect("9");
    let ten = counter_value(CounterInput::Text("10".to_owned())).expect("10");
    assert!(nine < ten, "10 is numerically larger than 9");
    assert_eq!(nine.cmp(&ten), Ordering::Less);
    assert_eq!(ten.cmp(&ten), Ordering::Equal);
}

#[test]
fn counter_value_refuses_unsafe_numbers() {
    // Number.isSafeInteger gate: 2^53, a fraction, infinities and NaN all
    // throw 'Unsafe counter encoding' before the digit regex is consulted.
    for bad in [9_007_199_254_740_992.0, 1.5, f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
        assert_eq!(
            counter_value(CounterInput::Number(bad)),
            Err(CounterError::Unsafe),
            "unsafe number {bad:?}"
        );
    }
    assert_eq!(CounterError::Unsafe.as_str(), "Unsafe counter encoding");
}

#[test]
fn counter_value_refuses_non_decimal_encodings() {
    // The /^\d+$/ gate rejects empty strings and every non-digit form.
    for bad in ["", "-1", "+1", "1.5", "1e3", " 1", "1 ", "abc", "１２"] {
        assert_eq!(
            counter_value(CounterInput::Text(bad.to_owned())),
            Err(CounterError::Invalid),
            "malformed counter {bad:?}"
        );
    }
    // A negative number is a *safe* integer, so it reaches the regex as
    // String(-3) === "-3" and fails there — a different error from unsafe.
    assert_eq!(
        counter_value(CounterInput::Number(-3.0)),
        Err(CounterError::Invalid)
    );
    assert_eq!(CounterError::Invalid.as_str(), "Invalid counter encoding");
}

#[test]
fn counter_value_round_trips_through_the_wire_form() {
    for digits in ["0", "1", "25", "9007199254740993", "18446744073709551615"] {
        let value = counter_value(CounterInput::Text(digits.to_owned())).expect("digits");
        assert_eq!(wire_counter(&value), digits);
    }
}

#[test]
fn same_handle_requires_both_handles_and_both_fields() {
    let a = handle("sess-1", "epoch-1");
    let same = handle("sess-1", "epoch-1");
    let different_session = handle("sess-2", "epoch-1");
    let different_epoch = handle("sess-1", "epoch-2");

    assert!(same_handle(Some(&a), Some(&same)));
    assert!(!same_handle(Some(&a), Some(&different_session)));
    assert!(!same_handle(Some(&a), Some(&different_epoch)));

    // A missing (null) handle is never the same as anything, including null.
    assert!(!same_handle(None, Some(&a)));
    assert!(!same_handle(Some(&a), None));
    assert!(!same_handle(None, None));
}

#[test]
fn wire_counter_emits_the_decimal_string_without_loss() {
    // wireCounter(0n) and wireCounter(2^53+1) are literally those digits.
    let zero = counter_value(CounterInput::Number(0.0)).expect("zero");
    assert_eq!(wire_counter(&zero), "0");

    let big = counter_value(CounterInput::Text("9007199254740993".to_owned())).expect("big");
    assert_eq!(wire_counter(&big), "9007199254740993");
    assert_eq!(wire_counter(&big), big.as_str());
}
