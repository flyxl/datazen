//! T110: the `windows/connection/ConnectionPage.tsx` port.
//!
//! These tests pin the container page at the composition level: the shell keys
//! React's page owns, all Sections staying reachable through the composed body,
//! the tab/toolbar behaviour of the content workspace, the settings-back glue,
//! and a two-size × two-theme runtime-rect sweep for horizontal overflow. All
//! observations come from the laid-out element tree (`rt.elements()`), not from
//! any component's self-report.

use datazen_ui::test_support::*;

/// Invariants for the connections landing composition. Other routes still
/// share the window shell, but mount the database navigator conditionally.
const SHELL_KEYS: &[&str] = &[
    "datazen-root",
    "rune-connection-page",
    "datazen-titlebar",
    "datazen-titlebar-label",
    "datazen-theme-toggle",
    "rune-titlebar-left",
    "menu-bar-host",
    "datazen-body",
    "datazen-rail",
    "datazen-navigator",
    "navigator-connections",
    "navigator-filter-input",
    "datazen-page",
    "content-toolbar",
    "content-status-bar",
    "status-connection-label",
    "status-context",
    "status-shortcuts",
];

fn key_set(rt: &Runtime<AppState>) -> std::collections::HashSet<String> {
    // Rune keeps exiting nodes in `elements()` for animation; they are inert
    // and stale. A key-presence assertion after a transition must ignore them
    // or it passes whether or not the new state actually mounted the key.
    rt.elements()
        .into_iter()
        .filter(|e| !e.exiting)
        .filter_map(|e| e.key)
        .collect()
}

#[test]
fn the_port_composes_the_connection_page_shell() {
    let mut rt = runtime();
    rt.frame(0.0);
    let keys = key_set(&rt);

    for key in SHELL_KEYS {
        assert!(
            keys.contains(*key),
            "connection page shell is missing composed key {key:?}; keys: {keys:?}"
        );
    }

    // The default section is Connections: its home body rides the page, and
    // the tab strip is absent (React only mounts tabs inside ContentView).
    assert!(
        keys.contains("connection-list"),
        "the default section composes the connections home body"
    );
    assert!(
        keys.contains("tab-strip-absent"),
        "Connections has no tab strip (ContentView tab mount is Workspace-only)"
    );
    assert!(
        !keys.contains("tab-strip"),
        "Connections must not mount the Workspace tab strip"
    );

    // The dialog hosts are part of the page even when no dialog is open: the
    // modal layer simply mounts nothing. Opening one proves the host is wired.
    assert!(
        rt.find("connection-new").is_some(),
        "the new-connection trigger that opens the dialog host is composed"
    );
    let (x, y) = rt
        .find("connection-new")
        .expect("the new-connection press is on the page");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.frame(0.0);
    let keys = key_set(&rt);
    for key in ["connection-dialog", "connection-dialog-title", "connection-form"] {
        assert!(
            keys.contains(key),
            "the dialog host mounts {key:?}: {keys:?}"
        );
    }
}

