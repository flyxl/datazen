//! Independent audit of the T0-09 table base (commit 7ca97f558): the kit-reuse
//! claim, positional keys after sorting, selection paint, pointer/keyboard
//! semantics, the no-handler tab order, and a 60-row bounded window.

use crate::ui::table::{CellValue, Column, DataTable, Row};
use rune::{col, text, Align, Cx, El, Env, Key, KeyEvent, Mods, Runtime};
use rune_kit::{SortDir, TableStyle};

const KEY: &str = "tbl";

fn env() -> Env {
    Env {
        system_dark: true,
        scale: 1.0,
    }
}

fn columns() -> Vec<Column> {
    vec![
        Column::new("Name"),
        Column::new("Size").width(90.0).align(Align::End),
        Column::new("Rows").width(80.0).align(Align::End),
    ]
}

#[derive(Default)]
struct Audit {
    rows: Vec<Vec<&'static str>>,
    selected: Option<usize>,
    selects: Vec<usize>,
    activations: Vec<usize>,
    sort: Option<(usize, SortDir)>,
    select: bool,
    activate: bool,
    grow: bool,
    tail: bool,
}

fn view(state: &Audit, _cx: &mut Cx<Audit>) -> El {
    let columns = columns();
    let rows = state.rows.iter().map(|row| {
        row.iter()
            .map(|cell| CellValue::from(*cell))
            .collect::<Vec<_>>()
    });
    let mut table = DataTable::new(KEY, &columns)
        .rows(rows)
        .selected(state.selected);
    if state.select {
        table = table.on_select(|s: &mut Audit, i| {
            s.selected = Some(i);
            s.selects.push(i);
        });
    }
    if state.activate {
        table = table.on_activate(|s: &mut Audit, i| s.activations.push(i));
    }
    if let Some((column, dir)) = state.sort {
        table = table.sort(column, dir);
    }
    if state.grow {
        table = table.grow();
    }
    let mut root = col()
        .w_full()
        .h_full()
        .padding(24.0)
        .child(table.build(true));
    if state.tail {
        root = root.child(text("after the table").key("below"));
    }
    root
}

fn probe(state: Audit, size: (f32, f32)) -> Runtime<Audit> {
    let mut rt = Runtime::new(state, view, size, env());
    rt.frame(0.0);
    rt
}

fn press(rt: &mut Runtime<Audit>, key: Key) {
    rt.key(KeyEvent {
        key,
        mods: Mods::default(),
    });
    rt.frame(1.0 / 60.0);
}

fn click(rt: &mut Runtime<Audit>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn focused(rt: &Runtime<Audit>) -> Option<String> {
    rt.elements()
        .into_iter()
        .find(|element| element.focused)
        .and_then(|element| element.key)
}

fn element(rt: &Runtime<Audit>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|element| element.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} laid out"))
}

fn rows3() -> Vec<Vec<&'static str>> {
    vec![
        vec!["Alpha", "10", "100"],
        vec!["Bravo", "2", "20"],
        vec!["Charlie", "1", "1"],
    ]
}

/// A table with neither handler must not put its rows in the tab order: they
/// are visible text, and a Tab stop that ignores Enter/Space is an empty shell.
#[test]
fn a_static_table_has_no_empty_tab_stops() {
    let mut state = Audit::default();
    state.rows = rows3();
    let mut rt = probe(state, (960.0, 640.0));
    press(&mut rt, Key::Tab);
    assert_eq!(focused(&rt), None, "a static table took Tab focus");
    press(&mut rt, Key::Enter);
    assert!(rt.state.activations.is_empty());
}

/// `on_activate` alone keeps rows as real Tab stops with a working Enter: the
/// row is not an empty shell, and no container focus group is needed.
#[test]
fn activate_only_rows_are_tab_stops_with_a_working_enter() {
    let mut state = Audit::default();
    state.rows = rows3();
    state.activate = true;
    let mut rt = probe(state, (960.0, 640.0));
    press(&mut rt, Key::Tab);
    assert_eq!(focused(&rt).as_deref(), Some("tbl-row-0"));
    press(&mut rt, Key::Tab);
    assert_eq!(focused(&rt).as_deref(), Some("tbl-row-1"));
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.activations, vec![1], "Enter reaches the row");
}

