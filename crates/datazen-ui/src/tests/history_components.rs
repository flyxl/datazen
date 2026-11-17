//! T029: history components (HistoryEntryCard + HistoryFilterBar).

use rune::{col, Cx, El, Env, Key, KeyEvent, Mods, Runtime, TextEdit, Tx};

use crate::components::history::{
    HistoryConnectionOption, HistoryEntryAction, HistoryEntryCard, HistoryEntryVm,
    HistoryFilterBar, HistoryFilterOpen, HistoryQueryState,
};
use crate::i18n::t;

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn assert_in_window<S>(rt: &Runtime<S>, w: f32, h: f32, label: &str) {
    let mut outside = Vec::new();
    for e in rt.elements() {
        let r = e.rect;
        if r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > w + 0.5 || r[1] + r[3] > h + 0.5 {
            outside.push(format!("{} {:?}", e.key.unwrap_or(e.path), r));
        }
    }
    assert!(
        outside.is_empty(),
        "{label}: {} node(s) out of window {w}x{h}: {outside:?}",
        outside.len()
    );
}

fn sample_entry() -> HistoryEntryVm {
    HistoryEntryVm {
        id: "h1".to_string(),
        success: true,
        database: Some("appdb".to_string()),
        schema: Some("public".to_string()),
        execution_time_ms: 42,
        rows_affected: Some(7),
        executed_at: "2026-11-17 10:00:00".to_string(),
        sql: "select * from users;".to_string(),
        error_message: None,
    }
}

// ---- HistoryEntryCard --------------------------------------------------------

#[derive(Clone, Default)]
struct CardState {
    toggled: Vec<String>,
    actions: Vec<(String, String)>, // (action, id)
    copied_id: Option<String>,
    copy_calls: usize,
}

fn card_view(state: &CardState, _cx: &mut Cx<CardState>) -> El {
    let entry = sample_entry();
    let copied = state.copied_id.as_deref() == Some("h1");
    col().w_full().h_full().padding(16.0).child(
        HistoryEntryCard::<CardState>::new(entry, "Local PG")
            .selected(false)
            .copied(copied)
            .can_open(true)
            .on_toggle_select(|s: &mut CardState, id| s.toggled.push(id))
            .on_action(|s: &mut CardState, action, id| {
                s.actions.push((action.as_str().to_string(), id))
            })
            .on_copy(|s: &mut CardState, id, _tx: &mut Tx| {
                s.copied_id = Some(id);
                s.copy_calls += 1;
            })
            .build(true),
    )
}

fn card_selected_error_view(_state: &CardState, _cx: &mut Cx<CardState>) -> El {
    let mut entry = sample_entry();
    entry.success = false;
    entry.error_message = Some("boom".to_string());
    col().w_full().h_full().padding(16.0).child(
        HistoryEntryCard::<CardState>::new(entry, "Local PG")
            .selected(true)
            .copied(false)
            .can_open(false)
            .build(true),
    )
}

fn card_long_sql_view(_state: &CardState, _cx: &mut Cx<CardState>) -> El {
    let mut entry = sample_entry();
    entry.sql = "select ".to_string() + &"very_long_identifier_".repeat(40) + " from t;";
    col().w_full().h_full().padding(16.0).child(
        HistoryEntryCard::<CardState>::new(entry, "Local PG")
            .selected(false)
            .copied(false)
            .can_open(true)
            .build(true),
    )
}

#[test]
fn history_entry_card_renders_expected_keys_and_meta() {
    let mut rt = Runtime::new(CardState::default(), card_view, (960.0, 640.0), env(true));
    rt.frame(0.0);
    for key in [
        "history-entry-card-h1",
        "history-entry-card-h1-header",
        "history-entry-card-h1-meta",
        "history-entry-card-h1-select",
        "history-entry-card-h1-status",
        "history-entry-card-h1-connection",
        "history-entry-card-h1-database",
        "history-entry-card-h1-schema",
        "history-entry-card-h1-time",
        "history-entry-card-h1-rows",
        "history-entry-card-h1-executed-at",
        "history-entry-card-h1-actions",
        "history-entry-card-h1-action-copy",
        "history-entry-card-h1-action-favorite",
        "history-entry-card-h1-action-open",
        "history-entry-card-h1-action-delete",
        "history-entry-card-h1-sql",
    ] {
        assert!(has_key(&rt, key), "expected key present: {key}");
    }
    assert_eq!(
        element(&rt, "history-entry-card-h1-connection")
            .text
            .as_deref(),
        Some("Local PG")
    );
    assert_eq!(
        element(&rt, "history-entry-card-h1-database")
            .text
            .as_deref(),
        Some("appdb")
    );
    assert_eq!(
        element(&rt, "history-entry-card-h1-schema").text.as_deref(),
        Some("public")
    );
    assert_eq!(
        element(&rt, "history-entry-card-h1-sql-text")
            .text
            .as_deref(),
        Some("select * from users;"),
        "sql text rendered"
    );
    assert!(
        !has_key(&rt, "history-entry-card-h1-error"),
        "no error block"
    );
}

#[test]
fn history_entry_card_copy_button_copies_and_shows_copied_label() {
    let clipboard = rune::clipboard::memory();
    let mut rt = Runtime::new(CardState::default(), card_view, (960.0, 640.0), env(true));
    rt.use_clipboard(&clipboard);
    rt.frame(0.0);

    click(&mut rt, "history-entry-card-h1-action-copy");
    assert_eq!(
        clipboard.text().as_deref().map(String::as_str),
        Some("select * from users;"),
        "exact SQL copied to clipboard"
    );
    assert_eq!(rt.state.copy_calls, 1, "on_copy callback reached");
    assert_eq!(rt.state.copied_id.as_deref(), Some("h1"));

    // label becomes copied feedback (prop-driven; on_copy updated state)
    rt.frame(0.0);
    let copied_label = t("common.copied");
    // action button carries copied label text in its subtree
    let has_copied = rt
        .elements()
        .iter()
        .any(|e| e.text.as_deref() == Some(copied_label));
    assert!(
        has_copied,
        "copied feedback label present after copy: expected text {copied_label:?}"
    );
}

