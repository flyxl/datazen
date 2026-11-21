use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use serde_json::json;

use super::*;
use crate::{FavoriteQuery, QueryHistoryEntry};

#[derive(Default)]
struct RecordingHost {
    confirm_answer: bool,
    can_cancel: bool,
    cancel_ok: bool,
    sessions: RefCell<BTreeSet<String>>,
    session_keys_by_panel: BTreeMap<String, Vec<String>>,
    remove_fail: BTreeSet<String>,
    blocked: RefCell<Vec<String>>,
    cancels: RefCell<Vec<(String, String)>>,
    queries: RefCell<Vec<RunQueryRequest>>,
    history: Vec<QueryHistoryEntry>,
    favorites: Vec<FavoriteQuery>,
    favorites_root: Option<String>,
    added: RefCell<Vec<(String, String, String)>>,
    deleted: RefCell<Vec<String>>,
    commits: RefCell<Vec<CommitCellRequest>>,
    commit_error: Option<String>,
}

impl PanelStoreHost for RecordingHost {
    fn confirm_close(&self, _panel_id: &str, _pane_ids: &[String]) -> bool {
        self.confirm_answer
    }

    fn has_session(&self, key: &str) -> bool {
        self.sessions.borrow().contains(key)
    }

    fn session_keys_of_panel(&self, panel_id: &str) -> Vec<String> {
        self.session_keys_by_panel
            .get(panel_id)
            .cloned()
            .unwrap_or_default()
    }

    fn remove_session(&self, key: &str) -> Result<(), String> {
        if self.remove_fail.contains(key) {
            Err("busy".to_owned())
        } else {
            self.sessions.borrow_mut().remove(key);
            Ok(())
        }
    }

    fn mark_blocked(&self, key: &str) {
        self.blocked.borrow_mut().push(key.to_owned());
    }

    fn can_cancel(&self, _panel_id: &str) -> bool {
        self.can_cancel
    }

    fn request_cancel(&self, panel_id: &str, pane_id: &str) -> bool {
        self.cancels
            .borrow_mut()
            .push((panel_id.to_owned(), pane_id.to_owned()));
        self.cancel_ok
    }

    fn close_blocked_message(&self) -> String {
        "close blocked".to_owned()
    }

    fn cancel_failed_message(&self) -> String {
        "cancel failed".to_owned()
    }

    fn run_query(&self, request: &RunQueryRequest) {
        self.queries.borrow_mut().push(request.clone());
    }

    fn load_history(&self, _connection_id: Option<&str>) -> Vec<QueryHistoryEntry> {
        self.history.clone()
    }

    fn load_favorites(&self, _connection_id: Option<&str>) -> Vec<FavoriteQuery> {
        self.favorites.clone()
    }

    fn refresh_favorites(
        &self,
        connection_id: Option<&str>,
    ) -> (Vec<FavoriteQuery>, Option<String>) {
        let _ = connection_id;
        (self.favorites.clone(), self.favorites_root.clone())
    }

    fn add_favorite(&self, connection_id: &str, title: &str, sql: &str) {
        self.added
            .borrow_mut()
            .push((connection_id.to_owned(), title.to_owned(), sql.to_owned()));
    }

    fn delete_favorite(&self, id: &str) {
        self.deleted.borrow_mut().push(id.to_owned());
    }

    fn commit_cell(&self, request: &CommitCellRequest) -> Result<(), String> {
        self.commits.borrow_mut().push(request.clone());
        match &self.commit_error {
            Some(error) => Err(error.clone()),
            None => Ok(()),
        }
    }
}

fn host() -> RecordingHost {
    RecordingHost {
        confirm_answer: true,
        can_cancel: true,
        cancel_ok: true,
        ..RecordingHost::default()
    }
}

fn writable_result() -> ResultView {
    ResultView {
        columns: vec![ResultColumn {
            name: "name".to_owned(),
        }],
        rows: vec![vec![json!("old")]],
        pinned: false,
        runtime_binding: Some("binding-1".to_owned()),
        statement_index: Some(0),
        writable_mapping_verified: true,
    }
}

fn running_exec() -> PanelExecState {
    let mut exec = PanelExecState::empty();
    exec.sql = " SELECT 1 ".to_owned();
    exec.running = true;
    exec.execution_id = Some("exec-1".to_owned());
    exec
}

fn add_query(store: &mut PanelStore, id: &str, connection: &str) {
    store.add_panel(StorePanel::query(id, connection), true, None);
}

