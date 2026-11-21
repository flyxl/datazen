use std::collections::{BTreeMap, BTreeSet};

use crate::{RelationColumns, SqlNamespace};

pub const SCHEMA_STORE_GAP: [&str; 7] = [
    "databaseCommands.getDatabases / listCatalog / listTables IPC",
    "SchemaRequestTracker generation tokens",
    "schemaColumnBatcher.ensure asynchronous read",
    "ensureNamespacePathImpl asynchronous tree walking",
    "metadataScopeKey async barrier",
    "bindSchemaStore / registerMetadataIdentityBinder seams",
    "t() error translations",
];

pub const SCHEMA_STORE_SURFACE: [&str; 8] = [
    "loadForConnection",
    "setLoadedTables",
    "setCurrentDatabase",
    "bindMetadataIdentity",
    "ensureColumns",
    "mergeNamespace",
    "schemaOfRelation",
    "useConnectionSchemaField",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NamespaceMergeKind {
    Tables,
    Branch,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SchemaTableInfo {
    pub name: String,
    pub schema: Option<String>,
    pub table_type: String,
    pub row_count: Option<i64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SchemaSessionState {
    pub connection_id: String,
    pub metadata_revision: u64,
    pub metadata_scope_key: Option<String>,
    pub current_database: Option<String>,
    pub current_schema: Option<String>,
    pub databases: Vec<String>,
    pub database_type: Option<String>,
    pub is_multi_database: bool,
    pub tables: Vec<SchemaTableInfo>,
    pub table_catalogs: BTreeMap<String, Vec<SchemaTableInfo>>,
    pub views: Vec<SchemaTableInfo>,
    pub schema_names: Vec<String>,
    pub relation_columns: BTreeMap<String, RelationColumns>,
    pub namespace_tree: SqlNamespace,
    pub loaded_paths: BTreeSet<String>,
    pub path_items: BTreeMap<String, Vec<SchemaTableInfo>>,
    pub path_aliases: BTreeMap<String, String>,
    pub namespace_owned_by_plugin: bool,
    pub schema_epoch: u64,
    pub expanded: BTreeSet<String>,
    pub selected_id: Option<String>,
    pub loading: bool,
    pub ensuring_count: u64,
    pub error: Option<String>,
    pub column_inflight: BTreeSet<String>,
}

#[derive(Clone, Debug, Default)]
pub struct SchemaPatch {
    pub connection_id: Option<String>,
    pub metadata_revision: Option<u64>,
    pub metadata_scope_key: Option<Option<String>>,
    pub current_database: Option<Option<String>>,
    pub current_schema: Option<Option<String>>,
    pub databases: Option<Vec<String>>,
    pub database_type: Option<Option<String>>,
    pub is_multi_database: Option<bool>,
    pub tables: Option<Vec<SchemaTableInfo>>,
    pub table_catalogs: Option<BTreeMap<String, Vec<SchemaTableInfo>>>,
    pub views: Option<Vec<SchemaTableInfo>>,
    pub schema_names: Option<Vec<String>>,
    pub relation_columns: Option<BTreeMap<String, RelationColumns>>,
    pub namespace_tree: Option<SqlNamespace>,
    pub loaded_paths: Option<BTreeSet<String>>,
    pub path_items: Option<BTreeMap<String, Vec<SchemaTableInfo>>>,
    pub path_aliases: Option<BTreeMap<String, String>>,
    pub namespace_owned_by_plugin: Option<bool>,
    pub schema_epoch: Option<u64>,
    pub expanded: Option<BTreeSet<String>>,
    pub selected_id: Option<Option<String>>,
    pub loading: Option<bool>,
    pub ensuring_count: Option<u64>,
    pub error: Option<Option<String>>,
    pub column_inflight: Option<BTreeSet<String>>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SchemaStoreSnapshot {
    pub schemas: BTreeMap<String, SchemaSessionState>,
    pub active_db_session_id: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LoadForConnectionOptions {
    pub skip_load_tables: bool,
    pub preferred_database: Option<String>,
    pub database_type: Option<String>,
    pub activate: bool,
}

impl LoadForConnectionOptions {
    pub fn activates(&self) -> bool {
        self.activate
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct VisibleDatabases {
    pub databases: Vec<String>,
    pub preferred: Option<String>,
    pub locked_to_configured: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathDatabaseEntry {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DatabaseTypeMeta {
    pub has_multi_database: bool,
    pub schema_tree_mode_custom: bool,
    pub namespace_ensure_path_hierarchy: bool,
    pub namespace_owned_by_plugin: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SetLoadedTablesOptions {
    pub pin_current_database: bool,
    pub schema_names: Vec<String>,
}

impl SetLoadedTablesOptions {
    pub fn pins() -> Self {
        Self { pin_current_database: true, schema_names: Vec::new() }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LoadDatabasesDecision {
    pub databases: Vec<String>,
    pub preferred: Option<String>,
    pub locked_to_configured: bool,
    pub is_multi_database: bool,
    pub current_database: Option<String>,
    pub uses_plugin_db_list: bool,
    pub is_path_hierarchy: bool,
    pub alias_entries: Vec<PathDatabaseEntry>,
}