/// `on_select` alone makes the container the single tab stop and rows
/// programmatic-only, and Enter/Space reach the focused rendered row.
#[test]
fn select_makes_the_container_the_only_tab_stop_and_enter_activates() {
    let mut state = Audit::default();
    state.rows = rows3();
    state.select = true;
    state.activate = true;
    let mut rt = probe(state, (960.0, 640.0));
    press(&mut rt, Key::Tab);
    assert_eq!(focused(&rt).as_deref(), Some(KEY));
    press(&mut rt, Key::Down);
    assert_eq!(focused(&rt).as_deref(), Some("tbl-row-0"));
    assert_eq!(rt.state.selects, vec![0]);
    press(&mut rt, Key::End);
    assert_eq!(focused(&rt).as_deref(), Some("tbl-row-2"));
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.activations, vec![2]);
    press(&mut rt, Key::Text(" ".into()));
    assert_eq!(rt.state.activations, vec![2, 2]);
}

/// Pointer semantics from the kit: one press selects and focuses, two presses
/// also activate, and the callback sees the rendered index.
#[test]
fn pointer_selects_and_double_press_activates() {
    let mut state = Audit::default();
    state.rows = rows3();
    state.select = true;
    state.activate = true;
    let mut rt = probe(state, (960.0, 640.0));
    click(&mut rt, "tbl-row-2");
    assert_eq!(rt.state.selects, vec![2]);
    assert_eq!(rt.state.selected, Some(2));
    assert_eq!(focused(&rt).as_deref(), Some("tbl-row-2"));
    let (x, y) = rt.find("tbl-row-1").expect("row 1");
    for _ in 0..2 {
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
    }
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.state.activations, vec![1]);
}

/// Keys and data stay aligned after sorting: row/cell keys are positional in
/// rendered order, and `row_key` maps them back to the app's keys.
#[test]
fn row_keys_and_cells_follow_the_rendered_order_after_sort() {
    let columns = columns();
    let table = DataTable::new(KEY, &columns)
        .rows(vec![
            Row::from(vec!["Charlie", "1", "1"]).key("c"),
            Row::from(vec!["Alpha", "10", "100"]).key("a"),
            Row::from(vec!["Bravo", "2", "20"]).key("b"),
        ])
        .sort(0, SortDir::Ascending);
    assert_eq!(table.row_key(0), Some("a"));
    assert_eq!(table.row_key(1), Some("b"));
    assert_eq!(table.row_key(2), Some("c"));
    assert_eq!(table.row_key(3), None);

    let mut state = Audit::default();
    state.rows = vec![
        vec!["Charlie", "1", "1"],
        vec!["Alpha", "10", "100"],
        vec!["Bravo", "2", "20"],
    ];
    state.sort = Some((0, SortDir::Ascending));
    state.selected = Some(0);
    let rt = probe(state, (960.0, 640.0));
    assert_eq!(
        element(&rt, "tbl-cell-0-0-text").text.as_deref(),
        Some("Alpha")
    );
    assert_eq!(
        element(&rt, "tbl-cell-1-0-text").text.as_deref(),
        Some("Bravo")
    );
    assert_eq!(
        element(&rt, "tbl-cell-2-0-text").text.as_deref(),
        Some("Charlie")
    );
}