#[test]
fn pane_key_space_and_navigation_helpers_are_pinned() {
    assert_eq!(pane_key("p", None), "p");
    assert_eq!(pane_key("p", Some("main")), "p");
    assert_eq!(pane_key("p", Some("side")), "p::side");
    assert_eq!(panel_id_of_pane_key("p::side"), "p");
    assert_eq!(pane_id_of_pane_key("p::side"), "side");
    assert_eq!(pane_id_of_pane_key("p"), "main");
    assert!(is_pane_key_of_panel("p::side", "p"));
    assert!(!is_pane_key_of_panel("other::side", "p"));
    assert_eq!(pane_args(None), Vec::<&str>::new());
    assert_eq!(pane_args(Some("main")), Vec::<&str>::new());
    assert_eq!(pane_args(Some("side")), vec!["side"]);
    assert_eq!(resolve_focused_pane_id(None), "main");
    let panels = vec![StorePanel::query("a", "c"), StorePanel::query("b", "c")];
    assert_eq!(
        resolve_next_active(&panels, "a", Some("a")).as_deref(),
        Some("b")
    );
    assert_eq!(
        resolve_next_active(&panels, "a", Some("b")).as_deref(),
        Some("b")
    );
    assert_eq!(
        resolve_next_active(&panels, "b", Some("b")).as_deref(),
        Some("a")
    );
    assert_eq!(resolve_next_active(&panels, "b", None), None);
}

#[test]
fn adding_a_query_panel_seeds_only_query_exec_and_its_own_focus() {
    let mut store = PanelStore::new();
    add_query(&mut store, "q1", "c1");
    store.add_panel(StorePanel::table("t1", "c1", "users"), true, None);
    assert_eq!(store.state.panels.len(), 2);
    assert_eq!(store.state.query_exec.len(), 1);
    assert!(store.state.query_exec.contains_key("q1"));
    assert_eq!(store.state.active_panel_id.as_deref(), Some("t1"));
    assert_eq!(store.state.focused_pane_id, None);

    add_query(&mut store, "q2", "c1");
    assert_eq!(store.state.active_panel_id.as_deref(), Some("q2"));
}

#[test]
fn removing_a_panel_honours_the_close_guard_and_resolves_active() {
    let mut h = host();
    let mut store = PanelStore::new();
    add_query(&mut store, "a", "c");
    add_query(&mut store, "b", "c");
    store.set_active_panel(Some("b"));
    h.confirm_answer = false;
    assert!(store.remove_panel(&h, "b").is_ok());
    assert_eq!(store.state.panels.len(), 2);

    h.confirm_answer = true;
    let _ = store.remove_panel(&h, "b");
    assert_eq!(store.state.panels.len(), 1);
    assert_eq!(store.state.active_panel_id.as_deref(), Some("a"));
}

#[test]
fn a_blocked_session_release_keeps_the_tab_and_marks_the_pane() {
    let mut h = host();
    h.sessions.borrow_mut().insert("q1".to_owned());
    h.remove_fail.insert("q1".to_owned());
    let mut store = PanelStore::new();
    add_query(&mut store, "q1", "c");
    let _ = store.remove_panel(&h, "q1");
    assert_eq!(store.state.panels.len(), 1);
    assert_eq!(h.blocked.borrow().as_slice(), &["q1".to_owned()]);
}

#[test]
fn relation_and_database_filters_use_the_documented_fallbacks() {
    let mut panel = StorePanel::table("t", "c", "users");
    panel.database = Some(" app ".to_owned());
    assert!(matches_relation(&panel, "c", "users", Some("app")));
    assert!(!matches_relation(&panel, "other", "users", Some("app")));
    assert!(!matches_relation(&panel, "c", "orders", Some("app")));
    assert!(
        matches_database(&panel, "c", "app", None),
        "both sides are trimmed before comparison"
    );
    panel.database = Some("app".to_owned());
    assert!(matches_database(&panel, "c", "app", None));
    let mut session_panel = StorePanel::table("t2", "c", "users");
    assert!(matches_database(&session_panel, "c", "app", Some("app")));
    assert!(!matches_database(&session_panel, "c", "app", Some("other")));
    session_panel.panel_type = StorePanelType::Query;
    assert!(!matches_database(&session_panel, "c", "app", Some("app")));
}

#[test]
fn open_pane_focuses_it_and_close_pane_only_clears_its_own_tab() {
    let mut h = host();
    let mut store = PanelStore::new();
    add_query(&mut store, "q1", "c");
    store.open_pane("q1", "side");
    assert!(store.state.query_exec.contains_key("q1::side"));
    assert_eq!(store.state.focused_pane_id.as_deref(), Some("side"));

    let _ = store.close_pane(&h, "q1", "side");
    assert!(!store.state.query_exec.contains_key("q1::side"));
    assert_eq!(store.state.focused_pane_id, None);

    add_query(&mut store, "q2", "c");
    store.set_focused_pane(Some("side"), Some("q1"));
    store.set_active_panel(Some("q2"));
    let _ = store.close_pane(&h, "q1", "side");
    assert_eq!(store.state.active_panel_id.as_deref(), Some("q2"));
}

