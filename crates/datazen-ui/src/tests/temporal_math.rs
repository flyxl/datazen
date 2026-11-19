//! Independent audit for T207 (the valuePresentation temporalMath port).
//!
//! Every assertion comes from re-reading
//! `packages/frontend/src/lib/valuePresentation/temporalMath.ts` and its
//! React test `__tests__/temporalMath.test.ts` (96 cases) — the PRD anchor
//! instants, the floor invariant for negative instants, fraction-digit
//! preservation, the ±100000-year support bounds, DST ambiguity refusal,
//! historical LMT offsets and the offset text helpers.
//!
//! Type-level elisions (documented in the module header, not retestable
//! against Rust types): `not-a-string` / `not-a-number-or-string` /
//! non-object wall input, fractional or negative numeric fields
//! (`epochMs: 1.5`, `nanos: -1`, `nanos: 0.5`, `fractionDigits: -1`) — the
//! `&str` / `UnixValue` / `WallComponents` / `InstantParts` types cannot
//! carry them. The JS `process.env.TZ` pinning test is likewise structural
//! here: the wall-clock parser takes no ambient zone input at all (no
//! `chrono::Local` read exists in the module), so the three samples are
//! pinned to their exact components instead.

use std::cmp::Ordering;

use crate::support::value_presentation::temporal_math::{
    format_offset_minutes, format_offset_seconds, instant_to_wall_parts, is_valid_time_zone_name,
    parse_unix_value, parse_wall_clock_text, parse_zoned_timestamp, validate_instant_parts,
    wall_clock_to_instant, compare_instants, InstantParts, InstantParseResult,
    InstantToWallError, InstantValidationError, TemporalParseError, UnixUnit, UnixValue,
    WallClockTextParse, WallComponents, WallDate, WallTime, WallToInstantError,
    MAX_SUPPORTED_EPOCH_MS, MIN_SUPPORTED_EPOCH_MS,
};

// ---------------------------------------------------------------------------
// Helpers (mirroring the React test file's own helpers)
// ---------------------------------------------------------------------------

/// Parse a zoned timestamp expected to succeed; fail loud with the input.
fn instant_of(text: &str) -> InstantParts {
    match parse_zoned_timestamp(text) {
        InstantParseResult::Ok(instant) => instant,
        InstantParseResult::Err(reason) => {
            panic!("parse_zoned_timestamp({text:?}) failed: {reason:?}")
        }
    }
}

/// Parse a Unix value expected to succeed; fail loud.
fn unix_of(value: UnixValue<'_>, unit: UnixUnit) -> InstantParts {
    match parse_unix_value(value, unit) {
        InstantParseResult::Ok(instant) => instant,
        InstantParseResult::Err(reason) => panic!("parse_unix_value failed: {reason:?}"),
    }
}

/// Wall components with the React helper's neutral default (2026-01-15
/// 00:00:00, no fraction digits).
fn wall() -> WallComponents {
    WallComponents {
        year: 2026,
        month: 1,
        day: 15,
        hour: 0,
        minute: 0,
        second: 0,
        nanos: 0,
        fraction_digits: None,
    }
}

fn parts(epoch_ms: i64, sub_second_nanos: u32) -> InstantParts {
    InstantParts { epoch_ms, sub_second_nanos, fraction_digits: None }
}

/// Exact ordering reference: the instant as a single integer nanosecond
/// count (React's BigInt `instantKey`).
fn instant_key(instant: &InstantParts) -> i128 {
    i128::from(instant.epoch_ms) * 1_000_000 + i128::from(instant.sub_second_nanos)
}

// ---------------------------------------------------------------------------
// parseZonedTimestamp — valid forms
// ---------------------------------------------------------------------------

#[test]
fn zoned_parses_the_prd_anchor_instant() {
    assert_eq!(
        parse_zoned_timestamp("2026-01-15T14:30:00Z"),
        Ok(InstantParts {
            epoch_ms: 1_768_487_400_000,
            sub_second_nanos: 0,
            fraction_digits: Some(0),
        })
    );
}

#[test]
fn zoned_accepts_lowercase_tz_separators_and_space_style() {
    assert_eq!(instant_of("2026-01-15t14:30:00z").epoch_ms, 1_768_487_400_000);
    assert_eq!(instant_of("2026-01-15 14:30:00Z").epoch_ms, 1_768_487_400_000);
}

#[test]
fn zoned_applies_hh_mm_offsets() {
    assert_eq!(
        instant_of("2026-01-15T22:30:00+08:00"),
        InstantParts {
            epoch_ms: 1_768_487_400_000,
            sub_second_nanos: 0,
            fraction_digits: Some(0),
        }
    );
}

#[test]
fn zoned_applies_compact_hhmm_offsets() {
    assert_eq!(instant_of("2026-01-15T09:30:00-0500").epoch_ms, 1_768_487_400_000);
}

#[test]
fn zoned_applies_non_whole_hour_offsets() {
    assert_eq!(instant_of("2026-01-15T20:15:00+05:45").epoch_ms, 1_768_487_400_000);
    assert_eq!(instant_of("2026-01-15T20:15:00+0545").epoch_ms, 1_768_487_400_000);
}

#[test]
fn zoned_accepts_postgres_short_offsets() {
    assert_eq!(instant_of("2026-01-15T22:30:00+08").epoch_ms, 1_768_487_400_000);
    assert_eq!(instant_of("2026-01-15T09:30:00-05").epoch_ms, 1_768_487_400_000);
}

#[test]
fn zoned_treats_negative_zero_offset_as_zero() {
    assert_eq!(instant_of("2026-01-15T14:30:00-00:00").epoch_ms, 1_768_487_400_000);
}

#[test]
fn zoned_accepts_seconds_omitted_shape() {
    // 14:30 wall at +08:00 → 06:30Z same day.
    assert_eq!(
        instant_of("2026-01-15T14:30+08:00"),
        InstantParts {
            epoch_ms: 1_768_458_600_000,
            sub_second_nanos: 0,
            fraction_digits: Some(0),
        }
    );
}