/// The selected fill is the kit's accent alpha, not a transparent no-op; the
/// unselected rows paint nothing.
#[test]
fn selection_paints_a_visible_kit_accent() {
    let mut state = Audit::default();
    state.rows = rows3();
    state.selected = Some(1);
    let rt = probe(state, (960.0, 640.0));
    let normal = element(&rt, "tbl-row-0").fill;
    let selected = element(&rt, "tbl-row-1").fill;
    assert_eq!(normal.a, 0.0, "an unselected row paints no fill");
    assert!(
        selected.a > 0.2,
        "the selected fill is visible, not transparent: {selected:?}"
    );
    let kit = crate::ui::kit::kit(true);
    let style = TableStyle::from_kit(kit);
    assert_eq!(
        selected, style.selected,
        "the fill is the kit style's selected token"
    );
    assert_eq!(style.row_height, 28.0);
    assert_eq!(style.header_height, 30.0);
    assert_eq!(style.cell_pad, 10.0);
}

/// A 60-row table that grows inside 960x640 keeps its own nodes and its
/// sibling inside the window; the overflow is the body's scroll, not layout
/// spill. Without `grow` the table hugs all rows (the documented no-
/// virtualisation limit), so this test only pins the bounded form.
#[test]
fn sixty_rows_scroll_inside_the_window_without_pushing_the_sibling_out() {
    let mut state = Audit::default();
    state.rows = (0..60).map(|_| vec!["row", "1", "2"]).collect();
    state.grow = true;
    state.tail = true;
    let rt = probe(state, (960.0, 640.0));

    let root = element(&rt, KEY).rect;
    assert!(
        root[1] >= 24.0 - 0.5 && root[1] + root[3] <= 640.0 - 24.0 + 0.5,
        "the growing table stays in the padded window: {root:?}"
    );
    let body = element(&rt, "tbl-body").rect;
    assert!(body[3] > 0.0 && body[3] < 60.0 * 28.0);
    assert!(element(&rt, "tbl-row-59").rect[1] > body[1] + body[3] - 0.5);
    let below = element(&rt, "below").rect;
    assert!(
        below[1] + below[3] <= 640.0 + 0.5,
        "the sibling stays inside the window: {below:?}"
    );
}

/// The documented limit: without a bounded parent the table is as tall as its
/// rows, so a 60-row table is taller than 640 and any following sibling moves
/// below the window. A caller that wants a viewport must use `grow`/`height`.
#[test]
fn without_grow_sixty_rows_take_their_intrinsic_height() {
    let mut state = Audit::default();
    state.rows = (0..60).map(|_| vec!["row", "1", "2"]).collect();
    state.tail = true;
    let rt = probe(state, (960.0, 640.0));
    let root = element(&rt, KEY).rect;
    assert_eq!(root[3], 30.0 + 60.0 * 28.0);
    assert!(root[3] > 640.0);
}

/// Every visible node of the table subtree stays inside the window at both
/// supported sizes, with the table growing inside a padded column.
#[test]
fn the_table_subtree_stays_inside_the_window_at_both_sizes() {
    for (width, height) in [(960.0, 640.0), (1440.0, 816.0)] {
        let mut state = Audit::default();
        state.rows = (0..10).map(|_| vec!["row", "1", "2"]).collect();
        state.grow = true;
        let rt = probe(state, (width, height));
        let path = element(&rt, KEY).path;
        for element in rt.elements() {
            if element.path.starts_with(&path)
                && element.visible[2] > 0.01
                && element.visible[3] > 0.01
            {
                let [x, y, w, h] = element.rect;
                assert!(
                    x >= -0.5 && y >= -0.5 && x + w <= width + 0.5 && y + h <= height + 0.5,
                    "{} at {width}x{height} overflows: {element:?}",
                    element.key.as_deref().unwrap_or("<unnamed>")
                );
            }
        }
    }
}

/// A zero-row table mounts exactly the empty status row and no data row.
#[test]
fn a_zero_row_table_mounts_only_the_empty_status() {
    let state = Audit::default();
    let rt = probe(state, (960.0, 640.0));
    assert!(element(&rt, "tbl-empty").visible[2] > 0.01);
    assert!(!rt.elements().iter().any(|element| element
        .key
        .as_deref()
        .is_some_and(|key| key.starts_with("tbl-row-"))));
}
