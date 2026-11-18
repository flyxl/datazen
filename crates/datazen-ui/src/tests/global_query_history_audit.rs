//! Independent audit coverage for T005 (the GlobalQueryHistoryDialog Rune port).
//!
//! Focus: behaviour the coder's own test file does not pin — focus return via a
//! real trigger, the status-filter predicate, pagination bounds and real page
//! swaps, warm-reload/empty-error precedence, callback cardinality, source-level
//! reuse/no-raw-colour checks, and a denser geometry stress at both window sizes
//! in both themes. Rect evidence comes from `Runtime::elements()` (runtime
//! layout), never from a value the component reports about itself.

use rune::{col, Cx, El, Env, Key, KeyEvent, Mods, Runtime, TextEdit, Tx};
use rune_kit::{Button, ButtonVariant, ControlSize};

use crate::components::history::global_query_history_dialog::{
    GlobalHistoryEntry, GlobalHistoryVm, GlobalQueryHistoryDialog, HistoryPageVm,
};
use crate::components::history::history_filter_bar::HistoryStatus;
use crate::components::history::{HistoryConnectionOption, HistoryEntryVm, HistoryFilterOpen};
use crate::i18n::t;
use crate::ui::kit::kit;

const KEY: &str = "global-query-history-dialog";

#[derive(Clone)]
struct AuditState {
    vm: GlobalHistoryVm,
    search: TextEdit,
    dark: bool,
    closes: usize,
    copies: Vec<String>,
    actions: Vec<(String, String)>,
    exports: usize,
    toggle_all: usize,
    clears: usize,
    pages: Vec<usize>,
    filter_history: Vec<HistoryFilterOpen>,
}

