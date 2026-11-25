//! T067: the Rune port of `windows/connection/ConnectionNavigatorTree.tsx`.
//!
//! Every assertion observes the product through the runtime's own channels —
//! the element tree (keys, rects, focus, selected state), point picking by key,
//! and the accessibility tree — never through a helper the implementation
//! calls. Geometry is measured from element rects at both required window sizes
//! in both themes, so a self-reported "in bounds" flag cannot make a red green.

use datazen_ui::test_support::*;

use datazen_ui::i18n::t;
use rune::{ElementInfo, Key, KeyEvent, Mods};
use serde_json::Value;

/// Controlled connections: ids are addressable (`c00` …) and names/engines are
/// distinct so filtering by each of the three fields can be told apart.
fn connections(n: usize) -> Vec<ConnectionProfile> {
    (0..n)
        .map(|i| ConnectionProfile {
            id: format!("c{i:02}"),
            name: format!("Local {i:02}"),
            engine: if i % 2 == 0 { "SQLite" } else { "PostgreSQL" }.to_owned(),
        })
        .collect()
}

fn catalog() -> datazen_ui_api::Catalog {
    datazen_ui_api::Catalog {
        databases: vec![datazen_ui_api::DatabaseNode {
            name: "app".into(),
            schemas: vec![datazen_ui_api::SchemaNode {
                name: "public".into(),
                tables: vec!["users".into(), "orders".into()],
            }],
        }],
    }
}

fn nav_rt(size: (f32, f32), dark: bool, n: usize) -> Runtime<AppState> {
    let mut rt = Runtime::new(
        AppState::default(),
        view,
        size,
        Env { system_dark: dark, scale: 1.0 },
    );
    rt.state.write_ui_setting(Setting::Theme { dark }).ok();
    rt.state.desktop_connections = Some(connections(n));
    rt.state.desktop_catalog = Some(catalog());
    settle(&mut rt);
    rt
}

fn settle(rt: &mut Runtime<AppState>) {
    for _ in 0..4 {
        rt.invalidate();
        rt.frame(1.0 / 60.0);
    }
}

fn element(rt: &Runtime<AppState>, key: &str) -> ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("missing element {key}"))
}

fn has(rt: &Runtime<AppState>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

/// Text drawn inside a keyed row, found through the runtime's element paths —
/// the row itself is a container and has no `text` of its own.
fn row_text(rt: &Runtime<AppState>, key: &str) -> Option<String> {
    let row = element(rt, key);
    let prefix = format!("{}/", row.path);
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && (e.path.starts_with(&prefix) || e.path == row.path) && e.text.is_some())
        .and_then(|e| e.text)
}

fn press_key(rt: &mut Runtime<AppState>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no point for {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(rt);
}

fn find_json<'a>(node: &'a Value, key: &str) -> Option<&'a Value> {
    if node.get("key").and_then(Value::as_str) == Some(key) {
        return Some(node);
    }
    node.get("children")?.as_array()?.iter().find_map(|c| find_json(c, key))
}

/// `(activeDescendant, the key it resolves to when that row is selected)`.
fn a11y_cursor(rt: &Runtime<AppState>) -> (Option<String>, Option<String>) {
    let json = rt.accessibility_json();
    let tree = find_json(&json, "navigator-connections").expect("the connections tree is in a11y");
    let active = tree
        .get("activeDescendant")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let selected = active.as_deref().and_then(|key| find_json(&json, key)).and_then(|row| {
        let states = row
            .get("states")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).collect::<Vec<_>>())
            .unwrap_or_default();
        states.contains(&"selected").then(|| row.get("key").and_then(Value::as_str).unwrap().to_owned())
    });
    (active, selected)
}

// ---------------------------------------------------------------------------
// 1. Group collapse/expand and filter-driven auto-expand, with exact keys.
// ---------------------------------------------------------------------------

