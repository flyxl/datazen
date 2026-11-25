//! T151 independent audit: chart recommendation + overlap dashes, fresh
//! vectors. The coder tests cover one scene per rule; this audit attacks
//! the rules from the other side: boundary rows/counts, cascade precedence
//! with mixed field bags, three-series overlap shapes, and the NaN-adjacent
//! coercion corners.

use std::collections::HashMap;

use datazen_ui::components::chart::chart_canvas::{AggregationType, ChartType};
use datazen_ui::support::chart_field_inference::{ChartField, ChartValue, InferredFieldType};
use datazen_ui::support::chart_recommend::{recommend_chart, ChartRecommendation};
use datazen_ui::support::chart_series_overlap::{build_overlap_dash_map, js_number, ChartSeriesPoint};

fn field(name: &str, kind: InferredFieldType, distinct: usize) -> ChartField {
    ChartField {
        name: name.to_owned(),
        data_type: "text".to_owned(),
        inferred_type: kind,
        distinct_count: distinct,
        sample_values: Vec::new(),
    }
}

fn point(entries: &[(&str, ChartValue)]) -> ChartSeriesPoint {
    entries
        .iter()
        .map(|(k, v)| ((*k).to_owned(), v.clone()))
        .collect()
}

fn num(v: f64) -> ChartValue {
    ChartValue::Number(v)
}

// ---- recommend: boundary vectors ---------------------------------------------

#[test]
fn the_pie_boundary_is_inclusive_on_both_sides() {
    // 8 distinct over 20 rows: still a pie.
    let fields = vec![
        field("kind", InferredFieldType::Categorical, 8),
        field("amount", InferredFieldType::Numeric, 20),
    ];
    assert_eq!(recommend_chart(&fields, 20).unwrap().chart_type, ChartType::Pie);
    // 9 distinct: a bar comparison instead.
    let fields = vec![
        field("kind", InferredFieldType::Categorical, 9),
        field("amount", InferredFieldType::Numeric, 20),
    ];
    assert_eq!(recommend_chart(&fields, 20).unwrap().chart_type, ChartType::Bar);
    // 21 rows with 8 distinct: a bar too.
    let fields = vec![
        field("kind", InferredFieldType::Categorical, 8),
        field("amount", InferredFieldType::Numeric, 21),
    ];
    assert_eq!(recommend_chart(&fields, 21).unwrap().chart_type, ChartType::Bar);
}

#[test]
fn the_aggregation_boundary_sits_at_half_the_axis() {
    // Ratio exactly 0.5: NOT under 0.5 — raw values.
    let fields = vec![
        field("kind", InferredFieldType::Categorical, 10),
        field("amount", InferredFieldType::Numeric, 20),
    ];
    assert_eq!(recommend_chart(&fields, 20).unwrap().aggregation, AggregationType::None);
    // Ratio 10/21 < 0.5: summed.
    let fields = vec![
        field("kind", InferredFieldType::Categorical, 10),
        field("amount", InferredFieldType::Numeric, 21),
    ];
    assert_eq!(recommend_chart(&fields, 21).unwrap().aggregation, AggregationType::Sum);
}

#[test]
fn the_trend_area_needs_strictly_more_than_five_rows() {
    let fields = vec![field("amount", InferredFieldType::Numeric, 7)];
    assert!(recommend_chart(&fields, 5).is_none());
    assert!(recommend_chart(&fields, 6).is_some());
}

#[test]
fn the_cascade_precedence_is_time_over_category_over_correlation() {
    // Datetime + numeric + categorical + extra numerics: the time rule wins
    // and the categorical never groups.
    let fields = vec![
        field("ts", InferredFieldType::Datetime, 50),
        field("kind", InferredFieldType::Categorical, 4),
        field("a", InferredFieldType::Numeric, 50),
        field("b", InferredFieldType::Numeric, 50),
        field("c", InferredFieldType::Numeric, 50),
    ];
    let rec = recommend_chart(&fields, 100).unwrap();
    assert_eq!(rec.chart_type, ChartType::Line);
    assert_eq!(rec.yAxes.len(), 3);
    assert_eq!(rec.group_by, None);
}

#[test]
fn the_group_by_takes_the_second_categorical_in_field_order() {
    let fields = vec![
        field("region", InferredFieldType::Categorical, 40),
        field("other", InferredFieldType::Categorical, 3),
        field("amount", InferredFieldType::Numeric, 200),
    ];
    let rec = recommend_chart(&fields, 200).unwrap();
    assert_eq!(rec.group_by.as_deref(), Some("other"));
    // One categorical: no grouping at all.
    let fields = vec![
        field("region", InferredFieldType::Categorical, 40),
        field("amount", InferredFieldType::Numeric, 200),
    ];
    assert_eq!(recommend_chart(&fields, 200).unwrap().group_by, None);
}

#[test]
fn booleans_and_unknowns_never_trigger_any_rule() {
    // Boolean + numeric, few rows: neither categorical nor datetime nor two
    // numerics — and the trend rule needs strictly more than five rows.
    let fields = vec![
        field("flag", InferredFieldType::Boolean, 2),
        field("amount", InferredFieldType::Numeric, 9),
    ];
    assert_eq!(recommend_chart(&fields, 5), None);
    // Unknown + numeric, few rows: no rule.
    let fields = vec![
        field("mystery", InferredFieldType::Unknown, 1),
        field("amount", InferredFieldType::Numeric, 3),
    ];
    assert_eq!(recommend_chart(&fields, 3), None);
    // The same boolean shape with many rows still takes the trend area —
    // the cascade's fourth rule never looks at the boolean.
    let fields = vec![
        field("flag", InferredFieldType::Boolean, 2),
        field("amount", InferredFieldType::Numeric, 9),
    ];
    assert_eq!(recommend_chart(&fields, 6).unwrap().chart_type, ChartType::Area);
}

