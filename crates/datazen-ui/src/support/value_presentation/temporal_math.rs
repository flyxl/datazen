//! Port of `packages/frontend/src/lib/valuePresentation/temporalMath.ts`.
//!
//! The pure math core of the timestamp presentation: exact instants
//! (epoch-milliseconds **floored** plus an intra-millisecond nanosecond
//! remainder), strict text parsers for zoned / wall-clock / Unix inputs, and
//! the wall-clock ↔ instant conversions with DST ambiguity detection
//! (issue-32 design §8, T02). No state, no Store, no Rune tree.
//!
//! # What maps where
//!
//! - **Intl `DateTimeFormat(timeZone)` readback → `chrono-tz`.** React reads
//!   wall components out of `Intl.formatToParts` (with an explicit `era`
//!   part so "501 BC" becomes astronomical year -500). `chrono-tz`'s
//!   `DateTime<Tz>` carries astronomical years natively (negative for BC) and
//!   the same IANA transition table, including historical sub-minute LMT
//!   offsets (Asia/Shanghai 1850 → +08:05:43 = 29143 s). The era juggling is
//!   therefore unnecessary here.
//! - **`Date.UTC`'s 0–99-year quirk → nothing.** React corrects
//!   `Date.UTC(50, …) == 1950` by adding/subtracting 400 Gregorian years.
//!   `chrono`'s `NaiveDate::from_ymd_opt(50, …)` is already astronomical
//!   year 50, so [`epoch_ms_from_utc_parts`] needs no correction; the test
//!   that pins `0050-01-01` → first century still passes byte-identically.
//! - **`Date.UTC` overflow to NaN → `None`.** Six-digit garbage years
//!   (999999) exceed chrono's representable range
//!   (±262143 years); [`epoch_ms_from_utc_parts`] returns `None` and the
//!   callers report `OutOfRange`, exactly like React's NaN path.
//! - **BigInt seconds→milliseconds math → `i128`.** [`parse_unix_value`]
//!   multiplies in `i128` and range-checks against the support bounds before
//!   narrowing to `i64`; every in-range epoch ms is far inside `i64`.
//! - **The formatter cache (Map, 64-entry LRU) is elided.** It exists purely
//!   so `Intl.DateTimeFormat` construction is not repeated per cell;
//!   `chrono-tz`'s `Tz` is a `Copy` enum and `DateTime::from_timestamp_millis`
//!   has nothing to construct. Same input, same output — caching would only
//!   move the clock.
//! - **Intl's case-insensitive zone-name canonicalization** (ICU accepts
//!   `asia/shanghai`) is mirrored by [`resolve_zone`]: an exact
//!   `chrono-tz` parse first, then an ASCII case-insensitive scan of
//!   `TZ_VARIANTS`. Whitespace-padded and non-IANA names still fail.
//! - **Regexes → hand-rolled cursor parsers.** The three React regexes
//!   (`ZONED_TIMESTAMP_RE`, `DATE_ONLY_RE`, `TIME_ONLY_RE`,
//!   `WALL_DATETIME_RE`) are small enough to parse by hand, which also keeps
//!   the `regex` crate out of this crate's dependency graph. Acceptance is
//!   char-for-char the regex grammar, including the full-consumption
//!   requirement (trailing whitespace is `invalid-format`, not trimmed).
//!
//! # Known deltas (type-level guards)
//!
//! React guards against runtime garbage with `not-a-string` /
//! `not-a-number-or-string` / non-object wall input; those inputs are
//! unrepresentable against `&str` / [`UnixValue`] / [`WallComponents`], so
//! the error codes do not exist here. Every guard that survives typing is
//! kept: `sub_second_nanos > 999_999`, `fraction_digits > 9`, month/day/
//! hour/minute/second range, `nanos > 999_999_999` all report the same
//! stable codes as React. Negative and fractional field values that Rust
//! types cannot hold (`nanos: -1`, `nanos: 0.5`) are likewise type-level.
//!
//! # Precision invariants (from the React header, unchanged)
//!
//! - `epoch_ms` is the instant **floored** to whole milliseconds; for
//!   negative instants floor is one millisecond further from zero than
//!   truncation (`-1234.000567 ms → epoch_ms -1235`).
//! - `sub_second_nanos ∈ [0, 999_999]` is the intra-**millisecond** remainder
//!   (not intra-second); the decomposition is unique, so
//!   [`compare_instants`] is a strict total order by component comparison.
//! - Wall-second nanos = `positive_mod(epoch_ms, 1000) × 10⁶ +
//!   sub_second_nanos`; civil milliseconds carry, the remainder only adds.
//! - Support range: years [-100000, 100000] inclusive
//!   (`epoch_ms ∈ [MIN_SUPPORTED_EPOCH_MS, MAX_SUPPORTED_EPOCH_MS]`);
//!   outside is the stable `OutOfRange` code — no guessing, no clamping.