#[test]
fn zoned_keeps_3_digit_fraction_in_epoch_ms() {
    assert_eq!(
        instant_of("2026-01-15T14:30:00.123Z"),
        InstantParts {
            epoch_ms: 1_768_487_400_123,
            sub_second_nanos: 0,
            fraction_digits: Some(3),
        }
    );
}

#[test]
fn zoned_splits_6_digit_fraction_across_epoch_and_nanos() {
    assert_eq!(
        instant_of("2026-01-15T14:30:00.123456Z"),
        InstantParts {
            epoch_ms: 1_768_487_400_123,
            sub_second_nanos: 456_000,
            fraction_digits: Some(6),
        }
    );
}

#[test]
fn zoned_keeps_9_digit_fraction_losslessly() {
    assert_eq!(
        instant_of("2026-01-15T14:30:00.123456789Z"),
        InstantParts {
            epoch_ms: 1_768_487_400_123,
            sub_second_nanos: 456_789,
            fraction_digits: Some(9),
        }
    );
}

#[test]
fn zoned_keeps_sub_millisecond_fractions_out_of_epoch_ms() {
    assert_eq!(
        instant_of("2026-01-15T14:30:00.000456Z"),
        InstantParts {
            epoch_ms: 1_768_487_400_000,
            sub_second_nanos: 456_000,
            fraction_digits: Some(6),
        }
    );
}

#[test]
fn zoned_preserves_all_zero_9_digit_source_precision() {
    assert_eq!(
        instant_of("2026-01-15T14:30:00.000000000Z"),
        InstantParts {
            epoch_ms: 1_768_487_400_000,
            sub_second_nanos: 0,
            fraction_digits: Some(9),
        }
    );
}

#[test]
fn zoned_satisfies_floor_invariant_for_negative_instants() {
    // -876.544 ms → epochMs floor -877, remainder 456000 ns.
    assert_eq!(
        instant_of("1969-12-31T23:59:59.123456Z"),
        InstantParts {
            epoch_ms: -877,
            sub_second_nanos: 456_000,
            fraction_digits: Some(6),
        }
    );
}

#[test]
fn zoned_accepts_leap_days_for_actual_leap_years_only() {
    assert_eq!(instant_of("2024-02-29T00:00:00Z").epoch_ms, 1_709_164_800_000);
    assert_eq!(instant_of("2000-02-29T00:00:00Z").epoch_ms, 951_782_400_000);
}

#[test]
fn zoned_maps_0_99_years_to_first_century() {
    // Date.UTC(50, …) is 1950 in JS; the module must land on astronomical
    // year 50. chrono has no such quirk — same expected value either way.
    assert_eq!(instant_of("0050-01-01T00:00:00Z").epoch_ms, -60_589_296_000_000);
    assert_eq!(instant_of("0000-01-01T00:00:00Z").epoch_ms, -62_167_219_200_000);
}

#[test]
fn zoned_accepts_signed_extended_years_in_range() {
    assert_eq!(instant_of("+10000-01-01T00:00:00Z").epoch_ms, 253_402_300_800_000);
    assert_eq!(instant_of("-0050-01-15T12:00:00Z").epoch_ms, -63_743_803_200_000);
}

// ---------------------------------------------------------------------------
// parseZonedTimestamp — invalid inputs
// ---------------------------------------------------------------------------

#[test]
fn zoned_rejects_non_timestamp_text() {
    assert_eq!(parse_zoned_timestamp("not-a-date"), Err(TemporalParseError::InvalidFormat));
}

#[test]
fn zoned_rejects_out_of_range_months() {
    assert_eq!(
        parse_zoned_timestamp("2026-13-01T00:00:00Z"),
        Err(TemporalParseError::InvalidMonth)
    );
    assert_eq!(
        parse_zoned_timestamp("2026-00-10T00:00:00Z"),
        Err(TemporalParseError::InvalidMonth)
    );
}

#[test]
fn zoned_rejects_impossible_days() {
    assert_eq!(
        parse_zoned_timestamp("2026-02-30T00:00:00Z"),
        Err(TemporalParseError::InvalidDay)
    );
    assert_eq!(
        parse_zoned_timestamp("2026-02-29T00:00:00Z"),
        Err(TemporalParseError::InvalidDay)
    );
    // 1900 is not a leap year (the 400-year exception does not hit).
    assert_eq!(
        parse_zoned_timestamp("1900-02-29T00:00:00Z"),
        Err(TemporalParseError::InvalidDay)
    );
}

#[test]
fn zoned_rejects_end_of_day_minutes_60_and_leap_seconds() {
    assert_eq!(
        parse_zoned_timestamp("2026-01-15T24:00:00Z"),
        Err(TemporalParseError::InvalidHour)
    );
    assert_eq!(
        parse_zoned_timestamp("2026-01-15T23:60:00Z"),
        Err(TemporalParseError::InvalidMinute)
    );
    assert_eq!(
        parse_zoned_timestamp("2026-01-15T23:59:60Z"),
        Err(TemporalParseError::InvalidSecond)
    );
}

#[test]
fn zoned_rejects_empty_string() {
    assert_eq!(parse_zoned_timestamp(""), Err(TemporalParseError::EmptyInput));
}

#[test]
fn zoned_rejects_fractions_beyond_9_digits() {
    assert_eq!(
        parse_zoned_timestamp("2026-01-15T14:30:00.1234567890Z"),
        Err(TemporalParseError::FractionDigitsOutOfRange)
    );
}

#[test]
fn zoned_rejects_surrounding_whitespace() {
    assert_eq!(
        parse_zoned_timestamp(" 2026-01-15T14:30:00Z"),
        Err(TemporalParseError::InvalidFormat)
    );
    assert_eq!(
        parse_zoned_timestamp("2026-01-15T14:30:00Z "),
        Err(TemporalParseError::InvalidFormat)
    );
}

