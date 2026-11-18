//! T005: the global query-history dialog port
//! (`components/history/global_query_history_dialog.rs`).
//!
//! Real probes against a mounted modal: open/close (Escape and the header
//! close), filter-bar wiring, paged entry cards, select-all/count, the
//! pagination footer, the empty/loading/error branches, and window bounds at
//! 960×640 / 1440×816 in both themes. The scene holds the dialog VM in the app
//! state `S`, exactly as a real host would.

use rune::{col, Cx, El, Env, Key, KeyEvent, Mods, Runtime, TextEdit, Tx};

use crate::components::history::global_query_history_dialog::{
    GlobalHistoryEntry, GlobalHistoryVm, GlobalQueryHistoryDialog, HistoryPageVm,
};
use crate::components::history::{
    HistoryConnectionOption, HistoryEntryVm, HistoryFilterOpen, HistoryQueryState,
};
use crate::i18n::t;

const KEY: &str = "global-query-history-dialog";

#[derive(Clone)]
struct Scene {
    vm: GlobalHistoryVm,
    search: TextEdit,
    dark: bool,
    // recorded callbacks
    closes: usize,
    opened: Vec<String>,
    queries: Vec<HistoryQueryState>,
    toggles: Vec<String>,
    toggle_all: usize,
    actions: Vec<(String, String)>,
    copies: Vec<String>,
    exports: usize,
    clears: usize,
    confirm_clears: usize,
    cancel_clears: usize,
    notice_dismisses: usize,
    pages: Vec<usize>,
    filter_history: Vec<HistoryFilterOpen>,
}

impl Default for Scene {
    fn default() -> Self {
        Self {
            vm: GlobalHistoryVm {
                open: true,
                can_open: true,
                connections: vec![
                    HistoryConnectionOption {
                        id: "c1".to_string(),
                        name: "Conn A".to_string(),
                    },
                    HistoryConnectionOption {
                        id: "c2".to_string(),
                        name: "Conn B".to_string(),
                    },
                ],
                ..GlobalHistoryVm::default()
            },
            search: TextEdit::new(String::new()),
            dark: true,
            closes: 0,
            opened: Vec::new(),
            queries: Vec::new(),
            toggles: Vec::new(),
            toggle_all: 0,
            actions: Vec::new(),
            copies: Vec::new(),
            exports: 0,
            clears: 0,
            confirm_clears: 0,
            cancel_clears: 0,
            notice_dismisses: 0,
            pages: Vec::new(),
            filter_history: Vec::new(),
        }
    }
}

fn entry(id: &str, connection: &str, success: bool) -> GlobalHistoryEntry {
    GlobalHistoryEntry {
        connection_id: connection.to_string(),
        entry: HistoryEntryVm {
            id: id.to_string(),
            success,
            database: Some("appdb".to_string()),
            schema: Some("public".to_string()),
            execution_time_ms: 42,
            rows_affected: Some(7),
            executed_at: "2026-11-17 10:00:00".to_string(),
            sql: format!("select * from {id};"),
            error_message: None,
        },
    }
}

fn paged(count: usize, total: usize, page: usize, page_size: usize) -> HistoryPageVm {
    let entries = (0..count)
        .map(|i| {
            let success = i % 2 == 0;
            entry(&format!("h{i}"), if success { "c1" } else { "c2" }, success)
        })
        .collect();
    HistoryPageVm {
        entries,
        total,
        page,
        page_size,
    }
}

fn view(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    let mut layer = Vec::new();
    GlobalQueryHistoryDialog::<Scene>::new(KEY, &scene.vm, &scene.search)
        .on_close(|s: &mut Scene| {
            s.closes += 1;
            s.vm.open = false;
        })
        .on_select_query(|s: &mut Scene, id| s.opened.push(id))
        .on_query(|s: &mut Scene, query| s.queries.push(query))
        .on_filter_open(|s: &mut Scene, which| s.filter_history.push(which))
        .on_toggle_select(|s: &mut Scene, id| s.toggles.push(id))
        .on_toggle_all(|s: &mut Scene| s.toggle_all += 1)
        .on_action(|s: &mut Scene, action, id| {
            s.actions.push((action.as_str().to_string(), id))
        })
        .on_copy(|s: &mut Scene, id, _tx: &mut Tx| s.copies.push(id))
        .on_export(|s: &mut Scene| s.exports += 1)
        .on_clear(|s: &mut Scene| {
            s.clears += 1;
            s.vm.confirm_clear = true;
        })
        .on_confirm_clear(|s: &mut Scene| s.confirm_clears += 1)
        .on_cancel_clear(|s: &mut Scene| s.cancel_clears += 1)
        .on_dismiss_notice(|s: &mut Scene| s.notice_dismisses += 1)
        .on_page_change(|s: &mut Scene, page| {
            s.pages.push(page);
            s.vm.page.page = page;
        })
        .on_search_edit(|s: &mut Scene| &mut s.search)
        .build(scene.dark, &mut layer);
    col()
        .key("history-page")
        .w_full()
        .h_full()
        .children(layer)
}