use std::cmp::Ordering;

use chrono::{Datelike, Offset, Timelike};
use chrono_tz::{Tz, TZ_VARIANTS};

// ---------------------------------------------------------------------------
// Support-range constants
// ---------------------------------------------------------------------------

/// -100000-01-01T00:00:00.000Z (chrono-verified: -3217862419200000).
pub(crate) const MIN_SUPPORTED_EPOCH_MS: i64 = -3_217_862_419_200_000;
/// 100000-12-31T23:59:59.999Z inclusive (chrono-verified: 3093559603199999).
pub(crate) const MAX_SUPPORTED_EPOCH_MS: i64 = 3_093_559_603_199_999;

const DAY_MS: i64 = 86_400_000;

/// JavaScript's `Number.MAX_SAFE_INTEGER` — the boundary of the `f64` input
/// path in [`parse_unix_value`].
const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0; // 2^53 - 1

// ---------------------------------------------------------------------------
// Value types
// ---------------------------------------------------------------------------

/// Exact instant: floored epoch milliseconds + intra-millisecond nanoseconds
/// + optional source fraction digits (0–9) for lossless iso re-output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct InstantParts {
    /// The instant floored to whole milliseconds (negative instants floor
    /// away from zero relative to truncation).
    pub(crate) epoch_ms: i64,
    /// Intra-millisecond nanosecond remainder, [0, 999_999].
    pub(crate) sub_second_nanos: u32,
    /// Source fraction-digit count (0–9); `None` means "derive minimal
    /// precise digits at format time" (no invented zeros).
    pub(crate) fraction_digits: Option<u8>,
}

/// Wall date components (no zone semantics, Gregorian, month 1–12).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WallDate {
    pub(crate) year: i32,
    pub(crate) month: u32,
    pub(crate) day: u32,
}

/// Wall time components (no zone semantics; nanos are within-second).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WallTime {
    pub(crate) hour: u32,
    pub(crate) minute: u32,
    pub(crate) second: u32,
    pub(crate) nanos: u32,
    pub(crate) fraction_digits: Option<u8>,
}

/// Full wall date-time components (date + time flattened, like React's
/// `WallComponents extends WallDate, WallTime`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WallComponents {
    pub(crate) year: i32,
    pub(crate) month: u32,
    pub(crate) day: u32,
    pub(crate) hour: u32,
    pub(crate) minute: u32,
    pub(crate) second: u32,
    pub(crate) nanos: u32,
    pub(crate) fraction_digits: Option<u8>,
}

/// The input side of [`parse_unix_value`]: a JSON number or its string form.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum UnixValue<'a> {
    Number(f64),
    Text(&'a str),
}

/// Unix timestamp unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UnixUnit {
    Seconds,
    Milliseconds,
}

// ---------------------------------------------------------------------------
// Stable error codes (React's string-literal unions)
// ---------------------------------------------------------------------------

/// Parse-side error codes for the three text/number parsers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TemporalParseError {
    EmptyInput,
    InvalidFormat,
    InvalidIntegerSyntax,
    NotSafeInteger,
    InvalidMonth,
    InvalidDay,
    InvalidHour,
    InvalidMinute,
    InvalidSecond,
    InvalidOffset,
    FractionDigitsOutOfRange,
    OutOfRange,
}

/// `parse_zoned_timestamp` / `parse_unix_value` result.
pub(crate) type InstantParseResult = Result<InstantParts, TemporalParseError>;

/// `parse_wall_clock_text` result: three zone-free shapes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WallClockTextParse {
    Datetime(WallComponents),
    Date(WallDate),
    Time(WallTime),
    Err(TemporalParseError),
}

/// `wall_clock_to_instant` failure codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WallToInstantError {
    DstAmbiguous,
    DstInvalid,
    InvalidTimeZone,
    InvalidComponents,
    OutOfRange,
}

pub(crate) type WallToInstantResult = Result<InstantParts, WallToInstantError>;

/// `instant_to_wall_parts` failure codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InstantToWallError {
    InvalidTimeZone,
    InvalidInstant,
    OutOfRange,
    /// Defensive: the readback produced no usable components (unreachable
    /// for validated inputs; kept as the stable upper-layer fallback code).
    ReadbackFailed,
}

