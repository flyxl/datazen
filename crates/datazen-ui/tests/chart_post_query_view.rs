//! T117: `resolvePostQueryViewMode` (lib/chart/postQueryView.ts) — the
//! table-vs-chart decision after a successful query, mirroring the React
//! test vectors (enabled+chartable → chart; disabled/unset/no result/not
//! chartable → table).

use datazen_ui::support::chart_field_inference::{ChartColumn, ChartResult, ChartValue};
use datazen_ui::support::chart_post_query_view::{
    resolve_post_query_view_mode, PostQueryViewMode,
};

/// The React test fixture: a day text column and an int4 count column
/// with two rows — chartable.
fn chartable() -> ChartResult {
    ChartResult {
        columns: vec![
            ChartColumn {
                name: "day".to_owned(),
                data_type: "text".to_owned(),
            },
            ChartColumn {
                name: "count".to_owned(),
                data_type: "int4".to_owned(),
            },
        ],
        rows: vec![
            vec![ChartValue::String("Mon".to_owned()), ChartValue::Number(1.0)],
            vec![ChartValue::String("Tue".to_owned()), ChartValue::Number(2.0)],
        ],
    }
}

#[test]
fn chart_when_enabled_and_chartable() {
    assert_eq!(
        resolve_post_query_view_mode(Some(&chartable()), Some(true)),
        PostQueryViewMode::Chart
    );
}

#[test]
fn table_when_auto_chart_disabled() {
    assert_eq!(
        resolve_post_query_view_mode(Some(&chartable()), Some(false)),
        PostQueryViewMode::Table
    );
}

#[test]
fn table_when_the_setting_is_unset() {
    // React's `=== true`: an unset setting is not true.
    assert_eq!(
        resolve_post_query_view_mode(Some(&chartable()), None),
        PostQueryViewMode::Table
    );
}

#[test]
fn table_when_there_is_no_result() {
    assert_eq!(
        resolve_post_query_view_mode(None, Some(true)),
        PostQueryViewMode::Table
    );
}

#[test]
fn table_when_the_result_is_not_chartable() {
    // No rows: nothing to chart.
    let empty = ChartResult {
        columns: chartable().columns,
        rows: Vec::new(),
    };
    assert_eq!(
        resolve_post_query_view_mode(Some(&empty), Some(true)),
        PostQueryViewMode::Table
    );

    // No numeric field: a chart has nothing to plot.
    let all_text = ChartResult {
        columns: vec![ChartColumn {
            name: "day".to_owned(),
            data_type: "text".to_owned(),
        }],
        rows: vec![vec![ChartValue::String("Mon".to_owned())]],
    };
    assert_eq!(
        resolve_post_query_view_mode(Some(&all_text), Some(true)),
        PostQueryViewMode::Table
    );
}
