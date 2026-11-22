use crate::{QueryCancelState, QueryExecState};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExecMap(pub Vec<(String, QueryExecState)>);

impl ExecMap {
    pub fn from_entries(entries: Vec<(String, QueryExecState)>) -> Self {
        Self(entries)
    }

    pub fn get(&self, key: &str) -> Option<&QueryExecState> {
        self.0
            .iter()
            .find(|(entry_key, _)| entry_key == key)
            .map(|(_, state)| state)
    }

    pub fn delete(&mut self, key: &str) {
        self.0.retain(|(entry_key, _)| entry_key != key);
    }

    pub fn keys(&self) -> Vec<String> {
        self.0.iter().map(|(key, _)| key.clone()).collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelType {
    Query,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Panel {
    pub id: String,
    pub connection_id: String,
    pub panel_type: PanelType,
}

impl Panel {
    pub fn query(id: impl Into<String>, connection_id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            connection_id: connection_id.into(),
            panel_type: PanelType::Query,
        }
    }

    pub fn other(id: impl Into<String>, connection_id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            connection_id: connection_id.into(),
            panel_type: PanelType::Other,
        }
    }
}

pub trait SessionController {
    fn cancel(&self) -> Result<(), String>;
}

pub trait PaneCancelStore {
    fn cancel_query(&self, panel_id: &str, pane_id: Option<&str>) -> Result<(), String>;
    fn query_exec(&self) -> &ExecMap;
}

pub trait PanelExecDeps {
    fn can_cancel_pane(&self, panel: &Panel) -> bool;
    fn pane_key(&self, panel_id: &str, pane_id: &str) -> String;
    fn pane_keys_of_panel(&self, exec_keys: &[String], panel_id: &str) -> Vec<String>;
    fn registry_keys_of_panel(&self, panel_id: &str) -> Vec<String>;
    fn peek_session(&self, key: &str) -> Option<Box<dyn SessionController>>;
    fn remove_session(&self, key: &str) -> Result<(), String>;
}

pub const PANEL_EXEC_CLEANUP_SURFACE: [&str; 6] = [
    "canCancelPane",
    "requestPaneCancel",
    "sessionKeysOfPanel",
    "cancelAndCleanupPaneExec",
    "cancelAndCleanupExec",
    "PANEL_EXEC_CLEANUP_GAP",
];

pub const PANEL_EXEC_CLEANUP_GAP: [&str; 5] = [
    "useActiveConnectionStore.getState().connections[panel.connectionId]?.capabilities",
    "getCancelCapability(capabilities) === 'supported'",
    "getQueryPanelSessionRegistry().peek(key)?.cancel()",
    "getQueryPanelSessionRegistry().keysOfPanel(panelId)",
    "getQueryPanelSessionRegistry().remove(key)",
];

pub const PANEL_EXEC_CLEANUP_ASYNC_GAP: [&str; 3] = [
    "paneKey / paneKeysOfPanel from stores/paneKeys",
    "void promise.catch(() => {}) on cancel and remove",
    "await getStore().cancelQuery(panelId, paneId)",
];

pub fn can_cancel_pane(deps: &dyn PanelExecDeps, panel: &Panel) -> bool {
    deps.can_cancel_pane(panel)
}

pub fn request_pane_cancel(
    store: &dyn PaneCancelStore,
    deps: &dyn PanelExecDeps,
    panel_id: &str,
    pane_id: &str,
) -> bool {
    let _ = store.cancel_query(panel_id, Some(pane_id));
    let key = deps.pane_key(panel_id, pane_id);
    match store.query_exec().get(&key) {
        Some(exec) => exec.cancel_state == QueryCancelState::Requested || !exec.running,
        None => false,
    }
}

fn cancel_pane_exec(deps: &dyn PanelExecDeps, panel: &Panel, exec: &QueryExecState, key: &str) {
    if !exec.running || exec.execution_id.is_none() {
        return;
    }
    if !deps.can_cancel_pane(panel) {
        return;
    }
    if let Some(controller) = deps.peek_session(key) {
        let _ = controller.cancel();
    }
}

pub fn session_keys_of_panel(
    deps: &dyn PanelExecDeps,
    current_exec: &ExecMap,
    panel_id: &str,
) -> Vec<String> {
    let mut keys = Vec::new();
    for key in deps.pane_keys_of_panel(&current_exec.keys(), panel_id) {
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    for key in deps.registry_keys_of_panel(panel_id) {
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    keys
}

pub fn cancel_and_cleanup_pane_exec(
    deps: &dyn PanelExecDeps,
    panel: &Panel,
    pane_id: &str,
    current_exec: &ExecMap,
) -> ExecMap {
    let key = deps.pane_key(&panel.id, pane_id);
    let exec = current_exec.get(&key).cloned();
    if let Some(exec) = exec.as_ref() {
        cancel_pane_exec(deps, panel, exec, &key);
    }
    let _ = deps.remove_session(&key);
    if exec.is_none() {
        return current_exec.clone();
    }
    let mut next = current_exec.clone();
    next.delete(&key);
    next
}

pub fn cancel_and_cleanup_exec(
    deps: &dyn PanelExecDeps,
    panels_to_remove: &[Panel],
    current_exec: &ExecMap,
) -> ExecMap {
    let mut next = current_exec.clone();
    for panel in panels_to_remove {
        for key in session_keys_of_panel(deps, current_exec, &panel.id) {
            if panel.panel_type == PanelType::Query {
                if let Some(exec) = current_exec.get(&key) {
                    cancel_pane_exec(deps, panel, exec, &key);
                }
            }
            let _ = deps.remove_session(&key);
            next.delete(&key);
        }
    }
    next
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;

    #[derive(Default)]
    struct Log {
        cancels: Vec<String>,
        removes: Vec<String>,
        registry_queries: Vec<String>,
        cancel_queries: Vec<(String, Option<String>)>,
    }

    struct Controller {
        cancels: Rc<RefCell<Log>>,
        key: String,
        fail: bool,
    }

    impl SessionController for Controller {
        fn cancel(&self) -> Result<(), String> {
            self.cancels.borrow_mut().cancels.push(self.key.clone());
            if self.fail {
                Err("runtime refused".to_owned())
            } else {
                Ok(())
            }
        }
    }

    struct Deps {
        log: Rc<RefCell<Log>>,
        can_cancel: bool,
        registry_keys: Vec<String>,
        controllers: Vec<String>,
        fail_cancel: bool,
    }

    impl Deps {
        fn new(can_cancel: bool, registry_keys: Vec<String>) -> Self {
            Self {
                log: Rc::new(RefCell::new(Log::default())),
                can_cancel,
                registry_keys,
                controllers: Vec::new(),
                fail_cancel: false,
            }
        }
    }

    impl PanelExecDeps for Deps {
        fn can_cancel_pane(&self, _panel: &Panel) -> bool {
            self.can_cancel
        }

        fn pane_key(&self, panel_id: &str, pane_id: &str) -> String {
            if pane_id == "main" {
                panel_id.to_owned()
            } else {
                format!("{panel_id}::{pane_id}")
            }
        }

        fn pane_keys_of_panel(&self, exec_keys: &[String], panel_id: &str) -> Vec<String> {
            let prefix = format!("{panel_id}::");
            exec_keys
                .iter()
                .filter(|key| *key == panel_id || key.starts_with(&prefix))
                .cloned()
                .collect()
        }

        fn registry_keys_of_panel(&self, panel_id: &str) -> Vec<String> {
            self.log
                .borrow_mut()
                .registry_queries
                .push(panel_id.to_owned());
            let prefix = format!("{panel_id}::");
            self.registry_keys
                .iter()
                .filter(|key| *key == panel_id || key.starts_with(&prefix))
                .cloned()
                .collect()
        }

        fn peek_session(&self, key: &str) -> Option<Box<dyn SessionController>> {
            if self.controllers.iter().any(|candidate| candidate == key) {
                Some(Box::new(Controller {
                    cancels: Rc::clone(&self.log),
                    key: key.to_owned(),
                    fail: self.fail_cancel,
                }))
            } else {
                None
            }
        }

        fn remove_session(&self, key: &str) -> Result<(), String> {
            self.log.borrow_mut().removes.push(key.to_owned());
            Ok(())
        }
    }

    struct Store {
        log: Rc<RefCell<Log>>,
        exec: ExecMap,
    }

    impl PaneCancelStore for Store {
        fn cancel_query(&self, panel_id: &str, pane_id: Option<&str>) -> Result<(), String> {
            self.log
                .borrow_mut()
                .cancel_queries
                .push((panel_id.to_owned(), pane_id.map(str::to_owned)));
            Ok(())
        }

        fn query_exec(&self) -> &ExecMap {
            &self.exec
        }
    }

    fn exec(key: &str, state: QueryExecState) -> (String, QueryExecState) {
        (key.to_owned(), state)
    }

    #[test]
    fn can_cancel_pane_uses_the_injected_capability_gate() {
        let panel = Panel::query("p", "c");
        let allowed = Deps::new(true, Vec::new());
        assert!(can_cancel_pane(&allowed, &panel));
        let denied = Deps::new(false, Vec::new());
        assert!(!can_cancel_pane(&denied, &panel));
    }

    #[test]
    fn request_pane_cancel_reports_delivery_and_preserves_the_pane_argument() {
        let deps = Deps::new(true, Vec::new());
        let cases = [
            (
                "requested",
                QueryExecState {
                    cancel_state: QueryCancelState::Requested,
                    ..QueryExecState::running("e")
                },
                true,
            ),
            (
                "settled",
                QueryExecState {
                    running: false,
                    cancel_state: QueryCancelState::Idle,
                    execution_id: None,
                    ..QueryExecState::empty()
                },
                true,
            ),
            (
                "failed",
                QueryExecState {
                    cancel_state: QueryCancelState::Failed,
                    ..QueryExecState::running("e")
                },
                false,
            ),
        ];
        for (key, state, expected) in cases {
            let store = Store {
                log: Rc::clone(&deps.log),
                exec: ExecMap::from_entries(vec![exec(&deps.pane_key("p", key), state)]),
            };
            assert_eq!(
                request_pane_cancel(&store, &deps, "p", key),
                expected,
                "{key}"
            );
        }
        let empty = Store {
            log: Rc::clone(&deps.log),
            exec: ExecMap::default(),
        };
        assert!(!request_pane_cancel(&empty, &deps, "p", "missing"));
        let calls = &deps.log.borrow().cancel_queries;
        assert_eq!(calls.len(), 4);
        assert_eq!(calls[0], ("p".to_owned(), Some("requested".to_owned())));
    }

    #[test]
    fn a_pane_cancel_requires_a_running_execution_and_capability() {
        let panel = Panel::query("p", "c");
        let key = "p::1".to_owned();
        let cases = [
            (
                QueryExecState {
                    running: false,
                    execution_id: Some("e".to_owned()),
                    cancel_state: QueryCancelState::Idle,
                    ..QueryExecState::empty()
                },
                true,
                0,
            ),
            (
                QueryExecState {
                    running: true,
                    execution_id: None,
                    cancel_state: QueryCancelState::Idle,
                    ..QueryExecState::empty()
                },
                true,
                0,
            ),
            (QueryExecState::running("e"), false, 0),
            (QueryExecState::running("e"), true, 1),
        ];
        for (state, allowed, expected) in cases {
            let mut deps = Deps::new(allowed, Vec::new());
            deps.controllers = vec![key.clone()];
            let current = ExecMap::from_entries(vec![exec(&key, state)]);
            let _ = cancel_and_cleanup_pane_exec(&deps, &panel, "1", &current);
            assert_eq!(deps.log.borrow().cancels.len(), expected);
        }
    }

    #[test]
    fn a_refused_controller_cancel_is_swallowed() {
        let panel = Panel::query("p", "c");
        let key = "p".to_owned();
        let mut deps = Deps::new(true, Vec::new());
        deps.controllers = vec![key.clone()];
        deps.fail_cancel = true;
        let current = ExecMap::from_entries(vec![exec(&key, QueryExecState::running("e"))]);
        let next = cancel_and_cleanup_pane_exec(&deps, &panel, "main", &current);
        assert_eq!(deps.log.borrow().cancels, vec![key.clone()]);
        assert_eq!(deps.log.borrow().removes, vec![key]);
        assert!(next.get("p").is_none());
    }

    #[test]
    fn session_keys_union_both_sources_without_duplicates() {
        let deps = Deps::new(
            true,
            vec!["p::2".to_owned(), "p::3".to_owned(), "other::1".to_owned()],
        );
        let current = ExecMap::from_entries(vec![
            exec("p", QueryExecState::running("a")),
            exec("p::1", QueryExecState::running("b")),
            exec("p::2", QueryExecState::running("c")),
            exec("other::1", QueryExecState::running("d")),
        ]);
        assert_eq!(
            session_keys_of_panel(&deps, &current, "p"),
            ["p", "p::1", "p::2", "p::3"]
        );
        assert_eq!(deps.log.borrow().registry_queries, vec!["p".to_owned()]);
    }

    #[test]
    fn dropping_one_pane_releases_its_session_even_without_an_exec_row() {
        let panel = Panel::query("p", "c");
        let deps = Deps::new(true, Vec::new());
        let current = ExecMap::from_entries(vec![exec("p::other", QueryExecState::running("e"))]);
        let untouched = cancel_and_cleanup_pane_exec(&deps, &panel, "missing", &current);
        assert_eq!(untouched, current);
        assert_eq!(deps.log.borrow().removes, vec!["p::missing".to_owned()]);
        assert!(deps.log.borrow().cancels.is_empty());
    }

    #[test]
    fn closing_panels_cleans_every_pane_but_cancels_only_query_execs() {
        let mut deps = Deps::new(true, vec!["p::never-executed".to_owned()]);
        deps.controllers = vec!["p".to_owned(), "p::never-executed".to_owned()];
        let current = ExecMap::from_entries(vec![
            exec("p", QueryExecState::running("e1")),
            exec(
                "p::2",
                QueryExecState {
                    running: false,
                    execution_id: None,
                    cancel_state: QueryCancelState::Idle,
                    ..QueryExecState::empty()
                },
            ),
            exec("table", QueryExecState::running("e2")),
        ]);
        let panels = vec![Panel::query("p", "c"), Panel::other("table", "c")];
        let next = cancel_and_cleanup_exec(&deps, &panels, &current);
        assert!(next.0.is_empty());
        assert_eq!(deps.log.borrow().cancels, vec!["p".to_owned()]);
        let mut removes = deps.log.borrow().removes.clone();
        removes.sort();
        assert_eq!(removes, ["p", "p::2", "p::never-executed", "table"]);
    }

    #[test]
    fn gap_and_surface_are_pinned_and_root_reexports_match() {
        assert_eq!(PANEL_EXEC_CLEANUP_SURFACE.len(), 6);
        assert!(PANEL_EXEC_CLEANUP_SURFACE.contains(&"cancelAndCleanupExec"));
        assert_eq!(PANEL_EXEC_CLEANUP_GAP.len(), 5);
        assert_eq!(PANEL_EXEC_CLEANUP_ASYNC_GAP.len(), 3);
        assert_eq!(crate::PANEL_EXEC_CLEANUP_SURFACE.len(), 6);
        assert_eq!(crate::PANEL_EXEC_CLEANUP_GAP.len(), 5);
        let _: Option<Panel> = Some(Panel::query("p", "c"));
        let _: Option<crate::ExecMap> = Some(ExecMap::default());
    }
}