// ---- overlap: fresh shapes ------------------------------------------------------

#[test]
fn two_overlap_cliques_dash_separately() {
    // Range 0..100, threshold 5: a/b overlap (diff 2), c/d overlap (diff 1),
    // the two cliques far apart.
    let data = vec![
        point(&[("a", num(0.0)), ("b", num(2.0)), ("c", num(90.0)), ("d", num(91.0))]),
        point(&[("a", num(1.0)), ("b", num(3.0)), ("c", num(90.0)), ("d", num(91.0))]),
    ];
    let map = build_overlap_dash_map(
        &data,
        &["a".to_owned(), "b".to_owned(), "c".to_owned(), "d".to_owned()],
    );
    // The dash index walks the overlapping series in yAxes order: solid,
    // dashed, dotted, dash-dot.
    assert_eq!(map.get("a"), Some(&None), "first overlapping series: solid");
    assert_eq!(map.get("b"), Some(&Some("8 4")));
    assert_eq!(map.get("c"), Some(&Some("2 4")), "third overlapping: dotted");
    assert_eq!(map.get("d"), Some(&Some("8 4 2 4")));
}

#[test]
fn a_negative_range_counts_as_zero_only_when_identical() {
    // -10..-8: range 2, threshold 0.1; diff 2 breaks the overlap.
    let data = vec![
        point(&[("a", num(-10.0)), ("b", num(-10.0))]),
        point(&[("a", num(-10.0)), ("b", num(-8.0))]),
    ];
    let map = build_overlap_dash_map(&data, &["a".to_owned(), "b".to_owned()]);
    assert_eq!(map.get("a"), Some(&None));
    assert_eq!(map.get("b"), Some(&None));

    // Identical negatives: the zero-range arm dashes all series.
    let data = vec![point(&[("a", num(-10.0)), ("b", num(-10.0))])];
    let map = build_overlap_dash_map(&data, &["a".to_owned(), "b".to_owned()]);
    assert_eq!(map.get("b"), Some(&Some("8 4")));
}

#[test]
fn more_than_six_overlapping_series_wrap_the_pattern_list() {
    // Identical values: 8 series dash in order, wrapping at 6 patterns.
    let entries: Vec<(&str, ChartValue)> = (0..8)
        .map(|i| ((format!("s{i}"), num(1.0))))
        .map(|(k, v)| {
            // leak-free: build the entry from owned data below instead
            (Box::leak(k.into_boxed_str()) as &str, v)
        })
        .collect();
    let data = vec![point(&entries)];
    let keys: Vec<String> = (0..8).map(|i| format!("s{i}")).collect();
    let map = build_overlap_dash_map(&data, &keys);
    assert_eq!(map.get("s0"), Some(&None));
    assert_eq!(map.get("s1"), Some(&Some("8 4")));
    assert_eq!(map.get("s5"), Some(&Some("4 4")));
    // s6 and s7 wrap back to the solid and dashed patterns.
    assert_eq!(map.get("s6"), Some(&None));
    assert_eq!(map.get("s7"), Some(&Some("8 4")));
}

#[test]
fn mixed_string_cells_coerce_the_js_way() {
    // Numeric strings participate; null counts as 0 (JS Number(null)).
    let data = vec![
        point(&[("a", ChartValue::String("10".into())), ("b", num(10.0))]),
        point(&[("a", ChartValue::Null), ("b", num(0.0))]),
    ];
    // a: 10 then 0; b: 10 then 0 — identical series, zero range.
    let map = build_overlap_dash_map(&data, &["a".to_owned(), "b".to_owned()]);
    assert_eq!(map.get("b"), Some(&Some("8 4")));

    // A null-only series against a numeric one: null → 0 keeps the scan on.
    let data = vec![
        point(&[("a", ChartValue::Null), ("b", num(0.0))]),
        point(&[("a", ChartValue::Null), ("b", num(0.0))]),
    ];
    let map = build_overlap_dash_map(&data, &["a".to_owned(), "b".to_owned()]);
    assert_eq!(map.get("b"), Some(&Some("8 4")), "null reads as 0, like JS");
}

#[test]
fn js_number_corners_stay_honest() {
    // The numeric string with a sign and exponent parses, like JS.
    assert_eq!(js_number(&ChartValue::String("-2.5e2".into())), Some(-250.0));
    // A float NaN value is filtered (React's isFinite).
    assert_eq!(js_number(&ChartValue::Number(f64::NAN)), None);
    // Whitespace-only strings are the empty string to JS: 0.
    assert_eq!(js_number(&ChartValue::String("  ".into())), Some(0.0));
}

#[test]
fn the_recommendation_struct_is_the_react_shape() {
    // The fields the results pane reads: the reason is an i18n KEY (the
    // caller translates), the confidence the cascade's number.
    let fields = vec![
        field("day", InferredFieldType::Datetime, 9),
        field("amount", InferredFieldType::Numeric, 9),
    ];
    let rec: ChartRecommendation = recommend_chart(&fields, 9).unwrap();
    assert!(rec.reason.starts_with("chart.recommend."));
    assert!((0.0..=1.0).contains(&rec.confidence));
    let map: HashMap<String, Option<&str>> = HashMap::new();
    let _ = map; // the overlap map's key set is the caller's y_axes, verbatim
}
