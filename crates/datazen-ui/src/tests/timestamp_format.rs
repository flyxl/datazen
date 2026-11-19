//! Independent audit for T272 (the valuePresentation timestampFormat port).
//!
//! Every assertion comes from re-reading
//! `packages/frontend/src/lib/valuePresentation/timestampFormat.ts` and its
//! React test `__tests__/timestampFormat.test.ts` — the three output
//! formats, source-precision preservation, extended years, DST-day
//! offsets, historical LMT, the BUG-003 remainder regression, the locale
//! structural substrings, and the zone-name validation layers.
//!
//! Type-level elisions (module header documents them): non-string locales,
//! `null`/`{kind:'mars'}` zone specs and unrecognized format values are
//! unrepresentable against `&str` / `TimeZoneSpec` / `TimestampOutputFormat`.
//! The React `process.env.TZ` pinning for `{kind:'local'}` becomes an
//! equivalence check against `iana_time_zone::get_timezone()` (the same
//! answer source the port uses); the machine zone itself stays whatever
//! the runner's environment says.

use crate::support::value_presentation::temporal_math::{
    InstantParts, TemporalFormatError, UnixUnit, UnixValue, parse_unix_value,
    parse_zoned_timestamp, InstantParseResult,
};
use crate::support::value_presentation::timestamp_format::{
    TimestampFormatOptions, TimestampOutputFormat, TimeZoneSpec, format_timestamp_output,
    is_valid_iana_time_zone_name, resolve_time_zone_name,
};

// ---------------------------------------------------------------------------
// Helpers (mirroring the React test file)
// ---------------------------------------------------------------------------

fn instant_of(text: &str) -> InstantParts {
    match parse_zoned_timestamp(text) {
        InstantParseResult::Ok(instant) => instant,
        InstantParseResult::Err(reason) => {
            panic!("parse_zoned_timestamp({text:?}) failed: {reason:?}")
        }
    }
}

fn unix_of(value: UnixValue<'_>, unit: UnixUnit) -> InstantParts {
    match parse_unix_value(value, unit) {
        InstantParseResult::Ok(instant) => instant,
        InstantParseResult::Err(reason) => panic!("parse_unix_value failed: {reason:?}"),
    }
}

fn fmt(
    instant: &InstantParts,
    format: TimestampOutputFormat,
    display_time_zone: &TimeZoneSpec,
) -> Result<String, TemporalFormatError> {
    fmt_locale(instant, format, display_time_zone, "en-US")
}

fn fmt_locale(
    instant: &InstantParts,
    format: TimestampOutputFormat,
    display_time_zone: &TimeZoneSpec,
    locale: &str,
) -> Result<String, TemporalFormatError> {
    let options = TimestampFormatOptions { format, display_time_zone, locale };
    format_timestamp_output(instant, &options)
}

fn text_of(result: Result<String, TemporalFormatError>) -> String {
    match result {
        Ok(text) => text,
        Err(reason) => panic!("formatting failed: {reason:?}"),
    }
}

fn shanghai() -> TimeZoneSpec {
    TimeZoneSpec::Iana { name: "Asia/Shanghai".to_string() }
}

fn new_york() -> TimeZoneSpec {
    TimeZoneSpec::Iana { name: "America/New_York".to_string() }
}

fn kathmandu() -> TimeZoneSpec {
    TimeZoneSpec::Iana { name: "Asia/Kathmandu".to_string() }
}

const UTC_SPEC: TimeZoneSpec = TimeZoneSpec::Utc;

// ---------------------------------------------------------------------------
// iso — UTC output with source precision
// ---------------------------------------------------------------------------

