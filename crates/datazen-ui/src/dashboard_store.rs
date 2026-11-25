//! T184: the `stores/dashboardStore.ts` port — the dashboards store's state
//! model and transitions.
//!
//! React's module is a zustand store over `dashboardCommands` (async, over
//! the IPC) plus a cross-window listener pair. In immediate mode the store
//! is the state: the dashboard host owns one [`DashboardStore`] field and
//! each zustand action becomes a method. The async halves split the
//! established way: a `begin_*` method writes the in-flight state, the host
//! runs the command (the `commands/dashboard.rs` request builders), and an
//! `apply_*` method writes the settled state — the exact `set` calls the
//! React action makes, in the same order.
//!
//! ## What maps where
//!
//! | React | here |
//! |---|---|
//! | `DashboardEntry` | [`DashboardEntry`] (`dashboard: Option<DashboardVm>`, `runs: HashMap<String, Option<WidgetRunVm>>`) |
//! | `emptyEntry()` | [`DashboardEntry::empty`] |
//! | `patchEntry(byId, id, patch)` | [`DashboardStore::patch_entry`] — missing ids get the empty entry first |
//! | `mountDashboard` (refCount+1) | [`DashboardStore::mount_dashboard`] |
//! | `releaseDashboard` (refCount−1, drop at 0) | [`DashboardStore::release_dashboard`] |
//! | `fetchDashboards`'s three `set`s | [`DashboardStore::begin_fetch`] / `apply_fetch_ok` / `apply_fetch_err` |
//! | `loadDashboard`'s three `set`s | [`DashboardStore::begin_load`] / `apply_load_ok` / `apply_load_err` |
//! | `loadLatestRun`'s pick | [`pick_latest_run`] (the first `ok` entry, else the first, else none) |
//! | `saveDashboard`'s stamp + `set` | [`stamp_save_input`] / [`DashboardStore::apply_save`] |
//! | `deleteDashboard`'s `set` | [`DashboardStore::apply_delete`] |
//! | `refreshWidget`'s busy/run/error `set`s | [`DashboardStore::begin_refresh`] / `apply_refresh_ok` / `apply_refresh_err` |
//! | `refreshAllWidgets`'s widget selection | [`refresh_all_targets`] (the enabled widgets) |
//! | `setRun` | [`DashboardStore::set_run`] |
//! | the cross-window `dashboard:run-updated` gate | [`run_updated_gate`] |
//! | the cross-window `dashboard:changed` gate | [`changed_gate`] |
//!
//! `run-updated`'s `runs[widgetId] = run` (the listener's write) is
//! [`DashboardStore::set_run`] — the same method the listener calls, with
//! the gate decided first because the listener itself early-returns.
//!
//! ## Deliberate deviations from the React source
//!
//! * **`dashboard: null` vs `undefined` in `runs`.** React's entry starts
//!   `runs: {}` (no key = never loaded) and writes `runs[w.id] = null`
//!   when the run could not be fetched — the distinction the panel's
//!   on-open refresh gate reads (its own module doc). The port keeps the
//!   same two-level shape: a missing key is never-loaded, a `None` value
//!   is loaded-and-empty.
//! * **The cross-window rail itself is not here.** `listenCrossWindow` is
//!   the web host's window-event bus — a host concern. What the store
//!   owns is the gating logic, which is why the two gates are pure
//!   functions the host calls before the store methods.
//! * **`updatedAt` stamping is the host's clock.** React stamps
//!   `new Date().toISOString()` into the save request; the port takes the
//!   stamp as a parameter (see [`stamp_save_input`]) so tests and the
//!   host pin the instant.
//! * **`fetchDashboards` inside `dashboard:changed`** is a fire-and-forget
//!   the host performs — the gate says whether to; the store does not
//!   start commands.

use std::collections::{HashMap, HashSet};

use crate::views::dashboard::dashboard_panel::DashboardVm;
use crate::views::dashboard::chart_widget_tile::WidgetRunVm;

