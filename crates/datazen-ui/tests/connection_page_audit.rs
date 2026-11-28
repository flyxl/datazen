//! Independent audit for T110: the `ConnectionPage.tsx` → `connection_page.rs`
//! container port. These are observer-side checks on the laid-out element tree
//! (keys and runtime rects), not on any component's self-report. They target
//! branches the port-mover could silently drop: the root paste glue, the full
//! contract-key families the React page composes, and a stronger geometry
//! assertion than "nothing hangs past the edge" (a collapsed body passes that
//! test but is visibly wrong).

use datazen_ui::test_support::*;

fn key_set(rt: &Runtime<AppState>) -> std::collections::HashSet<String> {
    // Rune keeps exiting nodes in `elements()` for animation; they are inert
    // and stale. Only non-exiting elements are actually mounted in the current
    // tree, so a key-presence assertion after a state transition must ignore
    // them or it passes whether or not the new state mounted the key.
    rt.elements()
        .into_iter()
        .filter(|e| !e.exiting)
        .filter_map(|e| e.key)
        .collect()
}

/// The React page composes these key families into its shell. With a session
/// open the navigator mounts its object tree, and on the Workspace section the
/// tab strip mounts. The port must keep every one of them reachable.
#[test]
fn the_port_keeps_the_contract_key_families_composed() {
    use datazen_ui_api::WorkspaceApi;

    let mut rt = runtime();
    rt.state.workspace.connect("local").expect("memory session");
    rt.frame(0.0);

    let keys = key_set(&rt);
    for key in [
        "datazen-root",
        "rune-connection-page",
        "datazen-titlebar",
        "datazen-body",
        "datazen-navigator",
        "navigator-toolbar",
        "navigator-title-row",
        "navigator-connection-search-input",
        "navigator-filter-input",
        "object-tree",
        "datazen-page",
        "content-toolbar",
        "content-toolbar-title",
        "content-status-bar-absent",
    ] {
        assert!(
            keys.contains(key),
            "composed contract key {key:?} is missing: {keys:?}"
        );
    }

    // Tabs are a Workspace-only mount (React ContentView). The tree must still
    // produce `tab-strip` once that section is active.
    rt.state.active = Section::Workspace;
    let tab = rt
        .state
        .tabs
        .open(TabKind::Table, "customers")
        .expect("a tab");
    let _ = tab;
    rt.invalidate();
    rt.frame(0.0);
    let keys = key_set(&rt);
    for key in ["tab-strip", "content-toolbar", "datazen-page", "content-status-bar"] {
        assert!(
            keys.contains(key),
            "Workspace contract key {key:?} is missing: {keys:?}"
        );
    }
}

/// The tab strip is a Workspace-only mount (React ContentView). Assert both
/// sides of that branch through observable keys, using an explicit invalidate
/// and a second frame after the rail press so the observation cannot trail the
/// state change.
#[test]
fn the_tab_strip_mounts_only_on_workspace_after_the_port() {
    let mut rt = runtime();
    rt.frame(0.0);
    let keys = key_set(&rt);
    assert!(
        keys.contains("tab-strip-absent") && !keys.contains("tab-strip"),
        "Connections has no tab strip: {keys:?}"
    );

    let (x, y) = rt
        .find(&format!("rail-{}", Section::Workspace.key()))
        .expect("workspace rail item");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.invalidate();
    rt.frame(0.0);
    rt.frame(0.0);
    assert_eq!(rt.state.active, Section::Workspace);
    let keys = key_set(&rt);
    assert!(
        keys.contains("tab-strip") && !keys.contains("tab-strip-absent"),
        "Workspace mounts the tab strip: {keys:?}"
    );
}

/// The root `on_paste` closure was moved verbatim from `app.rs::view` into the
/// new container. With a loaded grid and no focused field, an OS paste must
/// still reach the grid through the composed root — a port that forgets the
/// handler renders correctly but silently breaks pasting.
#[test]
fn the_composed_root_still_routes_paste_to_the_grid() {
    use datazen_ui_api::WorkspaceApi;

    let mut rt = runtime();
    rt.state.workspace.connect("local").expect("memory session");
    rt.state.open_table_tab("customers").expect("a table tab");
    rt.frame(0.0);
    assert_eq!(rt.state.active, Section::Workspace);
    assert!(rt.state.grid.has_page(), "the grid has a loaded page");

    // No field is focused, so the paste falls through to the app/root handler.
    rt.paste("Z9\tY8");
    rt.frame(0.0);
    assert_eq!(
        rt.state.grid.cell(0, 0),
        "Z9",
        "root paste writes the first column of the focus cell"
    );
    assert_eq!(
        rt.state.grid.cell(0, 1),
        "Y8",
        "root paste writes the second column of the focus cell"
    );
}

/// A no-horizontal-overflow check alone is weak: a zero-width body trivially
/// satisfies it. Assert the composed page actually fills the window in both
/// sizes and both palettes, via the laid-out rects.
#[test]
fn the_composed_page_fills_both_windows_in_both_themes() {
    for (width, height) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let env = Env {
                system_dark: dark,
                scale: 1.0,
            };
            let mut rt = Runtime::new(AppState::default(), view, (width, height), env);
            rt.frame(0.0);
            let els = rt.elements();
            let by_key = |k: &str| {
                els.iter()
                    .find(|e| e.key.as_deref() == Some(k))
                    .unwrap_or_else(|| panic!("{k} is laid out at {width}x{height} dark={dark}"))
            };

            // ElementInfo.rect is [x, y, w, h]; the right edge is x + w.
            let root = by_key("datazen-root");
            assert!(
                root.rect[0].abs() <= 0.5
                    && (root.rect[0] + root.rect[2] - width).abs() <= 0.5,
                "{width}x{height} dark={dark}: root {:?} does not span the window",
                root.rect
            );

            let page = by_key("rune-connection-page");
            assert!(
                (page.rect[0] + page.rect[2] - width).abs() <= 0.5,
                "{width}x{height} dark={dark}: page {:?} does not span the width",
                page.rect
            );
            assert!(
                (page.rect[3] - height).abs() <= 1.0,
                "{width}x{height} dark={dark}: page {:?} does not span the height",
                page.rect
            );

            let body = by_key("datazen-body");
            assert!(
                body.rect[0].abs() <= 0.5
                    && (body.rect[0] + body.rect[2] - width).abs() <= 0.5,
                "{width}x{height} dark={dark}: body {:?} does not span the window",
                body.rect
            );

            let content = by_key("datazen-page");
            assert!(
                (content.rect[0] + content.rect[2] - width).abs() <= 0.5,
                "{width}x{height} dark={dark}: content {:?} does not reach the right edge",
                content.rect
            );
        }
    }
}
