//! T0-09 table base: geometry, alignment, empty state, sorting, selection,
//! keyboard and pointer activation, and window-fit at two viewport sizes.

use super::*;

use crate::ui::table::{CellValue, Column, DataTable};
use rune::{col, Align, Cx, El, Env, Key, KeyEvent, Mods, Runtime};
use rune_kit::SortDir;

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
struct Probe {
    rows: Vec<Vec<&'static str>>,
    selected: Option<usize>,
    selects: Vec<usize>,
    activations: Vec<usize>,
    sort: Option<(usize, SortDir)>,
    grow: bool,
}

fn view(state: &Probe, _cx: &mut Cx<Probe>) -> El {
    let columns = columns();
    let rows = state
        .rows
        .iter()
        .map(|row| row.iter().map(|cell| CellValue::from(*cell)).collect::<Vec<_>>());
    let mut table = DataTable::new(KEY, &columns)
        .rows(rows)
        .selected(state.selected)
        .on_select(|s: &mut Probe, i| {
            s.selected = Some(i);
            s.selects.push(i);
        })
        .on_activate(|s: &mut Probe, i| s.activations.push(i));
    if let Some((column, dir)) = state.sort {
        table = table.sort(column, dir);
    }
    if state.grow {
        table = table.grow();
    }
    col()
        .w_full()
        .h_full()
        .padding(24.0)
        .child(table.build(true))
}

fn probe(state: Probe, size: (f32, f32)) -> Runtime<Probe> {
    let mut rt = Runtime::new(state, view, size, env());
    rt.frame(0.0);
    rt
}

fn press<S>(rt: &mut Runtime<S>, key: Key) {
    rt.key(KeyEvent {
        key,
        mods: Mods::default(),
    });
    rt.frame(1.0 / 60.0);
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn double_click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    for _ in 0..2 {
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
    }
    rt.frame(1.0 / 60.0);
}

fn focused<S>(rt: &Runtime<S>) -> Option<String> {
    rt.elements()
        .into_iter()
        .find(|e| e.focused)
        .and_then(|e| e.key)
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn rect<S>(rt: &Runtime<S>, key: &str) -> [f32; 4] {
    element(rt, key).rect
}

fn text_of<S>(rt: &Runtime<S>, key: &str) -> String {
    element(rt, key).text.unwrap_or_default()
}

#[track_caller]
fn assert_rect(actual: [f32; 4], expected: [f32; 4]) {
    for (axis, label) in ["x", "y", "w", "h"].into_iter().enumerate() {
        assert!(
            (actual[axis] - expected[axis]).abs() <= 0.75,
            "{label}: got {} expected {} (rect {actual:?} vs {expected:?})",
            actual[axis],
            expected[axis]
        );
    }
}

fn rows3() -> Vec<Vec<&'static str>> {
    vec![
        vec!["Alpha", "10", "100"],
        vec!["Bravo", "2", "20"],
        vec!["Charlie", "1", "1"],
    ]
}

fn keys_starting<S>(rt: &Runtime<S>, prefix: &str) -> Vec<rune::ElementInfo> {
    rt.elements()
        .into_iter()
        .filter(|e| e.key.as_deref().is_some_and(|k| k.starts_with(prefix)))
        .collect()
}

// ---- Geometry -----------------------------------------------------------------------------

#[test]
fn header_and_rows_take_the_declared_geometry() {
    let mut state = Probe::default();
    state.rows = rows3();
    state.grow = true;
    let rt = probe(state, (1440.0, 816.0));

    // Padding 24 on each side: the table body is 1392 wide and 768 tall.
    assert_rect(rect(&rt, "tbl"), [24.0, 24.0, 1392.0, 768.0]);
    assert_rect(rect(&rt, "tbl-head"), [24.0, 24.0, 1392.0, 30.0]);
    // Fixed 90 + 80 on the right, Name takes the rest.
    assert_rect(rect(&rt, "tbl-col-0"), [24.0, 24.0, 1222.0, 30.0]);
    assert_rect(rect(&rt, "tbl-col-1"), [1246.0, 24.0, 90.0, 30.0]);
    assert_rect(rect(&rt, "tbl-col-2"), [1336.0, 24.0, 80.0, 30.0]);
    // Rows stack under the 30 pt header at 28 pt each.
    assert_rect(rect(&rt, "tbl-row-0"), [24.0, 54.0, 1392.0, 28.0]);
    assert_rect(rect(&rt, "tbl-row-1"), [24.0, 82.0, 1392.0, 28.0]);
    assert_rect(rect(&rt, "tbl-row-2"), [24.0, 110.0, 1392.0, 28.0]);
    assert_rect(rect(&rt, "tbl-cell-1-1"), [1246.0, 82.0, 90.0, 28.0]);
    assert_rect(rect(&rt, "tbl-cell-2-2"), [1336.0, 110.0, 80.0, 28.0]);
}

