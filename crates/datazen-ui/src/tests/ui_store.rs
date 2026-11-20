//! T471: the shell layout store — seeds, the persisted sidebar mode's
//! load/toggle/set rules, the dialog slot, and the platform-probed
//! fullscreen sync.

use std::collections::HashMap;

use crate::ui_store::{
    load_workspace_sidebar_mode, DialogId, SidebarModeStorage, UiStore, ConnectionsViewMode,
    WorkspaceSidebarMode, WORKSPACE_SIDEBAR_MODE_KEY,
};

/// A `window.localStorage` double: a plain map, absent keys answer `None`.
#[derive(Default)]
struct MapStorage {
    map: HashMap<String, String>,
}

impl SidebarModeStorage for MapStorage {
    fn get(&self, key: &str) -> Option<String> {
        self.map.get(key).cloned()
    }
    fn set(&mut self, key: &str, value: &str) {
        self.map.insert(key.to_string(), value.to_string());
    }
}

#[test]
fn the_seeds_match_the_react_store() {
    let storage = MapStorage::default();
    let store = UiStore::new(&storage);
    assert_eq!(store.main_sidebar_width, 220);
    assert_eq!(store.connection_sidebar_width, 280);
    assert_eq!(store.editor_height, 320);
    assert_eq!(store.result_height, 360);
    assert_eq!(store.connections_view_mode, ConnectionsViewMode::Grid);
    assert_eq!(store.active_dialog, None);
    assert!(!store.is_fullscreen);
    assert_eq!(store.workspace_sidebar_mode, WorkspaceSidebarMode::Icons);
}

#[test]
fn the_load_rule_spells_expanded_exactly() {
    assert_eq!(
        load_workspace_sidebar_mode(Some("expanded")),
        WorkspaceSidebarMode::Expanded
    );
    // Everything else — including absent (the SSR guard) — is 'icons'.
    assert_eq!(
        load_workspace_sidebar_mode(Some("icons")),
        WorkspaceSidebarMode::Icons
    );
    assert_eq!(
        load_workspace_sidebar_mode(Some("Expanded")),
        WorkspaceSidebarMode::Icons,
        "spelling is exact, case included"
    );
    assert_eq!(
        load_workspace_sidebar_mode(Some("")),
        WorkspaceSidebarMode::Icons
    );
    assert_eq!(
        load_workspace_sidebar_mode(None),
        WorkspaceSidebarMode::Icons
    );
}

#[test]
fn the_load_seed_reads_the_storage_once() {
    let mut storage = MapStorage::default();
    storage.map.insert(
        WORKSPACE_SIDEBAR_MODE_KEY.to_string(),
        "expanded".to_string(),
    );
    let store = UiStore::new(&storage);
    assert_eq!(store.workspace_sidebar_mode, WorkspaceSidebarMode::Expanded);
}

#[test]
fn the_size_setters_write_their_own_cell_only() {
    let storage = MapStorage::default();
    let mut store = UiStore::new(&storage);
    store.set_main_sidebar_width(300);
    store.set_connection_sidebar_width(340);
    store.set_editor_height(400);
    store.set_result_height(480);
    assert_eq!(store.main_sidebar_width, 300);
    assert_eq!(store.connection_sidebar_width, 340);
    assert_eq!(store.editor_height, 400);
    assert_eq!(store.result_height, 480);

    store.set_result_height(500);
    assert_eq!(store.editor_height, 400, "the editor height is untouched");
}

#[test]
fn the_dialog_slot_round_trips() {
    let storage = MapStorage::default();
    let mut store = UiStore::new(&storage);
    store.open_dialog(DialogId::NewConnection);
    assert_eq!(store.active_dialog, Some(DialogId::NewConnection));
    store.close_dialog();
    assert_eq!(store.active_dialog, None);
}

#[test]
fn the_fullscreen_sync_sets_only_on_a_platform_answer() {
    let storage = MapStorage::default();
    let mut store = UiStore::new(&storage);

    // A platform answer sets and returns.
    assert!(store.sync_fullscreen(|| Some(true)));
    assert!(store.is_fullscreen);
    assert!(!store.sync_fullscreen(|| Some(false)));
    assert!(!store.is_fullscreen);

    // No platform (or a thrown call) returns false and leaves the flag.
    store.set_fullscreen(true);
    assert!(!store.sync_fullscreen(|| None));
    assert!(store.is_fullscreen, "the stored flag is untouched");
}

#[test]
fn the_mode_toggle_persists_the_new_mode() {
    let mut storage = MapStorage::default();
    let mut store = UiStore::new(&storage);
    store.toggle_workspace_sidebar_mode(&mut storage);
    assert_eq!(store.workspace_sidebar_mode, WorkspaceSidebarMode::Expanded);
    assert_eq!(
        storage.map.get(WORKSPACE_SIDEBAR_MODE_KEY).map(String::as_str),
        Some("expanded")
    );
    store.toggle_workspace_sidebar_mode(&mut storage);
    assert_eq!(store.workspace_sidebar_mode, WorkspaceSidebarMode::Icons);
    assert_eq!(
        storage.map.get(WORKSPACE_SIDEBAR_MODE_KEY).map(String::as_str),
        Some("icons")
    );
}

#[test]
fn the_mode_set_persists_and_survives_a_restart() {
    let mut storage = MapStorage::default();
    let mut store = UiStore::new(&storage);
    store.set_workspace_sidebar_mode(WorkspaceSidebarMode::Expanded, &mut storage);
    assert_eq!(storage.map.get(WORKSPACE_SIDEBAR_MODE_KEY).map(String::as_str), Some("expanded"));

    // A fresh store built from the same storage reloads the mode.
    let reopened = UiStore::new(&storage);
    assert_eq!(reopened.workspace_sidebar_mode, WorkspaceSidebarMode::Expanded);
    // And its other seeds are untouched by the persisted mode.
    assert_eq!(reopened.main_sidebar_width, 220);
}
