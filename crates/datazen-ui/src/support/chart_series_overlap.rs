//! T151: series overlap dashes — port of
//! `packages/frontend/src/lib/chart/seriesOverlap.ts` (88 lines).
//!
//! A pure function for the chart view: when two series of a multi-series
//! chart run so close that their lines would be indistinguishable (the max
//! absolute difference across all points under 5% of the overall Y range),
//! the overlapping ones get stroke dash patterns so they stay readable;
//! everything else stays solid.
//!
//! ## Deliberate deviations from the React source
//!
//! * The data point is the inference slice's [`ChartValue`] bag keyed by
//!   series name, and `Number(pt[key])` becomes [`js_number`]: JS's `Number`
//!   coercion for the value subset a query result carries (null → 0, booleans
//!   → 0/1, numeric strings → the number, blanks → 0; anything else is NaN
//!   and skipped, exactly React's `Number.isFinite` filter).
//! * The dash pattern strings are the SVG `stroke-dasharray` forms React
//!   hands to recharts, verbatim (`STROKE_DASH_PATTERNS` in
//!   `lib/chart/colors.ts`); the caller maps them to its renderer's dash
//!   shape. `None` is the pattern's own first entry — the solid line.
//! * The map answers every series key (React initializes every key to
//!   `undefined`), so a missing key is a lookup miss, not a "solid" default
//!   that could mask a wiring gap.

use std::collections::HashMap;

use crate::support::chart_field_inference::ChartValue;

/// React's `STROKE_DASH_PATTERNS` (`lib/chart/colors.ts`), in order.
pub(crate) const STROKE_DASH_PATTERNS: [Option<&str>; 6] = [
    None,            // solid
    Some("8 4"),     // dashed
    Some("2 4"),     // dotted
    Some("8 4 2 4"), // dash-dot
    Some("12 4"),    // long dash
    Some("4 4"),     // short dash
];

/// React's `OVERLAP_RATIO`: two series overlap when their max absolute
/// difference across all points is under 5% of the overall Y range — the
/// lines would be nearly indistinguishable.
const OVERLAP_RATIO: f64 = 0.05;

/// One chart data point: series key → cell value (React's `ChartDataPoint`,
/// a `[key: string]: string | number | null` bag).
pub(crate) type ChartSeriesPoint = HashMap<String, ChartValue>;

/// JS's `Number(value)` for the value subset a chart point carries:
/// `None` is the NaN outcome (React's non-finite filter skips it).
pub(crate) fn js_number(value: &ChartValue) -> Option<f64> {
    match value {
        ChartValue::Null => Some(0.0),
        ChartValue::Bool(true) => Some(1.0),
        ChartValue::Bool(false) => Some(0.0),
        ChartValue::Number(v) if v.is_finite() => Some(*v),
        ChartValue::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                Some(0.0) // JS: Number('') === 0
            } else {
                trimmed.parse::<f64>().ok()
            }
        }
        // JS: Number([]) === 0, Number([7]) === 7 — the array/object arms
        // never reach a chart point on this wire; treat them as NaN rather
        // than guess at the JS coercion.
        _ => None,
    }
}

/// `buildOverlapDashMap(data, yAxes)`: every series key → its dash pattern,
/// `None` for solid. Only series that overlap too closely with another get
/// a pattern; when every value is identical (zero range) all series dash so
/// they stay distinguishable.
pub(crate) fn build_overlap_dash_map(
    data: &[ChartSeriesPoint],
    y_axes: &[String],
) -> HashMap<String, Option<&'static str>> {
    let mut result: HashMap<String, Option<&'static str>> =
        y_axes.iter().map(|k| (k.clone(), None)).collect();

    if y_axes.len() < 2 || data.is_empty() {
        return result;
    }

    let mut global_min = f64::INFINITY;
    let mut global_max = f64::NEG_INFINITY;
    for pt in data {
        for key in y_axes {
            if let Some(v) = pt.get(key).and_then(js_number) {
                global_min = global_min.min(v);
                global_max = global_max.max(v);
            }
        }
    }
    let range = global_max - global_min;
    if range == 0.0 {
        // All values identical — dash every series in order.
        let mut dash_idx = 0;
        for key in y_axes {
            result.insert(
                key.clone(),
                STROKE_DASH_PATTERNS[dash_idx % STROKE_DASH_PATTERNS.len()],
            );
            dash_idx += 1;
        }
        return result;
    }

    let threshold = range * OVERLAP_RATIO;
    let mut overlapping: Vec<&String> = Vec::new();

    for (i, key_a) in y_axes.iter().enumerate() {
        for key_b in y_axes.iter().skip(i + 1) {
            if are_series_too_close(data, key_a, key_b, threshold) {
                if !overlapping.contains(&key_a) {
                    overlapping.push(key_a);
                }
                if !overlapping.contains(&key_b) {
                    overlapping.push(key_b);
                }
            }
        }
    }

    if overlapping.is_empty() {
        return result;
    }

    // Assign dash patterns only to the overlapping series (the first
    // overlapping one stays solid — React's dashIdx starts at the solid
    // pattern).
    let mut dash_idx = 0;
    for key in y_axes {
        if overlapping.contains(&key) {
            result.insert(
                key.clone(),
                STROKE_DASH_PATTERNS[dash_idx % STROKE_DASH_PATTERNS.len()],
            );
            dash_idx += 1;
        }
    }
    result
}

