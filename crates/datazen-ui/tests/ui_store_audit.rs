//! T471 audit (tester): independent checks of the uiStore port. The
//! coder's tests exercise each transition once; this audit drives the store
//! through longer sequences — toggle storms, a restart round-trip with a
//! stale non-matching stored value, interleaved fullscreen syncs and
//! dialog flips — from a separately-written storage double, so a wrong
//! field or a lost persistence write cannot cancel out.

use std::collections::HashMap;

use datazen_ui::ui_store::{
    load_workspace_sidebar_mode, DialogId, SidebarModeStorage, UiStore, ConnectionsViewMode,
    WorkspaceSidebarMode,
};

/// Independent storage double: unlike the coder's, it also records the
/// write ORDER (each set pushes "key=value" onto a log) so the audit can
/// observe the exact persistence sequence, not just the final value.
#[derive(Default)]
struct AuditStorage {
    values: HashMap<String, String>,
    writes: Vec<String>,
}

impl SidebarModeStorage for AuditStorage {
    fn get(&self, key: &str) -> Option<String> {
        self.values.get(key).cloned()
    }
    fn set(&mut self, key: &str, value: &str) {
        self.values.insert(key.to_string(), value.to_string());
        self.writes.push(format!("{key}={value}"));
    }
}

#[test]
fn audit_a_toggle_storm_persists_every_step_in_order() {
    let mut storage = AuditStorage::default();
    let mut store = UiStore::new(&storage);
    // Six toggles: icons -> expanded -> icons -> expanded -> icons -> expanded.
    for expected in [
        WorkspaceSidebarMode::Expanded,
        WorkspaceSidebarMode::Icons,
        WorkspaceSidebarMode::Expanded,
        WorkspaceSidebarMode::Icons,
        WorkspaceSidebarMode::Expanded,
    ] {
        store.toggle_workspace_sidebar_mode(&mut storage);
        assert_eq!(store.workspace_sidebar_mode, expected);
    }
    assert_eq!(
        storage.writes,
        vec![
            "datazen:workspace-sidebar-mode=expanded",
            "datazen:workspace-sidebar-mode=icons",
            "datazen:workspace-sidebar-mode=expanded",
            "datazen:workspace-sidebar-mode=icons",
            "datazen:workspace-sidebar-mode=expanded",
        ],
        "each toggle persists exactly once, in call order"
    );
}

#[test]
fn audit_a_stale_stored_value_degrades_to_icons_across_a_restart() {
    // localStorage may hold anything (older builds, manual edits); only
    // the exact 'expanded' spelling survives.
    let mut storage = AuditStorage::default();
    storage
        .values
        .insert("datazen:workspace-sidebar-mode".to_string(), "compact".to_string());
    let store = UiStore::new(&storage);
    assert_eq!(store.workspace_sidebar_mode, WorkspaceSidebarMode::Icons);
    // The stale value is NOT overwritten by a plain read.
    assert_eq!(
        storage.values.get("datazen:workspace-sidebar-mode").map(String::as_str),
        Some("compact")
    );
    assert!(storage.writes.is_empty(), "the load path never writes");

    // The load rule agrees with the direct function on the same input.
    assert_eq!(
        load_workspace_sidebar_mode(Some("compact")),
        store.workspace_sidebar_mode
    );
}

#[test]
fn audit_fullscreen_syncs_interleave_with_explicit_sets() {
    let storage = AuditStorage::default();
    let mut store = UiStore::new(&storage);
    store.set_fullscreen(true);
    // The platform says otherwise: the sync overwrites the explicit set.
    assert!(!store.sync_fullscreen(|| Some(false)));
    assert!(!store.is_fullscreen);
    // A failed probe after an explicit set wins nothing.
    store.set_fullscreen(true);
    assert!(!store.sync_fullscreen(|| None));
    assert!(store.is_fullscreen);
    // And the platform answer still wins over a stale true.
    assert!(!store.sync_fullscreen(|| Some(false)));
    assert!(!store.is_fullscreen);
}

#[test]
fn audit_dialog_and_view_mode_are_independent_cells() {
    let storage = AuditStorage::default();
    let mut store = UiStore::new(&storage);
    store.open_dialog(DialogId::NewConnection);
    store.set_connections_view_mode(ConnectionsViewMode::List);
    store.set_fullscreen(true);
    assert_eq!(store.active_dialog, Some(DialogId::NewConnection));
    assert_eq!(store.connections_view_mode, ConnectionsViewMode::List);
    assert!(store.is_fullscreen);
    // Closing the dialog disturbs nothing else.
    store.close_dialog();
    assert_eq!(store.active_dialog, None);
    assert_eq!(store.connections_view_mode, ConnectionsViewMode::List);
    assert!(store.is_fullscreen);
    // Persistence stayed untouched by all of it.
    assert!(storage.writes.is_empty());
}

#[test]
fn audit_the_persisted_mode_round_trips_through_a_fresh_storage_handoff() {
    // Write with one store, then hand the same storage to a second store —
    // the localStorage restart the React module does on load.
    let mut storage = AuditStorage::default();
    let mut first = UiStore::new(&storage);
    first.set_workspace_sidebar_mode(WorkspaceSidebarMode::Expanded, &mut storage);
    let mut second = UiStore::new(&storage);
    assert_eq!(second.workspace_sidebar_mode, WorkspaceSidebarMode::Expanded);
    second.set_workspace_sidebar_mode(WorkspaceSidebarMode::Icons, &mut storage);
    let third = UiStore::new(&storage);
    assert_eq!(third.workspace_sidebar_mode, WorkspaceSidebarMode::Icons);
    assert_eq!(storage.writes.len(), 2, "exactly one write per explicit set");
}
