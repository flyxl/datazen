//! T540: `TableFilterToolbar` — exact E2E keys, filter-active/seeding rules,
//! enabled/disabled states, toggle/clear/quick-filter actions, the error strip
//! and rect geometry at both sizes in both palettes.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Key, KeyEvent, Mods, Runtime, TextEdit};

use datazen_ui::i18n::t;
use datazen_ui::views::table_filter_toolbar::{
    build, TableFilterAction, TableFilterToolbarVm, ERROR_KEY, NAMED_VIEW_KEY, NL_INPUT_KEY,
    QUICK_KEY, TOGGLE_KEY,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

#[derive(Clone)]
struct Scene {
    vm: TableFilterToolbarVm,
    quick: TextEdit,
    actions: Rc<RefCell<Vec<TableFilterAction>>>,
    dark: bool,
}

fn ready() -> TableFilterToolbarVm {
    TableFilterToolbarVm {
        first_column: "id".to_owned(),
        ..TableFilterToolbarVm::default()
    }
}

fn scene(vm: TableFilterToolbarVm, dark: bool) -> Scene {
    Scene { vm, quick: TextEdit::new(String::new()), actions: Rc::new(RefCell::new(Vec::new())), dark }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let actions = s.actions.clone();
    let callback = Rc::new(move |state: &mut Scene, action| {
        match &action {
            TableFilterAction::ToggleFilterPanel { open } => state.vm.filter_panel_open = *open,
            TableFilterAction::AddDefaultFilter { .. } => state.vm.draft_filter_count = 1,
            TableFilterAction::ClearFilters => {
                state.vm.applied_filter_count = 0;
                state.vm.draft_filter_count = 0;
            }
            TableFilterAction::ApplyQuickFilter { .. } => {}
            TableFilterAction::ClearQuickFilterError => state.vm.quick_filter_error.clear(),
        }
        actions.borrow_mut().push(action);
    });
    build(
        &s.vm,
        &s.quick,
        Some(Rc::new(|state: &mut Scene| &mut state.quick)),
        None,
        None,
        Some(callback),
        s.dark,
    )
}

fn runtime(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn has_key(rt: &Runtime<Scene>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn click(rt: &mut Runtime<Scene>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out before click"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn assert_inside(rt: &Runtime<Scene>, size: (f32, f32), label: &str) {
    let outside: Vec<String> = rt
        .elements()
        .iter()
        .filter(|e| {
            let r = e.rect;
            r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > size.0 + 0.5 || r[1] + r[3] > size.1 + 0.5
        })
        .map(|e| format!("{} {:?}", e.key.as_deref().unwrap_or(&e.path), e.rect))
        .collect();
    assert!(outside.is_empty(), "{label}: outside {outside:?}");
}

// ── pure rules ────────────────────────────────────────────────────────────────

#[test]
fn filter_active_and_seed_rules() {
    let mut vm = ready();
    assert!(!vm.filter_active());
    assert_eq!(vm.quick_disabled(), false);

    vm.applied_filter_count = 1;
    assert!(vm.filter_active(), "applied filters light the button up");
    vm.applied_filter_count = 0;
    vm.draft_filter_count = 2;
    assert!(vm.filter_active(), "draft filters light the button up");
    vm.draft_filter_count = 0;
    vm.filter_panel_open = true;
    assert!(vm.filter_active(), "open panel lights the button up");

    // Seeding: only when opening with no drafts.
    let mut opening = ready();
    assert!(opening.should_seed_default_filter());
    opening.filter_panel_open = true;
    assert!(!opening.should_seed_default_filter(), "closing never seeds");
    let mut with_draft = ready();
    with_draft.draft_filter_count = 1;
    assert!(!with_draft.should_seed_default_filter(), "existing draft is not overwritten");

    let mut busy = ready();
    busy.loading = true;
    assert!(busy.quick_disabled());
    busy.loading = false;
    busy.pending_busy = true;
    assert!(busy.quick_disabled());
}

// ── keys, actions and states ──────────────────────────────────────────────────

#[test]
fn e2e_keys_and_slots() {
    let mut vm = ready();
    vm.has_named_view = true;
    let rt = runtime(scene(vm, true), DESIGN);
    for key in ["table-filter-toolbar", "table-filter-bar", TOGGLE_KEY, QUICK_KEY, NAMED_VIEW_KEY, NL_INPUT_KEY] {
        assert!(has_key(&rt, key), "{key} is mounted");
    }
    assert!(!has_key(&rt, ERROR_KEY), "no error strip initially");

    let mut absent = ready();
    absent.has_named_view = false;
    let rt = runtime(scene(absent, true), DESIGN);
    assert!(!has_key(&rt, NAMED_VIEW_KEY), "absent backend hides the named-view entry");
}

#[test]
fn toggle_opens_and_seeds_default_filter() {
    let mut rt = runtime(scene(ready(), true), DESIGN);
    click(&mut rt, TOGGLE_KEY);
    assert_eq!(
        *rt.state.actions.borrow(),
        [
            TableFilterAction::ToggleFilterPanel { open: true },
            TableFilterAction::AddDefaultFilter { column: "id".to_owned() },
        ]
    );
}

#[test]
fn toggling_closed_does_not_seed() {
    let mut vm = ready();
    vm.filter_panel_open = true;
    let mut rt = runtime(scene(vm, true), DESIGN);
    click(&mut rt, TOGGLE_KEY);
    assert_eq!(*rt.state.actions.borrow(), [TableFilterAction::ToggleFilterPanel { open: false }]);
}

#[test]
fn disabled_toggle_is_inert() {
    let mut vm = ready();
    vm.loading = true;
    let mut rt = runtime(scene(vm, true), DESIGN);
    click(&mut rt, TOGGLE_KEY);
    assert!(rt.state.actions.borrow().is_empty(), "a loading toggle emits no action");
}

#[test]
fn quick_filter_enter_routes_through_the_host() {
    let mut rt = runtime(scene(ready(), true), DESIGN);
    let (x, y) = rt.find(QUICK_KEY).expect("quick field is laid out");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.key(KeyEvent { key: Key::Text("status = 'open'".to_owned()), mods: Mods::default() });
    rt.frame(0.0);
    rt.key(KeyEvent { key: Key::Enter, mods: Mods::default() });
    rt.frame(0.0);
    assert_eq!(
        *rt.state.actions.borrow(),
        [TableFilterAction::ApplyQuickFilter { text: "status = 'open'".to_owned() }]
    );
}

#[test]
fn empty_quick_filter_enter_clears_filters() {
    let mut vm = ready();
    vm.applied_filter_count = 3;
    vm.draft_filter_count = 2;
    let mut rt = runtime(scene(vm, true), DESIGN);
    let (x, y) = rt.find(QUICK_KEY).expect("quick field is laid out");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.key(KeyEvent { key: Key::Enter, mods: Mods::default() });
    rt.frame(0.0);
    assert_eq!(*rt.state.actions.borrow(), [TableFilterAction::ClearFilters]);
}

#[test]
fn quick_filter_error_strip_is_pinned() {
    let mut vm = ready();
    vm.quick_filter_error = t("filter.nestedLogic").to_owned();
    let rt = runtime(scene(vm, true), DESIGN);
    assert!(has_key(&rt, ERROR_KEY));
}

#[test]
fn geometry_fits_both_sizes_in_both_themes() {
    for size in [MIN, DESIGN] {
        for dark in [true, false] {
            let mut vm = ready();
            vm.has_named_view = true;
            vm.quick_filter_error = t("filter.invalidExpression").to_owned();
            let rt = runtime(scene(vm, dark), size);
            assert_inside(&rt, size, &format!("{size:?} dark={dark}"));
        }
    }
}

// ── T540 independent audit (appended; reuses this module's Scene) ────────

/// React's quick-filter `onChange` clears `quickFilterError` on every edit
/// (`TableFilterToolbar.tsx:203-206`). The port binds the field text through an
/// accessor but emits no action on edit, so a host-owned error strip persists
/// after the user starts correcting the expression.
#[test]
fn audit_editing_quick_filter_clears_the_error_strip() {
    let mut vm = ready();
    vm.quick_filter_error = t("filter.nestedLogic").to_owned();
    let mut rt = runtime(scene(vm, true), DESIGN);
    assert!(has_key(&rt, ERROR_KEY), "error strip present initially");
    let (x, y) = rt.find(QUICK_KEY).unwrap_or_else(|| panic!("quick field laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.key(KeyEvent { key: Key::Text("i".to_owned()), mods: Mods::default() });
    rt.frame(0.0);
    assert!(
        !has_key(&rt, ERROR_KEY),
        "typing must clear the quick-filter error strip like React's onChange"
    );
}

/// React's quick filter applies the expression on blur when it has text
/// (`TableFilterToolbar.tsx:213-215`), not just on Enter. The port only wires
/// `on_submit` (Enter), so leaving the field does not apply the filter.
#[test]
fn audit_blur_with_text_applies_the_quick_filter() {
    let mut rt = runtime(scene(ready(), true), DESIGN);
    let (x, y) = rt.find(QUICK_KEY).unwrap_or_else(|| panic!("quick field laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.key(KeyEvent { key: Key::Text("status = 'open'".to_owned()), mods: Mods::default() });
    rt.frame(0.0);
    // Move focus away with Tab (a blur in the browser).
    rt.key(KeyEvent { key: Key::Tab, mods: Mods::default() });
    rt.frame(0.0);
    assert!(
        rt.state.actions.borrow().iter().any(|a| matches!(a, TableFilterAction::ApplyQuickFilter { .. })),
        "blurring a non-empty quick filter must apply it like React's onBlur; actions {:?}",
        rt.state.actions.borrow()
    );
}