#[test]
fn iso_emits_utc_with_z_and_no_invented_fraction() {
    let anchor = instant_of("2026-01-15T14:30:00Z");
    // The display time zone must not influence iso output.
    assert_eq!(text_of(fmt(&anchor, TimestampOutputFormat::Iso, &shanghai())), "2026-01-15T14:30:00Z");
    assert_eq!(text_of(fmt(&anchor, TimestampOutputFormat::Iso, &new_york())), "2026-01-15T14:30:00Z");
    assert_eq!(
        text_of(fmt(&unix_of(UnixValue::Number(0.0), UnixUnit::Milliseconds), TimestampOutputFormat::Iso, &UTC_SPEC)),
        "1970-01-01T00:00:00Z"
    );
}

#[test]
fn iso_preserves_source_fraction_digit_count() {
    for (text, expected) in [
        ("2026-01-15T14:30:00.123Z", "2026-01-15T14:30:00.123Z"),
        ("2026-01-15T14:30:00.100Z", "2026-01-15T14:30:00.100Z"),
        ("2026-01-15T14:30:00.123456Z", "2026-01-15T14:30:00.123456Z"),
        ("2026-01-15T14:30:00.123456789Z", "2026-01-15T14:30:00.123456789Z"),
        ("2026-01-15T14:30:00.000000000Z", "2026-01-15T14:30:00.000000000Z"),
    ] {
        assert_eq!(
            text_of(fmt(&instant_of(text), TimestampOutputFormat::Iso, &UTC_SPEC)),
            expected,
            "source {text}"
        );
    }
}

#[test]
fn iso_derives_minimal_exact_digits_when_fraction_digits_absent() {
    // 123000 ns of a second → six digits (…000123); 1 ns → nine.
    let micro = InstantParts {
        epoch_ms: 1_768_487_400_000,
        sub_second_nanos: 123_000,
        fraction_digits: None,
    };
    assert_eq!(
        text_of(fmt(&micro, TimestampOutputFormat::Iso, &UTC_SPEC)),
        "2026-01-15T14:30:00.000123Z"
    );
    let nano = InstantParts {
        epoch_ms: 1_768_487_400_000,
        sub_second_nanos: 1,
        fraction_digits: None,
    };
    assert_eq!(
        text_of(fmt(&nano, TimestampOutputFormat::Iso, &UTC_SPEC)),
        "2026-01-15T14:30:00.000000001Z"
    );
    // Zero nanos with absent digits → no fraction at all (the derivation's
    // zero branch; the React suite reaches it only through parser-supplied
    // explicit zeros, so it is pinned directly here).
    let whole = InstantParts {
        epoch_ms: 1_768_487_400_000,
        sub_second_nanos: 0,
        fraction_digits: None,
    };
    assert_eq!(
        text_of(fmt(&whole, TimestampOutputFormat::Iso, &UTC_SPEC)),
        "2026-01-15T14:30:00Z"
    );
}

#[test]
fn iso_round_trips_negative_and_extended_year_instants() {
    for (text, expected) in [
        ("1969-12-31T23:59:59.123456Z", "1969-12-31T23:59:59.123456Z"),
        ("0050-01-01T00:00:00Z", "0050-01-01T00:00:00Z"),
        ("0000-01-01T00:00:00Z", "0000-01-01T00:00:00Z"),
        ("-0050-01-01T00:00:00Z", "-0050-01-01T00:00:00Z"),
        ("+10000-01-01T00:00:00Z", "+10000-01-01T00:00:00Z"),
    ] {
        assert_eq!(
            text_of(fmt(&instant_of(text), TimestampOutputFormat::Iso, &UTC_SPEC)),
            expected,
            "source {text}"
        );
    }
}

#[test]
fn iso_formats_unix_seconds_and_milliseconds_as_utc() {
    assert_eq!(
        text_of(fmt(&unix_of(UnixValue::Number(1_705_316_400.0), UnixUnit::Seconds), TimestampOutputFormat::Iso, &UTC_SPEC)),
        "2024-01-15T11:00:00Z"
    );
    assert_eq!(
        text_of(fmt(&unix_of(UnixValue::Number(-1.0), UnixUnit::Seconds), TimestampOutputFormat::Iso, &UTC_SPEC)),
        "1969-12-31T23:59:59Z"
    );
}

