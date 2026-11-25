//! Independent audit for T047 (the commands/adb.ts port).
//!
//! adb.ts has no React test of its own (it is a typed wrapper over the
//! driver-command IPC; the driver side tests live in
//! crates/drivers/sqlite). The audit pins the wrapper's contract with a
//! recording adapter: every helper routes through the unified
//! execute_driver_command entry with the exact (driverType, command,
//! input) shapes, the typed rows deserialize, and the pull result's
//! savedPath null-collapse behaves (null when cancelled, absent → null).

use serde_json::json;

use datazen_ui::commands::adb::{
    AdbDatabaseFile, AdbPackage, adb_list_databases, adb_list_packages,
    adb_pull_database_with_dialog,
};
use datazen_ui::state_def::AppState;
use datazen_ui_api::{DesktopAdapter, ConnectionProfile, ColumnSort, ColumnFilter, TablePage};

/// A recording adapter: answers scripted payloads, records every
/// driver-command request.
struct RecordingAdapter {
    requests: Vec<(String, String, serde_json::Value)>,
    response: serde_json::Value,
}

impl DesktopAdapter for RecordingAdapter {
    fn list_connections(&mut self) -> Result<Vec<ConnectionProfile>, String> {
        Ok(Vec::new())
    }
    fn read_table_page(
        &mut self,
        _table: &str,
        _page: u32,
        _page_size: u32,
        _sort: Option<&ColumnSort>,
        _filter: Option<&ColumnFilter>,
    ) -> Result<TablePage, String> {
        Ok(TablePage {
            table: String::new(),
            columns: Vec::new(),
            rows: Vec::new(),
            page: 0,
            page_size: 0,
            total_rows: 0,
            sort: None,
            filter: None,
            primary_key: Vec::new(),
            read_only: None,
        })
    }
    fn commit_cells(
        &mut self,
        _table: &str,
        _edits: &[(u32, String, String)],
    ) -> Result<u32, String> {
        Ok(0)
    }
    fn execute_driver_command(
        &mut self,
        driver_type: &str,
        command: &str,
        input: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        self.requests
            .push((driver_type.to_string(), command.to_string(), input.clone()));
        Ok(self.response.clone())
    }
}

/// Shared recording cell the audit reads after the call (single-threaded
/// rune runtime; Rc/RefCell is the honest sharing primitive).
type Shared = std::rc::Rc<std::cell::RefCell<RecordingAdapter>>;

struct RecordingHandle {
    shared: Shared,
}

impl DesktopAdapter for RecordingHandle {
    fn list_connections(&mut self) -> Result<Vec<ConnectionProfile>, String> {
        Ok(Vec::new())
    }
    fn read_table_page(
        &mut self,
        _table: &str,
        _page: u32,
        _page_size: u32,
        _sort: Option<&ColumnSort>,
        _filter: Option<&ColumnFilter>,
    ) -> Result<TablePage, String> {
        Ok(TablePage {
            table: String::new(),
            columns: Vec::new(),
            rows: Vec::new(),
            page: 0,
            page_size: 0,
            total_rows: 0,
            sort: None,
            filter: None,
            primary_key: Vec::new(),
            read_only: None,
        })
    }
    fn commit_cells(
        &mut self,
        _table: &str,
        _edits: &[(u32, String, String)],
    ) -> Result<u32, String> {
        Ok(0)
    }
    fn execute_driver_command(
        &mut self,
        driver_type: &str,
        command: &str,
        input: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let mut adapter = self.shared.borrow_mut();
        adapter
            .requests
            .push((driver_type.to_string(), command.to_string(), input.clone()));
        Ok(adapter.response.clone())
    }
}

fn state_with(response: serde_json::Value) -> (AppState, Shared) {
    let shared: Shared = std::rc::Rc::new(std::cell::RefCell::new(RecordingAdapter {
        requests: Vec::new(),
        response,
    }));
    let mut state = AppState::default();
    state.desktop = Box::new(RecordingHandle { shared: std::rc::Rc::clone(&shared) });
    (state, shared)
}

#[test]
fn list_packages_routes_the_unified_command() {
    let (mut state, adapter) = state_with(json!([
        { "package_name": "com.example.app" },
        { "package_name": "com.example.other" },
    ]));
    let packages = adb_list_packages(&mut state).expect("typed rows");
    assert_eq!(
        packages,
        vec![
            AdbPackage { package_name: "com.example.app".to_string() },
            AdbPackage { package_name: "com.example.other".to_string() },
        ]
    );
    assert_eq!(adapter.borrow().requests.len(), 1);
    assert_eq!(
        adapter.borrow().requests[0],
        ("sqlite".to_string(), "adb_list_packages".to_string(), json!({}))
    );
}

#[test]
fn list_databases_carries_the_package_input() {
    let (mut state, adapter) = state_with(json!([
        { "path": "/data/0/db", "name": "main.db" },
    ]));
    let databases = adb_list_databases(&mut state, "com.example.app").expect("typed rows");
    assert_eq!(
        databases,
        vec![AdbDatabaseFile {
            path: "/data/0/db".to_string(),
            name: "main.db".to_string(),
        }]
    );
    assert_eq!(
        adapter.borrow().requests[0],
        (
            "sqlite".to_string(),
            "adb_list_databases".to_string(),
            json!({ "package": "com.example.app" })
        )
    );
}

#[test]
fn pull_database_sends_both_inputs_and_unwraps_saved_path() {
    let (mut state, adapter) = state_with(json!({ "savedPath": "/tmp/pulled.db" }));
    let saved = adb_pull_database_with_dialog(&mut state, "com.example.app", "/data/0/db")
        .expect("pull ok");
    assert_eq!(saved, Some("/tmp/pulled.db".to_string()));
    assert_eq!(
        adapter.borrow().requests[0],
        (
            "sqlite".to_string(),
            "adb_pull_database".to_string(),
            json!({ "package": "com.example.app", "dbPath": "/data/0/db" })
        )
    );
}

#[test]
fn pull_database_collapses_cancelled_and_absent_paths_to_none() {
    // Cancelled: the host shell answered null.
    let (mut state, _adapter) = state_with(json!({ "savedPath": null }));
    assert_eq!(
        adb_pull_database_with_dialog(&mut state, "com.example.app", "/db").expect("pull ok"),
        None
    );
    // Absent key: the ?? null normalization.
    let (mut state, _adapter) = state_with(json!({}));
    assert_eq!(
        adb_pull_database_with_dialog(&mut state, "com.example.app", "/db").expect("pull ok"),
        None
    );
}

#[test]
fn adapter_errors_and_shape_mismatches_surface() {
    // The default adapter answers the fixed error (unwired hosts).
    let mut state = AppState::default();
    assert!(adb_list_packages(&mut state).is_err());
    // A payload that does not match the typed rows is an error, not a
    // silent cast (React's `as T` could not fail).
    let (mut state, _adapter) = state_with(json!({ "unexpected": "shape" }));
    assert!(adb_list_packages(&mut state).is_err());
}
