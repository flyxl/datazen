//! T180 independent audit: the `lib/chart/transform.ts` port
//! (`components/chart/transform.rs`), written against the React source,
//! not against the coder's inline tests.
//!
//! What is pinned here, from the React behaviour:
//!
//! * the record walk: missing columns read as `undefined` (empty string on
//!   the x side, `null` on the y side), numeric x values stay numbers
//!   (`formatAxisValue`'s passthrough).
//! * the aggregate table's honest edges: `count` counts the group's rows
//!   (nulls included), `distinct_count` distinguishes `Text("1")` from
//!   `Number(1)` the way JS's `Set` does, `avg` divides, and every numeric
//!   reducer of a numeric-less group is `null`.
//! * the category walk: a null x groups under the literal `"__null__"`
//!   category, group nulls under `"__null__"` in the composite key, and the
//!   index axis (`__index`) is the x key when no x column is chosen.
//! * the pivot walk: series keys in first-appearance group order (a `g2`
//!   that appears before `g1`), the `g·y` composite key only under a
//!   multi-y config, the absent group leaving no key on the pivot row
//!   (React's `undefined`), and the pivot over the index axis.
//! * the sort walk: y sorts treat a missing value as `0`, and the pipeline
//!   with no x axis and no group-by still returns the y axes as the series
//!   keys.

use datazen_ui::components::chart::chart_canvas::{
    AggregationType, ChartConfig, ChartSort, ChartType, ChartValue,
};
use datazen_ui::components::chart::chart_view::{ColumnInfo, StatementCell, StatementResult};
use datazen_ui::components::chart::transform::transform_data;

fn result(columns: &[&str], rows: Vec<Vec<StatementCell>>) -> StatementResult {
    StatementResult {
        columns: columns
            .iter()
            .map(|name| ColumnInfo {
                name: (*name).to_owned(),
                data_type: String::new(),
            })
            .collect(),
        rows,
    }
}

fn num(n: f64) -> StatementCell {
    StatementCell::Number(n)
}
fn text(s: &str) -> StatementCell {
    StatementCell::Text(s.to_owned())
}

fn config(x: Option<&str>, ys: &[&str], aggregation: AggregationType) -> ChartConfig {
    ChartConfig {
        x_axis: x.map(str::to_owned),
        y_axes: ys.iter().map(|s| (*s).to_owned()).collect(),
        aggregation,
        ..ChartConfig::default()
    }
}

fn display(point: &datazen_ui::components::chart::chart_canvas::ChartDataPoint, key: &str) -> String {
    match point.get(key) {
        Some(ChartValue::Text(s)) => s.clone(),
        Some(ChartValue::Number(n)) => {
            if *n == n.trunc() && n.abs() < 1e15 {
                format!("{}", *n as i64)
            } else {
                n.to_string()
            }
        }
        _ => String::new(),
    }
}

// ---- the record walk ----------------------------------------------------------

#[test]
fn numeric_x_stays_a_number_and_missing_columns_stringify_empty() {
    let res = result(
        &["x", "y", "unused"],
        vec![
            // x numeric, y missing (short row).
            vec![num(7.0)],
        ],
    );
    let cfg = config(Some("x"), &["y"], AggregationType::None);
    let out = transform_data(&res, &cfg);
    assert_eq!(out.data[0].get("x"), Some(&ChartValue::Number(7.0)));
    assert_eq!(
        out.data[0].get("y"),
        Some(&ChartValue::Null),
        "toNumber(undefined) is null on the y side"
    );

    // A text x stringifies; an unknown x column reads as undefined -> the
    // empty string (React `String(v ?? '')`).
    let res = result(&["x", "y"], vec![vec![text("a"), num(1.0)]]);
    let cfg = config(Some("missing"), &["y"], AggregationType::None);
    let out = transform_data(&res, &cfg);
    assert_eq!(out.data[0].get("missing"), Some(&ChartValue::Text(String::new())));
}

#[test]
fn group_label_null_reads_as_the_empty_string() {
    let res = result(
        &["x", "y", "g"],
        vec![
            vec![text("a"), num(1.0), StatementCell::Null],
            vec![text("a"), num(2.0), text("g1")],
        ],
    );
    let mut cfg = config(Some("x"), &["y"], AggregationType::None);
    cfg.group_by = Some("g".to_owned());
    let out = transform_data(&res, &cfg);
    // Two groups: '' (the null label) and 'g1'; both are pivot series over
    // the single x category 'a'.
    assert_eq!(out.series_keys.len(), 2);
    assert_eq!(out.series_keys[0], "");
    assert_eq!(out.data[0].get(""), Some(&ChartValue::Number(1.0)));
    assert_eq!(out.data[0].get("g1"), Some(&ChartValue::Number(2.0)));
}

