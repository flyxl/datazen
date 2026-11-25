//! Independent audit of the T067 Rune port of ConnectionNavigatorTree.
//!
//! These tests build their own observation helpers (element keys, runtime rects
//! and the accessibility tree) and do not call any helper the implementation
//! uses. They target branches the feature's own suite leaves unverified:
//! isolated Left/Right keyboard toggling, trim + case-insensitive filtering,
//! the "mounted + selected" contract of `active_descendant`, key preservation,
//! and object-tree geometry at the two required window sizes.

use datazen_ui::test_support::*;

use rune::ElementInfo;
use serde_json::Value;

fn conns(n: usize) -> Vec<ConnectionProfile> {
    (0..n)
        .map(|i| ConnectionProfile {
            id: format!("c{i:02}"),
            name: format!("Local {i:02}"),
            engine: if i % 2 == 0 { "SQLite" } else { "PostgreSQL" }.to_owned(),
        })
        .collect()
}

fn cat() -> datazen_ui_api::Catalog {
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

fn rt_at(size: (f32, f32), dark: bool, n: usize) -> Runtime<AppState> {
    let mut rt = Runtime::new(
        AppState::default(),
        view,
        size,
        Env { system_dark: dark, scale: 1.0 },
    );
    rt.state.write_ui_setting(Setting::Theme { dark }).ok();
    rt.state.desktop_connections = Some(conns(n));
    rt.state.desktop_catalog = Some(cat());
    for _ in 0..4 {
        rt.invalidate();
        rt.frame(1.0 / 60.0);
    }
    rt
}

fn by_key<'a>(rt: &'a Runtime<AppState>, key: &str) -> ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("audit: missing element {key}"))
}

fn mounted(rt: &Runtime<AppState>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

/// The drawn left edge of the first text label inside a keyed row. The row's
/// own rect spans the column; the indent lives in the row's left padding, so
/// the child label is the observable channel for depth.
fn row_label_x(rt: &Runtime<AppState>, key: &str) -> f32 {
    let row = by_key(rt, key);
    let prefix = format!("{}/", row.path);
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.path.starts_with(&prefix) && e.text.is_some())
        .unwrap_or_else(|| panic!("audit: no label under {key}"))
        .rect[0]
}

fn click(rt: &mut Runtime<AppState>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("audit: no point for {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    for _ in 0..4 {
        rt.invalidate();
        rt.frame(1.0 / 60.0);
    }
}

fn find_key<'a>(node: &'a Value, key: &str) -> Option<&'a Value> {
    if node.get("key").and_then(Value::as_str) == Some(key) {
        return Some(node);
    }
    node.get("children")?.as_array()?.iter().find_map(|c| find_key(c, key))
}

/// The raw `activeDescendant` string on the connections tree and whether that
/// key currently resolves to a mounted element that reports `selected`.
fn a11y(rt: &Runtime<AppState>) -> (Option<String>, bool) {
    let json = rt.accessibility_json();
    let tree = find_key(&json, "navigator-connections").expect("audit: tree in a11y");
    let active = tree
        .get("activeDescendant")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let resolves = active
        .as_deref()
        .and_then(|k| find_key(&json, k))
        .map(|row| {
            row.get("states")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).any(|s| s == "selected"))
                .unwrap_or(false)
        })
        .unwrap_or(false);
    (active, resolves)
}

// ---------------------------------------------------------------------------
// A. Left/Right keyboard toggling, isolated from the click used to gain focus.
// ---------------------------------------------------------------------------
#[test]
fn audit_left_right_keys_toggle_groups_independently() {
    let mut rt = rt_at((1440.0, 816.0), true, 6);

    // Click focuses AND presses, so Recent is collapsed after this.
    click(&mut rt, "nav-group-recent");
    assert_eq!(by_key(&rt, "nav-group-recent").expanded, Some(false));

    // Right on the focused header expands it (this is the `!expanded` branch).
    rt.key(rune::KeyEvent { key: rune::Key::Right, mods: rune::Mods::default() });
    for _ in 0..4 { rt.invalidate(); rt.frame(1.0 / 60.0); }
    assert_eq!(
        by_key(&rt, "nav-group-recent").expanded,
        Some(true),
        "Right key must expand the focused header"
    );

    // Left on the focused header collapses it (the `expanded` branch that the
    // feature suite never actually exercises).
    rt.key(rune::KeyEvent { key: rune::Key::Left, mods: rune::Mods::default() });
    for _ in 0..4 { rt.invalidate(); rt.frame(1.0 / 60.0); }
    assert_eq!(
        by_key(&rt, "nav-group-recent").expanded,
        Some(false),
        "Left key must collapse the focused header"
    );
}