// ---- Alignment ----------------------------------------------------------------------------

#[test]
fn end_aligned_text_hugs_the_cell_right_edge() {
    let mut state = Probe::default();
    state.rows = rows3();
    let rt = probe(state, (1440.0, 816.0));

    let cell = rect(&rt, "tbl-cell-1-1");
    let text = rect(&rt, "tbl-cell-1-1-text");
    let cell_right = cell[0] + cell[2];
    let text_right = text[0] + text[2];
    assert!(
        (cell_right - 10.0 - text_right).abs() <= 0.75,
        "End-aligned text right edge {text_right} should hug cell right {cell_right} minus \
         cell_pad 10 (cell {cell:?}, text {text:?})"
    );

    let start_cell = rect(&rt, "tbl-cell-0-0");
    let start_text = rect(&rt, "tbl-cell-0-0-text");
    assert!(
        (start_text[0] - (start_cell[0] + 10.0)).abs() <= 0.75,
        "Start-aligned text left edge {} should hug cell left {} plus cell_pad 10",
        start_text[0],
        start_cell[0]
    );
}

// ---- Empty state --------------------------------------------------------------------------

#[test]
fn empty_rows_render_one_empty_state_and_no_rows() {
    let rt = probe(Probe::default(), (1440.0, 816.0));

    assert!(keys_starting(&rt, "tbl-row-").is_empty());
    let empties = keys_starting(&rt, "tbl-empty");
    assert_eq!(empties.len(), 1, "exactly one empty-state element");
    assert_eq!(empties[0].label.as_deref(), Some("暂无可显示的行"));
    // The header stays visible so the columns remain legible.
    assert_rect(rect(&rt, "tbl-head"), [24.0, 24.0, 1392.0, 30.0]);
    assert!(rt.presented("tbl-empty").is_some());
}

// ---- Sorting ------------------------------------------------------------------------------

#[test]
fn sort_reorders_rows_and_is_numeric_aware() {
    let mut state = Probe::default();
    state.rows = vec![
        vec!["Charlie", "1", "1"],
        vec!["Alpha", "10", "100"],
        vec!["Bravo", "2", "20"],
    ];
    state.sort = Some((0, SortDir::Ascending));
    let rt = probe(state, (1440.0, 816.0));
    assert_eq!(text_of(&rt, "tbl-cell-0-0-text"), "Alpha");
    assert_eq!(text_of(&rt, "tbl-cell-1-0-text"), "Bravo");
    assert_eq!(text_of(&rt, "tbl-cell-2-0-text"), "Charlie");

    // Numeric ordering: 1, 2, 10 (lexical order would be 1, 10, 2).
    let mut state = Probe::default();
    state.rows = vec![
        vec!["Alpha", "10", "100"],
        vec!["Bravo", "2", "20"],
        vec!["Charlie", "1", "1"],
    ];
    state.sort = Some((1, SortDir::Descending));
    let rt = probe(state, (1440.0, 816.0));
    assert_eq!(text_of(&rt, "tbl-cell-0-1-text"), "10");
    assert_eq!(text_of(&rt, "tbl-cell-1-1-text"), "2");
    assert_eq!(text_of(&rt, "tbl-cell-2-1-text"), "1");
}

// ---- Selection ----------------------------------------------------------------------------

