//! T460: `GlobalObjectSearch` + `NavigatorDialogs` ports. React's PageIntegration
//! journey is reproduced through real pointer/key events and `Runtime::elements()`
//! observation: query → matched result → routed table action. Also pins the new
//! group / rename / object-filter host actions, host-supplied confirm children,
//! and rect-based geometry at both window sizes in both themes.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, row, Cx, El, Env, Runtime};

use crate::i18n::t;
use crate::views::navigator::global_object_search::{
    build as build_search, GlobalObjectSearchVm, ObjectSearchAction, RelationInfo, SchemaIndexEntry,
    TableActionKind,
};
use crate::views::navigator::navigator_dialogs::{
    build as build_dialogs, NavigatorDialogsAction, NavigatorDialogsVm,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

fn index() -> Vec<SchemaIndexEntry> {
    vec![SchemaIndexEntry {
        connection_id: "connection-1".to_owned(),
        db_session_id: "session-1".to_owned(),
        database_type: "postgresql".to_owned(),
        connection_name: Some("Local database".to_owned()),
        host: Some("db.example.test".to_owned()),
        database: Some("sales".to_owned()),
        schema: Some("public".to_owned()),
        relations: vec![RelationInfo {
            name: "orders".to_owned(),
            schema: Some("public".to_owned()),
            table_type: "table".to_owned(),
        }],
        objects: Vec::new(),
        columns: vec![("orders".to_owned(), vec!["id".to_owned(), "customer_email".to_owned()])],
    }]
}

fn search_vm(entries: Vec<SchemaIndexEntry>) -> GlobalObjectSearchVm {
    GlobalObjectSearchVm {
        open: true,
        query: rune::TextEdit::new(String::new()),
        index: entries,
    }
}

#[derive(Clone)]
struct SearchScene {
    vm: GlobalObjectSearchVm,
    actions: Rc<RefCell<Vec<ObjectSearchAction>>>,
    dark: bool,
}

fn search_scene(vm: GlobalObjectSearchVm, dark: bool) -> SearchScene {
    SearchScene {
        vm,
        actions: Rc::new(RefCell::new(Vec::new())),
        dark,
    }
}

fn search_view(s: &SearchScene, _cx: &mut Cx<SearchScene>) -> El {
    let mut layer = Vec::new();
    let actions = s.actions.clone();
    let callback = Rc::new(move |state: &mut SearchScene, action| {
        actions.borrow_mut().push(action);
        let _ = state;
    });
    let edit: Rc<dyn for<'b> Fn(&'b mut SearchScene) -> &'b mut rune::TextEdit> =
        Rc::new(|state| &mut state.vm.query);
    build_search(&s.vm, Some(edit), Some(callback), s.dark, &mut layer);
    col().w_full().h_full().children(layer)
}

fn search_runtime(scene: SearchScene, size: (f32, f32)) -> Runtime<SearchScene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, search_view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn text_of<S>(rt: &Runtime<S>, key: &str) -> String {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .and_then(|e| e.text)
        .unwrap_or_default()
}

