//! T151: chart type recommendation — port of
//! `packages/frontend/src/lib/chart/recommend.ts` (73 lines).
//!
//! A pure function: from inferred fields and a row count to the chart the
//! results pane suggests (type, axes, grouping, aggregation, confidence and
//! the i18n key of the human reason). The rules mirror React's cascade in
//! order: a time series first, then a distribution / comparison bar chart,
//! then a correlation scatter, then a bare trend area; nothing matches when
//! no rule fires.
//!
//! ## Input boundary (cross-segment)
//!
//! React's `ChartField.distinctCount` is optional (`?? rowCount` / `?? 0`
//! fallbacks). The inference port ([`ChartField`]) always fills
//! `distinct_count`, so the `??` arms are no-ops here — the field the
//! host hands over already carries the number. The consumer is the query
//! results pane (`stores/queryExecActions`), which wires this with its own
//! port.

use crate::components::chart::chart_canvas::{AggregationType, ChartType};
use crate::support::chart_field_inference::{ChartField, InferredFieldType};

/// React's `ChartRecommendation`.
#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)] // consumed by the query-results pane wiring
pub(crate) struct ChartRecommendation {
    pub chart_type: ChartType,
    pub xAxis: Option<String>,
    pub yAxes: Vec<String>,
    pub group_by: Option<String>,
    pub aggregation: AggregationType,
    /// React's confidence (0.5–0.9 in the cascade).
    pub confidence: f64,
    /// The i18n key of the reason line (`chart.recommend.*`).
    pub reason: &'static str,
}