#[test]
fn every_section_stays_reachable_after_the_port() {
    // Wiring the page must not strand any Section: the composed body still
    // dispatches each one to its own keyed page.
    let bodies: &[(Section, &str)] = &[
        (Section::Connections, "connection-list"),
        (Section::Workspace, "workspace-empty"),
        (Section::Settings, "settings-page"),
        (Section::Tools, "tools-page"),
        (Section::Dashboards, "dashboards-page"),
        // T199/WorkflowPage port: the workflows section now mounts the real
        // workflow page (`workflow-workspace`), not the old placeholder.
        (Section::Workflows, "workflow-workspace"),
        // T072 ported the real WappManagementPage, so the old C4
        // "wapps-page" placeholder is gone: the section's body is the
        // ported page's own testid root.
        (Section::Wapps, "extension-management-page"),
    ];
    for (section, body_key) in bodies {
        let mut rt = runtime();
        rt.state.active = *section;
        rt.invalidate();
        rt.frame(0.0);
        let keys = key_set(&rt);
        assert!(
            keys.contains(*body_key),
            "{section:?} must still dispatch to {body_key:?}; keys: {keys:?}"
        );
        let database = matches!(*section, Section::Connections | Section::Workspace);
        for key in [
            "datazen-navigator",
            "navigator-connections",
            "navigator-filter-input",
            "splitter:navigator-width",
        ] {
            assert_eq!(keys.contains(key), database, "{section:?}: {key}");
        }
        assert_eq!(keys.contains("workflow-sidebar"), *section == Section::Workflows);
        assert_eq!(
            keys.contains("datazen-rail"),
            *section != Section::Settings,
            "{section:?}: Settings has its own full-window body"
        );
        let title = rt
            .elements()
            .into_iter()
            .filter(|e| !e.exiting)
            .find(|e| e.key.as_deref() == Some("datazen-titlebar-label"))
            .expect("the title band is composed on every section");
        let expected_title = match (*section, rt.state.builtin_language()) {
            (Section::Settings, datazen_ui_api::BuiltinLanguage::English) => "Settings - DataZen",
            (Section::Connections, datazen_ui_api::BuiltinLanguage::English) => "Connections - DataZen",
            (Section::Workflows, datazen_ui_api::BuiltinLanguage::English) => "Workflow - DataZen",
            (Section::Dashboards, datazen_ui_api::BuiltinLanguage::English) => "Dashboards - DataZen",
            (Section::Settings, _) => "偏好设置 - DataZen",
            (Section::Connections | Section::Workflows | Section::Dashboards, _) =>
                datazen_ui::i18n::t(match section {
                    Section::Connections => "win.connections",
                    Section::Workflows => "win.workflow",
                    _ => "win.dashboard",
                }),
            _ => section.label(),
        };
        assert_eq!(
            title.text.as_deref(),
            Some(expected_title),
            "{section:?}: the titlebar follows the dispatched section"
        );
    }

    // And the rail can actually move between them after the port.
    let mut rt = runtime();
    rt.frame(0.0);
    let (x, y) = rt
        .find(&format!("rail-{}", Section::Workspace.key()))
        .expect("workspace rail item");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    assert_eq!(
        rt.state.active,
        Section::Workspace,
        "the composed rail still selects a section"
    );
    assert!(
        key_set(&rt).contains("tab-strip"),
        "Workspace now mounts the tab strip"
    );
}

#[test]
fn switching_to_workflow_and_back_preserves_the_database_navigator_state() {
    let mut rt = runtime();
    rt.state.set_layout_size("navigator-width", 310).unwrap();
    rt.state.navigator_filter = rune::TextEdit::new("local");
    rt.state.toggle_expand("catalog-fixture");
    let rail_was_expanded = rt.state.sidebar_expanded();
    rt.frame(0.0);

    for (key, section) in [
        ("rail-workflows", Section::Workflows),
        ("rail-connections", Section::Connections),
        ("rail-workspace", Section::Workspace),
    ] {
        let (x, y) = rt.find(key).expect("route is reachable through the mode rail");
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.0);
        rt.frame(0.0);
        assert_eq!(rt.state.active, section);
        let asides: Vec<_> = rt.elements().into_iter()
            .filter(|e| !e.exiting && e.key.as_deref() == Some("datazen-navigator"))
            .collect();
        if section == Section::Workflows {
            assert!(asides.is_empty(), "Workflow owns only its dedicated sidebar");
            assert!(key_set(&rt).contains("workflow-sidebar"));
            assert!(!key_set(&rt).contains("splitter:navigator-width"));
        } else {
            assert_eq!(asides.len(), 1, "one database navigator per database route");
            assert!((asides[0].rect[2] - 310.0).abs() <= 0.5, "saved width resumes");
        }
        assert_eq!(rt.state.navigator_filter.text, "local");
        assert!(rt.state.collapsed.contains("catalog-fixture"));
        assert_eq!(rt.state.sidebar_expanded(), rail_was_expanded);
    }
}

