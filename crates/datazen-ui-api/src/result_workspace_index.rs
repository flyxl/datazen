pub const RESULT_WORKSPACE_INDEX_SURFACE: [&str; 9] = [
    "ResultWorkspace",
    "ResultWorkspaceProps",
    "ResultTableView",
    "ResultTableViewProps",
    "canRenderResultChart",
    "resolveResultWorkspaceView",
    "ResultWorkspaceFallbackReason",
    "ResultWorkspaceView",
    "ResultWorkspaceViewResolution",
];

pub const RESULT_WORKSPACE_INDEX_GAP: [&str; 3] = [
    "React components ResultWorkspace/ResultTableView and their props stay view-side",
    "the barrel file itself adds no logic beyond re-export",
    "T600 already owns every non-view export this barrel forwards",
];

#[cfg(test)]
mod tests {
    use crate::{
        can_render_result_chart, is_mutation_execution, resolve_result_workspace_view,
        ResultWorkspaceFallbackReason, ResultWorkspaceView, ResultWorkspaceViewResolution,
        StatementResultView,
    };

    use super::*;

    #[test]
    fn the_barrel_forwards_every_non_view_export() {
        let _ = crate::can_render_result_chart;
        let _ = crate::resolve_result_workspace_view;
        let _ = crate::ResultWorkspaceView::Table;
        let _ = crate::ResultWorkspaceFallbackReason::EmptyResult;
        let _: Option<crate::ResultWorkspaceViewResolution> = None;
        assert_eq!(ResultWorkspaceView::Table.as_str(), "table");
        assert_eq!(ResultWorkspaceView::Chart.as_str(), "chart");
        assert_eq!(ResultWorkspaceFallbackReason::EmptyResult.as_str(), "empty-result");
        assert_eq!(ResultWorkspaceFallbackReason::NotChartable.as_str(), "not-chartable");
    }

    #[test]
    fn forwarded_helpers_keep_their_t600_semantics() {
        let empty = StatementResultView::default();
        assert!(!can_render_result_chart(Some(&empty), None));
        assert!(!is_mutation_execution(&empty), "an empty statement is not a confirmed mutation");
        let mutation = StatementResultView { sql: "DELETE FROM t".to_owned(), ..empty.clone() };
        assert!(is_mutation_execution(&mutation));
        let resolution = resolve_result_workspace_view(Some(&empty), ResultWorkspaceView::Chart, None);
        assert_eq!(resolution.view, ResultWorkspaceView::Table);
        assert!(!resolution.chart_available);
        assert_eq!(resolution.fallback_reason, Some(ResultWorkspaceFallbackReason::EmptyResult));
    }

    #[test]
    fn surface_and_gap_are_pinned() {
        assert_eq!(RESULT_WORKSPACE_INDEX_SURFACE.len(), 9);
        assert_eq!(RESULT_WORKSPACE_INDEX_SURFACE[4], "canRenderResultChart");
        assert_eq!(RESULT_WORKSPACE_INDEX_GAP.len(), 3);
        assert_eq!(crate::RESULT_WORKSPACE_INDEX_SURFACE.len(), 9);
        assert_eq!(crate::RESULT_WORKSPACE_INDEX_GAP.len(), 3);
        let _: ResultWorkspaceViewResolution = resolve_result_workspace_view(
            None,
            ResultWorkspaceView::Table,
            None,
        );
    }
}