fn texts<S>(rt: &Runtime<S>) -> Vec<String> {
    rt.elements().iter().filter_map(|e| e.text.clone()).collect()
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn type_into<S>(rt: &mut Runtime<S>, key: &str, value: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.paste(value);
    rt.frame(0.0);
}

fn assert_inside<S>(rt: &Runtime<S>, size: (f32, f32), label: &str) {
    let (w, h) = size;
    let outside: Vec<String> = rt
        .elements()
        .iter()
        .filter(|e| {
            let r = e.rect;
            r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > w + 0.5 || r[1] + r[3] > h + 0.5
        })
        .map(|e| format!("{} {:?}", e.key.as_deref().unwrap_or(&e.path), e.rect))
        .collect();
    assert!(outside.is_empty(), "{label}: {} node(s) outside {size:?}: {outside:?}", outside.len());
}

// --- GlobalObjectSearch -------------------------------------------------------

#[test]
fn column_query_shows_match_reason_and_routes_the_result() {
    let mut rt = search_runtime(search_scene(search_vm(index()), true), DESIGN);
    assert!(has_key(&rt, "global-object-search"), "React testId mounts");
    assert!(has_key(&rt, "global-object-search-input"), "search input mounts");
    type_into(&mut rt, "global-object-search-input", "customer_email");

    let rendered = texts(&rt);
    assert!(rendered.iter().any(|x| x == "customer_email"), "matching column is shown");
    let matched = t("objectSearch.matched").replace("{field}", "column");
    assert!(
        rendered.iter().any(|x| x.contains(&matched)),
        "the match reason is the column field: {rendered:?}"
    );
    assert!(has_key(&rt, "global-object-search-result"), "result rows mount");

    click(&mut rt, "global-object-search-result-name");
    let actions = rt.state.actions.borrow();
    let Some(ObjectSearchAction::OpenResult(result)) = actions.first() else {
        panic!("clicking a result emits OpenResult: {actions:?}");
    };
    assert_eq!(result.object_type, crate::views::navigator::global_object_search::ObjectType::Column);
    assert_eq!(result.table_name.as_deref(), Some("orders"));
    assert_eq!(result.name, "customer_email");
}

#[test]
fn table_result_offers_all_five_actions_and_routes_open_data() {
    let mut rt = search_runtime(search_scene(search_vm(index()), true), DESIGN);
    type_into(&mut rt, "global-object-search-input", "orders");

    for action in TableActionKind::ALL {
        let key = format!("object-search-action-{}", action.as_str());
        assert!(has_key(&rt, &key), "table result offers {key}");
    }
    click(&mut rt, "object-search-action-openData");
    let actions = rt.state.actions.borrow();
    let Some(ObjectSearchAction::OpenTableAction(result, kind)) = actions.first() else {
        panic!("clicking Data emits OpenTableAction: {actions:?}");
    };
    assert_eq!(result.name, "orders");
    assert_eq!(*kind, TableActionKind::OpenData);
}

#[test]
fn non_table_results_have_no_table_actions() {
    let mut rt = search_runtime(search_scene(search_vm(index()), true), DESIGN);
    type_into(&mut rt, "global-object-search-input", "customer_email");
    assert!(!has_key(&rt, "object-search-action-openData"), "column hits have no table actions");
}

#[test]
fn empty_index_and_no_match_show_distinct_placeholders() {
    let rt = search_runtime(search_scene(search_vm(Vec::new()), true), DESIGN);
    assert_eq!(text_of(&rt, "global-object-search-empty"), t("objectSearch.noLoadedSchema"));

    let mut rt = search_runtime(search_scene(search_vm(index()), true), DESIGN);
    type_into(&mut rt, "global-object-search-input", "zzz-no-match");
    assert_eq!(text_of(&rt, "global-object-search-empty"), t("objectSearch.noResults"));
}

#[test]
fn clear_button_appears_with_a_query_and_emits_clear() {
    let mut rt = search_runtime(search_scene(search_vm(index()), true), DESIGN);
    assert!(!has_key(&rt, "global-object-search-clear"), "no clear button while empty");
    type_into(&mut rt, "global-object-search-input", "orders");
    assert!(has_key(&rt, "global-object-search-clear"), "clear button appears with a query");
    click(&mut rt, "global-object-search-clear");
    assert!(
        rt.state.actions.borrow().first() == Some(&ObjectSearchAction::ClearQuery),
        "clear emits ClearQuery"
    );
}

#[test]
fn closed_search_dialog_mounts_nothing() {
    let mut vm = search_vm(index());
    vm.open = false;
    let rt = search_runtime(search_scene(vm, true), DESIGN);
    assert!(!has_key(&rt, "global-object-search"), "closed dialog is absent");
    assert!(!has_key(&rt, "global-object-search-input"), "closed input is absent");
}

// --- NavigatorDialogs ---------------------------------------------------------

#[derive(Clone)]
struct DialogScene {
    vm: NavigatorDialogsVm,
    actions: Rc<RefCell<Vec<NavigatorDialogsAction>>>,
    dark: bool,
}

fn dialog_scene(vm: NavigatorDialogsVm, dark: bool) -> DialogScene {
    DialogScene {
        vm,
        actions: Rc::new(RefCell::new(Vec::new())),
        dark,
    }
}

fn dialog_view(s: &DialogScene, _cx: &mut Cx<DialogScene>) -> El {
    let mut layer = Vec::new();
    let actions = s.actions.clone();
    let callback = Rc::new(move |state: &mut DialogScene, action| {
        actions.borrow_mut().push(action);
        let _ = state;
    });
    let new_edit: Rc<dyn for<'b> Fn(&'b mut DialogScene) -> &'b mut rune::TextEdit> =
        Rc::new(|state| &mut state.vm.new_group_name);
    let rename_edit: Rc<dyn for<'b> Fn(&'b mut DialogScene) -> &'b mut rune::TextEdit> =
        Rc::new(|state| &mut state.vm.rename_value);
    let filter: Rc<dyn for<'b> Fn(&'b mut DialogScene) -> &'b mut crate::components::connection::object_filter_dialog::ObjectFilterVm> =
        Rc::new(|state| &mut state.vm.object_filter);
    let confirms = [
        row().key("confirm-delete-group").h(4.0),
        row().key("confirm-drop-database").h(4.0),
    ];
    build_dialogs(&s.vm, Some(new_edit), Some(rename_edit), Some(filter), Some(callback), &confirms, s.dark, &mut layer);
    col().w_full().h_full().children(layer)
}

