//! T485: `PanelContentRenderer` port — slot routing, KV relay identity,
//! empty/loading states, actions and two-window/two-palette geometry.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use crate::i18n::t;
use crate::views::panel_content_renderer::{
    create_kv_slot_state, format_context_path, is_key_value_driver, panel_content_renderer,
    KvSlotAtom, KvViewHostPort, PanelAction, PanelHostPort, PanelType, PanelVm, SubTabId,
};

#[derive(Clone)]
struct Probe {
    panel: Option<PanelVm>,
    current_database: Option<String>,
    last_table_schema: Option<String>,
    dark: bool,
    slot_state: Option<KvSlotAtom>,
    events: Vec<PanelAction>,
    kv_received: Rc<RefCell<Option<Option<KvSlotAtom>>>>,
}

fn probe(panel_type: PanelType) -> Probe {
    Probe {
        panel: Some(PanelVm {
            id: "panel-1".to_owned(),
            panel_type,
            connection_id: "cfg-1".to_owned(),
            db_session_id: "sess-1".to_owned(),
            connection_name: "Local".to_owned(),
            database_type: "postgresql".to_owned(),
            database: Some("app".to_owned()),
            table_schema: Some("public".to_owned()),
            table_name: "orders".to_owned(),
            ..PanelVm::default()
        }),
        current_database: Some("app".to_owned()),
        last_table_schema: None,
        dark: true,
        slot_state: None,
        events: Vec::new(),
        kv_received: Rc::new(RefCell::new(None)),
    }
}

fn probe_view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let panel = s.panel.clone();
    let current = s.current_database.clone();
    let last = s.last_table_schema.clone();
    let dark = s.dark;
    let atom = s.slot_state.clone();
    let kv_received = s.kv_received.clone();
    let host = PanelHostPort {
        // Rune invokes press handlers with the real app state, so events land
        // in `rt.state` even though the renderer itself gets a throwaway slice.
        on_action: Some(Rc::new(move |st: &mut Probe, action| st.events.push(action))),
        kv: KvViewHostPort {
            on_view: Some(Rc::new(move |_: &mut Probe, _props, received| {
                *kv_received.borrow_mut() = Some(received)
            })),
        },
    };
    let mut dummy = Probe {
        panel: None, current_database: None, last_table_schema: None, dark,
        slot_state: None, events: Vec::new(), kv_received: Rc::new(RefCell::new(None)),
    };
    panel_content_renderer(
        panel.as_ref(),
        None,
        current.as_deref(),
        last.as_deref(),
        dark,
        atom,
        &host,
        &mut dummy,
    )
    .unwrap_or_else(|| col().w_full().h_full().a11y_hidden().key("panel-none"))
}

fn runtime(p: Probe, size: (f32, f32)) -> Runtime<Probe> {
    let dark = p.dark;
    let mut rt = Runtime::new(p, probe_view, size, Env { system_dark: dark, scale: 1.0 });
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

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out before click"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0);
}

#[test]
fn every_panel_type_routes_to_its_slot() {
    let cases: [(PanelType, &str); 10] = [
        (PanelType::Table, "panel-slot-table"),
        (PanelType::View, "panel-slot-view"),
        (PanelType::Query, "panel-slot-query"),
        (PanelType::CreateTable, "panel-slot-create-table"),
        (PanelType::ErDiagram, "panel-slot-er-diagram"),
        (PanelType::Objects, "panel-slot-objects"),
        (PanelType::Privileges, "panel-slot-privileges"),
        (PanelType::ServerStatus, "panel-slot-server-status"),
        (PanelType::Processes, "panel-slot-processes"),
        (PanelType::DbObject, "panel-slot-db-object"),
    ];
    for (kind, key) in cases {
        let rt = runtime(probe(kind), (1440.0, 816.0));
        assert!(has(&rt, key), "{kind:?} routes to {key}");
        assert!(has(&rt, "panel-content-renderer"));
    }
}