// ---------------------------------------------------------------------------
// iso-tz — target-zone wall time with the actual offset
// ---------------------------------------------------------------------------

#[test]
fn iso_tz_converts_prd_anchor_to_utc8() {
    assert_eq!(
        text_of(fmt(&instant_of("2026-01-15T14:30:00Z"), TimestampOutputFormat::IsoTz, &shanghai())),
        "2026-01-15T22:30:00+08:00"
    );
}

#[test]
fn iso_tz_renders_unix_anchor_in_shanghai() {
    // String and number inputs agree.
    assert_eq!(
        text_of(fmt(&unix_of(UnixValue::Text("1705316400000"), UnixUnit::Milliseconds), TimestampOutputFormat::IsoTz, &shanghai())),
        "2024-01-15T19:00:00+08:00"
    );
    assert_eq!(
        text_of(fmt(&unix_of(UnixValue::Number(1_705_316_400_000.0), UnixUnit::Milliseconds), TimestampOutputFormat::IsoTz, &shanghai())),
        "2024-01-15T19:00:00+08:00"
    );
}

#[test]
fn iso_tz_spells_offset_explicitly_for_utc_spec() {
    assert_eq!(
        text_of(fmt(&instant_of("2026-01-15T14:30:00Z"), TimestampOutputFormat::IsoTz, &UTC_SPEC)),
        "2026-01-15T14:30:00+00:00"
    );
}

#[test]
fn iso_tz_crosses_the_date_line_when_target_day_rolls_over() {
    assert_eq!(
        text_of(fmt(&instant_of("2026-01-15T23:30:00Z"), TimestampOutputFormat::IsoTz, &shanghai())),
        "2026-01-16T07:30:00+08:00"
    );
}

#[test]
fn iso_tz_emits_negative_offsets() {
    assert_eq!(
        text_of(fmt(&instant_of("2026-01-15T14:30:00Z"), TimestampOutputFormat::IsoTz, &new_york())),
        "2026-01-15T09:30:00-05:00"
    );
}

#[test]
fn iso_tz_preserves_nanosecond_precision_through_conversion() {
    assert_eq!(
        text_of(fmt(&instant_of("2026-01-15T14:30:00.123456789Z"), TimestampOutputFormat::IsoTz, &kathmandu())),
        "2026-01-15T20:15:00.123456789+05:45"
    );
}

#[test]
fn iso_tz_uses_the_offset_in_effect_on_dst_switch_day() {
    // 2026-03-08: New York springs forward at 07:00Z (02:00→03:00 local).
    assert_eq!(
        text_of(fmt(&instant_of("2026-03-08T06:30:00Z"), TimestampOutputFormat::IsoTz, &new_york())),
        "2026-03-08T01:30:00-05:00"
    );
    assert_eq!(
        text_of(fmt(&instant_of("2026-03-08T07:30:00Z"), TimestampOutputFormat::IsoTz, &new_york())),
        "2026-03-08T03:30:00-04:00"
    );
}

#[test]
fn iso_tz_emits_hhmmss_for_historical_sub_minute_offsets() {
    // 1850 Asia/Shanghai LMT +08:05:43; -0050 New York LMT -04:56:02.
    assert_eq!(
        text_of(fmt(&instant_of("1850-01-15T12:00:00Z"), TimestampOutputFormat::IsoTz, &shanghai())),
        "1850-01-15T20:05:43+08:05:43"
    );
    assert_eq!(
        text_of(fmt(&instant_of("-0050-01-15T12:00:00Z"), TimestampOutputFormat::IsoTz, &new_york())),
        "-0050-01-15T07:03:58-04:56:02"
    );
}

