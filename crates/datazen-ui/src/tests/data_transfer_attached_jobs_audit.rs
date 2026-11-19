//! T073 audit: React-faithful `read_job_counter` number-branch behaviour.
//!
//! React `lib/migrationJobVerdict.ts:124-141`: the number branch is
//! `Number.isFinite(value) && value >= 0 ? Math.trunc(value) : null` —
//! there is NO `Number.isSafeInteger` gate on numbers. That gate exists
//! only on the string branch, via `Number.isSafeInteger(parsed)`.
//! The first port applied the safe-integer gate to numbers too, so a JSON
//! number past 2^53 - 1 read as `None` (counts line showed 0) where React
//! shows the truncated value; fix 95ce3e79 dropped the number-branch gate
//! (see the audit report).

use crate::views::{read_job_counter, JobCounter};

#[test]
fn read_job_counter_number_branch_has_no_safe_integer_gate() {
    // 2^53 itself: React returns it (`Math.trunc`, no safe check).
    assert_eq!(
        read_job_counter(Some(&JobCounter::Number(9_007_199_254_740_992.0))),
        Some(9_007_199_254_740_992)
    );
    // 10^16, past the safe-integer gate but exactly representable as f64.
    assert_eq!(
        read_job_counter(Some(&JobCounter::Number(10_000_000_000_000_000.0))),
        Some(10_000_000_000_000_000)
    );
}

#[test]
fn read_job_counter_string_branch_keeps_the_safe_integer_gate() {
    // Exactly 2^53 as a string: React's parseInt + isSafeInteger rejects it.
    assert_eq!(
        read_job_counter(Some(&JobCounter::Text("9007199254740992".to_string()))),
        None
    );
}
