//! Independent audit for T269 (the SchemaDiffTableListPanel Rune port).
//!
//! Tester-written: the coder's module is a black box; every assertion comes
//! from re-reading
//! `packages/frontend/src/windows/schema-diff/SchemaDiffTableListPanel.tsx`
//! and its React test (`__tests__/SchemaDiffPanels.test.tsx`): the row keys,
//! the click-to-select closure, the selected-row highlight, the three
//! `hasDiff` states (badge / hint / nothing), the empty-state hint and the
//! header label. Evidence comes from `Runtime::elements()` (real layout),
//! presses go through real pointer events.

use std::collections::HashMap;

use rune::{Cx, El, Env, Runtime};

use datazen_ui::i18n::t;
use datazen_ui::ui::kit::kit;
use datazen_ui::views::schema_diff::schema_diff_table_list_panel::SchemaDiffTableListPanel;

#[derive(Clone)]
struct Scene {
    tables: Vec<String>,
    selected: Option<String>,
    has_diff: Option<HashMap<String, bool>>,
    width: Option<f32>,
    selected_events: Vec<String>,
    dark: bool,
}

impl Scene {
    fn base() -> Self {
        Self {
            tables: vec!["users".to_owned(), "orders".to_owned()],
            selected: Some("users".to_owned()),
            has_diff: Some(HashMap::from([
                ("users".to_owned(), true),
                ("orders".to_owned(), false),
            ])),
            width: Some(220.0),
            selected_events: Vec::new(),
            dark: true,
        }
    }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    // The real parent (the compare step) places the rail inside a flex row
    // with the resizer and the detail panel; reproduce that host so the
    // pinned width is observable (a view root stretches to the window).
    let mut panel = SchemaDiffTableListPanel::<Scene>::new(&s.tables, s.selected.as_deref())
        .table_has_diff(s.has_diff.as_ref())
        .on_select(|s: &mut Scene, table| s.selected_events.push(table));
    if let Some(width) = s.width {
        panel = panel.width(width);
    }
    rune::row()
        .key("audit-host")
        .w_full()
        .h_full()
        .child(panel.build(s.dark))
}

fn rt(scene: Scene) -> Runtime<Scene> {
    let mut rt = Runtime::new(
        scene,
        view,
        (960.0, 640.0),
        Env { system_dark: true, scale: 1.0 },
    );
    rt.frame(0.0);
    rt
}

fn maybe(rt: &Runtime<Scene>, key: &str) -> Option<rune::ElementInfo> {
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.key.as_deref() == Some(key))
}

fn text_of(rt: &Runtime<Scene>, key: &str) -> String {
    maybe(rt, key).and_then(|e| e.text).unwrap_or_default()
}

fn press(rt: &mut Runtime<Scene>, key: &str) {
    let point = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    // Hit-testing follows the pointer: move before the press.
    rt.pointer_move(point.0, point.1);
    rt.frame(0.0);
    rt.pointer_down(point.0, point.1);
    rt.pointer_up(point.0, point.1);
    rt.frame(0.0);
    rt.frame(0.0);
}

/// The React test's core: rows render keyed, and clicking the orders row
/// calls `onSelect('orders')`.
#[test]
fn rows_render_and_press_selects() {
    let mut runtime = rt(Scene::base());
    assert!(maybe(&runtime, "schema-diff-table-list").is_some(), "root key");
    assert!(maybe(&runtime, "schema-diff-table-row-users").is_some());
    assert!(maybe(&runtime, "schema-diff-table-row-orders").is_some());
    assert!(runtime.state.selected_events.is_empty());

    press(&mut runtime, "schema-diff-table-row-orders");
    assert_eq!(runtime.state.selected_events, ["orders".to_owned()]);
}

/// The header shows the Tables label (uppercased, like the CSS class).
#[test]
fn header_shows_the_tables_label() {
    let runtime = rt(Scene::base());
    let label = t("schemaDiff.tables").to_uppercase();
    assert_eq!(text_of(&runtime, "schema-diff-table-list-header-label"), label);
}

