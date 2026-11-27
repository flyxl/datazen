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
        assert_eq!(
            title.text.as_deref(),
            Some(section.label()),
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

fn click(rt: &mut Runtime<AppState>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is reachable"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.frame(0.0);
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