/// The store's per-dashboard entry — React's `DashboardEntry`.
#[derive(Clone, Debug, Default)]
pub struct DashboardEntry {
    /// React `dashboard: Dashboard | null`; `None` until the load lands.
    pub dashboard: Option<DashboardVm>,
    /// React `runs: Record<string, WidgetRun | null>` — a missing key is
    /// "never loaded" (React's absent property), a `None` value is
    /// "loaded, none available".
    pub runs: HashMap<String, Option<WidgetRunVm>>,
    /// React `busyWidgets: Record<string, boolean>` — membership is `true`.
    pub busy_widgets: HashSet<String>,
    /// React `loading`.
    pub loading: bool,
    /// React `error: string | null`.
    pub error: Option<String>,
    /// React `refCount`.
    pub ref_count: i64,
}

impl DashboardEntry {
    /// React's `emptyEntry()`.
    pub fn empty() -> Self {
        DashboardEntry {
            dashboard: None,
            runs: HashMap::new(),
            busy_widgets: HashSet::new(),
            loading: false,
            error: None,
            ref_count: 0,
        }
    }
}

/// The store: the map plus the list surface. The zustand hook becomes the
/// host's single field.
#[derive(Clone, Debug, Default)]
pub struct DashboardStore {
    /// React `dashboardsById`.
    pub dashboards_by_id: HashMap<String, DashboardEntry>,
    /// React `list: Dashboard[]`.
    pub list: Vec<DashboardVm>,
    /// React `listError`.
    pub list_error: Option<String>,
    /// React `listLoading`.
    pub list_loading: bool,
}

impl DashboardStore {
    /// React's `patchEntry`, the write half of every entry transition:
    /// a missing id starts from the empty entry, the patch overlays it.
    fn patch_entry<F>(&mut self, id: &str, patch: F)
    where
        F: FnOnce(DashboardEntry) -> DashboardEntry,
    {
        let prev = self
            .dashboards_by_id
            .remove(id)
            .unwrap_or_else(DashboardEntry::empty);
        self.dashboards_by_id.insert(id.to_owned(), patch(prev));
    }

    /// The entry at `id`, if any (read access for hosts and tests).
    pub fn entry(&self, id: &str) -> Option<&DashboardEntry> {
        self.dashboards_by_id.get(id)
    }

    // ---- mount / release ---------------------------------------------------

    /// React `mountDashboard`: refCount+1. The listener install it triggers
    /// is a host concern (the module doc's rail ruling).
    pub fn mount_dashboard(&mut self, id: &str) {
        let prev = self
            .dashboards_by_id
            .remove(id)
            .unwrap_or_else(DashboardEntry::empty);
        let mut next = prev;
        next.ref_count += 1;
        self.dashboards_by_id.insert(id.to_owned(), next);
    }

    /// React `releaseDashboard`: refCount−1, and the entry disappears
    /// entirely once the count reaches zero (an unknown id is a no-op —
    /// React's `if (!prev) return s`).
    pub fn release_dashboard(&mut self, id: &str) {
        let Some(mut prev) = self.dashboards_by_id.remove(id) else {
            return;
        };
        prev.ref_count -= 1;
        if prev.ref_count > 0 {
            self.dashboards_by_id.insert(id.to_owned(), prev);
        }
    }

    // ---- fetchDashboards ---------------------------------------------------

    /// The first `set` of `fetchDashboards`: the list goes busy, the error
    /// clears.
    pub fn begin_fetch(&mut self) {
        self.list_loading = true;
        self.list_error = None;
    }

    /// The success `set`: the list lands, busy clears.
    pub fn apply_fetch_ok(&mut self, list: Vec<DashboardVm>) {
        self.list = list;
        self.list_loading = false;
    }

    /// The catch `set`: busy clears, the error lands (React's
    /// `e instanceof Error ? e.message : String(e)` is the host's
    /// formatting — the port takes the string).
    pub fn apply_fetch_err(&mut self, message: impl Into<String>) {
        self.list_loading = false;
        self.list_error = Some(message.into());
    }

    // ---- loadDashboard -----------------------------------------------------

