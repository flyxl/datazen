//! Formatting helpers, ported from `packages/frontend/src/lib/formatters.ts`.
//!
//! The timestamp rule is the load-bearing one: database text without a zone
//! designator is **wall clock**, not an instant. Round-tripping it through
//! `to_iso_string` would shift it by the local UTC offset and append a bogus
//! `Z`, so such strings are returned verbatim.
#![allow(dead_code)] // consumers: CellRenderer/GridCell/DetailPanel (T004/T015/T009)

use serde_json::Value;

use crate::i18n::t;

/// Whether the text carries an explicit UTC designator (`Z`) or a numeric
/// offset (`+08:00`, `-0800`).
fn has_zone_designator(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.ends_with('Z') || trimmed.ends_with('z') {
        return true;
    }
    let bytes = trimmed.as_bytes();
    if bytes.len() < 5 {
        return false;
    }
    let tail = &bytes[bytes.len() - 5..];
    let sign_ok = tail[0] == b'+' || tail[0] == b'-';
    let digits_ok = tail[1].is_ascii_digit()
        && tail[2].is_ascii_digit()
        && tail[3].is_ascii_digit()
        && tail[4].is_ascii_digit();
    if sign_ok && digits_ok {
        return true;
    }
    if bytes.len() >= 6 {
        let tail = &bytes[bytes.len() - 6..];
        let sign_ok = tail[0] == b'+' || tail[0] == b'-';
        let digits_ok = tail[1].is_ascii_digit()
            && tail[2].is_ascii_digit()
            && tail[3] == b':'
            && tail[4].is_ascii_digit()
            && tail[5].is_ascii_digit();
        return sign_ok && digits_ok;
    }
    false
}

/// `formatTimestamp` from React: nullish → `NULL`; numbers are epoch
/// milliseconds; zone-less strings are returned verbatim; zoned strings are
/// normalised to `YYYY-MM-DDTHH:MM:SS.sssZ`; everything else falls back to
/// its textual form.
pub(crate) fn format_timestamp(value: &Value) -> String {
    match value {
        Value::Null => "NULL".to_owned(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => {
            if let Some(millis) = number.as_i64() {
                return iso_from_epoch_millis(millis);
            }
            if let Some(millis) = number.as_f64() {
                if millis.is_finite() {
                    return iso_from_epoch_millis(millis.trunc() as i64);
                }
            }
            number.to_string()
        }
        Value::String(text) => {
            if !has_zone_designator(text) {
                return text.clone();
            }
            match parse_zoned_instant(text) {
                Some(millis) => iso_from_epoch_millis(millis),
                None => text.clone(),
            }
        }
        other => other.to_string(),
    }
}

/// `formatCell` from React: nullish → `NULL`, booleans are lower-case, and
/// structured values become JSON text.
pub(crate) fn format_cell(value: &Value) -> String {
    match value {
        Value::Null => "NULL".to_owned(),
        Value::Bool(flag) => flag.to_string(),
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        other => serde_json::to_string(other).unwrap_or_else(|_| other.to_string()),
    }
}

/// `formatLastConnected` from React. The React side renders the browser's
/// locale string; this port has no locale machinery, so a valid instant is
/// rendered as `YYYY-MM-DD HH:MM:SS` in UTC and an unparseable input is
/// returned unchanged. The missing case is the one locale-backed string, so
/// it goes through `t()` like the React version.
pub(crate) fn format_last_connected(iso: Option<&str>) -> String {
    let Some(text) = iso.filter(|text| !text.is_empty()) else {
        return t("conn.neverConnected").to_owned();
    };
    match parse_zoned_instant(text) {
        Some(millis) => {
            let (year, month, day, hour, minute, second) = civil_from_epoch(millis);
            format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}")
        }
        None => text.to_owned(),
    }
}