#[test]
fn zoned_rejects_zone_less_date_only_and_unpadded_forms() {
    assert_eq!(
        parse_zoned_timestamp("2026-01-15T14:30:00"),
        Err(TemporalParseError::InvalidFormat)
    );
    assert_eq!(parse_zoned_timestamp("2026-01-15"), Err(TemporalParseError::InvalidFormat));
    assert_eq!(
        parse_zoned_timestamp("2026-1-15T14:30:00Z"),
        Err(TemporalParseError::InvalidFormat)
    );
}

#[test]
fn zoned_rejects_out_of_range_zone_offsets() {
    assert_eq!(
        parse_zoned_timestamp("2026-01-15T14:30:00+24:00"),
        Err(TemporalParseError::InvalidOffset)
    );
    // +05:60 combines to a whole hour — the parsed fields must still reject.
    assert_eq!(
        parse_zoned_timestamp("2026-01-15T14:30:00+05:60"),
        Err(TemporalParseError::InvalidOffset)
    );
}

#[test]
fn zoned_rejects_years_beyond_support_range() {
    assert_eq!(
        parse_zoned_timestamp("+100001-01-01T00:00:00Z"),
        Err(TemporalParseError::OutOfRange)
    );
    assert_eq!(
        parse_zoned_timestamp("-100001-01-01T00:00:00Z"),
        Err(TemporalParseError::OutOfRange)
    );
    // Six-digit garbage year: beyond chrono's range → the NaN path.
    assert_eq!(
        parse_zoned_timestamp("999999-12-31T23:59:59.999Z"),
        Err(TemporalParseError::OutOfRange)
    );
}

#[test]
fn zoned_accepts_exact_support_range_boundaries() {
    assert_eq!(instant_of("+100000-12-31T23:59:59.999Z").epoch_ms, MAX_SUPPORTED_EPOCH_MS);
    assert_eq!(instant_of("-100000-01-01T00:00:00Z").epoch_ms, MIN_SUPPORTED_EPOCH_MS);
}

// ---------------------------------------------------------------------------
// parseUnixValue — number input
// ---------------------------------------------------------------------------

#[test]
fn unix_parses_zero_as_epoch() {
    for unit in [UnixUnit::Milliseconds, UnixUnit::Seconds] {
        assert_eq!(
            parse_unix_value(UnixValue::Number(0.0), unit),
            Ok(InstantParts {
                epoch_ms: 0,
                sub_second_nanos: 0,
                fraction_digits: Some(0),
            })
        );
    }
}

#[test]
fn unix_parses_negative_values_as_pre_1970() {
    assert_eq!(
        parse_unix_value(UnixValue::Number(-1.0), UnixUnit::Seconds),
        Ok(InstantParts {
            epoch_ms: -1000,
            sub_second_nanos: 0,
            fraction_digits: Some(0),
        })
    );
    assert_eq!(
        unix_of(UnixValue::Number(-1.0), UnixUnit::Seconds),
        unix_of(UnixValue::Number(-1000.0), UnixUnit::Milliseconds)
    );
}

#[test]
fn unix_converts_seconds_to_milliseconds_exactly() {
    // PRD anchor: 1705316400 s = 2024-01-15T11:00:00Z.
    assert_eq!(
        parse_unix_value(UnixValue::Number(1_705_316_400.0), UnixUnit::Seconds),
        Ok(InstantParts {
            epoch_ms: 1_705_316_400_000,
            sub_second_nanos: 0,
            fraction_digits: Some(0),
        })
    );
}

#[test]
fn unix_rejects_numbers_that_are_not_safe_integers() {
    for bad in [
        9_007_199_254_740_992.0,  // 2^53
        -9_007_199_254_740_992.0, // -(2^53)
        1.5,
        f64::NAN,
        f64::INFINITY,
    ] {
        assert_eq!(
            parse_unix_value(UnixValue::Number(bad), UnixUnit::Milliseconds),
            Err(TemporalParseError::NotSafeInteger)
        );
    }
}

// ---------------------------------------------------------------------------
// parseUnixValue — BUG-003 millisecond remainder
// ---------------------------------------------------------------------------

#[test]
fn unix_reports_3_digits_for_nonzero_millisecond_remainder() {
    assert_eq!(
        parse_unix_value(UnixValue::Number(1_705_316_400_123.0), UnixUnit::Milliseconds),
        Ok(InstantParts {
            epoch_ms: 1_705_316_400_123,
            sub_second_nanos: 0,
            fraction_digits: Some(3),
        })
    );
}

#[test]
fn unix_reports_3_digits_for_negative_millisecond_value() {
    // -1 ms → civil millisecond 999 within the wall second.
    assert_eq!(
        parse_unix_value(UnixValue::Number(-1.0), UnixUnit::Milliseconds),
        Ok(InstantParts { epoch_ms: -1, sub_second_nanos: 0, fraction_digits: Some(3) })
    );
}

#[test]
fn unix_reports_0_digits_when_remainder_is_zero() {
    assert_eq!(
        parse_unix_value(UnixValue::Number(1_705_316_400_000.0), UnixUnit::Milliseconds),
        Ok(InstantParts {
            epoch_ms: 1_705_316_400_000,
            sub_second_nanos: 0,
            fraction_digits: Some(0),
        })
    );
    assert_eq!(
        parse_unix_value(UnixValue::Number(1_705_316_400.0), UnixUnit::Seconds),
        Ok(InstantParts {
            epoch_ms: 1_705_316_400_000,
            sub_second_nanos: 0,
            fraction_digits: Some(0),
        })
    );
}

#[test]
fn unix_agrees_between_number_and_string_paths() {
    assert_eq!(
        parse_unix_value(UnixValue::Text("1705316400123"), UnixUnit::Milliseconds),
        Ok(InstantParts {
            epoch_ms: 1_705_316_400_123,
            sub_second_nanos: 0,
            fraction_digits: Some(3),
        })
    );
    assert_eq!(
        parse_unix_value(UnixValue::Text("-1"), UnixUnit::Milliseconds),
        Ok(InstantParts { epoch_ms: -1, sub_second_nanos: 0, fraction_digits: Some(3) })
    );
}

