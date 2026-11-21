#[cfg(test)]
mod tests;

use crate::{widget_run_to_statement_result, ChartConfig, ChartDataPoint, StatementResult, WidgetRun, WidgetRunStatus};

#[derive(Clone, Debug, PartialEq)]
pub struct ChartTransformResult {
    pub data: Vec<ChartDataPoint>,
    pub series_keys: Vec<String>,
}

pub fn widget_run_to_chart_data<F>(
    run: &WidgetRun,
    chart_config: &ChartConfig,
    transform: &F,
) -> Option<ChartTransformResult>
where
    F: Fn(&StatementResult, &ChartConfig) -> Result<ChartTransformResult, String>,
{
    if run.status != WidgetRunStatus::Ok || run.row_count == 0 {
        return None;
    }
    let statement_result = widget_run_to_statement_result(run);
    transform(&statement_result, chart_config).ok()
}

pub fn has_renderable_chart(
    chart_data: Option<&ChartTransformResult>,
    chart_config: &ChartConfig,
) -> bool {
    chart_data.map_or(false, |data| !data.data.is_empty()) && !chart_config.y_axes.is_empty()
}
