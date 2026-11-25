//! T078: `lib/chart/fieldInference.ts`.

use datazen_ui::support::chart_field_inference::{
    infer_all_fields, infer_field_type, is_chartable_result, ChartColumn, ChartField, ChartResult,
    ChartValue, InferredFieldType,
};

fn col(data_type: &str) -> ChartColumn {
    ChartColumn {
        name: "c".into(),
        data_type: data_type.into(),
    }
}

fn num(n: f64) -> ChartValue {
    ChartValue::Number(n)
}

fn s(text: &str) -> ChartValue {
    ChartValue::String(text.into())
}

#[test]
fn type_name_beats_samples() {
    assert_eq!(
        infer_field_type(&col("BOOLEAN"), &[num(1.0)]),
        InferredFieldType::Boolean
    );
    assert_eq!(
        infer_field_type(&col("int4"), &[s("nope")]),
        InferredFieldType::Numeric
    );
    // `/int/` matches `interval` — the regex runs before samples.
    assert_eq!(
        infer_field_type(&col("interval"), &[s("not a number")]),
        InferredFieldType::Numeric
    );
    assert_eq!(
        infer_field_type(&col("timestamp"), &[num(1.0)]),
        InferredFieldType::Datetime
    );
    // `/time/` matches `runtime`.
    assert_eq!(
        infer_field_type(&col("runtime"), &[]),
        InferredFieldType::Datetime
    );
}

#[test]
fn samples_classify_an_untyped_column() {
    let text = col("text");
    assert_eq!(
        infer_field_type(&text, &[ChartValue::Null, ChartValue::Null]),
        InferredFieldType::Unknown
    );
    assert_eq!(
        infer_field_type(&text, &[num(1.0), ChartValue::Null, num(2.0)]),
        InferredFieldType::Numeric
    );
    assert_eq!(
        infer_field_type(&text, &[ChartValue::Bool(true), ChartValue::Bool(false)]),
        InferredFieldType::Boolean
    );
    assert_eq!(
        infer_field_type(&text, &[s("2020-01-02"), s("2020-03-04T00:00:00Z")]),
        InferredFieldType::Datetime
    );
    assert_eq!(
        infer_field_type(&text, &[s("1"), s(" 2.5 "), num(3.0)]),
        InferredFieldType::Numeric
    );
    assert_eq!(
        infer_field_type(&text, &[s("a"), s("b")]),
        InferredFieldType::Categorical
    );
    assert_eq!(
        infer_field_type(&text, &[s(""), s(" ")]),
        InferredFieldType::Categorical
    );
    // JS `Number`: exact Infinity spellings only. `infinity` and `0x10`
    // are not numeric (`0x10` is the documented hex deviation).
    assert_eq!(
        infer_field_type(&text, &[s("Infinity"), s("+Infinity"), s("-Infinity")]),
        InferredFieldType::Numeric
    );
    assert_eq!(
        infer_field_type(&text, &[s("infinity")]),
        InferredFieldType::Categorical
    );
    assert_eq!(
        infer_field_type(&text, &[s("0x10"), s("0b10")]),
        InferredFieldType::Categorical
    );
    assert_eq!(
        infer_field_type(&text, &[s("1e999")]),
        InferredFieldType::Numeric
    );
}

#[test]
fn infer_all_fields_samples_fifty_and_keeps_five_distinct() {
    let mut rows = Vec::new();
    for i in 0..60 {
        rows.push(vec![s(&format!("v{i}")), num(i as f64)]);
    }
    let result = ChartResult {
        columns: vec![
            ChartColumn {
                name: "label".into(),
                data_type: "text".into(),
            },
            ChartColumn {
                name: "n".into(),
                data_type: "int".into(),
            },
        ],
        rows,
    };
    let fields = infer_all_fields(&result);
    assert_eq!(fields.len(), 2);
    assert_eq!(fields[0].distinct_count, 50);
    assert_eq!(
        fields[0].sample_values,
        vec!["v0", "v1", "v2", "v3", "v4"]
    );
    assert_eq!(fields[0].inferred_type, InferredFieldType::Categorical);
    assert_eq!(fields[1].inferred_type, InferredFieldType::Numeric);
    assert_eq!(fields[1].name, "n");
    assert!(is_chartable_result(&result));
}

#[test]
fn empty_result_is_not_chartable_and_text_without_a_number_is_not() {
    let empty = ChartResult {
        columns: vec![col("int")],
        rows: vec![],
    };
    assert!(!is_chartable_result(&empty));
    let no_cols = ChartResult {
        columns: vec![],
        rows: vec![vec![num(1.0)]],
    };
    assert!(!is_chartable_result(&no_cols));
    let labels = ChartResult {
        columns: vec![ChartColumn {
            name: "label".into(),
            data_type: "text".into(),
        }],
        rows: vec![vec![s("a")], vec![s("b")]],
    };
    assert!(!is_chartable_result(&labels));
    let fields = infer_all_fields(&labels);
    assert_eq!(
        fields,
        vec![ChartField {
            name: "label".into(),
            data_type: "text".into(),
            inferred_type: InferredFieldType::Categorical,
            distinct_count: 2,
            sample_values: vec!["a".into(), "b".into()],
        }]
    );
}

#[test]
fn nulls_do_not_enter_the_distinct_set_and_objects_stringify() {
    let result = ChartResult {
        columns: vec![ChartColumn {
            name: "v".into(),
            data_type: "json".into(),
        }],
        rows: vec![
            vec![ChartValue::Null],
            vec![ChartValue::Object],
            vec![ChartValue::Array(vec![num(1.0), s("a")])],
            vec![ChartValue::Object],
        ],
    };
    let fields = infer_all_fields(&result);
    assert_eq!(fields[0].distinct_count, 2);
    assert_eq!(
        fields[0].sample_values,
        vec!["[object Object]", "1,a"]
    );
    assert_eq!(fields[0].inferred_type, InferredFieldType::Categorical);
}
