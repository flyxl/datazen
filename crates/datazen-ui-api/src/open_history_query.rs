#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HistoryQueryEntry {
    pub sql: String,
    pub connection_id: Option<String>,
    pub database: Option<String>,
    pub schema: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OpenHistoryQueryOptions {
    pub select_connection: bool,
    pub current_connection_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedConnection {
    pub id: String,
    pub name: String,
    pub database_type: String,
    pub database: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveConnection {
    pub connection_id: String,
    pub status: String,
    pub db_session_id: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PathHierarchySplit {
    pub root: Option<String>,
    pub namespace_path: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryPanelState {
    pub id: String,
    pub connection_id: String,
    pub db_session_id: String,
    pub connection_name: String,
    pub database_type: String,
    pub title: String,
    pub database: String,
    pub schema: Option<String>,
    pub namespace_path: Option<Vec<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingHistoryQuery {
    pub connection_id: String,
    pub sql: String,
    pub database: Option<String>,
    pub schema: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum OpenHistoryOutcome {
    NoTargetConnection,
    SavedConnectionMissing { connection_id: String },
    PanelOpened { panel_id: String },
    PendingSet { connection_id: String },
}

pub trait OpenHistoryQueryHost {
    fn saved_connections(&self) -> Vec<SavedConnection>;

    fn active_connections(&self) -> Vec<ActiveConnection>;

    fn select_connection(&self, connection_id: &str);

    fn next_panel_id(&self, prefix: &str) -> String;

    fn namespace_ensure(&self, database_type: &str) -> Option<String>;

    fn split_path_hierarchy_database_pin(&self, value: &str) -> PathHierarchySplit;

    fn add_panel(&mut self, panel: QueryPanelState, activate: bool);

    fn update_sql(&mut self, panel_id: &str, sql: &str);

    fn set_active_panel(&mut self, panel_id: &str);

    fn set_pending_history_query(&mut self, query: PendingHistoryQuery);
}

fn trimmed(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

fn resolve_target(
    entry: &HistoryQueryEntry,
    options: &OpenHistoryQueryOptions,
    active: &[ActiveConnection],
    saved: &[SavedConnection],
) -> Option<String> {
    entry
        .connection_id
        .clone()
        .filter(|id| !id.is_empty())
        .or_else(|| {
            options
                .current_connection_id
                .clone()
                .filter(|id| !id.is_empty())
        })
        .or_else(|| {
            active
                .iter()
                .find(|connection| connection.status == "connected")
                .map(|connection| connection.connection_id.clone())
        })
        .or_else(|| saved.first().map(|connection| connection.id.clone()))
}

pub fn open_history_query(
    entry: &HistoryQueryEntry,
    options: &OpenHistoryQueryOptions,
    host: &mut dyn OpenHistoryQueryHost,
) -> OpenHistoryOutcome {
    let saved = host.saved_connections();
    let active = host.active_connections();
    let target = match resolve_target(entry, options, &active, &saved) {
        Some(target) => target,
        None => return OpenHistoryOutcome::NoTargetConnection,
    };
    let saved_conn = match saved.iter().find(|connection| connection.id == target) {
        Some(connection) => connection.clone(),
        None => {
            return OpenHistoryOutcome::SavedConnectionMissing {
                connection_id: target,
            }
        }
    };
    if options.select_connection {
        host.select_connection(&target);
    }

    let active_entry = active
        .iter()
        .find(|connection| connection.connection_id == target);
    let session = active_entry.and_then(|connection| {
        if connection.status == "connected" {
            trimmed(connection.db_session_id.as_deref())
        } else {
            None
        }
    });
    let db_session_id = match session {
        Some(session) => session,
        None => {
            host.set_pending_history_query(PendingHistoryQuery {
                connection_id: target.clone(),
                sql: entry.sql.clone(),
                database: entry.database.clone(),
                schema: entry.schema.clone(),
            });
            return OpenHistoryOutcome::PendingSet {
                connection_id: target,
            };
        }
    };

    let panel_id = host.next_panel_id("qry");
    let mut panel_database =
        trimmed(entry.database.as_deref()).or_else(|| trimmed(saved_conn.database.as_deref()));
    let mut namespace_path = None;
    let is_path_hierarchy =
        host.namespace_ensure(&saved_conn.database_type).as_deref() == Some("path-hierarchy");
    if is_path_hierarchy {
        if let Some(database) = panel_database.as_ref() {
            if database.contains('/') {
                let split = host.split_path_hierarchy_database_pin(database);
                panel_database = trimmed(split.root.as_deref());
                let mut path = Vec::new();
                if let Some(root) = split.root {
                    path.push(root);
                }
                path.extend(split.namespace_path);
                namespace_path = Some(path);
            }
        }
    }
    let db = panel_database.clone().unwrap_or_default();
    let title = if db.is_empty() {
        saved_conn.name.clone()
    } else {
        format!("{}@{db}", saved_conn.name)
    };
    let panel = QueryPanelState {
        id: panel_id.clone(),
        connection_id: target,
        db_session_id,
        connection_name: saved_conn.name,
        database_type: saved_conn.database_type,
        title,
        database: db,
        schema: trimmed(entry.schema.as_deref()),
        namespace_path,
    };
    host.add_panel(panel, true);
    if !entry.sql.is_empty() {
        host.update_sql(&panel_id, &entry.sql);
    }
    host.set_active_panel(&panel_id);
    OpenHistoryOutcome::PanelOpened { panel_id }
}

pub const OPEN_HISTORY_QUERY_SURFACE: [&str; 8] = [
    "openHistoryQuery",
    "HistoryQueryEntry",
    "OpenHistoryQueryOptions",
    "SavedConnection",
    "ActiveConnection",
    "QueryPanelState",
    "PendingHistoryQuery",
    "OPEN_HISTORY_QUERY_GAP",
];

pub const OPEN_HISTORY_QUERY_GAP: [&str; 6] = [
    "the two zustand store reads are injected as host connection snapshots",
    "nextPanelId / DB_REGISTRY / splitPathHierarchyDatabasePin are injected seams",
    "panelStore addPanel / updateSql / setActivePanel / setPendingHistoryQuery are host calls",
    "onSelectConnection is a host callback seam",
    "no DOM or IPC calls exist in this module",
    "the pending intent is consumed by usePanelHandlers outside this file",
];

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    #[derive(Default)]
    struct Host {
        saved: Vec<SavedConnection>,
        active: Vec<ActiveConnection>,
        path_hierarchy: bool,
        split: PathHierarchySplit,
        selected: RefCell<Vec<String>>,
        panels: RefCell<Vec<(QueryPanelState, bool)>>,
        updates: RefCell<Vec<(String, String)>>,
        activated: RefCell<Vec<String>>,
        pending: RefCell<Vec<PendingHistoryQuery>>,
    }

    impl OpenHistoryQueryHost for Host {
        fn saved_connections(&self) -> Vec<SavedConnection> {
            self.saved.clone()
        }

        fn active_connections(&self) -> Vec<ActiveConnection> {
            self.active.clone()
        }

        fn select_connection(&self, connection_id: &str) {
            self.selected.borrow_mut().push(connection_id.to_owned());
        }

        fn next_panel_id(&self, prefix: &str) -> String {
            format!("{prefix}-1")
        }

        fn namespace_ensure(&self, _database_type: &str) -> Option<String> {
            if self.path_hierarchy {
                Some("path-hierarchy".to_owned())
            } else {
                None
            }
        }

        fn split_path_hierarchy_database_pin(&self, _value: &str) -> PathHierarchySplit {
            self.split.clone()
        }

        fn add_panel(&mut self, panel: QueryPanelState, activate: bool) {
            self.panels.borrow_mut().push((panel, activate));
        }

        fn update_sql(&mut self, panel_id: &str, sql: &str) {
            self.updates
                .borrow_mut()
                .push((panel_id.to_owned(), sql.to_owned()));
        }

        fn set_active_panel(&mut self, panel_id: &str) {
            self.activated.borrow_mut().push(panel_id.to_owned());
        }

        fn set_pending_history_query(&mut self, query: PendingHistoryQuery) {
            self.pending.borrow_mut().push(query);
        }
    }

    fn saved() -> SavedConnection {
        SavedConnection {
            id: "c1".to_owned(),
            name: "Prod".to_owned(),
            database_type: "postgres".to_owned(),
            database: Some("app".to_owned()),
        }
    }

    fn connected(session: &str) -> ActiveConnection {
        ActiveConnection {
            connection_id: "c1".to_owned(),
            status: "connected".to_owned(),
            db_session_id: Some(session.to_owned()),
        }
    }

    fn host() -> Host {
        Host {
            saved: vec![saved()],
            active: vec![connected("s1")],
            ..Host::default()
        }
    }

    #[test]
    fn an_open_session_creates_a_query_panel_with_the_sql() {
        let mut h = host();
        let entry = HistoryQueryEntry {
            sql: "SELECT 1".to_owned(),
            connection_id: Some("c1".to_owned()),
            database: Some(" app2 ".to_owned()),
            schema: Some(" public ".to_owned()),
        };
        let outcome = open_history_query(&entry, &OpenHistoryQueryOptions::default(), &mut h);
        assert_eq!(
            outcome,
            OpenHistoryOutcome::PanelOpened {
                panel_id: "qry-1".to_owned()
            }
        );
        let (panel, activate) = &h.panels.borrow()[0];
        assert!(activate);
        assert_eq!(panel.id, "qry-1");
        assert_eq!(panel.db_session_id, "s1");
        assert_eq!(panel.database, "app2");
        assert_eq!(panel.schema.as_deref(), Some("public"));
        assert_eq!(panel.title, "Prod@app2");
        assert_eq!(panel.namespace_path, None);
        assert_eq!(
            h.updates.borrow().as_slice(),
            &[("qry-1".to_owned(), "SELECT 1".to_owned())]
        );
        assert_eq!(h.activated.borrow().as_slice(), &["qry-1".to_owned()]);
    }

    #[test]
    fn a_missing_sql_does_not_issue_an_update() {
        let mut h = host();
        let entry = HistoryQueryEntry {
            connection_id: Some("c1".to_owned()),
            ..HistoryQueryEntry::default()
        };
        let _ = open_history_query(&entry, &OpenHistoryQueryOptions::default(), &mut h);
        assert!(h.updates.borrow().is_empty());
        assert_eq!(h.panels.borrow()[0].0.title, "Prod@app");
    }

    #[test]
    fn a_path_hierarchy_pin_splits_the_database_and_binds_the_path() {
        let mut h = host();
        h.path_hierarchy = true;
        h.split = PathHierarchySplit {
            root: Some("root".to_owned()),
            namespace_path: vec!["cat".to_owned(), "sch".to_owned()],
        };
        let entry = HistoryQueryEntry {
            sql: "SELECT 1".to_owned(),
            connection_id: Some("c1".to_owned()),
            database: Some("root/cat/sch".to_owned()),
            ..HistoryQueryEntry::default()
        };
        let _ = open_history_query(&entry, &OpenHistoryQueryOptions::default(), &mut h);
        let panel = &h.panels.borrow()[0].0;
        assert_eq!(panel.database, "root");
        assert_eq!(panel.title, "Prod@root");
        assert_eq!(
            panel.namespace_path.clone(),
            Some(vec!["root".to_owned(), "cat".to_owned(), "sch".to_owned()])
        );
        assert_eq!(h.split.namespace_path.len(), 2);
    }

    #[test]
    fn a_disconnected_session_sets_a_pending_intent() {
        let mut h = host();
        h.active = vec![ActiveConnection {
            status: "connecting".to_owned(),
            ..connected("s1")
        }];
        let entry = HistoryQueryEntry {
            sql: "SELECT 1".to_owned(),
            connection_id: Some("c1".to_owned()),
            database: Some("app".to_owned()),
            schema: Some("public".to_owned()),
        };
        let outcome = open_history_query(&entry, &OpenHistoryQueryOptions::default(), &mut h);
        assert_eq!(
            outcome,
            OpenHistoryOutcome::PendingSet {
                connection_id: "c1".to_owned()
            }
        );
        assert!(h.panels.borrow().is_empty());
        let pending = &h.pending.borrow()[0];
        assert_eq!(pending.sql, "SELECT 1");
        assert_eq!(pending.database.as_deref(), Some("app"));
        assert_eq!(pending.schema.as_deref(), Some("public"));
    }

    #[test]
    fn target_resolution_follows_the_documented_priority() {
        let mut h = host();
        let mut second = saved();
        second.id = "c2".to_owned();
        h.saved.push(second);
        let empty = HistoryQueryEntry::default();

        let outcome = open_history_query(&empty, &OpenHistoryQueryOptions::default(), &mut h);
        assert_eq!(
            outcome,
            OpenHistoryOutcome::PanelOpened {
                panel_id: "qry-1".to_owned()
            }
        );
        assert_eq!(h.panels.borrow()[0].0.connection_id, "c1");

        let without_active = Host {
            saved: h.saved.clone(),
            active: Vec::new(),
            ..Host::default()
        };
        let mut without_active = without_active;
        assert_eq!(
            open_history_query(
                &empty,
                &OpenHistoryQueryOptions::default(),
                &mut without_active
            ),
            OpenHistoryOutcome::PendingSet {
                connection_id: "c1".to_owned()
            }
        );

        h.active.push(ActiveConnection {
            connection_id: "c2".to_owned(),
            status: "connected".to_owned(),
            db_session_id: Some("s2".to_owned()),
        });
        let options = OpenHistoryQueryOptions {
            current_connection_id: Some("c2".to_owned()),
            ..OpenHistoryQueryOptions::default()
        };
        let _ = open_history_query(&empty, &options, &mut h);
        assert_eq!(h.panels.borrow()[1].0.connection_id, "c2");

        let entry = HistoryQueryEntry {
            connection_id: Some("c1".to_owned()),
            ..HistoryQueryEntry::default()
        };
        let _ = open_history_query(&entry, &options, &mut h);
        assert_eq!(h.panels.borrow()[2].0.connection_id, "c1");
    }

    #[test]
    fn a_missing_target_or_saved_connection_stops() {
        let mut h = host();
        h.saved.clear();
        h.active.clear();
        let outcome = open_history_query(
            &HistoryQueryEntry::default(),
            &OpenHistoryQueryOptions::default(),
            &mut h,
        );
        assert_eq!(outcome, OpenHistoryOutcome::NoTargetConnection);

        h.saved = vec![saved()];
        let entry = HistoryQueryEntry {
            connection_id: Some("missing".to_owned()),
            ..HistoryQueryEntry::default()
        };
        assert_eq!(
            open_history_query(&entry, &OpenHistoryQueryOptions::default(), &mut h),
            OpenHistoryOutcome::SavedConnectionMissing {
                connection_id: "missing".to_owned()
            }
        );
        assert!(h.panels.borrow().is_empty());
    }

    #[test]
    fn the_connection_selection_callback_is_observable() {
        let mut h = host();
        let options = OpenHistoryQueryOptions {
            select_connection: true,
            ..OpenHistoryQueryOptions::default()
        };
        let _ = open_history_query(&HistoryQueryEntry::default(), &options, &mut h);
        assert_eq!(h.selected.borrow().as_slice(), &["c1".to_owned()]);
    }

    #[test]
    fn gap_and_surface_are_pinned_and_root_reexports_match() {
        assert_eq!(OPEN_HISTORY_QUERY_SURFACE.len(), 8);
        assert!(OPEN_HISTORY_QUERY_SURFACE.contains(&"openHistoryQuery"));
        assert_eq!(OPEN_HISTORY_QUERY_GAP.len(), 6);
        assert_eq!(crate::OPEN_HISTORY_QUERY_SURFACE.len(), 8);
        assert_eq!(crate::OPEN_HISTORY_QUERY_GAP.len(), 6);
        let _: Option<crate::HistoryQueryEntry> = Some(HistoryQueryEntry::default());
    }
}
