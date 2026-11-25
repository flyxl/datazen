//! T427: `RecentQueriesList` port — real branch tests.
//!
//! Pinned values come from the three React home suites: five-row cap, source
//! fallback, keyboard/mouse activation, copy feedback, relative-time labels,
//! empty state and the preserved home keys.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Key, KeyEvent, Runtime};

use datazen_ui::i18n::t;
use datazen_ui::views::home::recent_queries_list::{
    build, relative_label, row_activates, QueryHistoryVm, RecentQueriesHostPort, RecentQueriesVm,
    SavedConnectionVm, CARD_KEY, EMPTY_KEY, ROOT_KEY, TITLE_KEY, VIEW_ALL_KEY,
};

#[derive(Default)]
struct Probe {
    vm: RecentQueriesVm,
    dark: bool,
    calls: Rc<RefCell<Vec<String>>>,
}

fn entry(id: &str, sql: &str, offset_ms: i64) -> QueryHistoryVm {
    QueryHistoryVm {
        id: id.to_owned(),
        connection_id: "conn-1".to_owned(),
        database: "postgres".to_owned(),
        sql: sql.to_owned(),
        executed_at_ms: 1_700_000_000_000 - offset_ms,
        execution_time_ms: 12,
        success: true,
    }
}

fn saved() -> Vec<SavedConnectionVm> {
    vec![SavedConnectionVm {
        id: "conn-1".to_owned(),
        name: "Local".to_owned(),
    }]
}

fn view(probe: &Probe, _cx: &mut Cx<Probe>) -> El {
    let calls = probe.calls.clone();
    let on_select = Rc::new(move |_s: &mut Probe, id: &str| {
        calls.borrow_mut().push(format!("select:{id}"));
    });
    let calls = probe.calls.clone();
    let on_open = Rc::new(move |_s: &mut Probe| {
        calls.borrow_mut().push("open-history".to_owned());
    });
    let calls = probe.calls.clone();
    let on_copy = Rc::new(move |_s: &mut Probe, id: &str, sql: &str| {
        calls.borrow_mut().push(format!("copy:{id}:{sql}"));
    });
    let calls = probe.calls.clone();
    let on_copied = Rc::new(move |_s: &mut Probe, id: Option<String>, on: bool| {
        calls.borrow_mut().push(format!("copied:{id:?}:{on}"));
    });
    let mut layer = Vec::new();
    build(
        &probe.vm,
        probe.dark,
        RecentQueriesHostPort {
            on_select: Some(on_select),
            on_open_history: Some(on_open),
            on_copy: Some(on_copy),
            on_copied_change: Some(on_copied),
        },
        &mut layer,
    );
    col().w_full().h_full().children(layer)
}

fn runtime(probe: Probe, size: (f32, f32)) -> Runtime<Probe> {
    let dark = probe.dark;
    Runtime::new(probe, view, size, Env { system_dark: dark, scale: 1.0 })
}

