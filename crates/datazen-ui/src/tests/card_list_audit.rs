//! T409 independent audit: `ConnectionCardList` vs React.
//!
//! Probes branches the coder's suite does not: the engine-label chip on each
//! card, the connecting-state action button, card keyboard activation, filter
//! by engine label, and rect geometry with long names/groups.

use std::rc::Rc;

use rune::{col, Cx, El, Env, Key, KeyEvent, Mods, Runtime, TextEdit};

use crate::views::home::connection_card_list::{
    connection_card_list, filtered_connections, CardConnectionStatus, ConnectionCardListHostPort,
    ConnectionCardListVm, ConnectionCardVm,
};

#[derive(Clone, Debug, PartialEq, Eq)]
enum Event {
    Connect(String),
}

#[derive(Clone)]
struct Probe {
    vm: ConnectionCardListVm,
    dark: bool,
    events: Vec<Event>,
}

fn conn(id: &str, name: &str, engine: &str, host: Option<&str>) -> ConnectionCardVm {
    ConnectionCardVm {
        id: id.to_owned(),
        name: name.to_owned(),
        engine: engine.to_owned(),
        host: host.map(str::to_owned),
        port: Some("5432".to_owned()),
        database: None,
        group: Some("prod".to_owned()),
        pinned: false,
        last_connected_at: None,
    }
}

fn probe(connections: Vec<ConnectionCardVm>) -> Probe {
    Probe {
        vm: ConnectionCardListVm {
            connections,
            active: Default::default(),
            filter: TextEdit::new(String::new()),
            expanded: false,
            can_import: false,
            loading: false,
        },
        dark: true,
        events: Vec::new(),
    }
}

fn view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let host = ConnectionCardListHostPort {
        on_connect: Some(Rc::new(|st: &mut Probe, id: String| st.events.push(Event::Connect(id)))),
        ..ConnectionCardListHostPort::default()
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

fn texts<S>(rt: &Runtime<S>) -> Vec<String> {
    rt.elements().iter().filter_map(|e| e.text.clone()).collect()
}

/// React renders `<span>{getDbLabel(databaseType)}</span>` beside the name: the
/// full engine label ("PostgreSQL"), not a two-letter monogram or nothing.
#[test]
fn card_shows_the_full_engine_label() {
    let rt = runtime(probe(vec![conn("c1", "Orders", "postgresql", Some("db.internal"))]), (1440.0, 816.0));
    let rendered = texts(&rt);
    let expected = "PostgreSQL"; // React getDbLabel('postgresql')
    assert!(
        rendered.iter().any(|t| t == &expected),
        "card must show the engine label {expected:?}; rendered {rendered:?}"
    );
}

/// A connecting card replaces the Connect/Open button with a disabled spinner
/// that has NO React data-testid; the port must not expose a connect button.
#[test]
fn connecting_card_has_no_connect_testid() {
    let mut p = probe(vec![conn("c1", "Orders", "postgresql", Some("db.internal"))]);
    p.vm.active.insert("c1".to_owned(), CardConnectionStatus::Connecting);
    let rt = runtime(p, (1440.0, 816.0));
    assert!(
        !has(&rt, "home-conn-connect-c1"),
        "React renders a testid-less spinner while connecting"
    );
}

/// The card row is a keyboard-activatable button in React (Enter/Space).
#[test]
fn card_keyboard_enter_connects() {
    let mut rt = runtime(probe(vec![conn("c1", "Orders", "postgresql", Some("db.internal"))]), (1440.0, 816.0));
    let (x, y) = rt.find("connection-c1").unwrap_or_else(|| panic!("card laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.key(KeyEvent { key: Key::Enter, mods: Mods::default() });
    rt.frame(1.0);
    assert_eq!(rt.state.events, [Event::Connect("c1".to_owned())]);
}

/// The filter matches the engine label, not only the raw id.
#[test]
fn filter_matches_the_engine_label() {
    let c = conn("c1", "Orders", "postgresql", Some("db.internal"));
    let label = "PostgreSQL"; // React getDbLabel('postgresql')
    let matched = filtered_connections(&[c], &label);
    assert_eq!(matched.len(), 1, "filtering by {label:?} must match the card");
}

/// Long names/groups stay inside both windows in both palettes (rect channel).
#[test]
fn long_card_content_fits_both_windows_both_palettes() {
    for dark in [true, false] {
        for size in [(960.0, 640.0), (1440.0, 816.0)] {
            let mut p = probe(vec![conn(
                "c1",
                "A very long connection name that should truncate gracefully in the card",
                "postgresql",
                Some("a.very.long.host.example.internal"),
            )]);
            p.dark = dark;
            let rt = runtime(p, size);
            let mut outside = Vec::new();
            for e in rt.elements() {
                let r = e.rect;
                if r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > size.0 + 0.5 || r[1] + r[3] > size.1 + 0.5 {
                    outside.push(format!("{} {:?}", e.key.unwrap_or(e.path), r));
                }
            }
            assert!(outside.is_empty(), "dark={dark} {size:?}: {outside:?}");
        }
    }
}