impl Default for AuditState {
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
            copies: Vec::new(),
            actions: Vec::new(),
            exports: 0,
            toggle_all: 0,
            clears: 0,
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

fn single(id: &str, connection: &str, success: bool) -> HistoryPageVm {
    HistoryPageVm {
        entries: vec![entry(id, connection, success)],
        total: 1,
        page: 0,
        page_size: 200,
    }
}

/// A page whose ids encode the page number, so a host reload can be observed by
/// watching which cards mount after a page-change callback.
fn mk_page(page: usize, total: usize, page_size: usize) -> HistoryPageVm {
    let entries = (0..page_size)
        .map(|i| entry(&format!("p{page}e{i}"), "c1", true))
        .collect();
    HistoryPageVm {
        entries,
        total,
        page,
        page_size,
    }
}

fn mixed_page() -> HistoryPageVm {
    HistoryPageVm {
        entries: vec![
            entry("h0", "c1", true),
            entry("h1", "c2", false),
            entry("h2", "c1", true),
            entry("h3", "c2", false),
        ],
        total: 4,
        page: 0,
        page_size: 200,
    }
}

fn base_scene() -> AuditState {
    AuditState::default()
}

/// The dialog probe: callbacks mutate the held VM exactly as a host would.
fn dialog_view(scene: &AuditState, _cx: &mut Cx<AuditState>) -> El {
    let mut layer = Vec::new();
    GlobalQueryHistoryDialog::<AuditState>::new(KEY, &scene.vm, &scene.search)
        .on_close(|s: &mut AuditState| {
            s.closes += 1;
            s.vm.open = false;
        })
        .on_select_query(|_s: &mut AuditState, _id| {})
        .on_query(|_s: &mut AuditState, _q| {})
        .on_filter_open(|s: &mut AuditState, which| s.filter_history.push(which))
        .on_toggle_select(|_s: &mut AuditState, _id| {})
        .on_toggle_all(|s: &mut AuditState| s.toggle_all += 1)
        .on_action(|s: &mut AuditState, action, id| {
            s.actions.push((action.as_str().to_string(), id))
        })
        .on_copy(|s: &mut AuditState, id, _tx: &mut Tx| s.copies.push(id))
        .on_export(|s: &mut AuditState| s.exports += 1)
        .on_clear(|s: &mut AuditState| {
            s.clears += 1;
            s.vm.confirm_clear = true;
        })
        .on_confirm_clear(|_s: &mut AuditState| {})
        .on_cancel_clear(|_s: &mut AuditState| {})
        .on_dismiss_notice(|_s: &mut AuditState| {})
        .on_page_change(|s: &mut AuditState, page| {
            s.pages.push(page);
            // Simulate the host loading the requested page.
            let total = s.vm.page.total;
            let size = s.vm.page.page_size;
            s.vm.page = mk_page(page, total, size);
        })
        .on_search_edit(|s: &mut AuditState| &mut s.search)
        .build(scene.dark, &mut layer);
    col()
        .key("history-page")
        .w_full()
        .h_full()
        .children(layer)
}

/// A focus probe: a real trigger opens the modal, so Rune's modal-focus stack
/// has something to restore to on Escape.
fn focus_view(scene: &AuditState, _cx: &mut Cx<AuditState>) -> El {
    let k = kit(scene.dark);
    let mut layer = Vec::new();
    let trigger = Button::new("Open history")
        .variant(ButtonVariant::Neutral)
        .control_size(ControlSize::Small)
        .on_press(|s: &mut AuditState| {
            s.vm.open = true;
            s.vm.open_epoch += 1;
        })
        .build(&k)
        .key("gqh-trigger");
    GlobalQueryHistoryDialog::<AuditState>::new(KEY, &scene.vm, &scene.search)
        .on_close(|s: &mut AuditState| {
            s.closes += 1;
            s.vm.open = false;
        })
        .build(scene.dark, &mut layer);
    col()
        .key("focus-page")
        .w_full()
        .h_full()
        .child(trigger)
        .children(layer)
}

fn runtime(
    scene: AuditState,
    view: fn(&AuditState, &mut Cx<AuditState>) -> El,
    size: (f32, f32),
) -> Runtime<AuditState> {
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

fn dialog_rt(scene: AuditState, size: (f32, f32)) -> Runtime<AuditState> {
    runtime(scene, dialog_view, size)
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

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    maybe(rt, key).unwrap_or_else(|| panic!("{key} is mounted"))
}

fn press(rt: &mut Runtime<AuditState>, key: &str) {
    let (x, y) = rt
        .find(key)
        .unwrap_or_else(|| panic!("{key} is laid out and pressable"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.frame(0.0);
}

fn focused_key(rt: &Runtime<AuditState>) -> Option<String> {
    rt.elements()
        .into_iter()
        .find(|e| e.focused)
        .and_then(|e| e.key)
}

fn tab_to(rt: &mut Runtime<AuditState>, key: &str) {
    for _ in 0..64 {
        rt.key(KeyEvent {
            key: Key::Tab,
            mods: Mods::default(),
        });
        rt.frame(0.0);
        if focused_key(rt).as_deref() == Some(key) {
            return;
        }
    }
    panic!("{key} is reachable from the keyboard");
}

fn assert_in_window(rt: &Runtime<AuditState>, w: f32, h: f32, label: &str) {
    // `rect` is the unscrolled layout box: rows past a scroll viewport keep a
    // rect outside the window even though they are clipped and never drawn.
    // `visible` is runtime-computed as rect ∩ every ancestor clip (including
    // the scroll viewport), so only a positive-area `visible` box that escapes
    // the window is a genuine, observable overflow.
    let outside: Vec<String> = rt
        .elements()
        .iter()
        .filter(|e| {
            if e.virtual_extent {
                return false;
            }
            let v = e.visible;
            if v[2] <= 0.0 || v[3] <= 0.0 {
                return false;
            }
            v[0] < -0.5 || v[1] < -0.5 || v[0] + v[2] > w + 0.5 || v[1] + v[3] > h + 0.5
        })
        .map(|e| {
            format!(
                "{} visible={:?} rect={:?}",
                e.key.clone().unwrap_or(e.path.clone()),
                e.visible,
                e.rect
            )
        })
        .collect();
    assert!(
        outside.is_empty(),
        "{label}: {} visible node(s) out of window {w}x{h}: {outside:?}",
        outside.len()
    );
}

// ---- window shell / focus return ---------------------------------------------

#[test]
fn escape_returns_focus_to_the_trigger_that_opened_the_dialog() {
    // The trigger starts closed so only it is focusable; open it from the
    // keyboard, confirm focus enters the modal, then Escape must restore it.
    let mut scene = base_scene();
    scene.vm.open = false;
    scene.vm.page = single("h0", "c1", true);
    let mut rt = runtime(scene, focus_view, (1440.0, 816.0));

    tab_to(&mut rt, "gqh-trigger");
    rt.key(KeyEvent {
        key: Key::Text(" ".into()),
        mods: Mods::default(),
    });
    rt.frame(0.0);
    rt.frame(0.0);
    assert!(has(&rt, KEY), "space opened the dialog");
    assert_ne!(
        focused_key(&rt).as_deref(),
        Some("gqh-trigger"),
        "focus left the trigger while the modal is open"
    );

    rt.key(KeyEvent {
        key: Key::Escape,
        mods: Mods::default(),
    });
    rt.frame(0.0);
    rt.frame(0.0);
    assert_eq!(rt.state.closes, 1, "Escape ran the close handler");
    assert!(!has(&rt, KEY), "the modal unmounted");
    assert_eq!(
        focused_key(&rt).as_deref(),
        Some("gqh-trigger"),
        "focus returned to the opener"
    );
}

// ---- status filter (component-side predicate) --------------------------------

#[test]
fn the_status_filter_shows_only_matching_rows() {
    let mut success = base_scene();
    success.vm.page = mixed_page();
    success.vm.query.status = HistoryStatus::Success;
    let rt = dialog_rt(success, (1440.0, 816.0));
    assert!(has(&rt, "history-entry-card-h0"), "success row stays");
    assert!(has(&rt, "history-entry-card-h2"), "success row stays");
    assert!(!has(&rt, "history-entry-card-h1"), "failed row hidden under Success");
    assert!(!has(&rt, "history-entry-card-h3"), "failed row hidden under Success");

    let mut failed = base_scene();
    failed.vm.page = mixed_page();
    failed.vm.query.status = HistoryStatus::Failed;
    let rt = dialog_rt(failed, (1440.0, 816.0));
    assert!(has(&rt, "history-entry-card-h1"), "failed row stays");
    assert!(has(&rt, "history-entry-card-h3"), "failed row stays");
    assert!(!has(&rt, "history-entry-card-h0"), "success row hidden under Failed");
    assert!(!has(&rt, "history-entry-card-h2"), "success row hidden under Failed");
}

// ---- pagination footer (Rune extension) --------------------------------------

#[test]
fn pagination_bounds_are_respected_and_page_changes_swap_entries() {
    let mut scene = base_scene();
    scene.vm.page = mk_page(0, 12, 5); // 3 pages
    let mut rt = dialog_rt(scene, (1440.0, 816.0));

    assert!(has(&rt, "history-entry-card-p0e0"), "page 0 cards mounted");
    // Prev is inert on the first page.
    press(&mut rt, &format!("{KEY}-page-prev"));
    assert!(
        rt.state.pages.is_empty(),
        "prev at first page must not request a page: {:?}",
        rt.state.pages
    );
    assert_eq!(
        text_of(&rt, &format!("{KEY}-page-label")),
        t("pagination.pageOnly").replace("{page}", "1")
    );

    // Next loads page 1 and the cards actually change.
    press(&mut rt, &format!("{KEY}-page-next"));
    assert_eq!(rt.state.pages, vec![1], "next requests page index 1");
    assert!(has(&rt, "history-entry-card-p1e0"), "page 1 entries rendered");
    assert!(!has(&rt, "history-entry-card-p0e0"), "page 0 entries gone");
    assert_eq!(
        text_of(&rt, &format!("{KEY}-page-label")),
        t("pagination.pageOnly").replace("{page}", "2")
    );

    // Next to the last page (index 2) then inert at the bound.
    press(&mut rt, &format!("{KEY}-page-next"));
    assert_eq!(rt.state.pages, vec![1, 2]);
    assert!(has(&rt, "history-entry-card-p2e0"));
    press(&mut rt, &format!("{KEY}-page-next"));
    assert_eq!(
        rt.state.pages,
        vec![1, 2],
        "next at the last page must not request a page"
    );
    assert_eq!(
        text_of(&rt, &format!("{KEY}-page-label")),
        t("pagination.pageOnly").replace("{page}", "3")
    );

    // Prev walks back.
    press(&mut rt, &format!("{KEY}-page-prev"));
    assert_eq!(rt.state.pages, vec![1, 2, 1]);
    assert!(has(&rt, "history-entry-card-p1e0"));
}

#[test]
fn a_zero_total_dialog_has_no_movement_but_still_mounts_the_footer() {
    let scene = base_scene();
    let rt = dialog_rt(scene, (1440.0, 816.0));
    assert!(has(&rt, &format!("{KEY}-pagination")), "footer still present");
    assert_eq!(
        text_of(&rt, &format!("{KEY}-page-label")),
        t("pagination.pageOnly").replace("{page}", "1")
    );
}

// ---- state precedence / honesty ----------------------------------------------

#[test]
fn a_warm_reload_keeps_cards_visible_instead_of_spinning() {
    let mut scene = base_scene();
    scene.vm.loading = true;
    scene.vm.page = single("h0", "c1", true);
    let rt = dialog_rt(scene, (1440.0, 816.0));
    assert!(has(&rt, "history-entry-card-h0"), "rows remain during a refilter");
    assert!(
        !has(&rt, &format!("{KEY}-loading")),
        "spinner is only for a cold, empty list"
    );
}

#[test]
fn an_empty_error_falls_back_to_common_error_and_outranks_loading() {
    let mut scene = base_scene();
    scene.vm.error = Some(String::new());
    scene.vm.loading = true;
    let rt = dialog_rt(scene, (1440.0, 816.0));
    assert!(has(&rt, &format!("{KEY}-error")));
    assert_eq!(element(&rt, &format!("{KEY}-error")).role, "alert");
    assert!(!has(&rt, &format!("{KEY}-loading")), "error outranks loading");
    assert!(
        rt.elements()
            .iter()
            .any(|e| e.text.as_deref() == Some(t("common.error"))),
        "empty error text falls back to common.error"
    );
}

#[test]
fn truncation_is_hidden_when_every_match_is_loaded() {
    let mut scene = base_scene();
    scene.vm.page = mk_page(0, 5, 5); // total == entries.len()
    let rt = dialog_rt(scene, (1440.0, 816.0));
    assert!(
        !has(&rt, &format!("{KEY}-truncation")),
        "no truncated-list notice when nothing is hidden"
    );
}

#[test]
fn an_unknown_connection_id_is_used_as_the_display_name() {
    let mut scene = base_scene();
    scene.vm.page = single("hX", "c-unknown", true);
    let rt = dialog_rt(scene, (1440.0, 816.0));
    assert_eq!(
        text_of(&rt, "history-entry-card-hX-connection"),
        "c-unknown",
        "no connection name crashes into an id fallback"
    );
}

// ---- callback cardinality ----------------------------------------------------

#[test]
fn row_and_bar_callbacks_fire_exactly_once() {
    let mut scene = base_scene();
    scene.vm.page = single("h0", "c1", true);
    scene.vm.selected = vec!["h0".into()];
    let mut rt = dialog_rt(scene, (1440.0, 816.0));

    press(&mut rt, "history-entry-card-h0-action-copy");
    assert_eq!(rt.state.copies, vec!["h0"], "copy callback fires once");

    press(&mut rt, "history-entry-card-h0-action-favorite");
    assert_eq!(
        rt.state.actions
            .iter()
            .filter(|(a, _)| a == "favorite")
            .count(),
        1,
        "favorite action fires once"
    );

    press(&mut rt, &format!("{KEY}-export"));
    assert_eq!(rt.state.exports, 1, "export fires once");
    press(&mut rt, &format!("{KEY}-select-all"));
    assert_eq!(rt.state.toggle_all, 1, "select-all fires once");
}

// ---- source-level invariants --------------------------------------------------

#[test]
fn the_port_reuses_the_shared_ports_and_uses_no_raw_colour_literals() {
    let src = include_str!("../components/history/global_query_history_dialog.rs");
    assert!(!src.contains("oklch("), "no raw oklch in the dialog port");
    assert!(!src.contains("oklcha("), "no raw oklcha in the dialog port");
    assert!(
        src.contains("HistoryFilterBar::<S>::new"),
        "reuses the shared filter bar"
    );
    assert!(
        src.contains("HistoryEntryCard::<S>::new"),
        "reuses the shared entry card"
    );
    assert!(src.contains("Dialog::new("), "builds on the ui::dialog base");
    assert!(
        src.contains("t(\"query.historyTitle\")"),
        "header title goes through i18n"
    );
}

// ---- geometry under load ------------------------------------------------------

#[test]
fn a_busy_dialog_fits_both_windows_in_both_themes() {
    for dark in [true, false] {
        for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
            let mut scene = base_scene();
            scene.dark = dark;
            scene.vm.notice = Some(t("query.historyFavoriteSaved").to_string());
            scene.vm.page = mk_page(0, 42, 5); // truncation notice + pagination footer
            scene.vm.confirm_clear = true; // extra confirm row in the action bar
            scene.vm.page.entries[0].entry.sql = "select a_very_long_column_name from some_schema.some_table where id in (1,2,3); ".repeat(8);
            let rt = dialog_rt(scene, (w, h));
            assert_in_window(&rt, w, h, &format!("busy dark={dark} {w}x{h}"));
            assert_eq!(element(&rt, KEY).role, "dialog");
        }
    }
}
