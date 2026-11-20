//! Independent T078 retest. Expected strings were taken from Node
//! (`Number`, `toLocaleString('en-US')`, `Number#toFixed`, `String`,
//! `Date.parse`) against `lib/chart/fieldInference.ts` and `format.ts`.
//!
//! Header deviations that this file treats as specified, not as failures:
//! hex/binary/octal `Number` rejected, `Date.parse` limited to RFC3339 and
//! RFC2822, en-US grouping, axis truncation by Unicode scalar.

use crate::support::chart_field_inference::{
    infer_all_fields, infer_field_type, is_chartable_result, js_string, ChartColumn, ChartResult,
    ChartValue, InferredFieldType,
};
use crate::support::chart_format::{
    format_axis_tick, format_compact, format_epoch_ms, format_number, format_percent, is_epoch_ms,
    EPOCH_MS_MAX, EPOCH_MS_MIN,
};

fn col(name: &str, data_type: &str) -> ChartColumn {
    ChartColumn {
        name: name.to_owned(),
        data_type: data_type.to_owned(),
    }
}

fn text_col(values: &[&str]) -> ChartResult {
    ChartResult {
        columns: vec![col("c", "varchar")],
        rows: values
            .iter()
            .map(|value| vec![ChartValue::String((*value).to_owned())])
            .collect(),
    }
}

#[test]
fn format_number_matches_en_us_intl_including_carry_and_signed_zero() {
    assert_eq!(format_number(None), "");
    assert_eq!(format_number(Some(1.004)), "1");
    assert_eq!(format_number(Some(0.995)), "1");
    assert_eq!(format_number(Some(999_999.996)), "1,000,000");
    assert_eq!(format_number(Some(-1234.565)), "-1,234.57");
    assert_eq!(format_number(Some(0.004)), "0");
    assert_eq!(format_number(Some(2.675)), "2.68");
    assert_eq!(format_number(Some(1.015)), "1.02");
    assert_eq!(format_number(Some(-0.0)), "-0");
    assert_eq!(format_number(Some(9.95)), "9.95");
    assert_eq!(format_number(Some(f64::NAN)), "NaN");
    assert_eq!(format_number(Some(f64::INFINITY)), "∞");
}

#[test]
fn format_compact_and_percent_match_node_for_ordinary_magnitudes() {
    assert_eq!(format_compact(None), "");
    assert_eq!(format_compact(Some(9_999.0)), "9999");
    assert_eq!(format_compact(Some(10_000.0)), "10.0K");
    assert_eq!(format_compact(Some(999_999.0)), "1000.0K");
    assert_eq!(format_compact(Some(1.25e6)), "1.3M");
    assert_eq!(format_compact(Some(1.25e9)), "1.3B");
    assert_eq!(format_compact(Some(-25_000.0)), "-25.0K");
    assert_eq!(format_compact(Some(0.013)), "0.01");
    assert_eq!(format_compact(Some(100.4)), "100");
    assert_eq!(format_compact(Some(1.25)), "1.3");
    assert_eq!(format_compact(Some(0.13)), "0.13");
    assert_eq!(format_compact(Some(-0.0)), "0");
    assert_eq!(format_percent(None), "");
    assert_eq!(format_percent(Some(0.0)), "0.0%");
    assert_eq!(format_percent(Some(-0.256)), "-25.6%");
    assert_eq!(format_percent(Some(0.013)), "1.3%");
}

#[test]
fn compact_9_95_matches_v8_to_fixed() {
    // Node: (9.95).toFixed(1) === "9.9", so formatCompact(9.95) === "9.9".
    assert_eq!(format_compact(Some(9.95)), "9.9");
}

#[test]
fn compact_8_95_matches_v8_to_fixed() {
    assert_eq!(format_compact(Some(8.95)), "8.9");
}

#[test]
fn compact_1e30_uses_v8_exponential_to_fixed() {
    // Node: (1e30 / 1e9).toFixed(1) === "1e+21".
    assert_eq!(format_compact(Some(1e30)), "1e+21B");
    assert_eq!(format_compact(Some(-1e30)), "-1e+21B");
}

#[test]
fn percent_1e19_uses_v8_exponential_to_fixed() {
    // Node: (1e19 * 100).toFixed(1) === "1e+21".
    assert_eq!(format_percent(Some(1e19)), "1e+21%");
}

#[test]
fn epoch_window_is_inclusive_and_axis_truncates_ascii_at_18_plus_ellipsis() {
    assert_eq!(EPOCH_MS_MIN, 946_684_800_000.0);
    assert_eq!(EPOCH_MS_MAX, 4_102_444_800_000.0);
    assert!(is_epoch_ms(EPOCH_MS_MIN));
    assert!(is_epoch_ms(EPOCH_MS_MAX));
    assert!(!is_epoch_ms(EPOCH_MS_MIN - 1.0));
    assert!(!is_epoch_ms(EPOCH_MS_MAX + 1.0));
    assert!(!is_epoch_ms(f64::NAN));
    let clock = format_epoch_ms(1_704_167_045_000.0);
    assert_eq!(clock.len(), 8);
    assert_eq!(clock.as_bytes()[2], b':');
    let long = "123456789012345678901";
    assert_eq!(long.len(), 21);
    assert_eq!(
        format_axis_tick(&ChartValue::String(long.to_owned())),
        "123456789012345678…"
    );
    assert_eq!(format_axis_tick(&ChartValue::Null), "");
    assert_eq!(format_axis_tick(&ChartValue::Bool(true)), "true");
    // Documented scalar-vs-UTF-16 deviation: 11 emoji is JS length 22
    // (would truncate) and 11 scalars (kept).
    let emoji = "😀".repeat(11);
    assert_eq!(format_axis_tick(&ChartValue::String(emoji.clone())), emoji);
}

