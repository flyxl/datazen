//! T180: the `lib/chart/transform.ts` port — the chart family's data
//! pipeline, from a `StatementResult` and a `ChartConfig` to the plot rows
//! and the series keys.
//!
//! React's `transformData` is the seam `ChartView` injects as
//! `ChartPipeline::transform` (see `chart_view.rs`); this module is the
//! production implementation of that member, with the aggregate / pivot /
//! sort walk kept branch-for-branch against the source:
//!
//! * `none` aggregation → [`transform_direct`]: one point per record, the x
//!   value formatted (scatter's null-x becomes the row index), every y
//!   coerced through `toNumber`, the group label stringified.
//! * any other aggregation → [`transform_aggregated`]: group by the x value
//!   (null reads as the literal `"__null__"` category, exactly the React
//!   key), composite `x\0g` groups when a group-by is set, each y reduced by
//!   [`aggregate`] — `count` counts the group's rows (nulls included, the
//!   React `values.length`), `distinct_count` the group's distinct raw
//!   values.
//! * a group-by set → [`pivot_by_group`]: rows pivoted wide, one row per x
//!   category in first-appearance order, one series per group × y with the
//!   `g·y` composite key exactly when more than one y axis is in force.
//! * the final [`sort_data`]: x sorts compare the stringified axis values
//!   (codepoint order — the honest stand-in for `localeCompare`), y sorts
//!   the numeric coercion with `0` for missing values.
//!
//! The numeric coercion and the number-to-string form reuse the engine
//! family's own [`js_string`] / [`to_number`] (`renderers/
//! area_chart_renderer.rs`, T166's ruling).
//!
//! ## Deliberate deviations from the React source
//!
//! * **Group order is first-appearance**, not `HashMap` order: React's
//!   `Map` iterates in insertion order, so the port keeps an explicit
//!   order list — a `BTreeMap` would alphabetize categories React left in
//!   arrival order.
//! * **`localeCompare` is codepoint order** (`str::cmp`): ICU collation is
//!   not in the runtime; typical chart categories (identifiers, dates as
//!   ISO strings) sort identically, and the difference is documented here
//!   rather than silently approximated.
//! * **`distinct_count` compares values, not references**: React's
//!   `new Set(values)` distinguishes two equal non-scalar cells by
//!   identity; the wire has already erased object identity, so two
//!   [`StatementCell::NonScalar`] cells count once — the honest reading.
//! * **A non-finite numeric string** (`"Infinity"` parses under Rust) reads
//!   as `null` here, where JS's `Number("Infinity")` would keep it; the
//!   only reachable inputs (JSON numbers) cannot carry it.
//! * **Absent group/y cells** (rows shorter than the column list) map as
//!   React's `undefined`: `?? ''` on the string side, `Null` on the value
//!   side.

use std::collections::HashMap;

use crate::components::chart::chart_canvas::{
    AggregationType, ChartConfig, ChartDataPoint, ChartSort, ChartType, ChartValue,
};
use crate::components::chart::chart_view::{StatementCell, StatementResult, TransformResult};
use crate::components::chart::renderers::area_chart_renderer::{js_string, to_number};

/// The pivot output's group label key (`'__group__'`).
const GROUP_KEY: &str = "__group__";
/// The index axis when no x column is chosen (`'__index'`).
const INDEX_KEY: &str = "__index";
/// The category a null x value groups under (`'__null__'`).
const NULL_CATEGORY: &str = "__null__";
/// The composite series key separator (`'·'`).
const SERIES_SEPARATOR: &str = "·";

