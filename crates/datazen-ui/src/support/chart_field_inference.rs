//! Field inference for chart results, ported from
//! `packages/frontend/src/lib/chart/fieldInference.ts`.
//!
//! ## Deviations
//!
//! * **`Date.parse` fallback is narrower than JS.** The `YYYY-MM-DD`
//!   prefix test is verbatim. The second clause (`!isNaN(Date.parse(s))
//!   && s.length > 4`) accepts RFC3339 and RFC2822 only. JS parses a
//!   pile of implementation-defined strings (`"Jan 1, 2020"`, bare
//!   years) that this port does not.
//! * **`Number(s)` does not accept hex, binary or octal.** `"0x10"` is
//!   16 in JS and a non-numeric string here, so a column of those values
//!   is categorical. Decimal, exponent and the exact spellings
//!   `Infinity` / `+Infinity` / `-Infinity` match JS. `infinity` and
//!   `NaN` do not count as numeric, matching `isNaN(Number(s))`.
//! * **Substring quirks are kept.** `/int/` matches `interval`,
//!   `/time/` matches `runtime`. Those columns take the regex branch
//!   and never look at samples.

#![allow(dead_code)] // consumer: the chart view, not landed yet

use std::cmp::Ordering;

use chrono::DateTime;

/// `InferredFieldType` (`types/chart.ts`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InferredFieldType {
    Numeric,
    Datetime,
    Categorical,
    Boolean,
    Unknown,
}

/// A cell the inference looks at. Objects become [`ChartValue::Object`]
/// (`String(obj)` is `[object Object]`). Arrays keep their elements so
/// `String(array)` is the join, as in JS.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ChartValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<ChartValue>),
    Object,
}

/// `ColumnInfo` minus `nullable`, which inference never reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ChartColumn {
    pub name: String,
    pub data_type: String,
}

/// The slice of `StatementResult` inference reads: columns and rows.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ChartResult {
    pub columns: Vec<ChartColumn>,
    pub rows: Vec<Vec<ChartValue>>,
}

/// `ChartField`. `sample_values` are the JS `String(value)` forms: the
/// React set is built from `samples.map(String)`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ChartField {
    pub name: String,
    pub data_type: String,
    pub inferred_type: InferredFieldType,
    pub distinct_count: usize,
    pub sample_values: Vec<String>,
}

/// `inferFieldType`.
pub(crate) fn infer_field_type(column: &ChartColumn, sample_values: &[ChartValue]) -> InferredFieldType {
    let dt = column.data_type.to_lowercase();
    if dt.contains("bool") {
        return InferredFieldType::Boolean;
    }
    if is_numeric_type(&dt) {
        return InferredFieldType::Numeric;
    }
    if is_datetime_type(&dt) {
        return InferredFieldType::Datetime;
    }
    let non_null: Vec<&ChartValue> = sample_values.iter().filter(|v| !is_null(v)).collect();
    if non_null.is_empty() {
        return InferredFieldType::Unknown;
    }
    if non_null.iter().all(|v| matches!(v, ChartValue::Number(_))) {
        return InferredFieldType::Numeric;
    }
    if non_null.iter().all(|v| matches!(v, ChartValue::Bool(_))) {
        return InferredFieldType::Boolean;
    }
    if non_null.iter().all(|v| match v {
        ChartValue::String(text) => is_date_like(text),
        _ => false,
    }) {
        return InferredFieldType::Datetime;
    }
    if non_null.iter().all(|v| is_numeric_like(v)) {
        return InferredFieldType::Numeric;
    }
    InferredFieldType::Categorical
}

