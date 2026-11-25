//! T409: `ConnectionCardList` port — keys, card/meta/status rendering, sorting,
//! filter and paging, host callbacks, empty/loading states and geometry at
//! 960×640 / 1440×816 in both palettes.

use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime, TextEdit};

use datazen_ui::i18n::t;
use datazen_ui::views::home::connection_card_list::{
    connection_card_list, filtered_connections, host_port_label, sorted_connections,
    CardConnectionStatus, ConnectionCardListHostPort, ConnectionCardListVm, ConnectionCardVm,
    COLLAPSED_COUNT,
};

#[derive(Clone, Debug, PartialEq, Eq)]
enum Event {
    Connect(String),
    NewConnection,
    Import,
    Filter(String),
    Expand(bool),
}

#[derive(Clone)]
struct Probe {
    vm: ConnectionCardListVm,
    dark: bool,
    events: Vec<Event>,
}

fn conn(id: &str, name: &str, engine: &str) -> ConnectionCardVm {
    ConnectionCardVm {
        id: id.to_owned(),
        name: name.to_owned(),
        engine: engine.to_owned(),
        host: Some(format!("{engine}.internal")),
        port: Some("5432".to_owned()),
        database: Some("app".to_owned()),
        group: Some("prod".to_owned()),
        pinned: false,
        last_connected_at: None,
    }
}

fn probe(count: usize) -> Probe {
    Probe {
        vm: ConnectionCardListVm {
            connections: (0..count)
                .map(|i| conn(&format!("conn-{i}"), &format!("DB {i}"), "postgresql"))
                .collect(),
            active: Default::default(),
            filter: TextEdit::new(String::new()),
            expanded: false,
            can_import: true,
            loading: false,
        },
        dark: true,
        events: Vec::new(),
    }
}

fn view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let host = ConnectionCardListHostPort {
        on_connect: Some(Rc::new(|st: &mut Probe, id: String| st.events.push(Event::Connect(id)))),
        on_new_connection: Some(Rc::new(|st: &mut Probe| st.events.push(Event::NewConnection))),
        on_import: Some(Rc::new(|st: &mut Probe| st.events.push(Event::Import))),
        on_filter: Some(Rc::new(|st: &mut Probe, value: String| {
            st.events.push(Event::Filter(value))
        })),
        on_expand: Some(Rc::new(|st: &mut Probe, value: bool| st.events.push(Event::Expand(value)))),
        filter_edit: Some(Rc::new(|st: &mut Probe| &mut st.vm.filter)),
    };
    col().w_full().h_full().child(connection_card_list(&s.vm, s.dark, host))
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

