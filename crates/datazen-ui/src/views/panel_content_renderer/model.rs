//! Pure view models and rules for the panel content renderer.

use std::cell::RefCell;
use std::rc::Rc;

/// React `Panel['type']`.
#[allow(dead_code)] // view VM; consumer: the content-view host (not wired).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PanelType {
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
}

impl PanelType {
    pub(crate) fn as_str(self) -> &'static str {
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
        }
    }
}

/// React `SubTabId`.
#[allow(dead_code)] // view VM; consumer: the content-view host (not wired).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SubTabId {
    Data,
    Structure,
    Indexes,
    ForeignKeys,
    Ddl,
}

impl SubTabId {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Data => "data",
            Self::Structure => "structure",
            Self::Indexes => "indexes",
            Self::ForeignKeys => "foreignKeys",
            Self::Ddl => "ddl",
        }
    }

    fn label_key(self) -> &'static str {
        match self {
            Self::Data => "connWin.data",
            Self::Structure => "connWin.structure",
            Self::Indexes => "connWin.indexes",
            Self::ForeignKeys => "connWin.foreignKeys",
            Self::Ddl => "",
        }
    }

    pub(crate) fn label(self, t: &dyn Fn(&str) -> String) -> String {
        if self == Self::Ddl {
            "DDL".to_owned()
        } else {
            t(self.label_key())
        }
    }
}

/// React `getSubTabs`.
#[allow(dead_code)] // pure rule; pinned by the renderer tests.
pub(crate) fn sub_tabs(read_only: bool, t: &dyn Fn(&str) -> String) -> Vec<(SubTabId, String)> {
    let mut tabs = vec![
        (SubTabId::Data, SubTabId::Data.label(t)),
        (SubTabId::Structure, SubTabId::Structure.label(t)),
    ];
    if !read_only {
        tabs.push((SubTabId::Indexes, SubTabId::Indexes.label(t)));
        tabs.push((SubTabId::ForeignKeys, SubTabId::ForeignKeys.label(t)));
        tabs.push((SubTabId::Ddl, SubTabId::Ddl.label(t)));
    }
    tabs
}

/// React `getViewSubTabs`.
#[allow(dead_code)] // pure rule; pinned by the renderer tests.
pub(crate) fn view_sub_tabs(t: &dyn Fn(&str) -> String) -> Vec<(SubTabId, String)> {
    vec![
        (SubTabId::Data, SubTabId::Data.label(t)),
        (SubTabId::Structure, SubTabId::Structure.label(t)),
        (SubTabId::Ddl, SubTabId::Ddl.label(t)),
    ]
}

/// React `formatPanelContextPath` — connection name verbatim, trimmed database
/// and schema appended when non-empty, all joined with `.`.
#[allow(dead_code)] // pure rule; pinned by the renderer tests.
pub(crate) fn format_context_path(
    connection_name: &str,
    database: Option<&str>,
    schema: Option<&str>,
) -> String {
    let mut parts = vec![connection_name];
    if let Some(database) = database {
        let database = database.trim();
        if !database.is_empty() {
            parts.push(database);
        }
    }
    if let Some(schema) = schema {
        let schema = schema.trim();
        if !schema.is_empty() {
            parts.push(schema);
        }
    }
    parts.join(".")
}

/// The driver registries this renderer consults: only the two facts it needs.
#[allow(dead_code)] // registry stand-in; replaced by the shared driver meta port.
pub(crate) fn is_key_value_driver(database_type: &str) -> bool {
    matches!(database_type, "kvredis" | "redis")
}

#[allow(dead_code)] // registry stand-in; replaced by the shared driver meta port.
pub(crate) fn is_read_only_driver(database_type: &str) -> bool {
    matches!(database_type, "sqlite")
}

/// Host-owned KV atom (React `createKvSlotState`). The `Rc<RefCell<_>>` is the
/// identity the renderer forwards untouched to the driver view — a recreated
/// atom would fork the workbench publish side from the slot read side.
#[allow(dead_code)] // host contract; consumer: the KV workbench host (not wired).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct KvSlotState {
    pub selected_key: String,
    pub dirty: bool,
}

pub(crate) type KvSlotAtom = Rc<RefCell<KvSlotState>>;

#[allow(dead_code)] // host contract; consumer: the KV workbench host (not wired).
pub(crate) fn create_kv_slot_state() -> KvSlotAtom {
    Rc::new(RefCell::new(KvSlotState::default()))
}

/// The panel state the renderer reads.
#[allow(dead_code)] // view VM; consumer: the content-view host (not wired).
#[derive(Clone, Debug)]
pub(crate) struct PanelVm {
    pub id: String,
    pub panel_type: PanelType,
    pub connection_id: String,
    pub db_session_id: String,
    pub connection_name: String,
    pub database_type: String,
    pub database: Option<String>,
    pub schema: Option<String>,
    pub table_name: String,
    pub table_schema: Option<String>,
    pub sub_tab: SubTabId,
    pub structure_editing: bool,
    /// A known KV panel's selected database (`kvPanel.dbName`).
    pub kv_db_name: Option<String>,
    /// True while a multi-database session has not picked a database yet.
    pub multi_db_without_database: bool,
    /// Server-status / processes panel cache marker.
    pub has_panel_data: bool,
}

impl Default for PanelVm {
    fn default() -> Self {
        Self {
            id: String::new(),
            panel_type: PanelType::Table,
            connection_id: String::new(),
            db_session_id: String::new(),
            connection_name: String::new(),
            database_type: "postgresql".to_owned(),
            database: None,
            schema: None,
            table_name: String::new(),
            table_schema: None,
            sub_tab: SubTabId::Data,
            structure_editing: false,
            kv_db_name: None,
            multi_db_without_database: false,
            has_panel_data: false,
        }
    }
}

/// Props the KV driver view receives (mirrors the React branch).
#[allow(dead_code)] // host contract; consumer: the KV workbench host (not wired).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct KvViewProps {
    pub panel_id: String,
    pub db_session_id: String,
    pub connection_id: String,
    pub connection_name: String,
    pub database_type: String,
    pub initial_database: Option<String>,
    pub hide_sidebar: bool,
    pub is_active: bool,
    pub has_slot_state: bool,
    pub selected_key: String,
    pub dirty: bool,
}
