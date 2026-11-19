//! React `components/query/*` ports, at the same path shape as the source tree.
//!
//! One module per React file; registration stays in `components/mod.rs`.

pub(crate) mod explain_plan_tree;
pub(crate) mod query_artifact_export;
pub(crate) mod query_context_selectors;
pub(crate) mod query_error_panel;

pub(crate) use query_context_selectors::{
    level_label_key, path_hierarchy_placeholder_segments, PathHierarchySegment,
    QueryContextSelectors, QueryContextSelectorsState,
};
pub(crate) use query_error_panel::{QueryErrorPanel, COPIED_FEEDBACK_MS};
