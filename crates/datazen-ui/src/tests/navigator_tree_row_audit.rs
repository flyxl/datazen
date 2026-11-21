//! T470 independent audit: `NavigatorTreeRow` vs React + its ARIA test.
//!
//! Probes branches the coder's suite does not: Space activation, the KV-db
//! click, the object/namespace context menu, the connection chevron's event
//! isolation, the db-open ARIA label, and rect-based indent at both windows in
//! both palettes. Observations come from the runtime tree and host events.

use std::rc::Rc;

use rune::{col, Cx, El, Env, Key, KeyEvent, Mods, Runtime};

use crate::i18n::t;
use crate::views::navigator::navigator_tree_row::{
    navigator_tree_row, ContextMenuTarget, RowAction, RowConnectionStatus, RowKind, RowVm,
    TreeRowHostPort,
};

#[derive(Clone)]
struct Probe {
    events: Vec<RowAction>,
    row: RowVm,
    dark: bool,
}

fn row_view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let host = TreeRowHostPort {
        on_action: Some(Rc::new(move |st: &mut Probe, action| st.events.push(action))),
    };
    col().w_full().h_full().child(navigator_tree_row(&s.row, s.dark, &host))
}

fn runtime(kind: RowKind, dark: bool, size: (f32, f32)) -> Runtime<Probe> {
    let row = RowVm { kind, depth: 2, level_depth: 2 };
    let mut rt = Runtime::new(
        Probe { events: Vec::new(), row, dark },
        row_view,
        size,
        Env { system_dark: dark, scale: 1.0 },
    );
    rt.frame(0.0);
    rt
}

fn click(rt: &mut Runtime<Probe>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
}

