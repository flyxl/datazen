//! Port of `packages/frontend/src/lib/valuePresentation/timestampFormat.ts`.
//!
//! The three output formats of the timestamp presentation (issue-32 design
//! §8.7, T02):
//!
//! - `iso` — UTC ISO 8601 with the source fraction-digit count preserved
//!   (no padding, no truncation): `2026-01-15T14:30:00.123456789Z`.
//! - `iso-tz` — the target zone's wall time plus the offset actually in
//!   effect (`…+08:00`; historical sub-minute LMT offsets render
//!   `±HH:MM:SS`, e.g. 1850 Shanghai `+08:05:43`).
//! - `locale` — a locale-shaped rendering with an explicit `GMT±H` offset
//!   marker (see the locale table below).
//!
//! # What maps where
//!
//! - **`Intl.DateTimeFormat` → this module's own assembly.** `iso` /
//!   `iso-tz` were never ICU-dependent in React either (string math over
//!   `Date` UTC accessors / `instantToWallParts`); they port verbatim onto
//!   chrono readbacks. The `local` zone spec resolves through the
//!   `iana-time-zone` crate instead of `Intl.resolvedOptions().timeZone`
//!   (same answer source: the machine's zone).
//! - **The `locale` format renders from a three-locale CLDR table**
//!   (`en-US` / `de-DE` / `zh-CN`) plus an en-shaped default for every
//!   other well-formed tag (ICU's data-less-tag fallback approximated —
//!   React's own test only asserts `ok` for `'zzz'`). The React tests pin
//!   *structural substrings only* (wall time digits, AM/PM, the `GMT±H`
//!   marker, the fraction separator), explicitly because exact punctuation
//!   varies between ICU versions; this port reproduces exactly those
//!   substrings and treats punctuation beyond them as a documented delta.
//! - **Sub-millisecond precision in `locale` output truncates to 3 digits**
//!   (toward the past, never rounding) — React's documented decision
//!   (Intl caps `fractionalSecondDigits` at 3), kept verbatim: an error
//!   would make the locale format useless for MySQL(6)/Postgres microsecond
//!   values.
//! - **Offset marker** (`shortOffset`): `GMT+8` for whole hours, `GMT+5:45`
//!   for whole minutes, `GMT+8:05:43` for sub-minute LMT.
//!
//! # Known deltas (type-level)
//!
//! - The JS runtime-garbage guards — `null` / `{kind:'mars'}` specs, a
//!   non-string locale, an unrecognized format value (whose default branch
//!   reports the semantically odd `invalid-locale`) — are unrepresentable
//!   against [`TimeZoneSpec`] / `&str` / [`TimestampOutputFormat`].
//! - The empty-string locale rejection (`invalid-locale`) is kept: empty is
//!   representable and is the machine-default leak React refuses.
//!
//! Failure modes are React's verbatim: the display zone is fully validated
//! up front for every format (invalid IANA names fail identically in
//! preview and display), and the instant is validated before formatting.

use chrono::{Datelike, Timelike};

use crate::support::value_presentation::temporal_math::{
    InstantParts, InstantToWallError, InstantValidationError, TemporalFormatError,
    format_offset_seconds, instant_to_wall_parts, is_valid_time_zone_name,
    validate_instant_parts,
};

/// The three output formats (React `TimestampOutputFormat`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TimestampOutputFormat {
    Iso,
    IsoTz,
    Locale,
}

/// Display zone selection (React `TimeZoneSpec`). The serde shape is
/// React's persisted one (`{"kind":"iana","name":"Asia/Shanghai"}`).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub(crate) enum TimeZoneSpec {
    Utc,
    Local,
    Iana { name: String },
}

/// `formatTimestampOutput` options (React `TimestampFormatOptions`).
#[derive(Clone, Debug)]
pub(crate) struct TimestampFormatOptions<'a> {
    pub(crate) format: TimestampOutputFormat,
    pub(crate) display_time_zone: &'a TimeZoneSpec,
    pub(crate) locale: &'a str,
}

