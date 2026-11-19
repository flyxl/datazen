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

/// `String(value)` for the distinct-set.
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

fn js_number_text(number: f64) -> String {
    if number.is_nan() {
        return "NaN".to_owned();
    }
    if number.is_infinite() {
        return if number.is_sign_positive() {
            "Infinity".to_owned()
        } else {
            "-Infinity".to_owned()
        };
    }
    if number == 0.0 {
        return "0".to_owned();
    }
    if number.is_finite() && number.fract() == 0.0 && number.abs() < 1e21 {
        return format!("{number:.0}");
    }
    // JS `String(number)` for a non-integral finite float.
    let text = format!("{number}");
    text
}