#[test]
fn execute_query_routes_the_trimmed_sql_and_targets() {
    let mut h = host();
    let mut store = PanelStore::new();
    add_query(&mut store, "q1", "conn");
    store.update_sql("q1", "  SELECT 1  ".to_owned(), None);
    assert!(store.execute_query(&h, "q1", None, None));
    let queries = h.queries.borrow();
    assert_eq!(queries.len(), 1);
    assert_eq!(queries[0].sql, "SELECT 1");
    assert_eq!(queries[0].key, "q1");
    assert_eq!(queries[0].connection_id, "conn");
    assert!(queries[0].params.is_none());
    drop(queries);

    store.update_sql("q1", "   ".to_owned(), None);
    assert!(!store.execute_query(&h, "q1", None, None));
    assert_eq!(h.queries.borrow().len(), 1);
}

#[test]
fn bound_params_switch_the_query_route() {
    let mut h = host();
    let mut store = PanelStore::new();
    add_query(&mut store, "q1", "conn");
    store.update_sql("q1", "SELECT ?".to_owned(), None);
    let params = BTreeMap::from([("id".to_owned(), json!(3))]);
    assert!(store.execute_query(&h, "q1", Some(params.clone()), None));
    assert_eq!(h.queries.borrow()[0].params, Some(params));
}

#[test]
fn cancel_query_checks_running_state_and_reports_failure() {
    let mut h = host();
    let mut store = PanelStore::new();
    add_query(&mut store, "q1", "conn");
    assert!(!store.cancel_query(&h, "q1", None));

    set_exec(&mut store, "q1", running_exec());
    h.cancel_ok = false;
    assert!(!store.cancel_query(&h, "q1", None));
    let exec = exec_of(store.state.query_exec.get("q1"));
    assert_eq!(exec.cancel_state, crate::QueryCancelState::Failed);
    assert_eq!(exec.error.as_deref(), Some("cancel failed"));

    set_exec(&mut store, "q1", running_exec());
    h.cancel_ok = true;
    assert!(store.cancel_query(&h, "q1", None));
    assert_eq!(h.cancels.borrow().len(), 2);
}

fn exec_of(value: Option<&PanelExecState>) -> PanelExecState {
    match value {
        Some(state) => state.clone(),
        None => panic!("expected exec state"),
    }
}

fn exec_mut<'a>(store: &'a mut PanelStore, key: &str) -> &'a mut PanelExecState {
    store
        .state
        .query_exec
        .get_mut(key)
        .unwrap_or_else(|| panic!("expected exec state"))
}

fn set_exec(store: &mut PanelStore, key: &str, state: PanelExecState) {
    match store.state.query_exec.get_mut(key) {
        Some(entry) => *entry = state,
        None => panic!("expected exec state"),
    }
}

#[test]
fn result_view_actions_patch_only_the_target_pane() {
    let mut store = PanelStore::new();
    add_query(&mut store, "q1", "conn");
    exec_mut(&mut store, "q1").results.push(writable_result());
    store.set_active_result("q1", 0, None);
    assert_eq!(
        exec_of(store.state.query_exec.get("q1")).active_result_idx,
        0
    );
    assert!(store.toggle_pin_result("q1", 0, None));
    assert!(exec_of(store.state.query_exec.get("q1")).results[0].pinned);
    assert!(!store.toggle_pin_result("q1", 5, None));
    store.set_result_detail_row("q1", Some(3), None);
    assert_eq!(
        exec_of(store.state.query_exec.get("q1")).result_detail_row_index,
        Some(3)
    );
    store.set_result_view_mode("q1", "chart", None);
    assert_eq!(
        exec_of(store.state.query_exec.get("q1"))
            .result_view_mode
            .as_deref(),
        Some("chart")
    );
}

#[test]
fn result_edit_requires_a_writable_binding_and_skips_unchanged_cells() {
    let mut h = host();
    let mut store = PanelStore::new();
    add_query(&mut store, "q1", "conn");
    exec_mut(&mut store, "q1").results.push(writable_result());

    let same = vec![ResultEdit {
        row: 0,
        col: "name".to_owned(),
        value: json!("old"),
    }];
    assert!(store.update_result_cells(&h, "q1", 0, same, None).is_ok());
    assert!(h.commits.borrow().is_empty());

    let changed = vec![ResultEdit {
        row: 0,
        col: "name".to_owned(),
        value: json!("new"),
    }];
    assert!(store
        .update_result_cells(&h, "q1", 0, changed, None)
        .is_ok());
    assert_eq!(h.commits.borrow().len(), 1);
    assert_eq!(h.commits.borrow()[0].value, json!("new"));
    assert_eq!(
        exec_of(store.state.query_exec.get("q1")).results[0].rows[0][0],
        json!("new")
    );
    assert!(store.in_flight.is_empty());
}