// ---------------------------------------------------------------------------
// parseUnixValue — string input
// ---------------------------------------------------------------------------

#[test]
fn unix_accepts_integer_syntax_with_explicit_sign() {
    // BUG-003: +12 ms has a nonzero remainder → 3 digits.
    assert_eq!(
        parse_unix_value(UnixValue::Text("+12"), UnixUnit::Milliseconds),
        Ok(InstantParts { epoch_ms: 12, sub_second_nanos: 0, fraction_digits: Some(3) })
    );
    assert_eq!(
        parse_unix_value(UnixValue::Text("-0"), UnixUnit::Seconds),
        Ok(InstantParts { epoch_ms: 0, sub_second_nanos: 0, fraction_digits: Some(0) })
    );
}

#[test]
fn unix_parses_the_prd_millisecond_anchor_exactly() {
    assert_eq!(
        unix_of(UnixValue::Text("1705316400000"), UnixUnit::Milliseconds).epoch_ms,
        1_705_316_400_000
    );
    assert_eq!(
        unix_of(UnixValue::Number(1_705_316_400_000.0), UnixUnit::Milliseconds).epoch_ms,
        1_705_316_400_000
    );
}

#[test]
fn unix_keeps_full_precision_for_in_range_strings() {
    assert_eq!(
        unix_of(UnixValue::Text("3093559603199999"), UnixUnit::Milliseconds).epoch_ms,
        MAX_SUPPORTED_EPOCH_MS
    );
    assert_eq!(
        unix_of(UnixValue::Text("-3217862419200000"), UnixUnit::Milliseconds).epoch_ms,
        MIN_SUPPORTED_EPOCH_MS
    );
}

#[test]
fn unix_rejects_non_integer_string_syntax() {
    for bad in ["12.5", " 12", "12 ", "", "1e3", "0x1f", "+ 12", "12,000"] {
        assert_eq!(
            parse_unix_value(UnixValue::Text(bad), UnixUnit::Seconds),
            Err(TemporalParseError::InvalidIntegerSyntax)
        );
    }
}

#[test]
fn unix_rejects_values_beyond_supported_range() {
    assert_eq!(
        parse_unix_value(UnixValue::Text("3162240000000"), UnixUnit::Seconds),
        Err(TemporalParseError::OutOfRange)
    );
    assert_eq!(
        parse_unix_value(UnixValue::Text("3093559603200000"), UnixUnit::Milliseconds),
        Err(TemporalParseError::OutOfRange)
    );
    assert_eq!(
        parse_unix_value(UnixValue::Text("-3217862419200001"), UnixUnit::Milliseconds),
        Err(TemporalParseError::OutOfRange)
    );
}

// ---------------------------------------------------------------------------
// parseWallClockText — shapes
// ---------------------------------------------------------------------------

#[test]
fn wall_text_parses_date_only() {
    assert_eq!(
        parse_wall_clock_text("2026-03-01"),
        WallClockTextParse::Date(WallDate { year: 2026, month: 3, day: 1 })
    );
}

#[test]
fn wall_text_parses_datetime_with_all_three_separators() {
    let expected = WallClockTextParse::Datetime(WallComponents {
        year: 2026,
        month: 3,
        day: 1,
        hour: 0,
        minute: 15,
        second: 30,
        nanos: 123_000_000,
        fraction_digits: Some(3),
    });
    assert_eq!(parse_wall_clock_text("2026-03-01 00:15:30.123"), expected);
    assert_eq!(parse_wall_clock_text("2026-03-01T00:15:30.123"), expected);
    assert_eq!(parse_wall_clock_text("2026-03-01t00:15:30.123"), expected);
}

#[test]
fn wall_text_defaults_omitted_seconds_and_fraction() {
    assert_eq!(
        parse_wall_clock_text("2026-03-01T00:15"),
        WallClockTextParse::Datetime(WallComponents {
            year: 2026,
            month: 3,
            day: 1,
            hour: 0,
            minute: 15,
            second: 0,
            nanos: 0,
            fraction_digits: Some(0),
        })
    );
}

#[test]
fn wall_text_parses_time_only_including_minute_precision() {
    assert_eq!(
        parse_wall_clock_text("14:30"),
        WallClockTextParse::Time(WallTime {
            hour: 14,
            minute: 30,
            second: 0,
            nanos: 0,
            fraction_digits: Some(0),
        })
    );
    assert_eq!(
        parse_wall_clock_text("00:15:30.123456789"),
        WallClockTextParse::Time(WallTime {
            hour: 0,
            minute: 15,
            second: 30,
            nanos: 123_456_789,
            fraction_digits: Some(9),
        })
    );
}

// ---------------------------------------------------------------------------
// parseWallClockText — zone-freedom (React pins process.env.TZ; the Rust
// parser has no ambient-zone read at all, so the samples pin to components)
// ---------------------------------------------------------------------------

#[test]
fn wall_text_is_zone_free_by_construction() {
    // Same three samples as the React TZ-pinning test; the exact components
    // are asserted (they were already asserted above for two of them — here
    // the third, '2026-03-01 00:15:30.123', is pinned byte-exactly).
    assert_eq!(
        parse_wall_clock_text("2026-03-01 00:15:30.123"),
        WallClockTextParse::Datetime(WallComponents {
            year: 2026,
            month: 3,
            day: 1,
            hour: 0,
            minute: 15,
            second: 30,
            nanos: 123_000_000,
            fraction_digits: Some(3),
        })
    );
}