/// React `transformData`: the full pipeline.
pub(crate) fn transform_data(
    result: &StatementResult,
    config: &ChartConfig,
) -> TransformResult {
    // React builds `records` (name-keyed row objects) once; the port keeps
    // the row slices plus a name → index map, the same lookup without the
    // per-row allocation.
    let mut index: HashMap<&str, usize> = HashMap::with_capacity(result.columns.len());
    for (i, col) in result.columns.iter().enumerate() {
        index.insert(col.name.as_str(), i);
    }

    let flat = if config.aggregation == AggregationType::None {
        transform_direct(&result.rows, &index, config)
    } else {
        transform_aggregated(&result.rows, &index, config)
    };

    if config.group_by.is_some() {
        pivot_by_group(&flat, config)
    } else {
        TransformResult {
            data: sort_data(flat, config),
            series_keys: config.y_axes.clone(),
        }
    }
}

/// The cell at `column` of a row — `None` when the column is unknown or the
/// row is short (React's `undefined`).
fn cell_at<'a>(row: &'a [StatementCell], index: &HashMap<&str, usize>, column: &str) -> Option<&'a StatementCell> {
    index.get(column).and_then(|i| row.get(*i))
}

/// `String(v ?? '')` over the raw cell space.
fn cell_display(cell: Option<&StatementCell>) -> String {
    match cell {
        Some(StatementCell::Text(s)) => s.clone(),
        Some(StatementCell::Number(n)) => js_string(*n),
        Some(StatementCell::Bool(b)) => if *b { "true" } else { "false" }.to_owned(),
        Some(StatementCell::Null) | None => String::new(),
        // JS `String({})` for the record branch; arrays join, but the
        // non-scalar payload is opaque on the wire either way.
        Some(StatementCell::NonScalar) => "[object Object]".to_owned(),
    }
}

/// `toNumber` over the raw cell space (bool / null / objects fail).
fn cell_to_number(cell: Option<&StatementCell>) -> Option<f64> {
    match cell {
        Some(StatementCell::Number(n)) => Some(*n),
        Some(StatementCell::Text(s)) => to_number(&ChartValue::Text(s.clone())),
        _ => None,
    }
}

/// `formatAxisValue`: numbers stay numbers, everything else stringifies
/// (null / missing → the empty string).
fn format_axis_value(cell: Option<&StatementCell>) -> ChartValue {
    match cell {
        Some(StatementCell::Number(n)) => ChartValue::Number(*n),
        other => ChartValue::Text(cell_display(other)),
    }
}

/// The category key of the x cell: `String(rec[xAxis] ?? '__null__')`.
fn x_category(cell: Option<&StatementCell>) -> String {
    match cell {
        Some(StatementCell::Null) | None => NULL_CATEGORY.to_owned(),
        Some(StatementCell::Text(s)) => s.clone(),
        Some(StatementCell::Number(n)) => js_string(*n),
        Some(StatementCell::Bool(b)) => if *b { "true" } else { "false" }.to_owned(),
        Some(StatementCell::NonScalar) => "[object Object]".to_owned(),
    }
}

/// React `transformDirect` — one point per record.
fn transform_direct(
    rows: &[Vec<StatementCell>],
    index: &HashMap<&str, usize>,
    config: &ChartConfig,
) -> Vec<ChartDataPoint> {
    rows.iter()
        .enumerate()
        .map(|(idx, row)| {
            let mut point = ChartDataPoint::new();
            match &config.x_axis {
                Some(x_axis) => {
                    let x_val = cell_at(row, index, x_axis);
                    if matches!(x_val, Some(StatementCell::Null) | None)
                        && config.chart_type == ChartType::Scatter
                    {
                        // A scatter with a null x plots at the row index.
                        point.insert(x_axis.clone(), ChartValue::Number(idx as f64));
                    } else {
                        point.insert(x_axis.clone(), format_axis_value(x_val));
                    }
                }
                None => {
                    point.insert(INDEX_KEY, ChartValue::Number(idx as f64));
                }
            }
            for y in &config.y_axes {
                point.insert(
                    y.clone(),
                    cell_to_number(cell_at(row, index, y))
                        .map(ChartValue::Number)
                        .unwrap_or(ChartValue::Null),
                );
            }
            if let Some(group) = &config.group_by {
                point.insert(GROUP_KEY, ChartValue::Text(cell_display(cell_at(row, index, group))));
            }
            point
        })
        .collect()
}

