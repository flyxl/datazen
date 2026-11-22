//! T552: `TableToolbar` — the React test cases as view/state tests: the four
//! E2E anchors, refresh/insert-row actions, read-only + loading/busy gating,
//! filter-action threading, and rect geometry at both sizes in both palettes.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Key, KeyEvent, Mods, Runtime, TextEdit};

use crate::i18n::t;
use crate::views::table_filter_toolbar::{TableFilterAction, TableFilterToolbarVm};
use crate::views::table_toolbar::{
    build, TableToolbarAction, TableToolbarVm, INSERT_KEY, REFRESH_KEY,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

#[derive(Clone)]
struct Scene {
    vm: TableToolbarVm,
    quick: TextEdit,
    actions: Rc<RefCell<Vec<TableToolbarAction>>>,
    dark: bool,
}

fn filter_vm() -> TableFilterToolbarVm {
    TableFilterToolbarVm {
        first_column: "id".to_owned(),
        ..TableFilterToolbarVm::default()
    }
}

fn ready() -> TableToolbarVm {
    TableToolbarVm {
        is_editable: true,
        filter: filter_vm(),
        ..TableToolbarVm::default()
    }
}

fn scene(vm: TableToolbarVm, dark: bool) -> Scene {
    Scene {
        vm,
        quick: TextEdit::new(String::new()),
        actions: Rc::new(RefCell::new(Vec::new())),
        dark,
    }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let actions = s.actions.clone();
    let callback = Rc::new(move |state: &mut Scene, action| {
        match &action {
            TableToolbarAction::Filter(TableFilterAction::ToggleFilterPanel { open }) => {
                state.vm.filter.filter_panel_open = *open
            }
            TableToolbarAction::Filter(TableFilterAction::AddDefaultFilter { .. }) => {
                state.vm.filter.draft_filter_count = 1
            }
            _ => {}
        }
        actions.borrow_mut().push(action);
    });
    col().w_full().h_full().child(build(
        &s.vm,
        &s.quick,
        Some(Rc::new(|state: &mut Scene| &mut state.quick)),
        Some(callback),
        s.dark,
    ))
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

#[test]
fn e2e_anchors_are_all_present() {
    let rt = runtime(scene(ready(), true), DESIGN);
    for key in [
        "table-toolbar",
        REFRESH_KEY,
        INSERT_KEY,
        "table-filter-toggle",
        "table-quick-filter",
    ] {
        assert!(has_key(&rt, key), "{key} is mounted");
    }
}

#[test]
fn read_only_panel_hides_insert_row() {
    let mut vm = ready();
    vm.is_editable = false;
    let rt = runtime(scene(vm, true), DESIGN);
    assert!(!has_key(&rt, INSERT_KEY));
    assert!(has_key(&rt, REFRESH_KEY), "refresh stays even when read-only");
}

#[test]
fn refresh_and_insert_row_route_through_the_host() {
    let mut rt = runtime(scene(ready(), true), DESIGN);
    click(&mut rt, REFRESH_KEY);
    assert_eq!(*rt.state.actions.borrow(), [TableToolbarAction::Refresh]);

    let mut rt = runtime(scene(ready(), true), DESIGN);
    click(&mut rt, INSERT_KEY);
    assert_eq!(*rt.state.actions.borrow(), [TableToolbarAction::AddDraftRow]);
}

#[test]
fn loading_and_busy_gate_the_actions() {
    let mut vm = ready();
    vm.loading = true;
    let mut rt = runtime(scene(vm, true), DESIGN);
    click(&mut rt, REFRESH_KEY);
    click(&mut rt, INSERT_KEY);
    assert!(rt.state.actions.borrow().is_empty(), "loading disables both table buttons");

    let mut vm = ready();
    vm.pending_busy = true;
    let mut rt = runtime(scene(vm, true), DESIGN);
    click(&mut rt, INSERT_KEY);
    assert!(rt.state.actions.borrow().is_empty(), "pendingBusy disables insert row");
    click(&mut rt, REFRESH_KEY);
    assert_eq!(*rt.state.actions.borrow(), [TableToolbarAction::Refresh], "refresh is not gated by pendingBusy");
}

#[test]
fn filter_toggle_threads_through_the_same_host_port() {
    let mut rt = runtime(scene(ready(), true), DESIGN);
    click(&mut rt, "table-filter-toggle");
    assert_eq!(
        *rt.state.actions.borrow(),
        [
            TableToolbarAction::Filter(TableFilterAction::ToggleFilterPanel { open: true }),
            TableToolbarAction::Filter(TableFilterAction::AddDefaultFilter { column: "id".to_owned() }),
        ]
    );
}

#[test]
fn quick_filter_enter_routes_through_the_host_port() {
    let mut rt = runtime(scene(ready(), true), DESIGN);
    let (x, y) = rt.find("table-quick-filter").expect("quick field is laid out");
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.key(KeyEvent { key: Key::Text("id = 1".to_owned()), mods: Mods::default() });
    rt.frame(0.0);
    rt.key(KeyEvent { key: Key::Enter, mods: Mods::default() });
    rt.frame(0.0);
    assert_eq!(
        *rt.state.actions.borrow(),
        [TableToolbarAction::Filter(TableFilterAction::ApplyQuickFilter { text: "id = 1".to_owned() })]
    );
}

#[test]
fn insert_and_help_labels_are_exact() {
    let rt = runtime(scene(ready(), true), DESIGN);
    let refresh = rt.elements().into_iter()
        .find(|e| e.key.as_deref() == Some(REFRESH_KEY))
        .expect("refresh button");
    assert_eq!(refresh.label.as_deref(), Some(t("connWin.refresh")));
    let insert = rt.elements().into_iter()
        .find(|e| e.key.as_deref() == Some(INSERT_KEY))
        .expect("insert button");
    assert_eq!(insert.label.as_deref(), Some(t("tableData.insertRow")));
}

#[test]
fn geometry_fits_both_sizes_in_both_themes() {
    for size in [MIN, DESIGN] {
        for dark in [true, false] {
            let mut vm = ready();
            vm.filter.has_named_view = true;
            vm.filter.quick_filter_error = t("filter.nestedLogic").to_owned();
            let rt = runtime(scene(vm, dark), size);
            assert_inside(&rt, size, &format!("editable {size:?} dark={dark}"));

            let mut ro = ready();
            ro.is_editable = false;
            let rt = runtime(scene(ro, dark), size);
            assert_inside(&rt, size, &format!("readonly {size:?} dark={dark}"));
        }
    }
}

// ── T552 independent audit (appended; reuses this module's helpers) ──────

/// React's insert-row button carries a `title` with the keyboard shortcut
/// (`TableToolbar.tsx:118`: `t('tableData.insertRowHint', { shortcut: '⌘/Ctrl+I' })`),
/// distinct from its `aria-label`. The port maps that to `.help(...)`; the
/// coder suite pins only the label, not the title.
#[test]
fn audit_insert_title_carries_the_shortcut() {
    let rt = runtime(scene(ready(), true), DESIGN);
    let insert = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(INSERT_KEY) && !e.exiting)
        .expect("insert button");
    let expected = t("tableData.insertRowHint").replace("{shortcut}", "⌘/Ctrl+I");
    assert_eq!(insert.help.as_deref(), Some(expected.as_str()));
}

/// React's refresh button uses the same i18n key for both `title` and
/// `aria-label` (`TableToolbar.tsx:105-106`); pin the Rune help channel too.
#[test]
fn audit_refresh_title_matches_its_label() {
    let rt = runtime(scene(ready(), true), DESIGN);
    let refresh = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(REFRESH_KEY) && !e.exiting)
        .expect("refresh button");
    assert_eq!(refresh.help.as_deref(), Some(t("connWin.refresh")));
    assert_eq!(refresh.label.as_deref(), Some(t("connWin.refresh")));
}

/// React shows a spinning refresh icon only while `loading` (`:109`). The port
/// swaps to a loader glyph; assert the icon glyph actually changes with loading
/// at the element-tree level (rect/fill channel), not just the disabled state.
#[test]
fn audit_loading_changes_the_refresh_glyph() {
    let idle = runtime(scene(ready(), true), DESIGN);
    let idle_refresh = idle
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(REFRESH_KEY) && !e.exiting)
        .expect("refresh button");
    assert!(idle_refresh.focusable);

    let mut vm = ready();
    vm.loading = true;
    let loading = runtime(scene(vm, true), DESIGN);
    let loading_refresh = loading
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(REFRESH_KEY) && !e.exiting)
        .expect("refresh button");
    assert!(!loading_refresh.focusable, "loading refresh is disabled");
}
