//! React `windows/connection/navigator/types.ts` — the formal port.
//!
//! The row union is the navigator's spine: `buildFlatRows` emits it (T570, in
//! [`super::build_flat_rows`]), `treeRowAria` announces it (T572, in
//! [`super::tree_row_aria`]) and the tree row paints it (T470's
//! `navigator_tree_row`, through its row VM). It started life inside the
//! builder as `row_model.rs`; T572 is the task the shard assigned to
//! `types.ts`, so the union lives here now and the builder re-exports it.
//!
//! The host contracts at the bottom (`ConnectionNavigatorTreeHandle` /
//! `ConnectionNavigatorTreeProps`) are the ports the future navigator tree
//! host will satisfy — callbacks, not IPC, matching the codebase's host-port
//! convention. Removal stage: the shared tree kit task owns
//! `TREE_TOP_LEVEL`/`ariaLevelOf` formally; until then they stay in
//! `tree_row_aria.rs`.

use std::rc::Rc;

use super::build_flat_rows::inputs::{DatabaseObject, TableInfo, TableType};

/// React `NAVIGATOR_ROW_HEIGHT` — every row of the virtualized tree is this
/// many px tall (the virtualizer's fixed step).
pub const NAVIGATOR_ROW_HEIGHT: f32 = 28.0;

/// A row's depth pair: where it is painted, and where it *announces* —
/// a search removes exactly one announced level at and below the connection
/// children, and the two numbers deliberately disagree there.
pub type DepthPair = (usize, usize);

/// React `UnifiedRow`. Every variant carries its depth pair first, matching
/// the `...levels(depth)` spread at each emit site.
#[derive(Clone, Debug, PartialEq)]
pub enum NavigatorRow {
    /// A pinned/recent shortcut section header.
    Section {
        depth_pair: DepthPair,
        section: &'static str,
        display_name: String,
        count: usize,
        expanded: bool,
    },
    /// A named group header.
    Group {
        depth_pair: DepthPair,
        group_name: String,
        display_name: String,
        count: usize,
        expanded: bool,
    },
    Connection {
        depth_pair: DepthPair,
        id: String,
        name: String,
        engine: String,
        section_group: String,
        is_selected: bool,
        /// The status string the host tracks (`idle`/`connecting`/`connected`/`error`).
        status: String,
        expanded: bool,
        /// The locator's best match, when a search produced one.
        match_payload: Option<super::build_flat_rows::deps::locator::ConnectionMatch>,
    },
    Db {
        depth_pair: DepthPair,
        connection_id: String,
        db_session_id: String,
        db_name: String,
        expanded: bool,
        loading: bool,
        is_open: bool,
    },
    Schema {
        depth_pair: DepthPair,
        connection_id: String,
        db_name: String,
        schema_name: String,
        expanded: bool,
    },
    Category {
        depth_pair: DepthPair,
        key: String,
        db_name: String,
        cat_id: &'static str,
        cat_label_key: &'static str,
        count: usize,
        expanded: bool,
    },
    Table {
        depth_pair: DepthPair,
        item: TableInfo,
        cat_id: &'static str,
        is_selected: bool,
        connection_id: String,
        db_session_id: String,
        db_name: String,
    },
    Object {
        depth_pair: DepthPair,
        obj: DatabaseObject,
        cat_id: &'static str,
        /// Owner tuple — without it two connections' identically named
        /// objects collide (see [`row_key`]).
        connection_id: String,
        db_name: String,
        schema_name: Option<String>,
    },
    KvDb {
        depth_pair: DepthPair,
        connection_id: String,
        db_session_id: String,
        db_name: String,
        is_selected: bool,
        db_counts_command: Option<String>,
    },
    /// A spinner standing in for the row named by `owner_key`
    /// (`conn:` / `session:` / `db:` / `ns:` prefixed).
    DbLoading {
        depth_pair: DepthPair,
        owner_key: String,
    },
    /// A failed load, shown instead of a silent empty list.
    DbError {
        depth_pair: DepthPair,
        owner_key: String,
        message: String,
    },
    NamespaceNode {
        depth_pair: DepthPair,
        name: String,
        expanded: bool,
        is_leaf: bool,
        /// Leaf rows only; branches carry `None`.
        leaf_kind: Option<TableType>,
        segments: Vec<String>,
        key: String,
        connection_id: String,
        db_session_id: String,
    },
    EmptyGroup {
        depth_pair: DepthPair,
        group_name: String,
    },
    NoConnections {
        depth_pair: DepthPair,
    },
}

impl NavigatorRow {
    pub fn depth(&self) -> usize {
        self.depth_pair().0
    }

    pub fn level_depth(&self) -> usize {
        self.depth_pair().1
    }