/// `inferAllFields`. Samples are the first 50 rows. Distinct values are
/// insertion-ordered `String(value)` of the non-null samples; the field
/// keeps the first five.
pub(crate) fn infer_all_fields(result: &ChartResult) -> Vec<ChartField> {
    let sample_size = result.rows.len().min(50);
    result
        .columns
        .iter()
        .enumerate()
        .map(|(col_idx, col)| {
            let samples: Vec<ChartValue> = result
                .rows
                .iter()
                .take(sample_size)
                .map(|row| row.get(col_idx).cloned().unwrap_or(ChartValue::Null))
                .collect();
            let mut distinct = Vec::new();
            for sample in samples.iter().filter(|v| !is_null(v)) {
                let text = js_string(sample);
                if !distinct.iter().any(|seen: &String| seen == &text) {
                    distinct.push(text);
                }
            }
            let distinct_count = distinct.len();
            distinct.truncate(5);
            ChartField {
                name: col.name.clone(),
                data_type: col.data_type.clone(),
                inferred_type: infer_field_type(col, &samples),
                distinct_count,
                sample_values: distinct,
            }
        })
        .collect()
}

/// `isChartableResult`: at least one row, one column, and one numeric field.
pub(crate) fn is_chartable_result(result: &ChartResult) -> bool {
    if result.rows.is_empty() || result.columns.is_empty() {
        return false;
    }
    infer_all_fields(result)
        .iter()
        .any(|field| field.inferred_type == InferredFieldType::Numeric)
}

fn is_null(value: &ChartValue) -> bool {
    matches!(value, ChartValue::Null)
}

/// `/int|serial|double|numeric|decimal|real|float|money|bigint|smallint/`
fn is_numeric_type(dt: &str) -> bool {
    const KEYS: &[&str] = &[
        "int", "serial", "double", "numeric", "decimal", "real", "float", "money", "bigint",
        "smallint",
    ];
    KEYS.iter().any(|key| dt.contains(key))
}

/// `/timestamp|date|time/`
fn is_datetime_type(dt: &str) -> bool {
    dt.contains("timestamp") || dt.contains("date") || dt.contains("time")
}

/// `isNumericLike`. A JS number (including NaN and Infinity) is numeric
/// before the string parse. Strings use `Number(trim(s))`, empty rejected.
fn is_numeric_like(value: &ChartValue) -> bool {
    match value {
        ChartValue::Number(_) => true,
        ChartValue::String(text) => {
            let trimmed = text.trim();
            !trimmed.is_empty() && js_number(trimmed).is_some_and(|n| !n.is_nan())
        }
        _ => false,
    }
}

/// `Number(s)` for a trimmed string.
///
/// JS accepts `Infinity`, `+Infinity`, `-Infinity`, decimal and exponent
/// forms, and (unlike this port) hex/binary/octal prefixes. It does
/// **not** accept `infinity`, `inf` or `NaN` as numbers. Rust's `f64`
/// parser is the opposite on those spellings, so they are rejected
/// before `parse`.
fn js_number(text: &str) -> Option<f64> {
    match text {
        "Infinity" | "+Infinity" => return Some(f64::INFINITY),
        "-Infinity" => return Some(f64::NEG_INFINITY),
        _ => {}
    }
    if js_rejects_but_rust_parses(text) {
        return None;
    }
    text.parse::<f64>().ok().filter(|n| !n.is_nan())
}

/// `infinity` / `inf` / `nan` (any case, optional sign) and `0x` / `0b` /
/// `0o` prefixes. Hex, binary and octal are a documented deviation: JS
/// `Number("0x10")` is 16, this port returns none.
fn js_rejects_but_rust_parses(text: &str) -> bool {
    let unsigned = text.trim_start_matches(['+', '-']);
    if unsigned.eq_ignore_ascii_case("infinity")
        || unsigned.eq_ignore_ascii_case("inf")
        || unsigned.eq_ignore_ascii_case("nan")
    {
        return true;
    }
    let bytes = unsigned.as_bytes();
    bytes.len() >= 2
        && bytes[0] == b'0'
        && matches!(bytes[1], b'x' | b'X' | b'b' | b'B' | b'o' | b'O')
}

/// `isDateLike`.
fn is_date_like(text: &str) -> bool {
    if has_iso_date_prefix(text) {
        return true;
    }
    text.len() > 4 && js_date_parse_ok(text)
}

fn has_iso_date_prefix(text: &str) -> bool {
    let b = text.as_bytes();
    b.len() >= 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b[..4].iter().all(|c| c.is_ascii_digit())
        && b[5..7].iter().all(|c| c.is_ascii_digit())
        && b[8..10].iter().all(|c| c.is_ascii_digit())
}