/// A successful `instant_to_wall_parts`: wall components plus the zone's
/// actual UTC offset at that instant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct InstantToWall {
    pub(crate) wall: WallComponents,
    /// Exact offset in seconds (historical sub-minute LMT included).
    pub(crate) offset_seconds: i64,
    /// `offset_seconds / 60` truncated toward zero (convenience field).
    pub(crate) offset_minutes: i64,
}

pub(crate) type InstantToWallResult = Result<InstantToWall, InstantToWallError>;

/// `validate_instant_parts` failure codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InstantValidationError {
    InvalidInstant,
    OutOfRange,
}

// ---------------------------------------------------------------------------
// Basic arithmetic (React's private helpers)
// ---------------------------------------------------------------------------

/// Mathematical modulo, always non-negative: `positive_mod(-1235, 1000) == 765`.
fn positive_mod(value: i64, modulus: i64) -> i64 {
    ((value % modulus) + modulus) % modulus
}

/// Proleptic Gregorian leap year (negative years follow the same rule).
fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

/// Wall components read as UTC → epoch milliseconds. `None` = outside
/// chrono's representable range (React's `Date.UTC` NaN path).
fn epoch_ms_from_utc_parts(
    year: i32,
    month: u32, // 1–12
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
    millisecond: u32,
) -> Option<i64> {
    let naive = chrono::NaiveDate::from_ymd_opt(year, month, day)?
        .and_hms_milli_opt(hour, minute, second, millisecond)?;
    Some(naive.and_utc().timestamp_millis())
}

// ---------------------------------------------------------------------------
// Zone resolution + Intl readback (chrono-tz)
// ---------------------------------------------------------------------------

/// Resolve an IANA zone name. Exact `chrono-tz` parse first, then an
/// ASCII case-insensitive scan over the variant table — ICU accepts and
/// canonicalizes lowercase zone ids (`asia/shanghai`), and this mirrors that
/// locked Node behavior. Whitespace-padded / non-IANA names resolve to None.
pub(crate) fn resolve_zone(name: &str) -> Option<Tz> {
    if let Ok(tz) = name.parse::<Tz>() {
        return Some(tz);
    }
    TZ_VARIANTS
        .iter()
        .find(|variant| variant.name().eq_ignore_ascii_case(name))
        .copied()
}

/// Whether the name is a usable IANA zone (React `isValidTimeZoneName`).
pub(crate) fn is_valid_time_zone_name(name: &str) -> bool {
    resolve_zone(name).is_some()
}

/// Readback wall components of an instant in a zone (civil milliseconds are
/// `positive_mod(epoch_ms, 1000)` — offsets are whole seconds, so they never
/// disturb the millisecond field).
#[derive(Clone, Copy, Debug)]
struct ReadbackWall {
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
    millisecond: i64,
}

fn read_wall_parts(epoch_ms: i64, zone: Tz) -> Option<ReadbackWall> {
    let dt = chrono::DateTime::from_timestamp_millis(epoch_ms)?.with_timezone(&zone);
    Some(ReadbackWall {
        year: dt.year(),
        month: dt.month(),
        day: dt.day(),
        hour: dt.hour(),
        minute: dt.minute(),
        second: dt.second(),
        millisecond: positive_mod(epoch_ms, 1000),
    })
}

/// The zone's actual UTC offset (seconds) at an instant.
fn read_offset_seconds_at(epoch_ms: i64, zone: Tz) -> Option<i64> {
    let dt = chrono::DateTime::from_timestamp_millis(epoch_ms)?.with_timezone(&zone);
    Some(i64::from(dt.offset().fix().local_minus_utc()))
}

// ---------------------------------------------------------------------------
// Hand-rolled cursor parsing (the React regex grammars, char for char)
// ---------------------------------------------------------------------------

fn is_digit(byte: u8) -> bool {
    byte.is_ascii_digit()
}

/// Read exactly `n` ASCII digits at `i`; `None` if fewer are available.
fn digits(bytes: &[u8], i: &mut usize, n: usize) -> Option<u32> {
    if *i + n > bytes.len() || !bytes[*i..*i + n].iter().all(|&b| is_digit(b)) {
        return None;
    }
    let mut value: u32 = 0;
    for _ in 0..n {
        value = value * 10 + u32::from(bytes[*i] - b'0');
        *i += 1;
    }
    Some(value)
}

/// `[+-]?\d{4,6}` — the extended-year field (sign optional, 4–6 digits).
struct YearField {
    year: i32,
}