    pub fn depth_pair(&self) -> DepthPair {
        match self {
            Self::Section { depth_pair, .. }
            | Self::Group { depth_pair, .. }
            | Self::Connection { depth_pair, .. }
            | Self::Db { depth_pair, .. }
            | Self::Schema { depth_pair, .. }
            | Self::Category { depth_pair, .. }
            | Self::Table { depth_pair, .. }
            | Self::Object { depth_pair, .. }
            | Self::KvDb { depth_pair, .. }
            | Self::DbLoading { depth_pair, .. }
            | Self::DbError { depth_pair, .. }
            | Self::NamespaceNode { depth_pair, .. }
            | Self::EmptyGroup { depth_pair, .. }
            | Self::NoConnections { depth_pair } => *depth_pair,
        }
    }

    /// Test/VM helper for React's `{ ...row, levelDepth }` spread: clone
    /// semantics live with the type, so the equivalence suite can spell the
    /// builder's search rule without a second hand-rolled union.
    pub fn set_level_depth(&mut self, level_depth: usize) {
        let (depth, _) = self.depth_pair();
        let pair = (depth, level_depth);
        match self {
            Self::Section { depth_pair, .. }
            | Self::Group { depth_pair, .. }
            | Self::Connection { depth_pair, .. }
            | Self::Db { depth_pair, .. }
            | Self::Schema { depth_pair, .. }
            | Self::Category { depth_pair, .. }
            | Self::Table { depth_pair, .. }
            | Self::Object { depth_pair, .. }
            | Self::KvDb { depth_pair, .. }
            | Self::DbLoading { depth_pair, .. }
            | Self::DbError { depth_pair, .. }
            | Self::NamespaceNode { depth_pair, .. }
            | Self::EmptyGroup { depth_pair, .. }
            | Self::NoConnections { depth_pair } => *depth_pair = pair,
        }
    }

    /// The row's type tag, for assertions and host dispatch.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Section { .. } => "section",
            Self::Group { .. } => "group",
            Self::Connection { .. } => "connection",
            Self::Db { .. } => "db",
            Self::Schema { .. } => "schema",
            Self::Category { .. } => "category",
            Self::Table { .. } => "table",
            Self::Object { .. } => "object",
            Self::KvDb { .. } => "kv-db",
            Self::DbLoading { .. } => "db-loading",
            Self::DbError { .. } => "db-error",
            Self::NamespaceNode { .. } => "namespace-node",
            Self::EmptyGroup { .. } => "empty-group",
            Self::NoConnections { .. } => "no-connections",
        }
    }
}

/// React `getUnifiedRowKey` — the stable identity of a row, built only from
/// fields that say *which object the row is*, so it survives re-sorts,
/// re-filters and window changes. No index, on purpose (see the React doc
/// comment for the two bugs an index key caused).
pub fn row_key(row: &NavigatorRow) -> String {
    match row {
        NavigatorRow::Section { section, .. } => format!("sec:{section}"),
        NavigatorRow::Group { group_name, .. } => format!("grp:{group_name}"),
        NavigatorRow::Connection {
            id, section_group, ..
        } => {
            format!("conn:{section_group}:{id}")
        }
        NavigatorRow::Db {
            connection_id,
            db_name,
            ..
        } => {
            format!("db:{connection_id}:{db_name}")
        }
        NavigatorRow::Schema {
            connection_id,
            db_name,
            schema_name,
            ..
        } => {
            format!("schema:{connection_id}:{db_name}:{schema_name}")
        }
        NavigatorRow::Category { key, .. } => format!("cat:{key}"),
        NavigatorRow::Table {
            connection_id,
            db_name,
            item,
            ..
        } => {
            format!(
                "tbl:{connection_id}:{db_name}:{}:{}",
                item.schema.as_deref().unwrap_or(""),
                item.name
            )
        }
        NavigatorRow::Object {
            connection_id,
            db_name,
            schema_name,
            cat_id,
            obj,
            ..
        } => {
            // React: `databaseObjectIdentityKey(obj)` — kind + schema + name.
            format!(
                "obj:{connection_id}:{db_name}:{}:{cat_id}:{}:{}:{}",
                schema_name.as_deref().unwrap_or(""),
                obj.kind,
                obj.schema.as_deref().unwrap_or(""),
                obj.name
            )
        }
        NavigatorRow::KvDb {
            connection_id,
            db_name,
            ..
        } => {
            format!("kv:{connection_id}:{db_name}")
        }
        NavigatorRow::DbLoading { owner_key, .. } => format!("loading:{owner_key}"),
        NavigatorRow::DbError { owner_key, .. } => format!("error:{owner_key}"),
        NavigatorRow::NamespaceNode { key, .. } => format!("ns:{key}"),
        NavigatorRow::EmptyGroup { group_name, .. } => format!("empty:{group_name}"),
        NavigatorRow::NoConnections { .. } => "no-connections".to_owned(),
    }
}

// ---------------------------------------------------------------------------
// Host contracts (React `ConnectionNavigatorTreeHandle` / `...TreeProps`)
// ---------------------------------------------------------------------------

