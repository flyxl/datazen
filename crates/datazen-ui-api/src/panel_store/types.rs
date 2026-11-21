use std::collections::BTreeMap;

use crate::{ChartConfig, FavoriteQuery, QueryHistoryEntry, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorePanelType {
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
    Other,
}

impl StorePanelType {
    pub fn from_wire(value: &str) -> Self {
        match value {
            "table" => Self::Table,
            "view" => Self::View,
            "query" => Self::Query,
            "create-table" => Self::CreateTable,
            "er-diagram" => Self::ErDiagram,
            "objects" => Self::Objects,
            "privileges" => Self::Privileges,
            "server-status" => Self::ServerStatus,
            "processes" => Self::Processes,
            "db-object" => Self::DbObject,
            "redis-db" => Self::RedisDb,
            _ => Self::Other,
        }
    }

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
            Self::Other => "other",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct StorePanel {
    pub id: String,
    pub connection_id: String,
    pub panel_type: StorePanelType,
    pub database: Option<String>,
    pub schema: Option<String>,
    pub table_name: Option<String>,
    pub view_name: Option<String>,
    pub db_name: Option<String>,
    pub sql: Option<String>,
}

impl StorePanel {
    pub fn query(id: impl Into<String>, connection_id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            connection_id: connection_id.into(),
            panel_type: StorePanelType::Query,
            database: None,
            schema: None,
            table_name: None,
            view_name: None,
            db_name: None,
            sql: None,
        }
    }

    pub fn table(
        id: impl Into<String>,
        connection_id: impl Into<String>,
        table: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            connection_id: connection_id.into(),
            panel_type: StorePanelType::Table,
            database: None,
            schema: None,
            table_name: Some(table.into()),
            view_name: None,
            db_name: None,
            sql: None,
        }
    }

    pub fn view(
        id: impl Into<String>,
        connection_id: impl Into<String>,
        view: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            connection_id: connection_id.into(),
            panel_type: StorePanelType::View,
            database: None,
            schema: None,
            table_name: None,
            view_name: Some(view.into()),
            db_name: None,
            sql: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResultColumn {
    pub name: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResultView {
    pub columns: Vec<ResultColumn>,
    pub rows: Vec<Vec<Value>>,
    pub pinned: bool,
    pub runtime_binding: Option<String>,
    pub statement_index: Option<u32>,
    pub writable_mapping_verified: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PanelExecState {
    pub sql: String,
    pub results: Vec<ResultView>,
    pub active_result_idx: usize,
    pub running: bool,
    pub execution_id: Option<String>,
    pub cancel_state: crate::QueryCancelState,
    pub result_detail_row_index: Option<u64>,
    pub chart_config: Option<ChartConfig>,
    pub result_view_mode: Option<String>,
    pub error: Option<String>,
}

impl PanelExecState {
    pub fn empty() -> Self {
        Self {
            sql: String::new(),
            results: Vec::new(),
            active_result_idx: 0,
            running: false,
            execution_id: None,
            cancel_state: crate::QueryCancelState::Idle,
            result_detail_row_index: None,
            chart_config: None,
            result_view_mode: None,
            error: None,
        }
    }
}

pub type ExecTable = BTreeMap<String, PanelExecState>;
pub type BindParams = BTreeMap<String, Value>;

#[derive(Clone, Debug, PartialEq)]
pub struct PendingHistoryQuery {
    pub connection_id: String,
    pub sql: String,
    pub database: Option<String>,
    pub schema: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResultEdit {
    pub row: u64,
    pub col: String,
    pub value: Value,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PanelPatch {
    pub database: Option<Option<String>>,
    pub schema: Option<Option<String>>,
    pub table_name: Option<Option<String>>,
    pub view_name: Option<Option<String>>,
    pub db_name: Option<Option<String>>,
    pub sql: Option<Option<String>>,
}

impl PanelPatch {
    pub fn apply(&self, panel: &mut StorePanel) {
        if let Some(value) = &self.database {
            panel.database = value.clone();
        }
        if let Some(value) = &self.schema {
            panel.schema = value.clone();
        }
        if let Some(value) = &self.table_name {
            panel.table_name = value.clone();
        }
        if let Some(value) = &self.view_name {
            panel.view_name = value.clone();
        }
        if let Some(value) = &self.db_name {
            panel.db_name = value.clone();
        }
        if let Some(value) = &self.sql {
            panel.sql = value.clone();
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PanelState {
    pub panels: Vec<StorePanel>,
    pub active_panel_id: Option<String>,
    pub query_exec: ExecTable,
    pub focused_pane_id_by_panel: BTreeMap<String, String>,
    pub focused_pane_id: Option<String>,
    pub query_history: Vec<QueryHistoryEntry>,
    pub query_favorites: Vec<FavoriteQuery>,
    pub favorites_root: Option<String>,
    pub history_visible: bool,
    pub favorites_visible: bool,
    pub pending_query_history_connection_id: Option<String>,
    pub pending_history_query: Option<PendingHistoryQuery>,
}

impl Default for PanelState {
    fn default() -> Self {
        Self {
            panels: Vec::new(),
            active_panel_id: None,
            query_exec: BTreeMap::new(),
            focused_pane_id_by_panel: BTreeMap::new(),
            focused_pane_id: None,
            query_history: Vec::new(),
            query_favorites: Vec::new(),
            favorites_root: None,
            history_visible: false,
            favorites_visible: false,
            pending_query_history_connection_id: None,
            pending_history_query: None,
        }
    }
}
