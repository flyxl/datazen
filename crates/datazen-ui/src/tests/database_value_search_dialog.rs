//! T454: `DatabaseValueSearchDialog` port — keys, validation, scope/type rules,
//! answer surfaces, host actions and geometry at 960×640 / 1440×816 in both
//! palettes. Observations come from the runtime element tree and host events.

use std::collections::BTreeMap;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime, TextEdit};

use crate::views::navigator::database_value_search_dialog::{
    cell_types_for_scope, database_value_search_dialog, mode_from_str, rank_tables, target_key,
    type_scope_from_str, type_scopes_for_driver, with_hits, DIALOG_KEY, LocateIntent,
    SearchProgress, SearchResult, SearchScope, SearchStatus, SearchTarget, StartRequest,
    TableVm, TypeScope, ValueSearchMode, ValueSearchVm,
};

#[derive(Clone, Debug, PartialEq, Eq)]
enum Event {
    Close,
    Start(StartRequest),
    Pause,
    Resume,
    Cancel,
    Locate(LocateIntent),
    Mode(ValueSearchMode),
    Scope(SearchScope),
    Type(TypeScope),
    Toggle(String),
}

#[derive(Clone)]
struct Probe {
    vm: ValueSearchVm,
    dark: bool,
    events: Vec<Event>,
    select_open: BTreeMap<String, bool>,
}

fn table(name: &str, schema: Option<&str>) -> TableVm {
    TableVm {
        name: name.to_owned(),
        schema: schema.map(str::to_owned),
        table_type: "table".to_owned(),
    }
}

fn probe() -> Probe {
    Probe {
        vm: ValueSearchVm {
            open: true,
            db_session_id: "session-1".to_owned(),
            database: "app".to_owned(),
            tables: vec![table("orders", None), table("customers", None)],
            ..ValueSearchVm::default()
        },
        dark: true,
        events: Vec::new(),
        select_open: BTreeMap::new(),
    }
}

fn view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let mut layer = Vec::new();
    let host = crate::views::navigator::database_value_search_dialog::ValueSearchHostPort {
        on_close: Some(Rc::new(|st: &mut Probe| st.events.push(Event::Close))),
        on_start: Some(Rc::new(|st: &mut Probe, req: StartRequest| st.events.push(Event::Start(req)))),
        on_pause: Some(Rc::new(|st: &mut Probe| st.events.push(Event::Pause))),
        on_resume: Some(Rc::new(|st: &mut Probe| st.events.push(Event::Resume))),
        on_cancel: Some(Rc::new(|st: &mut Probe| st.events.push(Event::Cancel))),
        on_locate: Some(Rc::new(|st: &mut Probe, intent: LocateIntent| st.events.push(Event::Locate(intent)))),
        on_mode: Some(Rc::new(|st: &mut Probe, mode| st.events.push(Event::Mode(mode)))),
        on_scope: Some(Rc::new(|st: &mut Probe, scope| st.events.push(Event::Scope(scope)))),
        on_type_scope: Some(Rc::new(|st: &mut Probe, scope| st.events.push(Event::Type(scope)))),
        on_toggle_target: Some(Rc::new(|st: &mut Probe, key| st.events.push(Event::Toggle(key)))),
        value_edit: Some(Rc::new(|st: &mut Probe| &mut st.vm.value)),
        picker_filter_edit: Some(Rc::new(|st: &mut Probe| &mut st.vm.picker_filter)),
        select_open: Some(Rc::new(|st: &mut Probe, key: &str| {
            st.select_open.entry(key.to_owned()).or_insert(false)
        })),
    };
    database_value_search_dialog(&s.vm, s.dark, &mut layer, host);
    rune_kit::root(col().w_full().h_full(), layer)
}