#[test]
fn selected_row_is_highlighted_differently() {
    let mut state = Probe::default();
    state.rows = rows3();
    state.selected = Some(1);
    let rt = probe(state, (1440.0, 816.0));

    let normal = element(&rt, "tbl-row-0").fill;
    let selected = element(&rt, "tbl-row-1").fill;
    let normal_again = element(&rt, "tbl-row-2").fill;
    assert_ne!(selected, normal, "selected row must not use the plain row fill");
    assert_eq!(normal, normal_again, "unselected rows share one fill");
}

// ---- Keyboard -----------------------------------------------------------------------------

#[test]
fn keyboard_moves_focus_and_activates_the_current_row() {
    let mut state = Probe::default();
    state.rows = rows3();
    state.grow = true;
    let mut rt = probe(state, (1440.0, 816.0));

    press(&mut rt, Key::Tab);
    assert_eq!(focused(&rt).as_deref(), Some("tbl"), "Tab reaches the table");
    assert!(rt.state.selected.is_none());

    press(&mut rt, Key::Down);
    assert_eq!(focused(&rt).as_deref(), Some("tbl-row-0"));
    assert_eq!(rt.state.selects, vec![0]);

    press(&mut rt, Key::Down);
    assert_eq!(focused(&rt).as_deref(), Some("tbl-row-1"));
    assert_eq!(rt.state.selected, Some(1));

    press(&mut rt, Key::Up);
    assert_eq!(focused(&rt).as_deref(), Some("tbl-row-0"));
    assert_eq!(rt.state.selects, vec![0, 1, 0]);

    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.activations, vec![0], "Enter activates the current row");
    press(&mut rt, Key::Text(" ".into()));
    assert_eq!(rt.state.activations, vec![0, 0], "Space activates too");

    press(&mut rt, Key::End);
    assert_eq!(focused(&rt).as_deref(), Some("tbl-row-2"));
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.activations, vec![0, 0, 2]);
    press(&mut rt, Key::Home);
    assert_eq!(focused(&rt).as_deref(), Some("tbl-row-0"));
}

#[test]
fn fixed_height_table_does_not_grow() {
    let mut state = Probe::default();
    state.rows = rows3();
    let rt = probe(state, (1440.0, 816.0));
    // Without `.grow()` the table hugs its 30 pt header and 3 rows.
    assert_rect(rect(&rt, "tbl"), [24.0, 24.0, 1392.0, 30.0 + 3.0 * 28.0]);
}

// ---- Pointer ------------------------------------------------------------------------------

#[test]
fn clicking_a_row_selects_and_focuses_it() {
    let mut state = Probe::default();
    state.rows = rows3();
    let mut rt = probe(state, (1440.0, 816.0));

    click(&mut rt, "tbl-row-2");
    assert_eq!(rt.state.selects, vec![2]);
    assert_eq!(rt.state.selected, Some(2));
    assert_eq!(focused(&rt).as_deref(), Some("tbl-row-2"));
}

#[test]
fn double_pressing_a_row_activates_it() {
    let mut state = Probe::default();
    state.rows = rows3();
    let mut rt = probe(state, (1440.0, 816.0));

    double_click(&mut rt, "tbl-row-1");
    assert_eq!(rt.state.activations, vec![1]);
}

// ---- Window fit ---------------------------------------------------------------------------

fn assert_no_overflow(size: (f32, f32)) {
    let mut state = Probe::default();
    state.rows = (0..10).map(|_| vec!["row", "1", "2"]).collect();
    state.grow = true;
    let rt = probe(state, size);

    let elements = keys_starting(&rt, "tbl");
    assert!(!elements.is_empty());
    let mut outside = Vec::new();
    for element in elements {
        let [x, y, w, h] = element.visible;
        if x < -0.5 || y < -0.5 || x + w > size.0 + 0.5 || y + h > size.1 + 0.5 {
            outside.push((element.key, element.visible));
        }
    }
    assert!(
        outside.is_empty(),
        "{} table nodes leave a {}x{} window: {outside:?}",
        outside.len(),
        size.0,
        size.1
    );
}

#[test]
fn table_stays_inside_a_960x640_window() {
    assert_no_overflow((960.0, 640.0));
}

#[test]
fn table_stays_inside_a_1440x816_window() {
    assert_no_overflow((1440.0, 816.0));
}