#[test]
fn embedded_workflow_is_flush_to_the_rail_and_keeps_its_own_controls() {
    use datazen_ui_api::WorkspaceApi;

    let mut rt = runtime();
    rt.state.workspace.connect("local").expect("memory database session");
    rt.state.open_table_tab("customers").expect("an open database tab");
    rt.frame(0.0);
    assert!(key_set(&rt).contains("content-status-bar"));
    let selected = rt.state.tabs.selected().expect("the table tab").id;

    click(&mut rt, "rail-workflows");

    let keys = key_set(&rt);
    for key in ["content-toolbar", "content-toolbar-title", "content-status-bar",
                "status-connection-label", "datazen-navigator"] {
        assert!(!keys.contains(key), "workflow has no database chrome: {key}");
    }

    let elements = rt.elements();
    let rect = |key: &str| {
        elements.iter()
            .find(|e| !e.exiting && e.key.as_deref() == Some(key))
            .unwrap_or_else(|| panic!("missing workflow geometry {key}"))
            .rect
    };
    let close = |a: f32, b: f32| (a - b).abs() <= 0.5;
    let [rx, ry, rw, rh] = rect("datazen-rail");
    let [wx, wy, ww, wh] = rect("workflow-workspace");
    let [tx, ty, tw, th] = rect("workflow-toolbar");
    let [sx, sy, _, sh] = rect("workflow-sidebar");
    assert!(close(wx, rx + rw) && close(wy, ry) && close(wh, rh));
    assert!(close(wx + ww, 1440.0), "workflow reaches the right edge");
    assert!(close(tx, wx) && close(ty, wy) && close(tw, ww));
    assert!(close(th, 41.0), "24pt controls + 16pt padding + one layout border");
    assert!(close(sx, wx) && close(sy, ty + th) && close(sy + sh, wy + wh),
            "workflow sidebar fills the area below its own toolbar");

    click(&mut rt, "workflow-side-tab-history");
    assert!(key_set(&rt).contains("workflow-history-list"));
    click(&mut rt, "workflow-side-tab-workflows");
    click(&mut rt, "workflow-create");
    assert!(key_set(&rt).contains("workflow-form-root"));
    click(&mut rt, "workflow-editor-cancel");
    click(&mut rt, "rail-workspace");
    let keys = key_set(&rt);
    assert!(keys.contains("datazen-navigator") && keys.contains("content-status-bar"));
    assert!(!keys.contains("workflow-sidebar"));
    assert_eq!(rt.state.tabs.selected().map(|tab| tab.id), Some(selected));
}

#[test]
fn dashboard_and_extensions_own_their_full_page_and_keep_their_controls() {
    use datazen_ui_api::WorkspaceApi;

    let mut rt = runtime();
    rt.state.workspace.connect("local").expect("memory database session");
    let stored_tab = rt.state.open_table_tab("customers").expect("database table tab");
    rt.frame(0.0);

    let assert_owned_page = |rt: &Runtime<AppState>, page: &str| {
        let keys = key_set(rt);
        for key in ["content-toolbar", "content-toolbar-title", "content-status-bar",
                    "status-connection-label", "datazen-navigator", "splitter:navigator-width"] {
            assert!(!keys.contains(key), "{page}: no foreign database chrome {key}");
        }
        let elements = rt.elements();
        let rect = |key: &str| {
            elements.iter()
                .find(|e| !e.exiting && e.key.as_deref() == Some(key))
                .unwrap_or_else(|| panic!("missing {key}"))
                .rect
        };
        let [x, y, width, height] = rect("datazen-rail");
        assert_eq!(rect(page), [x + width, y, 1440.0 - x - width, height]);
    };

    click(&mut rt, "rail-dashboards");
    assert_owned_page(&rt, "dashboards-page");
    assert!(key_set(&rt).contains("widget-form"));
    rt.state.widget_title = rune::TextEdit::new("Still a real widget");
    rt.state.widget_query = rune::TextEdit::new("SELECT 1");
    rt.invalidate();
    rt.frame(0.0);
    click(&mut rt, "widget-save");
    let widget = rt.state.host_widgets().into_iter()
        .find(|widget| widget.title == "Still a real widget")
        .expect("existing widget save runs");
    assert!(key_set(&rt).contains(&format!("widget-{}", widget.id)));
    click(&mut rt, "widget-refresh");
    assert!(rt.state.widget_bars.contains_key(&widget.id));

    click(&mut rt, "rail-wapps");
    assert_owned_page(&rt, "extension-management-page");
    let keys = key_set(&rt);
    assert!(keys.contains("extension-page-title") && keys.contains("extension-search-input"));
    assert!(keys.contains("extension-page-error"), "unsupported host list remains visible");
    click(&mut rt, "extension-filter-theme");
    assert_eq!(rt.state.wapp_filter.key(), "extension-filter-theme");
    click(&mut rt, "extension-install-button");
    assert!(key_set(&rt).contains("install-wapp-dialog"));
    click(&mut rt, "extension-install-cancel");
    assert!(!key_set(&rt).contains("install-wapp-dialog"));

    click(&mut rt, "rail-workspace");
    let keys = key_set(&rt);
    assert!(keys.contains("datazen-navigator") && keys.contains("content-status-bar"));
    assert_eq!(rt.state.tabs.selected().map(|tab| tab.id), Some(stored_tab));
}