pub(crate) type TimestampFormatResult = Result<String, TemporalFormatError>;

// ---------------------------------------------------------------------------
// Zone-name validation and resolution
// ---------------------------------------------------------------------------

/// IANA zone-name *format* validation (non-empty, no whitespace, sane
/// charset, length cap). Full validity (does the zone exist) is delegated
/// to the Intl-side check ([`is_valid_time_zone_name`]) at format time —
/// lowercase passes here, exactly like React.
pub(crate) fn is_valid_iana_time_zone_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 100 {
        return false;
    }
    name.bytes().all(|byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'+' | b'-' | b'/')
    })
}

/// The machine's default zone (`{ kind: 'local' }` resolution; 'UTC' when
/// undeterminable) — `iana-time-zone` instead of Intl's
/// `resolvedOptions().timeZone`.
fn local_time_zone_name() -> String {
    match iana_time_zone::get_timezone() {
        Ok(name) if !name.is_empty() => name,
        _ => "UTC".to_string(),
    }
}

/// TimeZoneSpec → a zone name usable by the conversion layer. `iana` only
/// passes the format check here (React's contract: full validation happens
/// at format time).
pub(crate) fn resolve_time_zone_name(spec: &TimeZoneSpec) -> Result<String, TemporalFormatError> {
    match spec {
        TimeZoneSpec::Utc => Ok("UTC".to_string()),
        TimeZoneSpec::Local => Ok(local_time_zone_name()),
        TimeZoneSpec::Iana { name } => {
            if is_valid_iana_time_zone_name(name) {
                Ok(name.clone())
            } else {
                Err(TemporalFormatError::InvalidTimeZone)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Fraction digits + ISO assembly
// ---------------------------------------------------------------------------

fn pad2(value: u32) -> String {
    format!("{value:02}")
}

/// ISO year text (Date.toISOString extended-year convention): negative
/// `-0500`, 4-digit zero-padded `0050`, beyond 9999 explicitly `+100000`.
fn format_iso_year(year: i32) -> String {
    if year < 0 {
        format!("-{:04}", -(year as i64))
    } else if year > 9999 {
        format!("+{year}")
    } else {
        format!("{year:04}")
    }
}

/// Nanos-within-second → 9-digit string (123456789 ns → "123456789").
fn nanos_to_digits9(nanos: u32) -> String {
    format!("{nanos:09}")
}

/// The fraction-digit count actually rendered: an explicit
/// `fractionDigits` wins (source precision, no padding no truncation —
/// including explicit zero); absent → minimal exact digits (123000 ns → 6,
/// 1 ns → 9, 0 → none).
fn resolve_fraction_digits(fraction_digits: Option<u8>, nanos: u32) -> usize {
    if let Some(explicit) = fraction_digits {
        return usize::from(explicit);
    }
    if nanos == 0 {
        return 0;
    }
    nanos_to_digits9(nanos).trim_end_matches('0').len()
}

/// The iso / iso-tz body (datetime + fraction, no offset suffix).
fn format_iso_body(
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
    fraction_digits: Option<u8>,
    sub_second_nanos: u32,
) -> String {
    let use_digits = resolve_fraction_digits(fraction_digits, sub_second_nanos);
    let fraction = if use_digits == 0 {
        String::new()
    } else {
        format!(".{}", &nanos_to_digits9(sub_second_nanos)[..use_digits])
    };
    format!(
        "{}-{}-{}T{}:{}:{}{}",
        format_iso_year(year),
        pad2(month),
        pad2(day),
        pad2(hour),
        pad2(minute),
        pad2(second),
        fraction
    )
}

/// The instant's UTC wall components (`Date` UTC accessors — no 0–99-year
/// mapping quirk in chrono). `None` is unreachable for validated instants
/// (defensive, mirrors React's trust in `new Date`).
struct UtcWall {
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
    nanos: u32,
}

fn utc_wall_of(instant: &InstantParts) -> Option<UtcWall> {
    let dt = chrono::DateTime::from_timestamp_millis(instant.epoch_ms)?;
    Some(UtcWall {
        year: dt.year(),
        month: dt.month(),
        day: dt.day(),
        hour: dt.hour(),
        minute: dt.minute(),
        second: dt.second(),
        // civil milliseconds + intra-millisecond nanos = nanos within the
        // second (the file-header invariant).
        nanos: dt.timestamp_subsec_millis() * 1_000_000 + instant.sub_second_nanos,
    })
}

fn format_iso_utc(instant: &InstantParts) -> TimestampFormatResult {
    let wall = utc_wall_of(instant).ok_or(TemporalFormatError::InvalidInstant)?;
    Ok(format!(
        "{}Z",
        format_iso_body(
            wall.year,
            wall.month,
            wall.day,
            wall.hour,
            wall.minute,
            wall.second,
            instant.fraction_digits,
            wall.nanos
        )
    ))
}

fn format_iso_tz(instant: &InstantParts, time_zone_name: &str) -> TimestampFormatResult {
    let converted = instant_to_wall_parts(instant, time_zone_name)
        .map_err(|reason| match reason {
            // instant and zone were validated at the entry point; this arm
            // is defensive. readback-failed has no format-layer code and
            // folds into invalid-instant per React's convention.
            InstantToWallError::ReadbackFailed => TemporalFormatError::InvalidInstant,
            InstantToWallError::InvalidTimeZone => TemporalFormatError::InvalidTimeZone,
            InstantToWallError::InvalidInstant => TemporalFormatError::InvalidInstant,
            InstantToWallError::OutOfRange => TemporalFormatError::OutOfRange,
        })?;
    let body = format_iso_body(
        converted.wall.year,
        converted.wall.month,
        converted.wall.day,
        converted.wall.hour,
        converted.wall.minute,
        converted.wall.second,
        instant.fraction_digits,
        converted.wall.nanos,
    );
    // ±HH:MM for whole-minute offsets; historical sub-minute offsets render
    // ±HH:MM:SS — the same rule as temporalMath's format_offset_seconds.
    Ok(format!("{body}{}", format_offset_seconds(converted.offset_seconds as f64)))
}

// ---------------------------------------------------------------------------
// locale format (three-locale CLDR table + en-shaped default)
// ---------------------------------------------------------------------------

/// The `shortOffset` marker: `GMT+8` (whole hour), `GMT+5:45` (whole
/// minute), `GMT+8:05:43` (sub-minute LMT).
fn short_offset_marker(offset_seconds: i64) -> String {
    let sign = if offset_seconds < 0 { '-' } else { '+' };
    let abs = offset_seconds.unsigned_abs();
    let hours = abs / 3600;
    let minutes = (abs % 3600) / 60;
    let seconds = abs % 60;
    if minutes == 0 && seconds == 0 {
        format!("GMT{sign}{hours}")
    } else if seconds == 0 {
        format!("GMT{sign}{hours}:{minutes:02}")
    } else {
        format!("GMT{sign}{hours}:{minutes:02}:{seconds:02}")
    }
}

/// A locale's rendering shape. The table covers the three locales the
/// React tests pin; other well-formed tags use the en shape (ICU's
/// default-language fallback for data-less tags, approximated).
struct LocaleShape {
    /// `true` → 12-hour clock with a 2-digit hour and AM/PM (en-US).
    twelve_hour: bool,
    /// How the date part is ordered/punctuated.
    date: fn(u32, u32, i32) -> String,
    /// Separator between date and time.
    date_time_sep: &'static str,
    /// The decimal separator inside the fractional-second part.
    fraction_sep: char,
}

fn date_en(month: u32, day: u32, year: i32) -> String {
    format!("{}/{}/{}", pad2(month), pad2(day), year)
}

fn date_de(month: u32, day: u32, year: i32) -> String {
    format!("{:02}.{:02}.{year}", day, month)
}

fn date_zh(month: u32, day: u32, year: i32) -> String {
    format!("{year}/{}/{:02}", pad2(month), day)
}

fn locale_shape(locale: &str) -> LocaleShape {
    let lowered = locale.to_ascii_lowercase();
    let (twelve_hour, date, date_time_sep, fraction_sep) = if lowered.starts_with("de") {
        (false, date_de as fn(u32, u32, i32) -> String, ", ", ',')
    } else if lowered.starts_with("zh") {
        (false, date_zh as fn(u32, u32, i32) -> String, " ", '.')
    } else {
        // en-* and every other well-formed tag (including data-less ones
        // like 'zzz', which ICU falls back to its default language for).
        (true, date_en as fn(u32, u32, i32) -> String, ", ", '.')
    };
    LocaleShape { twelve_hour, date, date_time_sep, fraction_sep }
}

fn format_locale(
    instant: &InstantParts,
    locale: &str,
    time_zone_name: &str,
) -> TimestampFormatResult {
    // An empty locale would leak ICU's machine default — reject explicitly.
    if locale.is_empty() {
        return Err(TemporalFormatError::InvalidLocale);
    }
    // The fraction option keys off the wall nanos (React reads the UTC
    // wall; offsets are whole seconds, so the target-zone answer is the
    // same). When present it renders exactly 3 digits — the millisecond
    // part, truncated toward the past.
    let utc_wall = utc_wall_of(instant).ok_or(TemporalFormatError::InvalidInstant)?;
    let show_fraction = utc_wall.nanos > 0;
    let converted = instant_to_wall_parts(instant, time_zone_name)
        .map_err(|_| TemporalFormatError::InvalidInstant)?;
    let wall = converted.wall;

    let shape = locale_shape(locale);
    let date = (shape.date)(wall.month, wall.day, wall.year);

    let (time, meridiem) = if shape.twelve_hour {
        let hour12 = match wall.hour % 12 {
            0 => 12,
            other => other,
        };
        (
            format!("{:02}:{:02}:{:02}", hour12, wall.minute, wall.second),
            if wall.hour < 12 { "AM" } else { "PM" },
        )
    } else {
        (
            format!(
                "{}:{}:{}",
                pad2(wall.hour),
                pad2(wall.minute),
                pad2(wall.second)
            ),
            "",
        )
    };
    let time = if show_fraction {
        format!(
            "{time}{}{:03}",
            shape.fraction_sep,
            wall.nanos / 1_000_000
        )
    } else {
        time
    };

    let marker = short_offset_marker(converted.offset_seconds);
    let meridiem_part = if meridiem.is_empty() { String::new() } else { format!(" {meridiem}") };
    Ok(format!(
        "{date}{}{time}{meridiem_part} {marker}",
        shape.date_time_sep
    ))
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Format an instant per the configuration. DST day-crossings, negative
/// offsets and non-whole-minute offsets all come out of
/// [`instant_to_wall_parts`]'s actual-offset calculation.
pub(crate) fn format_timestamp_output(
    instant: &InstantParts,
    opts: &TimestampFormatOptions<'_>,
) -> TimestampFormatResult {
    match validate_instant_parts(instant) {
        Some(InstantValidationError::InvalidInstant) => {
            return Err(TemporalFormatError::InvalidInstant);
        }
        Some(InstantValidationError::OutOfRange) => {
            return Err(TemporalFormatError::OutOfRange);
        }
        None => {}
    }
    let zone_name = resolve_time_zone_name(opts.display_time_zone)?;
    // Every format validates the zone fully: an invalid configuration must
    // fail identically in preview and display.
    if !is_valid_time_zone_name(&zone_name) {
        return Err(TemporalFormatError::InvalidTimeZone);
    }
    match opts.format {
        TimestampOutputFormat::Iso => format_iso_utc(instant),
        TimestampOutputFormat::IsoTz => format_iso_tz(instant, &zone_name),
        TimestampOutputFormat::Locale => format_locale(instant, opts.locale, &zone_name),
    }
}