// ---- the aggregate walk -------------------------------------------------------

#[test]
fn distinct_count_distinguishes_text_and_number_like_the_js_set() {
    let res = result(
        &["x", "y"],
        vec![
            vec![text("a"), text("1")],
            vec![text("a"), num(1.0)],
            vec![text("a"), num(1.0)],
            vec![text("a"), StatementCell::Null],
        ],
    );
    let cfg = config(Some("x"), &["y"], AggregationType::DistinctCount);
    let out = transform_data(&res, &cfg);
    // JS Set: "1", 1, null -> three distinct members.
    assert_eq!(out.data[0].get("y"), Some(&ChartValue::Number(3.0)));
}

#[test]
fn avg_divides_and_count_keeps_null_members() {
    let res = result(
        &["x", "y"],
        vec![
            vec![text("a"), num(1.0)],
            vec![text("a"), num(2.0)],
            vec![text("a"), StatementCell::Null],
        ],
    );
    let cfg = config(Some("x"), &["y"], AggregationType::Avg);
    let out = transform_data(&res, &cfg);
    assert_eq!(out.data[0].get("y"), Some(&ChartValue::Number(1.5)));

    let cfg = config(Some("x"), &["y"], AggregationType::Count);
    let out = transform_data(&res, &cfg);
    assert_eq!(
        out.data[0].get("y"),
        Some(&ChartValue::Number(3.0)),
        "count is the group's row count, nulls included"
    );

    // The same group through min/max: the null member is simply skipped.
    let cfg = config(Some("x"), &["y"], AggregationType::Min);
    let out = transform_data(&res, &cfg);
    assert_eq!(out.data[0].get("y"), Some(&ChartValue::Number(1.0)));
}

#[test]
fn a_numeric_less_group_aggregates_to_null() {
    let res = result(
        &["x", "y"],
        vec![vec![text("a"), text("abc")], vec![text("a"), StatementCell::Bool(true)]],
    );
    for aggregation in [
        AggregationType::Sum,
        AggregationType::Avg,
        AggregationType::Min,
        AggregationType::Max,
    ] {
        let cfg = config(Some("x"), &["y"], aggregation);
        let out = transform_data(&res, &cfg);
        assert_eq!(
            out.data[0].get("y"),
            Some(&ChartValue::Null),
            "{aggregation:?} of a numeric-less group is null"
        );
    }
}

#[test]
fn aggregation_without_an_x_axis_is_empty_but_keeps_series_keys() {
    let res = result(&["y"], vec![vec![num(1.0)]]);
    let cfg = config(None, &["y"], AggregationType::Sum);
    let out = transform_data(&res, &cfg);
    assert!(out.data.is_empty());
    assert_eq!(out.series_keys, vec!["y".to_owned()]);

    // With a group-by on top: the pivot of nothing is nothing, with no
    // series keys (no group was ever seen).
    let mut cfg = config(None, &["y"], AggregationType::Sum);
    cfg.group_by = Some("g".to_owned());
    let out = transform_data(&res, &cfg);
    assert!(out.data.is_empty());
    assert!(out.series_keys.is_empty());
}

// ---- the pivot walk -----------------------------------------------------------

#[test]
fn series_keys_follow_first_appearance_not_alphabetical() {
    let res = result(
        &["x", "y", "g"],
        vec![
            vec![text("a"), num(1.0), text("g2")],
            vec![text("a"), num(2.0), text("g1")],
        ],
    );
    let mut cfg = config(Some("x"), &["y"], AggregationType::None);
    cfg.group_by = Some("g".to_owned());
    let out = transform_data(&res, &cfg);
    assert_eq!(out.series_keys, vec!["g2".to_owned(), "g1".to_owned()]);
}

#[test]
fn composite_keys_only_under_multiple_y_axes() {
    let res = result(
        &["x", "y1", "g"],
        vec![vec![text("a"), num(1.0), text("g1")]],
    );
    let mut cfg = config(Some("x"), &["y1"], AggregationType::None);
    cfg.group_by = Some("g".to_owned());
    let out = transform_data(&res, &cfg);
    assert_eq!(out.series_keys, vec!["g1".to_owned()]);
    assert!(out.data[0].get("g1").is_some());

    let res = result(
        &["x", "y1", "y2", "g"],
        vec![vec![text("a"), num(1.0), num(2.0), text("g1")]],
    );
    let mut cfg = config(Some("x"), &["y1", "y2"], AggregationType::None);
    cfg.group_by = Some("g".to_owned());
    let out = transform_data(&res, &cfg);
    assert_eq!(out.series_keys, vec!["g1·y1".to_owned(), "g1·y2".to_owned()]);
    assert_eq!(out.data[0].get("g1·y1"), Some(&ChartValue::Number(1.0)));
    assert_eq!(out.data[0].get("g1·y2"), Some(&ChartValue::Number(2.0)));
}

