//! Independent audit of T078 field inference
//! (`packages/frontend/src/lib/chart/fieldInference.ts`).
//!
//! Expected labels are the React results, not a second call into the
//! helpers under test. Fresh columns (not the coder's `int4` / `v0`
//! fixtures): `smallint` and `float8` take the type-name branch,
//! `timestamptz` is datetime, a `BOOLEAN` column ignores a numeric
//! sample, and the documented `Number` / `Date.parse` deviations are
//! pinned (`0x10` and `infinity` are categorical; `Jan 1, 2020` is
//! categorical here even though JS `Date.parse` accepts it).

use datazen_ui::support::chart_field_inference::{
    infer_all_fields, infer_field_type, is_chartable_result, ChartColumn, ChartResult, ChartValue,
    InferredFieldType,
};

fn col(name: &str, data_type: &str) -> ChartColumn {
    ChartColumn {
        name: name.into(),
        data_type: data_type.into(),
    }
}

fn s(text: &str) -> ChartValue {
    ChartValue::String(text.into())
}

#[test]
fn audit_type_name_branch_uses_substring_not_the_samples() {
    assert_eq!(
        infer_field_type(&col("n", "smallint"), &[s("nope")]),
        InferredFieldType::Numeric
    );
    assert_eq!(
        infer_field_type(&col("n", "float8"), &[s("nope")]),
        InferredFieldType::Numeric
    );
    assert_eq!(
        infer_field_type(&col("t", "timestamptz"), &[s("nope")]),
        InferredFieldType::Datetime
    );
    assert_eq!(
        infer_field_type(
            &col("b", "BOOLEAN"),
            &[ChartValue::Number(1.0), s("nope")]
        ),
        InferredFieldType::Boolean
    );
}

#[test]
fn audit_js_number_and_date_parse_deviations() {
    let text = col("label", "text");
    assert_eq!(
        infer_field_type(&text, &[s("0x10")]),
        InferredFieldType::Categorical,
        "JS Number(\"0x10\") is 16; this port must not treat it as numeric"
    );
    assert_eq!(
        infer_field_type(&text, &[s("infinity"), s("NaN")]),
        InferredFieldType::Categorical
    );
    assert_eq!(
        infer_field_type(&text, &[s("Infinity"), s("1e999")]),
        InferredFieldType::Numeric
    );
    // `/^\\d{4}-\\d{2}-\\d{2}/` does not care that the calendar is invalid.
    assert_eq!(
        infer_field_type(&text, &[s("2024-13-99")]),
        InferredFieldType::Datetime
    );
    // JS Date.parse("Jan 1, 2020") is a timestamp. The port only accepts
    // the ISO prefix plus RFC3339/RFC2822, so this stays categorical.
    assert_eq!(
        infer_field_type(&text, &[s("Jan 1, 2020")]),
        InferredFieldType::Categorical
    );
}

#[test]
fn audit_distinct_set_is_first_seen_string_forms_and_chartable_needs_a_number() {
    let mut rows = Vec::new();
    for i in 0..55 {
        rows.push(vec![
            if i == 3 {
                ChartValue::Null
            } else {
                s(&format!("k{i}"))
            },
            s("1"),
        ]);
    }
    let result = ChartResult {
        columns: vec![col("label", "text"), col("qty", "text")],
        rows,
    };
    let fields = infer_all_fields(&result);
    // 55 rows, sample cap 50, one null inside that window → 49 distinct.
    assert_eq!(fields[0].distinct_count, 49);
    assert_eq!(
        fields[0].sample_values,
        vec!["k0", "k1", "k2", "k4", "k5"]
    );
    assert_eq!(fields[1].inferred_type, InferredFieldType::Numeric);
    assert_eq!(fields[1].distinct_count, 1);
    assert_eq!(fields[1].sample_values, vec!["1".to_owned()]);
    assert!(is_chartable_result(&result));

    let only_infinity = ChartResult {
        columns: vec![col("label", "text")],
        rows: vec![vec![s("infinity")]],
    };
    assert!(!is_chartable_result(&only_infinity));
}