/// React `transformAggregated` — one point per x (or x × group) category,
/// each y reduced by [`aggregate`].
fn transform_aggregated(
    rows: &[Vec<StatementCell>],
    index: &HashMap<&str, usize>,
    config: &ChartConfig,
) -> Vec<ChartDataPoint> {
    let Some(x_axis) = config.x_axis.clone() else {
        return Vec::new();
    };

    // First-appearance grouping, like React's `Map` iteration order.
    let mut order: Vec<String> = Vec::new();
    let mut groups: HashMap<String, Vec<usize>> = HashMap::new();
    let composite = config.group_by.clone();
    for (row_index, row) in rows.iter().enumerate() {
        let x_val = x_category(cell_at(row, index, &x_axis));
        let key = match &composite {
            Some(group) => {
                let g_val = x_category(cell_at(row, index, group));
                format!("{x_val}\0{g_val}")
            }
            None => x_val,
        };
        if !groups.contains_key(&key) {
            order.push(key.clone());
        }
        groups.entry(key).or_default().push(row_index);
    }

    order
        .into_iter()
        .map(|key| {
            // React splits the composite key back at the `\0`.
            let (x_val, g_val) = match &composite {
                Some(_) => match key.split_once('\0') {
                    Some((x, g)) => (x.to_owned(), g.to_owned()),
                    // An x value that itself contained a `\0`: React's
                    // `split('\0')` takes the first two pieces the same way.
                    None => (key.clone(), String::new()),
                },
                None => (key.clone(), String::new()),
            };
            let mut point = ChartDataPoint::new();
            point.insert(x_axis.clone(), ChartValue::Text(x_val));
            if composite.is_some() {
                point.insert(GROUP_KEY, ChartValue::Text(g_val));
            }
            let members = groups
                .remove(&key)
                .expect("every ordered key has a group");
            for y in &config.y_axes {
                let values: Vec<Option<&StatementCell>> = members
                    .iter()
                    .map(|row_index| cell_at(&rows[*row_index], index, y))
                    .collect();
                point.insert(
                    y.clone(),
                    aggregate(&values, config.aggregation)
                        .map(ChartValue::Number)
                        .unwrap_or(ChartValue::Null),
                );
            }
            point
        })
        .collect()
}

/// React `pivotByGroup` — one row per x category, one series per group × y.
fn pivot_by_group(flat: &[ChartDataPoint], config: &ChartConfig) -> TransformResult {
    let x_key = config.x_axis.clone().unwrap_or_else(|| INDEX_KEY.to_owned());

    // The distinct group values, in first-appearance order.
    let mut group_values: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for point in flat {
        let g_val = value_display(point.get(GROUP_KEY));
        if seen.insert(g_val.clone()) {
            group_values.push(g_val);
        }
    }

    let series_key = |g_val: &str, y: &str| {
        if config.y_axes.len() > 1 {
            format!("{g_val}{SERIES_SEPARATOR}{y}")
        } else {
            g_val.to_owned()
        }
    };

    // The pivot rows, keyed by the stringified x value.
    let mut pivot_order: Vec<String> = Vec::new();
    let mut pivot: HashMap<String, ChartDataPoint> = HashMap::new();
    for point in flat {
        let x_val = match point.get(&x_key) {
            Some(value) => value_display(Some(value)),
            None => String::new(),
        };
        if !pivot.contains_key(&x_val) {
            let mut row = ChartDataPoint::new();
            // React seeds the row with the ORIGINAL x value under the key.
            if let Some(x) = point.get(&x_key) {
                row.insert(x_key.clone(), x.clone());
            }
            pivot.insert(x_val.clone(), row);
            pivot_order.push(x_val.clone());
        }
        let row = pivot
            .get_mut(&x_val)
            .expect("the row was just inserted");
        let g_val = value_display(point.get(GROUP_KEY));
        for y in &config.y_axes {
            let key = series_key(&g_val, y);
            let value = point.get(y).cloned().unwrap_or(ChartValue::Null);
            row.insert(key, value);
        }
    }

    let series_keys: Vec<String> = group_values
        .iter()
        .flat_map(|g_val| {
            config
                .y_axes
                .iter()
                .map(|y| series_key(g_val, y))
                .collect::<Vec<_>>()
        })
        .collect();

    let data: Vec<ChartDataPoint> = pivot_order
        .into_iter()
        .map(|x_val| {
            pivot
                .remove(&x_val)
                .expect("every ordered key has a row")
        })
        .collect();
    TransformResult {
        data: sort_data(data, config),
        series_keys,
    }
}

