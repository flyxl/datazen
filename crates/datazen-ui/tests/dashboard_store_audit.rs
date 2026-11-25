//! T184 independent audit: the `stores/dashboardStore.ts` port
//! (`dashboard_store.rs`), written against the React source, not against
//! the coder's inline tests.
//!
//! What is pinned here, from the React behaviour:
//!
//! * the refcount lifecycle: two mounts stack, a middle release keeps the
//!   entry, the LAST release drops it (with its runs — a remount starts
//!   from the empty entry), and an unknown release is inert.
//! * the fetch walk: a failed refetch does not clear the old list.
//! * the load walk: the entry materialises even for an id nobody
//!   mounted, and the error path leaves `dashboard` and `runs` exactly as
//!   they were (only `loading` and `error` move).
//! * the save upsert: the list member is replaced in place (no append,
//!   no reorder), an unknown id appends at the end, and save never
//!   materialises an entry for an unmounted dashboard.
//! * the refresh walk: the error exit writes no run slot, and a begin on
//!   an absent dashboard id materialises its busy entry.
//! * `pick_latest_run`: the first `ok`, else the first, else nothing.
//! * `refresh_all_targets`: only the enabled widgets, in layout order.
//! * both cross-window gates: full fields + live entry; a half-released
//!   entry is still live; a fully released one is not.

use datazen_ui::dashboard_store::{
    changed_gate, pick_latest_run, refresh_all_targets, run_updated_gate, DashboardStore,
};
use datazen_ui::views::dashboard::chart_widget_tile::{
    TileViewMode, WidgetRunStatus, WidgetRunVm,
};
use datazen_ui::views::dashboard::dashboard_panel::{
    DashboardVm, PanelWidgetVm, RefreshPolicyVm, WidgetLayoutVm,
};

fn board(id: &str, widgets: Vec<PanelWidgetVm>) -> DashboardVm {
    DashboardVm {
        id: id.to_owned(),
        name: format!("board-{id}"),
        created_at: String::new(),
        updated_at: String::new(),
        layout_cols: 12,
        layout_row_height: 4,
        widgets,
        enabled: true,
        refresh_paused: false,
    }
}

fn widget(id: &str, enabled: bool) -> PanelWidgetVm {
    PanelWidgetVm {
        id: id.to_owned(),
        title: id.to_owned(),
        workflow_id: String::new(),
        view_mode: TileViewMode::Chart,
        chart_config: None,
        layout: WidgetLayoutVm {
            x: 0,
            y: 0,
            w: 6,
            h: 4,
        },
        refresh: RefreshPolicyVm {
            mode: Default::default(),
            refresh_sec: None,
        },
        enabled,
    }
}

fn run(row_count: u64) -> WidgetRunVm {
    WidgetRunVm {
        status: WidgetRunStatus::Ok,
        error: None,
        row_count,
        columns: Vec::new(),
        rows: Vec::new(),
        alert_fired: false,
    }
}

#[test]
fn the_last_release_drops_the_runs_too() {
    let mut store = DashboardStore::default();
    store.mount_dashboard("d");
    let mut runs = std::collections::HashMap::new();
    runs.insert("w".to_owned(), Some(run(1)));
    store.apply_load_ok("d", board("d", vec![widget("w", true)]), runs);
    assert!(store.entry("d").unwrap().runs.contains_key("w"));

    store.release_dashboard("d");
    assert!(
        store.entry("d").is_none(),
        "the last release drops the entry and its runs with it"
    );

    // A remount starts from the empty entry — no stale runs.
    store.mount_dashboard("d");
    let entry = store.entry("d").unwrap();
    assert!(entry.runs.is_empty());
    assert!(entry.dashboard.is_none());
    assert_eq!(entry.ref_count, 1);
}

#[test]
fn a_failed_refetch_keeps_the_old_list() {
    let mut store = DashboardStore::default();
    store.begin_fetch();
    store.apply_fetch_ok(vec![board("a", vec![]), board("b", vec![])]);
    store.begin_fetch();
    store.apply_fetch_err("HTTP 500");
    assert_eq!(store.list.len(), 2, "the old list survives");
    assert_eq!(store.list_error.as_deref(), Some("HTTP 500"));
    assert!(!store.list_loading);
}

#[test]
fn the_load_error_path_leaves_the_loaded_state_alone() {
    let mut store = DashboardStore::default();
    store.mount_dashboard("d");
    let mut runs = std::collections::HashMap::new();
    runs.insert("w".to_owned(), Some(run(2)));
    store.apply_load_ok("d", board("d", vec![widget("w", true)]), runs);

    // A later load that fails: only loading and error move.
    store.begin_load("d");
    store.apply_load_err("d", "boom");
    let entry = store.entry("d").unwrap();
    assert!(!entry.loading);
    assert_eq!(entry.error.as_deref(), Some("boom"));
    assert!(entry.dashboard.is_some(), "the dashboard survives");
    assert_eq!(
        entry.runs["w"].as_ref().unwrap().row_count,
        2,
        "the runs survive"
    );
    assert_eq!(entry.ref_count, 1, "the refcount is untouched");
}