/// `recommendChart(fields, rowCount)`: the first rule whose shape matches
/// wins, exactly in React's order.
pub(crate) fn recommend_chart(fields: &[ChartField], row_count: usize) -> Option<ChartRecommendation> {
    let numerics: Vec<&ChartField> = fields
        .iter()
        .filter(|f| f.inferred_type == InferredFieldType::Numeric)
        .collect();
    let datetimes: Vec<&ChartField> = fields
        .iter()
        .filter(|f| f.inferred_type == InferredFieldType::Datetime)
        .collect();
    let categoricals: Vec<&ChartField> = fields
        .iter()
        .filter(|f| f.inferred_type == InferredFieldType::Categorical)
        .collect();

    // A datetime axis plus at least one numeric: a time series.
    if !datetimes.is_empty() && !numerics.is_empty() {
        return Some(ChartRecommendation {
            chart_type: ChartType::Line,
            xAxis: Some(datetimes[0].name.clone()),
            yAxes: numerics.iter().take(3).map(|f| f.name.clone()).collect(),
            group_by: None,
            aggregation: AggregationType::None,
            confidence: 0.9,
            reason: "chart.recommend.timeSeries",
        });
    }

    // A categorical axis plus at least one numeric: distribution or
    // comparison.
    if !categoricals.is_empty() && !numerics.is_empty() {
        let cat = categoricals[0];
        // React's `(cat.distinctCount ?? rowCount) / rowCount`; the port's
        // inference always fills the count.
        let distinct_ratio = cat.distinct_count as f64 / row_count as f64;

        // Few buckets over few rows: a pie answers the distribution.
        if cat.distinct_count <= 8 && row_count <= 20 {
            return Some(ChartRecommendation {
                chart_type: ChartType::Pie,
                xAxis: Some(cat.name.clone()),
                yAxes: vec![numerics[0].name.clone()],
                group_by: None,
                aggregation: AggregationType::None,
                confidence: 0.8,
                reason: "chart.recommend.distribution",
            });
        }

        // Otherwise a bar comparison, grouped by the second categorical when
        // there is one, aggregated when the axis is dense.
        return Some(ChartRecommendation {
            chart_type: ChartType::Bar,
            xAxis: Some(cat.name.clone()),
            yAxes: numerics.iter().take(3).map(|f| f.name.clone()).collect(),
            group_by: categoricals.get(1).map(|f| f.name.clone()),
            aggregation: if distinct_ratio < 0.5 {
                AggregationType::Sum
            } else {
                AggregationType::None
            },
            confidence: 0.85,
            reason: "chart.recommend.comparison",
        });
    }

    // Two numerics and nothing else: a correlation scatter.
    if numerics.len() >= 2 && categoricals.is_empty() && datetimes.is_empty() {
        return Some(ChartRecommendation {
            chart_type: ChartType::Scatter,
            xAxis: Some(numerics[0].name.clone()),
            yAxes: vec![numerics[1].name.clone()],
            group_by: None,
            aggregation: AggregationType::None,
            confidence: 0.7,
            reason: "chart.recommend.correlation",
        });
    }

    // One numeric over enough rows: a bare trend area.
    if numerics.len() == 1 && row_count > 5 {
        return Some(ChartRecommendation {
            chart_type: ChartType::Area,
            xAxis: None,
            yAxes: vec![numerics[0].name.clone()],
            group_by: None,
            aggregation: AggregationType::None,
            confidence: 0.5,
            reason: "chart.recommend.trend",
        });
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::chart_field_inference::ChartColumn;

    fn field(name: &str, kind: InferredFieldType, distinct: usize) -> ChartField {
        ChartField {
            name: name.to_owned(),
            data_type: "text".to_owned(),
            inferred_type: kind,
            distinct_count: distinct,
            sample_values: Vec::new(),
        }
    }

    fn numeric_column() -> ChartColumn {
        ChartColumn {
            name: "amount".to_owned(),
            data_type: "float".to_owned(),
        }
    }

    #[test]
    fn a_datetime_axis_with_a_numeric_answers_a_line_time_series() {
        let fields = vec![
            field("day", InferredFieldType::Datetime, 30),
            field("amount", InferredFieldType::Numeric, 25),
        ];
        let rec = recommend_chart(&fields, 30).unwrap_or_else(|| panic!("a rule fired"));
        assert_eq!(rec.chart_type, ChartType::Line);
        assert_eq!(rec.xAxis.as_deref(), Some("day"));
        assert_eq!(rec.yAxes, ["amount"]);
        assert_eq!(rec.group_by, None);
        assert_eq!(rec.aggregation, AggregationType::None);
        assert_eq!(rec.confidence, 0.9);
        assert_eq!(rec.reason, "chart.recommend.timeSeries");
        // The datetime rule outranks everything: a categorical sits unused.
        let mixed = vec![
            field("day", InferredFieldType::Datetime, 30),
            field("amount", InferredFieldType::Numeric, 25),
            field("kind", InferredFieldType::Categorical, 3),
        ];
        assert_eq!(recommend_chart(&mixed, 30).unwrap().chart_type, ChartType::Line);
    }

    #[test]
    fn the_time_series_takes_at_most_three_numerics() {
        let fields = vec![
            field("day", InferredFieldType::Datetime, 30),
            field("a", InferredFieldType::Numeric, 25),
            field("b", InferredFieldType::Numeric, 25),
            field("c", InferredFieldType::Numeric, 25),
            field("d", InferredFieldType::Numeric, 25),
        ];
        let rec = recommend_chart(&fields, 30).unwrap();
        assert_eq!(rec.yAxes, ["a", "b", "c"]);
    }

    #[test]
    fn a_small_categorical_grid_answers_a_pie() {
        let fields = vec![
            field("kind", InferredFieldType::Categorical, 3),
            field("amount", InferredFieldType::Numeric, 25),
        ];
        let rec = recommend_chart(&fields, 10).unwrap();
        assert_eq!(rec.chart_type, ChartType::Pie);
        assert_eq!(rec.xAxis.as_deref(), Some("kind"));
        assert_eq!(rec.yAxes, ["amount"]);
        assert_eq!(rec.confidence, 0.8);
        assert_eq!(rec.reason, "chart.recommend.distribution");
    }

    #[test]
    fn a_dense_categorical_answers_a_summed_grouped_bar() {
        // 40 distinct values over 200 rows: ratio 0.2 < 0.5, so aggregated.
        let fields = vec![
            field("region", InferredFieldType::Categorical, 40),
            field("sector", InferredFieldType::Categorical, 5),
            field("amount", InferredFieldType::Numeric, 200),
        ];
        let rec = recommend_chart(&fields, 200).unwrap();
        assert_eq!(rec.chart_type, ChartType::Bar);
        assert_eq!(rec.group_by.as_deref(), Some("sector"));
        assert_eq!(rec.aggregation, AggregationType::Sum);
        assert_eq!(rec.confidence, 0.85);
        assert_eq!(rec.reason, "chart.recommend.comparison");

        // A sparse axis (ratio >= 0.5) keeps the raw values.
        let fields = vec![
            field("region", InferredFieldType::Categorical, 10),
            field("amount", InferredFieldType::Numeric, 20),
        ];
        let rec = recommend_chart(&fields, 20).unwrap();
        assert_eq!(rec.aggregation, AggregationType::None);
        assert_eq!(rec.group_by, None);
    }

    #[test]
    fn two_lonely_numerics_answer_a_scatter() {
        let fields = vec![
            field("x", InferredFieldType::Numeric, 25),
            field("y", InferredFieldType::Numeric, 25),
        ];
        let rec = recommend_chart(&fields, 30).unwrap();
        assert_eq!(rec.chart_type, ChartType::Scatter);
        assert_eq!(rec.xAxis.as_deref(), Some("x"));
        assert_eq!(rec.yAxes, ["y"]);
        assert_eq!(rec.confidence, 0.7);
        assert_eq!(rec.reason, "chart.recommend.correlation");
    }

    #[test]
    fn one_numeric_over_five_rows_answers_a_trend_area() {
        let fields = vec![field("amount", InferredFieldType::Numeric, 25)];
        let rec = recommend_chart(&fields, 6).unwrap();
        assert_eq!(rec.chart_type, ChartType::Area);
        assert_eq!(rec.xAxis, None);
        assert_eq!(rec.yAxes, ["amount"]);
        assert_eq!(rec.confidence, 0.5);
        assert_eq!(rec.reason, "chart.recommend.trend");
        // Five rows or fewer: not even the trend rule fires.
        assert_eq!(recommend_chart(&fields, 5), None);
    }

    #[test]
    fn nothing_matches_when_no_rule_fires() {
        // Only categoricals, no numeric.
        let fields = vec![field("kind", InferredFieldType::Categorical, 5)];
        assert_eq!(recommend_chart(&fields, 100), None);
        // Nothing at all.
        assert_eq!(recommend_chart(&[], 0), None);
    }

    #[test]
    fn the_inference_slice_types_compose_with_the_recommendation() {
        // The port's own inference output feeds straight in: a float column
        // infers numeric, and the recommendation reads it.
        let columns = vec![numeric_column()];
        let inferred = crate::support::chart_field_inference::infer_all_fields(
            &crate::support::chart_field_inference::ChartResult {
                columns,
                rows: Vec::new(),
            },
        );
        // No rows: nothing to sample, so the recommendation has nothing to
        // say — the boundary holds.
        assert_eq!(recommend_chart(&inferred, 0), None);
    }
}