fn type_filter<S>(rt: &mut Runtime<S>, text: &str) {
    let (x, y) = rt.find("home-connections-filter").unwrap_or_else(|| panic!("filter is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.paste(text);
    rt.frame(0.0);
}

#[test]
fn section_and_card_keys_are_pinned() {
    let mut p = probe(2);
    p.vm.active.insert("conn-0".to_owned(), CardConnectionStatus::Connected);
    p.vm.active.insert("conn-1".to_owned(), CardConnectionStatus::Connecting);
    let rt = runtime(p, (1440.0, 816.0));
    for key in [
        "home-connections-section",
        "home-connections-filter",
        "connection-conn-0",
        "home-conn-card-conn-0",
        "conn-chip-conn-0",
        "conn-meta-conn-0",
        "conn-status-conn-0",
        "conn-open-conn-0",
        "home-conn-connect-conn-0",
        "connection-new",
        "empty-new-connection-button",
        "import-connections",
        "empty-import-connections-button",
    ] {
        assert!(has(&rt, key), "{key} is mounted");
    }
    let rendered = texts(&rt);
    for key in [
        "connWin.home.yourConnections",
        "connWin.home.status.connected",
        "conn.connecting",
        "common.newConnection",
        "common.importConnections",
    ] {
        assert!(rendered.iter().any(|text| text == t(key)), "{key}: exact translated text");
    }
    assert_eq!(
        element(&rt, "conn-meta-conn-0").text.as_deref(),
        Some("postgresql.internal:5432 · prod"),
        "host:port and group metadata"
    );
}

#[test]
fn sorting_and_filter_follow_react_rules() {
    let mut pinned = conn("p", "Pinned", "postgresql");
    pinned.pinned = true;
    let mut recent = conn("r", "Recent", "mysql");
    recent.last_connected_at = Some("2026-01-02".to_owned());
    let mut older = conn("o", "Older", "sqlite");
    older.last_connected_at = Some("2026-01-01".to_owned());
    let plain = conn("z", "Zulu", "postgres");
    let sorted = sorted_connections(&[plain.clone(), older.clone(), recent.clone(), pinned.clone()]);
    let ids: Vec<&str> = sorted.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, ["p", "r", "o", "z"], "pinned, then recency, then name");

    let needle = filtered_connections(&sorted, "mysql");
    assert_eq!(needle.len(), 1, "engine label matches the filter");
    assert_eq!(needle[0].id, "r");
    let mut no_host = conn("x", "No host", "redis");
    no_host.host = None;
    no_host.database = None;
    assert_eq!(host_port_label(&no_host), "redis", "engine fallback");
}

#[test]
fn connection_new_import_and_card_actions_reach_the_host() {
    let mut rt = runtime(probe(1), (960.0, 640.0));
    click(&mut rt, "connection-conn-0");
    assert_eq!(rt.state.events, [Event::Connect("conn-0".to_owned())]);

    let mut rt = runtime(probe(1), (960.0, 640.0));
    click(&mut rt, "conn-open-conn-0");
    assert_eq!(rt.state.events, [Event::Connect("conn-0".to_owned())]);

    let mut rt = runtime(probe(1), (960.0, 640.0));
    click(&mut rt, "connection-new");
    click(&mut rt, "import-connections");
    assert_eq!(rt.state.events, [Event::NewConnection, Event::Import]);
}

#[test]
fn filter_and_clear_dispatch_and_empty_state_is_localized() {
    let mut rt = runtime(probe(2), (1440.0, 816.0));
    type_filter(&mut rt, "mysql");
    assert_eq!(rt.state.vm.filter.text, "mysql", "the host owns the filter text");
    // Keystrokes land through the injected TextEdit accessor; the explicit
    // filter callback is reported by controls such as the clear affordance.

    let mut rt = runtime(probe(1), (960.0, 640.0));
    rt.state.vm.filter = TextEdit::new("zzz-no-match".to_owned());
    rt.invalidate();
    rt.frame(1.0);
    assert!(has(&rt, "home-connections-no-match"));
    assert!(has(&rt, "home-connections-filter-clear"));
    let rendered = texts(&rt);
    assert!(rendered.iter().any(|text| text.contains("zzz-no-match")), "the query is disclosed");
    click(&mut rt, "home-connections-filter-clear");
    assert_eq!(rt.state.events, [Event::Filter(String::new())]);
}

#[test]
fn six_card_page_expands_and_collapses() {
    assert_eq!(COLLAPSED_COUNT, 6);
    let mut rt = runtime(probe(7), (1440.0, 816.0));
    let visible = |rt: &Runtime<Probe>| {
        (0..7).filter(|i| has(rt, &format!("connection-conn-{i}"))).count()
    };
    assert_eq!(visible(&rt), 6, "six cards before expanding");
    assert!(has(&rt, "connections-show-all"));
    assert!(has(&rt, "home-connections-show-all"));
    click(&mut rt, "connections-show-all");
    assert_eq!(rt.state.events, [Event::Expand(true)], "show-all requests expansion");

    let mut p = probe(7);
    p.vm.expanded = true;
    let expanded = runtime(p, (1440.0, 816.0));
    assert_eq!(visible(&expanded), 7, "expanded shows all cards");
    assert!(texts(&expanded).iter().any(|text| text == t("connWin.home.connections.showLess")));
}

#[test]
fn loading_state_is_honest() {
    let mut p = probe(1);
    p.vm.loading = true;
    let rt = runtime(p, (960.0, 640.0));
    assert!(has(&rt, "connections-loading"));
    assert!(has(&rt, "connection-conn-0"), "loading does not hide known cards");
}

#[test]
fn card_list_fits_both_windows_in_both_palettes() {
    for dark in [true, false] {
        for size in [(960.0, 640.0), (1440.0, 816.0)] {
            let mut p = probe(7);
            p.dark = dark;
            p.vm.active.insert("conn-0".to_owned(), CardConnectionStatus::Connected);
            let rt = runtime(p, size);
            let outside: Vec<String> = rt
                .elements()
                .iter()
                .filter(|e| {
                    let r = e.rect;
                    r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > size.0 + 0.5 || r[1] + r[3] > size.1 + 0.5
                })
                .map(|e| format!("{} {:?}", e.key.as_deref().unwrap_or(&e.path), e.rect))
                .collect();
            assert!(outside.is_empty(),
                "card list dark={dark} {size:?}: {} node(s) outside: {outside:?}", outside.len());
        }
    }
}
