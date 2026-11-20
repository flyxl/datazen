//! Independent audit of T078 number/axis formatting
//! (`packages/frontend/src/lib/chart/format.ts`).
//!
//! Oracles are the en-US React results measured for these inputs
//! (`toLocaleString` for `formatNumber`, `Number#toFixed` for compact
//! and percent), not values produced by re-implementing the same
//! helpers. The epoch clock is `Date#getHours` in the process zone,
//! checked against `chrono::Local` rather than against `format_epoch_ms`.

use chrono::{Local, TimeZone};

use crate::support::chart_field_inference::ChartValue;
use crate::support::chart_format::{
    format_axis_tick, format_compact, format_epoch_ms, format_number, format_percent, is_epoch_ms,
    EPOCH_MS_MAX, EPOCH_MS_MIN,
};

#[test]
fn audit_format_number_matches_en_us_intl() {
    // Integer path, including the sign of -0 and a value past 2^53.
    assert_eq!(format_number(Some(-0.0)), "-0");
    assert_eq!(
        format_number(Some(-1e21)),
        "-1,000,000,000,000,000,000,000"
    );
    assert_eq!(
        format_number(Some(1e22)),
        "10,000,000,000,000,000,000,000"
    );
    // Intl half-expand on the decimal spelling, not binary toFixed
    // (`(1.005).toFixed(2)` is `1.00`).
    assert_eq!(format_number(Some(1.005)), "1.01");
    assert_eq!(format_number(Some(1.995)), "2");
    assert_eq!(format_number(Some(-1.995)), "-2");
    assert_eq!(format_number(Some(1000.004)), "1,000");
    assert_eq!(format_number(Some(1000.005)), "1,000.01");
    assert_eq!(format_number(Some(-1e-7)), "-0");
    assert_eq!(format_number(Some(0.1 + 0.2)), "0.3");
}

#[test]
fn audit_compact_threshold_is_1e4_and_percent_is_to_fixed() {
    // Just under the K cutoff stays a plain integer string. 10000 is
    // `(10000/1000).toFixed(1) + "K"`.
    assert_eq!(format_compact(Some(9999.0)), "9999");
    assert_eq!(format_compact(Some(10_000.0)), "10.0K");
    assert_eq!(format_compact(Some(-0.0)), "0");
    assert_eq!(format_compact(Some(1.005)), "1.0");
    // `(1/3 * 100).toFixed(1)` is `33.3`.
    assert_eq!(format_percent(Some(1.0 / 3.0)), "33.3%");
    assert_eq!(format_percent(Some(-0.256)), "-25.6%");
}

#[test]
fn audit_axis_clock_is_local_and_the_window_is_closed_on_both_ends() {
    let below = EPOCH_MS_MIN - 1.0;
    assert!(!is_epoch_ms(below));
    assert!(is_epoch_ms(EPOCH_MS_MIN));
    assert!(is_epoch_ms(EPOCH_MS_MAX));
    assert!(!is_epoch_ms(EPOCH_MS_MAX + 1.0));
    // 946_684_799_999 is an exact integer below 2^53. en-US grouping.
    assert_eq!(format_axis_tick(&ChartValue::Number(below)), "946,684,799,999");
    let clock = Local
        .timestamp_millis_opt(EPOCH_MS_MAX as i64)
        .single()
        .map(|dt| dt.format("%H:%M:%S").to_string());
    assert_eq!(
        format_axis_tick(&ChartValue::Number(EPOCH_MS_MAX)),
        clock.expect("2100-01-01 fits in a local timestamp")
    );
    assert_eq!(format_epoch_ms(EPOCH_MS_MIN).len(), 8);
    // 21 scalars: JS `length > 20` keeps 18 and an ellipsis. Not the
    // coder's 26-letter fixture.
    assert_eq!(
        format_axis_tick(&ChartValue::String("abcdefghijklmnopqrstu".into())),
        "abcdefghijklmnopqr…"
    );
    assert_eq!(
        format_axis_tick(&ChartValue::String("abcdefghij".into())),
        "abcdefghij"
    );
}