#[test]
fn tabs_and_toolbar_follow_the_content_workspace_contract() {
    let mut rt = runtime();
    rt.state.active = Section::Workspace;
    let first = rt
        .state
        .tabs
        .open(TabKind::Table, "customers")
        .expect("first tab");
    let second = rt
        .state
        .tabs
        .open(TabKind::Table, "orders")
        .expect("second tab");
    rt.invalidate();
    rt.frame(0.0);

    let strip = rt
        .elements()
        .into_iter()
        .filter(|e| !e.exiting)
        .find(|e| e.key.as_deref() == Some("tab-strip"))
        .expect("tab strip is mounted on Workspace");
    assert_eq!(strip.role, "tablist", "the tab strip keeps its tablist role");
    let keys = key_set(&rt);
    for key in [
        format!("tab-{first}"),
        format!("tab-close-{first}"),
        format!("tab-{second}"),
        format!("tab-close-{second}"),
    ] {
        assert!(keys.contains(&key), "tab key {key:?} is composed: {keys:?}");
    }

    // The content toolbar's new-query action still opens a query tab.
    let (x, y) = rt
        .find("toolbar-new-query")
        .expect("the workspace new-query action");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.frame(0.0);
    let keys = key_set(&rt);
    assert!(
        keys.contains("query-tab"),
        "new-query opens the query workspace: {keys:?}"
    );
    assert!(
        keys.contains("query-toolbar"),
        "the query workspace carries its toolbar"
    );

    // A tab's close control removes that tab without taking the other one.
    let (x, y) = rt
        .find(&format!("tab-close-{first}"))
        .expect("the first tab's close control");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    assert!(
        !rt.state.tabs.tabs().iter().any(|t| t.id == first),
        "closing a tab removes it"
    );
    assert!(
        rt.state.tabs.tabs().iter().any(|t| t.id == second),
        "closing one tab leaves the other"
    );
}

// Geometry and hit-testing follow the drawn boxes, not the new layout
// targets. Zero-delta frames commit a route but freeze its transition at the
// previous screen's coordinates. Progress the deterministic test clock.
fn settle_route(rt: &mut Runtime<AppState>) {
    for _ in 0..48 {
        rt.frame(1.0 / 60.0);
    }
}

fn click(rt: &mut Runtime<AppState>, key: &str) {
    settle_route(rt);
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is reachable"));
    rt.pointer_move(x, y);
    rt.frame(1.0 / 60.0);
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} remains reachable after pointer move"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle_route(rt);
}

