//! Counter and session-handle logic ported from
//! `packages/frontend/src/lib/session/counters.ts`.
//!
//! Three pure operations, kept faithful to the React source:
//!
//! * [`counter_value`] narrows a wire counter (a number or a decimal string)
//!   to a non-negative big integer. Numbers pass the `Number.isSafeInteger`
//!   gate first; both forms must then be plain decimal digits.
//! * [`same_handle`] compares two session handles by `(db_session_id,
//!   runtime_epoch)`. Both must exist and agree on *both* fields — after an
//!   epoch change the old handle is dead even if the session id was reused.
//! * [`wire_counter`] renders a big integer back to the decimal string Rust
//!   puts on the wire (React casts that string back to its nominal `Counter`).
//!
//! The React `bigint` is modelled by [`BigCounter`], a canonical non-negative
//! decimal string. `datazen-ui` does not depend on the kernel's `u64` counter
//! and Rust has no built-in arbitrary-precision integer, so the decimal string
//! *is* the value: parsing `"9007199254740993"` keeps every digit, which is
//! exactly the precision CM-01 exists to protect.

use std::cmp::Ordering;
use std::fmt;

/// JS `Number.MAX_SAFE_INTEGER` (`2^53 - 1`). Past it, an `f64` can no longer
/// hold an exact integer, so a number input is refused rather than rounded.
const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

/// The two wire forms of `Counter | string | number`. The React function
/// treats numbers differently (it applies the safe-integer gate), so the Rust
/// value keeps them distinct instead of stringifying up front.
#[allow(dead_code)] // foundation API; consumers land with the session controllers
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum CounterInput {
    /// A JSON number. Only a non-negative safe integer survives.
    Number(f64),
    /// A decimal string — the encoding Rust actually emits (CM-01).
    Text(String),
}

/// Why a counter was refused. The variants mirror the two `Error` messages the
/// React function throws, preserved verbatim by [`CounterError::as_str`].
#[allow(dead_code)] // foundation API; consumers land with the session controllers
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CounterError {
    /// A number that is not a safe integer (`Unsafe counter encoding`).
    Unsafe,
    /// A value that is not one or more ASCII digits (`Invalid counter
    /// encoding`).
    Invalid,
}

#[allow(dead_code)] // diagnostic surface of the reusable error
impl CounterError {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Unsafe => "Unsafe counter encoding",
            Self::Invalid => "Invalid counter encoding",
        }
    }
}

#[allow(dead_code)] // let callers display the refusal the way React throws it
impl fmt::Display for CounterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[allow(dead_code)] // idiomatic Rust error for a reusable foundation API
impl std::error::Error for CounterError {}

/// The wire form of a counter: a decimal string. Rust serializes its `u64`
/// counter with `collect_str`; the React facade only *nominally* types this as
/// a number.
#[allow(dead_code)] // foundation API; consumers land with the session controllers
pub(crate) type Counter = String;

/// A non-negative integer of arbitrary size, stored in canonical decimal form
/// (no leading zeros, `"0"` for zero). This is the React `bigint`.
#[allow(dead_code)] // foundation API; consumers land with the session controllers
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct BigCounter(String);

#[allow(dead_code)] // canonicalisation and display of the reusable big integer
impl BigCounter {
    /// Build from already-validated ASCII digits, dropping leading zeros the
    /// way `BigInt("007")` normalizes to `7n`.
    fn from_digits(digits: &str) -> Self {
        let trimmed = digits.trim_start_matches('0');
        Self(if trimmed.is_empty() { "0" } else { trimmed }.to_owned())
    }

    /// The canonical decimal form — exactly what goes back on the wire.
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

#[allow(dead_code)] // React compares bigints numerically, not lexicographically
impl PartialOrd for BigCounter {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[allow(dead_code)] // React compares bigints numerically, not lexicographically
impl Ord for BigCounter {
    fn cmp(&self, other: &Self) -> Ordering {
        // Canonical strings have no leading zeros: shorter means smaller, and
        // equal lengths compare digit by digit.
        self.0.len().cmp(&other.0.len()).then_with(|| self.0.cmp(&other.0))
    }
}

/// `Number.isSafeInteger`: finite, integral and within `±(2^53 - 1)`.
fn is_safe_integer(value: f64) -> bool {
    value.is_finite() && value.fract() == 0.0 && value.abs() <= MAX_SAFE_INTEGER
}

/// `counterValue`: narrow a wire counter to a big integer, or explain why it
/// cannot be trusted.
pub(crate) fn counter_value(input: CounterInput) -> Result<BigCounter, CounterError> {
    match input {
        CounterInput::Number(value) => {
            if !is_safe_integer(value) {
                return Err(CounterError::Unsafe);
            }
            // JS `String(n)` for a safe integer has no exponent and no
            // fractional part; only a leading '-' can break `^\d+$`. A
            // negative integer is therefore a *different* error from an
            // unsafe one — keep the React branch order.
            if value < 0.0 {
                return Err(CounterError::Invalid);
            }
            // The cast is exact: the value is a non-negative safe integer.
            // `-0.0` casts to `0`, matching JS `String(-0)` === "0".
            Ok(BigCounter::from_digits(&(value as u64).to_string()))
        }
        CounterInput::Text(value) => {
            // `^\d+$` in JS is exactly one or more ASCII [0-9]; no sign, no
            // whitespace, no exponent, no full-width digits.
            if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(CounterError::Invalid);
            }
            Ok(BigCounter::from_digits(&value))
        }
    }
}

/// Minimal mirror of the backend-client `SessionHandle`: a session id paired
/// with the runtime epoch it was bound to. Both are opaque ids (`Id` in TS).
#[allow(dead_code)] // foundation API; consumers land with the session controllers
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SessionHandle {
    pub db_session_id: String,
    pub runtime_epoch: String,
}

/// `sameHandle`: both handles must exist and agree on both the session id and
/// the runtime epoch. A missing handle (`null` on the React side) is never
/// equal to anything, not even another missing handle.
pub(crate) fn same_handle(a: Option<&SessionHandle>, b: Option<&SessionHandle>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => {
            a.db_session_id == b.db_session_id && a.runtime_epoch == b.runtime_epoch
        }
        _ => false,
    }
}

/// `wireCounter`: the decimal string a Rust counter travels as. The input is a
/// big integer (React `bigint`); the output is the wire `Counter`.
pub(crate) fn wire_counter(value: &BigCounter) -> Counter {
    value.as_str().to_owned()
}
