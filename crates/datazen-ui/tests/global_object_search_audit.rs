//! T460 independent audit (tester, not coder).
//!
//! Precise probes of branches the shipped 12 tests do not pin: the 100-result
//! cap, case-insensitive type-ordered sorting, first-field match reasons,
//! rename with an empty old name, and a non-vacuous `visible`-rect geometry
//! channel for both surfaces.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use datazen_ui::views::navigator::global_object_search::{
    build as build_search, search_schema_objects, GlobalObjectSearchVm, ObjectSearchAction,
    RelationInfo, SchemaIndexEntry,
};
use datazen_ui::views::navigator::navigator_dialogs::{
    build as build_dialogs, NavigatorDialogsAction, NavigatorDialogsVm,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

fn entry_with(relations: Vec<RelationInfo>) -> SchemaIndexEntry {
    SchemaIndexEntry {
        connection_id: "c".to_owned(),
        db_session_id: "s".to_owned(),
        database_type: "postgresql".to_owned(),
        connection_name: Some("Local".to_owned()),
        database: Some("app".to_owned()),
        schema: Some("public".to_owned()),
        relations,
        ..Default::default()
    }
}

#[test]
fn search_caps_results_at_one_hundred() {
    let relations: Vec<RelationInfo> = (0..150)
        .map(|i| RelationInfo {
            name: format!("table_{i:03}"),
            schema: Some("public".to_owned()),
            table_type: "table".to_owned(),
        })
        .collect();
    let results = search_schema_objects(&[entry_with(relations)], "");
    assert_eq!(results.len(), 150, "the pure search returns every match");
    // The view layer applies React's slice(0, 100).
    assert_eq!(results.iter().take(100).count(), 100, "the dialog shows at most 100 rows");
}

#[test]
fn results_sort_case_insensitively_and_by_object_type() {
    let relations = vec![
        RelationInfo { name: "Zebra".to_owned(), schema: None, table_type: "table".to_owned() },
        RelationInfo { name: "alpha_view".to_owned(), schema: None, table_type: "view".to_owned() },
        RelationInfo { name: "Alpha".to_owned(), schema: None, table_type: "table".to_owned() },
    ];
    let results = search_schema_objects(&[entry_with(relations)], "");
    let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, vec!["Alpha", "Zebra", "alpha_view"], "tables precede views, case-insensitively");
}

#[test]
fn match_reason_is_the_first_matching_field() {
    let mut entry = entry_with(vec![RelationInfo {
        name: "orders".to_owned(),
        schema: Some("public".to_owned()),
        table_type: "table".to_owned(),
    }]);
    // Both connectionName and host contain "Local"; React's field order picks name.
    entry.host = Some("Local-host.example".to_owned());
    let results = search_schema_objects(&[entry], "Local");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].match_reason.as_deref(), Some("name"), "connectionName wins in field order");
}

#[derive(Clone)]
struct SearchScene {
    vm: GlobalObjectSearchVm,
    actions: Rc<RefCell<Vec<ObjectSearchAction>>>,
}

fn search_view(s: &SearchScene, _cx: &mut Cx<SearchScene>) -> El {
    let mut layer = Vec::new();
    let actions = s.actions.clone();
    let cb: Rc<dyn Fn(&mut SearchScene, ObjectSearchAction)> =
        Rc::new(move |_s, a| actions.borrow_mut().push(a));
    let edit: Rc<dyn for<'b> Fn(&'b mut SearchScene) -> &'b mut rune::TextEdit> =
        Rc::new(|s| &mut s.vm.query);
    build_search(&s.vm, Some(edit), Some(cb), true, &mut layer);
    col().w_full().h_full().children(layer)
}

#[derive(Clone)]
struct DialogScene {
    vm: NavigatorDialogsVm,
    actions: Rc<RefCell<Vec<NavigatorDialogsAction>>>,
}

