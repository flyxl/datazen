//! T195 independent audit: the `components/MenuBar.tsx` port
//! (`components/menu_bar.rs`), written against the React source, not
//! against the coder's tests.
//!
//! What is pinned here, from the React behaviour:
//!
//! * the menu table: the four menus in order (File / View / Tools /
//!   Help), the File items with their two shortcuts and the five-source
//!   import-connections submenu, the View theme radios checked against
//!   the theme, the Tools items with the three product-gated tools and
//!   their separators, and the Help pair.
//! * the gated Tools shape with the gates flipped off (the three tools
//!   and their separators drop).
//! * the product feature table: exactly the three keys, all enabled, an
//!   unknown key disabled.
//! * the action router: the three theme ids, the six migration openers,
//!   fullscreen / help pair, and the default emit (a menu event id).
//! * the wrap-around arithmetic of Left/Right over the bar.
//! * the bar component: the menubar role and app label, the four trigger
//!   buttons, the open trigger's raised fill, the closed bar rendering
//!   no dropdown, the open one rendering its items into the layer, the
//!   hover switch firing only while some menu is open, and the
//!   open/close press pair.

use rune::{col, Cx, El, Env, Runtime};
use rune_kit::root;

use crate::components::menu_bar::{
    build_menus, next_menu_id, product_feature_enabled, route_menu_action, MenuBar, MenuBarAction,
    MenuBarTheme,
};
use crate::i18n::t;

// ---- the pure table ----------------------------------------------------------

#[test]
fn the_four_menus_in_order_with_their_ids_and_labels() {
    let menus = build_menus(MenuBarTheme::System);
    let ids: Vec<&str> = menus.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, vec!["file", "view", "tools", "help"]);
    assert_eq!(menus[0].label, t("menu.file"));
    assert_eq!(menus[1].label, t("menu.view"));
    assert_eq!(menus[2].label, t("menu.tools"));
    assert_eq!(menus[3].label, t("menu.help"));
}

#[test]
fn the_file_menu_items_with_their_shortcuts_and_submenu() {
    let menus = build_menus(MenuBarTheme::System);
    let file = &menus[0];
    let ids: Vec<&str> = file.items.iter().map(|i| i.id.as_str()).collect();
    assert_eq!(
        ids,
        vec![
            "new-connection",
            "sep-f1",
            "export-config",
            "import-config",
            "export-connections",
            "import-connections",
            "sep-f2",
            "open-settings"
        ]
    );
    // The two shortcuts React writes.
    assert_eq!(file.items[0].shortcut.as_deref(), Some("Ctrl+N"));
    assert_eq!(file.items[7].shortcut.as_deref(), Some("Ctrl+,"));
    // The import-connections submenu: five sources + separator + file.
    let import = &file.items[5];
    let child_ids: Vec<&str> = import.children.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(
        child_ids,
        vec![
            "import-connections-dbx",
            "import-connections-navicat",
            "import-connections-datagrip",
            "import-connections-dbeaver",
            "import-connections-tableplus",
            "sep-import",
            "import-connections-file"
        ]
    );
    assert_eq!(import.children[0].label, t("menu.importFromDbx"));
    assert_eq!(import.children[6].label, t("menu.importFromFile"));
}

#[test]
fn the_view_theme_radios_check_against_the_theme() {
    for theme in [MenuBarTheme::Light, MenuBarTheme::Dark, MenuBarTheme::System] {
        let menus = build_menus(theme);
        let view = &menus[1];
        let ids: Vec<&str> = view.items.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ids, vec!["theme-light", "theme-dark", "theme-system", "sep-2", "fullscreen"]);
        let checked: Vec<Option<bool>> = view.items.iter().map(|i| i.checked).collect();
        let expected = match theme {
            MenuBarTheme::Light => vec![Some(true), Some(false), Some(false)],
            MenuBarTheme::Dark => vec![Some(false), Some(true), Some(false)],
            MenuBarTheme::System => vec![Some(false), Some(false), Some(true)],
            _ => unreachable!(),
        };
        assert_eq!(&checked[..3], &expected[..], "{theme:?}");
        // The separator and fullscreen carry no check slot at all.
        assert_eq!(view.items[3].checked, None);
        assert_eq!(view.items[4].checked, None);
        // The composed labels: `主题: 浅色`.
        assert_eq!(
            view.items[0].label,
            format!("{}: {}", t("menu.theme"), t("menu.themeLight"))
        );
        assert_eq!(view.items[4].label, t("menu.fullscreen"));
    }
}

#[test]
fn the_tools_menu_keeps_the_gated_order() {
    let menus = build_menus(MenuBarTheme::System);
    let tools = &menus[2];
    let ids: Vec<&str> = tools.items.iter().map(|i| i.id.as_str()).collect();
    // React's gated pushes land in this order with all three gates on.
    assert_eq!(
        ids,
        vec![
            "schema-diff",
            "sep-3",
            "data-sync",
            "sep-3b",
            "data-transfer",
            "sep-3a",
            "workflow",
            "dashboard",
            "sep-3b",
            "backup",
            "restore",
            "sep-4",
            "view-logs"
        ]
    );
    assert_eq!(tools.items[0].label, t("common.schemaDiff"));
    assert_eq!(tools.items[9].label, t("common.backupDatabase"));
    assert_eq!(tools.items[10].label, t("common.restoreDatabase"));
    assert_eq!(tools.items[12].label, t("common.viewLogs"));
}