#[test]
fn wall_text_rejects_zone_designators() {
    assert_eq!(
        parse_wall_clock_text("2026-03-01T00:15:30Z"),
        WallClockTextParse::Err(TemporalParseError::InvalidFormat)
    );
    assert_eq!(
        parse_wall_clock_text("2026-03-01 00:15:30+08:00"),
        WallClockTextParse::Err(TemporalParseError::InvalidFormat)
    );
    assert_eq!(
        parse_wall_clock_text("14:30+08:00"),
        WallClockTextParse::Err(TemporalParseError::InvalidFormat)
    );
}

#[test]
fn wall_text_rejects_empty_and_non_timestamp_text() {
    assert_eq!(
        parse_wall_clock_text(""),
        WallClockTextParse::Err(TemporalParseError::EmptyInput)
    );
    assert_eq!(
        parse_wall_clock_text("not-a-date"),
        WallClockTextParse::Err(TemporalParseError::InvalidFormat)
    );
}

#[test]
fn wall_text_rejects_invalid_calendar_components() {
    assert_eq!(
        parse_wall_clock_text("2026-13-01"),
        WallClockTextParse::Err(TemporalParseError::InvalidMonth)
    );
    assert_eq!(
        parse_wall_clock_text("2026-02-30"),
        WallClockTextParse::Err(TemporalParseError::InvalidDay)
    );
    assert_eq!(
        parse_wall_clock_text("24:00"),
        WallClockTextParse::Err(TemporalParseError::InvalidHour)
    );
}

#[test]
fn wall_text_rejects_fractions_beyond_9_digits() {
    assert_eq!(
        parse_wall_clock_text("2026-03-01T00:15:30.1234567890"),
        WallClockTextParse::Err(TemporalParseError::FractionDigitsOutOfRange)
    );
}

// ---------------------------------------------------------------------------
// wallClockToInstant — normal conversion
// ---------------------------------------------------------------------------

#[test]
fn wall_to_instant_converts_utc_with_nanosecond_precision() {
    // 1969-07-20 20:17:40.123456 UTC (negative epochMs).
    let mut input = wall();
    input.year = 1969;
    input.month = 7;
    input.day = 20;
    input.hour = 20;
    input.minute = 17;
    input.second = 40;
    input.nanos = 123_456_000;
    input.fraction_digits = Some(6);
    assert_eq!(
        wall_clock_to_instant(&input, "UTC"),
        Ok(InstantParts {
            epoch_ms: -14_182_939_877,
            sub_second_nanos: 456_000,
            fraction_digits: Some(6),
        })
    );
}

#[test]
fn wall_to_instant_applies_floor_carry_for_negative_sub_second() {
    // 1969-12-31 23:59:59.765999999 UTC = -234.000001 ms → floor -235,
    // remainder 999999 ns (the documented InstantParts invariant).
    let mut input = wall();
    input.year = 1969;
    input.month = 12;
    input.day = 31;
    input.hour = 23;
    input.minute = 59;
    input.second = 59;
    input.nanos = 765_999_999;
    input.fraction_digits = Some(9);
    assert_eq!(
        wall_clock_to_instant(&input, "UTC"),
        Ok(InstantParts {
            epoch_ms: -235,
            sub_second_nanos: 999_999,
            fraction_digits: Some(9),
        })
    );
}

#[test]
fn wall_to_instant_converts_shanghai_across_the_date_line() {
    // 2026-01-16 07:30 at +08:00 → 2026-01-15T23:30Z. fractionDigits stays
    // absent when the wall input carries none (no invented zeros).
    let mut input = wall();
    input.month = 1;
    input.day = 16;
    input.hour = 7;
    input.minute = 30;
    assert_eq!(
        wall_clock_to_instant(&input, "Asia/Shanghai"),
        Ok(InstantParts {
            epoch_ms: 1_768_519_800_000,
            sub_second_nanos: 0,
            fraction_digits: None,
        })
    );
}

#[test]
fn wall_to_instant_converts_kathmandu_non_whole_minute_offset() {
    let mut input = wall();
    input.hour = 22;
    input.minute = 30;
    assert_eq!(
        wall_clock_to_instant(&input, "Asia/Kathmandu"),
        Ok(InstantParts {
            epoch_ms: 1_768_495_500_000,
            sub_second_nanos: 0,
            fraction_digits: None,
        })
    );
}

#[test]
fn wall_to_instant_round_trips_negative_instant() {
    let wall_in = WallComponents {
        year: 1969,
        month: 12,
        day: 31,
        hour: 23,
        minute: 59,
        second: 59,
        nanos: 765_999_999,
        fraction_digits: Some(9),
    };
    let to_instant = wall_clock_to_instant(&wall_in, "UTC").expect("valid conversion");
    let back = instant_to_wall_parts(&to_instant, "UTC").expect("valid readback");
    assert_eq!(back.wall, wall_in);
    assert_eq!(back.offset_seconds, 0);
    assert_eq!(back.offset_minutes, 0);
}

// ---------------------------------------------------------------------------
// wallClockToInstant — DST transitions
// ---------------------------------------------------------------------------

#[test]
fn wall_to_instant_reports_spring_forward_as_dst_invalid() {
    // America/New_York jumps 02:00 → 03:00 on 2026-03-08 (2nd Sunday).
    let mut input = wall();
    input.month = 3;
    input.day = 8;
    input.hour = 2;
    input.minute = 30;
    assert_eq!(
        wall_clock_to_instant(&input, "America/New_York"),
        Err(WallToInstantError::DstInvalid)
    );
}

#[test]
fn wall_to_instant_accepts_wall_times_around_spring_forward_gap() {
    for (hour, minute) in [(1, 30), (3, 30)] {
        let mut input = wall();
        input.month = 3;
        input.day = 8;
        input.hour = hour;
        input.minute = minute;
        assert!(wall_clock_to_instant(&input, "America/New_York").is_ok());
    }
}

#[test]
fn wall_to_instant_reports_fall_back_as_dst_ambiguous() {
    // America/New_York repeats 01:00–02:00 on 2026-11-01 (1st Sunday).
    let mut input = wall();
    input.month = 11;
    input.day = 1;
    input.hour = 1;
    input.minute = 30;
    input.nanos = 500_000_000;
    assert_eq!(
        wall_clock_to_instant(&input, "America/New_York"),
        Err(WallToInstantError::DstAmbiguous)
    );
}