#[test]
fn kv_panel_forwards_the_exact_slot_atom() {
    let atom = create_kv_slot_state();
    {
        let mut state = atom.borrow_mut();
        state.selected_key = "app:cache:session:1".to_owned();
        state.dirty = true;
    }
    let mut p = probe(PanelType::ServerStatus);
    p.panel.as_mut().unwrap().database_type = "kvredis".to_owned();
    p.panel.as_mut().unwrap().kv_db_name = Some("db7".to_owned());
    p.slot_state = Some(atom.clone());
    let rt = runtime(p, (1440.0, 816.0));
    assert!(has(&rt, "panel-kv-view"), "KV panels route to the driver view");
    let received = rt.state.kv_received.borrow().as_ref().cloned().flatten().expect("host received the atom");
    assert!(Rc::ptr_eq(&received, &atom), "the very same atom is forwarded, identity intact");
    assert_eq!(element(&rt, "panel-kv-selected-key").label.as_deref(), Some("app:cache:session:1"));
    assert_eq!(element(&rt, "panel-kv-dirty").label.as_deref(), Some("true"));
    assert_eq!(element(&rt, "panel-kv-database").label.as_deref(), Some("db7"));
    assert!(is_key_value_driver("kvredis"));
}

#[test]
fn kv_panel_without_a_relay_still_renders() {
    let mut p = probe(PanelType::ServerStatus);
    p.panel.as_mut().unwrap().database_type = "kvredis".to_owned();
    let rt = runtime(p, (960.0, 640.0));
    assert!(has(&rt, "panel-kv-view"));
    assert_eq!(element(&rt, "panel-kv-has-state").label.as_deref(), Some("no"));
    assert!(rt.state.kv_received.borrow().as_ref().is_some_and(|atom| atom.is_none()), "callback fired with no atom");
}

#[test]
fn no_panel_and_er_without_database_render_nothing() {
    let mut p = probe(PanelType::Table);
    p.panel = None;
    let rt = runtime(p, (960.0, 640.0));
    assert!(has(&rt, "panel-none"), "React returns null for a missing panel");

    let mut p = probe(PanelType::ErDiagram);
    p.panel.as_mut().unwrap().database = None;
    p.current_database = None;
    let rt = runtime(p, (960.0, 640.0));
    assert!(has(&rt, "panel-none"), "ER diagram without a database renders nothing");
}

#[test]
fn create_table_multi_db_without_database_shows_loading() {
    let mut p = probe(PanelType::CreateTable);
    p.panel.as_mut().unwrap().database = None;
    p.panel.as_mut().unwrap().multi_db_without_database = true;
    p.current_database = None;
    let rt = runtime(p, (1440.0, 816.0));
    assert!(has(&rt, "panel-slot-create-table-loading"));
    assert!(!has(&rt, "panel-slot-create-table"));
    let rendered: Vec<String> = rt.elements().iter().filter_map(|e| e.text.clone()).collect();
    assert!(rendered.iter().any(|text| text == t("common.loading")));
}

#[test]
fn sub_tabs_follow_read_only_and_dispatch_selection() {
    let mut rt = runtime(probe(PanelType::Table), (1440.0, 816.0));
    for key in [
        "sub-tab-data",
        "sub-tab-structure",
        "sub-tab-indexes",
        "sub-tab-foreignKeys",
        "sub-tab-ddl",
        "panel-context-path",
    ] {
        assert!(has(&rt, key), "{key} is mounted");
    }
    assert_eq!(element(&rt, "panel-context-path").text.as_deref(), Some("Local.app.public"));
    click(&mut rt, "sub-tab-ddl");
    assert!(rt.state.events.iter().any(|a| matches!(
        a,
        PanelAction::SetSubTab { panel_id, sub_tab }
            if panel_id == "panel-1" && *sub_tab == SubTabId::Ddl
    )), "selecting a sub-tab dispatches to the host");

    let mut read_only = probe(PanelType::Table);
    read_only.panel.as_mut().unwrap().database_type = "sqlite".to_owned();
    let rt = runtime(read_only, (1440.0, 816.0));
    assert!(has(&rt, "sub-tab-data"));
    assert!(has(&rt, "sub-tab-structure"));
    assert!(!has(&rt, "sub-tab-indexes"), "read-only drivers show only two tabs");
    assert!(crate::views::panel_content_renderer::is_read_only_driver("sqlite"));
}

#[test]
fn structure_editing_and_data_cache_slots_render() {
    let mut p = probe(PanelType::Table);
    p.panel.as_mut().unwrap().sub_tab = SubTabId::Structure;
    p.panel.as_mut().unwrap().structure_editing = true;
    let rt = runtime(p, (1440.0, 816.0));
    assert!(has(&rt, "panel-slot-structure-editor"));

    let mut p = probe(PanelType::ServerStatus);
    p.panel.as_mut().unwrap().has_panel_data = true;
    let mut rt = runtime(p, (1440.0, 816.0));
    click(&mut rt, "panel-slot-server-status");
    assert!(rt.state.events.iter().any(|a| matches!(
        a,
        PanelAction::UpdatePanelData { panel_id, data_key }
            if panel_id == "panel-1" && *data_key == "status"
    )), "a data-bearing slot publishes through the host");
}