/// Milliseconds since the Unix epoch for a zoned ISO-8601-ish string.
///
/// Accepts `YYYY-MM-DD` with an optional `T`/space time, optional seconds and
/// fraction, then a `Z` or `±HH:MM`/`±HHMM` offset. Returns `None` when the
/// text has no zone or is not a calendar date.
pub(crate) fn parse_zoned_instant(text: &str) -> Option<i64> {
    let mut input = text.trim();
    let (offset_minutes, zoned) = if let Some(rest) = input.strip_suffix(['Z', 'z']) {
        input = rest;
        (0, true)
    } else if let Some((rest, offset)) = split_offset(input) {
        input = rest;
        (offset, true)
    } else {
        (0, false)
    };
    if !zoned {
        return None;
    }
    let (date, time) = match input.split_once(['T', ' ']) {
        Some((date, time)) => (date, Some(time)),
        None => (input, None),
    };
    let mut date_parts = date.split('-');
    let year: i32 = date_parts.next()?.parse().ok()?;
    let month: u32 = date_parts.next()?.parse().ok()?;
    let day: u32 = date_parts.next()?.parse().ok()?;
    if date_parts.next().is_some() {
        return None;
    }
    if !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
        return None;
    }
    let (mut hour, mut minute, mut second, mut millis) = (0, 0, 0, 0);
    if let Some(time) = time {
        let (clock, fraction) = match time.split_once('.') {
            Some((clock, fraction)) => (clock, Some(fraction)),
            None => (time, None),
        };
        let mut fields = clock.split(':');
        hour = fields.next()?.parse().ok()?;
        minute = fields.next()?.parse().ok()?;
        if let Some(sec) = fields.next() {
            second = sec.parse().ok()?;
        }
        if fields.next().is_some() {
            return None;
        }
        if hour > 23 || minute > 59 || second > 59 {
            return None;
        }
        if let Some(fraction) = fraction {
            let digits: String = fraction.chars().take(3).collect();
            let padded = format!("{digits:0<3}");
            millis = padded.parse().ok()?;
        }
    }
    let days = days_from_civil(year, month, day);
    let seconds =
        days * 86_400 + i64::from(hour) * 3_600 + i64::from(minute) * 60 + i64::from(second);
    Some((seconds - i64::from(offset_minutes) * 60) * 1_000 + millis)
}

/// Splits a trailing numeric offset off a timestamp.
fn split_offset(text: &str) -> Option<(&str, i32)> {
    let bytes = text.as_bytes();
    if bytes.len() < 5 {
        return None;
    }
    let last = bytes[bytes.len() - 1];
    if !last.is_ascii_digit() {
        return None;
    }
    let colon_form = bytes.len() >= 6 && bytes[bytes.len() - 3] == b':';
    let offset_len = if colon_form { 6 } else { 5 };
    if bytes.len() <= offset_len {
        return None;
    }
    let tail = &text[text.len() - offset_len..];
    let (sign, hour_text, minute_text) = if colon_form {
        (&tail[..1], &tail[1..3], &tail[4..6])
    } else {
        (&tail[..1], &tail[1..3], &tail[3..5])
    };
    if sign != "+" && sign != "-" {
        return None;
    }
    let hours: i32 = hour_text.parse().ok()?;
    let minutes: i32 = minute_text.parse().ok()?;
    if hours > 23 || minutes > 59 {
        return None;
    }
    let total = hours * 60 + minutes;
    let signed = if sign == "-" { -total } else { total };
    Some((&text[..text.len() - offset_len], signed))
}

/// `YYYY-MM-DDTHH:MM:SS.sssZ` for an epoch-milliseconds instant.
fn iso_from_epoch_millis(millis: i64) -> String {
    let (year, month, day, hour, minute, second) = civil_from_epoch(millis);
    format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis_rem:03}Z",
        millis_rem = millis.rem_euclid(1_000)
    )
}

/// Civil date/time (UTC) from milliseconds since the epoch.
fn civil_from_epoch(millis: i64) -> (i32, u32, u32, u32, u32, u32) {
    let days = millis.div_euclid(86_400_000);
    let seconds = millis.rem_euclid(86_400_000) / 1_000;
    let (year, month, day) = civil_from_days(days);
    let hour = (seconds / 3_600) as u32;
    let minute = ((seconds % 3_600) / 60) as u32;
    let second = (seconds % 60) as u32;
    (year, month, day, hour, minute, second)
}

