//! The flat row the builder emits — React `navigator/types.ts`'s
//! `UnifiedRow`, all thirteen variants with the fields the builder fills.
//! Removal stage: T572 ports `types.ts` formally; the variants and fields
//! move there, and [`to_row_vm`] becomes the bridge to the T470 row VM.

use super::inputs::{DatabaseObject, TableInfo, TableType};

/// A row's depth pair: where it is painted, and where it *announces* —
/// a search removes exactly one announced level at and below the connection
/// children, and the two numbers deliberately disagree there.
pub(crate) type DepthPair = (usize, usize);

/// React `UnifiedRow`. Every variant carries its depth pair first, matching
/// the `...levels(depth)` spread at each emit site.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum NavigatorRow {
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
        match_payload: Option<super::deps::locator::ConnectionMatch>,
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
    pub(crate) fn depth(&self) -> usize {
        self.depth_pair().0
    }

    pub(crate) fn level_depth(&self) -> usize {
        self.depth_pair().1
    }

    pub(crate) fn depth_pair(&self) -> DepthPair {
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

    /// The row's type tag, for assertions and host dispatch.
    pub(crate) fn kind(&self) -> &'static str {
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
pub(crate) fn row_key(row: &NavigatorRow) -> String {
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