#[test]
fn iso_tz_resolves_local_spec_against_machine_zone() {
    // React pins process.env.TZ and derives the expectation from Date's
    // local accessors; the Rust `local` spec resolves through
    // iana-time-zone, so the equivalence under test is: formatting with
    // {kind:'local'} equals formatting with {kind:'iana', name =
    // iana_time_zone's answer}. Holds on any machine zone.
    let anchor = instant_of("2026-01-15T14:30:00Z");
    let local_name = iana_time_zone::get_timezone().expect("zone name available");
    let via_local = text_of(fmt(&anchor, TimestampOutputFormat::IsoTz, &TimeZoneSpec::Local));
    let via_iana = text_of(fmt(
        &anchor,
        TimestampOutputFormat::IsoTz,
        &TimeZoneSpec::Iana { name: local_name.clone() },
    ));
    assert_eq!(via_local, via_iana, "local spec must resolve to {local_name}");
}

// ---------------------------------------------------------------------------
// BUG-003 — the millisecond remainder survives all three formats
// ---------------------------------------------------------------------------

#[test]
fn bug003_keeps_123_in_all_three_formats() {
    let instant = unix_of(UnixValue::Number(1_705_316_400_123.0), UnixUnit::Milliseconds);
    assert_eq!(
        text_of(fmt(&instant, TimestampOutputFormat::Iso, &shanghai())),
        "2024-01-15T11:00:00.123Z"
    );
    assert_eq!(
        text_of(fmt(&instant, TimestampOutputFormat::IsoTz, &shanghai())),
        "2024-01-15T19:00:00.123+08:00"
    );
    // 19:00 wall in 12-hour en-US is "07:00:00 PM" (2-digit hour).
    let locale = text_of(fmt_locale(&instant, TimestampOutputFormat::Locale, &shanghai(), "en-US"));
    assert!(
        locale.contains("07:00:00.123 "),
        "locale text {locale:?} must keep .123"
    );
}

#[test]
fn bug003_keeps_999_for_negative_millisecond_value() {
    let instant = unix_of(UnixValue::Number(-1.0), UnixUnit::Milliseconds);
    assert_eq!(
        text_of(fmt(&instant, TimestampOutputFormat::Iso, &shanghai())),
        "1969-12-31T23:59:59.999Z"
    );
    assert_eq!(
        text_of(fmt(&instant, TimestampOutputFormat::IsoTz, &shanghai())),
        "1970-01-01T07:59:59.999+08:00"
    );
}

// ---------------------------------------------------------------------------
// locale — explicit locale + time zone (structural substrings, as React)
// ---------------------------------------------------------------------------

#[test]
fn locale_honors_explicit_locale_and_time_zone() {
    let anchor = instant_of("2026-01-15T14:30:00Z");
    let en_us = text_of(fmt_locale(&anchor, TimestampOutputFormat::Locale, &shanghai(), "en-US"));
    assert!(en_us.contains("10:30:00 PM"), "en-US: {en_us}");
    assert!(en_us.contains("GMT+8"), "en-US: {en_us}");

    let de_de = text_of(fmt_locale(&anchor, TimestampOutputFormat::Locale, &shanghai(), "de-DE"));
    assert!(de_de.contains("22:30:00"), "de-DE: {de_de}");
    assert!(!de_de.contains("PM"), "de-DE: {de_de}");
    assert!(de_de.contains("GMT+8"), "de-DE: {de_de}");

    let zh_cn = text_of(fmt_locale(&anchor, TimestampOutputFormat::Locale, &shanghai(), "zh-CN"));
    assert!(zh_cn.contains("22:30:00"), "zh-CN: {zh_cn}");
}

#[test]
fn locale_reflects_target_zone_in_wall_time_and_marker() {
    let anchor = instant_of("2026-01-15T14:30:00Z");
    let new_york = text_of(fmt_locale(&anchor, TimestampOutputFormat::Locale, &new_york(), "en-US"));
    assert!(new_york.contains("09:30:00 AM"), "NY: {new_york}");
    assert!(new_york.contains("GMT-5"), "NY: {new_york}");
}

