//! Independent audit for T034 (`packages/frontend/src/lib/session/counters.ts`).
//!
//! The coder's `tests/counters.rs` pins the happy paths. This file adds the
//! boundary semantics that a faithful port of the 19-line source must also
//! hold, each derived line-by-line from the React implementation:
//!
//! ```ts
//! if (typeof value === 'number' && !Number.isSafeInteger(value))
//!   throw new Error('Unsafe counter encoding');
//! if (!/^\d+$/.test(String(value))) throw new Error('Invalid counter encoding');
//! return BigInt(value);
//! ```
//!
//! In particular: the safe-integer gate is checked *before* the digit regex
//! (branch order decides which of the two errors a value produces), `^\d+$`
//! is stricter than `BigInt`'s own string parser (a trailing `\n` is rejected
//! by the regex even though `BigInt` would trim it), and a negative *safe*
//! integer reaches the regex and fails there — a different error from an
//! unsafe number.

use std::collections::HashSet;

use crate::session::counters::{
    counter_value, same_handle, wire_counter, BigCounter, CounterError, CounterInput, SessionHandle,
};

fn handle(id: &str, epoch: &str) -> SessionHandle {
    SessionHandle {
        db_session_id: id.to_owned(),
        runtime_epoch: epoch.to_owned(),
    }
}

#[test]
fn the_safe_integer_gate_sits_at_exactly_2_53_minus_one() {
    const MAX_SAFE: f64 = 9_007_199_254_740_991.0; // 2^53 - 1
    // MAX_SAFE itself is the last accepted number; 2^53 is the first refused.
    assert_eq!(
        counter_value(CounterInput::Number(MAX_SAFE)).map(|v| v.as_str().to_owned()),
        Ok("9007199254740991".to_owned())
    );
    assert_eq!(
        counter_value(CounterInput::Number(MAX_SAFE + 1.0)),
        Err(CounterError::Unsafe),
        "2^53 is outside Number.isSafeInteger"
    );
    // Powers of two below the boundary stay exact through the f64 -> u64 cast.
    assert_eq!(
        counter_value(CounterInput::Number(4_503_599_627_370_496.0)) // 2^52
            .map(|v| v.as_str().to_owned()),
        Ok("4503599627370496".to_owned())
    );
    assert_eq!(
        counter_value(CounterInput::Number(4_503_599_627_370_497.0)) // 2^52 + 1
            .map(|v| v.as_str().to_owned()),
        Ok("4503599627370497".to_owned())
    );
}

#[test]
fn an_unsafe_number_is_refused_before_the_digit_gate() {
    // 1e21 stringifies to "1e+21", which would also fail /^\d+$/. The React
    // branch order means the *unsafe* error wins; the port must not collapse
    // the two branches into one "invalid" check.
    assert_eq!(
        counter_value(CounterInput::Number(1.0e21)),
        Err(CounterError::Unsafe)
    );
    assert_eq!(
        counter_value(CounterInput::Number(-1.0e21)),
        Err(CounterError::Unsafe)
    );
}

#[test]
fn a_negative_safe_integer_is_invalid_not_unsafe() {
    // Number.isSafeInteger(-3) is true, so the first gate passes; String(-3)
    // is "-3" which fails the digit regex -> 'Invalid counter encoding'.
    assert_eq!(
        counter_value(CounterInput::Number(-3.0)),
        Err(CounterError::Invalid)
    );
    assert_eq!(
        counter_value(CounterInput::Number(-9_007_199_254_740_991.0)),
        Err(CounterError::Invalid)
    );
}

#[test]
fn text_rejects_what_bigint_would_silently_trim() {
    // /^\d+$/ is stricter than BigInt(string): JS BigInt("123\n") === 123n,
    // but the regex refuses the trailing newline (verified against node:
    // /^\d+$/.test("123\n") === false). The port must reject it too.
    for bad in ["123\n", "\n123", "123 ", " 123", "12\t3", "1_000"] {
        assert_eq!(
            counter_value(CounterInput::Text(bad.to_owned())),
            Err(CounterError::Invalid),
            "text {bad:?} must fail the digit gate"
        );
    }
}

#[test]
fn text_and_number_forms_normalize_to_one_value() {
    let from_text = counter_value(CounterInput::Text("007".to_owned())).expect("leading zeros");
    let from_plain = counter_value(CounterInput::Text("7".to_owned())).expect("plain digits");
    let from_number = counter_value(CounterInput::Number(7.0)).expect("number 7");
    let all_zero = counter_value(CounterInput::Text("0000".to_owned())).expect("all zeros");
    let zero_number = counter_value(CounterInput::Number(-0.0)).expect("negative zero");

    assert_eq!(from_text, from_plain);
    assert_eq!(from_plain, from_number);
    assert_eq!(wire_counter(&from_text), "7");
    assert_eq!(all_zero, zero_number);
    assert_eq!(wire_counter(&all_zero), "0");
}

#[test]
fn big_counter_ordering_and_hash_track_numeric_value() {
    let parse = |s: &str| counter_value(CounterInput::Text(s.to_owned())).expect("digits");
    let two = parse("2");
    let ten = parse("10");
    let ten_again = parse("010"); // canonicalized to "10"
    let nine_billion = parse("9000000000");

    assert!(two < ten && ten < nine_billion, "order is numeric, not lexicographic");
    assert_eq!(ten.cmp(&ten_again), std::cmp::Ordering::Equal);

    // Hash agrees with Eq: canonicalisation makes "010" and "10" one key.
    let mut set = HashSet::new();
    set.insert(ten.clone());
    assert!(set.contains(&ten_again), "equal values hash to one bucket");
    assert!(!set.contains(&two));
}

#[test]
fn same_handle_matches_js_value_equality_of_both_fields() {
    // JS uses `===` on both Id strings, so empty strings are equal to each
    // other and comparison is case/whitespace sensitive.
    assert!(same_handle(Some(&handle("", "")), Some(&handle("", ""))));
    assert!(!same_handle(Some(&handle("sess", "epoch")), Some(&handle("SESS", "epoch"))));
    assert!(!same_handle(Some(&handle("sess", "epoch")), Some(&handle("sess", " epoch"))));

    // Reusing a session id across an epoch change is a different handle.
    assert!(!same_handle(
        Some(&handle("sess", "epoch-1")),
        Some(&handle("sess", "epoch-2"))
    ));
}

#[test]
fn error_messages_match_the_js_thrown_strings_verbatim() {
    assert_eq!(CounterError::Unsafe.as_str(), "Unsafe counter encoding");
    assert_eq!(CounterError::Invalid.as_str(), "Invalid counter encoding");
    assert_eq!(CounterError::Unsafe.to_string(), "Unsafe counter encoding");
    assert_eq!(CounterError::Invalid.to_string(), "Invalid counter encoding");
    // The error type is a real std error, not a string-only shim.
    fn assert_error<E: std::error::Error>() {}
    assert_error::<CounterError>();
}

#[test]
fn wire_counter_round_trips_the_full_u64_range() {
    // Rust emits counters as decimal strings up to u64::MAX; every digit must
    // survive counter_value -> wire_counter unchanged (CM-01's whole point).
    for digits in ["0", "1", "9007199254740993", "18446744073709551614", "18446744073709551615"] {
        let parsed: BigCounter = counter_value(CounterInput::Text(digits.to_owned())).expect("digits");
        assert_eq!(wire_counter(&parsed), digits);
    }
}