#[test]
fn js_string_matches_number_to_string() {
    let cases = [
        (1e-7, "1e-7"),
        (1.23e-7, "1.23e-7"),
        (-1.23e-7, "-1.23e-7"),
        (1e21, "1e+21"),
        (-1e21, "-1e+21"),
        (1e20, "100000000000000000000"),
        (-0.0, "0"),
    ];
    let mut misses = Vec::new();
    for (number, expect) in cases {
        let got = js_string(&ChartValue::Number(number));
        if got != expect {
            misses.push(format!("{number:?} rust={got:?} js={expect:?}"));
        }
    }
    let joined = js_string(&ChartValue::Array(vec![
        ChartValue::Number(1.0),
        ChartValue::String("a".into()),
    ]));
    if joined != "1,a" {
        misses.push(format!("array rust={joined:?}"));
    }
    assert_eq!(js_string(&ChartValue::Object), "[object Object]");
    assert!(misses.is_empty(), "{}", misses.join("\n"));
}

#[test]
fn inference_order_samples_and_documented_number_and_date_narrowing() {
    let flag = col("b", "BOOL");
    assert_eq!(
        infer_field_type(&flag, &[ChartValue::String("no".into())]),
        InferredFieldType::Boolean
    );
    assert_eq!(
        infer_field_type(&col("i", "interval"), &[]),
        InferredFieldType::Numeric
    );
    assert_eq!(
        infer_field_type(&col("r", "runtime"), &[]),
        InferredFieldType::Datetime
    );
    assert_eq!(
        infer_field_type(&col("u", "text"), &[ChartValue::Null]),
        InferredFieldType::Unknown
    );
    assert_eq!(
        infer_field_type(
            &col("n", "text"),
            &[
                ChartValue::String("  1e2 ".into()),
                ChartValue::String(".5".into()),
                ChartValue::String("Infinity".into()),
            ]
        ),
        InferredFieldType::Numeric
    );
    assert_eq!(
        infer_field_type(&col("n", "text"), &[ChartValue::String("infinity".into())]),
        InferredFieldType::Categorical
    );
    // Documented: Number("0x10") is 16 in JS; this port stays categorical.
    let hex = text_col(&["0x10", "0x20"]);
    assert_eq!(
        infer_all_fields(&hex)[0].inferred_type,
        InferredFieldType::Categorical
    );
    // Prefix regex wins even when the calendar date is impossible.
    assert_eq!(
        infer_field_type(&col("d", "text"), &[ChartValue::String("2020-13-99".into())]),
        InferredFieldType::Datetime
    );
    // Documented narrower Date.parse: "Jan 1, 2020" and "2020/01/02" are
    // date-like in JS and categorical here.
    assert_eq!(
        infer_field_type(&col("d", "text"), &[ChartValue::String("Jan 1, 2020".into())]),
        InferredFieldType::Categorical
    );
    assert_eq!(
        infer_field_type(&col("d", "text"), &[ChartValue::String("2020/01/02".into())]),
        InferredFieldType::Categorical
    );
    assert_eq!(
        infer_field_type(
            &col("d", "text"),
            &[ChartValue::String("Wed, 01 Jan 2020 00:00:00 +0000".into())]
        ),
        InferredFieldType::Datetime
    );
    assert_eq!(
        infer_field_type(
            &col("d", "text"),
            &[ChartValue::String("2020-01-02T03:04:05Z".into())]
        ),
        InferredFieldType::Datetime
    );

    let mut rows = Vec::new();
    for value in ["a", "b", "a", "c", "d", "e", "f"] {
        rows.push(vec![ChartValue::String(value.into())]);
    }
    rows.push(vec![ChartValue::Null]);
    let sampled = ChartResult {
        columns: vec![col("s", "text")],
        rows,
    };
    let field = &infer_all_fields(&sampled)[0];
    assert_eq!(field.distinct_count, 6);
    assert_eq!(
        field.sample_values,
        vec![
            "a".to_owned(),
            "b".to_owned(),
            "c".to_owned(),
            "d".to_owned(),
            "e".to_owned()
        ]
    );
    assert!(!is_chartable_result(&sampled));
    assert!(!is_chartable_result(&ChartResult {
        columns: vec![col("s", "text")],
        rows: vec![],
    }));
    assert!(is_chartable_result(&text_col(&["42"])));

    let numeric_samples = ChartResult {
        columns: vec![col("v", "text")],
        rows: vec![
            vec![ChartValue::Number(1e-7)],
            vec![ChartValue::Number(1e21)],
        ],
    };
    let numeric = &infer_all_fields(&numeric_samples)[0];
    assert_eq!(numeric.inferred_type, InferredFieldType::Numeric);
    assert_eq!(numeric.sample_values, vec!["1e-7".to_owned(), "1e+21".to_owned()]);
}
