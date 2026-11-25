//! `lib/chart/postQueryView.ts` (T117): decide table vs chart after a
//! successful query, honoring `Settings.autoChartOnQuery`.
//!
//! ## Input boundary (cross-segment)
//!
//! The setting read: React pulls it from the settings store
//! (`useSettingsStore.getState().settings.autoChartOnQuery === true`).
//! The store is T456 (this segment, not ported yet), so the read arrives
//! as a parameter — `None` is the unset setting, `Some(false)` the
//! explicit opt-out, and only `Some(true)` charts. The wiring re-homes
//! with the store port.
//!
//! The consumer is the query execution flow (`stores/queryExecActions.ts`
//! calls `resolvePostQueryViewMode(exec.results[0])`) — its port wires
//! this in.
//!
//! `StatementResult` arrives as the inference slice [`ChartResult`]
//! (`columns` + `rows`, the only fields the decision reads — the shape
//! T078's `isChartableResult` already takes).

use crate::support::chart_field_inference::{is_chartable_result, ChartResult};

/// The view the results pane opens in (React's `'table' | 'chart'`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PostQueryViewMode {
    Table,
    Chart,
}

/// `resolvePostQueryViewMode`: chart only when the setting opts in, a
/// result exists, and it is chartable; table in every other case.
pub fn resolve_post_query_view_mode(
    result: Option<&ChartResult>,
    auto_chart_on_query: Option<bool>,
) -> PostQueryViewMode {
    if auto_chart_on_query == Some(true) && result.is_some_and(is_chartable_result) {
        return PostQueryViewMode::Chart;
    }
    PostQueryViewMode::Table
}