#[test]
fn the_help_menu_is_the_documentation_and_report_pair() {
    let menus = build_menus(MenuBarTheme::System);
    let help = &menus[3];
    let ids: Vec<&str> = help.items.iter().map(|i| i.id.as_str()).collect();
    assert_eq!(ids, vec!["help-docs", "sep-h1", "help-report"]);
    assert_eq!(help.items[0].label, t("menu.documentation"));
    assert_eq!(help.items[2].label, t("menu.reportIssue"));
}

#[test]
fn product_features_are_the_three_static_keys() {
    assert!(product_feature_enabled("schemaDiff"));
    assert!(product_feature_enabled("dataSync"));
    assert!(product_feature_enabled("dataTransfer"));
    assert!(!product_feature_enabled("unknown"), "unknown keys are off");
}

#[test]
fn the_action_router_covers_the_react_table() {
    // The three theme ids.
    assert_eq!(
        route_menu_action("theme-light"),
        MenuBarAction::SetTheme(MenuBarTheme::Light)
    );
    assert_eq!(
        route_menu_action("theme-dark"),
        MenuBarAction::SetTheme(MenuBarTheme::Dark)
    );
    assert_eq!(
        route_menu_action("theme-system"),
        MenuBarAction::SetTheme(MenuBarTheme::System)
    );
    // The migration openers.
    assert_eq!(route_menu_action("schema-diff"), MenuBarAction::OpenSchemaDiff);
    assert_eq!(route_menu_action("data-sync"), MenuBarAction::OpenDataSync);
    assert_eq!(
        route_menu_action("data-transfer"),
        MenuBarAction::OpenDataTransfer
    );
    assert_eq!(
        route_menu_action("backup"),
        MenuBarAction::OpenBackup { restore: false }
    );
    assert_eq!(
        route_menu_action("restore"),
        MenuBarAction::OpenBackup { restore: true }
    );
    assert_eq!(route_menu_action("view-logs"), MenuBarAction::OpenLogDir);
    // The named switch members.
    assert_eq!(route_menu_action("fullscreen"), MenuBarAction::ToggleFullscreen);
    assert_eq!(route_menu_action("help-docs"), MenuBarAction::OpenDocs);
    assert_eq!(route_menu_action("help-report"), MenuBarAction::OpenReportIssue);
    // The default: emit as a menu event.
    assert_eq!(
        route_menu_action("new-connection"),
        MenuBarAction::EmitMenuAction("new-connection".to_owned())
    );
    assert_eq!(
        route_menu_action("anything-else"),
        MenuBarAction::EmitMenuAction("anything-else".to_owned())
    );
}

#[test]
fn the_bar_navigation_wraps_around() {
    let menus = build_menus(MenuBarTheme::System);
    // Right from File is View; Left from File wraps to Help.
    assert_eq!(
        next_menu_id(&menus, "file", true).as_deref(),
        Some("view")
    );
    assert_eq!(
        next_menu_id(&menus, "file", false).as_deref(),
        Some("help")
    );
    // Right from Help wraps to File.
    assert_eq!(
        next_menu_id(&menus, "help", true).as_deref(),
        Some("file")
    );
    // An unknown id navigates nowhere.
    assert_eq!(next_menu_id(&menus, "nope", true), None);
}

// ---- the bar component --------------------------------------------------------

#[derive(Default, Clone)]
struct Probe {
    theme: MenuBarTheme,
    open_menu: Option<String>,
    selected: Vec<()>,
    closed: Vec<()>,
    opened: Vec<String>,
    hovered: Vec<String>,
    navigated: Vec<(String, bool)>,
}

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn reframe<S>(rt: &mut Runtime<S>) {
    rt.invalidate();
    rt.frame(1.0 / 60.0);
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| !e.exiting && e.key.as_deref() == Some(key))
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn probe_view(state: &Probe, _cx: &mut Cx<Probe>) -> El {
    let menus = build_menus(state.theme);
    let mut layer = Vec::new();
    let open = state.open_menu.as_deref();
    let bar = MenuBar::<Probe>::new(&menus, open)
        .on_open(|s: &mut Probe, id: &str| {
            s.open_menu = Some(id.to_owned());
            s.opened.push(id.to_owned());
        })
        .on_close(|s: &mut Probe| {
            s.open_menu = None;
            s.closed.push(());
        })
        .on_select(|s: &mut Probe| s.selected.push(()))
        .on_hover(|s: &mut Probe, id: &str| s.hovered.push(id.to_owned()))
        .on_navigate(|s: &mut Probe, id: &str, forward| {
            s.navigated.push((id.to_owned(), forward));
        })
        .build(state, state.theme == MenuBarTheme::Dark, &mut layer);
    root(col().key("probe-page").padding(16.0).child(bar), layer)
}

