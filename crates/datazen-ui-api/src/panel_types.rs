use std::cell::RefCell;

use serde_json::Value;

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

    pub fn from_str(value: &str) -> Self {
        match value {
            "structure" => Self::Structure,
            "indexes" => Self::Indexes,
            "foreignKeys" => Self::ForeignKeys,
            "ddl" => Self::Ddl,
            _ => Self::Data,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PanelBase {
    pub id: String,
    pub connection_id: String,
    pub db_session_id: String,
    pub connection_name: String,
    pub database_type: String,
}

impl PanelBase {
    pub fn new(
        id: impl Into<String>,
        connection_id: impl Into<String>,
        db_session_id: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            connection_id: connection_id.into(),
            db_session_id: db_session_id.into(),
            connection_name: String::new(),
            database_type: String::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FkOrigin {
    pub source_panel_id: String,
    pub source_table: String,
    pub local_columns: Vec<String>,
    pub target_table: String,
    pub target_columns: Vec<String>,
    pub nav_entry_id: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TablePanel {
    pub base: PanelBase,
    pub table_name: String,
    pub database: String,
    pub table_schema: Option<String>,
    pub sub_tab: SubTabId,
    pub structure_editing: bool,
    pub target_column: Option<String>,
    pub fk_origin: Option<FkOrigin>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ViewPanel {
    pub base: PanelBase,
    pub view_name: String,
    pub database: String,
    pub view_schema: Option<String>,
    pub sub_tab: SubTabId,
}

#[derive(Clone, Debug, PartialEq)]
pub struct QueryPanel {
    pub base: PanelBase,
    pub title: String,
    pub database: String,
    pub schema: Option<String>,
    pub namespace_path: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CreateTablePanel {
    pub base: PanelBase,
    pub database: String,
    pub table_schema: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ErDiagramPanel {
    pub base: PanelBase,
    pub database: Option<String>,
    pub focus_table: Option<String>,
    pub schema: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ObjectsPanel {
    pub base: PanelBase,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PrivilegesPanel {
    pub base: PanelBase,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ServerStatusCache {
    pub status: std::collections::BTreeMap<String, Value>,
    pub variables: Vec<(String, Option<String>)>,
    pub history: std::collections::BTreeMap<String, Value>,
    pub updated_at: Option<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ServerStatusPanel {
    pub base: PanelBase,
    pub data: Option<ServerStatusCache>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProcessListCacheData {
    pub rows: Vec<Vec<Value>>,
    pub columns: Vec<(String, String, bool)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProcessesPanel {
    pub base: PanelBase,
    pub data: Option<ProcessListCacheData>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DbObjectKind {
    Function,
    Procedure,
    Trigger,
    Sequence,
    Type,
}

impl DbObjectKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Procedure => "procedure",
            Self::Trigger => "trigger",
            Self::Sequence => "sequence",
            Self::Type => "type",
        }
    }

    pub fn from_str(value: &str) -> Self {
        match value {
            "procedure" => Self::Procedure,
            "trigger" => Self::Trigger,
            "sequence" => Self::Sequence,
            "type" => Self::Type,
            _ => Self::Function,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DatabaseObjectPanel {
    pub base: PanelBase,
    pub object_kind: DbObjectKind,
    pub object_name: String,
    pub object_schema: Option<String>,
    pub object_database: Option<String>,
    pub object_signature: Option<String>,
    pub object_target_schema: Option<String>,
    pub object_target_name: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RedisTab {
    #[default]
    Items,
    Console,
    Pubsub,
    Monitor,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MonitorSubPage {
    #[default]
    Info,
    Monitor,
    Memory,
    Slowlog,
    Streams,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RedisPendingAction {
    pub tab: Option<RedisTab>,
    pub monitor_sub_page: Option<MonitorSubPage>,
    pub select_key: Option<String>,
    pub key_db_index: Option<u64>,
    pub open_import_export: bool,
    pub open_new_key: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RedisDbPanel {
    pub base: PanelBase,
    pub db_name: String,
    pub pending_action: Option<RedisPendingAction>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TypedPanel {
    Table(TablePanel),
    View(ViewPanel),
    Query(QueryPanel),
    CreateTable(CreateTablePanel),
    ErDiagram(ErDiagramPanel),
    Objects(ObjectsPanel),
    Privileges(PrivilegesPanel),
    ServerStatus(ServerStatusPanel),
    Processes(ProcessesPanel),
    DbObject(DatabaseObjectPanel),
    RedisDb(RedisDbPanel),
}

impl TypedPanel {
    pub fn base(&self) -> &PanelBase {
        match self {
            Self::Table(panel) => &panel.base,
            Self::View(panel) => &panel.base,
            Self::Query(panel) => &panel.base,
            Self::CreateTable(panel) => &panel.base,
            Self::ErDiagram(panel) => &panel.base,
            Self::Objects(panel) => &panel.base,
            Self::Privileges(panel) => &panel.base,
            Self::ServerStatus(panel) => &panel.base,
            Self::Processes(panel) => &panel.base,
            Self::DbObject(panel) => &panel.base,
            Self::RedisDb(panel) => &panel.base,
        }
    }

    pub fn type_str(&self) -> &'static str {
        match self {
            Self::Table(_) => "table",
            Self::View(_) => "view",
            Self::Query(_) => "query",
            Self::CreateTable(_) => "create-table",
            Self::ErDiagram(_) => "er-diagram",
            Self::Objects(_) => "objects",
            Self::Privileges(_) => "privileges",
            Self::ServerStatus(_) => "server-status",
            Self::Processes(_) => "processes",
            Self::DbObject(_) => "db-object",
            Self::RedisDb(_) => "redis-db",
        }
    }

    pub fn schema(&self) -> Option<String> {
        match self {
            Self::Table(panel) => panel.table_schema.clone(),
            Self::View(panel) => panel.view_schema.clone(),
            Self::Query(panel) => panel.schema.clone(),
            Self::CreateTable(panel) => panel.table_schema.clone(),
            Self::ErDiagram(panel) => panel.schema.clone(),
            Self::DbObject(panel) => panel.object_schema.clone(),
            _ => None,
        }
    }

    pub fn to_store_panel(&self) -> crate::StorePanel {
        let base = self.base();
        let mut panel = crate::StorePanel {
            id: base.id.clone(),
            connection_id: base.connection_id.clone(),
            panel_type: crate::StorePanelType::from_wire(self.type_str()),
            database: None,
            schema: self.schema(),
            table_name: None,
            view_name: None,
            db_name: None,
            sql: None,
        };
        match self {
            Self::Table(value) => {
                panel.database = Some(value.database.clone());
                panel.table_name = Some(value.table_name.clone());
            }
            Self::View(value) => {
                panel.database = Some(value.database.clone());
                panel.view_name = Some(value.view_name.clone());
            }
            Self::Query(value) => {
                panel.database = Some(value.database.clone());
                panel.sql = Some(String::new());
            }
            Self::CreateTable(value) => panel.database = Some(value.database.clone()),
            Self::ErDiagram(value) => panel.database = value.database.clone(),
            Self::DbObject(value) => panel.database = value.object_database.clone(),
            Self::RedisDb(value) => panel.db_name = Some(value.db_name.clone()),
            _ => {}
        }
        panel
    }
}

thread_local! {
    static PANEL_ID_COUNTER: RefCell<u64> = const { RefCell::new(0) };
}

pub fn next_panel_id(prefix: &str) -> String {
    PANEL_ID_COUNTER.with(|counter| {
        let mut value = counter.borrow_mut();
        *value += 1;
        format!("panel-{prefix}-{value}")
    })
}

pub fn reset_panel_id_counter() {
    PANEL_ID_COUNTER.with(|counter| *counter.borrow_mut() = 0);
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConnectionContext {
    pub connection_id: String,
    pub db_session_id: String,
    pub connection_name: String,
    pub database_type: String,
}

pub fn typed_resolve_next_active(
    panels: &[TypedPanel],
    removed_id: &str,
    current_active_id: Option<&str>,
) -> Option<String> {
    let lean = panels
        .iter()
        .map(TypedPanel::to_store_panel)
        .collect::<Vec<_>>();
    crate::resolve_next_active(&lean, removed_id, current_active_id)
}

pub fn panel_schema(panel: Option<&TypedPanel>) -> Option<String> {
    panel.and_then(TypedPanel::schema)
}

pub const PANEL_TYPES_SURFACE: [&str; 8] = [
    "SubTabId",
    "FkOrigin",
    "TypedPanel",
    "panelSchema",
    "nextPanelId",
    "resetPanelIdCounter",
    "ConnectionContext",
    "PANEL_TYPES_GAP",
];

pub const PANEL_TYPES_GAP: [&str; 4] = [
    "the React Panel discriminated union is mirrored as TypedPanel",
    "DatabaseType and TrendSeries are opaque as String / JSON values here",
    "the panel store and view layer own the runtime Panel instances",
    "no DOM or IPC calls exist in this module",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn base(id: &str) -> PanelBase {
        PanelBase::new(id, "conn", "session")
    }

    fn table(id: &str, schema: Option<&str>) -> TypedPanel {
        TypedPanel::Table(TablePanel {
            base: base(id),
            table_name: "users".to_owned(),
            database: "app".to_owned(),
            table_schema: schema.map(str::to_owned),
            sub_tab: SubTabId::Data,
            structure_editing: false,
            target_column: None,
            fk_origin: None,
        })
    }

    #[test]
    fn sub_tab_and_object_kind_literals_round_trip() {
        for (kind, literal) in [
            (SubTabId::Data, "data"),
            (SubTabId::Structure, "structure"),
            (SubTabId::Indexes, "indexes"),
            (SubTabId::ForeignKeys, "foreignKeys"),
            (SubTabId::Ddl, "ddl"),
        ] {
            assert_eq!(kind.as_str(), literal);
            assert_eq!(SubTabId::from_str(literal), kind);
        }
        assert_eq!(SubTabId::from_str("unknown"), SubTabId::Data);
        assert_eq!(DbObjectKind::Function.as_str(), "function");
        assert_eq!(DbObjectKind::from_str("sequence"), DbObjectKind::Sequence);
        assert_eq!(DbObjectKind::from_str("nope"), DbObjectKind::Function);
    }

    #[test]
    fn panel_schema_reads_only_the_declared_shapes() {
        assert_eq!(panel_schema(None), None);
        assert_eq!(
            panel_schema(Some(&table("p", Some("public")))).as_deref(),
            Some("public")
        );
        assert_eq!(panel_schema(Some(&table("p", None))), None);
        let view = TypedPanel::View(ViewPanel {
            base: base("v"),
            view_name: "v".to_owned(),
            database: "app".to_owned(),
            view_schema: Some("vs".to_owned()),
            sub_tab: SubTabId::Structure,
        });
        assert_eq!(panel_schema(Some(&view)).as_deref(), Some("vs"));
        let objects = TypedPanel::Objects(ObjectsPanel { base: base("o") });
        assert_eq!(panel_schema(Some(&objects)), None);
    }

    #[test]
    fn typed_panels_project_to_the_lean_store_shape() {
        let store = table("p", Some("public")).to_store_panel();
        assert_eq!(store.id, "p");
        assert_eq!(store.connection_id, "conn");
        assert_eq!(store.panel_type, crate::StorePanelType::Table);
        assert_eq!(store.database.as_deref(), Some("app"));
        assert_eq!(store.table_name.as_deref(), Some("users"));
        assert_eq!(store.schema.as_deref(), Some("public"));
        let query = TypedPanel::Query(QueryPanel {
            base: base("q"),
            title: "Q".to_owned(),
            database: "db".to_owned(),
            schema: None,
            namespace_path: vec!["a".to_owned()],
        });
        let projected = query.to_store_panel();
        assert_eq!(projected.panel_type, crate::StorePanelType::Query);
        assert_eq!(projected.database.as_deref(), Some("db"));
    }

    #[test]
    fn next_panel_id_is_sequential_and_resettable() {
        reset_panel_id_counter();
        assert_eq!(next_panel_id("x"), "panel-x-1");
        assert_eq!(next_panel_id("y"), "panel-y-2");
        reset_panel_id_counter();
        assert_eq!(next_panel_id("z"), "panel-z-1");
    }

    #[test]
    fn next_active_follows_the_lean_port() {
        let panels = vec![table("a", None), table("b", None)];
        assert_eq!(
            typed_resolve_next_active(&panels, "a", Some("a")).as_deref(),
            Some("b")
        );
        assert_eq!(
            typed_resolve_next_active(&panels, "a", Some("b")).as_deref(),
            Some("b")
        );
        assert_eq!(typed_resolve_next_active(&panels, "b", None), None);
    }

    #[test]
    fn gap_and_surface_are_pinned_and_root_reexports_match() {
        assert_eq!(PANEL_TYPES_SURFACE.len(), 8);
        assert!(PANEL_TYPES_SURFACE.contains(&"TypedPanel"));
        assert_eq!(PANEL_TYPES_GAP.len(), 4);
        assert_eq!(crate::PANEL_TYPES_SURFACE.len(), 8);
        assert_eq!(crate::PANEL_TYPES_GAP.len(), 4);
        let _: Option<crate::PanelBase> = Some(base("x"));
    }
}