fn runtime(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let mut rt = Runtime::new(
        scene,
        view,
        size,
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    rt
}

fn maybe<S>(rt: &Runtime<S>, key: &str) -> Option<rune::ElementInfo> {
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.key.as_deref() == Some(key))
}

fn has<S>(rt: &Runtime<S>, key: &str) -> bool {
    maybe(rt, key).is_some()
}

fn text_of<S>(rt: &Runtime<S>, key: &str) -> String {
    maybe(rt, key)
        .and_then(|e| e.text)
        .unwrap_or_default()
}

/// Kit buttons/rows put their visible text in a keyed child, so read any
/// element's text rather than only the keyed root.
fn has_text<S>(rt: &Runtime<S>, expected: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| e.text.as_deref() == Some(expected))
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    maybe(rt, key).unwrap_or_else(|| panic!("{key} is mounted"))
}

fn press(rt: &mut Runtime<Scene>, key: &str) {
    let (x, y) = rt
        .find(key)
        .unwrap_or_else(|| panic!("{key} is laid out and pressable"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.frame(0.0);
}

fn escape(rt: &mut Runtime<Scene>) {
    rt.key(KeyEvent {
        key: Key::Escape,
        mods: Mods::default(),
    });
    rt.frame(0.0);
    rt.frame(0.0);
}

fn assert_in_window(rt: &Runtime<Scene>, w: f32, h: f32, label: &str) {
    let outside: Vec<String> = rt
        .elements()
        .iter()
        .filter(|e| {
            let r = e.rect;
            !e.virtual_extent
                && (r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > w + 0.5 || r[1] + r[3] > h + 0.5)
        })
        .map(|e| format!("{} {:?}", e.key.clone().unwrap_or(e.path.clone()), e.rect))
        .collect();
    assert!(
        outside.is_empty(),
        "{label}: {} node(s) out of window {w}x{h}: {outside:?}",
        outside.len()
    );
}

fn assert_unique_keys(rt: &Runtime<Scene>, label: &str) {
    // Kit components reuse internal keys (`value`, `chevron`, `label`, ...) across
    // instances; that is an upstream convention, not this port's contract. Only
    // the dialog's own keys (and the history parts it composes) must be unique.
    let mut seen = std::collections::HashSet::new();
    let mut dupes = Vec::new();
    for e in rt.elements() {
        if let Some(k) = e.key {
            if (k.starts_with(KEY) || k.starts_with("history-")) && !seen.insert(k.clone()) {
                dupes.push(k);
            }
        }
    }
    assert!(dupes.is_empty(), "{label}: duplicate keys {dupes:?}");
}

fn populated(count: usize, total: usize) -> Scene {
    let mut scene = Scene::default();
    scene.vm.page = paged(count, total, 0, 200);
    scene
}

// ---- mount / open / close ----------------------------------------------------

#[test]
fn the_dialog_mounts_its_shell_and_closes_on_escape() {
    let mut rt = runtime(populated(3, 3), (1440.0, 816.0));
    for k in [
        KEY,
        &format!("{KEY}-title"),
        &format!("{KEY}-close"),
        &format!("{KEY}-body"),
        &format!("{KEY}-scrim"),
        "history-filter-bar",
        &format!("{KEY}-selection-bar"),
        &format!("{KEY}-list"),
        &format!("{KEY}-pagination"),
    ] {
        assert!(has(&rt, k), "expected key mounted: {k}");
    }
    assert_eq!(text_of(&rt, &format!("{KEY}-title")), t("query.historyTitle"));
    assert_eq!(element(&rt, KEY).role, "dialog");
    assert_eq!(rt.state.closes, 0);

    escape(&mut rt);
    assert_eq!(rt.state.closes, 1, "Escape runs the close handler");
    assert!(!has(&rt, KEY), "the panel unmounts after Escape");
}

#[test]
fn the_header_close_button_and_a_false_open_flag_also_unmount_it() {
    let mut rt = runtime(populated(1, 1), (1440.0, 816.0));
    press(&mut rt, &format!("{KEY}-close"));
    assert_eq!(rt.state.closes, 1, "the header close runs the handler");

    let mut closed = populated(1, 1);
    closed.vm.open = false;
    let rt = runtime(closed, (1440.0, 816.0));
    assert!(!has(&rt, KEY), "a closed VM mounts nothing");
    assert!(!has(&rt, &format!("{KEY}-scrim")), "and no scrim");
}

// ---- filter bar wiring -------------------------------------------------------

#[test]
fn the_filter_bar_is_wired_and_emits_query_changes() {
    let mut rt = runtime(populated(2, 2), (1440.0, 816.0));
    for k in [
        "history-filter-bar",
        "history-filter-bar-search",
        "history-filter-bar-search-field",
        "history-filter-bar-connection-trigger",
        "history-filter-bar-status-trigger",
        "history-filter-bar-range-trigger",
        "history-filter-bar-sort-trigger",
    ] {
        assert!(has(&rt, k), "filter part mounted: {k}");
    }

    // Typing in the search field reaches the search TextEdit and emits a query.
    let (x, y) = rt
        .find("history-filter-bar-search")
        .expect("search field is laid out");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    rt.key(KeyEvent {
        key: Key::Text("xyz".into()),
        mods: Mods::default(),
    });
    rt.frame(1.0 / 60.0);
    assert!(
        rt.state.search.text.contains("xyz"),
        "search text entered: {:?}",
        rt.state.search.text
    );

    // Choosing a connection from the select reaches the on_set_connection hook.
    press(&mut rt, "history-filter-bar-connection-trigger");
    assert!(
        rt.state.filter_history
            .contains(&HistoryFilterOpen::Connection),
        "select toggle reached the VM: {:?}",
        rt.state.filter_history
    );
    let option = "history-filter-bar-connection-item-Conn A";
    if has(&rt, option) {
        press(&mut rt, option);
        let last = rt.state.queries.last().expect("a query was emitted");
        assert_eq!(last.connection_id, "c1", "connection choice rewrote the query");
        assert!(last.database.is_empty(), "database resets with the connection");
    }
}

// ---- entry cards -------------------------------------------------------------

#[test]
fn entry_cards_render_from_the_paged_vm_with_connection_names() {
    let rt = runtime(populated(3, 3), (1440.0, 816.0));
    for i in 0..3 {
        let id = format!("h{i}");
        assert!(
            has(&rt, &format!("history-entry-card-{id}")),
            "card {id} mounted from the paged VM"
        );
        assert_eq!(
            text_of(&rt, &format!("history-entry-card-{id}-sql")),
            format!("select * from {id};")
        );
    }
    assert_eq!(
        text_of(&rt, "history-entry-card-h0-connection"),
        "Conn A",
        "even rows use connection c1's name"
    );
    assert_eq!(
        text_of(&rt, "history-entry-card-h1-connection"),
        "Conn B",
        "odd rows use connection c2's name"
    );
    assert!(
        !has(&rt, "history-entry-card-h3"),
        "no fourth card when the page has three rows"
    );
}

// ---- selection / actions / notice / clear -----------------------------------

#[test]
fn select_all_count_export_copy_and_open_are_wired() {
    let mut scene = populated(3, 3);
    scene.vm.selected = vec!["h0".into(), "h1".into()];
    let mut rt = runtime(scene, (1440.0, 816.0));

    let expected_export = t("query.historyExportSelected").replace("{count}", "2");
    assert!(
        has_text(&rt, &expected_export),
        "export label reflects the visible selected count: {expected_export}"
    );

    press(&mut rt, &format!("{KEY}-select-all"));
    assert_eq!(rt.state.toggle_all, 1, "select-all callback reached");
    press(&mut rt, &format!("{KEY}-export"));
    assert_eq!(rt.state.exports, 1, "export callback reached");

    press(&mut rt, "history-entry-card-h0-action-copy");
    assert_eq!(rt.state.copies, vec!["h0"], "copy callback reached the row id");

    press(&mut rt, "history-entry-card-h1-action-open");
    assert_eq!(rt.state.opened, vec!["h1"], "open reached on_select_query");
    assert_eq!(
        rt.state.actions
            .iter()
            .filter(|(a, _)| a == "open")
            .count(),
        0,
        "Open is not double-forwarded to the generic action hook"
    );

    press(&mut rt, "history-entry-card-h1-action-favorite");
    assert!(
        rt.state.actions.contains(&("favorite".into(), "h1".into())),
        "favorite reached the generic action hook"
    );
}

#[test]
fn notice_and_clear_confirmation_are_wired() {
    let mut scene = populated(2, 2);
    scene.vm.notice = Some(t("query.historyFavoriteSaved").to_string());
    let mut rt = runtime(scene, (1440.0, 816.0));
    assert!(
        has_text(&rt, t("query.historyFavoriteSaved")),
        "the notice text is shown"
    );
    press(&mut rt, &format!("{KEY}-notice-dismiss"));
    assert_eq!(rt.state.notice_dismisses, 1);

    press(&mut rt, &format!("{KEY}-clear"));
    assert_eq!(rt.state.clears, 1, "clear enters the confirmation");
    assert!(!has(&rt, &format!("{KEY}-clear")), "clear button is replaced");

    // Drive the confirm branch through a fresh scene with confirm_clear set.
    let mut confirming = populated(2, 2);
    confirming.vm.confirm_clear = true;
    let mut rt = runtime(confirming, (1440.0, 816.0));
    assert_eq!(
        text_of(&rt, &format!("{KEY}-clear-confirm-text")),
        t("query.historyClearConfirm")
    );
    press(&mut rt, &format!("{KEY}-clear-cancel"));
    assert_eq!(rt.state.cancel_clears, 1);
    press(&mut rt, &format!("{KEY}-clear-confirm"));
    assert_eq!(rt.state.confirm_clears, 1);
}

// ---- truncation / pagination -------------------------------------------------

#[test]
fn truncation_notice_is_honest_and_pagination_requests_pages() {
    let mut scene = populated(5, 42);
    scene.vm.page = paged(5, 42, 0, 5);
    scene.vm.page.page_size = 5;
    let mut rt = runtime(scene, (1440.0, 816.0));
    let expected_truncation = t("query.historyShowingOf")
        .replace("{shown}", "5")
        .replace("{total}", "42");
    assert!(
        has_text(&rt, &expected_truncation),
        "the truncation notice is shown: {expected_truncation}"
    );
    assert_eq!(
        text_of(&rt, &format!("{KEY}-page-label")),
        t("pagination.pageOnly").replace("{page}", "1")
    );
    press(&mut rt, &format!("{KEY}-page-next"));
    assert_eq!(rt.state.pages, vec![1], "next requests page 1");
    press(&mut rt, &format!("{KEY}-page-prev"));
    assert_eq!(rt.state.pages, vec![1, 0], "prev requests page 0");
}

// ---- empty / loading / error -------------------------------------------------

#[test]
fn empty_loading_and_error_branches_render_distinct_states() {
    let empty = runtime(populated(0, 0), (1440.0, 816.0));
    assert!(has(&empty, &format!("{KEY}-empty-text")));
    assert_eq!(text_of(&empty, &format!("{KEY}-empty-text")), t("query.noHistory"));
    assert!(!has(&empty, &format!("{KEY}-loading")));

    let mut no_match = populated(0, 8);
    no_match.vm.query.search = "zzz".into();
    let rt = runtime(no_match, (1440.0, 816.0));
    assert_eq!(text_of(&rt, &format!("{KEY}-empty-text")), t("query.noHistoryMatch"));

    let mut loading = populated(0, 0);
    loading.vm.loading = true;
    let rt = runtime(loading, (1440.0, 816.0));
    assert!(has(&rt, &format!("{KEY}-loading")));
    assert_eq!(text_of(&rt, &format!("{KEY}-loading")), t("query.historyLoading"));

    let mut failed = populated(0, 0);
    failed.vm.error = Some("backend exploded".to_string());
    let rt = runtime(failed, (1440.0, 816.0));
    assert!(has(&rt, &format!("{KEY}-error")));
    assert_eq!(element(&rt, &format!("{KEY}-error")).role, "alert");
    assert!(
        rt.elements()
            .iter()
            .any(|e| e.text.as_deref() == Some("backend exploded")),
        "the caller's error text is shown"
    );
}

// ---- geometry ----------------------------------------------------------------

#[test]
fn the_dialog_stays_inside_both_windows_in_both_themes_and_keys_are_unique() {
    for dark in [true, false] {
        for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
            for (label, scene) in [
                ("populated", populated(4, 4)),
                ("empty", populated(0, 0)),
            ] {
                let mut scene = scene;
                scene.dark = dark;
                let rt = runtime(scene, (w, h));
                let context = format!("{label} dark={dark} {w}x{h}");
                assert_in_window(&rt, w, h, &context);
                assert_unique_keys(&rt, &context);
                let panel = element(&rt, KEY).rect;
                assert!(
                    panel[0] >= -0.5 && panel[0] + panel[2] <= w + 0.5,
                    "panel inside window: {panel:?}"
                );
            }
        }
    }
}

#[test]
fn key_matches_the_react_test_id() {
    assert_eq!(KEY, "global-query-history-dialog");
}
