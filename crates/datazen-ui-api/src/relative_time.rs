//! Relative time formatting utility (no external date library dependency).
//!
//! Ported from React `packages/frontend/src/lib/relativeTime.ts` (tasklist
//! T119). Splits a past timestamp into minute/hour/day buckets for
//! `Intl.RelativeTimeFormat`-style rendering and falls back to a short date
//! beyond the 7-day window.
//!
//! ## The `Intl` mapping (this port's only substitution decision)
//!
//! React leans on two `Intl` facilities. Neither exists in `std`, and no
//! dependency may be added, so both are mirrored in pure Rust:
//!
//! - **`new Intl.RelativeTimeFormat('en', { numeric: 'auto', style: 'long'
//!   }).format(value, unit)`** — this file only ever formats *past*
//!   timestamps (`delta < MINUTE_MS` returns `"just now"` first, so the
//!   formatter only ever sees `value <= -1`; the clock-skew `value: 0`
//!   that [`get_relative_time_parts`] can emit is rendered defensively as
//!   `"this minute"`/`"this hour"`/`"today"` but is unreachable from
//!   [`format_relative_time`]). The mirror table is therefore: minute and
//!   hour `|value| == 1` → `"1 minute/hour ago"`, else
//!   `"N minutes/hours ago"`; day `|value| == 1` → `"yesterday"`, else
//!   `"N days ago"`. React hardcodes the `'en'` locale in this very call,
//!   so pinning English strings is faithful rather than a loss; a
//!   locale-aware host composes its own label from the parts.
//! - **`new Date(timestamp).toLocaleDateString(undefined, { year:
//!   'numeric', month: 'short', day: 'numeric' })`** — this one *is*
//!   runtime-locale and runtime-timezone dependent in React. The port pins
//!   the en-US shape (`"May 4, 2026"`, the React doc comment's own example)
//!   and resolves the calendar day in **UTC**: `std` has no timezone
//!   access, and the file's relative formatter already pins `'en'`. The
//!   exported [`RELATIVE_WINDOW_MS`] cutoff is the seam the React comment
//!   promises locale-aware callers (the i18n-composed label in
//!   RecentQueriesList) — they share it and format their own date.
//!
//! ## Port decisions
//!
//! - JS `number` timestamps are `f64` (the crate's `iso_from_epoch_ms`
//!   precedent); `Number.isFinite` is [`f64::is_finite`], and
//!   `Number.NaN`/`±Infinity` are the same literals here.
//! - [`RelativeTimeParts`]'s `value` is `Math.floor` output, hence always
//!   integral → `i64` (the crate's count precedent over the double a
//!   fractional JS number would need). Float→int casts saturate rather
//!   than trap, so an absurd `now` degrades to a huge day count instead
//!   of panicking.
//! - `Date.now()` becomes a [`std::time::SystemTime`] read. The injected
//!   `now: Option<f64>` parameter is the seam React itself provides
//!   (`now ?? Date.now()` plus the `Number.isFinite` re-check), so no
//!   clock trait is introduced; a clock read before the Unix epoch (not a
//!   realistic state) saturates to `0.0` rather than panicking.
//! - The epoch→(year, month, day) conversion re-implements Howard
//!   Hinnant's `civil_from_days` — the same algorithm `ai_sessions`'s
//!   `iso_from_epoch_ms` already uses, spelled out again here because that
//!   copy is module-private and widening a sibling's surface to share 11
//!   lines is not worth it.
//! - No serde: the output is a display string plus parts for a UI-side
//!   formatter, nothing that crosses a storage/IPC boundary.

use std::time::{SystemTime, UNIX_EPOCH};

const MINUTE_MS: f64 = 60_000.0;
const HOUR_MS: f64 = 3_600_000.0;
const DAY_MS: f64 = 86_400_000.0;
const MAX_RELATIVE_DAYS: f64 = 7.0;