/// React's `areSeriesTooClose`: the max absolute difference across all
/// points stays under the threshold — one bigger gap and the series are
/// readable apart.
fn are_series_too_close(
    data: &[ChartSeriesPoint],
    key_a: &str,
    key_b: &str,
    threshold: f64,
) -> bool {
    let mut max_diff = 0.0f64;
    for pt in data {
        let (Some(a), Some(b)) = (
            pt.get(key_a).and_then(js_number),
            pt.get(key_b).and_then(js_number),
        ) else {
            continue; // React's Number.isFinite skip
        };
        let diff = (a - b).abs();
        if diff > threshold {
            return false;
        }
        max_diff = max_diff.max(diff);
    }
    let _ = max_diff;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(entries: &[(&str, ChartValue)]) -> ChartSeriesPoint {
        entries
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect()
    }

    fn num(v: f64) -> ChartValue {
        ChartValue::Number(v)
    }

    #[test]
    fn fewer_than_two_series_or_no_data_answer_all_solid() {
        let data = vec![point(&[("a", num(1.0))])];
        let map = build_overlap_dash_map(&data, &["a".to_owned()]);
        assert_eq!(map.get("a"), Some(&None));

        // No data at all, three keys: still all solid, and every key is
        // present (the lookup contract).
        let map = build_overlap_dash_map(&[], &["a".to_owned(), "b".to_owned(), "c".to_owned()]);
        assert_eq!(map.len(), 3);
        assert!(map.values().all(|v| v.is_none()));
    }

    #[test]
    fn identical_series_dash_in_order() {
        // Zero range: every series dashes, first stays solid.
        let data = vec![point(&[("a", num(5.0)), ("b", num(5.0)), ("c", num(5.0))])];
        let map = build_overlap_dash_map(&data, &["a".to_owned(), "b".to_owned(), "c".to_owned()]);
        assert_eq!(map.get("a"), Some(&None));
        assert_eq!(map.get("b"), Some(&Some("8 4")));
        assert_eq!(map.get("c"), Some(&Some("2 4")));
    }

    #[test]
    fn widely_apart_series_stay_solid() {
        let data = vec![
            point(&[("a", num(0.0)), ("b", num(50.0))]),
            point(&[("a", num(1.0)), ("b", num(49.0))]),
        ];
        let map = build_overlap_dash_map(&data, &["a".to_owned(), "b".to_owned()]);
        assert_eq!(map.get("a"), Some(&None));
        assert_eq!(map.get("b"), Some(&None));
    }

    #[test]
    fn close_series_dash_while_a_third_stays_solid() {
        // Range 0..100, threshold 5: a/b differ by 1 (overlap), c rides at 90.
        let data = vec![
            point(&[("a", num(0.0)), ("b", num(1.0)), ("c", num(90.0))]),
            point(&[("a", num(2.0)), ("b", num(3.0)), ("c", num(91.0))]),
        ];
        let map = build_overlap_dash_map(
            &data,
            &["a".to_owned(), "b".to_owned(), "c".to_owned()],
        );
        assert_eq!(map.get("a"), Some(&None), "the first overlapping series stays solid");
        assert_eq!(map.get("b"), Some(&Some("8 4")));
        assert_eq!(map.get("c"), Some(&None), "the far series stays solid");
    }

    #[test]
    fn one_gap_over_the_threshold_breaks_the_overlap() {
        // Diff 1 except one point at diff 40 (range 100, threshold 5).
        let data = vec![
            point(&[("a", num(0.0)), ("b", num(1.0))]),
            point(&[("a", num(0.0)), ("b", num(41.0))]),
            point(&[("a", num(0.0)), ("b", num(1.0))]),
        ];
        let map = build_overlap_dash_map(&data, &["a".to_owned(), "b".to_owned()]);
        assert_eq!(map.get("a"), Some(&None));
        assert_eq!(map.get("b"), Some(&None));
    }

    #[test]
    fn js_number_mirrors_the_js_coercion_for_the_point_values() {
        assert_eq!(js_number(&ChartValue::Null), Some(0.0));
        assert_eq!(js_number(&ChartValue::Bool(true)), Some(1.0));
        assert_eq!(js_number(&ChartValue::Bool(false)), Some(0.0));
        assert_eq!(js_number(&ChartValue::Number(2.5)), Some(2.5));
        assert_eq!(js_number(&ChartValue::String("42".into())), Some(42.0));
        assert_eq!(js_number(&ChartValue::String(" 7 ".into())), Some(7.0));
        assert_eq!(js_number(&ChartValue::String("".into())), Some(0.0));
        assert_eq!(js_number(&ChartValue::String("abc".into())), None);
        assert_eq!(js_number(&ChartValue::Object), None);
    }

    #[test]
    fn non_finite_cells_are_skipped_not_treated_as_zero_holes() {
        // "abc" is NaN for both series: the scan sees only the numeric pair.
        let data = vec![
            point(&[("a", num(0.0)), ("b", num(1.0)), ("c", ChartValue::String("abc".into()))]),
            point(&[("a", ChartValue::String("x".into())), ("b", num(3.0)), ("c", num(50.0))]),
        ];
        // a/b: one numeric pair, diff 2 over a range from 0..50 (c pulls the
        // max): threshold 2.5 — 2 stays under, so they overlap.
        let map = build_overlap_dash_map(
            &data,
            &["a".to_owned(), "b".to_owned(), "c".to_owned()],
        );
        assert_eq!(map.get("b"), Some(&Some("8 4")));
    }
}