#[test]
fn history_entry_card_geometry_dark_light_and_sizes() {
    for dark in [true, false] {
        for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
            let mut rt = Runtime::new(CardState::default(), card_view, (w, h), env(dark));
            rt.frame(0.0);
            assert_in_window(&rt, w, h, &format!("card dark={dark} {w}x{h}"));
        }
    }
    // selected + error variant and no-open variant
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        let mut rt = Runtime::new(
            CardState::default(),
            card_selected_error_view,
            (w, h),
            env(true),
        );
        rt.frame(0.0);
        assert_in_window(&rt, w, h, &format!("card selected/error {w}x{h}"));
        assert!(
            has_key(&rt, "history-entry-card-h1-error"),
            "error block present"
        );
        assert!(
            !has_key(&rt, "history-entry-card-h1-action-open"),
            "open action hidden when can_open is false"
        );
    }
}

#[test]
fn history_entry_card_long_sql_wraps_and_stays_in_window() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        let mut rt = Runtime::new(CardState::default(), card_long_sql_view, (w, h), env(true));
        rt.frame(0.0);
        assert_in_window(&rt, w, h, &format!("card long sql {w}x{h}"));
        let sql = element(&rt, "history-entry-card-h1-sql");
        assert!(
            sql.rect[3] <= 120.0,
            "sql block bounded by max height: {:?}",
            sql.rect
        );
    }
}

// ---- HistoryFilterBar --------------------------------------------------------

#[derive(Clone)]
struct FilterState {
    query: HistoryQueryState,
    search: TextEdit,
    open: HistoryFilterOpen,
    reset_calls: usize,
}

impl Default for FilterState {
    fn default() -> Self {
        Self {
            query: HistoryQueryState::default(),
            search: TextEdit::new(String::new()),
            open: HistoryFilterOpen::None,
            reset_calls: 0,
        }
    }
}

fn filter_view(state: &FilterState, _cx: &mut Cx<FilterState>) -> El {
    let mut layer = Vec::new();
    let bar = HistoryFilterBar::<FilterState>::new(state.query.clone(), &state.search)
        .connections(vec![
            HistoryConnectionOption {
                id: "c1".to_string(),
                name: "Conn A".to_string(),
            },
            HistoryConnectionOption {
                id: "c2".to_string(),
                name: "Conn B".to_string(),
            },
        ])
        .databases(vec!["db1".to_string(), "db2".to_string()])
        .schemas(vec!["public".to_string()])
        .open(state.open)
        .on_search_edit(|s: &mut FilterState| &mut s.search)
        .on_set_connection(|s: &mut FilterState, id| {
            s.query.connection_id = id;
            s.query.database.clear();
            s.query.schema.clear();
        })
        .on_toggle_open(|s: &mut FilterState, which| s.open = which)
        .on_close(|s: &mut FilterState| s.open = HistoryFilterOpen::None)
        .on_reset(|s: &mut FilterState| {
            s.query = HistoryQueryState::default();
            s.reset_calls += 1;
        })
        .build(true, &mut layer);
    let mut root = col().w_full().h_full().padding(16.0).child(bar);
    for l in layer {
        root = root.child(l);
    }
    root
}

#[test]
fn history_filter_bar_renders_search_and_select_keys() {
    let mut rt = Runtime::new(
        FilterState::default(),
        filter_view,
        (960.0, 640.0),
        env(true),
    );
    rt.frame(0.0);
    for key in [
        "history-filter-bar",
        "history-filter-bar-search",
        "history-filter-bar-connection-trigger",
        "history-filter-bar-status-trigger",
        "history-filter-bar-range-trigger",
        "history-filter-bar-sort-trigger",
    ] {
        assert!(has_key(&rt, key), "expected key present: {key}");
    }
    // connection is "all" so db/schema hidden; reset hidden in default state
    assert!(
        !has_key(&rt, "history-filter-bar-database"),
        "database hidden when connection all"
    );
    assert!(
        !has_key(&rt, "history-filter-bar-schema"),
        "schema hidden when connection all"
    );
    assert!(
        !has_key(&rt, "history-filter-bar-reset-filters"),
        "reset hidden in default state"
    );
}

#[test]
fn history_filter_bar_search_input_is_focusable_and_accepts_text() {
    let mut rt = Runtime::new(
        FilterState::default(),
        filter_view,
        (960.0, 640.0),
        env(true),
    );
    rt.frame(0.0);
    let (x, y) = rt
        .find("history-filter-bar-search")
        .expect("search input is laid out");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);

    rt.key(KeyEvent {
        key: Key::Text("abc".into()),
        mods: Mods::default(),
    });
    rt.frame(1.0 / 60.0);
    assert!(
        rt.state.search.text.contains("abc"),
        "typed text enters TextEdit: {:?}",
        &rt.state.search.text
    );
}

#[test]
fn history_filter_bar_geometry_dark_light_and_sizes() {
    for dark in [true, false] {
        for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
            let mut rt = Runtime::new(FilterState::default(), filter_view, (w, h), env(dark));
            rt.frame(0.0);
            assert_in_window(&rt, w, h, &format!("filter bar dark={dark} {w}x{h}"));
        }
    }
}
