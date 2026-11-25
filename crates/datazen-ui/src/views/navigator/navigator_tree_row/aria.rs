//! Pure ARIA/geometry rules for the navigator tree row, ported from React's
//! `treeRowAria.ts` and the `navigatorTreeRowAriaEquivalence` spec.
//!
//! Indent and announced level are deliberately different numbers under a
//! search: a global object search removes the group header, so rows at or below
//! a connection announce one rung higher while painting at the same indent.

/// React `TREE_TOP_LEVEL` — `aria-level` is 1-based while depth is 0-based.
pub const TREE_TOP_LEVEL: usize = 1;
/// React `INDENT_STEP_REM` (0.75rem = 12pt) and the base indent.
pub const INDENT_BASE_PT: f32 = 0.0;
pub const INDENT_STEP_PT: f32 = 12.0;
/// React `CONNECTION_CHILD_DEPTH` — rows at/under this depth lose one level in a search.
pub const CONNECTION_CHILD_DEPTH: usize = 2;

pub const PINNED_GROUP_KEY: &str = "__pinned__";
pub const RECENT_GROUP_KEY: &str = "__recent__";

/// Connection states the row renders (React `ConnectionEntry['status']`).
#[allow(dead_code)] // view VM; consumer: the navigator host (not wired).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RowConnectionStatus {
    #[default]
    Idle,
    Connecting,
    Connected,
    Error,
}

/// The kinds of context menu a row can request.
#[allow(dead_code)] // host port; consumer: the navigator host (not wired).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContextMenuTarget {
    Group(String),
    Connection(String),
    Database { db_name: String, connection_id: String },
    Schema { schema_name: String, db_name: String, connection_id: String },
    Table { name: String, schema: Option<String> },
    Category { key: String, cat_id: String },
    Object(String),
}

/// The 13 React `UnifiedRow` variants. Depth/level live in [`RowVm`].
#[allow(dead_code)] // view VM; consumer: the navigator host (not wired).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RowKind {
    Section { section: String, display_name: String, count: usize, expanded: bool },
    Group { group_name: String, display_name: String, count: usize, expanded: bool, drop_target: bool },
    Connection {
        id: String,
        name: String,
        engine: String,
        section_group: Option<String>,
        is_selected: bool,
        status: RowConnectionStatus,
        expanded: bool,
        match_reason: Option<String>,
        match_context: Option<String>,
        drop_before: bool,
        drop_after: bool,
    },
    Db { connection_id: String, db_session_id: String, db_name: String, expanded: bool, loading: bool, is_open: bool },
    Schema { connection_id: String, db_name: String, schema_name: String, expanded: bool },
    Category { key: String, cat_id: String, label_key: String, expanded: bool },
    Table { name: String, schema: Option<String>, cat_id: String, is_selected: bool, connection_id: String, db_session_id: String, db_name: String },
    Object { name: String, schema: Option<String>, cat_id: String },
    KvDb { connection_id: String, db_session_id: String, db_name: String, is_selected: bool, count: Option<usize> },
    DbLoading { owner_key: String },
    DbError { message: String },
    NamespaceNode { name: String, expanded: bool, is_leaf: bool, key: String, connection_id: String, db_session_id: String },
    EmptyGroup { group_name: Option<String>, drop_target: bool },
    NoConnections,
}

#[allow(dead_code)] // view VM; consumer: the navigator host (not wired).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowVm {
    pub kind: RowKind,
    pub depth: usize,
    pub level_depth: usize,
}

/// React `ariaLevelOf`.
#[allow(dead_code)] // pure rule; pinned by the row tests.
pub fn aria_level_of(row: &RowVm) -> usize {
    row.level_depth + TREE_TOP_LEVEL
}

/// React `indentOf` — painting indent, deliberately independent of `aria_level`.
#[allow(dead_code)] // pure rule; pinned by the row tests.
pub fn indent_of(row: &RowVm) -> f32 {
    INDENT_BASE_PT + row.depth as f32 * INDENT_STEP_PT
}

/// React `navigatorRowAria` returns `null` for the two decoration rows.
#[allow(dead_code)] // pure rule; pinned by the row tests.
pub fn is_decoration(row: &RowVm) -> bool {
    matches!(row.kind, RowKind::EmptyGroup { .. } | RowKind::NoConnections)
}

/// React `isNavigatorBranch`: connection is always a branch (its databases
/// exist even when idle); a namespace node is a branch only while it has children.
#[allow(dead_code)] // pure rule; pinned by the row tests.
pub fn is_navigator_branch(row: &RowVm) -> bool {
    match &row.kind {
        RowKind::EmptyGroup { .. } | RowKind::NoConnections => false,
        RowKind::Table { .. }
        | RowKind::Object { .. }
        | RowKind::KvDb { .. }
        | RowKind::DbLoading { .. }
        | RowKind::DbError { .. } => false,
        RowKind::NamespaceNode { is_leaf, .. } => !is_leaf,
        _ => true,
    }
}

/// React `connectionExpandedState`: a connected/connecting connection announces
/// its disclosure state; an idle or failed one announces nothing at all.
#[allow(dead_code)] // pure rule; pinned by the row tests.
pub fn connection_expanded_state(row: &RowVm) -> Option<bool> {
    match &row.kind {
        RowKind::Connection { status, expanded, .. }
            if *status == RowConnectionStatus::Connected
                || *status == RowConnectionStatus::Connecting =>
        {
            Some(*expanded)
        }
        _ => None,
    }
}

/// The `aria-expanded` value this row should announce. `None` means the
/// attribute is absent (leaves and idle connections), never `Some(false)`.
#[allow(dead_code)] // pure rule; pinned by the row tests.
pub fn aria_expanded(row: &RowVm) -> Option<bool> {
    if is_decoration(row) || !is_navigator_branch(row) {
        return None;
    }
    match &row.kind {
        RowKind::Connection { .. } => connection_expanded_state(row),
        RowKind::Section { expanded, .. }
        | RowKind::Group { expanded, .. }
        | RowKind::Db { expanded, .. }
        | RowKind::Schema { expanded, .. }
        | RowKind::Category { expanded, .. }
        | RowKind::NamespaceNode { expanded, .. } => Some(*expanded),
        _ => None,
    }
}

/// The group a section row toggles (reserved pinned/recent keys).
#[allow(dead_code)] // pure rule; pinned by the row tests.
pub fn section_group_key(section: &str) -> &'static str {
    if section == "pinned" {
        PINNED_GROUP_KEY
    } else {
        RECENT_GROUP_KEY
    }
}

/// Apply the search's one-level shift: rows at or below the connection lose
/// one rung while their paint depth stays put. Decorations are unaffected.
#[allow(dead_code)] // pure rule; pinned by the row tests.
pub fn searched(row: &RowVm) -> RowVm {
    if is_decoration(row) {
        return row.clone();
    }
    let mut next = row.clone();
    if row.depth >= CONNECTION_CHILD_DEPTH {
        next.level_depth = row.level_depth.saturating_sub(1);
    }
    next
}