fn runtime(p: Probe, size: (f32, f32)) -> Runtime<Probe> {
    let dark = p.dark;
    let mut rt = Runtime::new(p, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn has<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn texts<S>(rt: &Runtime<S>) -> Vec<String> {
    rt.elements().iter().filter_map(|e| e.text.clone()).collect()
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out before click"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0);
}

#[test]
fn dialog_pins_react_keys_and_closed_state() {
    let rt = runtime(probe(), (1440.0, 816.0));
    for key in [
        DIALOG_KEY,
        "value-search-input",
        "value-search-mode",
        "value-search-start",
        "value-search-scope",
        "value-search-types",
        "value-search-cost-note",
    ] {
        assert!(has(&rt, key), "{key} is mounted");
    }
    let rendered = texts(&rt);
    for key in ["valueSearch.title", "valueSearch.description"] {
        assert!(rendered.iter().any(|text| text == crate::i18n::t(key)), "{key}: exact translated text");
    }
    assert!(element(&rt, "value-search-cost-note").text.as_deref()
        .is_some_and(|text| text.contains("2")), "the cost note counts both tables");

    let mut p = probe();
    p.vm.open = false;
    let closed = runtime(p, (960.0, 640.0));
    assert!(!has(&closed, DIALOG_KEY), "closed dialog mounts nothing");
}

#[test]
fn no_tables_shows_the_honest_note() {
    let mut p = probe();
    p.vm.tables.clear();
    let rt = runtime(p, (960.0, 640.0));
    assert!(has(&rt, "value-search-no-tables"));
    let rendered = texts(&rt);
    assert!(rendered.iter().any(|text| text == crate::i18n::t("valueSearch.noTables")));
}

#[test]
fn start_is_refused_without_a_value_and_reports_scan_inputs() {
    let mut rt = runtime(probe(), (1440.0, 816.0));
    click(&mut rt, "value-search-start");
    assert!(rt.state.events.is_empty(), "blank value sends no scan request");

    let mut p = probe();
    p.vm.value = TextEdit::new("  acme  ".to_owned());
    let mut rt = runtime(p, (1440.0, 816.0));
    click(&mut rt, "value-search-start");
    match rt.state.events.first() {
        Some(Event::Start(request)) => {
            assert_eq!(request.value, "acme", "the value is trimmed");
            assert_eq!(request.database, "app");
            assert_eq!(request.db_session_id, "session-1");
            assert_eq!(request.targets.len(), 2, "both tables are in the plan");
            assert_eq!(request.mode, ValueSearchMode::Contains);
        }
        other => panic!("expected a start request, got {other:?}"),
    }
}

#[test]
fn pure_keymap_and_result_rules_match_react() {
    assert_eq!(mode_from_str("notContains"), ValueSearchMode::NotContains);
    assert_eq!(mode_from_str("bogus"), ValueSearchMode::Contains);
    assert_eq!(type_scope_from_str("text+number"), TypeScope::TextNumber);
    assert_eq!(type_scope_from_str("x"), TypeScope::Text);

    assert_eq!(type_scopes_for_driver(Some("postgresql")), vec![TypeScope::Text]);
    assert_eq!(
        type_scopes_for_driver(Some("mysql")),
        vec![TypeScope::Text, TypeScope::TextNumber, TypeScope::TextDatetime, TypeScope::TextNumberDatetime]
    );
    assert!(cell_types_for_scope(TypeScope::TextNumber).contains("number"));
    assert!(cell_types_for_scope(TypeScope::Text).contains("text"));

    assert_eq!(target_key(&SearchTarget { table: "orders".to_owned(), schema: Some("sales".to_owned()) }), "sales.orders");
    assert_eq!(target_key(&SearchTarget { table: "orders".to_owned(), schema: None }), "orders");

    let results = vec![
        SearchResult { table: "a".into(), schema: None, matches: Some(1), columns: vec![], reason: String::new() },
        SearchResult { table: "b".into(), schema: None, matches: Some(5), columns: vec![], reason: String::new() },
        SearchResult { table: "c".into(), schema: None, matches: Some(0), columns: vec![], reason: String::new() },
        SearchResult { table: "d".into(), schema: None, matches: None, columns: vec![], reason: "denied".into() },
    ];
    let ranked = rank_tables(&results, 3);
    assert_eq!(ranked.iter().map(|r| r.table.as_str()).collect::<Vec<_>>(), ["b", "a"]);
    assert_eq!(with_hits(&results).len(), 2);
}

#[test]
fn selected_scope_shows_schema_qualified_picker_and_counts() {
    let mut p = probe();
    p.vm.tables = vec![table("orders", Some("sales")), table("orders", Some("archive"))];
    p.vm.scope = SearchScope::Selected;
    let mut rt = runtime(p, (1440.0, 816.0));
    assert!(has(&rt, "value-search-table-picker"));
    assert!(has(&rt, "value-search-table-option-sales.orders"));
    assert!(has(&rt, "value-search-table-option-archive.orders"));
    assert!(has(&rt, "value-search-picker-count"));

    click(&mut rt, "value-search-table-option-sales.orders");
    assert_eq!(rt.state.events, [Event::Toggle("sales.orders".to_owned())]);
}

#[test]
fn answers_separate_hits_zero_and_errors_and_carry_the_scan_question() {
    let mut p = probe();
    p.vm.value = TextEdit::new("acme".to_owned());
    p.vm.mode = ValueSearchMode::NotContains;
    p.vm.last_run = Some(("acme".to_owned(), ValueSearchMode::NotContains));
    p.vm.progress = Some(SearchProgress {
        status: SearchStatus::Done,
        completed: 3,
        total: 3,
        results: vec![
            SearchResult { table: "orders".into(), schema: Some("sales".into()), matches: Some(5), columns: vec!["name".into()], reason: String::new() },
            SearchResult { table: "customers".into(), schema: None, matches: Some(0), columns: vec![], reason: String::new() },
            SearchResult { table: "archive".into(), schema: None, matches: None, columns: vec![], reason: "denied".into() },
        ],
        elapsed_ms: Some(1200),
        current: None,
    });
    let mut rt = runtime(p, (1440.0, 816.0));
    assert!(has(&rt, "value-search-results"));
    assert!(has(&rt, "value-search-result-sales.orders"));
    assert!(has(&rt, "value-search-zero-hits-summary"));
    assert!(has(&rt, "value-search-errors-summary"));
    assert!(element(&rt, "value-search-done-note").text.as_deref()
        .is_some_and(|text| text.contains('1')), "the measured seconds are shown");

    click(&mut rt, "value-search-locate-sales.orders");
    match rt.state.events.first() {
        Some(Event::Locate(intent)) => {
            assert_eq!(intent.value, "acme");
            assert_eq!(intent.mode, ValueSearchMode::NotContains);
            assert_eq!(intent.columns, vec!["name"]);
            assert_eq!(intent.schema.as_deref(), Some("sales"));
            assert_eq!(intent.database, "app");
        }
        other => panic!("expected a locate intent, got {other:?}"),
    }
}

#[test]
fn running_scan_offers_pause_and_cancel() {
    let mut p = probe();
    p.vm.progress = Some(SearchProgress {
        status: SearchStatus::Running,
        completed: 1,
        total: 2,
        results: vec![],
        elapsed_ms: None,
        current: Some("orders".to_owned()),
    });
    let mut rt = runtime(p, (1440.0, 816.0));
    assert!(has(&rt, "value-search-pause"));
    assert!(has(&rt, "value-search-cancel"));
    assert!(has(&rt, "value-search-progress"));
    click(&mut rt, "value-search-pause");
    assert_eq!(rt.state.events, [Event::Pause]);
}

#[test]
fn dialog_fits_both_windows_in_both_palettes() {
    for dark in [true, false] {
        for size in [(960.0, 640.0), (1440.0, 816.0)] {
            let mut p = probe();
            p.dark = dark;
            p.vm.scope = SearchScope::Selected;
            p.vm.type_scope = TypeScope::TextNumber;
            p.vm.last_run = Some(("acme".into(), ValueSearchMode::Contains));
            p.vm.progress = Some(SearchProgress {
                status: SearchStatus::Done,
                completed: 2,
                total: 2,
                results: vec![
                    SearchResult { table: "orders".into(), schema: Some("sales".into()), matches: Some(5), columns: vec!["name".into()], reason: String::new() },
                    SearchResult { table: "customers".into(), schema: None, matches: Some(0), columns: vec![], reason: String::new() },
                ],
                elapsed_ms: Some(1000),
                current: None,
            });
            let rt = runtime(p, size);
            let outside: Vec<String> = rt.elements().iter()
                .filter(|e| {
                    let r = e.rect;
                    r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > size.0 + 0.5 || r[1] + r[3] > size.1 + 0.5
                })
                .map(|e| format!("{} {:?}", e.key.as_deref().unwrap_or(&e.path), e.rect))
                .collect();
            assert!(outside.is_empty(), "dark={dark} {size:?}: {} nodes outside: {outside:?}", outside.len());
        }
    }
}
