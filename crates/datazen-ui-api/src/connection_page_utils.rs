use std::collections::BTreeMap;

pub const PENDING_CONNECTION_KEY: &str = "datazen:pending-connection";
pub const DEFAULT_DATABASE_TYPE: &str = "postgresql";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspaceMode {
    Connections,
    Workflow,
    Dashboard,
    Workspace,
    Extension,
}

impl WorkspaceMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Connections => "connections",
            Self::Workflow => "workflow",
            Self::Dashboard => "dashboard",
            Self::Workspace => "workspace",
            Self::Extension => "extension",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MainView {
    Workspace,
    Settings,
}

impl MainView {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Workspace => "workspace",
            Self::Settings => "settings",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionStatus {
    Connecting,
    Connected,
    Error,
}

impl ConnectionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Connecting => "connecting",
            Self::Connected => "connected",
            Self::Error => "error",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectionTab {
    pub connection_id: String,
    pub db_session_id: String,
    pub connection_name: String,
    pub database_type: String,
    pub initial_database: Option<String>,
    pub status: ConnectionStatus,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectionLookup {
    pub id: String,
    pub name: String,
    pub database_type: String,
    pub database: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingOpenQuery {
    pub connection_id: String,
    pub sql: String,
    pub request_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsumedConnection {
    pub tab: ConnectionTab,
    pub action: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectionStoreAction {
    SetActive(Option<String>),
    Remove(String),
}

pub fn make_tab_from_payload(data: &BTreeMap<String, String>) -> Option<ConnectionTab> {
    let connection_id = data.get("connectionId").cloned().unwrap_or_default();
    if connection_id.is_empty() {
        return None;
    }
    let db_session_id = data.get("dbSessionId").cloned().unwrap_or_default();
    let status = if db_session_id.is_empty() {
        ConnectionStatus::Connecting
    } else {
        ConnectionStatus::Connected
    };
    Some(ConnectionTab {
        connection_id,
        db_session_id,
        connection_name: data.get("connectionName").cloned().unwrap_or_default(),
        database_type: data
            .get("databaseType")
            .cloned()
            .unwrap_or_else(|| DEFAULT_DATABASE_TYPE.to_owned()),
        initial_database: data.get("database").cloned(),
        status,
        error: None,
    })
}

pub fn enrich_open_connection_payload(
    data: &BTreeMap<String, String>,
    connections: &[ConnectionLookup],
) -> BTreeMap<String, String> {
    let connection = match connections
        .iter()
        .find(|item| item.id == data.get("connectionId").cloned().unwrap_or_default())
    {
        Some(connection) => connection,
        None => return data.clone(),
    };
    let mut enriched = data.clone();
    let name = enriched.entry("connectionName".to_owned()).or_default();
    if name.is_empty() {
        *name = connection.name.clone();
    }
    let kind = enriched.entry("databaseType".to_owned()).or_default();
    if kind.is_empty() {
        *kind = connection.database_type.clone();
    }
    if enriched
        .get("database")
        .is_none_or(|value| value.is_empty())
    {
        if let Some(database) = &connection.database {
            enriched.insert("database".to_owned(), database.clone());
        }
    }
    enriched
}

pub fn pending_open_query_sql(
    pending: Option<&PendingOpenQuery>,
    tabs: &[(String, ConnectionStatus)],
    panels: &[String],
) -> Option<String> {
    let pending = pending?;
    let connected = tabs
        .iter()
        .any(|(id, status)| id == &pending.connection_id && *status == ConnectionStatus::Connected);
    if !connected {
        return None;
    }
    if panels.iter().any(|id| id == &pending.connection_id) {
        return None;
    }
    Some(pending.sql.clone())
}

pub fn consume_pending_connection(
    payload: Option<BTreeMap<String, String>>,
) -> Option<ConsumedConnection> {
    let data = payload?;
    let tab = make_tab_from_payload(&data)?;
    Some(ConsumedConnection {
        tab,
        action: data.get("action").cloned(),
    })
}

pub fn sync_stores_active_connection(db_session_id: Option<&str>) -> ConnectionStoreAction {
    ConnectionStoreAction::SetActive(db_session_id.map(str::to_owned))
}

pub fn remove_connection_from_stores(db_session_id: &str) -> ConnectionStoreAction {
    ConnectionStoreAction::Remove(db_session_id.to_owned())
}

pub const CONNECTION_PAGE_UTILS_SURFACE: [&str; 10] = [
    "makeTabFromPayload",
    "enrichOpenConnectionPayload",
    "pendingOpenQuerySql",
    "consumePendingConnection",
    "syncStoresActiveConnection",
    "removeConnectionFromStores",
    "WorkspaceMode",
    "MainView",
    "ConnectionTab",
    "CONNECTION_PAGE_UTILS_GAP",
];

pub const CONNECTION_PAGE_UTILS_GAP: [&str; 6] = [
    "localStorage read/remove of the pending-connection key and JSON.parse",
    "useSchemaStore setActiveConnection / removeConnection",
    "the host startup event that supplies only a connection id",
    "the schema store state used by the connection page",
    "React component consumption of WorkspaceMode / MainView",
    "the driver type union owned by the aggregate types",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn payload() -> BTreeMap<String, String> {
        BTreeMap::from([
            ("connectionId".to_owned(), "c1".to_owned()),
            ("dbSessionId".to_owned(), "s1".to_owned()),
            ("connectionName".to_owned(), "Local".to_owned()),
        ])
    }

    #[test]
    fn workspace_and_main_view_literals_are_pinned() {
        assert_eq!(WorkspaceMode::Connections.as_str(), "connections");
        assert_eq!(WorkspaceMode::Extension.as_str(), "extension");
        assert_eq!(MainView::Workspace.as_str(), "workspace");
        assert_eq!(MainView::Settings.as_str(), "settings");
        assert_eq!(ConnectionStatus::Connected.as_str(), "connected");
    }

    #[test]
    fn tab_from_payload_requires_a_connection_id() {
        let tab = make_tab_from_payload(&payload()).unwrap();
        assert_eq!(tab.connection_id, "c1");
        assert_eq!(tab.database_type, DEFAULT_DATABASE_TYPE);
        assert_eq!(tab.status, ConnectionStatus::Connected);
        assert_eq!(tab.initial_database, None);

        let mut missing = payload();
        missing.remove("connectionId");
        assert_eq!(make_tab_from_payload(&missing), None);

        let mut connecting = payload();
        connecting.remove("dbSessionId");
        assert_eq!(
            make_tab_from_payload(&connecting).map(|tab| tab.status),
            Some(ConnectionStatus::Connecting)
        );
    }

    #[test]
    fn enrichment_fills_only_missing_fields() {
        let connections = [ConnectionLookup {
            id: "c1".to_owned(),
            name: "Saved".to_owned(),
            database_type: "mysql".to_owned(),
            database: Some("shop".to_owned()),
        }];
        let enriched = enrich_open_connection_payload(&payload(), &connections);
        assert_eq!(enriched["connectionName"], "Local");
        assert_eq!(enriched["databaseType"], "mysql");
        assert_eq!(enriched["database"], "shop");

        let mut complete = payload();
        complete.insert("database".to_owned(), "other".to_owned());
        complete.insert("databaseType".to_owned(), "postgresql".to_owned());
        let enriched = enrich_open_connection_payload(&complete, &connections);
        assert_eq!(enriched["database"], "other");
        assert_eq!(enriched["databaseType"], "postgresql");

        let unknown = enrich_open_connection_payload(&payload(), &[]);
        assert_eq!(unknown, payload());
    }

    #[test]
    fn pending_query_needs_a_connected_tab_and_no_existing_panel() {
        let pending = PendingOpenQuery {
            connection_id: "c1".to_owned(),
            sql: "select 1".to_owned(),
            request_id: None,
        };
        let tabs = [("c1".to_owned(), ConnectionStatus::Connected)];
        assert_eq!(
            pending_open_query_sql(Some(&pending), &tabs, &[]).as_deref(),
            Some("select 1")
        );
        let connecting = [("c1".to_owned(), ConnectionStatus::Connecting)];
        assert_eq!(
            pending_open_query_sql(Some(&pending), &connecting, &[]),
            None
        );
        assert_eq!(
            pending_open_query_sql(Some(&pending), &tabs, &["c1".to_owned()]),
            None
        );
        assert_eq!(pending_open_query_sql(None, &tabs, &[]), None);
    }

    #[test]
    fn consuming_pending_connection_preserves_the_action() {
        let mut data = payload();
        data.insert("action".to_owned(), "connect".to_owned());
        let consumed = consume_pending_connection(Some(data)).unwrap();
        assert_eq!(consumed.tab.connection_id, "c1");
        assert_eq!(consumed.action.as_deref(), Some("connect"));
        assert_eq!(consume_pending_connection(None), None);
        assert_eq!(consume_pending_connection(Some(BTreeMap::new())), None);
    }

    #[test]
    fn store_actions_pin_their_arguments() {
        assert_eq!(
            sync_stores_active_connection(Some("s1")),
            ConnectionStoreAction::SetActive(Some("s1".to_owned()))
        );
        assert_eq!(
            sync_stores_active_connection(None),
            ConnectionStoreAction::SetActive(None)
        );
        assert_eq!(
            remove_connection_from_stores("s1"),
            ConnectionStoreAction::Remove("s1".to_owned())
        );
        assert_eq!(CONNECTION_PAGE_UTILS_SURFACE.len(), 10);
        assert_eq!(crate::CONNECTION_PAGE_UTILS_GAP.len(), 6);
    }
}
