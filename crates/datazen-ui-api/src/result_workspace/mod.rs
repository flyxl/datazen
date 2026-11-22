mod chart;
mod mutation;

pub use chart::{
    can_render_result_chart, infer_field_type, is_chartable_result, resolve_result_workspace_view,
    InferredFieldType,
};
pub use mutation::{
    get_top_level_sql_verb, is_mutation_execution, strip_leading_sql_comments, MUTATION_VERBS,
    QUERY_VERBS,
};

#[derive(Clone, Debug, PartialEq)]
pub enum ResultValue {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Opaque(String),
}

impl ResultValue {
    pub fn is_number(&self) -> bool {
        matches!(self, Self::Number(_))
    }

    pub fn is_boolean(&self) -> bool {
        matches!(self, Self::Bool(_))
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(text) => Some(text),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResultColumn {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct StatementResultView {
    pub sql: String,
    pub columns: Vec<ResultColumn>,
    pub rows: Vec<Vec<Option<ResultValue>>>,
    pub rows_affected: Option<u64>,
    pub execution_time_ms: u64,
    pub truncated: Option<bool>,
    pub pinned: Option<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResultWorkspaceView {
    Table,
    Chart,
}

impl ResultWorkspaceView {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Table => "table",
            Self::Chart => "chart",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResultWorkspaceFallbackReason {
    EmptyResult,
    NotChartable,
}

impl ResultWorkspaceFallbackReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::EmptyResult => "empty-result",
            Self::NotChartable => "not-chartable",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResultWorkspaceViewResolution {
    pub view: ResultWorkspaceView,
    pub chart_available: bool,
    pub fallback_reason: Option<ResultWorkspaceFallbackReason>,
}

pub const RESULT_WORKSPACE_SURFACE: [&str; 10] = [
    "isMutationExecution",
    "getTopLevelSqlVerb",
    "stripLeadingSqlComments",
    "canRenderResultChart",
    "resolveResultWorkspaceView",
    "isChartableResult",
    "inferFieldType",
    "StatementResultView",
    "ResultWorkspaceViewResolution",
    "RESULT_WORKSPACE_GAP",
];

pub const RESULT_WORKSPACE_GAP: [&str; 5] = [
    "the React StatementResult / ColumnInfo aggregate types",
    "ChartView initial-config derivation from a persisted ChartConfig",
    "React hooks that call onViewChange after resolving a fallback",
    "host IPC that produces the statement results",
    "DOM/chart rendering of the resolved view",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_literals_and_surfaces_are_pinned() {
        assert_eq!(ResultWorkspaceView::Table.as_str(), "table");
        assert_eq!(ResultWorkspaceView::Chart.as_str(), "chart");
        assert_eq!(
            ResultWorkspaceFallbackReason::EmptyResult.as_str(),
            "empty-result"
        );
        assert_eq!(
            ResultWorkspaceFallbackReason::NotChartable.as_str(),
            "not-chartable"
        );
        assert_eq!(RESULT_WORKSPACE_SURFACE.len(), 10);
        assert_eq!(RESULT_WORKSPACE_GAP.len(), 5);
        assert_eq!(crate::RESULT_WORKSPACE_GAP.len(), 5);
    }

    #[test]
    fn empty_result_defaults_are_pinned() {
        let result = StatementResultView::default();
        assert!(result.sql.is_empty());
        assert!(result.columns.is_empty());
        assert!(result.rows.is_empty());
    }
}