#[test]
fn context_path_rules_match_react() {
    assert_eq!(format_context_path("Local", Some("app"), Some("public")), "Local.app.public");
    assert_eq!(format_context_path("Local", None, None), "Local");
}

#[test]
fn slots_fit_both_windows_in_both_palettes() {
    for dark in [true, false] {
        for size in [(960.0, 640.0), (1440.0, 816.0)] {
            for kind in [
                PanelType::Table,
                PanelType::View,
                PanelType::Query,
                PanelType::ErDiagram,
                PanelType::ServerStatus,
            ] {
                let mut p = probe(kind);
                p.dark = dark;
                let rt = runtime(p, size);
                let outside: Vec<String> = rt.elements().iter()
                    .filter(|e| {
                        let r = e.rect;
                        r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > size.0 + 0.5 || r[1] + r[3] > size.1 + 0.5
                    })
                    .map(|e| format!("{} {:?}", e.key.as_deref().unwrap_or(&e.path), e.rect))
                    .collect();
                assert!(outside.is_empty(), "{kind:?} dark={dark} {size:?}: {outside:?}");
            }
        }
    }
}

// ── T485 independent audit (appended; reuses this module's Probe) ──────────

/// React `formatPanelContextPath` (`packages/frontend/src/lib/panelContextPath.ts`)
/// trims each part and joins with a dot (`panelContextPath.test.ts` pins
/// `'Local PG.app.public'`). The port joins with `" · "` and does not trim.
#[test]
fn audit_context_path_uses_react_dot_separator_and_trims() {
    assert_eq!(
        format_context_path("Local PG", Some("app"), Some("public")),
        "Local PG.app.public",
        "React joins the breadcrumb with a dot"
    );
    assert_eq!(
        format_context_path("Redis", Some("  "), None),
        "Redis",
        "React trims a whitespace-only part away"
    );
    assert_eq!(
        format_context_path("Local MySQL", Some("app"), None),
        "Local MySQL.app"
    );
}

/// React's table SubTabBar passes `database: panel.database ?? currentDatabase`
/// (`PanelContentRenderer.tsx:224-228`). The port's `sub_tab_bar` reads only
/// `panel.database`, dropping the session's current database.
#[test]
fn audit_table_context_path_falls_back_to_current_database() {
    let mut p = probe(PanelType::Table);
    p.panel.as_mut().unwrap().database = None;
    p.current_database = Some("app".to_owned());
    p.panel.as_mut().unwrap().table_schema = Some("public".to_owned());
    let rt = runtime(p, (1440.0, 816.0));
    assert_eq!(
        element(&rt, "panel-context-path").text.as_deref(),
        Some("Local.app.public"),
        "the sub-tab breadcrumb must use the current-database fallback"
    );
}

/// React's create-table gate is `isMultiDb && !database` where `database` is
/// `createPanel.database ?? currentDatabase` (`PanelContentRenderer.tsx:377`,
/// `:531`). The port tests `panel.database.is_none()` only.
#[test]
fn audit_create_table_uses_current_database_before_showing_loading() {
    let mut p = probe(PanelType::CreateTable);
    p.panel.as_mut().unwrap().database = None;
    p.panel.as_mut().unwrap().multi_db_without_database = true;
    p.current_database = Some("app".to_owned());
    let rt = runtime(p, (1440.0, 816.0));
    assert!(
        !has(&rt, "panel-slot-create-table-loading"),
        "a current database satisfies the create-table gate; no loading placeholder"
    );
    assert!(has(&rt, "panel-slot-create-table"), "the editor slot renders instead");
}

/// React always mounts the KV connection view for a KV panel, even without a
/// relay (`PanelContentRenderer.tsx:105-120`, pinned by the KV-slot test's
/// "without a relay" case). The port's `on_view` port is the only way a host
/// can mount that view, and it is skipped when no atom is given.
#[test]
fn audit_kv_host_port_is_notified_even_without_a_relay() {
    let mut p = probe(PanelType::ServerStatus);
    p.panel.as_mut().unwrap().database_type = "kvredis".to_owned();
    p.current_database = Some("db7".to_owned());
    let rt = runtime(p, (1440.0, 816.0));
    assert!(has(&rt, "panel-kv-view"), "the KV branch is mounted");
    assert!(
        rt.state.kv_received.borrow().is_some(),
        "the host must be told to mount the KV view even without a slot atom"
    );
}
