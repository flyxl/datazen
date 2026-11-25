//! React `components/query/*` ports, at the same path shape as the source tree.
//!
//! One module per React file; registration stays in `components/mod.rs`.

pub mod explain_plan_tree;
pub mod query_artifact_export;
pub mod query_context_selectors;
pub mod query_error_panel;
pub mod query_execution_status;

pub use query_context_selectors::{
    level_label_key, path_hierarchy_placeholder_segments, PathHierarchySegment,
    QueryContextSelectors, QueryContextSelectorsState,
};
pub use query_error_panel::{QueryErrorPanel, COPIED_FEEDBACK_MS};