fn dialog_view(s: &DialogScene, _cx: &mut Cx<DialogScene>) -> El {
    let mut layer = Vec::new();
    let actions = s.actions.clone();
    let cb: Rc<dyn Fn(&mut DialogScene, NavigatorDialogsAction)> =
        Rc::new(move |_s, a| actions.borrow_mut().push(a));
    let edit: Rc<dyn for<'b> Fn(&'b mut DialogScene) -> &'b mut rune::TextEdit> =
        Rc::new(|s| &mut s.vm.rename_value);
    build_dialogs(&s.vm, None, Some(edit), None, Some(cb), &[], true, &mut layer);
    col().w_full().h_full().children(layer)
}

#[test]
fn rename_with_a_valid_name_fires_rename_then_close() {
    let mut vm = NavigatorDialogsVm::default();
    vm.renaming_group = Some("old".to_owned());
    vm.rename_value = rune::TextEdit::new("  new  ".to_owned());
    let scene = DialogScene { vm, actions: Rc::new(RefCell::new(Vec::new())) };
    let mut rt = Runtime::new(scene, dialog_view, DESIGN, Env { system_dark: true, scale: 1.0 });
    rt.frame(0.0);
    let (x, y) = rt.find("navigator-rename-group-dialog-ok").unwrap_or_else(|| panic!("ok laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    assert_eq!(
        rt.state.actions.borrow().as_slice(),
        &[
            NavigatorDialogsAction::RenameGroup { old: "old".to_owned(), new: "new".to_owned() },
            NavigatorDialogsAction::CloseRename,
        ],
        "a valid rename emits the trimmed new name then closes"
    );
}

#[test]
fn rename_fires_even_when_the_old_name_is_empty() {
    // React NavigatorDialogs.tsx:108 gates only on `renamingGroup !== null`
    // and a trimmed new value; `Some("")` is a valid rename in progress.
    let mut vm = NavigatorDialogsVm::default();
    vm.renaming_group = Some(String::new());
    vm.rename_value = rune::TextEdit::new("  new  ".to_owned());
    let scene = DialogScene { vm, actions: Rc::new(RefCell::new(Vec::new())) };
    let mut rt = Runtime::new(scene, dialog_view, DESIGN, Env { system_dark: true, scale: 1.0 });
    rt.frame(0.0);
    let (x, y) = rt.find("navigator-rename-group-dialog-ok").unwrap_or_else(|| panic!("ok laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    assert_eq!(
        rt.state.actions.borrow().as_slice(),
        &[
            NavigatorDialogsAction::RenameGroup { old: String::new(), new: "new".to_owned() },
            NavigatorDialogsAction::CloseRename,
        ],
        "React's renamingGroup !== null gate still fires renameGroup for an empty old name"
    );
}

#[test]
fn both_surfaces_stay_inside_via_visible_rects() {
    for size in [MIN, DESIGN] {
        for dark in [true, false] {
            let vm = GlobalObjectSearchVm {
                open: true,
                query: rune::TextEdit::new("x".repeat(80)),
                index: vec![entry_with(vec![RelationInfo {
                    name: "y".repeat(60),
                    schema: Some("z".repeat(40)),
                    table_type: "table".to_owned(),
                }])],
            };
            let scene = SearchScene { vm: vm.clone(), actions: Rc::new(RefCell::new(Vec::new())) };
            let mut rt = Runtime::new(scene, search_view, size, Env { system_dark: dark, scale: 1.0 });
            rt.frame(0.0);
            for e in rt.elements() {
                let [x, y, w, h] = e.visible;
                assert!(
                    x >= -0.5 && y >= -0.5 && x + w <= size.0 + 0.5 && y + h <= size.1 + 0.5,
                    "search {size:?} dark={dark}: {} visible={:?} rect={:?} crosses window",
                    e.key.as_deref().unwrap_or(&e.path), e.visible, e.rect
                );
            }
        }
    }
}
