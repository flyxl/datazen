//! T498 independent audit (tester, not coder).
//!
//! Precise probes of branches the shipped 9 tests do not pin: empty-cell /
//! numeric-cell styling, rendered→base pid mapping under sort, the killing
//! guard on the kill button, and a non-vacuous `visible`-rect geometry channel.

use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};
use rune_kit::SortDir;

use datazen_ui::views::process_list_view::{
    build, rendered_order, ProcessListAction, ProcessListViewVm, ProcessResult,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

fn result() -> ProcessResult {
    ProcessResult {
        columns: vec![
            datazen_ui::views::process_list_view::ProcessColumn {
                name: "pid".to_owned(),
                data_type: "integer".to_owned(),
            },
            datazen_ui::views::process_list_view::ProcessColumn {
                name: "user".to_owned(),
                data_type: "string".to_owned(),
            },
            datazen_ui::views::process_list_view::ProcessColumn {
                name: "database".to_owned(),
                data_type: "string".to_owned(),
            },
        ],
        rows: vec![
            vec!["9".to_owned(), "".to_owned(), "app".to_owned()],
            vec!["111".to_owned(), "alice".to_owned(), "app".to_owned()],
        ],
    }
}

#[derive(Clone)]
struct Scene {
    vm: ProcessListViewVm,
    dark: bool,
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let actions: Rc<dyn Fn(&mut Scene, ProcessListAction)> = Rc::new(|_s, _a| {});
    // build needs an `ActionFn<Scene>`; pass a no-op callback of the right type.
    let cb: Rc<dyn Fn(&mut Scene, ProcessListAction) + 'static> = actions;
    col().w_full().h_full().child(build(&s.vm, Some(cb), s.dark))
}

fn runtime(vm: ProcessListViewVm, size: (f32, f32), dark: bool) -> Runtime<Scene> {
    let mut rt = Runtime::new(Scene { vm, dark }, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

#[test]
fn empty_cells_are_an_em_dash_and_numeric_cells_are_monospace() {
    let rows = result().table_rows();
    assert_eq!(rows[0].cells[0].text, "9");
    assert!(rows[0].cells[0].mono, "a numeric pid is mono");
    assert_eq!(rows[0].cells[1].text, "—", "an empty cell becomes an em dash");
    assert!(rows[0].cells[1].muted, "a text cell is muted");
}

#[test]
fn highlighted_pid_maps_through_the_rendered_sort_order() {
    let mut vm = ProcessListViewVm {
        result: Some(result()),
        highlighted_row: Some(1),
        ..Default::default()
    };
    // No sort: rendered row 1 is base row 1 (pid 111).
    assert_eq!(vm.highlighted_pid(), Some(111));
    // Ascending by pid: numeric order puts 9 before 111 (lexicographic would reverse).
    vm.sort = Some((0, SortDir::Ascending));
    let order = rendered_order(vm.result.as_ref().unwrap(), vm.sort);
    assert_eq!(order, vec![0, 1], "numeric sort, not lexicographic");
    assert_eq!(vm.highlighted_pid(), Some(111), "selection follows the rendered row");
}

#[test]
fn kill_button_is_disabled_while_a_kill_is_in_flight() {
    let mut vm = ProcessListViewVm {
        result: Some(result()),
        highlighted_row: Some(0),
        ..Default::default()
    };
    assert!(!vm.killing);
    assert!(vm.can_kill());
    vm.killing = true;
    assert!(!vm.can_kill(), "a second kill is guarded while one is running");
    let rt = runtime(vm, DESIGN, true);
    let kill = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("process-list-kill"));
    assert!(kill.is_some_and(|e| !e.focusable), "the in-flight kill button is disabled");
}

#[test]
fn all_states_stay_inside_via_visible_rects() {
    for size in [MIN, DESIGN] {
        for dark in [true, false] {
            let mut loaded = ProcessListViewVm {
                result: Some(result()),
                highlighted_row: Some(0),
                ..Default::default()
            };
            loaded.error = "a wrap-worthy failure".to_owned();
            for vm in [
                ProcessListViewVm::default(),
                ProcessListViewVm { loading: true, ..Default::default() },
                loaded,
            ] {
                let rt = runtime(vm, size, dark);
                assert!(rt.elements().iter().any(|e| e.key.as_deref() == Some("process-list-view")));
                for e in rt.elements() {
                    let [x, y, w, h] = e.visible;
                    assert!(
                        x >= -0.5 && y >= -0.5 && x + w <= size.0 + 0.5 && y + h <= size.1 + 0.5,
                        "{size:?} dark={dark}: {} visible={:?} rect={:?} crosses window",
                        e.key.as_deref().unwrap_or(&e.path), e.visible, e.rect
                    );
                }
            }
        }
    }
}