    /// The first `set` of `loadDashboard`: the entry goes busy, its error
    /// clears (the entry materialises through `patchEntry`'s empty default).
    pub fn begin_load(&mut self, id: &str) {
        self.patch_entry(id, |mut entry| {
            entry.loading = true;
            entry.error = None;
            entry
        });
    }

    /// The success `set`: the dashboard and its runs land, busy clears.
    /// `runs` is the full record the host built (each widget's
    /// [`pick_latest_run`] outcome, `None` for a widget whose latest run
    /// could not be fetched).
    pub fn apply_load_ok(
        &mut self,
        id: &str,
        dashboard: DashboardVm,
        runs: HashMap<String, Option<WidgetRunVm>>,
    ) {
        self.patch_entry(id, |mut entry| {
            entry.dashboard = Some(dashboard);
            entry.runs = runs;
            entry.loading = false;
            entry
        });
    }

    /// The catch `set`: busy clears, the error lands.
    pub fn apply_load_err(&mut self, id: &str, message: impl Into<String>) {
        self.patch_entry(id, |mut entry| {
            entry.loading = false;
            entry.error = Some(message.into());
            entry
        });
    }

    // ---- saveDashboard -----------------------------------------------------

    /// The `set` of `saveDashboard` (the saved answer landing): an existing
    /// entry takes the saved dashboard; the list upserts by id — a known
    /// member is replaced in place, an unknown one is appended (React's
    /// `some/map` vs `[...list, saved]`).
    pub fn apply_save(&mut self, saved: DashboardVm) {
        if self.dashboards_by_id.contains_key(&saved.id) {
            let dashboard = saved.clone();
            self.patch_entry(&saved.id, |mut entry| {
                entry.dashboard = Some(dashboard);
                entry
            });
        }
        if let Some(member) = self.list.iter_mut().find(|d| d.id == saved.id) {
            *member = saved;
        } else {
            self.list.push(saved);
        }
    }

    // ---- deleteDashboard ---------------------------------------------------

    /// The `set` of `deleteDashboard`: the entry and the list member both
    /// disappear (an unknown id simply changes nothing).
    pub fn apply_delete(&mut self, id: &str) {
        self.dashboards_by_id.remove(id);
        self.list.retain(|d| d.id != id);
    }

    // ---- refreshWidget -----------------------------------------------------

    /// The busy `set` of `refreshWidget`: the widget's busy flag goes on
    /// (the entry materialises from the empty default if absent —
    /// React's `?? emptyEntry()`).
    pub fn begin_refresh(&mut self, dashboard_id: &str, widget_id: &str) {
        self.patch_entry(dashboard_id, |mut entry| {
            entry.busy_widgets.insert(widget_id.to_owned());
            entry
        });
    }

    /// The success `set`s: the run lands under the widget, busy clears.
    pub fn apply_refresh_ok(
        &mut self,
        dashboard_id: &str,
        widget_id: &str,
        run: WidgetRunVm,
    ) {
        self.patch_entry(dashboard_id, |mut entry| {
            entry.runs.insert(widget_id.to_owned(), Some(run));
            entry.busy_widgets.remove(widget_id);
            entry
        });
    }

    /// The catch `set`s: busy clears and the error lands (the run slot is
    /// left untouched — React writes no `runs` entry on the error path).
    pub fn apply_refresh_err(
        &mut self,
        dashboard_id: &str,
        widget_id: &str,
        message: impl Into<String>,
    ) {
        self.patch_entry(dashboard_id, |mut entry| {
            entry.busy_widgets.remove(widget_id);
            entry.error = Some(message.into());
            entry
        });
    }

    // ---- setRun -------------------------------------------------------------

    /// React `setRun`: the run slot under the widget — `None` clears it
    /// the way React's `run: null` does. The entry materialises from the
    /// empty default when absent.
    pub fn set_run(
        &mut self,
        dashboard_id: &str,
        widget_id: &str,
        run: Option<WidgetRunVm>,
    ) {
        self.patch_entry(dashboard_id, |mut entry| {
            entry.runs.insert(widget_id.to_owned(), run);
            entry
        });
    }
}