fn js_date_parse_ok(text: &str) -> bool {
    DateTime::parse_from_rfc3339(text).is_ok() || DateTime::parse_from_rfc2822(text).is_ok()
}

/// `String(value)` for the distinct-set. Numbers use V8's `String(number)`:
/// scientific notation when the exponent is `<= -7` or `>= 21`, with `+`
/// on a positive exponent (`1e-7`, `1e+21`).
pub(crate) fn js_string(value: &ChartValue) -> String {
    match value {
        ChartValue::Null => "null".to_owned(),
        ChartValue::Bool(flag) => flag.to_string(),
        ChartValue::Number(number) => js_number_text(*number),
        ChartValue::String(text) => text.clone(),
        ChartValue::Array(items) => items
            .iter()
            .map(js_string)
            .collect::<Vec<_>>()
            .join(","),
        ChartValue::Object => "[object Object]".to_owned(),
    }
}

/// V8 `String(number)`. `-0` is `0`. A finite non-zero value uses the
/// shortest round-trip digits, then decimal notation while the scientific
/// exponent is in `-6..=20` and exponential notation otherwise. A tie
/// between two shortest spellings takes the even last digit.
pub(crate) fn js_number_text(number: f64) -> String {
    if number.is_nan() {
        return "NaN".to_owned();
    }
    if number == 0.0 {
        return "0".to_owned();
    }
    if number.is_infinite() {
        return if number.is_sign_positive() {
            "Infinity".to_owned()
        } else {
            "-Infinity".to_owned()
        };
    }
    let negative = number.is_sign_negative();
    let abs = number.abs();
    let Some((sig, exp)) = significand_from_debug(abs) else {
        return if negative {
            format!("-{abs}")
        } else {
            abs.to_string()
        };
    };
    let (sig, exp) = prefer_even_tie(abs, sig, exp);
    let body = render_js_body(&sig, exp);
    if negative {
        format!("-{body}")
    } else {
        body
    }
}

/// Shortest digits from Rust's debug printer, as `(significand, scientific
/// exponent)`. Debug and V8 agree on those digits except on a halfway tie,
/// which [`prefer_even_tie`] then resolves.
fn significand_from_debug(abs: f64) -> Option<(String, i32)> {
    normalize_debug(&format!("{abs:?}"))
}

fn normalize_debug(dbg: &str) -> Option<(String, i32)> {
    if let Some((mantissa, exponent)) = dbg.split_once(['e', 'E']) {
        let mut exp: i32 = exponent.parse().ok()?;
        let digits: String = mantissa.chars().filter(|ch| *ch != '.' && *ch != '+').collect();
        let lead = digits.len() - digits.trim_start_matches('0').len();
        let digits = digits.trim_start_matches('0');
        exp = exp.checked_sub(i32::try_from(lead).ok()?)?;
        let trimmed = digits.trim_end_matches('0');
        if trimmed.is_empty() {
            return Some(("0".to_owned(), 0));
        }
        return Some((trimmed.to_owned(), exp));
    }
    let plain = dbg.strip_suffix(".0").unwrap_or(dbg);
    let (digits, mut point) = if let Some((head, tail)) = plain.split_once('.') {
        (format!("{head}{tail}"), i32::try_from(head.len()).ok()?)
    } else {
        (plain.to_owned(), i32::try_from(plain.len()).ok()?)
    };
    let lead = digits.len() - digits.trim_start_matches('0').len();
    let digits = digits.trim_start_matches('0');
    point = point.checked_sub(i32::try_from(lead).ok()?)?;
    let trimmed = digits.trim_end_matches('0');
    if trimmed.is_empty() {
        return Some(("0".to_owned(), 0));
    }
    Some((trimmed.to_owned(), point.checked_sub(1)?))
}