/// Howard Hinnant's `civil_from_days`.
fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    let year = year + i64::from(month <= 2);
    (year as i32, month as u32, day as u32)
}

/// Howard Hinnant's `days_from_civil`.
fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let year = i64::from(year) - i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month = i64::from(month);
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::*;

    #[test]
    fn format_cell_handles_null_booleans_and_strings() {
        assert_eq!(format_cell(&Value::Null), "NULL");
        assert_eq!(format_cell(&json!(true)), "true");
        assert_eq!(format_cell(&json!(false)), "false");
        assert_eq!(format_cell(&json!("hello")), "hello");
    }

    #[test]
    fn format_cell_stringifies_objects_and_arrays_as_json() {
        assert_eq!(
            format_cell(&json!({ "key": "value" })),
            r#"{"key":"value"}"#
        );
        assert_eq!(format_cell(&json!([1, 2, 3])), "[1,2,3]");
        let nested = json!({ "a": { "b": [1, 2] }, "c": "test" });
        assert_eq!(
            format_cell(&nested),
            serde_json::to_string(&nested).unwrap()
        );
    }

    #[test]
    fn format_cell_converts_numbers_to_their_text() {
        assert_eq!(format_cell(&json!(42)), "42");
        assert_eq!(format_cell(&json!(3.14)), "3.14");
    }

    #[test]
    fn format_timestamp_returns_null_for_nullish() {
        assert_eq!(format_timestamp(&Value::Null), "NULL");
    }

    #[test]
    fn format_timestamp_keeps_zone_less_wall_clock_verbatim() {
        assert_eq!(
            format_timestamp(&json!("2026-03-01 00:15:30")),
            "2026-03-01 00:15:30"
        );
        assert_eq!(
            format_timestamp(&json!("2026-03-01 00:15:30.123")),
            "2026-03-01 00:15:30.123"
        );
        assert_eq!(format_timestamp(&json!("2026-03-01")), "2026-03-01");
    }

    #[test]
    fn format_timestamp_normalizes_zoned_values() {
        assert_eq!(
            format_timestamp(&json!("2024-01-15T10:00:00Z")),
            "2024-01-15T10:00:00.000Z"
        );
        assert_eq!(
            format_timestamp(&json!("2024-01-15T10:00:00+08:00")),
            "2024-01-15T02:00:00.000Z"
        );
        assert_eq!(
            format_timestamp(&json!("2024-01-15T10:00:00-0800")),
            "2024-01-15T18:00:00.000Z"
        );
    }

    #[test]
    fn format_timestamp_keeps_unparseable_text_as_is() {
        assert_eq!(format_timestamp(&json!("not-a-date")), "not-a-date");
        assert_eq!(
            format_timestamp(&json!("2026-03-01T25:00:00Z")),
            "2026-03-01T25:00:00Z"
        );
    }

    #[test]
    fn format_timestamp_renders_epoch_numbers_as_iso() {
        assert_eq!(
            format_timestamp(&json!(1_700_000_000_000i64)),
            "2023-11-14T22:13:20.000Z"
        );
        assert_eq!(format_timestamp(&json!(0)), "1970-01-01T00:00:00.000Z");
    }

    #[test]
    fn nan_numbers_cannot_exist_in_a_json_value() {
        // serde_json's number type is finite by construction; this is the
        // port's version of the React `Number.isNaN(d.getTime())` guard.
        assert!(serde_json::Number::from_f64(f64::NAN).is_none());
        assert!(serde_json::Number::from_f64(f64::INFINITY).is_none());
    }

    #[test]
    fn format_last_connected_uses_the_translated_fallback() {
        assert_eq!(
            format_last_connected(None),
            crate::i18n::t("conn.neverConnected")
        );
        assert_eq!(
            format_last_connected(Some("")),
            crate::i18n::t("conn.neverConnected")
        );
        assert_eq!(
            format_last_connected(Some("2024-06-01T12:00:00.000Z")),
            "2024-06-01 12:00:00"
        );
        assert_eq!(format_last_connected(Some("not-a-date")), "not-a-date");
    }
}
