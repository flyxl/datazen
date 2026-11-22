mod drop;
mod types;
mod workflows;

#[cfg(test)]
mod tests;

pub use drop::{create_query_drop_handler, detect_drop_context, DropEffect, DropOptions};
pub use types::{
    ChartConfigLike, DashboardWidgetRequest, DroppedTable, ExplainResultLike,
    RetryValidationInputLike, WorkflowEffect, WorkflowOptions, WorkflowState,
};
pub use workflows::{QueryPanelWorkflows, WorkflowHost};

pub const QUERY_DROP_HANDLER_SURFACE: [&str; 9] = [
    "detectDropContext",
    "createQueryDropHandler",
    "useQueryPanelWorkflows",
    "QueryPanelWorkflows",
    "QueryDropHandler",
    "DroppedTablePayload",
    "WorkflowHost",
    "WorkflowEffect",
    "QUERY_DROP_HANDLER_GAP",
];

pub const QUERY_DROP_HANDLER_GAP: [&str; 8] = [
    "React hooks/state and the SqlEditor MutableRefObject handles",
    "native drag/drop and clipboard DOM events",
    "fetchTableSchemaForSqlGeneration / generateTableSqlWithFallbacks / formatTableIdentifier are host seams",
    "runEditorExplain and the aiQueryActions builders are host seams",
    "dashboardCommands, crossWindowBus and openDashboardWindow are injected effects",
    "the 3 zustand store reads (panelStore / schemaStore / activeConnectionStore) are host seams",
    "T588 contracts are reused for diagnosis context and retry input",
    "confirm/showMessage dialogs are injected host calls",
];