/// `String(point[key] ?? '')` over the point's value space.
fn value_display(value: Option<&ChartValue>) -> String {
    match value {
        Some(ChartValue::Text(s)) => s.clone(),
        Some(ChartValue::Number(n)) => js_string(*n),
        Some(ChartValue::Null) | None => String::new(),
    }
}

/// React `aggregate` — one reducer over the group's raw values.
fn aggregate(values: &[Option<&StatementCell>], aggregation: AggregationType) -> Option<f64> {
    let nums: Vec<f64> = values
        .iter()
        .filter_map(|value| cell_to_number(*value))
        .collect();
    match aggregation {
        AggregationType::Count => {
            // React counts the group's rows, nulls included.
            return Some(values.len() as f64);
        }
        AggregationType::DistinctCount => {
            // Distinct raw values by value (the module doc's reference
            // ruling).
            let mut distinct: Vec<StatementCell> = Vec::new();
            for value in values.iter().flatten() {
                if !distinct.iter().any(|seen| seen == *value) {
                    distinct.push((*value).clone());
                }
            }
            return Some(distinct.len() as f64);
        }
        _ => {}
    }
    if nums.is_empty() {
        return None;
    }
    let sum: f64 = nums.iter().sum();
    match aggregation {
        AggregationType::Sum => Some(sum),
        AggregationType::Avg => Some(sum / nums.len() as f64),
        AggregationType::Min => Some(nums.iter().copied().fold(f64::INFINITY, f64::min)),
        AggregationType::Max => Some(nums.iter().copied().fold(f64::NEG_INFINITY, f64::max)),
        // `none` never reaches the aggregated path; the TS default returns
        // null for anything the enum does not name.
        AggregationType::None | AggregationType::Count | AggregationType::DistinctCount => None,
    }
}

/// React `sortData` — the final ordering, stable like V8's sort.
fn sort_data(data: Vec<ChartDataPoint>, config: &ChartConfig) -> Vec<ChartDataPoint> {
    if config.sort_by == ChartSort::None {
        return data;
    }
    let x_key = config.x_axis.clone().unwrap_or_else(|| INDEX_KEY.to_owned());
    let Some(y_key) = config.y_axes.first().cloned() else {
        return data;
    };

    let x_of = |point: &ChartDataPoint| value_display(point.get(&x_key));
    let y_of = |point: &ChartDataPoint| to_number(point.get(&y_key).unwrap_or(&ChartValue::Null)).unwrap_or(0.0);

    let mut data = data;
    match config.sort_by {
        ChartSort::XAsc => data.sort_by(|a, b| x_of(a).cmp(&x_of(b))),
        ChartSort::XDesc => data.sort_by(|a, b| x_of(b).cmp(&x_of(a))),
        ChartSort::YAsc => data.sort_by(|a, b| {
            y_of(a)
                .partial_cmp(&y_of(b))
                .unwrap_or(std::cmp::Ordering::Equal)
        }),
        ChartSort::YDesc => data.sort_by(|a, b| {
            y_of(b)
                .partial_cmp(&y_of(a))
                .unwrap_or(std::cmp::Ordering::Equal)
        }),
        ChartSort::None => {}
    }
    data
}

