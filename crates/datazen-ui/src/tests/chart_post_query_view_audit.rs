//! T117 independent audit (tester): fresh vectors for the post-query
//! table-vs-chart decision — different column types (float8, timestamptz,
//! a numeric-typed column the inference reads by name), different row
//! shapes, and the unset/None corners the React test pins.

use crate::support::chart_field_inference::{ChartColumn, ChartResult, ChartValue};
use crate::support::chart_post_query_view::{
    resolve_post_query_view_mode, PostQueryViewMode,
};

/// A timestamp column and a float8 column with three rows: chartable
/// through the float.
fn readings() -> ChartResult {
    ChartResult {
        columns: vec![
            ChartColumn {
                name: "taken_at".to_owned(),
                data_type: "timestamptz".to_owned(),
            },
            ChartColumn {
                name: "celsius".to_owned(),
                data_type: "float8".to_owned(),
            },
        ],
        rows: vec![
            vec![
                ChartValue::String("2026-11-19T08:00".to_owned()),
                ChartValue::Number(21.5),
            ],
            vec![
                ChartValue::String("2026-11-19T09:00".to_owned()),
                ChartValue::Number(21.8),
            ],
            vec![
                ChartValue::String("2026-11-19T10:00".to_owned()),
                ChartValue::Number(22.1),
            ],
        ],
    }
}

/// Columns but no rows: the decision is a no.
fn head_only() -> ChartResult {
    ChartResult {
        columns: vec![ChartColumn {
            name: "celsius".to_owned(),
            data_type: "float8".to_owned(),
        }],
        rows: Vec::new(),
    }
}

#[test]
fn audit_a_float_pair_charts_when_the_setting_opts_in() {
    assert_eq!(
        resolve_post_query_view_mode(Some(&readings()), Some(true)),
        PostQueryViewMode::Chart,
        "float8 is a numeric field the chart can plot"
    );
}

#[test]
fn audit_the_opt_out_and_the_unset_setting_stay_on_table() {
    for setting in [Some(false), None] {
        assert_eq!(
            resolve_post_query_view_mode(Some(&readings()), setting),
            PostQueryViewMode::Table,
            "React's `=== true` read: {setting:?} is not an opt-in"
        );
    }
}

#[test]
fn audit_a_rowless_result_never_charts() {
    assert_eq!(
        resolve_post_query_view_mode(Some(&head_only()), Some(true)),
        PostQueryViewMode::Table,
        "a header without rows has nothing to chart"
    );
}

#[test]
fn audit_a_numeric_type_name_alone_decides_the_field() {
    // The inference reads the declared type name; a `numeric` column with
    // string-form values is still the chartable field.
    let typed = ChartResult {
        columns: vec![
            ChartColumn {
                name: "label".to_owned(),
                data_type: "varchar".to_owned(),
            },
            ChartColumn {
                name: "amount".to_owned(),
                data_type: "numeric".to_owned(),
            },
        ],
        rows: vec![
            vec![ChartValue::String("a".to_owned()), ChartValue::String("12".to_owned())],
            vec![ChartValue::String("b".to_owned()), ChartValue::String("34".to_owned())],
        ],
    };
    assert_eq!(
        resolve_post_query_view_mode(Some(&typed), Some(true)),
        PostQueryViewMode::Chart,
        "the declared `numeric` column is the numeric field"
    );
}

#[test]
fn audit_only_a_true_opt_in_charts_and_every_corner_falls_back() {
    // The four-way corner grid: every combination that is not
    // (Some(true), chartable result) lands on table.
    let cases = [
        (None, Some(true)),
        (None, Some(false)),
        (None, None),
        (Some(&readings()), Some(false)),
        (Some(&readings()), None),
        (Some(&head_only()), Some(true)),
    ];
    for (result, setting) in cases {
        assert_eq!(
            resolve_post_query_view_mode(result, setting),
            PostQueryViewMode::Table,
            "the corner ({result:?}, {setting:?}) falls back to table"
        );
    }
}
