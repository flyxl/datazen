//! T322: DDLView port — three-way branch keys, copy feedback with a real 2000ms
//! wake, the DDL state machine and two-size/two-palette geometry.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime, Tx};

use crate::i18n::t;
use crate::views::ddl_view::{
    build, DdlAction, DdlIdentity, DdlViewState,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

#[derive(Clone)]
struct Scene {
    identity: DdlIdentity,
    state: DdlViewState,
    actions: Rc<RefCell<Vec<DdlAction>>>,
    copied_calls: Rc<RefCell<usize>>,
    dark: bool,
}

impl Scene {
    fn with_state(state: DdlViewState, dark: bool) -> Self {
        Self {
            identity: DdlIdentity {
                table_name: "users".to_owned(),
                database: "app".to_owned(),
                database_type: Some("postgresql".to_owned()),
                is_view: false,
            },
            state,
            actions: Rc::new(RefCell::new(Vec::new())),
            copied_calls: Rc::new(RefCell::new(0)),
            dark,
        }
    }

    fn loaded(dark: bool) -> Self {
        Self::with_state(
            DdlViewState {
                ddl: "CREATE TABLE users (id integer PRIMARY KEY);".to_owned(),
                ..Default::default()
            },
            dark,
        )
    }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let mut layer = Vec::new();
    let actions = s.actions.clone();
    let callback: Rc<dyn Fn(&mut Scene, DdlAction)> =
        Rc::new(move |state: &mut Scene, action: DdlAction| {
            state.actions.borrow_mut().push(action);
            let _ = &actions;
        });
    let copies = s.copied_calls.clone();
    let copy_callback: Rc<dyn Fn(&mut Scene, &mut Tx)> =
        Rc::new(move |state: &mut Scene, _tx: &mut Tx| {
            *state.copied_calls.borrow_mut() += 1;
            let _ = &copies;
        });
    build::<Scene>(
        &s.identity,
        &s.state,
        "ddl-view",
        0.0,
        Some(callback),
        Some(copy_callback),
        s.dark,
        &mut layer,
    );
    col().w_full().h_full().children(layer)
}

fn runtime(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
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

// --- 1. three branches -------------------------------------------------------

#[test]
fn loading_branch_shows_spinner_and_the_localized_generating_text() {
    let scene = Scene::with_state(DdlViewState { loading: true, ..Default::default() }, true);
    let rt = runtime(scene, DESIGN);
    assert!(has_key(&rt, "ddl-view-loading"));
    assert_eq!(text_of(&rt, "ddl-view-loading-text"), t("ddl.generating"));
    assert!(!has_key(&rt, "ddl-view-header"));
    assert!(!has_key(&rt, "ddl-view-error"));
}

#[test]
fn error_branch_shows_a_copyable_error_only() {
    let scene = Scene::with_state(DdlViewState { error: "boom".to_owned(), ..Default::default() }, true);
    let rt = runtime(scene, DESIGN);
    assert!(has_key(&rt, "ddl-view-error"));
    assert!(!has_key(&rt, "ddl-view"));
    assert!(!has_key(&rt, "ddl-view-header"));
}

#[test]
fn loaded_branch_pins_title_copy_and_ddl_keys() {
    let rt = runtime(Scene::loaded(true), DESIGN);
    for key in ["ddl-view", "ddl-view-header", "ddl-view-title", "ddl-view-copy", "ddl-view-code", "ddl-view-ddl"] {
        assert!(has_key(&rt, key), "{key} is mounted");
    }
    assert_eq!(text_of(&rt, "ddl-view-title"), "users · DDL");
    assert_eq!(text_of(&rt, "ddl-view-ddl"), "CREATE TABLE users (id integer PRIMARY KEY);");
    assert!(!has_key(&rt, "ddl-view-loading"));
    assert!(!has_key(&rt, "ddl-view-error"));
}

// --- 2. copy behavior --------------------------------------------------------

#[test]
fn copy_writes_the_ddl_and_schedules_a_real_2000ms_wake() {
    let mut rt = runtime(Scene::loaded(true), DESIGN);
    let clipboard = rune::clipboard::memory();
    rt.use_clipboard(&clipboard);
    rt.frame(0.0);
    click(&mut rt, "ddl-view-copy");

    assert_eq!(
        clipboard.text().as_deref().map(String::as_str),
        Some("CREATE TABLE users (id integer PRIMARY KEY);")
    );
    assert_eq!(*rt.state.copied_calls.borrow(), 1, "the copied callback runs once");
    assert_eq!(rt.state.actions.borrow().clone(), vec![DdlAction::Copy]);
    let deadline = rt.wake_deadline().expect("copy schedules the feedback wake");
    assert!(
        (deadline - 2.0).abs() <= 0.5,
        "React's copy feedback lasts 2000ms, wake deadline was {deadline}"
    );
}

#[test]
fn copied_label_uses_the_host_deadline_and_an_empty_ddl_never_copies() {
    let mut scene = Scene::loaded(true);
    scene.state.copied_until = Some(10.0);
    let rt = runtime(scene, DESIGN);
    // The snapshot is taken at now=0 in `view`, so the copied label is active.
    let label = element(&rt, "ddl-view-copy").label.unwrap_or_default();
    assert_eq!(label, t("common.copied"));

    let mut empty = Scene::with_state(DdlViewState::default(), true);
    empty.state.copied_until = None;
    let mut rt = runtime(empty, DESIGN);
    click(&mut rt, "ddl-view-copy");
    assert!(rt.state.actions.borrow().is_empty(), "an empty DDL never copies");
}

// --- 3. state machine --------------------------------------------------------

#[test]
fn loading_and_answer_rules_match_react() {
    let mut state = DdlViewState {
        ddl: "old".to_owned(),
        error: "old error".to_owned(),
        ..Default::default()
    };
    state.begin_load();
    assert!(state.loading && state.ddl.is_empty() && state.error.is_empty());

    state.apply_load(Ok(Some("CREATE TABLE t();".to_owned())));
    assert!(!state.loading && state.ddl == "CREATE TABLE t();" && state.error.is_empty());

    state.apply_load(Ok(None));
    assert!(!state.loading && state.ddl.is_empty() && state.error == t("ddl.loadFailed"));

    state.apply_load(Err("raw failure".to_owned()));
    assert_eq!(state.error, "raw failure");

    state.apply_load(Err(String::new()));
    assert_eq!(state.error, t("ddl.loadFailed"));
}

// --- 4. geometry -------------------------------------------------------------

#[test]
fn all_three_branches_fit_both_sizes_and_both_palettes() {
    for (name, state) in [
        ("loading", DdlViewState { loading: true, ..Default::default() }),
        ("error", DdlViewState { error: "x".repeat(400), ..Default::default() }),
        ("loaded", DdlViewState { ddl: "SELECT 'x'.repeat(200);".to_owned(), ..Default::default() }),
    ] {
        for size in [MIN, DESIGN] {
            for dark in [true, false] {
                let rt = runtime(Scene::with_state(state.clone(), dark), size);
                assert_inside(&rt, size, &format!("{name} {size:?} dark={dark}"));
            }
        }
    }
}

#[test]
fn the_object_editor_composes_the_real_ddl_view() {
    use crate::views::database_object_view::{
        build as build_object, ObjectIdentity, ObjectKind, ObjectViewState,
    };

    #[derive(Clone)]
    struct ObjectScene;
    fn object_view(_s: &ObjectScene, _cx: &mut Cx<ObjectScene>) -> El {
        let mut layer = Vec::new();
        build_object::<ObjectScene>(
            &ObjectIdentity {
                db_session_id: "sess".to_owned(),
                database_type: None,
                database: "app".to_owned(),
                kind: ObjectKind::Function,
                name: "f".to_owned(),
                object_schema: None,
                object_signature: None,
                object_target_schema: None,
                object_target_name: None,
            },
            &ObjectViewState { ddl: "SELECT 1;".to_owned(), ..Default::default() },
            None,
            true,
            &mut layer,
        );
        col().w_full().h_full().children(layer)
    }
    let mut rt = Runtime::new(ObjectScene, object_view, DESIGN, Env { system_dark: true, scale: 1.0 });
    rt.frame(0.0);
    assert!(has_key(&rt, "db-object-ddl"), "the composed DDL root keeps the pinned key");
    assert!(has_key(&rt, "db-object-ddl-copy"), "the real DDL view's copy control is composed");
}