/// React `ConnectionNavigatorTreeHandle` — the imperative facet the host
/// exposes (its ref): refresh one connection, or all of them.
#[allow(dead_code)] // host port; consumer: the navigator tree host (not wired).
pub struct NavigatorTreeHandlePort<S> {
    pub refresh_all_connections: Option<Rc<dyn Fn(&mut S) + 'static>>,
    pub refresh_connection: Option<Rc<dyn Fn(&mut S, String) + 'static>>,
}

impl<S> Default for NavigatorTreeHandlePort<S> {
    fn default() -> Self {
        Self {
            refresh_all_connections: None,
            refresh_connection: None,
        }
    }
}

/// React `ConnectionNavigatorTreeProps['onShowMessage']` payload kind.
#[allow(dead_code)] // host port payload; consumer: the navigator tree host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageKind {
    Error,
    Success,
}

/// React `ConnectionNavigatorTreeProps['onNodeContextMenu']` payload — what
/// the tree knows about the node a secondary press landed on.
#[allow(dead_code)] // host port payload; consumer: the navigator tree host.
#[derive(Clone, Debug, PartialEq)]
pub struct NodeContextMenuPayload {
    pub kind: String,
    pub name: String,
    pub x: f32,
    pub y: f32,
    pub schema: Option<String>,
}

/// React `Pick<TableContextInput, 'database' | 'schema'>` — the context half
/// of `viewActions.newQuery`, all a new editor tab needs to bind itself.
#[allow(dead_code)] // host port payload; consumer: the navigator tree host.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QueryOpenDatabaseContext {
    pub database: Option<String>,
    pub schema: Option<String>,
}

/// React `ConnectionNavigatorTreeProps['viewActions']` — the window-level
/// actions the navigator's context menus can request. Every field optional,
/// exactly as the React interface is: a menu item is offered only when its
/// action is wired.
#[allow(dead_code)] // host port; consumer: the navigator tree host (not wired).
pub struct NavigatorViewActionsPort<S> {
    pub new_query:
        Option<Rc<dyn Fn(&mut S, Option<String>, Option<QueryOpenDatabaseContext>, Option<String>) -> bool + 'static>>,
    pub open_sql_file: Option<Rc<dyn Fn(&mut S) + 'static>>,
    pub create_table: Option<Rc<dyn Fn(&mut S) + 'static>>,
    pub open_create_database: Option<Rc<dyn Fn(&mut S) + 'static>>,
    pub open_create_schema: Option<Rc<dyn Fn(&mut S) + 'static>>,
    pub open_create_user: Option<Rc<dyn Fn(&mut S) + 'static>>,
    pub open_er_diagram:
        Option<Rc<dyn Fn(&mut S, Option<String>, Option<String>) + 'static>>,
    pub open_table_structure: Option<Rc<dyn Fn(&mut S, String) + 'static>>,
    pub refresh: Option<Rc<dyn Fn(&mut S) + 'static>>,
    pub open_query_history: Option<Rc<dyn Fn(&mut S) + 'static>>,
}

impl<S> Default for NavigatorViewActionsPort<S> {
    fn default() -> Self {
        Self {
            new_query: None,
            open_sql_file: None,
            create_table: None,
            open_create_database: None,
            open_create_schema: None,
            open_create_user: None,
            open_er_diagram: None,
            open_table_structure: None,
            refresh: None,
            open_query_history: None,
        }
    }
}

/// React `ConnectionNavigatorTreeProps` — everything the navigator tree needs
/// from its host, as callbacks instead of props. The tree host (a later
/// task) fills this in; the menus (T578) and the db state (T580) read the
/// pieces they own through it.
#[allow(dead_code)] // host port; consumer: the navigator tree host (not wired).
pub struct NavigatorTreePropsPort<S> {
    pub on_select_connection: Rc<dyn Fn(&mut S, String) + 'static>,
    pub on_select_table: Rc<dyn Fn(&mut S, String, Option<String>, String) + 'static>,
    pub on_select_kv_db: Option<Rc<dyn Fn(&mut S, String, String) + 'static>>,
    pub active_connection_id: Option<String>,
    pub on_new_connection: Rc<dyn Fn(&mut S, Option<String>) + 'static>,
    pub on_refresh: Option<Rc<dyn Fn(&mut S) + 'static>>,
    pub on_edit_connection: Rc<dyn Fn(&mut S, String) + 'static>,
    pub on_delete_connection: Rc<dyn Fn(&mut S, String) + 'static>,
    pub on_disconnect: Rc<dyn Fn(&mut S, String) + 'static>,
    pub on_export_connections: Option<Rc<dyn Fn(&mut S) + 'static>>,
    pub on_import_connections: Option<Rc<dyn Fn(&mut S) + 'static>>,
    pub on_collapse_sidebar: Option<Rc<dyn Fn(&mut S) + 'static>>,
    pub on_show_message: Option<Rc<dyn Fn(&mut S, String, MessageKind) + 'static>>,
    pub on_node_context_menu: Option<Rc<dyn Fn(&mut S, NodeContextMenuPayload) + 'static>>,
    pub view_actions: NavigatorViewActionsPort<S>,
}