#[test]
fn the_list_upsert_replaces_in_place() {
    let mut store = DashboardStore::default();
    store.begin_fetch();
    store.apply_fetch_ok(vec![board("a", vec![]), board("b", vec![])]);

    // Replace member 'a': the order stays a, b — no append.
    let mut replacement = board("a", vec![]);
    replacement.name = "A2".to_owned();
    store.apply_save(replacement);
    assert_eq!(
        store.list.iter().map(|d| d.id.as_str()).collect::<Vec<_>>(),
        vec!["a", "b"]
    );
    assert_eq!(store.list[0].name, "A2");

    // An unknown id appends at the END, not the front.
    store.apply_save(board("c", vec![]));
    assert_eq!(
        store.list.iter().map(|d| d.id.as_str()).collect::<Vec<_>>(),
        vec!["a", "b", "c"]
    );
}

#[test]
fn save_does_not_materialise_an_unmounted_entry() {
    let mut store = DashboardStore::default();
    store.apply_save(board("fresh", vec![]));
    assert!(
        store.entry("fresh").is_none(),
        "React's hasEntry guard: no entry, no patch"
    );
    assert_eq!(store.list.len(), 1, "the list still upserts");
}

#[test]
fn refresh_error_writes_no_run_and_keeps_the_old_one() {
    let mut store = DashboardStore::default();
    store.mount_dashboard("d");
    store.set_run("d", "w", Some(run(9)));

    store.begin_refresh("d", "w");
    store.apply_refresh_err("d", "w", "widget error");
    let entry = store.entry("d").unwrap();
    assert!(!entry.busy_widgets.contains("w"));
    assert_eq!(
        entry.runs["w"].as_ref().unwrap().row_count,
        9,
        "the previous run survives the failed refresh"
    );
    assert_eq!(entry.error.as_deref(), Some("widget error"));
}

#[test]
fn a_refresh_begin_on_an_absent_id_materialises_busy_state() {
    let mut store = DashboardStore::default();
    store.begin_refresh("ghost", "w");
    let entry = store.entry("ghost").unwrap();
    assert!(entry.busy_widgets.contains("w"));
    assert_eq!(entry.ref_count, 0, "materialised, not mounted");
}

#[test]
fn pick_latest_run_matches_the_react_order() {
    // `index.find((e) => e.status === 'ok') ?? index[0]`
    assert_eq!(pick_latest_run(&["timeout", "error"]), Some(0));
    assert_eq!(pick_latest_run(&["error", "timeout", "ok"]), Some(2));
    assert_eq!(pick_latest_run(&["ok", "error", "ok"]), Some(0));
    assert_eq!(pick_latest_run(&[]), None);
}

#[test]
fn refresh_all_targets_keeps_layout_order_and_drops_disabled() {
    let mut store = DashboardStore::default();
    store.mount_dashboard("d");
    store.apply_load_ok(
        "d",
        board(
            "d",
            vec![
                widget("first-off", false),
                widget("second", true),
                widget("third", true),
                widget("fourth-off", false),
            ],
        ),
        std::collections::HashMap::new(),
    );
    assert_eq!(
        refresh_all_targets(store.entry("d")),
        vec!["second".to_owned(), "third".to_owned()]
    );
}

#[test]
fn run_updated_gate_rejects_every_incomplete_payload() {
    let mut store = DashboardStore::default();
    store.mount_dashboard("d");
    let full = run_updated_gate(&store, Some("d"), Some("w"), true);
    assert!(full, "the full payload on a live entry passes");
    assert!(!run_updated_gate(&store, None, Some("w"), true));
    assert!(!run_updated_gate(&store, Some("d"), None, true));
    assert!(!run_updated_gate(&store, Some("d"), Some("w"), false));
    assert!(!run_updated_gate(&store, Some("unknown"), Some("w"), true));
}

#[test]
fn changed_gate_reads_the_refcount_not_the_entry_presence() {
    let mut store = DashboardStore::default();
    // A materialised-but-never-mounted entry (a load begun without mount)
    // has refCount 0: dead to the gate.
    store.begin_load("d");
    assert!(
        !changed_gate(&store, Some("d")),
        "present with refCount 0 is dead"
    );

    // One mount: live.
    store.mount_dashboard("d");
    assert!(changed_gate(&store, Some("d")));

    // A middle release keeps it live; the last release kills it.
    store.mount_dashboard("d");
    store.release_dashboard("d");
    assert!(changed_gate(&store, Some("d")));
    store.release_dashboard("d");
    assert!(!changed_gate(&store, Some("d")));
    assert!(!changed_gate(&store, None));
}

#[test]
fn set_run_can_clear_a_slot_back_to_loaded_none() {
    let mut store = DashboardStore::default();
    store.set_run("d", "w", Some(run(1)));
    store.set_run("d", "w", None);
    let entry = store.entry("d").unwrap();
    assert!(
        entry.runs.get("w").is_some() && entry.runs["w"].is_none(),
        "null run: the key exists with None"
    );
    assert!(
        !entry.runs.contains_key("other"),
        "a never-loaded widget keeps no key"
    );
}
