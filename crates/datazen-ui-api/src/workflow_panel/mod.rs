mod host;
mod machine;
mod rules;
mod types;

#[cfg(test)]
mod tests;

pub use host::WorkflowPanelHost;
pub use machine::WorkflowPanel;
pub use rules::{
    connection_options, error_text, feedback_tone, is_connection_variable, is_loading,
    variable_defaults, variable_input,
};
pub use types::{
    ConfirmKind, ConnectionSummary, ExecuteParams, FeedbackTone, PanelTab, VariableInput,
    WorkflowPanelEffect, WorkflowPanelLabels, WorkflowPanelOptions, WorkflowPanelState,
    WorkflowSelectOption, WorkflowStoreView,
};

pub const WORKFLOW_PANEL_SURFACE: [&str; 9] = [
    "WorkflowPanel",
    "WorkflowPanelHost",
    "variableDefaults",
    "feedbackTone",
    "variableInput",
    "ConnectionSummary",
    "ExecuteParams",
    "PanelTab",
    "WORKFLOW_PANEL_GAP",
];

pub const WORKFLOW_PANEL_GAP: [&str; 8] = [
    "React useEffect/useState/useCallback and the AiStore selector hooks",
    "aiCommands workflowGetDir/Get/Delete/Save/Reload/HistoryList/HistoryGet/HistoryClear",
    "connectionCommands.getConnections",
    "useConfirmDialog at the delete and clear-history gates",
    "openDocsWindow('workflows')",
    "WorkflowForm, workflowDraftConvert, WorkflowExecutionResultPanel and WorkflowHistoryTab children",
    "setTimeout(2000) feedback clear",
    "onMouseDown preventDefault and stopPropagation on the row edit/delete buttons",
];