#[test]
fn wall_to_instant_accepts_wall_times_around_fall_back_overlap() {
    for (hour, minute) in [(0, 30), (2, 30)] {
        let mut input = wall();
        input.month = 11;
        input.day = 1;
        input.hour = hour;
        input.minute = minute;
        assert!(wall_clock_to_instant(&input, "America/New_York").is_ok());
    }
}

#[test]
fn wall_to_instant_does_not_flag_dst_in_transition_free_zones() {
    let mut input = wall();
    input.month = 3;
    input.day = 8;
    input.hour = 2;
    input.minute = 30;
    assert!(wall_clock_to_instant(&input, "Asia/Shanghai").is_ok());
}

// ---------------------------------------------------------------------------
// wallClockToInstant — error paths
// ---------------------------------------------------------------------------

#[test]
fn wall_to_instant_rejects_invalid_zone_names() {
    assert_eq!(
        wall_clock_to_instant(&wall(), "Mars/Olympus"),
        Err(WallToInstantError::InvalidTimeZone)
    );
    assert_eq!(
        wall_clock_to_instant(&wall(), ""),
        Err(WallToInstantError::InvalidTimeZone)
    );
}

#[test]
fn wall_to_instant_rejects_invalid_components() {
    // Each bad override must produce InvalidComponents. Negative/fractional
    // field values are type-level (u32 cannot hold them) — see the module
    // header; every representable violation is covered here.
    let cases: Vec<WallComponents> = vec![
        WallComponents { month: 13, ..wall() },
        WallComponents { day: 32, ..wall() },
        WallComponents { hour: 24, ..wall() },
        WallComponents { minute: 60, ..wall() },
        WallComponents { second: 60, ..wall() },
        WallComponents { nanos: 1_000_000_000, ..wall() },
        WallComponents { fraction_digits: Some(10), ..wall() },
    ];
    for case in cases {
        assert_eq!(
            wall_clock_to_instant(&case, "UTC"),
            Err(WallToInstantError::InvalidComponents),
            "case: {case:?}"
        );
    }
}

#[test]
fn wall_to_instant_rejects_years_beyond_supported_range() {
    assert_eq!(
        wall_clock_to_instant(&WallComponents { year: 100_001, ..wall() }, "UTC"),
        Err(WallToInstantError::OutOfRange)
    );
    assert_eq!(
        wall_clock_to_instant(&WallComponents { year: -100_001, ..wall() }, "UTC"),
        Err(WallToInstantError::OutOfRange)
    );
}

// ---------------------------------------------------------------------------
// instantToWallParts
// ---------------------------------------------------------------------------

#[test]
fn instant_to_wall_converts_utc_2330_to_shanghai_0730() {
    let result = instant_to_wall_parts(&parts(1_768_519_800_000, 0), "Asia/Shanghai");
    let value = result.expect("valid");
    assert_eq!(value.wall, wall_with(1, 16, 7, 30));
    assert_eq!(value.offset_seconds, 28_800);
    assert_eq!(value.offset_minutes, 480);
}

#[test]
fn instant_to_wall_handles_kathmandu_offset() {
    let result = instant_to_wall_parts(&parts(1_768_487_400_000, 0), "Asia/Kathmandu");
    let value = result.expect("valid");
    assert_eq!(value.wall, wall_with(1, 15, 20, 15));
    assert_eq!(value.offset_seconds, 20_700);
    assert_eq!(value.offset_minutes, 345);
}

#[test]
fn instant_to_wall_handles_negative_whole_hour_offset() {
    let result = instant_to_wall_parts(&parts(1_768_487_400_000, 0), "America/New_York");
    let value = result.expect("valid");
    assert_eq!(value.wall, wall_with(1, 15, 9, 30));
    assert_eq!(value.offset_seconds, -18_000);
    assert_eq!(value.offset_minutes, -300);
}

#[test]
fn instant_to_wall_exposes_historical_lmt_offsets_precisely() {
    // Asia/Shanghai before 1901 used LMT +08:05:43 (29143 s); the minutes
    // convenience field truncates to 485.
    let result = instant_to_wall_parts(&parts(-3_785_572_800_000, 0), "Asia/Shanghai");
    let value = result.expect("valid");
    let mut expected = wall_with(1, 15, 20, 5);
    expected.year = 1850;
    expected.second = 43;
    assert_eq!(value.wall, expected);
    assert_eq!(value.offset_seconds, 29_143);
    assert_eq!(value.offset_minutes, 485);
}

#[test]
fn instant_to_wall_maps_bc_era_to_negative_astronomical_years() {
    // -0050-01-15T12:00:00Z in New York (LMT -04:56:02): "51 BC 07:03:58"
    // → astronomical year -50.
    let result = instant_to_wall_parts(&parts(-63_743_803_200_000, 0), "America/New_York");
    let value = result.expect("valid");
    let mut expected = wall_with(1, 15, 7, 3);
    expected.year = -50;
    expected.second = 58;
    assert_eq!(value.wall, expected);
    assert_eq!(value.offset_seconds, -17_762);
    assert_eq!(value.offset_minutes, -296);
}

#[test]
fn instant_to_wall_maps_year_0_boundary_to_2bc() {
    let result = instant_to_wall_parts(&parts(-62_167_219_200_000, 0), "America/New_York");
    let value = result.expect("valid");
    let mut expected = wall_with(12, 31, 19, 3);
    expected.year = -1;
    expected.second = 58;
    assert_eq!(value.wall, expected);
    assert_eq!(value.offset_seconds, -17_762);
    assert_eq!(value.offset_minutes, -296);
}

