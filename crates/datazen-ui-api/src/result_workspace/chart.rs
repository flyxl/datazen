use super::{
    ResultValue, ResultWorkspaceFallbackReason, ResultWorkspaceView, ResultWorkspaceViewResolution,
    StatementResultView,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InferredFieldType {
    Numeric,
    Boolean,
    Datetime,
    Categorical,
    Unknown,
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn is_numeric_like(value: &ResultValue) -> bool {
    match value {
        ResultValue::Number(_) => true,
        ResultValue::String(text) => {
            let trimmed = text.trim();
            !trimmed.is_empty() && trimmed.parse::<f64>().is_ok()
        }
        _ => false,
    }
}

fn is_date_like(text: &str) -> bool {
    let bytes = text.as_bytes();
    let iso_prefix = bytes.len() >= 10
        && bytes[0..4].iter().all(u8::is_ascii_digit)
        && bytes[4] == b'-'
        && bytes[5..7].iter().all(u8::is_ascii_digit)
        && bytes[7] == b'-'
        && bytes[8..10].iter().all(u8::is_ascii_digit);
    iso_prefix || (text.len() > 4 && (text.contains('-') || text.contains('/')))
}

pub fn infer_field_type(data_type: &str, samples: &[Option<ResultValue>]) -> InferredFieldType {
    let dt = data_type.to_lowercase();
    if dt.contains("bool") {
        return InferredFieldType::Boolean;
    }
    if contains_any(
        &dt,
        &[
            "int", "serial", "double", "numeric", "decimal", "real", "float", "money", "bigint",
            "smallint",
        ],
    ) {
        return InferredFieldType::Numeric;
    }
    if contains_any(&dt, &["timestamp", "date", "time"]) {
        return InferredFieldType::Datetime;
    }
    let non_null: Vec<&ResultValue> = samples.iter().filter_map(|value| value.as_ref()).collect();
    if non_null.is_empty() {
        return InferredFieldType::Unknown;
    }
    if non_null.iter().all(|value| value.is_number()) {
        return InferredFieldType::Numeric;
    }
    if non_null.iter().all(|value| value.is_boolean()) {
        return InferredFieldType::Boolean;
    }
    if non_null
        .iter()
        .all(|value| value.as_str().is_some_and(is_date_like))
    {
        return InferredFieldType::Datetime;
    }
    if non_null.iter().all(|value| is_numeric_like(value)) {
        return InferredFieldType::Numeric;
    }
    InferredFieldType::Categorical
}

pub fn is_chartable_result(result: &StatementResultView) -> bool {
    if result.rows.is_empty() || result.columns.is_empty() {
        return false;
    }
    let sample_size = result.rows.len().min(50);
    result.columns.iter().enumerate().any(|(index, column)| {
        let samples: Vec<Option<ResultValue>> = result.rows[..sample_size]
            .iter()
            .map(|row| row.get(index).cloned().flatten())
            .collect();
        infer_field_type(&column.data_type, &samples) == InferredFieldType::Numeric
    })
}

pub fn can_render_result_chart(
    result: Option<&StatementResultView>,
    _chart_config: Option<&ResultValue>,
) -> bool {
    result.is_some_and(is_chartable_result)
}

pub fn resolve_result_workspace_view(
    result: Option<&StatementResultView>,
    requested_view: ResultWorkspaceView,
    chart_config: Option<&ResultValue>,
) -> ResultWorkspaceViewResolution {
    let chart_available = can_render_result_chart(result, chart_config);
    if requested_view == ResultWorkspaceView::Table {
        return ResultWorkspaceViewResolution {
            view: ResultWorkspaceView::Table,
            chart_available,
            fallback_reason: None,
        };
    }
    let empty = match result {
        None => true,
        Some(result) => result.rows.is_empty() || result.columns.is_empty(),
    };
    if empty {
        return ResultWorkspaceViewResolution {
            view: ResultWorkspaceView::Table,
            chart_available,
            fallback_reason: Some(ResultWorkspaceFallbackReason::EmptyResult),
        };
    }
    if !chart_available {
        return ResultWorkspaceViewResolution {
            view: ResultWorkspaceView::Table,
            chart_available,
            fallback_reason: Some(ResultWorkspaceFallbackReason::NotChartable),
        };
    }
    ResultWorkspaceViewResolution {
        view: ResultWorkspaceView::Chart,
        chart_available,
        fallback_reason: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::result_workspace::ResultColumn;

    fn numeric_result() -> StatementResultView {
        StatementResultView {
            sql: "select n from t".to_owned(),
            columns: vec![ResultColumn {
                name: "n".to_owned(),
                data_type: "int".to_owned(),
                nullable: false,
            }],
            rows: vec![
                vec![Some(ResultValue::Number("1".to_owned()))],
                vec![Some(ResultValue::Number("2".to_owned()))],
            ],
            ..Default::default()
        }
    }

    #[test]
    fn declared_numeric_and_bool_types_win_over_samples() {
        assert_eq!(
            infer_field_type(
                "NUMERIC(10,2)",
                &[Some(ResultValue::String("x".to_owned()))]
            ),
            InferredFieldType::Numeric
        );
        assert_eq!(infer_field_type("boolean", &[]), InferredFieldType::Boolean);
        assert_eq!(
            infer_field_type("timestamp", &[]),
            InferredFieldType::Datetime
        );
        assert_eq!(infer_field_type("text", &[]), InferredFieldType::Unknown);
    }

    #[test]
    fn sample_values_classify_unknown_types() {
        assert_eq!(
            infer_field_type(
                "text",
                &[
                    Some(ResultValue::Number("1".to_owned())),
                    Some(ResultValue::Number("2".to_owned()))
                ]
            ),
            InferredFieldType::Numeric
        );
        assert_eq!(
            infer_field_type(
                "text",
                &[
                    Some(ResultValue::String("1.5".to_owned())),
                    Some(ResultValue::String("2".to_owned()))
                ]
            ),
            InferredFieldType::Numeric
        );
        assert_eq!(
            infer_field_type(
                "text",
                &[Some(ResultValue::String("2026-01-02".to_owned()))]
            ),
            InferredFieldType::Datetime
        );
        assert_eq!(
            infer_field_type(
                "text",
                &[
                    Some(ResultValue::String("a".to_owned())),
                    Some(ResultValue::String("b".to_owned()))
                ]
            ),
            InferredFieldType::Categorical
        );
    }

    #[test]
    fn chartability_needs_rows_and_a_numeric_field() {
        assert!(is_chartable_result(&numeric_result()));
        let mut no_rows = numeric_result();
        no_rows.rows.clear();
        assert!(!is_chartable_result(&no_rows));
        let mut text = numeric_result();
        text.columns[0].data_type = "text".to_owned();
        text.rows = vec![vec![Some(ResultValue::String("abc".to_owned()))]];
        assert!(!is_chartable_result(&text));
        assert!(!can_render_result_chart(None, None));
        assert!(can_render_result_chart(Some(&numeric_result()), None));
    }

    #[test]
    fn view_resolution_falls_back_without_mutating_the_request() {
        let chart = resolve_result_workspace_view(
            Some(&numeric_result()),
            ResultWorkspaceView::Chart,
            None,
        );
        assert_eq!(chart.view, ResultWorkspaceView::Chart);
        assert!(chart.chart_available);
        assert_eq!(chart.fallback_reason, None);

        let table = resolve_result_workspace_view(
            Some(&numeric_result()),
            ResultWorkspaceView::Table,
            None,
        );
        assert_eq!(table.view, ResultWorkspaceView::Table);
        assert!(table.chart_available);

        let empty = resolve_result_workspace_view(None, ResultWorkspaceView::Chart, None);
        assert_eq!(empty.view, ResultWorkspaceView::Table);
        assert_eq!(
            empty.fallback_reason,
            Some(ResultWorkspaceFallbackReason::EmptyResult)
        );

        let mut text = numeric_result();
        text.columns[0].data_type = "text".to_owned();
        text.rows = vec![vec![Some(ResultValue::String("abc".to_owned()))]];
        let not_chartable =
            resolve_result_workspace_view(Some(&text), ResultWorkspaceView::Chart, None);
        assert_eq!(
            not_chartable.fallback_reason,
            Some(ResultWorkspaceFallbackReason::NotChartable)
        );
    }
}