#[test]
fn locale_truncates_sub_millisecond_precision_to_3_digits() {
    let nano = instant_of("2026-01-15T14:30:00.123456789Z");
    let en_us = text_of(fmt_locale(&nano, TimestampOutputFormat::Locale, &shanghai(), "en-US"));
    assert!(en_us.contains("10:30:00.123 "), "en-US: {en_us}");
    assert!(!en_us.contains(".1234"), "en-US: {en_us}");

    let de_de = text_of(fmt_locale(&nano, TimestampOutputFormat::Locale, &shanghai(), "de-DE"));
    assert!(de_de.contains("22:30:00,123 "), "de-DE: {de_de}");
}

#[test]
fn locale_appends_no_fraction_when_source_has_none() {
    let anchor = instant_of("2026-01-15T14:30:00Z");
    let en_us = text_of(fmt_locale(&anchor, TimestampOutputFormat::Locale, &shanghai(), "en-US"));
    assert!(!en_us.contains("30:00."), "no fraction expected: {en_us}");
    assert!(!en_us.contains("30:00,"), "no fraction expected: {en_us}");
}

#[test]
fn locale_renders_zero_millisecond_part_for_sub_milli_nanos_only() {
    // epochMs on a whole second + 500000 ns: the fraction option is on but
    // the millisecond part is zero → .000 (truncation toward the past).
    let instant = InstantParts {
        epoch_ms: 1_768_487_400_000,
        sub_second_nanos: 500_000,
        fraction_digits: None,
    };
    let en_us = text_of(fmt_locale(&instant, TimestampOutputFormat::Locale, &shanghai(), "en-US"));
    assert!(en_us.contains("10:30:00.000 "), "expected .000: {en_us}");
}

#[test]
fn locale_resolves_local_spec_against_machine_zone() {
    let anchor = instant_of("2026-01-15T14:30:00Z");
    let local_name = iana_time_zone::get_timezone().expect("zone name available");
    let via_local =
        text_of(fmt_locale(&anchor, TimestampOutputFormat::Locale, &TimeZoneSpec::Local, "en-US"));
    let via_iana = text_of(fmt_locale(
        &anchor,
        TimestampOutputFormat::Locale,
        &TimeZoneSpec::Iana { name: local_name.clone() },
        "en-US",
    ));
    assert_eq!(via_local, via_iana, "local spec must resolve to {local_name}");
}

#[test]
fn locale_rejects_empty_locales() {
    let anchor = instant_of("2026-01-15T14:30:00Z");
    assert_eq!(
        fmt_locale(&anchor, TimestampOutputFormat::Locale, &shanghai(), ""),
        Err(TemporalFormatError::InvalidLocale)
    );
}

#[test]
fn locale_falls_back_to_default_language_for_data_less_tags() {
    // 'zzz' is structurally valid → ICU falls back rather than failing.
    let anchor = instant_of("2026-01-15T14:30:00Z");
    assert!(fmt_locale(&anchor, TimestampOutputFormat::Locale, &shanghai(), "zzz").is_ok());
}

// ---------------------------------------------------------------------------
// Error paths
// ---------------------------------------------------------------------------

#[test]
fn errors_fail_every_format_for_invalid_iana_zone() {
    let anchor = instant_of("2026-01-15T14:30:00Z");
    let bogus = TimeZoneSpec::Iana { name: "Mars/Olympus".to_string() };
    for format in [
        TimestampOutputFormat::Iso,
        TimestampOutputFormat::IsoTz,
        TimestampOutputFormat::Locale,
    ] {
        assert_eq!(
            fmt(&anchor, format, &bogus),
            Err(TemporalFormatError::InvalidTimeZone),
            "format {format:?}"
        );
    }
}

