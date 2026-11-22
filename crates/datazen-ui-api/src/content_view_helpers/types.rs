pub const ICON_CLASS: &str = "h-3.5 w-3.5 shrink-0";

pub const CONTENT_VIEW_HELPERS_SURFACE: [&str; 9] = [
    "getPanelIcon",
    "getPanelShortLabel",
    "resolvePanelTabDatabase",
    "getPanelTabLabel",
    "getPanelLabel",
    "getSubTabs",
    "getViewSubTabs",
    "resolveConnectionContext",
    "resolveConnectionContextByConnection",
];

pub const CONTENT_VIEW_HELPERS_GAP: [&str; 3] = [
    "ThemedIcon/createElement React element construction and lucide fallback components",
    "t() locale lookups are injected as a translator seam",
    "the two zustand getState() reads are replaced by injected snapshots",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelKind {
    Table,
    View,
    Query,
    CreateTable,
    ErDiagram,
    Objects,
    Privileges,
    ServerStatus,
    Processes,
    DbObject,
    RedisDb,
    Unknown,
}

impl PanelKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Table => "table",
            Self::View => "view",
            Self::Query => "query",
            Self::CreateTable => "create-table",
            Self::ErDiagram => "er-diagram",
            Self::Objects => "objects",
            Self::Privileges => "privileges",
            Self::ServerStatus => "server-status",
            Self::Processes => "processes",
            Self::DbObject => "db-object",
            Self::RedisDb => "redis-db",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContentViewPanel {
    pub kind: PanelKind,
    pub connection_name: String,
    pub table_name: Option<String>,
    pub view_name: Option<String>,
    pub title: Option<String>,
    pub database: Option<String>,
    pub object_kind: Option<String>,
    pub object_name: Option<String>,
    pub db_name: Option<String>,
}

impl ContentViewPanel {
    pub fn new(kind: PanelKind, connection_name: &str) -> Self {
        Self {
            kind,
            connection_name: connection_name.to_owned(),
            table_name: None,
            view_name: None,
            title: None,
            database: None,
            object_kind: None,
            object_name: None,
            db_name: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelIcon {
    Table,
    View,
    Query,
    CreateTable,
    ErDiagram,
    Objects,
    Privileges,
    ServerStatus,
    Processes,
    Trigger,
    Procedure,
    Sequence,
    Type,
    Function,
    RedisDb,
}

impl PanelIcon {
    pub fn id(self) -> &'static str {
        match self {
            Self::Table => "schema.table",
            Self::View => "schema.view",
            Self::Query => "common.newQuery",
            Self::CreateTable => "common.newTable",
            Self::ErDiagram => "common.erDiagram",
            Self::Objects => "common.objects",
            Self::Privileges => "action.privileges",
            Self::ServerStatus => "action.serverStatus",
            Self::Processes => "action.processes",
            Self::Trigger => "schema.trigger",
            Self::Procedure => "schema.procedure",
            Self::Sequence => "schema.sequence",
            Self::Type => "schema.type",
            Self::Function => "schema.function",
            Self::RedisDb => "schema.redisDatabase",
        }
    }

    pub fn fallback(self) -> &'static str {
        match self {
            Self::Table => "Table2",
            Self::View => "Eye",
            Self::Query | Self::Objects => "Code2",
            Self::CreateTable => "TableProperties",
            Self::ErDiagram => "GitFork",
            Self::Privileges => "KeyRound",
            Self::ServerStatus => "Activity",
            Self::Processes | Self::RedisDb => "Database",
            Self::Trigger => "Zap",
            Self::Procedure | Self::Function => "Braces",
            Self::Sequence => "Hash",
            Self::Type => "Shapes",
        }
    }

    pub fn tint(self) -> Option<&'static str> {
        match self {
            Self::Table => Some("text-blue-400"),
            Self::View => Some("text-purple-400"),
            Self::ServerStatus | Self::Procedure => Some("text-emerald-400"),
            Self::Processes => Some("text-rose-400"),
            Self::Trigger => Some("text-amber-400"),
            Self::Sequence => Some("text-cyan-400"),
            Self::Type => Some("text-pink-400"),
            Self::Function => Some("text-orange-400"),
            Self::RedisDb => Some("text-teal-400"),
            _ => None,
        }
    }

    pub fn class(self) -> String {
        match self.tint() {
            Some(tint) => format!("{ICON_CLASS} {tint}"),
            None => ICON_CLASS.to_owned(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubTabId {
    Data,
    Structure,
    Indexes,
    ForeignKeys,
    Ddl,
}

impl SubTabId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Data => "data",
            Self::Structure => "structure",
            Self::Indexes => "indexes",
            Self::ForeignKeys => "foreignKeys",
            Self::Ddl => "ddl",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubTab {
    pub id: SubTabId,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContentViewConnectionContext {
    pub connection_id: String,
    pub db_session_id: String,
    pub connection_name: String,
    pub database_type: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveConnectionEntry {
    pub connection_id: String,
    pub db_session_id: String,
}