fn parse_extended_year(bytes: &[u8], i: &mut usize) -> Option<YearField> {
    let mut sign = 1i32;
    if *i < bytes.len() && (bytes[*i] == b'+' || bytes[*i] == b'-') {
        if bytes[*i] == b'-' {
            sign = -1;
        }
        *i += 1;
    }
    let start = *i;
    while *i < bytes.len() && is_digit(bytes[*i]) {
        *i += 1;
    }
    let count = *i - start;
    if !(4..=6).contains(&count) {
        return None;
    }
    let mut value: i32 = 0;
    for &b in &bytes[start..*i] {
        value = value * 10 + i32::from(b - b'0');
    }
    Some(YearField { year: sign * value })
}

/// Fractional seconds `\.\d+` → (digit count, nanos-within-second). The
/// digit run may exceed 9 — the caller reports `FractionDigitsOutOfRange`,
/// mirroring the regex `\d+` plus the length check. Only the first 9 digits
/// feed the nanosecond value, so overlong runs never overflow the parse.
fn parse_fraction(bytes: &[u8], i: &mut usize) -> Option<(usize, u32)> {
    debug_assert_eq!(bytes[*i], b'.');
    *i += 1;
    let start = *i;
    while *i < bytes.len() && is_digit(bytes[*i]) {
        *i += 1;
    }
    if *i == start {
        return None; // `\d+` needs at least one digit
    }
    let text = std::str::from_utf8(&bytes[start..*i]).ok()?;
    let mut padded = text[..text.len().min(9)].to_owned();
    while padded.len() < 9 {
        padded.push('0');
    }
    let nanos: u32 = padded.parse().ok()?;
    Some((text.len(), nanos))
}

/// One of the three wall-clock shapes the wall parser accepts:
/// `HH:MM`, `HH:MM:SS`, `HH:MM:SS.f…`.
struct TimeField {
    hour: u32,
    minute: u32,
    second: u32,
    fraction_digits: usize,
    nanos: u32,
}

fn parse_time_field(bytes: &[u8], i: &mut usize) -> Option<TimeField> {
    let hour = digits(bytes, i, 2)?;
    if bytes.get(*i) != Some(&b':') {
        return None;
    }
    *i += 1;
    let minute = digits(bytes, i, 2)?;
    let mut second = 0;
    let mut fraction_digits = 0;
    let mut nanos = 0;
    if bytes.get(*i) == Some(&b':') {
        *i += 1;
        second = digits(bytes, i, 2)?;
        if bytes.get(*i) == Some(&b'.') {
            let (fd, n) = parse_fraction(bytes, i)?;
            fraction_digits = fd;
            nanos = n;
        }
    }
    Some(TimeField { hour, minute, second, fraction_digits, nanos })
}

/// The parsed shape of a zoned timestamp, before component validation.
struct ZonedShape {
    year: i32,
    month: u32,
    day: u32,
    time: TimeField,
    offset_hour: u32,
    offset_minute: u32,
    offset_ms: i64,
}

/// `[+-]?\d{4,6}-\d{2}-\d{2}[Tt ]HH:MM(:SS(.f+)?)?(Z|z|[+-]\d{2}(:?\d{2})?)`
/// consumed to the very end of the input. `None` = `invalid-format`.
fn match_zoned_shape(text: &str) -> Option<ZonedShape> {
    let bytes = text.as_bytes();
    let mut i = 0;
    let year = parse_extended_year(bytes, &mut i)?.year;
    if bytes.get(i) != Some(&b'-') {
        return None;
    }
    i += 1;
    let month = digits(bytes, &mut i, 2)?;
    if bytes.get(i) != Some(&b'-') {
        return None;
    }
    i += 1;
    let day = digits(bytes, &mut i, 2)?;
    match bytes.get(i) {
        Some(b'T') | Some(b't') | Some(b' ') => i += 1,
        _ => return None,
    }
    let time = parse_time_field(bytes, &mut i)?;
    let zone = &bytes[i..];
    let (offset_hour, offset_minute, offset_ms) = if zone == b"Z" || zone == b"z" {
        (0, 0, 0)
    } else if zone.first() == Some(&b'+') || zone.first() == Some(&b'-') {
        let sign: i64 = if zone[0] == b'-' { -1 } else { 1 };
        let mut j = 1;
        let offset_hour = digits(zone, &mut j, 2)?;
        let offset_minute = if zone.len() > 3 {
            // `:?\d{2}` — exactly two more digits, optional colon before.
            if zone[j] == b':' {
                j += 1;
            }
            digits(zone, &mut j, 2)?
        } else {
            0
        };
        if j != zone.len() {
            return None; // trailing garbage after the offset
        }
        (
            offset_hour,
            offset_minute,
            sign * (i64::from(offset_hour) * 3_600_000 + i64::from(offset_minute) * 60_000),
        )
    } else {
        return None;
    };
    Some(ZonedShape {
        year,
        month,
        day,
        time,
        offset_hour,
        offset_minute,
        offset_ms,
    })
}