#[cfg(test)]
mod transform_tests {
    use super::*;
    use crate::components::chart::chart_canvas::ChartType;
    use crate::components::chart::chart_view::{ColumnInfo, StatementCell};

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

    fn x_of(point: &ChartDataPoint) -> String {
        point.get("x").map(|v| value_display(Some(v))).unwrap_or_default()
    }

    #[test]
    fn direct_transform_maps_x_and_ys() {
        let res = result(
            &["x", "y"],
            vec![
                vec![text("a"), num(1.0)],
                vec![text("b"), num(2.5)],
            ],
        );
        let cfg = config(Some("x"), &["y"], AggregationType::None);
        let out = transform_data(&res, &cfg);
        assert_eq!(out.series_keys, vec!["y".to_owned()]);
        assert_eq!(out.data.len(), 2);
        assert_eq!(x_of(&out.data[0]), "a");
        assert_eq!(
            out.data[0].get("y"),
            Some(&ChartValue::Number(1.0))
        );
        assert_eq!(
            out.data[1].get("y"),
            Some(&ChartValue::Number(2.5))
        );
    }

    #[test]
    fn no_x_axis_uses_the_index_key() {
        let res = result(&["y"], vec![vec![num(3.0)], vec![num(4.0)]]);
        let cfg = config(None, &["y"], AggregationType::None);
        let out = transform_data(&res, &cfg);
        assert_eq!(out.data[0].get("__index"), Some(&ChartValue::Number(0.0)));
        assert_eq!(out.data[1].get("__index"), Some(&ChartValue::Number(1.0)));
    }

    #[test]
    fn scatter_null_x_becomes_the_row_index() {
        let res = result(
            &["x", "y"],
            vec![
                vec![StatementCell::Null, num(1.0)],
                vec![text("b"), num(2.0)],
            ],
        );
        let mut cfg = config(Some("x"), &["y"], AggregationType::None);
        cfg.chart_type = ChartType::Scatter;
        let out = transform_data(&res, &cfg);
        assert_eq!(out.data[0].get("x"), Some(&ChartValue::Number(0.0)));
        assert_eq!(out.data[1].get("x"), Some(&ChartValue::Text("b".into())));

        // The same null x on a bar chart stringifies instead.
        let mut cfg = config(Some("x"), &["y"], AggregationType::None);
        cfg.chart_type = ChartType::Bar;
        let out = transform_data(&res, &cfg);
        assert_eq!(out.data[0].get("x"), Some(&ChartValue::Text(String::new())));
    }

    #[test]
    fn aggregation_groups_by_x_in_first_appearance_order() {
        let res = result(
            &["x", "y"],
            vec![
                vec![text("b"), num(1.0)],
                vec![text("a"), num(2.0)],
                vec![text("b"), num(3.0)],
            ],
        );
        let cfg = config(Some("x"), &["y"], AggregationType::Sum);
        let out = transform_data(&res, &cfg);
        assert_eq!(out.series_keys, vec!["y".to_owned()]);
        assert_eq!(
            out.data.iter().map(x_of).collect::<Vec<_>>(),
            vec!["b".to_owned(), "a".to_owned()],
            "insertion order, not sorted"
        );
        assert_eq!(out.data[0].get("y"), Some(&ChartValue::Number(4.0)));
        assert_eq!(out.data[1].get("y"), Some(&ChartValue::Number(2.0)));
    }