#[test]
fn a_failed_commit_records_the_error_and_releases_inflight() {
    let mut h = host();
    h.commit_error = Some("denied".to_owned());
    let mut store = PanelStore::new();
    add_query(&mut store, "q1", "conn");
    exec_mut(&mut store, "q1").results.push(writable_result());
    let edits = vec![ResultEdit {
        row: 0,
        col: "name".to_owned(),
        value: json!("new"),
    }];
    assert_eq!(
        store
            .update_result_cells(&h, "q1", 0, edits, None)
            .err()
            .map(|error| error),
        Some("denied".to_owned())
    );
    assert_eq!(
        exec_of(store.state.query_exec.get("q1")).error.as_deref(),
        Some("denied")
    );
    assert!(store.in_flight.is_empty());
}

#[test]
fn read_only_and_inflight_batches_are_refused() {
    let mut h = host();
    let mut store = PanelStore::new();
    add_query(&mut store, "q1", "conn");
    let mut read_only = writable_result();
    read_only.runtime_binding = None;
    exec_mut(&mut store, "q1").results.push(read_only);
    let edits = vec![ResultEdit {
        row: 0,
        col: "name".to_owned(),
        value: json!("x"),
    }];
    assert!(store.update_result_cells(&h, "q1", 0, edits, None).is_err());

    exec_mut(&mut store, "q1").results = vec![writable_result()];
    store.in_flight.insert("q1".to_owned());
    let edits = vec![ResultEdit {
        row: 0,
        col: "name".to_owned(),
        value: json!("x"),
    }];
    assert_eq!(
        store.update_result_cells(&h, "q1", 0, edits, None).err(),
        Some("A result edit is already in progress".to_owned())
    );
    assert!(h.commits.borrow().is_empty());
}

#[test]
fn history_and_favorite_actions_are_observable() {
    let mut h = host();
    h.history = vec![QueryHistoryEntry {
        id: "h".to_owned(),
        connection_id: "conn".to_owned(),
        database: "app".to_owned(),
        schema: None,
        sql: "SELECT 1".to_owned(),
        executed_at: "now".to_owned(),
        execution_time_ms: 1,
        rows_affected: None,
        success: true,
        error_message: None,
    }];
    h.favorites_root = Some("/home/.sql".to_owned());
    let mut store = PanelStore::new();
    store.open_query_history(&h, Some("conn"));
    assert!(store.state.history_visible);
    assert!(!store.state.favorites_visible);
    assert_eq!(store.state.query_history.len(), 1);
    store.toggle_history();
    assert!(!store.state.history_visible);

    store.refresh_favorites(&h, Some("conn"));
    assert_eq!(store.state.favorites_root.as_deref(), Some("/home/.sql"));
    store.add_favorite(&h, "T", "SELECT 1", "conn");
    assert_eq!(h.added.borrow()[0].1, "T");
    store.delete_favorite(&h, "fav");
    assert_eq!(h.deleted.borrow().as_slice(), &["fav".to_owned()]);
    store.toggle_favorites();
    assert!(store.state.favorites_visible);
    store.set_pending_query_history(Some("conn".to_owned()));
    assert_eq!(
        store.state.pending_query_history_connection_id.as_deref(),
        Some("conn")
    );
}

#[test]
fn reset_restores_every_slice_and_the_id_counter() {
    let mut h = host();
    let mut store = PanelStore::new();
    add_query(&mut store, "q1", "conn");
    let minted = store.next_panel_id("x");
    assert_eq!(minted, "panel-x-1");
    store.reset();
    assert!(store.state.panels.is_empty());
    assert!(store.state.query_exec.is_empty());
    assert_eq!(store.next_panel_id("x"), "panel-x-1");
    let _ = &h;
}

#[test]
fn gap_and_surface_are_pinned_and_root_reexports_match() {
    assert_eq!(PANEL_STORE_SURFACE.len(), 10);
    assert!(PANEL_STORE_SURFACE.contains(&"executeQuery"));
    assert_eq!(PANEL_STORE_GAP.len(), 8);
    assert_eq!(crate::PANEL_STORE_SURFACE.len(), 10);
    assert_eq!(crate::PANEL_STORE_GAP.len(), 8);
    let _: Option<crate::PanelState> = Some(PanelState::default());
}