#[test]
fn group_headers_toggle_by_press_and_keyboard_and_filter_auto_expands() {
    let mut rt = nav_rt((1440.0, 816.0), true, 6);

    let recent = element(&rt, "nav-group-recent");
    let all = element(&rt, "nav-group-all");
    assert_eq!(recent.expanded, Some(true), "Recent starts expanded");
    assert_eq!(all.expanded, Some(true), "All starts expanded");
    assert!(has(&rt, "nav-conn-c00"), "a connection row is mounted");

    // Press collapses All; its rows unmount while the header stays addressable.
    press_key(&mut rt, "nav-group-all");
    assert_eq!(element(&rt, "nav-group-all").expanded, Some(false), "press collapsed All");
    assert!(!has(&rt, "nav-conn-c00"), "collapsed All hides its rows");
    assert!(has(&rt, "nav-conn-recent-c00") || has(&rt, "nav-group-recent-empty"));

    // Press again re-expands it.
    press_key(&mut rt, "nav-group-all");
    assert_eq!(element(&rt, "nav-group-all").expanded, Some(true), "press re-expanded All");
    assert!(has(&rt, "nav-conn-c00"), "rows are back");

    // Right/Left on the focused header collapse and expand it.
    let (x, y) = rt.find("nav-group-recent").expect("recent header");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(&mut rt);
    rt.key(KeyEvent { key: Key::Left, mods: Mods::default() });
    settle(&mut rt);
    assert_eq!(element(&rt, "nav-group-recent").expanded, Some(false), "Left collapsed Recent");
    rt.key(KeyEvent { key: Key::Right, mods: Mods::default() });
    settle(&mut rt);
    assert_eq!(element(&rt, "nav-group-recent").expanded, Some(true), "Right expanded Recent");

    // A non-empty filter forces both groups open, even after an explicit
    // collapse — this is React's `filtering || expanded` behaviour.
    press_key(&mut rt, "nav-group-recent");
    press_key(&mut rt, "nav-group-all");
    assert_eq!(element(&rt, "nav-group-all").expanded, Some(false));
    rt.state.navigator_filter = rune::TextEdit::new("c03".to_owned());
    settle(&mut rt);
    assert_eq!(element(&rt, "nav-group-recent").expanded, Some(true), "filter auto-expands Recent");
    assert_eq!(element(&rt, "nav-group-all").expanded, Some(true), "filter auto-expands All");
    assert!(has(&rt, "nav-conn-c03"), "the match is visible despite the earlier collapse");
}

// ---------------------------------------------------------------------------
// 2. Connection row filtering (name / engine / id) and per-group empty states.
// ---------------------------------------------------------------------------

#[test]
fn filter_narrows_connection_rows_and_shows_per_group_empty_states() {
    let mut rt = nav_rt((1440.0, 816.0), true, 8);

    assert!(has(&rt, "navigator-filter-input"), "the keyed input survives the port");
    assert!(has(&rt, "navigator-connection-search-input"), "React testid key added on the wrapper");
    assert!(has(&rt, "nav-group-recent-empty"), "Recent is empty before any open");

    // By name.
    rt.state.navigator_filter = rune::TextEdit::new("Local 05".to_owned());
    settle(&mut rt);
    assert!(has(&rt, "nav-conn-c05"), "name match stays");
    for i in [0, 1, 2, 3, 4, 6, 7] {
        assert!(!has(&rt, &format!("nav-conn-c{i:02}")), "non-match c{i:02} is filtered out");
    }

    // By engine (case-insensitive substring of the engine label).
    rt.state.navigator_filter = rune::TextEdit::new("postgres".to_owned());
    settle(&mut rt);
    assert!(has(&rt, "nav-conn-c01"), "engine match stays");
    assert!(!has(&rt, "nav-conn-c00"), "SQLite row filtered out");

    // By id.
    rt.state.navigator_filter = rune::TextEdit::new("c07".to_owned());
    settle(&mut rt);
    assert!(has(&rt, "nav-conn-c07"));

    // No match: both groups stay expanded and show their own empty row with
    // the no-match translation, not a silent blank area.
    rt.state.navigator_filter = rune::TextEdit::new("zzz-no-match".to_owned());
    settle(&mut rt);
    let no_match = t("select.noMatches");
    assert_eq!(row_text(&rt, "nav-group-all-empty").as_deref(), Some(no_match), "All empty text");
    assert_eq!(row_text(&rt, "nav-group-recent-empty").as_deref(), Some(no_match), "Recent empty text");
    assert!(!has(&rt, "nav-conn-c00"), "no rows survive the filter");

    // Clearing restores the no-connections sentence for the never-populated
    // Recent section, while All is populated again.
    rt.state.navigator_filter = rune::TextEdit::new(String::new());
    settle(&mut rt);
    assert_eq!(row_text(&rt, "nav-group-recent-empty").as_deref(), Some(t("main.noConnections")));
    assert!(!has(&rt, "nav-group-all-empty"), "All has rows again");
}

