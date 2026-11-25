//! T305 independent audit (tester, not coder).
//!
//! Precise probes of branches the shipped 8 tests do not pin: load clearing a
//! prior message, DDL success replacing an error comment, the in-flight execute
//! guard, the three-way disabled predicate, an empty schema string falling back
//! to the bare name, a null schema in the execute action, and non-vacuous
//! `visible`-rect geometry. A temporary probe checks real composition into
//! `connection_page.rs`.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use datazen_ui::i18n::t;
use datazen_ui::views::database_object_view::{
    build, ObjectAction, ObjectIdentity, ObjectKind, ObjectViewState,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

#[derive(Clone)]
struct AuditScene {
    identity: ObjectIdentity,
    state: ObjectViewState,
    actions: Rc<RefCell<Vec<ObjectAction>>>,
    dark: bool,
}

impl Default for AuditScene {
    fn default() -> Self {
        Self {
            identity: ObjectIdentity {
                db_session_id: "sess".to_owned(),
                database_type: Some("postgresql".to_owned()),
                database: "app".to_owned(),
                kind: ObjectKind::Function,
                name: "users_by_status".to_owned(),
                object_schema: Some("public".to_owned()),
                object_signature: None,
                object_target_schema: None,
                object_target_name: None,
            },
            state: ObjectViewState::default(),
            actions: Rc::new(RefCell::new(Vec::new())),
            dark: true,
        }
    }
}

fn view(s: &AuditScene, _cx: &mut Cx<AuditScene>) -> El {
    let mut layer = Vec::new();
    let actions = s.actions.clone();
    let cb: Rc<dyn Fn(&mut AuditScene, ObjectAction)> =
        Rc::new(move |_s: &mut AuditScene, a: ObjectAction| actions.borrow_mut().push(a));
    build::<AuditScene>(&s.identity, &s.state, Some(cb), s.dark, &mut layer);
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

#[test]
fn begin_load_clears_a_prior_message() {
    let mut state = ObjectViewState::default();
    state.message = "stale".to_owned();
    state.begin_load();
    assert!(state.loading);
    assert!(state.message.is_empty(), "a reload clears the previous execute message");
}

#[test]
fn a_successful_ddl_load_replaces_an_error_comment() {
    let mut state = ObjectViewState::default();
    state.apply_ddl(Err("permission denied".to_owned()));
    assert_eq!(state.ddl, "-- permission denied");
    state.apply_ddl(Ok("SELECT 1;".to_owned()));
    assert_eq!(state.ddl, "SELECT 1;", "a later success replaces the fallback comment");
    assert!(!state.loading);
}

#[test]
fn an_in_flight_execute_is_guarded() {
    let mut state = ObjectViewState::default();
    state.ddl = "SELECT 1;".to_owned();
    assert!(state.begin_execute());
    assert!(!state.begin_execute(), "a second press while running must be ignored");
}

#[test]
fn execute_disabled_reflects_all_three_react_guards() {
    let mut state = ObjectViewState::default();
    assert!(state.execute_disabled(), "empty DDL disables");
    state.ddl = "SELECT 1;".to_owned();
    assert!(!state.execute_disabled());
    state.loading = true;
    assert!(state.execute_disabled(), "loading disables");
    state.loading = false;
    state.running = true;
    assert!(state.execute_disabled(), "running disables");
}

#[test]
fn an_empty_schema_string_falls_back_to_the_bare_object_name() {
    let mut scene = AuditScene::default();
    scene.identity.object_schema = Some(String::new());
    scene.state.loading = true;
    let rt = runtime(scene, DESIGN);
    assert_eq!(
        text_of(&rt, "db-object-title"),
        format!("{} · users_by_status", t("objects.function")),
        "an empty schema is falsy in React, so the title omits the dot prefix"
    );
}

#[test]
fn execute_action_carries_none_schema_when_absent() {
    let mut scene = AuditScene::default();
    scene.identity.object_schema = None;
    scene.state.ddl = "SELECT 1;".to_owned();
    let mut rt = runtime(scene, DESIGN);
    click(&mut rt, "db-object-execute");
    assert_eq!(
        rt.state.actions.borrow().clone(),
        vec![ObjectAction::Execute {
            sql: "SELECT 1;".to_owned(),
            database: "app".to_owned(),
            schema: None,
        }],
        "the action carries the null schema React passes to executeQuery"
    );
}

#[test]
fn long_ddl_and_message_stay_inside_via_visible_rects() {
    let long = "SELECT ".repeat(60);
    for size in [MIN, DESIGN] {
        for dark in [true, false] {
            let mut scene = AuditScene::default();
            scene.state.loading = false;
            scene.state.ddl = long.clone();
            scene.state.message = t("objects.executeOk").to_owned();
            scene.dark = dark;
            let rt = runtime(scene, size);
            assert!(has_key(&rt, "db-object-view"), "{size:?} dark={dark}: root mounted");
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