/// Upper bound of the relative window (7 days). Deltas beyond this fall
/// back to a short locale date — exported so locale-aware callers (e.g. the
/// i18n-composed label in RecentQueriesList) share the exact cutoff.
pub const RELATIVE_WINDOW_MS: f64 = MAX_RELATIVE_DAYS * DAY_MS;

/// React's `unit: 'minute' | 'hour' | 'day'` union — the
/// `Intl.RelativeTimeFormat` unit the value counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RelativeTimeUnit {
    Minute,
    Hour,
    Day,
}

impl RelativeTimeUnit {
    /// React's literal, pinned so the port stays auditable against the
    /// source.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Minute => "minute",
            Self::Hour => "hour",
            Self::Day => "day",
        }
    }
}

/// A value/unit pair for `Intl.RelativeTimeFormat.format` consumers (React
/// `RelativeTimeParts`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RelativeTimeParts {
    /// Signed value passed to Intl.RelativeTimeFormat (negative = past).
    pub value: i64,
    /// The bucket the value counts.
    pub unit: RelativeTimeUnit,
}

/// React's `resolveNow`: the injected `now` when finite, the real clock
/// otherwise (`now ?? Date.now()`, then the `Number.isFinite` re-check that
/// falls back to `Date.now()` again).
fn resolve_now(now: Option<f64>) -> f64 {
    match now {
        Some(resolved) if resolved.is_finite() => resolved,
        _ => real_clock_ms(),
    }
}

/// React's `Date.now()` fallback — milliseconds since the Unix epoch off
/// the system clock.
fn real_clock_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as f64)
        .unwrap_or(0.0)
}

/// Splits a past timestamp into a value/unit pair for
/// `Intl.RelativeTimeFormat.format`.
///
/// Timestamps within the last minute (or in the near future, tolerating
/// clock skew) snap to the minute bucket with value 0. Invalid timestamps
/// (non-finite or non-positive) return `None`, React's `null`. The parts
/// keep counting days past the 7-day window — falling back to a date is
/// the *caller's* decision, signalled via [`RELATIVE_WINDOW_MS`].
pub fn get_relative_time_parts(timestamp: f64, now: Option<f64>) -> Option<RelativeTimeParts> {
    if !timestamp.is_finite() || timestamp <= 0.0 {
        return None;
    }
    let reference = resolve_now(now);
    let delta = reference - timestamp;
    if delta < MINUTE_MS {
        Some(RelativeTimeParts { value: 0, unit: RelativeTimeUnit::Minute })
    } else if delta < HOUR_MS {
        Some(RelativeTimeParts {
            value: -((delta / MINUTE_MS).floor() as i64),
            unit: RelativeTimeUnit::Minute,
        })
    } else if delta < DAY_MS {
        Some(RelativeTimeParts {
            value: -((delta / HOUR_MS).floor() as i64),
            unit: RelativeTimeUnit::Hour,
        })
    } else {
        Some(RelativeTimeParts {
            value: -((delta / DAY_MS).floor() as i64),
            unit: RelativeTimeUnit::Day,
        })
    }
}

/// `Intl.RelativeTimeFormat('en', { numeric: 'auto', style: 'long' })` over
/// the value space this file can produce (`value <= 0`; see the module docs
/// for the mapping table and why the future tense never occurs).
fn format_relative_value(value: i64, unit: RelativeTimeUnit) -> String {
    let magnitude = value.unsigned_abs();
    match unit {
        RelativeTimeUnit::Minute => match magnitude {
            0 => "this minute".to_string(),
            1 => "1 minute ago".to_string(),
            minutes => format!("{minutes} minutes ago"),
        },
        RelativeTimeUnit::Hour => match magnitude {
            0 => "this hour".to_string(),
            1 => "1 hour ago".to_string(),
            hours => format!("{hours} hours ago"),
        },
        RelativeTimeUnit::Day => match magnitude {
            0 => "today".to_string(),
            1 => "yesterday".to_string(),
            days => format!("{days} days ago"),
        },
    }
}