// ---------------------------------------------------------------------------
// 3. Tree selection / keyboard / reveal after an external selection change.
// ---------------------------------------------------------------------------

#[test]
fn tree_selects_by_keyboard_and_reveals_an_externally_selected_row() {
    let mut rt = nav_rt((960.0, 640.0), true, 48);

    let tree = element(&rt, "navigator-connections");
    assert_eq!(tree.role, "tree", "the container exposes the tree role");
    assert!(element(&rt, "nav-group-all").focusable, "headers join the tree focus group");
    assert!(element(&rt, "nav-conn-c00").focusable, "rows join the tree focus group");
    assert_eq!(element(&rt, "nav-conn-c00").role, "treeitem");

    // Press a row: it is the Rune equivalent of selecting + opening it.
    press_key(&mut rt, "nav-conn-c00");
    assert_eq!(
        rt.state.recent_connection_ids.first().map(String::as_str),
        Some("c00"),
        "pressing a row records it as recent"
    );

    // Down moves focus to the next row; Enter presses it. Space presses too.
    rt.key(KeyEvent { key: Key::Down, mods: Mods::default() });
    settle(&mut rt);
    let focused = rt
        .elements()
        .into_iter()
        .find(|e| e.focused)
        .map(|e| e.key.unwrap_or_default())
        .unwrap_or_default();
    assert_eq!(focused, "nav-conn-c01", "Down roves to the next connection row: {focused}");
    rt.key(KeyEvent { key: Key::Enter, mods: Mods::default() });
    settle(&mut rt);
    assert_eq!(
        rt.state.recent_connection_ids.first().map(String::as_str),
        Some("c01"),
        "Enter presses the focused row"
    );
    rt.key(KeyEvent { key: Key::Down, mods: Mods::default() });
    settle(&mut rt);
    rt.key(KeyEvent { key: Key::Text(" ".into()), mods: Mods::default() });
    settle(&mut rt);
    assert_eq!(
        rt.state.recent_connection_ids.first().map(String::as_str),
        Some("c02"),
        "Space presses the focused row"
    );
    // Home/End keep focus inside the group.
    rt.key(KeyEvent { key: Key::End, mods: Mods::default() });
    settle(&mut rt);
    let at_end = rt.elements().into_iter().find(|e| e.focused).map(|e| e.key.unwrap_or_default());
    assert!(at_end.as_deref().is_some_and(|k| k.starts_with("nav-conn-") || k == "nav-group-all"));

    // External selection (the `model.select` equivalent) is observed through
    // the accessibility tree: active_descendant must name a mounted, selected
    // row. Clear the pointer first: rune deliberately suppresses reveal while
    // a pointer hovers a row, so the observation must be a pure state change.
    rt.pointer_leave();
    settle(&mut rt);
    rt.state.desktop_connection = Some("c40".into());
    settle(&mut rt);
    let (active, selected) = a11y_cursor(&rt);
    assert_eq!(active.as_deref(), Some("nav-conn-c40"), "active descendant follows selection");
    assert_eq!(selected.as_deref(), Some("nav-conn-c40"), "active descendant is selected and mounted");
    assert!(element(&rt, "nav-conn-c40").item_selected, "the selected row reports item_selected");

    // ...and the selected row is revealed by scrolling, observed from the row's
    // own rect and by point picking (not from any product-side "scrolled" flag).
    let row = element(&rt, "nav-conn-c40");
    let tree = element(&rt, "navigator-connections");
    assert!(
        row.rect[1] >= tree.rect[1] - 1.0 && row.rect[1] + row.rect[3] <= tree.rect[1] + tree.rect[3] + 1.0,
        "selected row is inside the tree viewport: row={:?} tree={:?}",
        row.rect,
        tree.rect
    );
    let (x, y) = rt.find("nav-conn-c40").expect("the revealed row has a pickable point");
    let picked = rt.element_at(x, y).expect("a pickable element at the revealed row's point");
    let prefix = format!("{}/", row.path);
    assert!(
        picked.key() == Some("nav-conn-c40") || picked.path().starts_with(&prefix),
        "the revealed row is what a click at its key position hits: {:?} / {:?}",
        picked.key(),
        picked.path()
    );
}