fn right_click(rt: &mut Runtime<Probe>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.secondary_down(x, y);
    rt.secondary_up(x, y);
    rt.frame(0.0);
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

/// React's section header handles Enter **or** Space (`e.key === 'Enter' ||
/// e.key === ' '`), toggling its group once per key. The port must not require
/// a pointer press and must not double-fire either key.
#[test]
fn section_toggles_on_space_and_enter_exactly_once() {
    let mut rt = runtime(
        RowKind::Section {
            section: "pinned".into(),
            display_name: "Pinned".into(),
            count: 1,
            expanded: true,
        },
        true,
        (1440.0, 816.0),
    );
    // Focus through the keyboard only: no pointer press that could itself
    // emit ToggleGroup and mask a missing key handler.
    rt.key(KeyEvent { key: Key::Tab, mods: Mods::default() });
    rt.frame(0.0);
    rt.key(KeyEvent { key: Key::Enter, mods: Mods::default() });
    rt.frame(0.0);
    assert_eq!(
        rt.state.events,
        [RowAction::ToggleGroup("__pinned__".to_owned())],
        "Enter must toggle the section exactly once"
    );
    rt.state.events.clear();
    rt.key(KeyEvent { key: Key::Text(" ".to_owned()), mods: Mods::default() });
    rt.frame(0.0);
    assert_eq!(
        rt.state.events,
        [RowAction::ToggleGroup("__pinned__".to_owned())],
        "Space must toggle the section exactly once (React onKeyDown ' ')"
    );
}

/// React `kv-db` rows call `onSelectKvDb(connectionId, dbName)` on click (or
/// the select-connection/table fallback). `RowAction::SelectKvDb` exists but a
/// click must actually emit it.
#[test]
fn kv_db_click_selects_the_kv_database() {
    let mut rt = runtime(
        RowKind::KvDb {
            connection_id: "c2".into(),
            db_session_id: "s2".into(),
            db_name: "db0".into(),
            is_selected: false,
            count: Some(3),
        },
        true,
        (1440.0, 816.0),
    );
    click(&mut rt, "nav-tree-kv-db0");
    assert!(
        rt.state.events.iter().any(|a| matches!(a, RowAction::SelectKvDb { connection_id, db_name }
            if connection_id == "c2" && db_name == "db0")),
        "clicking a kv-db must select it; events {:?}",
        rt.state.events
    );
}

/// The open-database pip announces the short ARIA label
/// (`schemaTree.databaseOpenAria`) in React, not the long tooltip title
/// (`schemaTree.databaseOpen`). A screen reader must hear the same text.
#[test]
fn db_open_pip_announces_the_aria_label() {
    let rt = runtime(
        RowKind::Db {
            connection_id: "c1".into(),
            db_session_id: "s1".into(),
            db_name: "db1".into(),
            expanded: true,
            loading: false,
            is_open: true,
        },
        true,
        (1440.0, 816.0),
    );
    let pip = element(&rt, "data-db-open");
    assert_eq!(
        pip.label.as_deref(),
        Some(t("schemaTree.databaseOpenAria")),
        "open db pip must announce the ARIA label, not the tooltip title"
    );
}

/// React requests an object context menu from `onContextMenu` (right click).
/// A primary press is a separate action (openObject for routines); it must not
/// be the only way to reach the context menu.
#[test]
fn object_right_click_requests_context_menu() {
    let mut rt = runtime(
        RowKind::Object {
            name: "fn_calc".into(),
            schema: None,
            cat_id: "routines".into(),
        },
        true,
        (1440.0, 816.0),
    );
    right_click(&mut rt, "nav-tree-object-fn_calc");
    assert!(
        rt.state.events.iter().any(|a| matches!(a, RowAction::ContextMenu(ContextMenuTarget::Object(n)) if n == "fn_calc")),
        "right-clicking an object must request its context menu; events {:?}",
        rt.state.events
    );
}

/// A namespace leaf's React `onContextMenu` also requests a context menu
/// (object for routines, table otherwise). Right-click must not be a no-op.
#[test]
fn namespace_leaf_right_click_requests_context_menu() {
    let mut rt = runtime(
        RowKind::NamespaceNode {
            name: "tbl".into(),
            expanded: false,
            is_leaf: true,
            key: "db1::tbl".into(),
            connection_id: "c3".into(),
            db_session_id: "s3".into(),
        },
        true,
        (1440.0, 816.0),
    );
    right_click(&mut rt, "nav-tree-namespace-leaf-tbl");
    assert!(
        rt.state.events.iter().any(|a| matches!(a, RowAction::ContextMenu(_))),
        "right-clicking a namespace leaf must request a context menu; events {:?}",
        rt.state.events
    );
}

/// React stops propagation on the connection chevron button, so pressing it
/// toggles the connection without also selecting it. Both intents reaching the
/// host from one click would be a double action.
#[test]
fn chevron_press_does_not_also_select_the_connection() {
    let mut rt = runtime(
        RowKind::Connection {
            id: "c1".into(),
            name: "Orders".into(),
            engine: "pg".into(),
            section_group: Some("team-a".into()),
            is_selected: false,
            status: RowConnectionStatus::Connected,
            expanded: true,
            match_reason: None,
            match_context: None,
            drop_before: false,
            drop_after: false,
        },
        true,
        (1440.0, 816.0),
    );
    click(&mut rt, "nav-conn-toggle-c1");
    assert!(
        rt.state.events.iter().any(|a| matches!(a, RowAction::ToggleConnection { .. })),
        "chevron must toggle the connection; events {:?}",
        rt.state.events
    );
    assert!(
        !rt.state.events.iter().any(|a| matches!(a, RowAction::SelectConnection(_))),
        "chevron press must not also select the row (React stopPropagation); events {:?}",
        rt.state.events
    );
}

/// Rect-based geometry: `paddingLeft` insets the row's children, so the laid
/// out label's rect must move 12pt per depth while the row height stays 24pt,
/// at both windows in both palettes. These read rects, not the indent helper.
#[test]
fn rect_indent_and_height_hold_at_both_sizes_and_palettes() {
    let kind = RowKind::Table {
        name: "users".into(),
        schema: Some("public".into()),
        cat_id: "tables".into(),
        is_selected: false,
        connection_id: "c1".into(),
        db_session_id: "s1".into(),
        db_name: "db1".into(),
    };
    for dark in [true, false] {
        for size in [(960.0, 640.0), (1440.0, 816.0)] {
            let mut prev_x: Option<f32> = None;
            for depth in [0usize, 1, 2, 3] {
                let row = RowVm { kind: kind.clone(), depth, level_depth: depth };
                let mut rt = Runtime::new(
                    Probe { events: Vec::new(), row, dark },
                    row_view,
                    size,
                    Env { system_dark: dark, scale: 1.0 },
                );
                rt.frame(0.0);
                let row_rect = element(&rt, "nav-tree-table-users").rect;
                assert!(
                    (row_rect[3] - 24.0).abs() < 0.5,
                    "row height 24pt at {size:?} dark={dark}: {row_rect:?}"
                );
                assert!(
                    row_rect[0] + row_rect[2] <= size.0 + 0.5,
                    "row stays inside the window at {size:?}: {row_rect:?}"
                );
                let label_x = rt
                    .elements()
                    .into_iter()
                    .find(|e| !e.exiting && e.text.as_deref() == Some("users"))
                    .unwrap_or_else(|| panic!("users label laid out at depth {depth}"))
                    .rect[0];
                if let Some(previous) = prev_x {
                    assert!(
                        (label_x - previous - 12.0).abs() < 0.5,
                        "depth {depth} label must shift 12pt from the previous depth at {size:?} dark={dark}: {previous} -> {label_x}"
                    );
                }
                prev_x = Some(label_x);
            }
        }
    }
}