// ---------------------------------------------------------------------------
// B. active_descendant must name a row that is actually mounted + selected.
// ---------------------------------------------------------------------------
#[test]
fn audit_active_descendant_never_points_at_an_unmounted_row() {
    let mut rt = rt_at((1440.0, 816.0), true, 6);

    rt.state.desktop_connection = Some("c00".into());
    rt.pointer_leave();
    for _ in 0..6 { rt.invalidate(); rt.frame(1.0 / 60.0); }

    let (active, resolves) = a11y(&rt);
    assert_eq!(active.as_deref(), Some("nav-conn-c00"), "baseline active descendant");
    assert!(resolves, "baseline active descendant resolves to a selected row");

    // Collapse the All group: the selected connection's row unmounts.
    click(&mut rt, "nav-group-all");
    assert!(!mounted(&rt, "nav-conn-c00"), "the selected row unmounted with its group");

    let (active2, resolves2) = a11y(&rt);
    assert!(
        active2.is_none() || resolves2,
        "active_descendant={active2:?} names a row that is no longer mounted/selected \
         after the group collapsed (React only announces rows present in flatRows)"
    );
}

// ---------------------------------------------------------------------------
// C. Filtering matches React semantics: lowercase + trim of the query.
// ---------------------------------------------------------------------------
#[test]
fn audit_filter_trims_and_is_case_insensitive() {
    let mut rt = rt_at((1440.0, 816.0), true, 3);

    rt.state.navigator_filter = rune::TextEdit::new("  LOCAL 01  ".to_owned());
    for _ in 0..4 { rt.invalidate(); rt.frame(1.0 / 60.0); }
    assert!(mounted(&rt, "nav-conn-c01"), "trimmed, lower-cased name matches");
    assert!(!mounted(&rt, "nav-conn-c00"));
    assert!(!mounted(&rt, "nav-conn-c02"));

    rt.state.navigator_filter = rune::TextEdit::new("SQLITE".to_owned());
    for _ in 0..4 { rt.invalidate(); rt.frame(1.0 / 60.0); }
    assert!(mounted(&rt, "nav-conn-c00"), "upper-case engine matches case-insensitively");
    assert!(mounted(&rt, "nav-conn-c02"));
    assert!(!mounted(&rt, "nav-conn-c01"));
}

// ---------------------------------------------------------------------------
// D. Every contract key survives the port (no silent rename).
// ---------------------------------------------------------------------------
#[test]
fn audit_all_contract_keys_are_present() {
    let mut rt = rt_at((1440.0, 816.0), true, 4);
    click(&mut rt, "nav-conn-c02"); // populate Recent so its row mounts

    for key in [
        "navigator-connections-title",
        "open-query",
        "navigator-filter-input",
        "navigator-connections",
        "nav-group-recent",
        "nav-group-all",
        "nav-conn-c02",
        "nav-conn-recent-c02",
        "object-tree",
        "tree-db-app",
        "tree-schema-app-public",
        "tree-table-users",
        "tree-table-orders",
    ] {
        assert!(mounted(&rt, key), "contract key missing: {key}");
    }
}

// ---------------------------------------------------------------------------
// E2. The revealed cursor stays put across 24 frames (no heartbeat churn).
// ---------------------------------------------------------------------------
#[test]
fn audit_reveal_stays_stable_over_24_frames() {
    let mut rt = rt_at((960.0, 640.0), true, 48);
    rt.pointer_leave();
    for _ in 0..4 { rt.invalidate(); rt.frame(1.0 / 60.0); }
    rt.state.desktop_connection = Some("c40".into());
    for _ in 0..6 { rt.invalidate(); rt.frame(1.0 / 60.0); }

    let row0 = by_key(&rt, "nav-conn-c40").rect;
    let tree0 = by_key(&rt, "navigator-connections").rect;

    for _ in 0..24 {
        rt.frame(1.0 / 60.0);
    }

    let row1 = by_key(&rt, "nav-conn-c40").rect;
    let tree1 = by_key(&rt, "navigator-connections").rect;
    for i in 0..4 {
        assert!((row0[i] - row1[i]).abs() < 0.5, "row rect drifted at {i}: {row0:?} -> {row1:?}");
        assert!((tree0[i] - tree1[i]).abs() < 0.5, "tree rect drifted at {i}: {tree0:?} -> {tree1:?}");
    }
    assert!(
        row1[1] >= tree1[1] - 1.0 && row1[1] + row1[3] <= tree1[1] + tree1[3] + 1.0,
        "row left the viewport after 24 frames: row={row1:?} tree={tree1:?}"
    );
}

// ---------------------------------------------------------------------------
// E. Object tree indents deepen with depth and stay inside both windows.
// ---------------------------------------------------------------------------
#[test]
fn audit_object_tree_indent_and_bounds_at_both_sizes() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let where_ = format!("{w}x{h} dark={dark}");
            let rt = rt_at((w, h), dark, 4);

            let db = row_label_x(&rt, "tree-db-app");
            let schema = row_label_x(&rt, "tree-schema-app-public");
            let table = row_label_x(&rt, "tree-table-users");
            assert!(
                db < schema && schema < table,
                "{where_}: label indent must deepen db<schema<table, got {db} < {schema} < {table}"
            );
            for e in rt.elements() {
                let [x, y, ew, eh] = e.rect;
                assert!(
                    x.is_finite() && y.is_finite() && ew.is_finite() && eh.is_finite(),
                    "{where_}: non-finite rect on {:?}: {:?}",
                    e.key, e.rect
                );
                assert!(
                    x >= -0.5 && x + ew <= w + 0.5,
                    "{where_}: {:?} overflows {w}: {:?}",
                    e.key, e.rect
                );
            }
        }
    }
}