// ---------------------------------------------------------------------------
// 4. Click target resolution through DOM keys, never viewport coordinates.
// ---------------------------------------------------------------------------

#[test]
fn every_named_target_is_hittable_at_its_key_position() {
    let mut rt = nav_rt((1440.0, 816.0), true, 5);
    press_key(&mut rt, "nav-conn-c02"); // populate Recent so its row exists

    let targets = [
        "navigator-connections-title",
        "open-query",
        "navigator-filter-input",
        "nav-group-recent",
        "nav-group-all",
        "nav-conn-c02",
        "nav-conn-recent-c02",
        "object-tree",
        "tree-db-app",
        "tree-schema-app-public",
        "tree-table-users",
    ];
    for key in targets {
        let target = element(&rt, key);
        let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no pickable point for {key}"));
        let picked = rt.element_at(x, y).unwrap_or_else(|| panic!("nothing pickable at {key}'s point"));
        let prefix = format!("{}/", target.path);
        assert!(
            picked.key() == Some(key) || picked.path().starts_with(&prefix),
            "clicking {key} at its own key point hits {:?} at {:?}, outside the keyed subtree",
            picked.key(),
            picked.path()
        );
    }

    // A resolved click performs the contract action, not merely a paint check.
    press_key(&mut rt, "nav-group-all");
    assert_eq!(element(&rt, "nav-group-all").expanded, Some(false), "key-resolved click collapses All");
    press_key(&mut rt, "nav-conn-recent-c02");
    assert_eq!(
        rt.state.recent_connection_ids.first().map(String::as_str),
        Some("c02"),
        "key-resolved click on a Recent row selects it"
    );
}

// ---------------------------------------------------------------------------
// 5. Geometry at both required sizes in both themes, from element rects.
// ---------------------------------------------------------------------------

fn assert_no_horizontal_overflow(rt: &Runtime<AppState>, width: f32, where_: &str) {
    for e in rt.elements() {
        let [x, _, w, _] = e.rect;
        assert!(
            x >= -0.5 && x + w <= width + 0.5,
            "{where_}: {:?} rect {:?} hangs past the {width}pt window",
            e.key,
            e.rect
        );
    }
}

#[test]
fn navigator_fits_both_window_sizes_in_both_themes() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let where_ = format!("{w}x{h} dark={dark}");
            let mut rt = nav_rt((w, h), dark, 14);
            press_key(&mut rt, "nav-conn-c03");

            for key in [
                "datazen-navigator",
                "navigator-title-row",
                "navigator-connections-title",
                "navigator-connection-search-input",
                "navigator-filter-input",
                "navigator-connections",
                "nav-group-recent",
                "nav-group-all",
                "object-tree",
            ] {
                let e = element(&rt, key);
                assert!(e.rect[2] > 0.0 && e.rect[3] > 0.0, "{where_}: {key} has no area: {:?}", e.rect);
            }
            assert_no_horizontal_overflow(&rt, w, &where_);

            // Filtering keeps the same no-overflow invariant with all rows visible.
            rt.state.navigator_filter = rune::TextEdit::new("c1".to_owned());
            settle(&mut rt);
            assert_no_horizontal_overflow(&rt, w, &format!("{where_} filtered"));
        }
    }
}
