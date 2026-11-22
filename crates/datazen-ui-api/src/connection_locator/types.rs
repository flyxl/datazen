use crate::connection_types::{ConnectionConfig, DatabaseObject, TableInfo};
use crate::build_editor_schema::SqlNamespace;

pub const RECENT_CONNECTION_LIMIT: usize = 5;
pub const RECENT_GROUP_KEY: &str = "__recent__";
pub const PINNED_GROUP_KEY: &str = "__pinned__";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionMatchReason {
    Name,
    Host,
    Database,
    DatabaseType,
    Schema,
    Table,
    View,
    Object,
    Namespace,
    Path,
}

impl ConnectionMatchReason {
    pub fn as_str(self) -> &'static str {
        match self {
            ConnectionMatchReason::Name => "name",
            ConnectionMatchReason::Host => "host",
            ConnectionMatchReason::Database => "database",
            ConnectionMatchReason::DatabaseType => "database-type",
            ConnectionMatchReason::Schema => "schema",
            ConnectionMatchReason::Table => "table",
            ConnectionMatchReason::View => "view",
            ConnectionMatchReason::Object => "object",
            ConnectionMatchReason::Namespace => "namespace",
            ConnectionMatchReason::Path => "path",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectionMatch {
    pub rank: i32,
    pub reason: ConnectionMatchReason,
    pub context: String,
}

pub type Candidate = ConnectionMatch;

#[derive(Clone, Debug, PartialEq)]
pub struct ConnectionLocatorResult {
    pub connection: ConnectionConfig,
    pub match_value: Option<ConnectionMatch>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ActiveStatus {
    Idle,
    Connecting,
    #[default]
    Connected,
    Error,
}

impl ActiveStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ActiveStatus::Idle => "idle",
            ActiveStatus::Connecting => "connecting",
            ActiveStatus::Connected => "connected",
            ActiveStatus::Error => "error",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ActiveConnectionSlice {
    pub db_session_id: String,
    pub status: ActiveStatus,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemaSlice {
    pub databases: Vec<String>,
    pub schema_names: Vec<String>,
    pub tables: Vec<TableInfo>,
    pub views: Vec<TableInfo>,
    pub namespace_tree: SqlNamespace,
    pub path_items: Vec<(String, Vec<TableInfo>)>,
}

impl Default for SchemaSlice {
    fn default() -> Self {
        Self {
            databases: Vec::new(),
            schema_names: Vec::new(),
            tables: Vec::new(),
            views: Vec::new(),
            namespace_tree: SqlNamespace::empty_branch(),
            path_items: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConnectionLocatorUsageState {
    pub active_connections: Vec<(String, ActiveConnectionSlice)>,
    pub schemas: Vec<(String, SchemaSlice)>,
    pub db_tables_map: Vec<(String, Vec<TableInfo>)>,
    pub db_objects_map: Vec<(String, Vec<DatabaseObject>)>,
}

impl ConnectionLocatorUsageState {
    pub fn active(&self, id: &str) -> Option<&ActiveConnectionSlice> {
        self.active_connections.iter().find(|(key, _)| key == id).map(|(_, value)| value)
    }

    pub fn schema(&self, session_id: &str) -> Option<&SchemaSlice> {
        self.schemas.iter().find(|(key, _)| key == session_id).map(|(_, value)| value)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ConnectionSection {
    pub group: String,
    pub connections: Vec<ConnectionConfig>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExpandKey {
    pub section_group: String,
    pub connection_id: String,
}

pub(crate) fn get<'a, V>(entries: &'a [(String, V)], key: &str) -> Option<&'a V> {
    entries.iter().find(|(entry_key, _)| entry_key == key).map(|(_, value)| value)
}
