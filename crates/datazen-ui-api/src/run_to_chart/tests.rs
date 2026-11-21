use super::*;
use crate::ChartDataPointValue;
use serde_json::json;
use std::cell::RefCell;
use std::collections::BTreeMap;

fn run(status: &str, row_count: u32) -> WidgetRun {
    match serde_json::from_value(json!({
        "id": "r1",
        "dashboardId": "d1",
        "widgetId": "w1",
        "workflowId": "wf1",
        "startedAt": "2026-01-01T00:00:00Z",
        "finishedAt": "2026-01-01T00:00:01Z",
        "status": status,
        "rowCount": row_count,
        "columns": ["id"],
        "rows": [[1]]
    })) {
        Ok(value) => value,
        Err(error) => panic!("fixture must decode: {error}"),
    }
}

fn point() -> ChartDataPoint {
    let mut map = BTreeMap::new();
    map.insert("id".to_owned(), ChartDataPointValue::Number(1.0));
    ChartDataPoint(map)
}

fn transformed() -> ChartTransformResult {
    ChartTransformResult {
        data: vec![point()],
        series_keys: vec!["id".to_owned()],
    }
}

#[test]
fn a_successful_run_with_rows_is_transformed() {
    let seen = RefCell::new(Vec::new());
    let transform = |result: &StatementResult, config: &ChartConfig| -> Result<ChartTransformResult, String> {
        seen.borrow_mut().push((result.sql.clone(), result.rows.len(), config.y_axes.len()));
        Ok(transformed())
    };
    let result = widget_run_to_chart_data(&run("ok", 1), &ChartConfig::default(), &transform);
    assert_eq!(result, Some(transformed()));
    assert_eq!(seen.into_inner(), [("".to_owned(), 1, 0)]);
}

#[test]
fn a_non_ok_or_empty_run_skips_the_transform() {
    let calls = RefCell::new(0);
    let transform = |_result: &StatementResult, _config: &ChartConfig| -> Result<ChartTransformResult, String> {
        *calls.borrow_mut() += 1;
        Ok(transformed())
    };
    assert_eq!(
        widget_run_to_chart_data(&run("error", 3), &ChartConfig::default(), &transform),
        None
    );
    assert_eq!(
        widget_run_to_chart_data(&run("timeout", 3), &ChartConfig::default(), &transform),
        None
    );
    assert_eq!(
        widget_run_to_chart_data(&run("ok", 0), &ChartConfig::default(), &transform),
        None
    );
    assert_eq!(*calls.borrow(), 0);
}

#[test]
fn a_throwing_transform_becomes_null() {
    let transform = |_result: &StatementResult, _config: &ChartConfig| -> Result<ChartTransformResult, String> {
        Err("transform failed".to_owned())
    };
    assert_eq!(
        widget_run_to_chart_data(&run("ok", 1), &ChartConfig::default(), &transform),
        None
    );
}

#[test]
fn renderability_needs_points_and_y_axes() {
    let mut config = ChartConfig::default();
    assert!(!has_renderable_chart(None, &config));
    assert!(!has_renderable_chart(Some(&transformed()), &config));
    config.y_axes = vec!["id".to_owned()];
    assert!(has_renderable_chart(Some(&transformed()), &config));
    let empty = ChartTransformResult {
        data: Vec::new(),
        series_keys: vec!["id".to_owned()],
    };
    assert!(!has_renderable_chart(Some(&empty), &config));
    assert!(!has_renderable_chart(None, &config));
}

#[test]
fn crate_root_reexports_the_chart_routing_surface() {
    fn resolves<T: 'static + ?Sized>() {}
    resolves::<crate::ChartTransformResult>();
    let transform = |_result: &StatementResult, _config: &ChartConfig| Ok(transformed());
    assert_eq!(
        crate::widget_run_to_chart_data(&run("ok", 1), &ChartConfig::default(), &transform),
        Some(transformed())
    );
    assert!(!crate::has_renderable_chart(None, &ChartConfig::default()));
}