#[test]
fn instant_to_wall_carries_sub_second_nanos_without_borrow() {
    let instant = InstantParts {
        epoch_ms: 1_768_487_400_123,
        sub_second_nanos: 456_789,
        fraction_digits: Some(9),
    };
    let value = instant_to_wall_parts(&instant, "Asia/Shanghai").expect("valid");
    assert_eq!(value.wall.nanos, 123_456_789);
    assert_eq!(value.wall.fraction_digits, Some(9));
}

#[test]
fn instant_to_wall_keeps_civil_millisecond_carry_for_negative_epoch() {
    // epochMs -235 (civil 765 ms) + 999999 ns → 23:59:59.765999999.
    let value =
        instant_to_wall_parts(&InstantParts { epoch_ms: -235, sub_second_nanos: 999_999, ..parts(0, 0) }, "UTC")
            .expect("valid");
    let mut expected = wall_with(12, 31, 23, 59);
    expected.year = 1969;
    expected.second = 59;
    expected.nanos = 765_999_999;
    assert_eq!(value.wall, expected);
    assert_eq!(value.offset_seconds, 0);
    assert_eq!(value.offset_minutes, 0);
}

#[test]
fn instant_to_wall_keeps_minutes_consistent_with_seconds() {
    let probes: [(i64, &str); 6] = [
        (1_768_487_400_000, "Asia/Shanghai"),
        (1_768_487_400_000, "Asia/Kathmandu"),
        (1_768_487_400_000, "America/New_York"),
        (1_772_951_400_000, "America/New_York"), // 2026-03-08T06:30Z, EST side
        (1_772_955_000_000, "America/New_York"), // 2026-03-08T07:30Z, EDT side
        (-3_785_572_800_000, "Asia/Shanghai"),   // 1850 LMT
    ];
    for (epoch_ms, zone) in probes {
        let value = instant_to_wall_parts(&parts(epoch_ms, 0), zone).expect("valid");
        assert_eq!(value.offset_minutes, value.offset_seconds / 60, "zone {zone}");
    }
}

#[test]
fn instant_to_wall_rejects_invalid_zones_and_instants() {
    assert_eq!(
        instant_to_wall_parts(&parts(0, 0), "Mars/Olympus"),
        Err(InstantToWallError::InvalidTimeZone)
    );
    assert_eq!(
        instant_to_wall_parts(
            &InstantParts { epoch_ms: MAX_SUPPORTED_EPOCH_MS + 1, ..parts(0, 0) },
            "UTC"
        ),
        Err(InstantToWallError::OutOfRange)
    );
    assert_eq!(
        instant_to_wall_parts(&InstantParts { sub_second_nanos: 1_000_000, ..parts(0, 0) }, "UTC"),
        Err(InstantToWallError::InvalidInstant)
    );
}

/// `wall()` with the date/time fields the React tests override at once
/// (month, day, hour, minute); second and nanos stay zero.
fn wall_with(month: u32, day: u32, hour: u32, minute: u32) -> WallComponents {
    WallComponents { month, day, hour, minute, ..wall() }
}

// ---------------------------------------------------------------------------
// InstantParts composition math + total order
// ---------------------------------------------------------------------------

#[test]
fn composition_represents_micro_and_nano_instants_losslessly() {
    assert_eq!(
        instant_of("2026-01-15T14:30:00.123456Z"),
        InstantParts {
            epoch_ms: 1_768_487_400_123,
            sub_second_nanos: 456_000,
            fraction_digits: Some(6),
        }
    );
    assert_eq!(
        instant_of("2026-01-15T14:30:00.123456789Z"),
        InstantParts {
            epoch_ms: 1_768_487_400_123,
            sub_second_nanos: 456_789,
            fraction_digits: Some(9),
        }
    );
}

#[test]
fn composition_satisfies_floor_with_non_negative_remainder() {
    assert_eq!(
        instant_of("1969-12-31T23:59:59.999999999Z"),
        InstantParts { epoch_ms: -1, sub_second_nanos: 999_999, fraction_digits: Some(9) }
    );
    let manual = InstantParts { epoch_ms: -1235, sub_second_nanos: 999_999, fraction_digits: None };
    assert_eq!(validate_instant_parts(&manual), None);
    // Instant = -1234.000001 ms, strictly before -1234 ms.
    assert_eq!(
        compare_instants(&manual, &parts(-1234, 0)),
        Ordering::Less
    );
}

#[test]
fn compare_returns_equal_for_identical_instants_regardless_of_digits() {
    assert_eq!(compare_instants(&parts(1_768_487_400_000, 0), &parts(1_768_487_400_000, 0)), Ordering::Equal);
    // .123 and .123000 denote the same instant with different precision.
    assert_eq!(
        compare_instants(&instant_of("2026-01-15T14:30:00.123Z"), &instant_of("2026-01-15T14:30:00.123000Z")),
        Ordering::Equal
    );
}

#[test]
fn compare_orders_by_epoch_then_nanos() {
    let a = parts(1000, 0);
    let b = parts(1000, 1);
    let c = parts(1001, 0);
    assert_eq!(compare_instants(&a, &b), Ordering::Less);
    assert_eq!(compare_instants(&b, &a), Ordering::Greater);
    assert_eq!(compare_instants(&b, &c), Ordering::Less);
    assert_eq!(compare_instants(&c, &b), Ordering::Greater);
}

#[test]
fn compare_orders_correctly_across_negative_floor_boundary() {
    assert_eq!(
        compare_instants(&parts(-1235, 999_999), &parts(-1234, 0)),
        Ordering::Less
    );
    assert_eq!(compare_instants(&parts(-1234, 0), &parts(-1234, 1)), Ordering::Less);
    assert_eq!(compare_instants(&parts(-2, 999_999), &parts(-1, 0)), Ordering::Less);
}