fn has_key(rt: &Runtime<Probe>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn has_text(rt: &Runtime<Probe>, needle: &str) -> bool {
    rt.elements().iter().filter_map(|e| e.text.clone()).any(|text| text.contains(needle))
}

fn click(rt: &mut Runtime<Probe>, key: &str) {
    rt.frame(0.0);
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
}

#[test]
fn relative_labels_follow_the_js_buckets_and_omit_invalid_timestamps() {
    let now = 1_700_000_000_000;
    assert_eq!(relative_label(0, now), None);
    assert_eq!(relative_label(-1, now), None);
    assert_eq!(
        relative_label(now - 10_000, now),
        Some(t("connWin.home.queries.justNow").to_owned())
    );
    assert_eq!(
        relative_label(now - 2 * 60_000, now),
        Some(t("connWin.home.queries.minutesAgo").replace("{count}", "2"))
    );
    assert_eq!(
        relative_label(now - 3 * 3_600_000, now),
        Some(t("connWin.home.queries.hoursAgo").replace("{count}", "3"))
    );
    assert_eq!(
        relative_label(now - 2 * 86_400_000, now),
        Some(t("connWin.home.queries.daysAgo").replace("{count}", "2"))
    );
    let old = relative_label(now - 8 * 86_400_000, now).expect("beyond 7 days is a date");
    assert!(!old.contains("connWin."), "no i18n key leakage: {old}");
    assert!(old.contains('-'), "a YYYY-MM-DD date: {old}");
}

#[test]
fn source_fallback_and_five_row_cap() {
    let mut vm = RecentQueriesVm {
        saved_connections: saved(),
        now_ms: 1_700_000_000_000,
        ..Default::default()
    };
    let known = entry("a", "SELECT 1;", 10_000);
    assert_eq!(vm.source_name(&known), "Local");
    let unknown = QueryHistoryVm { connection_id: "ghost".to_owned(), ..known.clone() };
    assert_eq!(vm.source_name(&unknown), "postgres", "falls back to the database");
    let no_db = QueryHistoryVm { database: String::new(), ..unknown };
    assert_eq!(vm.source_name(&no_db), "default");

    vm.entries = (0..8).map(|i| entry(&format!("h-{i}"), &format!("SELECT {i};"), 10_000)).collect();
    assert_eq!(vm.visible_entries().len(), 5);
    assert_eq!(vm.visible_entries()[0].id, "h-0");
    assert_eq!(vm.visible_entries()[4].id, "h-4");
    assert!(!vm.visible_entries().iter().any(|e| e.id == "h-5"));
}

#[test]
fn copied_marker_is_gated_on_both_flags() {
    let mut vm = RecentQueriesVm {
        copied_sql_id: Some("h-1".to_owned()),
        ..Default::default()
    };
    assert_eq!(vm.copied_row_id(), None, "shared flag off means no marker");
    vm.copied = true;
    assert_eq!(vm.copied_row_id(), Some("h-1"));
}

#[test]
fn section_keys_rows_and_actions_are_wired() {
    let mut probe = Probe::default();
    probe.vm.entries = vec![entry("hist-1", "SELECT 1;", 2 * 60_000)];
    probe.vm.saved_connections = saved();
    probe.vm.now_ms = 1_700_000_000_000;
    let mut rt = runtime(probe, (1440.0, 816.0));
    rt.frame(0.0);

    for key in [
        ROOT_KEY,
        "home-recent-queries-section",
        TITLE_KEY,
        VIEW_ALL_KEY,
        "view-all-history-button",
        CARD_KEY,
        "connections-recent-row-hist-1",
        "home-query-source-hist-1",
        "home-query-rerun-hist-1",
        "home-query-copy-hist-1",
        "home-query-sql-hist-1",
    ] {
        assert!(has_key(&rt, key), "composed key missing: {key}");
    }
    assert!(has_text(&rt, "SELECT 1;"));
    assert!(has_text(&rt, "Local"));
    assert!(has_text(&rt, "12ms"));
    assert!(has_text(&rt, &t("connWin.home.queries.minutesAgo").replace("{count}", "2")));

    click(&mut rt, "home-query-copy-hist-1");
    click(&mut rt, "home-query-rerun-hist-1");
    click(&mut rt, VIEW_ALL_KEY);
    click(&mut rt, "connections-recent-row-hist-1");
    assert_eq!(
        rt.state.calls.borrow().as_slice(),
        [
            "copy:hist-1:SELECT 1;",
            "copied:Some(\"hist-1\"):true",
            "select:hist-1",
            "open-history",
            "select:hist-1",
        ],
        "actions reach the host port without leaking into row activation"
    );
}

#[test]
fn keyboard_activation_matches_enter_and_space_only() {
    for (key, expected) in [
        (Key::Enter, true),
        (Key::Text(" ".to_owned()), true),
        (Key::Escape, false),
        (Key::Text("x".to_owned()), false),
    ] {
        assert_eq!(
            row_activates(&KeyEvent { key, mods: rune::Mods::default() }),
            expected,
            "row activation matches only Enter/Space"
        );
    }
}

#[test]
fn enter_and_space_activate_the_focused_history_row() {
    let mut probe = Probe::default();
    probe.vm.entries = vec![entry("hist-1", "SELECT 1;", 10_000)];
    probe.vm.saved_connections = saved();
    probe.vm.now_ms = 1_700_000_000_000;
    let mut rt = runtime(probe, (960.0, 640.0));
    rt.frame(0.0);
    click(&mut rt, "connections-recent-row-hist-1");
    assert_eq!(rt.state.calls.borrow().as_slice(), ["select:hist-1"]);

    rt.key(KeyEvent { key: Key::Enter, mods: rune::Mods::default() });
    rt.frame(0.0);
    assert_eq!(
        rt.state.calls.borrow().as_slice(),
        ["select:hist-1", "select:hist-1"],
        "React pins Enter activation on the focused row"
    );

    rt.key(KeyEvent { key: Key::Text(" ".to_owned()), mods: rune::Mods::default() });
    rt.frame(0.0);
    assert_eq!(
        rt.state.calls.borrow().as_slice(),
        ["select:hist-1", "select:hist-1", "select:hist-1"],
        "React pins Space activation on the focused row"
    );
}

#[test]
fn empty_and_loading_states_keep_the_pinned_keys() {
    let mut rt = runtime(Probe::default(), (960.0, 640.0));
    rt.frame(0.0);
    assert!(has_key(&rt, EMPTY_KEY));
    assert!(has_key(&rt, VIEW_ALL_KEY), "view-all is present even when empty");
    assert!(has_text(&rt, &t("connWin.home.noRecentQueries")));
    assert!(!has_key(&rt, "connections-recent-row-hist-1"));

    rt.state.vm.loading = true;
    rt.invalidate();
    rt.frame(0.0);
    assert!(has_key(&rt, "connections-recent-loading-spinner"));
    assert!(has_text(&rt, &t("common.loading")));
}

#[test]
fn section_fits_both_windows_in_both_themes() {
    for (width, height) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let mut probe = Probe::default();
            probe.dark = dark;
            probe.vm.entries = (0..5).map(|i| entry(&format!("h-{i}"), "SELECT 1;", 10_000)).collect();
            probe.vm.saved_connections = saved();
            probe.vm.now_ms = 1_700_000_000_000;
            let mut rt = runtime(probe, (width, height));
            rt.frame(0.0);
            assert!(has_key(&rt, ROOT_KEY), "section mounted at {width}x{height} dark={dark}");
            for element in rt.elements() {
                let [x, _, w, _] = element.rect;
                assert!(
                    x >= -0.5 && x + w <= width + 0.5,
                    "{width}x{height} dark={dark}: {:?} {:?} overflows horizontally",
                    element.key,
                    element.rect
                );
            }
        }
    }
}