/// The short month vocabulary of the pinned en-US date.
const MONTHS_SHORT: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

fn month_short(month: u32) -> &'static str {
    // `month` is 1..=12 by construction; a defensive `.get` keeps a future
    // edit of `civil_from_days` from panicking here.
    MONTHS_SHORT.get((month - 1) as usize).copied().unwrap_or("")
}

/// Howard Hinnant's `civil_from_days`: days since 1970-01-01 →
/// (year, month, day). The same algorithm `ai_sessions`'s
/// `iso_from_epoch_ms` uses; spelled out again because that copy is
/// module-private.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { y + 1 } else { y }, month, day)
}

/// `new Date(timestamp).toLocaleDateString(undefined, { year: 'numeric',
/// month: 'short', day: 'numeric' })` — the pinned en-US, UTC-day label
/// (module docs). The caller has already established the timestamp is
/// finite and positive.
fn short_locale_date(timestamp: f64) -> String {
    let days = (timestamp / DAY_MS).floor() as i64;
    let (year, month, day) = civil_from_days(days);
    format!("{} {}, {}", month_short(month), day, year)
}

/// Formats a past timestamp as a concise relative label:
///
/// - `"just now"` for < 60 s — future timestamps (clock skew) also land
///   here;
/// - `"N minute(s)/hour(s)/day(s) ago"` up to 7 days (`"yesterday"` for
///   exactly one day);
/// - short date (e.g. `"May 4, 2026"`) beyond 7 days;
/// - `""` for invalid timestamps.
pub fn format_relative_time(timestamp: f64, now: Option<f64>) -> String {
    if !timestamp.is_finite() || timestamp <= 0.0 {
        return String::new();
    }
    let reference = resolve_now(now);
    let delta = reference - timestamp;
    if delta < MINUTE_MS {
        // Future timestamps (clock skew) also land here.
        return "just now".to_string();
    }
    // React spells the bound `MAX_RELATIVE_DAYS * DAY_MS` — the same value
    // `RELATIVE_WINDOW_MS` exports.
    if delta <= RELATIVE_WINDOW_MS {
        return match get_relative_time_parts(timestamp, Some(reference)) {
            Some(parts) => format_relative_value(parts.value, parts.unit),
            None => String::new(),
        };
    }
    short_locale_date(timestamp)
}

#[cfg(test)]
mod tests {
    use super::*;

    // The React suite (`lib/__tests__/relativeTime.test.ts`) is carried
    // it() by it(): the seven under `describe('formatRelativeTime')`, the
    // one under `describe('getRelativeTimeParts')`, and the four
    // `[tester]` edge paths at the tail. React reads `Date.now()` per
    // test; the logic only ever sees the delta, so a fixed reference —
    // 2026-09-11T12:00:00Z, the same instant the React fallback test uses
    // — pins the same cases deterministically (only the real-clock
    // fallback test touches the wall clock).

    /// 2026-09-11T12:00:00Z in epoch ms.
    const NOW: f64 = 1_789_128_000_000.0;

    /// 2026-05-04T03:24:00Z in epoch ms — the React fallback test's
    /// timestamp, chosen so the label is UTC-stable.
    const MAY_4_2026: f64 = 1_777_865_040_000.0;

    #[test]
    fn returns_just_now_for_timestamps_within_the_last_minute() {
        assert_eq!(format_relative_time(NOW - 5_000.0, Some(NOW)), "just now");
        assert_eq!(format_relative_time(NOW, Some(NOW)), "just now");
    }

    #[test]
    fn returns_just_now_for_timestamps_in_the_future_clock_skew_tolerance() {
        assert_eq!(format_relative_time(NOW + 30_000.0, Some(NOW)), "just now");
        assert_eq!(format_relative_time(NOW + 5.0 * MINUTE_MS, Some(NOW)), "just now");
    }