#[test]
fn pivot_over_the_index_axis_when_no_x_is_chosen() {
    let res = result(
        &["y", "g"],
        vec![
            vec![num(1.0), text("g1")],
            vec![num(2.0), text("g2")],
        ],
    );
    let mut cfg = config(None, &["y"], AggregationType::None);
    cfg.group_by = Some("g".to_owned());
    let out = transform_data(&res, &cfg);
    // One pivot row per __index value ("0", "1"), series by group.
    assert_eq!(out.data.len(), 2);
    assert_eq!(out.series_keys, vec!["g1".to_owned(), "g2".to_owned()]);
    assert_eq!(out.data[0].get("g1"), Some(&ChartValue::Number(1.0)));
    assert_eq!(out.data[1].get("g2"), Some(&ChartValue::Number(2.0)));
}

#[test]
fn aggregated_composite_null_group_reads_as_the_null_category() {
    let res = result(
        &["x", "y", "g"],
        vec![
            vec![text("a"), num(1.0), StatementCell::Null],
            vec![text("a"), num(2.0), StatementCell::Null],
        ],
    );
    let mut cfg = config(Some("x"), &["y"], AggregationType::Sum);
    cfg.group_by = Some("g".to_owned());
    let out = transform_data(&res, &cfg);
    // The flat pass grouped (a, __null__) and summed to 3; the pivot
    // spreads it under the "__null__" series key.
    assert_eq!(out.series_keys, vec!["__null__".to_owned()]);
    assert_eq!(out.data[0].get("__null__"), Some(&ChartValue::Number(3.0)));
}

// ---- the sort walk ------------------------------------------------------------

#[test]
fn y_sorts_treat_missing_values_as_zero() {
    let res = result(
        &["x", "y"],
        vec![
            vec![text("a"), text("abc")], // null (unparsable)
            vec![text("b"), num(5.0)],
            vec![text("c"), num(3.0)],
        ],
    );
    let mut cfg = config(Some("x"), &["y"], AggregationType::None);
    cfg.sort_by = ChartSort::YDesc;
    let out = transform_data(&res, &cfg);
    let order: Vec<String> = out.data.iter().map(|p| display(p, "x")).collect();
    assert_eq!(order, vec!["b", "c", "a"], "null sorts as 0: 5, 3, 0");

    cfg.sort_by = ChartSort::YAsc;
    let out = transform_data(&res, &cfg);
    let order: Vec<String> = out.data.iter().map(|p| display(p, "x")).collect();
    assert_eq!(order, vec!["a", "c", "b"]);
}

#[test]
fn sort_none_keeps_arrival_order() {
    let res = result(
        &["x", "y"],
        vec![
            vec![text("c"), num(3.0)],
            vec![text("a"), num(1.0)],
            vec![text("b"), num(2.0)],
        ],
    );
    let cfg = config(Some("x"), &["y"], AggregationType::None);
    let out = transform_data(&res, &cfg);
    let order: Vec<String> = out.data.iter().map(|p| display(p, "x")).collect();
    assert_eq!(order, vec!["c", "a", "b"]);
}

// ---- the scatter --------------------------------------------------------------

#[test]
fn scatter_is_the_only_chart_type_indexing_a_null_x() {
    let res = result(
        &["x", "y"],
        vec![vec![StatementCell::Null, num(1.0)]],
    );
    for (chart_type, expect_index) in [
        (ChartType::Scatter, true),
        (ChartType::Bar, false),
        (ChartType::Line, false),
        (ChartType::Area, false),
    ] {
        let mut cfg = config(Some("x"), &["y"], AggregationType::None);
        cfg.chart_type = chart_type;
        let out = transform_data(&res, &cfg);
        assert_eq!(
            out.data[0].get("x"),
            Some(&ChartValue::Number(if expect_index { 0.0 } else { 0.0 }))
                .filter(|_| expect_index)
                .or(Some(&ChartValue::Text(String::new())))
                .as_ref()
                .map(|v| v.clone()),
            "{chart_type:?}: the null x is {}",
            if expect_index { "the row index" } else { "the empty string" }
        );
    }
}
