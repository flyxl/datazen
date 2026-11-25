//! T337 independent audit (tester, not coder).
//!
//! Precise probes of branches the shipped 11 tests do not pin: initial-database
//! choice, the query active-result clamp, `show_editor`, real slot composition,
//! a non-vacuous `visible`-rect geometry check, plus temporary probes for the
//! hardcoded dark-kit tab underline and a comma inside a string document id.

use rune::{col, Cx, El, Env, Runtime};

use datazen_ui::views::connection_content_with_vm;
use datazen_ui::views::content_view::ContentViewVm;
use datazen_ui::views::document_connection_view::{
    build, document_id, DocResult, DocTab, DocumentViewVm, QueryVm,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

#[derive(Clone)]
struct AuditScene {
    vm: DocumentViewVm,
    dark: bool,
}

impl Default for AuditScene {
    fn default() -> Self {
        Self { vm: DocumentViewVm::default(), dark: true }
    }
}

fn view(s: &AuditScene, _cx: &mut Cx<AuditScene>) -> El {
    let mut layer = Vec::new();
    build::<AuditScene>(&s.vm, None, s.dark, &mut layer);
    col().w_full().h_full().children(layer)
}

fn runtime(scene: AuditScene, size: (f32, f32)) -> Runtime<AuditScene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

#[test]
fn initial_database_choice_prefers_a_present_initial_then_the_first() {
    let mut vm = DocumentViewVm::default();
    vm.databases = vec!["app".to_owned(), "analytics".to_owned()];
    assert_eq!(vm.initial_database_choice(), Some("app"));
    vm.initial_database = Some("analytics".to_owned());
    assert_eq!(vm.initial_database_choice(), Some("analytics"));
    vm.initial_database = Some("missing".to_owned());
    assert_eq!(vm.initial_database_choice(), Some("app"), "an absent initial falls back to first");
}

#[test]
fn document_id_handles_string_numeric_null_and_missing() {
    assert_eq!(document_id("{\"_id\":\"abc\",\"x\":1}").as_deref(), Some("abc"));
    assert_eq!(document_id("{\"_id\":42}").as_deref(), Some("42"));
    assert!(document_id("{\"_id\":null}").is_none());
    assert!(document_id("{\"x\":1}").is_none());
}

#[test]
fn query_executed_clamps_a_stale_active_result_index() {
    let mut vm = DocumentViewVm::default();
    vm.query.active_result_idx = 5;
    vm.apply_event(datazen_ui::views::document_connection_view::DocEvent::QueryExecuted(Ok(vec![
        DocResult::default(),
        DocResult::default(),
    ])));
    assert_eq!(vm.query.active_result_idx, 1, "a stale index clamps to the last available result");
    assert!(!vm.query.running);
}

#[test]
fn show_editor_tracks_insert_or_a_selected_row() {
    let mut vm = DocumentViewVm::default();
    assert!(!vm.show_editor());
    vm.selected_row_idx = Some(0);
    assert!(vm.show_editor());
    vm.selected_row_idx = None;
    vm.insert_mode = true;
    assert!(vm.show_editor());
}

#[test]
fn connection_page_composes_the_real_document_view_when_a_vm_is_supplied() {
    use std::cell::RefCell;

    thread_local! {
        static DOC_VM: RefCell<Option<ContentViewVm>> = RefCell::new(None);
    }
    fn composed(state: &datazen_ui::state_def::AppState, _cx: &mut Cx<datazen_ui::state_def::AppState>) -> El {
        let vm = DOC_VM.with(|f| f.borrow().clone()).unwrap_or_default();
        connection_content_with_vm(state, rune::tok::theme(true), true, &vm, &mut Vec::new())
    }
    let mut vm = ContentViewVm::default();
    vm.document_vm = Some(DocumentViewVm {
        connection_name: "Local".to_owned(),
        ..Default::default()
    });
    DOC_VM.with(|f| *f.borrow_mut() = Some(vm));
    let mut rt = Runtime::new(
        datazen_ui::state_def::AppState::default(),
        composed,
        DESIGN,
        Env { system_dark: true, scale: 1.0 },
    );
    rt.frame(0.0);
    assert!(has_key(&rt, "doc-view-host"), "the slot is preserved");
    assert!(has_key(&rt, "doc-view"), "connection_page must compose the real document view");
}

#[test]
fn both_tabs_stay_inside_via_visible_rects() {
    for tab in [DocTab::Documents, DocTab::Queries] {
        let mut vm = DocumentViewVm::default();
        vm.active_tab = tab;
        vm.connection_name = "x".repeat(100);
        vm.databases = vec!["db".repeat(20)];
        vm.collections = vec!["coll".repeat(30)];
        vm.selected_db = Some("db".to_owned());
        vm.selected_collection = Some("coll".to_owned());
        vm.result = Some(DocResult {
            columns: vec!["field".to_owned()],
            rows: vec![vec!["value".repeat(50)]],
            ids: vec![None],
        });
        vm.query = QueryVm { sql: "SELECT 1;".to_owned(), ..Default::default() };
        for size in [MIN, DESIGN] {
            for dark in [true, false] {
                let rt = runtime(AuditScene { vm: vm.clone(), dark }, size);
                assert!(has_key(&rt, "doc-view"), "{tab:?} {size:?} dark={dark}: root mounted");
                for e in rt.elements() {
                    let [x, y, w, h] = e.visible;
                    assert!(
                        x >= -0.5 && y >= -0.5 && x + w <= size.0 + 0.5 && y + h <= size.1 + 0.5,
                        "{tab:?} {size:?} dark={dark}: {} visible={:?} rect={:?} crosses window",
                        e.key.as_deref().unwrap_or(&e.path), e.visible, e.rect
                    );
                }
            }
        }
    }
}