#[test]
fn plain_query_fills_its_lane_without_changing_other_tabs_or_drawers() {
    use datazen_ui::constants::{NAVIGATOR_WIDTH_KEY, SIDEBAR_RAIL_WIDTH_KEY};

    let rect = |rt: &Runtime<AppState>, key: &str| {
        rt.elements()
            .into_iter()
            .find(|e| !e.exiting && e.key.as_deref() == Some(key))
            .unwrap_or_else(|| panic!("missing visible content geometry {key}"))
            .rect
    };
    let mut rt = runtime();
    rt.state.set_layout_size(NAVIGATOR_WIDTH_KEY, 280).unwrap();
    rt.state.set_layout_size(SIDEBAR_RAIL_WIDTH_KEY, 40).unwrap();
    settle_route(&mut rt);
    let landing = rect(&rt, "content-view");
    let nav = rect(&rt, "datazen-navigator");
    let rail = rect(&rt, "datazen-rail");
    let divider = rect(&rt, "splitter:navigator-width");
    let titlebar = rect(&rt, "datazen-titlebar");
    let body = rect(&rt, "datazen-body");
    assert_eq!(landing[0], divider[0] + divider[2] + 24.0);
    assert_eq!(landing[1], body[1] + 24.0);

    rt.state.workspace.connect("local").unwrap();
    let table = rt.state.open_table_tab("customers").unwrap();
    settle_route(&mut rt);
    let table_box = rect(&rt, "content-view");

    click(&mut rt, "toolbar-new-query");
    let query_id = rt.state.tabs.selected().expect("selected Query tab").id;
    assert_ne!(table, query_id);
    let view = rect(&rt, "content-view");
    let page = rect(&rt, "datazen-page");
    let content_toolbar = rect(&rt, "content-toolbar");
    let tabs = rect(&rt, "tab-strip");
    let query = rect(&rt, "query-tab");
    let footer = rect(&rt, "content-status-bar");
    assert_eq!(view, page, "no outer inset or invisible-sibling gaps on plain Query");
    assert_eq!(page[0], divider[0] + divider[2], "keep the real resizable divider");
    assert_eq!(page[1], titlebar[1] + titlebar[3]);
    assert_eq!(content_toolbar, [page[0], page[1], page[2], 48.0]);
    assert_eq!(tabs, [page[0], content_toolbar[1] + content_toolbar[3], page[2], 48.0]);
    assert_eq!([query[0], query[1], query[2]], [page[0], tabs[1] + tabs[3], page[2]]);
    assert_eq!(query[1] + query[3], footer[1], "retain the single status bar below Query");
    assert_eq!(footer, [page[0], body[1] + body[3] - 40.0, page[2], 40.0]);
    for key in ["db-object-host", "doc-view-host"] {
        let slot = rect(&rt, key);
        assert_eq!([slot[1], slot[2], slot[3]], [page[1] + page[3], 0.0, 0.0]);
    }
    assert_eq!(rect(&rt, "datazen-rail"), rail);
    assert_eq!(rect(&rt, "datazen-navigator"), nav);
    assert_eq!(rect(&rt, "splitter:navigator-width"), divider);

    click(&mut rt, "conn-toolbar-ai");
    assert!(rt.state.ai_chat_open, "the real drawer button still opens the drawer");
    assert_eq!(rect(&rt, "content-view")[1], body[1] + 24.0, "drawer layout is not changed");
    click(&mut rt, "conn-toolbar-ai");
    assert!(!rt.state.ai_chat_open);
    assert_eq!(rect(&rt, "content-view"), view, "closing the drawer restores plain Query");
    click(&mut rt, &format!("tab-{table}"));
    assert_eq!(rt.state.tabs.selected().map(|tab| tab.id), Some(table));
    assert_eq!(rect(&rt, "content-view"), table_box, "non-Query tab retains its shell");
    assert_eq!(rt.state.layout_size(NAVIGATOR_WIDTH_KEY), Some(280));
    assert_eq!(rt.state.layout_size(SIDEBAR_RAIL_WIDTH_KEY), Some(40));
}

