//! Number and axis formatting, ported from
//! `packages/frontend/src/lib/chart/format.ts`.
//!
//! ## Deviations
//!
//! * **`toLocaleString` is en-US, not the process locale.** React calls
//!   `toLocaleString()` with no locale. This port groups with `,` and a
//!   `.` decimal. Integers (including values past `2^53`, and `-0`) take
//!   the integer path. Other numbers are rounded half-away-from-zero on
//!   the shortest round-trip decimal to at most 2 fraction digits, which
//!   is what en-US `Intl` does for `1.005` → `1.01` (not `Number#toFixed`,
//!   which yields `1.00` for that input).
//! * **`formatCompact` / `formatPercent` use `Number#toFixed`**, half away
//!   from zero on the binary value. That matches V8 for the pinned
//!   thresholds (`10.0K`, `1.3`, `0.13`, `-25.6%`).
//! * **Non-finite numbers.** `NaN` → `NaN`, `Infinity` → `∞`,
//!   `-Infinity` → `-∞` (en-US `toLocaleString`).
//! * **`formatEpochMs` uses the process local zone**, same as
//!   `Date#getHours`. The clock string is `HH:mm:ss`.
//! * **Axis truncation counts Unicode scalars, not UTF-16 code units.**
//!   An ASCII string past 20 characters keeps 18 and an ellipsis `…`.
//!   A string whose JS `.length` and scalar count disagree (emoji) can
//!   truncate at a different point.

#![allow(dead_code)] // consumer: the chart view, not landed yet

use chrono::{Local, TimeZone};

use super::chart_field_inference::ChartValue;

/// `Date.UTC(2000, 0, 1)`.
pub(crate) const EPOCH_MS_MIN: f64 = 946_684_800_000.0;
/// `Date.UTC(2100, 0, 1)`.
pub(crate) const EPOCH_MS_MAX: f64 = 4_102_444_800_000.0;

/// `formatNumber`.
pub(crate) fn format_number(value: Option<f64>) -> String {
    let Some(value) = value else {
        return String::new();
    };
    if value.is_nan() {
        return "NaN".to_owned();
    }
    if value.is_infinite() {
        return if value.is_sign_positive() {
            "∞".to_owned()
        } else {
            "-∞".to_owned()
        };
    }
    let rounded_is_integer = is_js_integer(value);
    if rounded_is_integer {
        return format_en_integer(value);
    }
    let negative = value.is_sign_negative();
    let (int_part, frac) = intl_fraction(value.abs(), 2);
    let body = assemble_grouped(&int_part, &frac);
    if negative {
        format!("-{body}")
    } else {
        body
    }
}

/// `formatCompact`.
pub(crate) fn format_compact(value: Option<f64>) -> String {
    let Some(value) = value else {
        return String::new();
    };
    let abs = value.abs();
    if abs >= 1e9 {
        return format!("{}B", to_fixed(value / 1e9, 1));
    }
    if abs >= 1e6 {
        return format!("{}M", to_fixed(value / 1e6, 1));
    }
    if abs >= 1e4 {
        return format!("{}K", to_fixed(value / 1e3, 1));
    }
    if is_js_integer(value) {
        return js_integer_text(value);
    }
    let digits = if abs >= 100.0 {
        0
    } else if abs >= 1.0 {
        1
    } else {
        2
    };
    to_fixed(value, digits)
}

/// `formatPercent`. `value` is a ratio: `0.5` → `50.0%`.
pub(crate) fn format_percent(value: Option<f64>) -> String {
    let Some(value) = value else {
        return String::new();
    };
    format!("{}%", to_fixed(value * 100.0, 1))
}

/// `isEpochMs`: inclusive `[2000-01-01, 2100-01-01]` UTC.
pub(crate) fn is_epoch_ms(value: f64) -> bool {
    value >= EPOCH_MS_MIN && value <= EPOCH_MS_MAX
}

/// `formatEpochMs`: local `HH:mm:ss`.
pub(crate) fn format_epoch_ms(ts: f64) -> String {
    let millis = ts.trunc() as i64;
    match Local.timestamp_millis_opt(millis).single() {
        Some(dt) => dt.format("%H:%M:%S").to_string(),
        None => String::new(),
    }
}

/// `formatAxisTick`.
pub(crate) fn format_axis_tick(value: &ChartValue) -> String {
    match value {
        ChartValue::Null => String::new(),
        ChartValue::Number(number) => {
            if is_epoch_ms(*number) {
                format_epoch_ms(*number)
            } else {
                format_number(Some(*number))
            }
        }
        other => {
            let text = super::chart_field_inference::js_string(other);
            if text.chars().count() > 20 {
                let head: String = text.chars().take(18).collect();
                format!("{head}…")
            } else {
                text
            }
        }
    }
}