/// React `loadLatestRun`'s pick: the first index entry whose status is
/// `ok`, else the first entry, else nothing. The entries are the
/// `listWidgetRuns(dashboardId, widgetId, 1)` answer — the single-entry
/// index the host fetched.
pub fn pick_latest_run(statuses: &[&str]) -> Option<usize> {
    if let Some(ok_index) = statuses.iter().position(|status| *status == "ok") {
        return Some(ok_index);
    }
    (!statuses.is_empty()).then_some(0)
}

/// React `saveDashboard`'s stamp: the saved request carries
/// `updatedAt: new Date().toISOString()`. The host owns the clock; the
/// stamp is written before the request, so the answer's `updatedAt` and
/// the list's copy agree.
pub fn stamp_save_input(mut dashboard: DashboardVm, updated_at: impl Into<String>) -> DashboardVm {
    dashboard.updated_at = updated_at.into();
    dashboard
}

/// React `refreshAllWidgets`'s selection: the enabled widgets of the
/// dashboard (the disabled ones are skipped, and an absent dashboard or
/// entry means nothing to refresh).
pub fn refresh_all_targets(entry: Option<&DashboardEntry>) -> Vec<String> {
    entry
        .and_then(|entry| entry.dashboard.as_ref())
        .map(|dashboard| {
            dashboard
                .widgets
                .iter()
                .filter(|widget| widget.enabled)
                .map(|widget| widget.id.clone())
                .collect()
        })
        .unwrap_or_default()
}

/// The cross-window `dashboard:run-updated` gate: the payload needs all
/// three fields present, and the entry must exist with a positive
/// refCount (React's two early returns). The store write the listener then
/// performs is [`DashboardStore::set_run`].
pub fn run_updated_gate(
    store: &DashboardStore,
    dashboard_id: Option<&str>,
    widget_id: Option<&str>,
    has_run: bool,
) -> bool {
    let (Some(dashboard_id), Some(widget_id)) = (dashboard_id, widget_id) else {
        return false;
    };
    if !has_run {
        return false;
    }
    match store.entry(dashboard_id) {
        Some(entry) => entry.ref_count > 0,
        None => false,
    }
}

/// The cross-window `dashboard:changed` gate: whether the entry should be
/// reloaded (it exists with a positive refCount). The list refetch that
/// follows unconditionally is the host's fire-and-forget — it does not
/// consult the store.
pub fn changed_gate(store: &DashboardStore, dashboard_id: Option<&str>) -> bool {
    match (dashboard_id, dashboard_id.and_then(|id| store.entry(id))) {
        (Some(_), Some(entry)) => entry.ref_count > 0,
        _ => false,
    }
}

#[cfg(test)]
mod store_tests {
    use super::*;
    use crate::views::dashboard::chart_widget_tile::{TileViewMode, WidgetRunStatus};
    use crate::views::dashboard::dashboard_panel::PanelWidgetVm;