/// Parse a zoned timestamp into an exact instant, keeping all fraction
/// precision. Error precedence is React's verbatim: shape → month → day →
/// hour → minute → second → fraction length → offset range → support range.
pub(crate) fn parse_zoned_timestamp(text: &str) -> InstantParseResult {
    if text.is_empty() {
        return Err(TemporalParseError::EmptyInput);
    }
    let Some(shape) = match_zoned_shape(text) else {
        return Err(TemporalParseError::InvalidFormat);
    };
    if !(1..=12).contains(&shape.month) {
        return Err(TemporalParseError::InvalidMonth);
    }
    if shape.day < 1 || shape.day > days_in_month(shape.year, shape.month) {
        return Err(TemporalParseError::InvalidDay);
    }
    if shape.time.hour > 23 {
        return Err(TemporalParseError::InvalidHour);
    }
    if shape.time.minute > 59 {
        return Err(TemporalParseError::InvalidMinute);
    }
    if shape.time.second > 59 {
        return Err(TemporalParseError::InvalidSecond);
    }
    if shape.time.fraction_digits > 9 {
        return Err(TemporalParseError::FractionDigitsOutOfRange);
    }
    // The offset's own range (React checks the parsed fields, so `+05:60`
    // is invalid even though it combines to a whole hour).
    if shape.offset_hour > 23 || shape.offset_minute > 59 {
        return Err(TemporalParseError::InvalidOffset);
    }
    let nanos_within_second = u64::from(shape.time.nanos);
    let epoch_ms = match epoch_ms_from_utc_parts(
        shape.year,
        shape.month,
        shape.day,
        shape.time.hour,
        shape.time.minute,
        shape.time.second,
        (nanos_within_second / 1_000_000) as u32,
    ) {
        Some(ms) => ms - shape.offset_ms,
        // Six-digit garbage years beyond chrono's range — React's NaN path.
        None => return Err(TemporalParseError::OutOfRange),
    };
    if !(MIN_SUPPORTED_EPOCH_MS..=MAX_SUPPORTED_EPOCH_MS).contains(&epoch_ms) {
        return Err(TemporalParseError::OutOfRange);
    }
    // Split invariant: first 3 fraction digits go into epoch_ms (already
    // there via the millisecond field), the last 6 are sub_second_nanos.
    // Offsets are whole seconds, so the millisecond alignment survives.
    Ok(InstantParts {
        epoch_ms,
        sub_second_nanos: (nanos_within_second % 1_000_000) as u32,
        fraction_digits: Some(shape.time.fraction_digits as u8),
    })
}

/// Parse a Unix timestamp. `f64` inputs must be safe integers (the JSON
/// number path); strings go through `i128` so seconds→milliseconds never
/// loses precision (React's BigInt domain).
pub(crate) fn parse_unix_value(value: UnixValue, unit: UnixUnit) -> InstantParseResult {
    let multiplier: i128 = match unit {
        UnixUnit::Seconds => 1000,
        UnixUnit::Milliseconds => 1,
    };
    let ms_big: i128 = match value {
        UnixValue::Number(number) => {
            // NaN / ±Infinity / fractional / beyond 2^53-1 are all rejected
            // with the same code as Number.isSafeInteger.
            if !number.is_finite() || number.fract() != 0.0 || number.abs() > MAX_SAFE_INTEGER {
                return Err(TemporalParseError::NotSafeInteger);
            }
            (number as i128)
                .checked_mul(multiplier)
                .ok_or(TemporalParseError::OutOfRange)?
        }
        UnixValue::Text(text) => {
            let bytes = text.as_bytes();
            let mut j = 0;
            if j < bytes.len() && (bytes[j] == b'+' || bytes[j] == b'-') {
                j += 1;
            }
            let digit_start = j;
            while j < bytes.len() && is_digit(bytes[j]) {
                j += 1;
            }
            if j != bytes.len() || j == digit_start {
                return Err(TemporalParseError::InvalidIntegerSyntax);
            }
            // Syntax is pre-validated, so a parse failure can only be an
            // i128 overflow — far beyond the support bounds either way
            // (React's BigInt path reports the same out-of-range).
            let parsed: i128 = text
                .parse()
                .map_err(|_| TemporalParseError::OutOfRange)?;
            parsed
                .checked_mul(multiplier)
                .ok_or(TemporalParseError::OutOfRange)?
        }
    };
    if ms_big < i128::from(MIN_SUPPORTED_EPOCH_MS)
        || ms_big > i128::from(MAX_SUPPORTED_EPOCH_MS)
    {
        return Err(TemporalParseError::OutOfRange);
    }
    let epoch_ms = ms_big as i64;
    // BUG-003: a nonzero millisecond remainder must report 3 fraction
    // digits or the iso/iso-tz output silently drops it; negative epochs
    // take the positive-modulo path (−1 ms → civil 999 ms → 3 digits).
    let fraction_digits = if positive_mod(epoch_ms, 1000) == 0 { 0 } else { 3 };
    Ok(InstantParts {
        epoch_ms,
        sub_second_nanos: 0,
        fraction_digits: Some(fraction_digits),
    })
}