fn is_js_integer(value: f64) -> bool {
    // `Number.isInteger`: finite and no fraction. No `2^53` cap
    // (`Number.isSafeInteger` is a different predicate). `-0` is an integer.
    value.is_finite() && value.fract() == 0.0
}

fn js_integer_text(value: f64) -> String {
    // Compact's integer branch only sees `|n| < 1e4`. `toString(-0)` is `0`.
    if value == 0.0 {
        return "0".to_owned();
    }
    format!("{value:.0}")
}

fn format_en_integer(value: f64) -> String {
    // `(-0).toLocaleString("en-US")` is `-0`, unlike `toString`.
    let negative = value.is_sign_negative();
    let digits = integer_digits(value.abs());
    let grouped = group_thousands(&digits);
    if negative {
        format!("-{grouped}")
    } else {
        grouped
    }
}

fn integer_digits(abs: f64) -> String {
    let text = plain_decimal(abs);
    match text.split_once('.') {
        Some((head, _)) => head.to_owned(),
        None => text,
    }
}

/// Shortest round-trip decimal with no exponent, so rounding sees the
/// same digits `Intl` uses (`1.005`, not the binary `1.00499…`).
fn plain_decimal(abs: f64) -> String {
    let text = format!("{abs}");
    let Some((mantissa, exponent)) = text.split_once(['e', 'E']) else {
        return text;
    };
    let Ok(exp) = exponent.parse::<i32>() else {
        return text;
    };
    expand_scientific(mantissa.trim_start_matches('+'), exp)
}

fn expand_scientific(mantissa: &str, exp: i32) -> String {
    let (head, tail) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let digits = format!("{head}{tail}");
    let point = head.len() as i32 + exp;
    if point <= 0 {
        let zeros = "0".repeat((-point) as usize);
        return format!("0.{zeros}{digits}");
    }
    let point = point as usize;
    if point >= digits.len() {
        let mut out = digits;
        out.push_str(&"0".repeat(point - out.len()));
        return out;
    }
    format!("{}.{}", &digits[..point], &digits[point..])
}

/// en-US `maximumFractionDigits`, half away from zero, trailing zeros stripped.
fn intl_fraction(abs: f64, digits: usize) -> (String, String) {
    let plain = plain_decimal(abs);
    let (int_part, frac) = plain.split_once('.').unwrap_or((plain.as_str(), ""));
    let mut frac_chars: Vec<char> = frac.chars().collect();
    while frac_chars.len() <= digits {
        frac_chars.push('0');
    }
    let round_up = frac_chars.get(digits).is_some_and(|ch| *ch >= '5');
    let mut kept: Vec<char> = frac_chars.into_iter().take(digits).collect();
    let mut int_digits: Vec<char> = int_part.chars().collect();
    if int_digits.is_empty() {
        int_digits.push('0');
    }
    if round_up {
        let mut carry = true;
        for ch in kept.iter_mut().rev() {
            if !carry {
                break;
            }
            if *ch == '9' {
                *ch = '0';
            } else if ch.is_ascii_digit() {
                *ch = char::from(*ch as u8 + 1);
                carry = false;
            }
        }
        if carry {
            for ch in int_digits.iter_mut().rev() {
                if !carry {
                    break;
                }
                if *ch == '9' {
                    *ch = '0';
                } else if ch.is_ascii_digit() {
                    *ch = char::from(*ch as u8 + 1);
                    carry = false;
                }
            }
            if carry {
                int_digits.insert(0, '1');
            }
        }
    }
    let frac_out: String = kept.into_iter().collect();
    (
        int_digits.into_iter().collect(),
        frac_out.trim_end_matches('0').to_owned(),
    )
}

fn assemble_grouped(int_part: &str, frac: &str) -> String {
    let grouped = group_thousands(int_part);
    if frac.is_empty() {
        grouped
    } else {
        format!("{grouped}.{frac}")
    }
}

fn group_thousands(digits: &str) -> String {
    let mut out = String::new();
    for (index, ch) in digits.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out.chars().rev().collect()
}

/// `Number#toFixed`: half away from zero on the binary value, exactly
/// `digits` fraction digits. Non-finite values share `formatNumber`'s words.
fn to_fixed(value: f64, digits: usize) -> String {
    if !value.is_finite() {
        return format_number(Some(value));
    }
    let factor = 10f64.powi(digits as i32);
    let rounded = (value * factor).round() / factor;
    format!("{rounded:.digits$}")
}
