//! T454 independent audit: `DatabaseValueSearchDialog` vs React + its test.
//!
//! Probes branches the coder's suite does not: Enter-to-start on the value
//! input, rounded measured seconds, paused/resume, picker filtering and busy
//! disabling. Observations come from the runtime tree and host events.

use std::collections::BTreeMap;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Key, KeyEvent, Mods, Runtime, TextEdit};

use crate::views::navigator::database_value_search_dialog::{
    database_value_search_dialog, LocateIntent, SearchProgress, SearchResult, SearchScope,
    SearchStatus, SearchTarget, StartRequest, ValueSearchHostPort, ValueSearchMode, ValueSearchVm,
};

#[derive(Clone, Debug, PartialEq, Eq)]
enum Event {
    Start(StartRequest),
    Pause,
    Resume,
    Cancel,
    Locate(LocateIntent),
    Toggle(String),
}

#[derive(Clone)]
struct Probe {
    vm: ValueSearchVm,
    dark: bool,
    events: Vec<Event>,
}

fn table(name: &str, schema: Option<&str>) -> crate::views::navigator::database_value_search_dialog::TableVm {
    crate::views::navigator::database_value_search_dialog::TableVm {
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
    }
}

fn view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let mut layer = Vec::new();
    let host = ValueSearchHostPort {
        on_start: Some(Rc::new(|st: &mut Probe, r: StartRequest| st.events.push(Event::Start(r)))),
        on_pause: Some(Rc::new(|st: &mut Probe| st.events.push(Event::Pause))),
        on_resume: Some(Rc::new(|st: &mut Probe| st.events.push(Event::Resume))),
        on_cancel: Some(Rc::new(|st: &mut Probe| st.events.push(Event::Cancel))),
        on_locate: Some(Rc::new(|st: &mut Probe, i: LocateIntent| st.events.push(Event::Locate(i)))),
        on_toggle_target: Some(Rc::new(|st: &mut Probe, k: String| st.events.push(Event::Toggle(k)))),
        value_edit: Some(Rc::new(|st: &mut Probe| &mut st.vm.value)),
        picker_filter_edit: Some(Rc::new(|st: &mut Probe| &mut st.vm.picker_filter)),
        ..ValueSearchHostPort::default()
    };
    database_value_search_dialog(&s.vm, s.dark, &mut layer, host);
    rune_kit::root(col().w_full().h_full(), layer)
}

fn runtime(p: Probe) -> Runtime<Probe> {
    let dark = p.dark;
    let mut rt = Runtime::new(p, view, (1440.0, 816.0), Env { system_dark: dark, scale: 1.0 });
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

fn press_enter(rt: &mut Runtime<Probe>) {
    rt.key(KeyEvent { key: Key::Enter, mods: Mods::default() });
    rt.frame(1.0);
}

/// React's value input starts the scan on Enter (onKeyDown); the port must
/// route the same keyboard intent to the host's on_start port.
#[test]
fn enter_in_value_starts_the_scan() {
    let mut p = probe();
    p.vm.value = TextEdit::new("acme".to_owned());
    let mut rt = runtime(p);
    let (x, y) = rt.find("value-search-input").unwrap_or_else(|| panic!("input laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    press_enter(&mut rt);
    assert!(
        rt.state.events.iter().any(|e| matches!(e, Event::Start(_))),
        "Enter on the value input must start the scan; events {:?}",
        rt.state.events
    );
}

/// React rounds elapsed ms to the nearest second (`Math.round`) before the
/// note; a floor would under-report a 1.5s scan as 1s.
#[test]
fn done_note_rounds_the_measured_seconds() {
    let mut p = probe();
    p.vm.progress = Some(SearchProgress {
        status: SearchStatus::Done,
        completed: 1,
        total: 1,
        results: Vec::new(),
        elapsed_ms: Some(1500),
        current: None,
    });
    let rt = runtime(p);
    let note = element(&rt, "value-search-done-note");
    let text = note.text.as_deref().unwrap_or("");
    assert!(
        text.contains('2'),
        "1.5s must round to 2 seconds, not floor to 1; note {text:?}"
    );
}

/// A paused scan offers Resume and a running one offers Pause; both keep the
/// Cancel affordance (the coder's suite only pins pause).
#[test]
fn paused_scan_offers_resume() {
    let mut p = probe();
    p.vm.progress = Some(SearchProgress {
        status: SearchStatus::Paused,
        completed: 1,
        total: 2,
        results: Vec::new(),
        elapsed_ms: None,
        current: None,
    });
    let mut rt = runtime(p);
    assert!(has(&rt, "value-search-resume"));
    assert!(!has(&rt, "value-search-pause"));
    assert!(has(&rt, "value-search-cancel"));
    let (x, y) = rt.find("value-search-resume").unwrap_or_else(|| panic!("resume laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0);
    assert_eq!(rt.state.events, [Event::Resume]);
}

/// The picker narrows by schema-qualified key; matching is case-insensitive.
#[test]
fn picker_filter_narrows_by_qualified_key() {
    let mut p = probe();
    p.vm.tables = vec![table("orders", Some("sales")), table("customers", Some("archive"))];
    p.vm.scope = SearchScope::Selected;
    p.vm.picker_filter = TextEdit::new("CUST".to_owned());
    let rt = runtime(p);
    assert!(!has(&rt, "value-search-table-option-sales.orders"));
    assert!(has(&rt, "value-search-table-option-archive.customers"));
}

/// While a scan runs the value input and the selectors are not editable.
#[test]
fn busy_scan_disables_the_value_input() {
    let mut p = probe();
    p.vm.progress = Some(SearchProgress {
        status: SearchStatus::Running,
        completed: 0,
        total: 2,
        results: Vec::new(),
        elapsed_ms: None,
        current: Some("orders".to_owned()),
    });
    let rt = runtime(p);
    assert!(!element(&rt, "value-search-input").focusable, "value input disabled while busy");
    assert!(!element(&rt, "value-search-mode").focusable, "mode select disabled while busy");
    assert!(has(&rt, "value-search-cancel"));
}