#[test]
fn settings_back_restores_the_actual_screen_and_preserves_its_state() {
    let mut rt = runtime();
    rt.frame(0.0);
    click(&mut rt, "rail-dashboards");
    rt.state.widget_title = rune::TextEdit::new("Unsubmitted dashboard title");
    rt.invalidate();
    rt.frame(0.0);

    click(&mut rt, "rail-settings");
    assert!(key_set(&rt).contains("settings-section-general"));
    assert!(!key_set(&rt).contains("dashboards-page"));
    click(&mut rt, "settings-nav-dataBrowsing");
    assert!(key_set(&rt).contains("settings-section-dataBrowsing"));
    click(&mut rt, "settings-back");

    let keys = key_set(&rt);
    assert_eq!(rt.state.active, Section::Dashboards);
    assert!(keys.contains("dashboards-page") && keys.contains("datazen-rail"));
    assert!(!keys.contains("settings-page") && !keys.contains("settings-back"));
    assert_eq!(rt.state.widget_title.text, "Unsubmitted dashboard title");

    click(&mut rt, "rail-settings");
    assert!(key_set(&rt).contains("settings-section-general"));
    click(&mut rt, "settings-back");
    assert_eq!(rt.state.active, Section::Dashboards);

    click(&mut rt, "rail-connections");
    rt.state.desktop_error = Some("retained connection state".to_owned());
    click(&mut rt, "rail-settings");
    click(&mut rt, "settings-back");
    assert_eq!(rt.state.active, Section::Connections);
    assert_eq!(
        rt.state.desktop_error.as_deref(),
        Some("retained connection state"),
        "returning does not re-run the connection list refresh"
    );
}

#[test]
fn settings_deep_link_keeps_the_original_workspace_and_selected_tab() {
    let mut rt = runtime();
    rt.frame(0.0);
    click(&mut rt, "rail-workspace");
    let tab = rt
        .state
        .tabs
        .open(TabKind::Table, "customers")
        .expect("retained workspace tab");
    rt.state.open_settings(Some("ai"));
    rt.invalidate();
    rt.frame(0.0);
    assert!(key_set(&rt).contains("settings-section-ai"));

    rt.state.open_settings(Some("dataBrowsing"));
    rt.invalidate();
    rt.frame(0.0);
    assert!(key_set(&rt).contains("settings-section-dataBrowsing"));
    click(&mut rt, "settings-back");

    assert_eq!(rt.state.active, Section::Workspace);
    assert_eq!(rt.state.tabs.selected().map(|selected| selected.id), Some(tab));
    assert!(key_set(&rt).contains(&format!("tab-{tab}")));
}

#[test]
fn the_connection_page_fits_both_windows_in_both_themes() {
    // Runtime rects are the observation channel: nothing here trusts a
    // component's own bounds report. Vertical overflow is allowed (the page
    // scrolls); horizontal overflow is not.
    for (width, height) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let env = Env {
                system_dark: dark,
                scale: 1.0,
            };
            let mut rt = Runtime::new(AppState::default(), view, (width, height), env);
            rt.frame(0.0);

            let keys = key_set(&rt);
            for key in ["rune-connection-page", "datazen-body", "content-status-bar"] {
                assert!(
                    keys.contains(key),
                    "{width}x{height} dark={dark}: shell key {key:?} missing"
                );
            }

            for element in rt.elements() {
                let [x, _, w, _] = element.rect;
                assert!(
                    x >= -0.5 && x + w <= width + 0.5,
                    "{width}x{height} dark={dark}: {:?} {:?} hangs past the window horizontally",
                    element.key,
                    element.rect
                );
            }
        }
    }
}