fn prefer_even_tie(abs: f64, sig: String, exp: i32) -> (String, i32) {
    let Some(last) = sig.as_bytes().last().copied() else {
        return (sig, exp);
    };
    if (last - b'0') % 2 == 0 {
        return (sig, exp);
    }
    for delta in [-1i32, 1] {
        let Some(alt) = add_sig_digits(&sig, delta) else {
            continue;
        };
        if alt.len() != sig.len() || (alt.as_bytes()[alt.len() - 1] - b'0') % 2 != 0 {
            continue;
        }
        let alt_text = render_js_body(&alt, exp);
        if alt_text.parse::<f64>().ok().map(f64::to_bits) != Some(abs.to_bits()) {
            continue;
        }
        match (scaled_distance(abs, &sig, exp), scaled_distance(abs, &alt, exp)) {
            (Some(left), Some(right)) if left == right => return (alt, exp),
            _ => {}
        }
    }
    (sig, exp)
}

fn render_js_body(sig: &str, exp: i32) -> String {
    if (-6..=20).contains(&exp) {
        format_js_decimal(sig, exp)
    } else {
        format_js_exp(sig, exp)
    }
}

fn add_sig_digits(sig: &str, delta: i32) -> Option<String> {
    let mut digits: Vec<u8> = sig.bytes().map(|byte| byte - b'0').collect();
    if digits.is_empty() {
        return None;
    }
    if delta == 1 {
        let mut carry = 1u8;
        for digit in digits.iter_mut().rev() {
            let sum = *digit + carry;
            *digit = sum % 10;
            carry = sum / 10;
            if carry == 0 {
                break;
            }
        }
        if carry != 0 {
            digits.insert(0, carry);
        }
    } else {
        let mut borrow = 1u8;
        for digit in digits.iter_mut().rev() {
            if *digit >= borrow {
                *digit -= borrow;
                borrow = 0;
                break;
            }
            *digit = *digit + 10 - borrow;
            borrow = 1;
        }
        if borrow != 0 {
            return None;
        }
        if digits.first() == Some(&0) {
            digits.remove(0);
        }
        if digits.is_empty() {
            return None;
        }
    }
    Some(digits.into_iter().map(|digit| char::from(b'0' + digit)).collect())
}

fn format_js_decimal(sig: &str, exp: i32) -> String {
    if exp >= 0 {
        let places = exp as usize + 1;
        if places >= sig.len() {
            let mut text = sig.to_owned();
            text.push_str(&"0".repeat(places - sig.len()));
            text
        } else {
            format!("{}.{}", &sig[..places], &sig[places..])
        }
    } else {
        let zeros = (-exp - 1) as usize;
        format!("0.{}{sig}", "0".repeat(zeros))
    }
}

fn format_js_exp(sig: &str, exp: i32) -> String {
    let sign = if exp >= 0 { '+' } else { '-' };
    let magnitude = exp.unsigned_abs();
    if sig.len() == 1 {
        format!("{sig}e{sign}{magnitude}")
    } else {
        format!("{}.{}e{sign}{magnitude}", &sig[..1], &sig[1..])
    }
}

/// `|sig * 10^(exp-len+1) - abs|` scaled onto a common integer, so two
/// candidates that share `exp` and length compare by limb equality.
fn scaled_distance(abs: f64, sig: &str, exp: i32) -> Option<Vec<u32>> {
    let digits: u128 = sig.parse().ok()?;
    let len = i32::try_from(sig.len()).ok()?;
    let power = exp.checked_sub(len.checked_sub(1)?)?;
    let (bin, bin_exp) = binary_decompose_js(abs);
    let ten_pow: u32 = if power < 0 {
        u32::try_from(power.checked_neg()?).ok()?
    } else {
        0
    };
    let two_pow: u32 = if bin_exp < 0 {
        u32::try_from(bin_exp.checked_neg()?).ok()?
    } else {
        0
    };
    let left_pow = u32::try_from(power.checked_add(i32::try_from(ten_pow).ok()?)?).ok()?;
    let right_pow = u32::try_from(bin_exp.checked_add(i32::try_from(two_pow).ok()?)?).ok()?;
    let left = shl_limbs(mul_pow10(from_u128(digits), left_pow), two_pow);
    let right = mul_pow10(shl_limbs(from_u128(u128::from(bin)), right_pow), ten_pow);
    Some(abs_sub_limbs(left, right))
}