fn dialog_runtime(scene: DialogScene, size: (f32, f32)) -> Runtime<DialogScene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, dialog_view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

#[test]
fn new_group_dialog_trims_the_name_and_routes_add_then_close() {
    let vm = NavigatorDialogsVm {
        new_group_open: true,
        ..Default::default()
    };
    let mut rt = dialog_runtime(dialog_scene(vm, true), DESIGN);
    type_into(&mut rt, "navigator-new-group-dialog-input", "  Dev Team  ");
    click(&mut rt, "navigator-new-group-dialog-ok");
    let actions = rt.state.actions.borrow();
    assert!(actions.contains(&NavigatorDialogsAction::AddGroup("Dev Team".to_owned())), "trimmed name added");
    assert!(actions.contains(&NavigatorDialogsAction::CloseNewGroup), "dialog closes after add");
}

#[test]
fn empty_group_name_only_closes_the_dialog() {
    let vm = NavigatorDialogsVm {
        new_group_open: true,
        ..Default::default()
    };
    let mut rt = dialog_runtime(dialog_scene(vm, true), DESIGN);
    click(&mut rt, "navigator-new-group-dialog-ok");
    assert_eq!(
        rt.state.actions.borrow().as_slice(),
        &[NavigatorDialogsAction::CloseNewGroup],
        "empty names never reach AddGroup"
    );
}

#[test]
fn rename_dialog_routes_old_and_trimmed_new_names() {
    let vm = NavigatorDialogsVm {
        renaming_group: Some("Old".to_owned()),
        ..Default::default()
    };
    let mut rt = dialog_runtime(dialog_scene(vm, true), DESIGN);
    assert_eq!(text_of(&rt, "navigator-rename-group-dialog-title"), t("main.ctx.renameGroup"));
    type_into(&mut rt, "navigator-rename-group-dialog-input", "  New  ");
    click(&mut rt, "navigator-rename-group-dialog-ok");
    assert!(rt.state.actions.borrow().contains(&NavigatorDialogsAction::RenameGroup {
        old: "Old".to_owned(),
        new: "New".to_owned(),
    }), "rename carries both names");
}

#[test]
fn object_filter_save_and_close_map_to_host_actions() {
    let mut vm = NavigatorDialogsVm::default();
    vm.object_filter.open = true;
    let mut rt = dialog_runtime(dialog_scene(vm, true), DESIGN);
    assert!(has_key(&rt, "object-filter-dialog"), "the reused filter dialog mounts");
    click(&mut rt, "object-filter-save");
    assert!(rt.state.actions.borrow().iter().any(|a| matches!(a, NavigatorDialogsAction::SaveObjectFilter(_))), "save reaches host");
    click(&mut rt, "object-filter-cancel");
    assert!(rt.state.actions.borrow().contains(&NavigatorDialogsAction::CloseObjectFilter), "cancel closes");
}

#[test]
fn host_supplied_confirm_children_are_mounted_verbatim() {
    let rt = dialog_runtime(dialog_scene(NavigatorDialogsVm::default(), true), DESIGN);
    assert!(has_key(&rt, "confirm-delete-group"), "ReactNode children survive");
    assert!(has_key(&rt, "confirm-drop-database"), "all confirm children survive");
}

#[test]
fn closed_dialogs_mount_no_navigator_overlays() {
    let rt = dialog_runtime(dialog_scene(NavigatorDialogsVm::default(), true), DESIGN);
    assert!(!has_key(&rt, "navigator-new-group-dialog"), "new group closed");
    assert!(!has_key(&rt, "navigator-rename-group-dialog"), "rename closed");
    assert!(!has_key(&rt, "object-filter-dialog"), "filter closed");
}

// --- Geometry -----------------------------------------------------------------

#[test]
fn both_surfaces_fit_both_sizes_in_both_themes() {
    for size in [MIN, DESIGN] {
        for dark in [true, false] {
            let search = search_runtime(search_scene(search_vm(index()), dark), size);
            assert_inside(&search, size, &format!("search {size:?} dark={dark}"));

            let mut vm = NavigatorDialogsVm {
                new_group_open: true,
                renaming_group: Some("Old".to_owned()),
                ..Default::default()
            };
            vm.object_filter.open = true;
            let dialogs = dialog_runtime(dialog_scene(vm, dark), size);
            assert_inside(&dialogs, size, &format!("dialogs {size:?} dark={dark}"));
        }
    }
}

// Reverse verification (mutation -> observed failure text -> restored):
// 1. Returning no results for a non-empty query made
//    `column_query_shows_match_reason_and_routes_the_result` fail on
//    `the match reason is the column field`.
// 2. Dropping the trim in the new-group OK action made
//    `new_group_dialog_trims_the_name_and_routes_add_then_close` fail on
//    `trimmed name added`.