#[test]
fn general_language_keeps_the_real_combobox_through_the_local_field_skin() {
    use datazen_ui_api::{BuiltinLanguage, Setting};
    use rune::{Env, Key, KeyEvent, Mods};

    // AppState intentionally ignores the UI memory fallback for appearance.
    // Read/write through a test desktop settings adapter instead, as the
    // stock host does with its file store. No system-mode substitution.
    struct SettingsFixture(MemoryWorkspace);
    impl DesktopAdapter for SettingsFixture {
        fn list_connections(&mut self) -> Result<Vec<ConnectionProfile>, String> {
            Ok(Vec::new())
        }

        fn read_table_page(
            &mut self, _: &str, _: u32, _: u32, _: Option<&ColumnSort>, _: Option<&ColumnFilter>,
        ) -> Result<TablePage, String> {
            Err(DESKTOP_NOT_IMPLEMENTED.into())
        }

        fn commit_cells(&mut self, _: &str, _: &[(u32, String, String)]) -> Result<u32, String> {
            Err(DESKTOP_NOT_IMPLEMENTED.into())
        }

        fn read_ui_settings(&self) -> Result<Vec<Setting>, String> {
            Ok(self.0.read_settings())
        }

        fn write_ui_setting(&mut self, setting: Setting) -> Result<(), String> {
            self.0.write_setting(setting)
        }
    }

    fn node(rt: &Runtime<AppState>, key: &str) -> rune::ElementInfo {
        rt.elements()
            .into_iter()
            .find(|e| !e.exiting && e.key.as_deref() == Some(key))
            .unwrap_or_else(|| panic!("missing {key}"))
    }

    fn settle(rt: &mut Runtime<AppState>) {
        rt.settle_all();
        rt.frame(1.0 / 60.0);
    }

    fn press(rt: &mut Runtime<AppState>, key: &str) {
        settle(rt);
        let (x, y) = rt.find(key).unwrap_or_else(|| panic!("missing live {key}"));
        rt.pointer_down(x, y);
        rt.frame(1.0 / 60.0);
        rt.pointer_up(x, y);
        rt.frame(1.0 / 60.0);
        settle(rt);
    }

    for (dark, theme) in [(false, "light"), (true, "dark")] {
        let mut state = AppState::default();
        state.desktop = Box::new(SettingsFixture(MemoryWorkspace::new()));
        state.write_ui_setting(Setting::ThemeMode { mode: theme.into() }).unwrap();
        state.write_ui_setting(Setting::BuiltinLanguage { locale: BuiltinLanguage::English }).unwrap();
        let mut rt = Runtime::new(
            state, view, (1440.0, 900.0), Env { scale: 1.0, system_dark: !dark },
        );
        rt.frame(0.0);
        press(&mut rt, "rail-settings");
        assert_eq!(rt.state.settings_section(), "general");
        assert_eq!(rt.state.dark_theme(), dark);

        const TRIGGER: &str = "settings-language-select-trigger";
        const LISTBOX: &str = "settings-language-select-content";
        let field = node(&rt, TRIGGER);
        assert_eq!(field.role, "combobox");
        assert_eq!(field.label.as_deref(), Some("Language"));
        assert!(field.focusable);
        assert_eq!(field.expanded, Some(false));
        assert_eq!(field.rect, [650.0, 103.5, 496.0, 36.0]);
        assert_eq!(field.fill.hex(), if dark { "#0a0d14ff" } else { "#ffffffff" });

        press(&mut rt, TRIGGER);
        assert!(key_set(&rt).contains(LISTBOX));
        assert_eq!(node(&rt, TRIGGER).expanded, Some(true));

        rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
        settle(&mut rt);
        assert!(!key_set(&rt).contains(LISTBOX));
        assert_eq!(rt.state.builtin_language(), BuiltinLanguage::English);

        press(&mut rt, TRIGGER);
        let choice = rt.elements().into_iter()
            .find(|e| !e.exiting && e.role == "option" && e.label.as_deref() == Some("简体中文"))
            .expect("the actual Chinese option is visible");
        // Match the option by its observed label, with its own listbox path.
        press(&mut rt, &choice.path);
        assert_eq!(rt.state.builtin_language(), BuiltinLanguage::SimplifiedChinese);
        assert_eq!(node(&rt, TRIGGER).expanded, Some(false));
        assert!(!key_set(&rt).contains(LISTBOX));
        assert_eq!(node(&rt, "datazen-titlebar-label").text.as_deref(), Some("偏好设置 - DataZen"));
    }
}