    #[test]
    fn formats_minutes_with_singular_and_plural() {
        assert_eq!(format_relative_time(NOW - 60_000.0, Some(NOW)), "1 minute ago");
        assert_eq!(format_relative_time(NOW - 2.0 * MINUTE_MS, Some(NOW)), "2 minutes ago");
        assert_eq!(format_relative_time(NOW - 59.0 * MINUTE_MS, Some(NOW)), "59 minutes ago");
    }

    #[test]
    fn formats_hours_with_singular_and_plural() {
        assert_eq!(format_relative_time(NOW - HOUR_MS, Some(NOW)), "1 hour ago");
        assert_eq!(format_relative_time(NOW - 5.0 * HOUR_MS, Some(NOW)), "5 hours ago");
    }

    #[test]
    fn formats_the_final_hour_before_the_day_boundary() {
        // [tester] The hour bucket's upper boundary was unpinned: the React
        // suite's largest formatted hour is 5. 23 hours is still the hour
        // bucket; the day cut is a full 24.
        assert_eq!(format_relative_time(NOW - 23.0 * HOUR_MS, Some(NOW)), "23 hours ago");
        assert_eq!(
            get_relative_time_parts(NOW - 23.0 * HOUR_MS, Some(NOW)),
            Some(RelativeTimeParts { value: -23, unit: RelativeTimeUnit::Hour })
        );
    }

    #[test]
    fn formats_days_up_to_the_7_day_cutoff() {
        // Intl.RelativeTimeFormat with numeric:'auto' renders -1 day as
        // "yesterday" — the mirror keeps that exact string.
        assert_eq!(format_relative_time(NOW - DAY_MS, Some(NOW)), "yesterday");
        assert_eq!(format_relative_time(NOW - 6.0 * DAY_MS, Some(NOW)), "6 days ago");
        assert_eq!(format_relative_time(NOW - 7.0 * DAY_MS, Some(NOW)), "7 days ago");
    }

    #[test]
    fn falls_back_to_a_short_date_beyond_7_days() {
        // React asserts only "a non-empty string without 'day'"; the port
        // can pin the exact label, because the locale and timezone are
        // pinned decisions (module docs): en-US shape, UTC calendar day.
        let formatted = format_relative_time(MAY_4_2026, Some(NOW));
        assert_eq!(formatted, "May 4, 2026");
        assert!(!formatted.contains("day"));
        assert!(!formatted.is_empty());
    }

    #[test]
    fn falls_back_to_a_short_date_just_beyond_the_window() {
        // [tester] The beyond-side of the cutoff was unpinned: nothing
        // stopped the window from quietly widening to 8 days. One
        // millisecond past the exported `RELATIVE_WINDOW_MS` —
        // 2026-09-04T11:59:59.999Z, UTC day Sep 4 — must already be a date.
        assert_eq!(
            format_relative_time(NOW - RELATIVE_WINDOW_MS - 1.0, Some(NOW)),
            "Sep 4, 2026"
        );
        // The first whole day past the window.
        assert_eq!(format_relative_time(NOW - 8.0 * DAY_MS, Some(NOW)), "Sep 3, 2026");
    }

    #[test]
    fn short_date_pins_the_leap_day_and_the_epoch_day_zero() {
        // [tester] `civil_from_days`'s leap term was unpinned: every pinned
        // date (May/Sep 2026) is non-leap, where dropping the term changes
        // nothing. 2024-02-29T00:00:00Z is a leap day.
        assert_eq!(format_relative_time(1_709_164_800_000.0, Some(NOW)), "Feb 29, 2024");
        // Day zero — the earliest civil day a valid timestamp can hit.
        // (`timestamp <= 0` returns "" first, so negative day counts are
        // unreachable by contract; `format_relative_time(-86_400_000.0)`
        // pins that side already.)
        assert_eq!(format_relative_time(1.0, Some(NOW)), "Jan 1, 1970");
    }

