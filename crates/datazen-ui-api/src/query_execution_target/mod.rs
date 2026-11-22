mod pane_height;
mod resolve;

pub use pane_height::{
    estimate_query_result_pane_height, query_result_pane_sizing, QueryResultPaneHeightInput,
    ResultPaneSizing, RESULT_PANE_MIN_HEIGHT,
};
pub use resolve::{
    offset_to_line, resolve_execution_target, statement_target_info, ExecutionTargetInput,
    ResolveExecutionTargetResult, StatementRange, StatementTargetInfo,
};

use crate::ResultWorkspaceView;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SqlExecutionStrategy {
    CurrentStatement,
    EntireScript,
    LargestStatement,
    Ask,
}

impl SqlExecutionStrategy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CurrentStatement => "current_statement",
            Self::EntireScript => "entire_script",
            Self::LargestStatement => "largest_statement",
            Self::Ask => "ask",
        }
    }
}

pub const SELECTION_STRATEGY: &str = "selection";

pub const QUERY_EXECUTION_TARGET_SURFACE: [&str; 8] = [
    "estimateQueryResultPaneHeight",
    "queryResultPaneSizing",
    "resolveExecutionTarget",
    "offsetToLine",
    "toStatementTargetInfo",
    "StatementTargetInfo",
    "ResolveExecutionTargetResult",
    "QUERY_EXECUTION_TARGET_GAP",
];

pub const QUERY_EXECUTION_TARGET_GAP: [&str; 6] = [
    "React CSSProperties and the host element that consumes the sizing",
    "buildStatementRanges / getValidStatementRanges / findStatementAtCursor from extension-points",
    "the editor's cursor offset and selection state",
    "the SqlExecutionStrategy union owned by the aggregate types",
    "the multi-statement tab strip and result surface DOM heights",
    "the caller that applies the pane height and the 50% CSS cap",
];

pub fn strategy_as_str(strategy: SqlExecutionStrategy) -> &'static str {
    strategy.as_str()
}

pub fn effective_strategy(is_selection: bool, strategy: SqlExecutionStrategy) -> &'static str {
    if is_selection {
        SELECTION_STRATEGY
    } else {
        strategy.as_str()
    }
}

pub fn view_is_chart(view: ResultWorkspaceView) -> bool {
    view == ResultWorkspaceView::Chart
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strategy_literals_and_surfaces_are_pinned() {
        assert_eq!(
            SqlExecutionStrategy::CurrentStatement.as_str(),
            "current_statement"
        );
        assert_eq!(SqlExecutionStrategy::EntireScript.as_str(), "entire_script");
        assert_eq!(
            SqlExecutionStrategy::LargestStatement.as_str(),
            "largest_statement"
        );
        assert_eq!(SqlExecutionStrategy::Ask.as_str(), "ask");
        assert_eq!(
            effective_strategy(true, SqlExecutionStrategy::Ask),
            "selection"
        );
        assert_eq!(effective_strategy(false, SqlExecutionStrategy::Ask), "ask");
        assert!(view_is_chart(ResultWorkspaceView::Chart));
        assert_eq!(QUERY_EXECUTION_TARGET_SURFACE.len(), 8);
        assert_eq!(QUERY_EXECUTION_TARGET_GAP.len(), 6);
        assert_eq!(crate::QUERY_EXECUTION_TARGET_GAP.len(), 6);
    }
}