/// The selected row highlights (`bg-surface-alt` → layer_01); the other
/// stays transparent.
#[test]
fn selected_row_highlights() {
    let runtime = rt(Scene::base());
    let k = kit(runtime.state.dark);
    let selected = maybe(&runtime, "schema-diff-table-row-users").expect("users row");
    let other = maybe(&runtime, "schema-diff-table-row-orders").expect("orders row");
    assert_eq!(selected.fill, k.c.layer_01, "active row bg-surface-alt");
    assert_eq!(other.fill.a, 0.0, "inactive row transparent");
}

/// Count an element's direct children via the runtime's parent links.
fn child_count(rt: &Runtime<Scene>, key: &str) -> usize {
    let elements = rt.elements();
    let index = elements
        .iter()
        .position(|e| !e.exiting && e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"));
    elements
        .iter()
        .filter(|e| e.parent == Some(index) && !e.exiting)
        .count()
}

/// Find a laid-out text element by its exact text (text elements carry
/// `text`; containers do not).
fn find_text(rt: &Runtime<Scene>, needle: &str) -> Option<rune::ElementInfo> {
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.text.as_deref() == Some(needle))
}

/// The three `hasDiff` states: `true` → the warning badge, `false` → the
/// identical hint, absent → neither.
#[test]
fn has_diff_states_render_badge_hint_or_nothing() {
    let runtime = rt(Scene::base());
    // users: changed badge (uppercased label).
    let label = t("schemaDiff.colChanged").to_uppercase();
    assert!(find_text(&runtime, &label).is_some(), "badge label present");
    assert_eq!(child_count(&runtime, "schema-diff-table-row-users"), 2, "name + badge");

    // orders: the identical hint.
    assert!(
        find_text(&runtime, t("schemaDiff.identical")).is_some(),
        "identical hint present"
    );
    assert_eq!(child_count(&runtime, "schema-diff-table-row-orders"), 2, "name + hint");

    // Absent entry: neither badge nor hint.
    let mut scene = Scene::base();
    scene.tables = vec!["ghost".to_owned()];
    scene.selected = Some("ghost".to_owned());
    scene.has_diff = Some(HashMap::new());
    let runtime = rt(scene);
    assert_eq!(child_count(&runtime, "schema-diff-table-row-ghost"), 1, "name only");

    // Absent map: same as absent entries.
    let mut scene = Scene::base();
    scene.has_diff = None;
    let runtime = rt(scene);
    assert_eq!(child_count(&runtime, "schema-diff-table-row-users"), 1, "no map, no badges");
}

/// The badge paints the warning wash (`bg-warning/15`) and warning ink.
#[test]
fn changed_badge_paints_the_warning_wash() {
    let runtime = rt(Scene::base());
    let k = kit(runtime.state.dark);
    let label = t("schemaDiff.colChanged").to_uppercase();
    let badge = runtime
        .elements()
        .into_iter()
        .find(|e| !e.exiting && e.text.as_deref() == Some(label.as_str()))
        .expect("badge element");
    let wash = k.alpha(k.c.warning.fg, 0.15);
    assert_eq!(badge.fill, wash, "bg-warning/15");
    assert_eq!(badge.text_color, Some(k.c.warning.fg), "text-warning");
}

/// The empty state shows the tableRequired hint, no rows.
#[test]
fn empty_state_shows_the_hint() {
    let mut scene = Scene::base();
    scene.tables = Vec::new();
    scene.selected = None;
    let runtime = rt(scene);
    assert_eq!(
        text_of(&runtime, "schema-diff-table-list-empty-hint"),
        t("schemaDiff.tableRequired")
    );
    assert!(maybe(&runtime, "schema-diff-table-row-users").is_none());
}

/// The pinned width lands on the root (React's `style={{ width }}`).
#[test]
fn pinned_width_lands_on_the_root() {
    let runtime = rt(Scene::base());
    let root = maybe(&runtime, "schema-diff-table-list").expect("root");
    // rect is [x, y, w, h].
    assert!(
        (root.rect[2] - 220.0).abs() < 0.5,
        "width pinned, got {}",
        root.rect[2]
    );
}

/// Pressing the already-selected row still reports the selection (React's
/// onClick fires on every press).
#[test]
fn pressing_the_selected_row_still_reports() {
    let mut runtime = rt(Scene::base());
    press(&mut runtime, "schema-diff-table-row-users");
    assert_eq!(runtime.state.selected_events, ["users".to_owned()]);
}