    #[test]
    fn null_x_groups_under_the_literal_category() {
        let res = result(
            &["x", "y"],
            vec![
                vec![StatementCell::Null, num(1.0)],
                vec![StatementCell::Null, num(2.0)],
            ],
        );
        let cfg = config(Some("x"), &["y"], AggregationType::Count);
        let out = transform_data(&res, &cfg);
        assert_eq!(x_of(&out.data[0]), "__null__");
        assert_eq!(out.data[0].get("y"), Some(&ChartValue::Number(2.0)));
    }

    #[test]
    fn aggregate_reducers_match_the_react_table() {
        // The owned cell list, adapted to the reducer's reference shape.
        let agg = |vals: Vec<StatementCell>, aggregation: AggregationType| {
            let refs: Vec<Option<&StatementCell>> = vals.iter().map(Some).collect();
            aggregate(&refs, aggregation)
        };
        let nums = vec![num(1.0), num(3.0), num(2.0)];
        assert_eq!(agg(nums.clone(), AggregationType::Sum), Some(6.0));
        assert_eq!(agg(nums.clone(), AggregationType::Avg), Some(2.0));
        assert_eq!(agg(nums.clone(), AggregationType::Min), Some(1.0));
        assert_eq!(agg(nums.clone(), AggregationType::Max), Some(3.0));
        assert_eq!(agg(nums.clone(), AggregationType::Count), Some(3.0));
        // count includes null and non-numeric members.
        let with_nulls = vec![num(1.0), StatementCell::Null, text("x")];
        assert_eq!(agg(with_nulls.clone(), AggregationType::Count), Some(3.0));
        assert_eq!(agg(with_nulls.clone(), AggregationType::Sum), Some(1.0));
        // distinct over raw values, not their numbers.
        let distinct = vec![text("a"), text("a"), text("b")];
        assert_eq!(agg(distinct.clone(), AggregationType::DistinctCount), Some(2.0));
        let distinct_nums = vec![num(1.0), num(1.0), num(2.0)];
        assert_eq!(agg(distinct_nums.clone(), AggregationType::DistinctCount), Some(2.0));
        // No numeric member: every numeric reducer returns null.
        let empty = vec![StatementCell::Null, text("x")];
        assert_eq!(agg(empty.clone(), AggregationType::Sum), None);
        assert_eq!(agg(empty.clone(), AggregationType::Min), None);
        assert_eq!(agg(nums.clone(), AggregationType::None), None);
    }

    #[test]
    fn group_by_pivots_one_series_per_group() {
        let res = result(
            &["x", "y", "g"],
            vec![
                vec![text("a"), num(1.0), text("g1")],
                vec![text("a"), num(2.0), text("g2")],
                vec![text("b"), num(3.0), text("g1")],
            ],
        );
        let mut cfg = config(Some("x"), &["y"], AggregationType::None);
        cfg.group_by = Some("g".to_owned());
        let out = transform_data(&res, &cfg);
        // One y axis → the series key is the bare group value.
        assert_eq!(out.series_keys, vec!["g1".to_owned(), "g2".to_owned()]);
        assert_eq!(out.data.len(), 2);
        assert_eq!(x_of(&out.data[0]), "a");
        assert_eq!(out.data[0].get("g1"), Some(&ChartValue::Number(1.0)));
        assert_eq!(out.data[0].get("g2"), Some(&ChartValue::Number(2.0)));
        assert_eq!(out.data[1].get("g1"), Some(&ChartValue::Number(3.0)));
        // React never assigns the absent group's key on the pivot row —
        // `undefined`, which reads as no entry here.
        assert_eq!(out.data[1].get("g2"), None);
    }

    #[test]
    fn two_y_axes_compose_the_series_key() {
        let res = result(
            &["x", "y1", "y2", "g"],
            vec![
                vec![text("a"), num(1.0), num(10.0), text("g1")],
                vec![text("a"), num(2.0), num(20.0), text("g2")],
            ],
        );
        let mut cfg = config(Some("x"), &["y1", "y2"], AggregationType::None);
        cfg.group_by = Some("g".to_owned());
        let out = transform_data(&res, &cfg);
        assert_eq!(
            out.series_keys,
            vec![
                "g1·y1".to_owned(),
                "g1·y2".to_owned(),
                "g2·y1".to_owned(),
                "g2·y2".to_owned()
            ]
        );
        assert_eq!(out.data[0].get("g1·y1"), Some(&ChartValue::Number(1.0)));
        assert_eq!(out.data[0].get("g2·y2"), Some(&ChartValue::Number(20.0)));
    }