    #[test]
    fn returns_empty_string_for_invalid_timestamps() {
        assert_eq!(format_relative_time(f64::NAN, None), "");
        assert_eq!(format_relative_time(0.0, None), "");
    }

    #[test]
    fn splits_value_and_unit_for_intl_relative_time_format_consumers() {
        assert_eq!(
            get_relative_time_parts(NOW - 90_000.0, Some(NOW)),
            Some(RelativeTimeParts { value: -1, unit: RelativeTimeUnit::Minute })
        );
        assert_eq!(
            get_relative_time_parts(NOW - 3.0 * HOUR_MS, Some(NOW)),
            Some(RelativeTimeParts { value: -3, unit: RelativeTimeUnit::Hour })
        );
        assert_eq!(
            get_relative_time_parts(NOW - 2.0 * DAY_MS, Some(NOW)),
            Some(RelativeTimeParts { value: -2, unit: RelativeTimeUnit::Day })
        );
        // At or beyond the 7-day cutoff: signal fallback — the parts
        // themselves keep counting days; the caller decides.
        assert_eq!(
            get_relative_time_parts(NOW - 8.0 * DAY_MS, Some(NOW)),
            Some(RelativeTimeParts { value: -8, unit: RelativeTimeUnit::Day })
        );
    }

    // [tester] edge paths from the tail of the React suite: invalid
    // inputs, the sub-minute bucket reachable only via direct calls, and
    // the non-finite `now` fallback in `resolveNow`.

    #[test]
    fn get_relative_time_parts_returns_none_for_non_finite_non_positive_timestamps() {
        assert_eq!(get_relative_time_parts(f64::NAN, None), None);
        assert_eq!(get_relative_time_parts(0.0, None), None);
        assert_eq!(get_relative_time_parts(-1.0, None), None);
        assert_eq!(get_relative_time_parts(f64::INFINITY, None), None);
    }

    #[test]
    fn get_relative_time_parts_snaps_sub_minute_deltas_to_the_zero_minute_bucket() {
        assert_eq!(
            get_relative_time_parts(NOW - 5_000.0, Some(NOW)),
            Some(RelativeTimeParts { value: 0, unit: RelativeTimeUnit::Minute })
        );
        assert_eq!(
            get_relative_time_parts(NOW + 30_000.0, Some(NOW)),
            Some(RelativeTimeParts { value: 0, unit: RelativeTimeUnit::Minute })
        );
    }

    #[test]
    fn format_relative_time_rejects_non_finite_and_negative_timestamps() {
        assert_eq!(format_relative_time(f64::INFINITY, None), "");
        assert_eq!(format_relative_time(f64::NEG_INFINITY, None), "");
        assert_eq!(format_relative_time(-86_400_000.0, None), "");
    }

    #[test]
    fn falls_back_to_the_real_clock_when_the_injected_now_is_not_finite() {
        // React: `formatRelativeTime(ts, Number.NaN)` must fall through to
        // `Date.now()` inside `resolveNow` and land in the hour bucket.
        let ts = real_clock_ms() - 2.0 * HOUR_MS;
        let formatted = format_relative_time(ts, Some(f64::NAN));
        assert!(formatted.contains("hour"), "formatted: {formatted}");
    }

    #[test]
    fn unit_as_str_pins_the_react_literals() {
        // [tester] React's `unit: 'minute' | 'hour' | 'day'` union crosses
        // to an Intl consumer as these literals (`RelativeTimeUnit::as_str`);
        // no other test in the crate exercises them, so the mirror table
        // was unpinned.
        assert_eq!(RelativeTimeUnit::Minute.as_str(), "minute");
        assert_eq!(RelativeTimeUnit::Hour.as_str(), "hour");
        assert_eq!(RelativeTimeUnit::Day.as_str(), "day");
    }
}