#[test]
fn errors_fail_for_zone_name_with_whitespace() {
    let anchor = instant_of("2026-01-15T14:30:00Z");
    let padded = TimeZoneSpec::Iana { name: "Asia/Shanghai ".to_string() };
    assert_eq!(
        fmt(&anchor, TimestampOutputFormat::IsoTz, &padded),
        Err(TemporalFormatError::InvalidTimeZone)
    );
}

#[test]
fn errors_validate_the_instant_before_formatting() {
    let bad_nanos = InstantParts {
        epoch_ms: 0,
        sub_second_nanos: 1_000_000,
        fraction_digits: None,
    };
    assert_eq!(
        fmt(&bad_nanos, TimestampOutputFormat::Iso, &UTC_SPEC),
        Err(TemporalFormatError::InvalidInstant)
    );
    let out_of_range = InstantParts {
        epoch_ms: 3_093_559_603_200_000,
        sub_second_nanos: 0,
        fraction_digits: None,
    };
    assert_eq!(
        fmt(&out_of_range, TimestampOutputFormat::Iso, &UTC_SPEC),
        Err(TemporalFormatError::OutOfRange)
    );
}

// ---------------------------------------------------------------------------
// resolveTimeZoneName
// ---------------------------------------------------------------------------

#[test]
fn resolve_maps_utc_to_fixed_name() {
    assert_eq!(resolve_time_zone_name(&TimeZoneSpec::Utc), Ok("UTC".to_string()));
}

#[test]
fn resolve_maps_local_to_machine_zone() {
    // React pins TZ and asserts Intl's resolved name; the port's answer
    // source is iana-time-zone, so the two must agree by construction.
    let expected = iana_time_zone::get_timezone().expect("zone name available");
    assert_eq!(resolve_time_zone_name(&TimeZoneSpec::Local), Ok(expected));
}

#[test]
fn resolve_passes_syntactically_valid_iana_names_through() {
    assert_eq!(
        resolve_time_zone_name(&TimeZoneSpec::Iana { name: "Asia/Shanghai".to_string() }),
        Ok("Asia/Shanghai".to_string())
    );
    // Format-only stage: a syntactically valid but non-existent zone still
    // resolves here; full validation happens at format time.
    assert_eq!(
        resolve_time_zone_name(&TimeZoneSpec::Iana { name: "Mars/Olympus".to_string() }),
        Ok("Mars/Olympus".to_string())
    );
}

#[test]
fn resolve_rejects_malformed_iana_names() {
    assert_eq!(
        resolve_time_zone_name(&TimeZoneSpec::Iana { name: String::new() }),
        Err(TemporalFormatError::InvalidTimeZone)
    );
    assert_eq!(
        resolve_time_zone_name(&TimeZoneSpec::Iana { name: "Foo Bar".to_string() }),
        Err(TemporalFormatError::InvalidTimeZone)
    );
}

// ---------------------------------------------------------------------------
// IANA name format validation
// ---------------------------------------------------------------------------

#[test]
fn iana_name_accepts_documented_charset_and_length() {
    for name in [
        "UTC",
        "GMT",
        "Asia/Shanghai",
        "America/New_York",
        "Etc/GMT+8",
        "Etc/GMT-14",
        "Asia/Kolkata",
    ] {
        assert!(is_valid_iana_time_zone_name(name), "name {name}");
    }
}

#[test]
fn iana_name_rejects_empty_whitespace_non_ascii_oversized() {
    assert!(!is_valid_iana_time_zone_name(""));
    assert!(!is_valid_iana_time_zone_name("Asia/Shanghai "));
    assert!(!is_valid_iana_time_zone_name("Asia/上海"));
    assert!(!is_valid_iana_time_zone_name("Asia\nShanghai"));
    assert!(!is_valid_iana_time_zone_name(&"A".repeat(101)));
    // Lowercase passes the format layer (full validity is delegated).
    assert!(is_valid_iana_time_zone_name("asia/shanghai"));
}
