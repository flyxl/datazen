//! T337: DocumentConnectionView port — composition keys, tab/sidebar branches,
//! host actions, state rules and two-size/two-palette geometry via rects.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use crate::i18n::t;
use crate::views::document_connection_view::{
    build, DocAction, DocEvent, DocResult, DocTab, DocumentViewVm,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

#[derive(Clone)]
struct Scene {
    vm: DocumentViewVm,
    actions: Rc<RefCell<Vec<DocAction>>>,
    dark: bool,
}

impl Scene {
    fn new(tab: DocTab, dark: bool) -> Self {
        Self {
            vm: DocumentViewVm {
                connection_name: "Local Mongo".to_owned(),
                db_session_id: "sess-1".to_owned(),
                connection_id: "cfg-1".to_owned(),
                database_type: "mongodb".to_owned(),
                active_tab: tab,
                databases: vec!["app".to_owned(), "analytics".to_owned()],
                selected_db: Some("app".to_owned()),
                collections: vec!["orders".to_owned(), "users".to_owned()],
                selected_collection: Some("orders".to_owned()),
                ..Default::default()
            },
            actions: Rc::new(RefCell::new(Vec::new())),
            dark,
        }
    }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let mut layer = Vec::new();
    let actions = s.actions.clone();
    let callback: Rc<dyn Fn(&mut Scene, DocAction)> =
        Rc::new(move |state: &mut Scene, action: DocAction| {
            state.actions.borrow_mut().push(action);
            let _ = &actions;
        });
    build::<Scene>(&s.vm, Some(callback), s.dark, &mut layer);
    col().w_full().h_full().children(layer)
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

fn text_of<S>(rt: &Runtime<S>, key: &str) -> String {
    element(rt, key).text.unwrap_or_default()
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn assert_inside<S>(rt: &Runtime<S>, size: (f32, f32), label: &str) {
    let outside: Vec<String> = rt
        .elements()
        .iter()
        .filter(|e| {
            let r = e.rect;
            r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > size.0 + 0.5 || r[1] + r[3] > size.1 + 0.5
        })
        .map(|e| format!("{} {:?}", e.key.as_deref().unwrap_or(&e.path), e.rect))
        .collect();
    assert!(outside.is_empty(), "{label}: {} node(s) outside {size:?}: {outside:?}", outside.len());
}

// --- 1. composition keys and tabs -------------------------------------------

#[test]
fn header_and_documents_shell_pin_all_composition_keys() {
    let rt = runtime(Scene::new(DocTab::Documents, true), DESIGN);
    for key in [
        "doc-view", "doc-view-header", "doc-view-refresh", "doc-view-tab-documents",
        "doc-view-tab-queries", "doc-view-connection-name", "doc-view-documents",
        "doc-view-sidebar", "doc-view-databases", "doc-view-collection-search",
        "doc-view-collections", "doc-view-main", "doc-view-toolbar", "doc-view-filter",
        "doc-view-apply-filter", "doc-view-insert",
    ] {
        assert!(has_key(&rt, key), "{key} is mounted");
    }
    assert_eq!(text_of(&rt, "doc-view-connection-name"), "Local Mongo");
    assert!(has_key(&rt, "doc-view-db-app"));
    assert!(has_key(&rt, "doc-view-collection-orders"));
}

#[test]
fn tab_and_sidebar_presses_emit_host_actions() {
    let mut rt = runtime(Scene::new(DocTab::Documents, true), DESIGN);
    click(&mut rt, "doc-view-tab-queries");
    click(&mut rt, "doc-view-refresh");
    click(&mut rt, "doc-view-db-analytics");
    click(&mut rt, "doc-view-collection-users");
    let actions = rt.state.actions.borrow().clone();
    assert!(actions.contains(&DocAction::SelectTab(DocTab::Queries)));
    assert!(actions.contains(&DocAction::Refresh));
    assert!(actions.contains(&DocAction::SelectDb("analytics".to_owned())));
    assert!(actions.contains(&DocAction::SelectCollection("users".to_owned())));
}

// --- 2. result / editor / errors --------------------------------------------

#[test]
fn result_table_and_detail_editor_render_with_exact_keys() {
    let mut scene = Scene::new(DocTab::Documents, true);
    scene.vm.result = Some(DocResult {
        columns: vec!["_id".to_owned(), "status".to_owned()],
        rows: vec![vec!["1".to_owned(), "paid".to_owned()]],
        ids: vec![Some("1".to_owned())],
    });
    scene.vm.selected_row_idx = Some(0);
    scene.vm.edit_text = "{\"_id\":1,\"status\":\"paid\"}".to_owned();
    let rt = runtime(scene, DESIGN);
    assert!(has_key(&rt, "doc-view-result"));
    assert!(has_key(&rt, "doc-view-result-row-0"));
    assert!(has_key(&rt, "doc-view-detail"));
    assert!(has_key(&rt, "doc-view-detail-save"));
    assert!(has_key(&rt, "doc-view-detail-delete"));
    assert_eq!(text_of(&rt, "doc-view-doc-count"), t("docView.docCount").replace("{count}", "1"));
    assert!(!has_key(&rt, "doc-view-detail-no-id"));
}

#[test]
fn missing_id_disables_save_and_shows_the_hint() {
    let mut scene = Scene::new(DocTab::Documents, true);
    scene.vm.insert_mode = true;
    scene.vm.edit_text = "{}".to_owned();
    let rt = runtime(scene, DESIGN);
    assert!(has_key(&rt, "doc-view-detail"));
    assert!(!has_key(&rt, "doc-view-detail-save"), "insert mode uses the insert action");
    assert!(has_key(&rt, "doc-view-detail-insert"));

    let mut scene = Scene::new(DocTab::Documents, true);
    scene.vm.selected_row_idx = Some(0);
    scene.vm.edit_text = "{\"status\":\"paid\"}".to_owned();
    let rt = runtime(scene, DESIGN);
    assert!(has_key(&rt, "doc-view-detail-no-id"));
}

#[test]
fn filter_and_docs_errors_render_under_their_keys() {
    let mut scene = Scene::new(DocTab::Documents, true);
    scene.vm.filter_error = "Filter must be JSON".to_owned();
    let rt = runtime(scene, DESIGN);
    assert!(has_key(&rt, "doc-view-filter-error"));

    let mut scene = Scene::new(DocTab::Documents, true);
    scene.vm.docs_error = "host refused".to_owned();
    let rt = runtime(scene, DESIGN);
    assert!(has_key(&rt, "doc-view-docs-error"));
}

#[test]
fn filter_error_classification_keeps_filter_errors_separate() {
    let mut vm = DocumentViewVm::default();
    vm.docs_loading = true;
    vm.apply_documents(Err("Filter must be object".to_owned()));
    assert!(!vm.docs_loading && !vm.filter_error.is_empty() && vm.docs_error.is_empty());
    vm.apply_documents(Err("host refused".to_owned()));
    assert!(vm.filter_error.is_empty() && vm.docs_error == "host refused");
}

// --- 3. query tab and state machine -----------------------------------------

#[test]
fn query_tab_pins_run_editor_and_empty_hint() {
    let rt = runtime(Scene::new(DocTab::Queries, true), DESIGN);
    for key in ["doc-view-queries", "doc-view-query-header", "doc-view-query-execute", "doc-view-query-editor", "doc-view-query-hint"] {
        assert!(has_key(&rt, key), "{key} is mounted");
    }
    let mut rt = runtime(Scene::new(DocTab::Queries, true), DESIGN);
    click(&mut rt, "doc-view-query-execute");
    assert!(rt.state.actions.borrow().contains(&DocAction::QueryExecute));
}

#[test]
fn host_events_drive_the_vm_without_losing_selection_rules() {
    let mut vm = DocumentViewVm::default();
    vm.selected_collection = Some("orders".to_owned());
    vm.select_db("app".to_owned());
    assert_eq!(vm.selected_db.as_deref(), Some("app"));
    assert!(vm.selected_collection.is_none() && vm.result.is_none());

    vm.apply_event(DocEvent::DocumentsLoaded(Ok(Some(DocResult {
        columns: vec!["_id".to_owned()],
        rows: vec![vec!["1".to_owned()]],
        ids: vec![Some("1".to_owned())],
    }))));
    assert_eq!(vm.doc_count(), 1);
    vm.open_insert();
    assert!(vm.insert_mode && vm.edit_text == "{}");
    vm.close_editor();
    assert!(!vm.insert_mode && vm.edit_text.is_empty());

    vm.apply_event(DocEvent::QueryExecuted(Ok(vec![DocResult::default()])));
    assert_eq!(vm.query.results.len(), 1);
}

#[test]
fn collection_filter_is_a_soft_substring_match() {
    let mut vm = DocumentViewVm::default();
    vm.collections = vec!["orders".to_owned(), "users".to_owned()];
    vm.collection_filter = "ORD".to_owned();
    assert_eq!(vm.filtered_collections().len(), 1);
    vm.collection_filter = "nope".to_owned();
    assert!(vm.filtered_collections().is_empty());
}

// --- 4. geometry -------------------------------------------------------------

#[test]
fn both_tabs_fit_both_sizes_and_both_palettes() {
    for tab in [DocTab::Documents, DocTab::Queries] {
        for size in [MIN, DESIGN] {
            for dark in [true, false] {
                let rt = runtime(Scene::new(tab, dark), size);
                assert!(has_key(&rt, "doc-view"), "{tab:?} {size:?} dark={dark}: root mounted");
                assert_inside(&rt, size, &format!("{tab:?} {size:?} dark={dark}"));
            }
        }
    }
}

#[test]
fn connection_page_keeps_the_document_slot() {
    use crate::views::connection_content_with_vm;
    use crate::views::content_view::ContentViewVm;

    fn slot_view(state: &crate::state_def::AppState, _cx: &mut Cx<crate::state_def::AppState>) -> El {
        connection_content_with_vm(state, rune::tok::theme(true), true, &ContentViewVm::default())
    }
    let mut rt = Runtime::new(
        crate::state_def::AppState::default(),
        slot_view,
        MIN,
        Env { system_dark: true, scale: 1.0 },
    );
    rt.frame(0.0);
    assert!(has_key(&rt, "doc-view-host"), "the document view composition slot is stable");
}

#[test]
fn tab_underline_follows_the_active_palette() {
    use crate::ui::kit::kit;
    let light = runtime(Scene::new(DocTab::Documents, false), DESIGN);
    let underline = element(&light, "doc-view-tab-documents-underline");
    assert_eq!(
        underline.fill,
        kit(false).c.text_base,
        "the selected tab underline uses the active theme"
    );
    let dark = runtime(Scene::new(DocTab::Documents, true), DESIGN);
    assert_eq!(element(&dark, "doc-view-tab-documents-underline").fill, kit(true).c.text_base);
}

#[test]
fn document_id_keeps_commas_inside_the_string() {
    use crate::views::document_connection_view::document_id;
    assert_eq!(document_id("{\"_id\":\"a,b\"}").as_deref(), Some("a,b"));
    assert_eq!(document_id("{\"_id\":\"abc\",\"x\":1}").as_deref(), Some("abc"));
    assert_eq!(document_id("{\"_id\":42}").as_deref(), Some("42"));
}