fn binary_decompose_js(abs: f64) -> (u64, i32) {
    let bits = abs.to_bits();
    let exp_bits = ((bits >> 52) & 0x7ff) as i32;
    let frac = bits & ((1u64 << 52) - 1);
    if exp_bits == 0 {
        return (frac, if frac == 0 { 0 } else { -1074 });
    }
    let significand = frac | (1u64 << 52);
    (significand, exp_bits - 1023 - 52)
}

fn from_u128(mut value: u128) -> Vec<u32> {
    if value == 0 {
        return vec![0];
    }
    let mut limbs = Vec::new();
    while value > 0 {
        limbs.push(value as u32);
        value >>= 32;
    }
    limbs
}

fn mul_u32_limbs(limbs: &mut Vec<u32>, factor: u64) {
    let mut carry = 0u64;
    for limb in limbs.iter_mut() {
        let prod = u64::from(*limb) * factor + carry;
        *limb = prod as u32;
        carry = prod >> 32;
    }
    if carry != 0 {
        limbs.push(carry as u32);
    }
}

fn mul_pow10(mut limbs: Vec<u32>, mut exp: u32) -> Vec<u32> {
    while exp >= 9 {
        mul_u32_limbs(&mut limbs, 1_000_000_000);
        exp -= 9;
    }
    if exp > 0 {
        let mut factor = 1u64;
        for _ in 0..exp {
            factor *= 10;
        }
        mul_u32_limbs(&mut limbs, factor);
    }
    limbs
}

fn shl_limbs(limbs: Vec<u32>, bits: u32) -> Vec<u32> {
    if bits == 0 || limbs.iter().all(|limb| *limb == 0) {
        return if limbs.is_empty() { vec![0] } else { limbs };
    }
    let limb_shift = (bits / 32) as usize;
    let bit = bits % 32;
    let mut out = vec![0u32; limbs.len() + limb_shift + 1];
    if bit == 0 {
        out[limb_shift..limb_shift + limbs.len()].copy_from_slice(&limbs);
    } else {
        let mut carry = 0u32;
        for (index, limb) in limbs.iter().copied().enumerate() {
            out[index + limb_shift] = (limb << bit) | carry;
            carry = limb >> (32 - bit);
        }
        out[limbs.len() + limb_shift] = carry;
    }
    trim_limbs(&mut out);
    out
}

fn abs_sub_limbs(mut left: Vec<u32>, mut right: Vec<u32>) -> Vec<u32> {
    trim_limbs(&mut left);
    trim_limbs(&mut right);
    if cmp_limbs(&left, &right) == Ordering::Less {
        std::mem::swap(&mut left, &mut right);
    }
    let mut borrow = 0i64;
    for index in 0..left.len() {
        let other = if index < right.len() {
            i64::from(right[index])
        } else {
            0
        };
        let mut diff = i64::from(left[index]) - other - borrow;
        if diff < 0 {
            diff += 1i64 << 32;
            borrow = 1;
        } else {
            borrow = 0;
        }
        left[index] = diff as u32;
    }
    trim_limbs(&mut left);
    left
}

fn cmp_limbs(left: &[u32], right: &[u32]) -> Ordering {
    let left = trim_slice(left);
    let right = trim_slice(right);
    match left.len().cmp(&right.len()) {
        Ordering::Equal => {}
        other => return other,
    }
    for (lhs, rhs) in left.iter().rev().zip(right.iter().rev()) {
        match lhs.cmp(rhs) {
            Ordering::Equal => {}
            other => return other,
        }
    }
    Ordering::Equal
}

fn trim_slice(limbs: &[u32]) -> &[u32] {
    let mut end = limbs.len();
    while end > 0 && limbs[end - 1] == 0 {
        end -= 1;
    }
    if end == 0 {
        &limbs[..0]
    } else {
        &limbs[..end]
    }
}

fn trim_limbs(limbs: &mut Vec<u32>) {
    while limbs.len() > 1 && limbs.last() == Some(&0) {
        limbs.pop();
    }
    if limbs.is_empty() {
        limbs.push(0);
    }
}
