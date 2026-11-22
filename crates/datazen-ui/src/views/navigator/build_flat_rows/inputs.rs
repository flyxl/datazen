//! Input snapshots for the flat-row builder — the fields `buildFlatRows.ts`
//! actually reads off its params, typed plainly. Removal stage: when the
//! formal store/type ports land (`stores/activeConnectionStore` T039,
//! `stores/schemaStore` T431, `types` in shard 2), the builder swaps these for
//! the real ones; the read set is frozen here so that swap is type-checked.
//!
//! The registry is caller-supplied, the same contract the connection form
//! chose: `DB_REGISTRY` is assembled from per-driver UI metadata at build
//! time in React, and no Rust-side registry exists yet (shard 2, T450).

use std::collections::{BTreeMap, HashMap, HashSet};

use serde_json::Value;

use super::deps::sql_namespace::SqlNamespace;

/// React `TableType` — the kinds a schema read can report.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum TableType {
    #[default]
    Table,
    SystemTable,
    View,
    MaterializedView,
}

impl TableType {
    /// The wire string the driver API uses.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Table => "table",
            Self::SystemTable => "systemTable",
            Self::View => "view",
            Self::MaterializedView => "materializedView",
        }
    }

    pub(crate) fn parse(raw: &str) -> Self {
        match raw {
            "systemTable" => Self::SystemTable,
            "view" => Self::View,
            "materializedView" => Self::MaterializedView,
            _ => Self::Table,
        }
    }
}

/// React `TableInfo` (the read set: name, schema, type).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TableInfo {
    pub name: String,
    pub schema: Option<String>,
    pub table_type: TableType,
}

/// React `DatabaseObject` (the read set: kind, schema, name).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DatabaseObject {
    pub kind: String,
    pub schema: Option<String>,
    pub name: String,
}

/// React `ConnectionConfig`, restricted to the builder's read set.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ConnectionSnapshot {
    pub id: String,
    pub name: String,
    pub database_type: String,
    pub host: Option<String>,
    pub database: Option<String>,
    pub group: Option<String>,
    pub pinned: Option<bool>,
    /// ISO timestamp string, compared as `Date.parse` millis.
    pub last_connected_at: Option<String>,
    /// Opaque per-driver options; the object filter is read from here
    /// defensively, exactly as React reads it.
    pub options: Option<Value>,
    /// `DB_REGISTRY[type].label`, injected by the caller so the locator can
    /// match on the display name without a registry port.
    pub driver_label: Option<String>,
}

impl ConnectionSnapshot {
    /// Test helper: the minimal connection the React tests construct.
    pub(crate) fn new(id: &str, name: &str, database_type: &str) -> Self {
        Self {
            id: id.to_owned(),
            name: name.to_owned(),
            database_type: database_type.to_owned(),
            ..Default::default()
        }
    }
}

/// React `ConnectionEntry` (the read set).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ActiveEntrySnapshot {
    /// Empty while connecting/errored.
    pub db_session_id: Option<String>,
    pub status: String,
}

/// React `ConnectionSchemaState` (the read set).
#[derive(Clone, Debug, Default)]
pub(crate) struct SchemaStateSnapshot {
    pub current_database: Option<String>,
    pub databases: Vec<String>,
    pub database_type: Option<String>,
    pub tables: Vec<TableInfo>,
    pub views: Vec<TableInfo>,
    pub schema_names: Vec<String>,
    pub namespace_tree: SqlNamespace,
    pub loaded_paths: HashSet<String>,
    pub path_items: BTreeMap<String, Vec<TableInfo>>,
    pub loading: bool,
    pub ensuring_count: usize,
    pub error: Option<String>,
    /// React `schemaEpoch` — bumped by admin ops; a change means the
    /// connection's schema surface genuinely changed (T580's cache-refresh
    /// fingerprint reads it).
    pub schema_epoch: u64,
}

/// React `ConnectionLocatorUsageState` — the runtime caches the locator
/// searches besides the connection's own fields.
#[derive(Clone, Debug, Default)]
pub(crate) struct UsageStateSnapshot {
    pub active_connections: HashMap<String, ActiveEntrySnapshot>,
    pub schemas: HashMap<String, SchemaStateSnapshot>,
    pub db_tables_map: BTreeMap<String, Vec<TableInfo>>,
    pub db_objects_map: BTreeMap<String, Vec<DatabaseObject>>,
}

/// React `DatabaseTypeMeta`, restricted to the fields the tree shape reads.
/// The registry lookup is caller-supplied (see [`BuildFlatRowsParams`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct DriverMeta {
    /// `label`, used by the locator as a `database-type` candidate.
    pub label: Option<String>,
    pub is_key_value: bool,
    pub has_multi_database: bool,
    /// `'name' | 'path' | 'index' | 'domain'`.
    pub database_field_type: String,
    /// `'default-sql' | 'postgresql' | 'path-hierarchy'`.
    pub namespace_ensure: Option<String>,
    /// `'standard' | 'multiDatabase' | 'custom'`.
    pub schema_tree_mode: Option<String>,
    pub default_schema: Option<String>,
    pub db_counts_command: Option<String>,
    /// Object kinds in declaration order; empty means tables/views only.
    pub supported_object_kinds: Vec<String>,
}

/// One grouped section of connections, as the host hands them over
/// (React's `grouped: { group, connections }[]` — already built, pinned and
/// recent sections included by the caller).
#[derive(Clone, Debug, Default)]
pub(crate) struct GroupedSection {
    pub group: String,
    pub connections: Vec<ConnectionSnapshot>,
}