    fn board(id: &str, name: &str, widgets: Vec<PanelWidgetVm>) -> DashboardVm {
        DashboardVm {
            id: id.to_owned(),
            name: name.to_owned(),
            created_at: "2026-01-01T00:00:00.000Z".to_owned(),
            updated_at: "2026-01-01T00:00:00.000Z".to_owned(),
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
            layout: Default::default(),
            refresh: Default::default(),
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
    fn mount_release_follow_the_react_refcount_lifecycle() {
        let mut store = DashboardStore::default();
        assert!(store.entry("d1").is_none(), "nothing mounted");

        store.mount_dashboard("d1");
        assert_eq!(store.entry("d1").unwrap().ref_count, 1);
        // A mount of an already-mounted dashboard stacks.
        store.mount_dashboard("d1");
        assert_eq!(store.entry("d1").unwrap().ref_count, 2);

        // A release that leaves a positive count keeps the entry.
        store.release_dashboard("d1");
        assert_eq!(store.entry("d1").unwrap().ref_count, 1);

        // The last release drops the entry entirely.
        store.release_dashboard("d1");
        assert!(store.entry("d1").is_none());

        // Releasing an unknown id is a no-op (React's early return).
        store.release_dashboard("never-mounted");
        assert!(store.entry("never-mounted").is_none());
    }

    #[test]
    fn fetch_walk_writes_the_three_sets_in_order() {
        let mut store = DashboardStore::default();
        store.begin_fetch();
        assert!(store.list_loading);
        assert!(store.list_error.is_none());

        store.apply_fetch_ok(vec![board("a", "A", Vec::new())]);
        assert!(!store.list_loading);
        assert_eq!(store.list.len(), 1);
        assert_eq!(store.list[0].id, "a");

        store.begin_fetch();
        store.apply_fetch_err("boom");
        assert!(!store.list_loading);
        assert_eq!(store.list_error.as_deref(), Some("boom"));
        // The list survives the failed refetch (React only clears it on
        // success).
        assert_eq!(store.list.len(), 1);
    }

    #[test]
    fn load_walk_materialises_the_entry_even_before_the_answer() {
        let mut store = DashboardStore::default();
        // Loading an id nobody mounted still materialises the entry —
        // React's patchEntry defaults the missing id to the empty entry.
        store.begin_load("d1");
        let entry = store.entry("d1").expect("the entry materialised");
        assert!(entry.loading);
        assert!(entry.error.is_none());
        assert!(entry.dashboard.is_none());

        let mut runs = HashMap::new();
        runs.insert("w1".to_owned(), Some(run(7)));
        runs.insert("w2".to_owned(), None); // loaded, none available
        store.apply_load_ok("d1", board("d1", "D", vec![widget("w1", true)]), runs);
        let entry = store.entry("d1").unwrap();
        assert!(!entry.loading);
        assert!(entry.dashboard.is_some());
        assert_eq!(entry.runs["w1"].as_ref().unwrap().row_count, 7);
        assert!(entry.runs["w2"].is_none(), "the loaded-none slot");
        assert!(!entry.runs.contains_key("w3"), "never loaded: no key");

        store.begin_load("d1");
        store.apply_load_err("d1", "network");
        let entry = store.entry("d1").unwrap();
        assert!(!entry.loading);
        assert_eq!(entry.error.as_deref(), Some("network"));
    }

    #[test]
    fn save_upserts_the_list_and_only_a_known_entry() {
        let mut store = DashboardStore::default();
        store.apply_fetch_ok(vec![board("a", "A0", Vec::new())]);

        // A known member is replaced in place.
        let saved = stamp_save_input(board("a", "A1", Vec::new()), "2026-02-02T00:00:00.000Z");
        assert_eq!(saved.updated_at, "2026-02-02T00:00:00.000Z");
        store.apply_save(saved);
        assert_eq!(store.list.len(), 1);
        assert_eq!(store.list[0].name, "A1");

        // An unknown member is appended, and no entry materialises for it.
        store.apply_save(board("b", "B", Vec::new()));
        assert_eq!(store.list.len(), 2);
        assert_eq!(store.list[1].id, "b");
        assert!(store.entry("b").is_none(), "save never creates an entry");

        // An entry that exists takes the saved dashboard.
        store.mount_dashboard("a");
        store.apply_save(board("a", "A2", Vec::new()));
        assert_eq!(store.entry("a").unwrap().dashboard.as_ref().unwrap().name, "A2");
        assert_eq!(store.list[0].name, "A2");
    }

    #[test]
    fn delete_clears_both_halves_and_tolerates_unknowns() {
        let mut store = DashboardStore::default();
        store.apply_fetch_ok(vec![board("a", "A", Vec::new()), board("b", "B", Vec::new())]);
        store.mount_dashboard("a");
        store.apply_delete("a");
        assert!(store.entry("a").is_none());
        assert_eq!(store.list.len(), 1);
        assert_eq!(store.list[0].id, "b");

        store.apply_delete("never-existed");
        assert_eq!(store.list.len(), 1);
    }

    #[test]
    fn refresh_walk_turns_busy_on_and_off_on_both_exits() {
        let mut store = DashboardStore::default();
        store.mount_dashboard("d1");

        store.begin_refresh("d1", "w1");
        assert!(store.entry("d1").unwrap().busy_widgets.contains("w1"));

        store.apply_refresh_ok("d1", "w1", run(3));
        let entry = store.entry("d1").unwrap();
        assert!(!entry.busy_widgets.contains("w1"));
        assert_eq!(entry.runs["w1"].as_ref().unwrap().row_count, 3);

        // The error exit clears busy and writes the error, never a run.
        store.begin_refresh("d1", "w2");
        store.apply_refresh_err("d1", "w2", "widget blew up");
        let entry = store.entry("d1").unwrap();
        assert!(!entry.busy_widgets.contains("w2"));
        assert_eq!(entry.error.as_deref(), Some("widget blew up"));
        assert!(!entry.runs.contains_key("w2"), "no run on the error path");

        // A refresh of an id nobody mounted still tracks busy (React's
        // ?? emptyEntry() default).
        store.begin_refresh("ghost", "w1");
        assert!(store.entry("ghost").unwrap().busy_widgets.contains("w1"));
    }

    #[test]
    fn set_run_writes_and_clears_the_slot() {
        let mut store = DashboardStore::default();
        store.set_run("d1", "w1", Some(run(5)));
        assert_eq!(store.entry("d1").unwrap().runs["w1"].as_ref().unwrap().row_count, 5);

        store.set_run("d1", "w1", None);
        assert!(store.entry("d1").unwrap().runs["w1"].is_none());
    }

    #[test]
    fn latest_run_pick_prefers_the_first_ok_entry() {
        assert_eq!(pick_latest_run(&[]), None);
        assert_eq!(pick_latest_run(&["error"]), Some(0));
        assert_eq!(pick_latest_run(&["error", "ok", "ok"]), Some(1));
        assert_eq!(pick_latest_run(&["ok", "error"]), Some(0));
        assert_eq!(pick_latest_run(&["error", "error"]), Some(0));
    }

    #[test]
    fn refresh_all_targets_are_the_enabled_widgets() {
        let mut store = DashboardStore::default();
        assert!(refresh_all_targets(store.entry("none")).is_empty());

        store.mount_dashboard("d1");
        // No dashboard loaded yet: nothing to refresh.
        assert!(refresh_all_targets(store.entry("d1")).is_empty());

        store.apply_load_ok(
            "d1",
            board(
                "d1",
                "D",
                vec![widget("on", true), widget("off", false), widget("on2", true)],
            ),
            HashMap::new(),
        );
        assert_eq!(
            refresh_all_targets(store.entry("d1")),
            vec!["on".to_owned(), "on2".to_owned()]
        );
    }

    #[test]
    fn run_updated_gate_needs_all_fields_and_a_live_entry() {
        let mut store = DashboardStore::default();
        // No entry at all.
        assert!(!run_updated_gate(&store, Some("d1"), Some("w1"), true));

        // Mounted (refCount > 0): the full payload passes.
        store.mount_dashboard("d1");
        assert!(run_updated_gate(&store, Some("d1"), Some("w1"), true));

        // Missing fields: rejected.
        assert!(!run_updated_gate(&store, None, Some("w1"), true));
        assert!(!run_updated_gate(&store, Some("d1"), None, true));
        assert!(!run_updated_gate(&store, Some("d1"), Some("w1"), false));

        // Released to zero: rejected.
        store.release_dashboard("d1");
        assert!(!run_updated_gate(&store, Some("d1"), Some("w1"), true));
    }

    #[test]
    fn changed_gate_reloads_only_a_live_entry() {
        let mut store = DashboardStore::default();
        assert!(!changed_gate(&store, None));
        assert!(!changed_gate(&store, Some("d1")), "no entry");

        store.mount_dashboard("d1");
        store.mount_dashboard("d1");
        assert!(changed_gate(&store, Some("d1")));

        // One release left: still live.
        store.release_dashboard("d1");
        assert!(changed_gate(&store, Some("d1")));

        // The last release: dead.
        store.release_dashboard("d1");
        assert!(!changed_gate(&store, Some("d1")));
    }
}