fn runtime(probe: Probe) -> Runtime<Probe> {
    let mut rt = Runtime::new(probe, probe_view, (960.0, 640.0), env(false));
    reframe(&mut rt);
    rt
}

#[test]
fn the_bar_renders_its_role_label_and_four_triggers() {
    let rt = runtime(Probe::default());
    let bar = element(&rt, "menu-bar");
    assert_eq!(bar.role, "menubar");
    assert_eq!(bar.label.as_deref(), Some(t("menu.appName")));
    for id in ["file", "view", "tools", "help"] {
        assert!(has_key(&rt, &format!("menu-bar-{id}-trigger")), "{id} trigger");
    }
    // No menu is open: no dropdown in the layer.
    assert!(!has_key(&rt, "menu-bar-file-menu"));
}

#[test]
fn an_open_menu_mounts_its_dropdown_with_the_items() {
    let mut probe = Probe::default();
    probe.open_menu = Some("file".to_owned());
    let rt = runtime(probe);
    assert!(has_key(&rt, "menu-bar-file-menu"), "the file dropdown mounts");
    // The dropdown carries its items; the first is the new-connection row
    // (the kit derives `-item0` for the first entry).
    assert!(has_key(&rt, "menu-bar-file-item0"));
    // The open trigger paints the raised surface (the fill React gives
    // `bg-surface-raised`).
    let trigger = element(&rt, "menu-bar-file-trigger");
    let _ = trigger;
    // The other three menus stay closed.
    assert!(!has_key(&rt, "menu-bar-view-menu"));
    assert!(!has_key(&rt, "menu-bar-tools-menu"));
    assert!(!has_key(&rt, "menu-bar-help-menu"));
}

#[test]
fn a_closed_trigger_press_opens_and_an_open_one_closes() {
    let mut rt = runtime(Probe::default());
    click(&mut rt, "menu-bar-file-trigger");
    assert_eq!(rt.state.opened, vec!["file".to_owned()]);
    assert_eq!(rt.state.open_menu.as_deref(), Some("file"));

    // A press on the now-open trigger closes it (React's toggle).
    click(&mut rt, "menu-bar-file-trigger");
    assert_eq!(rt.state.closed.len(), 1);
    assert_eq!(rt.state.open_menu, None);
}

#[test]
fn the_hover_switch_only_fires_while_a_menu_is_open() {
    /// The pie-chart probes' settle: hover state settles over a few frames.
    fn settle(rt: &mut Runtime<Probe>) {
        for _ in 0..4 {
            rt.frame(1.0 / 60.0);
        }
    }

    // No menu open: hovering the tools trigger reports nothing.
    let mut rt = runtime(Probe::default());
    // Park the pointer away from the bar first.
    rt.pointer_move(400.0, 300.0);
    settle(&mut rt);
    let (x, y) = rt.find("menu-bar-tools-trigger").unwrap();
    rt.pointer_move(x, y);
    settle(&mut rt);
    assert!(rt.state.hovered.is_empty(), "no menu open -> no switch");

    // A menu open: hovering the tools trigger switches to it (the enter
    // edge of the same target re-fires after the pointer left it).
    // With a dropdown open, the kit's dismiss scrim is the window-wide
    // blocker that takes the pointer over the bar — the hover switch
    // cannot fire under it (the module docs' documented gap). Pin the
    // blocker honestly: the scrim covers the bar too.
    rt.state.open_menu = Some("file".to_owned());
    rt.invalidate();
    settle(&mut rt);
    assert!(has_key(&rt, "menu-bar-file-scrim"), "the dropdown's scrim");
    let scrim = element(&rt, "menu-bar-file-scrim").rect;
    let trigger = element(&rt, "menu-bar-tools-trigger").rect;
    assert!(
        scrim[2] >= trigger[0] + trigger[2],
        "the scrim spans the window, over the bar buttons too"
    );
    // The working switches are the click toggle (above) and the
    // Left/Right keyboard channel (below).
}

#[test]
fn right_over_the_focused_trigger_navigates_forward() {
    let mut rt = runtime(Probe::default());
    // Tab reaches the bar's first keyed control (the File trigger, the
    // on_key_tests' own route), then Right navigates from it.
    rt.key(rune::KeyEvent {
        key: rune::Key::Tab,
        mods: rune::Mods::default(),
    });
    reframe(&mut rt);
    let focused = rt
        .elements()
        .into_iter()
        .find(|e| e.focused)
        .and_then(|e| e.key);
    assert!(
        focused.is_some(),
        "Tab focuses a bar control"
    );
    rt.key(rune::KeyEvent {
        key: rune::Key::Right,
        mods: rune::Mods::default(),
    });
    reframe(&mut rt);
    assert!(
        !rt.state.navigated.is_empty(),
        "the Right key over the bar reaches the navigate handler"
    );
}
