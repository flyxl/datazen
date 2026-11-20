//! T498: `ProcessListView` port — toolbar keys/copy, process rows, refresh and
//! kill host actions (with React's confirm copy), selection/sort, empty/loading
//! states and rect-based geometry at both window sizes in both themes.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};
use rune_kit::SortDir;

use crate::i18n::t;
use crate::views::process_list_view::{
    build, column_label_key, pid_column_index, ProcessColumn, ProcessListAction,
    ProcessListViewVm, ProcessResult,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

fn result() -> ProcessResult {
    ProcessResult {
        columns: vec![
            ProcessColumn { name: "pid".to_owned(), data_type: "integer".to_owned() },
            ProcessColumn { name: "user".to_owned(), data_type: "string".to_owned() },
            ProcessColumn { name: "database".to_owned(), data_type: "string".to_owned() },
        ],
        rows: vec![
            vec!["222".to_owned(), "bob".to_owned(), "app".to_owned()],
            vec!["111".to_owned(), "alice".to_owned(), "app".to_owned()],
        ],
    }
}

fn scene(vm: ProcessListViewVm, dark: bool) -> Scene {
    Scene { vm, actions: Rc::new(RefCell::new(Vec::new())), dark }
}

fn ready() -> ProcessListViewVm {
    ProcessListViewVm {
        db_session_id: "conn-pg".to_owned(),
        connection_name: "pg".to_owned(),
        result: Some(result()),
        ..Default::default()
    }
}

#[derive(Clone)]
struct Scene {
    vm: ProcessListViewVm,
    actions: Rc<RefCell<Vec<ProcessListAction>>>,
    dark: bool,
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let actions = s.actions.clone();
    let callback = Rc::new(move |state: &mut Scene, action| {
        match &action {
            ProcessListAction::SelectRow(row) => state.vm.highlighted_row = Some(*row),
            ProcessListAction::Sort(column) => {
                state.vm.sort = state
                    .vm
                    .sort
                    .filter(|(current, _)| current == column)
                    .map(|(_, dir)| (*column, if dir == SortDir::Ascending { SortDir::Descending } else { SortDir::Ascending }))
                    .or(Some((*column, SortDir::Ascending)));
            }
            _ => {}
        }
        actions.borrow_mut().push(action);
    });
    let mounted = build(&s.vm, Some(callback), s.dark);
    col().w_full().h_full().child(mounted)
}

fn runtime(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

// --- pure helpers -------------------------------------------------------------

#[test]
fn process_column_labels_are_exact_keys_and_pid_is_found_case_insensitively() {
    assert_eq!(column_label_key("pid"), Some("processList.colPid"));
    assert_eq!(column_label_key("user"), Some("processList.colUser"));
    assert_eq!(column_label_key("database"), Some("processList.colDatabase"));
    assert_eq!(column_label_key("client"), Some("processList.colClient"));
    assert_eq!(column_label_key("state"), Some("processList.colState"));
    assert_eq!(column_label_key("query"), Some("processList.colQuery"));
    assert_eq!(column_label_key("durationMs"), Some("processList.colDuration"));
    assert_eq!(column_label_key("unknown"), None);

    let mut r = result();
    assert_eq!(pid_column_index(&r), Some(0));
    r.columns[0].name = "PID".to_owned();
    assert_eq!(pid_column_index(&r), Some(0), "pid lookup is case-insensitive");
}

// --- keys, rows and toolbar ---------------------------------------------------

#[test]
fn toolbar_and_table_keys_and_copy_are_pinned() {
    let rt = runtime(scene(ready(), true), DESIGN);
    for key in [
        "process-list-view",
        "process-list-toolbar",
        "process-list-connection",
        "process-list-refresh",
        "process-list-kill",
        "process-list-table",
        "process-list-table-head",
        "process-list-table-row-0",
        "process-list-table-cell-0-0",
        "process-sort-0",
    ] {
        assert!(has_key(&rt, key), "{key} is mounted");
    }
    assert_eq!(element(&rt, "process-list-connection").text.as_deref(), Some("pg"));
    assert_eq!(element(&rt, "process-list-refresh").label.as_deref(), Some(t("processList.refresh")));
    assert_eq!(element(&rt, "process-list-kill").label.as_deref(), Some(t("processList.kill")));
    assert!((element(&rt, "process-list-toolbar").rect[3] - 36.0).abs() <= 0.5, "toolbar keeps h-9");
    // Both process rows and the localized column headers are visible.
    let texts: Vec<String> = rt.elements().iter().filter_map(|e| e.text.clone()).collect();
    for value in ["111", "222", "alice", "bob", t("processList.colPid"), t("processList.colUser")] {
        assert!(texts.iter().any(|x| x == value), "rendered value {value:?}");
    }
}

#[test]
fn kill_is_gated_on_a_selected_parsed_pid_row() {
    let rt = runtime(scene(ready(), true), DESIGN);
    assert!(!element(&rt, "process-list-kill").focusable, "no selection: kill is disabled");
    let mut vm = ready();
    vm.highlighted_row = Some(0);
    let rt = runtime(scene(vm, true), DESIGN);
    assert!(element(&rt, "process-list-kill").focusable, "selection enables kill");
}

// --- host actions -------------------------------------------------------------

#[test]
fn refresh_emits_refresh_and_row_click_emits_select_row() {
    let mut rt = runtime(scene(ready(), true), DESIGN);
    click(&mut rt, "process-list-refresh");
    assert_eq!(rt.state.actions.borrow().first(), Some(&ProcessListAction::Refresh));

    let mut rt = runtime(scene(ready(), true), DESIGN);
    click(&mut rt, "process-list-table-row-0");
    assert_eq!(rt.state.actions.borrow().first(), Some(&ProcessListAction::SelectRow(0)));
}

#[test]
fn kill_carries_pid_and_react_confirm_copy() {
    let mut vm = ready();
    vm.highlighted_row = Some(1); // rendered row 1 → pid 111
    let mut rt = runtime(scene(vm, true), DESIGN);
    click(&mut rt, "process-list-kill");
    let actions = rt.state.actions.borrow();
    let Some(ProcessListAction::KillRequest { db_session_id, pid, title, message, confirm_label }) = actions.first() else {
        panic!("kill emits KillRequest: {actions:?}");
    };
    assert_eq!(db_session_id, "conn-pg");
    assert_eq!(*pid, 111, "the pid comes from the highlighted row");
    assert_eq!(title, t("processList.killTitle"));
    assert_eq!(confirm_label, t("processList.kill"));
    let pid_title = t("processList.colPid");
    assert!(message.contains("111") && message.contains(pid_title), "confirm message: {message}");
}

#[test]
fn sort_strip_emits_sort_and_the_table_applies_it() {
    let mut rt = runtime(scene(ready(), true), DESIGN);
    click(&mut rt, "process-sort-0");
    assert_eq!(rt.state.actions.borrow().first(), Some(&ProcessListAction::Sort(0)));

    // With pid ascending the 111 row is first; with descending the 222 row is.
    let first_cell = |rt: &Runtime<Scene>| element(rt, "process-list-table-cell-0-0-text").text.unwrap_or_default();
    assert_eq!(first_cell(&rt), "111", "ascending puts the smaller pid first: {}", first_cell(&rt));
    click(&mut rt, "process-sort-0");
    assert_eq!(first_cell(&rt), "222", "descending reverses the order: {}", first_cell(&rt));
}

// --- empty / loading / error --------------------------------------------------

#[test]
fn empty_result_shows_the_table_empty_state() {
    let rt = runtime(scene(ProcessListViewVm::default(), true), DESIGN);
    assert!(has_key(&rt, "process-list-table-empty"), "empty state mounted");
    assert!(!has_key(&rt, "process-list-table-row-0"), "no rows");
}

#[test]
fn loading_disables_refresh_and_error_is_shown() {
    let mut vm = ready();
    vm.loading = true;
    vm.error = "session lost".to_owned();
    let rt = runtime(scene(vm, true), DESIGN);
    assert!(!element(&rt, "process-list-refresh").focusable, "refresh disabled while loading");
    assert!(element(&rt, "process-list-error-text").text.as_deref().unwrap_or_default().contains("session lost"), "error surfaced");
}

// --- geometry -----------------------------------------------------------------

#[test]
fn geometry_fits_both_sizes_in_both_themes() {
    for size in [MIN, DESIGN] {
        for dark in [true, false] {
            for vm in [ProcessListViewVm::default(), ready()] {
                let rt = runtime(scene(vm, dark), size);
                let outside: Vec<String> = rt
                    .elements()
                    .iter()
                    .filter(|e| {
                        let r = e.rect;
                        r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > size.0 + 0.5 || r[1] + r[3] > size.1 + 0.5
                    })
                    .map(|e| format!("{} {:?}", e.key.as_deref().unwrap_or(&e.path), e.rect))
                    .collect();
                assert!(outside.is_empty(), "{size:?} dark={dark}: outside {outside:?}");
            }
        }
    }
}

// Reverse verification (mutation -> observed failure text -> restored):
// 1. Wiring the sort strip to `Refresh` made `sort_strip_emits_sort_and_the_table_applies_it`
//    fail with `left: Some(Refresh) right: Some(Sort(0))`.
// 2. Dropping the highlighted-row gate from `can_kill` made `kill_is_gated_on_a_selected_parsed_pid_row`
//    fail on `no selection: kill is disabled`.