#[test]
fn compare_is_strict_total_order_over_scrambled_set() {
    let instants = [
        parts(1_768_487_400_123, 456_789),
        parts(-235, 999_999),
        parts(0, 0),
        parts(-1, 999_999),
        parts(1_768_487_400_123, 456_788),
        parts(MAX_SUPPORTED_EPOCH_MS, 0),
        parts(MIN_SUPPORTED_EPOCH_MS, 0),
        parts(1_768_487_400_122, 999_999),
    ];
    let mut sorted = instants;
    sorted.sort_by(compare_instants);
    let mut reference = instants;
    reference.sort_by_key(instant_key);
    assert_eq!(sorted, reference);
    // Antisymmetry on neighbours.
    for pair in sorted.windows(2) {
        assert_eq!(compare_instants(&pair[0], &pair[1]), Ordering::Less);
        assert_eq!(compare_instants(&pair[1], &pair[0]), Ordering::Greater);
    }
}

// ---------------------------------------------------------------------------
// validateInstantParts
// ---------------------------------------------------------------------------

#[test]
fn validate_accepts_boundary_and_normal_values() {
    assert_eq!(validate_instant_parts(&parts(0, 0)), None);
    assert_eq!(validate_instant_parts(&parts(MIN_SUPPORTED_EPOCH_MS, 0)), None);
    assert_eq!(
        validate_instant_parts(&InstantParts {
            epoch_ms: MAX_SUPPORTED_EPOCH_MS,
            sub_second_nanos: 999_999,
            fraction_digits: None,
        }),
        None
    );
    assert_eq!(
        validate_instant_parts(&InstantParts {
            epoch_ms: 0,
            sub_second_nanos: 0,
            fraction_digits: Some(9),
        }),
        None
    );
}

#[test]
fn validate_rejects_out_of_domain_components() {
    // Fractional/negative JS values are type-level here (see header).
    assert_eq!(
        validate_instant_parts(&parts(0, 1_000_000)),
        Some(InstantValidationError::InvalidInstant)
    );
    assert_eq!(
        validate_instant_parts(&InstantParts {
            epoch_ms: 0,
            sub_second_nanos: 0,
            fraction_digits: Some(10),
        }),
        Some(InstantValidationError::InvalidInstant)
    );
}

#[test]
fn validate_rejects_out_of_range_epochs() {
    assert_eq!(
        validate_instant_parts(&parts(MAX_SUPPORTED_EPOCH_MS + 1, 0)),
        Some(InstantValidationError::OutOfRange)
    );
    assert_eq!(
        validate_instant_parts(&parts(MIN_SUPPORTED_EPOCH_MS - 1, 0)),
        Some(InstantValidationError::OutOfRange)
    );
}

// ---------------------------------------------------------------------------
// Offset text helpers
// ---------------------------------------------------------------------------

#[test]
fn offset_minutes_formats_whole_minutes() {
    assert_eq!(format_offset_minutes(480.0), "+08:00");
    assert_eq!(format_offset_minutes(-300.0), "-05:00");
    assert_eq!(format_offset_minutes(345.0), "+05:45");
}

#[test]
fn offset_minutes_formats_zero_without_z() {
    // No Z special case; only the `iso` format emits Z.
    assert_eq!(format_offset_minutes(0.0), "+00:00");
}

#[test]
fn offset_minutes_truncates_toward_zero() {
    assert_eq!(format_offset_minutes(345.9), "+05:45");
    // Math.trunc(-0.5) === -0 and -0 < 0 is false → +00:00.
    assert_eq!(format_offset_minutes(-0.5), "+00:00");
}

#[test]
fn offset_seconds_formats_whole_minutes() {
    assert_eq!(format_offset_seconds(0.0), "+00:00");
    assert_eq!(format_offset_seconds(28_800.0), "+08:00");
    assert_eq!(format_offset_seconds(-18_000.0), "-05:00");
    assert_eq!(format_offset_seconds(20_700.0), "+05:45");
}

#[test]
fn offset_seconds_formats_sub_minute_historical_offsets() {
    assert_eq!(format_offset_seconds(29_143.0), "+08:05:43");
    assert_eq!(format_offset_seconds(-17_762.0), "-04:56:02");
}

// ---------------------------------------------------------------------------
// Time-zone name validation
// ---------------------------------------------------------------------------

#[test]
fn zone_names_accept_real_iana_names() {
    for zone in [
        "UTC",
        "GMT",
        "Asia/Shanghai",
        "America/New_York",
        "Asia/Kolkata",
        "Etc/GMT+8",
        "Etc/GMT-14",
    ] {
        assert!(is_valid_time_zone_name(zone), "zone {zone} must be valid");
    }
}

#[test]
fn zone_names_are_case_insensitive_like_icu() {
    // Intl/ICU canonicalizes lowercase zone ids — locked Node behavior.
    assert!(is_valid_time_zone_name("asia/shanghai"));
}

#[test]
fn zone_names_reject_empty_unknown_and_padded() {
    assert!(!is_valid_time_zone_name(""));
    assert!(!is_valid_time_zone_name("Mars/Olympus"));
    assert!(!is_valid_time_zone_name("Asia/Shanghai "));
}

#[test]
fn zone_names_reject_non_iana_character_sets() {
    assert!(!is_valid_time_zone_name("Asia/上海"));
}

// ---------------------------------------------------------------------------
// Support-range constants
// ---------------------------------------------------------------------------

#[test]
fn support_range_covers_exactly_plus_minus_100000_years() {
    // React asserts against Date.UTC; the same instants via chrono.
    let min = chrono::NaiveDate::from_ymd_opt(-100_000, 1, 1)
        .and_then(|d| d.and_hms_milli_opt(0, 0, 0, 0))
        .map(|t| t.and_utc().timestamp_millis())
        .expect("representable");
    assert_eq!(MIN_SUPPORTED_EPOCH_MS, min);
    let max = chrono::NaiveDate::from_ymd_opt(100_001, 1, 1)
        .and_then(|d| d.and_hms_milli_opt(0, 0, 0, 0))
        .map(|t| t.and_utc().timestamp_millis() - 1)
        .expect("representable");
    assert_eq!(MAX_SUPPORTED_EPOCH_MS, max);
}
