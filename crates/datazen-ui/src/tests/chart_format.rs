//! T078: `lib/chart/format.ts`.

use chrono::{Local, TimeZone};

use crate::support::chart_field_inference::ChartValue;
use crate::support::chart_format::{
    format_axis_tick, format_compact, format_epoch_ms, format_number, format_percent, is_epoch_ms,
    EPOCH_MS_MAX, EPOCH_MS_MIN,
};

#[test]
fn format_number_groups_and_rounds_to_two_digits() {
    assert_eq!(format_number(None), "");
    assert_eq!(format_number(Some(0.0)), "0");
    assert_eq!(format_number(Some(1_234_567.0)), "1,234,567");
    assert_eq!(format_number(Some(-1_234_567.0)), "-1,234,567");
    assert_eq!(format_number(Some(1.5)), "1.5");
    assert_eq!(format_number(Some(1.234)), "1.23");
    assert_eq!(format_number(Some(1.235)), "1.24");
    assert_eq!(format_number(Some(1234.5)), "1,234.5");
    assert_eq!(format_number(Some(-0.0)), "-0");
    assert_eq!(format_number(Some(1.005)), "1.01");
    assert_eq!(format_number(Some(1e-7)), "0");
    assert_eq!(format_number(Some(0.1 + 0.2)), "0.3");
    assert_eq!(format_number(Some(1000.005)), "1,000.01");
    assert_eq!(format_number(Some(-1.235)), "-1.24");
    assert_eq!(
        format_number(Some(1e21)),
        "1,000,000,000,000,000,000,000"
    );
    assert_eq!(
        format_number(Some(1e22)),
        "10,000,000,000,000,000,000,000"
    );
    assert_eq!(format_number(Some(-1e-7)), "-0");
    assert_eq!(format_number(Some(f64::NAN)), "NaN");
    assert_eq!(format_number(Some(f64::INFINITY)), "∞");
    assert_eq!(format_number(Some(f64::NEG_INFINITY)), "-∞");
}

#[test]
fn format_compact_thresholds() {
    assert_eq!(format_compact(None), "");
    assert_eq!(format_compact(Some(9999.0)), "9999");
    assert_eq!(format_compact(Some(10_000.0)), "10.0K");
    assert_eq!(format_compact(Some(-15_000.0)), "-15.0K");
    assert_eq!(format_compact(Some(1_500_000.0)), "1.5M");
    assert_eq!(format_compact(Some(2_000_000_000.0)), "2.0B");
    assert_eq!(format_compact(Some(1.25)), "1.3");
    assert_eq!(format_compact(Some(0.125)), "0.13");
    assert_eq!(format_compact(Some(150.4)), "150");
}

#[test]
fn format_percent_is_one_decimal_ratio() {
    assert_eq!(format_percent(None), "");
    assert_eq!(format_percent(Some(0.0)), "0.0%");
    assert_eq!(format_percent(Some(0.5)), "50.0%");
    assert_eq!(format_percent(Some(-0.256)), "-25.6%");
}

#[test]
fn epoch_window_is_inclusive_and_formats_local_hms() {
    assert!(is_epoch_ms(EPOCH_MS_MIN));
    assert!(is_epoch_ms(EPOCH_MS_MAX));
    assert!(!is_epoch_ms(EPOCH_MS_MIN - 1.0));
    assert!(!is_epoch_ms(EPOCH_MS_MAX + 1.0));
    let ts = EPOCH_MS_MIN;
    let expected = Local
        .timestamp_millis_opt(ts as i64)
        .single()
        .expect("2000-01-01 is in range")
        .format("%H:%M:%S")
        .to_string();
    assert_eq!(format_epoch_ms(ts), expected);
    assert_eq!(expected.len(), 8);
}

#[test]
fn axis_tick_uses_the_clock_for_epoch_ms_and_truncates_long_text() {
    assert_eq!(format_axis_tick(&ChartValue::Null), "");
    assert_eq!(format_axis_tick(&ChartValue::Number(12.0)), "12");
    assert_eq!(
        format_axis_tick(&ChartValue::Number(EPOCH_MS_MIN)),
        format_epoch_ms(EPOCH_MS_MIN)
    );
    assert_eq!(format_axis_tick(&ChartValue::Bool(true)), "true");
    let long = "abcdefghijklmnopqrstuvwxyz";
    assert_eq!(long.chars().count(), 26);
    assert_eq!(
        format_axis_tick(&ChartValue::String(long.into())),
        "abcdefghijklmnopqr…"
    );
    assert_eq!(
        format_axis_tick(&ChartValue::String("short".into())),
        "short"
    );
}