    #[test]
    fn aggregated_group_by_pivots_composite_categories() {
        let res = result(
            &["x", "y", "g"],
            vec![
                vec![text("a"), num(1.0), text("g1")],
                vec![text("a"), num(2.0), text("g1")],
                vec![text("a"), num(3.0), text("g2")],
                vec![text("b"), num(4.0), text("g1")],
            ],
        );
        let mut cfg = config(Some("x"), &["y"], AggregationType::Sum);
        cfg.group_by = Some("g".to_owned());
        let out = transform_data(&res, &cfg);
        // The flat pass aggregates (a,g1)=3, (a,g2)=3, (b,g1)=4; the pivot
        // spreads them back over the x categories.
        assert_eq!(out.data.len(), 2);
        assert_eq!(out.data[0].get("g1"), Some(&ChartValue::Number(3.0)));
        assert_eq!(out.data[0].get("g2"), Some(&ChartValue::Number(3.0)));
        assert_eq!(out.data[1].get("g1"), Some(&ChartValue::Number(4.0)));
        // (b, g2) never aggregated: the pivot row keeps no key for it.
        assert_eq!(out.data[1].get("g2"), None);
    }

    #[test]
    fn sorts_match_the_react_comparators() {
        let res = result(
            &["x", "y"],
            vec![
                vec![text("c"), num(3.0)],
                vec![text("a"), num(1.0)],
                vec![text("b"), num(2.0)],
            ],
        );
        let mut cfg = config(Some("x"), &["y"], AggregationType::None);
        cfg.sort_by = ChartSort::XAsc;
        let out = transform_data(&res, &cfg);
        assert_eq!(
            out.data.iter().map(x_of).collect::<Vec<_>>(),
            vec!["a", "b", "c"]
        );
        cfg.sort_by = ChartSort::XDesc;
        let out = transform_data(&res, &cfg);
        assert_eq!(
            out.data.iter().map(x_of).collect::<Vec<_>>(),
            vec!["c", "b", "a"]
        );
        cfg.sort_by = ChartSort::YDesc;
        let out = transform_data(&res, &cfg);
        assert_eq!(
            out.data
                .iter()
                .map(|p| p.get("y").map(|v| value_display(Some(v))).unwrap_or_default())
                .collect::<Vec<_>>(),
            vec!["3", "2", "1"]
        );
        cfg.sort_by = ChartSort::YAsc;
        let out = transform_data(&res, &cfg);
        assert_eq!(
            out.data
                .iter()
                .map(|p| p.get("y").map(|v| value_display(Some(v))).unwrap_or_default())
                .collect::<Vec<_>>(),
            vec!["1", "2", "3"]
        );
    }

    #[test]
    fn numeric_strings_coerce_and_fail_like_js() {
        let res = result(
            &["x", "y"],
            vec![
                vec![text("a"), text("2")],
                vec![text("b"), text("abc")],
                vec![text("c"), StatementCell::Null],
                vec![text("d"), StatementCell::Bool(true)],
            ],
        );
        let cfg = config(Some("x"), &["y"], AggregationType::Sum);
        let out = transform_data(&res, &cfg);
        // Only "2" aggregates; the sum over one numeric member is 2.
        assert_eq!(out.data.len(), 4);
        assert_eq!(out.data[0].get("y"), Some(&ChartValue::Number(2.0)));
        assert_eq!(out.data[1].get("y"), Some(&ChartValue::Null));
    }
}