/// Parse zone-free text into wall components — three shapes (datetime /
/// date / time), never any zone interpretation. Zone-bearing text is
/// `invalid-format` here; it belongs to [`parse_zoned_timestamp`].
pub(crate) fn parse_wall_clock_text(text: &str) -> WallClockTextParse {
    if text.is_empty() {
        return WallClockTextParse::Err(TemporalParseError::EmptyInput);
    }
    let bytes = text.as_bytes();

    // datetime: date + [Tt ] + time, consumed to the end.
    let mut i = 0;
    if let Some(year_field) = parse_extended_year(bytes, &mut i) {
        if bytes.get(i) == Some(&b'-') {
            i += 1;
            if let Some(month) = digits(bytes, &mut i, 2) {
                if bytes.get(i) == Some(&b'-') {
                    i += 1;
                    if let Some(day) = digits(bytes, &mut i, 2) {
                        if matches!(bytes.get(i), Some(b'T') | Some(b't') | Some(b' ')) {
                            i += 1;
                            if let Some(time) = parse_time_field(bytes, &mut i) {
                                if i == bytes.len() {
                                    return wall_components_or_err(
                                        year_field.year, month, day, time,
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // date-only.
    let mut i = 0;
    if let Some(year_field) = parse_extended_year(bytes, &mut i) {
        if bytes.get(i) == Some(&b'-') {
            i += 1;
            if let Some(month) = digits(bytes, &mut i, 2) {
                if bytes.get(i) == Some(&b'-') {
                    i += 1;
                    if let Some(day) = digits(bytes, &mut i, 2) {
                        if i == bytes.len() {
                            if !(1..=12).contains(&month) {
                                return WallClockTextParse::Err(
                                    TemporalParseError::InvalidMonth,
                                );
                            }
                            if day < 1 || day > days_in_month(year_field.year, month) {
                                return WallClockTextParse::Err(TemporalParseError::InvalidDay);
                            }
                            return WallClockTextParse::Date(WallDate {
                                year: year_field.year,
                                month,
                                day,
                            });
                        }
                    }
                }
            }
        }
    }

    // time-only.
    let mut i = 0;
    if let Some(time) = parse_time_field(bytes, &mut i) {
        if i == bytes.len() {
            if time.hour > 23 {
                return WallClockTextParse::Err(TemporalParseError::InvalidHour);
            }
            if time.minute > 59 {
                return WallClockTextParse::Err(TemporalParseError::InvalidMinute);
            }
            if time.second > 59 {
                return WallClockTextParse::Err(TemporalParseError::InvalidSecond);
            }
            if time.fraction_digits > 9 {
                return WallClockTextParse::Err(TemporalParseError::FractionDigitsOutOfRange);
            }
            return WallClockTextParse::Time(WallTime {
                hour: time.hour,
                minute: time.minute,
                second: time.second,
                nanos: time.nanos,
                fraction_digits: Some(time.fraction_digits as u8),
            });
        }
    }

    WallClockTextParse::Err(TemporalParseError::InvalidFormat)
}

/// datetime shape → validated components (React's `buildWallComponents`).
fn wall_components_or_err(
    year: i32,
    month: u32,
    day: u32,
    time: TimeField,
) -> WallClockTextParse {
    if !(1..=12).contains(&month) {
        return WallClockTextParse::Err(TemporalParseError::InvalidMonth);
    }
    if day < 1 || day > days_in_month(year, month) {
        return WallClockTextParse::Err(TemporalParseError::InvalidDay);
    }
    if time.hour > 23 {
        return WallClockTextParse::Err(TemporalParseError::InvalidHour);
    }
    if time.minute > 59 {
        return WallClockTextParse::Err(TemporalParseError::InvalidMinute);
    }
    if time.second > 59 {
        return WallClockTextParse::Err(TemporalParseError::InvalidSecond);
    }
    if time.fraction_digits > 9 {
        return WallClockTextParse::Err(TemporalParseError::FractionDigitsOutOfRange);
    }
    WallClockTextParse::Datetime(WallComponents {
        year,
        month,
        day,
        hour: time.hour,
        minute: time.minute,
        second: time.second,
        nanos: time.nanos,
        fraction_digits: Some(time.fraction_digits as u8),
    })
}

// ---------------------------------------------------------------------------
// Wall clock + zone → instant (DST double-check)
// ---------------------------------------------------------------------------

/// Runtime guard over [`WallComponents`] (React `checkWallComponents`).
fn check_wall_components(wall: &WallComponents) -> Option<WallToInstantError> {
    if !(1..=12).contains(&wall.month) {
        return Some(WallToInstantError::InvalidComponents);
    }
    if wall.day < 1 || wall.day > days_in_month(wall.year, wall.month) {
        return Some(WallToInstantError::InvalidComponents);
    }
    if wall.hour > 23 || wall.minute > 59 || wall.second > 59 {
        return Some(WallToInstantError::InvalidComponents);
    }
    if wall.nanos > 999_999_999 {
        return Some(WallToInstantError::InvalidComponents);
    }
    if let Some(digits) = wall.fraction_digits {
        if digits > 9 {
            return Some(WallToInstantError::InvalidComponents);
        }
    }
    None
}

/// Wall components + IANA zone → exact instant, refusing to guess at DST:
/// spring-forward gaps are `DstInvalid`, fall-back repeats are
/// `DstAmbiguous`. The candidate-offset probes at ±24 h cover every current
/// tzdata zone (including whole-day jumps like 2011 Pacific/Apia).
///
/// Precision passthrough: `wall.nanos`' millisecond digits join `epoch_ms`,
/// the remainder becomes `sub_second_nanos`, and `fraction_digits` passes
/// through untouched (`None` stays `None` — no invented zeros).
pub(crate) fn wall_clock_to_instant(
    wall: &WallComponents,
    time_zone: &str,
) -> WallToInstantResult {
    if let Some(error) = check_wall_components(wall) {
        return Err(error);
    }
    let Some(zone) = resolve_zone(time_zone) else {
        return Err(WallToInstantError::InvalidTimeZone);
    };
    let as_utc = match epoch_ms_from_utc_parts(
        wall.year,
        wall.month,
        wall.day,
        wall.hour,
        wall.minute,
        wall.second,
        wall.nanos / 1_000_000,
    ) {
        Some(ms) => ms,
        None => return Err(WallToInstantError::OutOfRange),
    };
    // Probes may go one day out of range; the final instant is tightened
    // against the support bounds below.
    if !(MIN_SUPPORTED_EPOCH_MS - DAY_MS..=MAX_SUPPORTED_EPOCH_MS + DAY_MS).contains(&as_utc) {
        return Err(WallToInstantError::OutOfRange);
    }

    let mut candidate_offsets: Vec<i64> = Vec::new();
    for probe in [as_utc - DAY_MS, as_utc, as_utc + DAY_MS] {
        if let Some(offset) = read_offset_seconds_at(probe, zone) {
            if !candidate_offsets.contains(&offset) {
                candidate_offsets.push(offset);
            }
        }
    }

    let wall_ms_part = i64::from(wall.nanos / 1_000_000);
    let mut matches: Vec<i64> = Vec::new();
    for offset_seconds in candidate_offsets {
        let candidate_ms = as_utc - offset_seconds * 1000;
        let Some(rb) = read_wall_parts(candidate_ms, zone) else {
            continue;
        };
        if rb.year == wall.year
            && rb.month == wall.month
            && rb.day == wall.day
            && rb.hour == wall.hour
            && rb.minute == wall.minute
            && rb.second == wall.second
            && rb.millisecond == wall_ms_part
        {
            matches.push(candidate_ms);
        }
    }

    match matches.len() {
        0 => Err(WallToInstantError::DstInvalid),
        1 => {
            let epoch_ms = matches[0];
            if !(MIN_SUPPORTED_EPOCH_MS..=MAX_SUPPORTED_EPOCH_MS).contains(&epoch_ms) {
                return Err(WallToInstantError::OutOfRange);
            }
            Ok(InstantParts {
                epoch_ms,
                sub_second_nanos: wall.nanos % 1_000_000,
                fraction_digits: wall.fraction_digits,
            })
        }
        _ => Err(WallToInstantError::DstAmbiguous),
    }
}

// ---------------------------------------------------------------------------
// Instant → wall components
// ---------------------------------------------------------------------------

/// Runtime guard over [`InstantParts`] (React `validateInstantParts`):
/// `None` = valid. The JS non-object / non-integer guards are type-level
/// here (`i64`/`u32`/`Option<u8>` cannot carry them).
pub(crate) fn validate_instant_parts(
    instant: &InstantParts,
) -> Option<InstantValidationError> {
    if instant.sub_second_nanos > 999_999 {
        return Some(InstantValidationError::InvalidInstant);
    }
    if let Some(digits) = instant.fraction_digits {
        if digits > 9 {
            return Some(InstantValidationError::InvalidInstant);
        }
    }
    if !(MIN_SUPPORTED_EPOCH_MS..=MAX_SUPPORTED_EPOCH_MS).contains(&instant.epoch_ms) {
        return Some(InstantValidationError::OutOfRange);
    }
    None
}

/// Instant → wall components in a zone + the zone's actual offset.
/// `sub_second_nanos` is carried into `wall.nanos` (civil milliseconds × 10⁶
/// + remainder — never a borrow); `fraction_digits` passes through.
pub(crate) fn instant_to_wall_parts(
    instant: &InstantParts,
    time_zone: &str,
) -> InstantToWallResult {
    match validate_instant_parts(instant) {
        Some(InstantValidationError::InvalidInstant) => {
            return Err(InstantToWallError::InvalidInstant);
        }
        Some(InstantValidationError::OutOfRange) => return Err(InstantToWallError::OutOfRange),
        None => {}
    }
    let Some(zone) = resolve_zone(time_zone) else {
        return Err(InstantToWallError::InvalidTimeZone);
    };
    let Some(rb) = read_wall_parts(instant.epoch_ms, zone) else {
        return Err(InstantToWallError::ReadbackFailed);
    };
    let Some(as_utc) = epoch_ms_from_utc_parts(
        rb.year,
        rb.month,
        rb.day,
        rb.hour,
        rb.minute,
        rb.second,
        rb.millisecond as u32,
    ) else {
        return Err(InstantToWallError::ReadbackFailed);
    };
    let offset_seconds = (as_utc - instant.epoch_ms) / 1000;
    Ok(InstantToWall {
        wall: WallComponents {
            year: rb.year,
            month: rb.month,
            day: rb.day,
            hour: rb.hour,
            minute: rb.minute,
            second: rb.second,
            nanos: (rb.millisecond as u32) * 1_000_000 + instant.sub_second_nanos,
            fraction_digits: instant.fraction_digits,
        },
        offset_seconds,
        // i64 division truncates toward zero — Math.trunc's exact behavior.
        offset_minutes: offset_seconds / 60,
    })
}

// ---------------------------------------------------------------------------
// Text helpers + ordering
// ---------------------------------------------------------------------------

/// Offset minutes → `±HH:MM` (zero renders `+00:00`, never `Z`). Non-integer
/// inputs truncate toward zero — documented, no rounding.
pub(crate) fn format_offset_minutes(offset_minutes: f64) -> String {
    let truncated = offset_minutes.trunc(); // -0.5 → -0.0, and -0.0 < 0 is false
    let sign = if truncated < 0.0 { '-' } else { '+' };
    let abs = truncated.abs() as i64;
    format!("{sign}{:02}:{:02}", abs / 60, abs % 60)
}

/// Offset seconds → `±HH:MM`, or `±HH:MM:SS` for sub-minute historical
/// offsets (29143 → `+08:05:43`).
pub(crate) fn format_offset_seconds(offset_seconds: f64) -> String {
    let truncated = offset_seconds.trunc();
    let sign = if truncated < 0.0 { '-' } else { '+' };
    let abs = truncated.abs() as i64;
    let hours = abs / 3600;
    let minutes = (abs % 3600) / 60;
    let seconds = abs % 60;
    let head = format!("{sign}{hours:02}:{minutes:02}");
    if seconds == 0 {
        head
    } else {
        format!("{head}:{seconds:02}")
    }
}

/// Strict total order over instants: `epoch_ms` first, then
/// `sub_second_nanos` (the decomposition is unique, so component comparison
/// is exact equality too). Fraction digits never participate.
pub(crate) fn compare_instants(a: &InstantParts, b: &InstantParts) -> Ordering {
    a.epoch_ms
        .cmp(&b.epoch_ms)
        .then(a.sub_second_nanos.cmp(&b.sub_second_nanos))
}
