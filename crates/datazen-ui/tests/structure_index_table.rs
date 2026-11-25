//! T532: `StructureIndexTable` — the React `suggestedIndexName` contract,
//! capability-gated columns/controls, rows/flags, host actions and rect
//! geometry at both sizes in both palettes. Every edit is a recorded
//! [`StructureIndexAction`]; no fake IPC.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use datazen_ui::i18n::t;
use datazen_ui::structure_editor::{StructureCapabilities, StructureIndexDraft};
use datazen_ui::views::structure_index_table::{
    build, suggested_index_name, IndexPatch, StructureIndexAction, StructureIndexActionFn,
    StructureIndexTableVm, EMPTY_KEY, ROOT_KEY,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

#[derive(Clone)]
struct Scene {
    vm: StructureIndexTableVm,
    actions: Rc<RefCell<Vec<StructureIndexAction>>>,
    dark: bool,
}

fn caps(create_index: bool, drop_index: bool, index_type: bool) -> StructureCapabilities {
    StructureCapabilities {
        create_index,
        drop_index,
        index_type,
        ..Default::default()
    }
}

fn draft() -> StructureIndexDraft {
    StructureIndexDraft {
        id: "idx-1".to_owned(),
        name: "idx_users_email".to_owned(),
        columns: vec!["email".to_owned(), "name".to_owned()],
        is_unique: true,
        is_primary: false,
        index_type: None,
        include_columns: None,
        filter: None,
        comment: None,
    }
}

fn vm(all_caps: bool) -> StructureIndexTableVm {
    StructureIndexTableVm {
        caps: Some(if all_caps {
            caps(true, true, true)
        } else {
            caps(true, true, false)
        }),
        index_methods: vec!["btree".to_owned(), "hash".to_owned()],
        column_names: vec!["id".to_owned(), "email".to_owned(), "name".to_owned()],
        table_name: "users".to_owned(),
        indexes: vec![draft()],
        ..Default::default()
    }
}

fn scene(vm: StructureIndexTableVm, dark: bool) -> Scene {
    Scene { vm, actions: Rc::new(RefCell::new(Vec::new())), dark }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let actions = s.actions.clone();
    let on_action: Option<StructureIndexActionFn<Scene>> =
        Some(Rc::new(move |_st, action| actions.borrow_mut().push(action)));
    build(&s.vm, &on_action, s.dark)
}

fn runtime(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn element(rt: &Runtime<Scene>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has_key(rt: &Runtime<Scene>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn click(rt: &mut Runtime<Scene>, key: &str) {
    rt.frame(0.0);
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
}

#[test]
fn suggested_name_matches_react() {
    assert_eq!(suggested_index_name("demo_sales", &[]), "idx_demo_sales");
    assert_eq!(suggested_index_name("demo_sales", &["region".to_owned(), "category".to_owned()]), "idx_demo_sales_region_category");
    assert_eq!(suggested_index_name("", &["id".to_owned()]), "idx_table_id");
}

#[test]
fn empty_state_and_capability_gated_headers() {
    let mut empty = vm(true);
    empty.indexes.clear();
    let rt = runtime(scene(empty, true), MIN);
    assert!(has_key(&rt, EMPTY_KEY));
    assert!(has_key(&rt, "structure-index-empty"));
    assert!(!has_key(&rt, ROOT_KEY));

    let rt = runtime(scene(vm(true), false), DESIGN);
    assert_eq!(text(&rt, "structure-index-head-0"), t("indexes.indexName"));
    assert_eq!(text(&rt, "structure-index-head-1"), t("indexes.columns"));
    assert_eq!(text(&rt, "structure-index-head-2"), t("indexes.unique"));
    assert_eq!(text(&rt, "structure-index-head-3"), t("indexes.indexType"));

    let rt = runtime(scene(vm(false), true), DESIGN);
    assert!(!has_key(&rt, "structure-index-head-3"), "indexType hidden without capability");
}

fn text(rt: &Runtime<Scene>, key: &str) -> String {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).and_then(|e| e.text).unwrap_or_default()
}

#[test]
fn rows_flags_and_host_actions() {
    let mut rt = runtime(scene(vm(true), true), DESIGN);
    assert!(has_key(&rt, "structure-index-row-idx-1"));
    assert!(has_key(&rt, "structure-index-chip-idx-1-0"));
    assert!(has_key(&rt, "structure-index-chip-idx-1-1"));
    assert_eq!(text(&rt, "structure-index-chip-order-idx-1-0"), "#1");
    assert!(has_key(&rt, "structure-index-add-idx-1-id"), "available column offered");

    click(&mut rt, "structure-index-chip-remove-idx-1-0");
    assert!(rt.state.actions.borrow().iter().any(|a| matches!(
        a,
        StructureIndexAction::Update { id, patch: IndexPatch::Columns(cols) }
            if id == "idx-1" && cols == &vec!["name".to_owned()]
    )));

    click(&mut rt, "structure-index-add-idx-1-id");
    assert!(rt.state.actions.borrow().iter().any(|a| matches!(
        a,
        StructureIndexAction::AddColumn { id, column } if id == "idx-1" && column == "id"
    )));

    click(&mut rt, "structure-index-remove-idx-1");
    assert_eq!(rt.state.actions.borrow().last(), Some(&StructureIndexAction::Remove("idx-1".to_owned())));

    click(&mut rt, "structure-index-sort-0");
    assert!(rt.state.actions.borrow().contains(&StructureIndexAction::Sort(0)));
}

#[test]
fn disabled_drop_hides_action_and_geometry() {
    let mut state = vm(true);
    state.caps = Some(caps(true, false, true));
    let mut rt = runtime(scene(state, false), MIN);
    assert!(!element(&rt, "structure-index-remove-idx-1").focusable, "drop disabled without capability");
    click(&mut rt, "structure-index-remove-idx-1");
    assert!(!rt.state.actions.borrow().iter().any(|a| matches!(a, StructureIndexAction::Remove(_))));

    for (w, h) in [MIN, DESIGN] {
        for dark in [true, false] {
            let rt = runtime(scene(vm(true), dark), (w, h));
            for el in rt.elements().into_iter().filter(|e| !e.exiting) {
                let [x, y, ew, eh] = el.rect;
                assert!(x >= -0.5 && x + ew <= w + 0.5, "{w}x{h} dark={dark}: {:?} {:?} horizontal", el.key, el.rect);
                assert!(y >= -0.5 && y + eh <= h + 0.5, "{w}x{h} dark={dark}: {:?} {:?} vertical", el.key, el.rect);
            }
        }
    }
}
