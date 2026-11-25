//! T305: DatabaseObjectView port — object detail branches, host actions and
//! two-size/two-palette geometry (including the connection-page slot).

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use datazen_ui::i18n::t;
use datazen_ui::views::database_object_view::{
    build, ObjectAction, ObjectEvent, ObjectIdentity, ObjectKind, ObjectViewState,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

#[derive(Clone)]
struct Scene {
    identity: ObjectIdentity,
    state: ObjectViewState,
    actions: Rc<RefCell<Vec<ObjectAction>>>,
    dark: bool,
}

impl Scene {
    fn new(kind: ObjectKind, schema: Option<&str>) -> Self {
        Self {
            identity: ObjectIdentity {
                db_session_id: "sess-1".to_owned(),
                database_type: Some("postgresql".to_owned()),
                database: "app".to_owned(),
                kind,
                name: "users_by_status".to_owned(),
                object_schema: schema.map(str::to_owned),
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

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let mut layer = Vec::new();
    let actions = s.actions.clone();
    let callback: Rc<dyn Fn(&mut Scene, ObjectAction)> =
        Rc::new(move |state: &mut Scene, action: ObjectAction| {
            actions.borrow_mut().push(action);
            let _ = state;
        });
    build::<Scene>(&s.identity, &s.state, Some(callback), s.dark, &mut layer);
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

// --- 1. keys and object detail ----------------------------------------------

#[test]
fn header_pins_kind_label_and_object_name_with_both_actions() {
    let mut scene = Scene::new(ObjectKind::Function, Some("public"));
    scene.state.loading = true;
    let rt = runtime(scene, DESIGN);
    for key in ["db-object-view", "db-object-header", "db-object-title", "db-object-refresh", "db-object-execute", "db-object-loading"] {
        assert!(has_key(&rt, key), "{key} is mounted");
    }
    assert_eq!(text_of(&rt, "db-object-title"), format!("{} · public.users_by_status", t("objects.function")));
    assert!(!has_key(&rt, "db-object-ddl"), "the loading state replaces the editor");
    assert!(!element(&rt, "db-object-execute").focusable, "a loading view cannot execute");
}

#[test]
fn procedure_trigger_and_untranslated_kinds_are_labelled() {
    for (kind, expected) in [
        (ObjectKind::Procedure, t("objects.procedure")),
        (ObjectKind::Trigger, t("objects.trigger")),
        (ObjectKind::Sequence, "sequence"),
        (ObjectKind::Type, "type"),
    ] {
        let mut scene = Scene::new(kind, None);
        scene.state.loading = false;
        scene.state.ddl = "SELECT 1;".to_owned();
        let rt = runtime(scene, DESIGN);
        assert_eq!(text_of(&rt, "db-object-title"), format!("{expected} · users_by_status"));
    }
}

// --- 2. host actions and state machine --------------------------------------

#[test]
fn refresh_and_execute_route_to_the_host_port() {
    let mut scene = Scene::new(ObjectKind::Function, Some("public"));
    scene.state.loading = false;
    scene.state.ddl = "SELECT 1;".to_owned();
    let mut rt = runtime(scene, DESIGN);
    click(&mut rt, "db-object-refresh");
    click(&mut rt, "db-object-execute");
    let actions = rt.state.actions.borrow().clone();
    assert_eq!(
        actions,
        vec![
            ObjectAction::LoadDdl,
            ObjectAction::Execute {
                sql: "SELECT 1;".to_owned(),
                database: "app".to_owned(),
                schema: Some("public".to_owned()),
            },
        ]
    );
}

#[test]
fn ddl_failure_is_shown_as_a_sql_comment_and_execute_is_guarded() {
    let mut state = ObjectViewState::default();
    state.begin_load();
    assert!(state.loading && state.message.is_empty());
    state.apply_ddl(Err("permission denied".to_owned()));
    assert!(!state.loading);
    assert_eq!(state.ddl, "-- permission denied");
    // A comment is non-empty, so React's `!ddl.trim()` guard lets it through.
    assert!(state.begin_execute(), "the fallback comment still passes React's non-empty guard");
    assert!(state.running);
    state.apply_execute(Ok(()));

    state.apply_ddl(Ok("SELECT 1;".to_owned()));
    assert!(state.begin_execute());
    state.apply_execute(Ok(()));
    assert_eq!(state.message, t("objects.executeOk"));
    assert!(!state.running);
    state.begin_execute();
    state.apply_execute(Err("syntax error".to_owned()));
    assert_eq!(state.message, "syntax error");

    let mut events = ObjectViewState::default();
    events.apply_ddl(Ok("SELECT 2;".to_owned()));
    assert_eq!(events.ddl, "SELECT 2;");
    assert!(matches!(ObjectEvent::DdlLoaded(Ok(String::new())), ObjectEvent::DdlLoaded(_)));
}

#[test]
fn an_empty_or_loading_ddl_disables_execute() {
    let mut scene = Scene::new(ObjectKind::Function, None);
    scene.state.loading = false;
    let rt = runtime(scene, DESIGN);
    assert!(!element(&rt, "db-object-execute").focusable);
    assert!(!has_key(&rt, "db-object-message"));
}

#[test]
fn a_message_footer_renders_only_when_present() {
    let mut scene = Scene::new(ObjectKind::Trigger, None);
    scene.state.loading = false;
    scene.state.ddl = "CREATE TRIGGER ...;".to_owned();
    scene.state.message = t("objects.executeOk").to_owned();
    let rt = runtime(scene, DESIGN);
    assert!(has_key(&rt, "db-object-ddl"));
    assert_eq!(text_of(&rt, "db-object-message-text"), t("objects.executeOk"));
}

// --- 3. geometry -------------------------------------------------------------

#[test]
fn loading_editor_and_message_states_fit_both_sizes_and_both_palettes() {
    let mut loaded = Scene::new(ObjectKind::Function, Some("public"));
    loaded.state.loading = false;
    loaded.state.ddl = "SELECT 'a long line of SQL'.repeat(20);".to_owned();
    let mut message = Scene::new(ObjectKind::Procedure, None);
    message.state.loading = false;
    message.state.message = t("objects.executeOk").to_owned();

    for (name, scene) in [("loading", Scene::new(ObjectKind::Function, None)), ("editor", loaded), ("message", message)] {
        for size in [MIN, DESIGN] {
            for dark in [true, false] {
                let rt = runtime(Scene { dark, ..scene.clone() }, size);
                assert!(has_key(&rt, "db-object-view"), "{name} {size:?} dark={dark}: root mounted");
                assert_inside(&rt, size, &format!("{name} {size:?} dark={dark}"));
            }
        }
    }
}

#[test]
fn the_connection_page_keeps_the_object_slot() {
    use datazen_ui::state_def::AppState;
    use rune::{tok, Cx};

    fn slot_view(state: &AppState, _cx: &mut Cx<AppState>) -> El {
        datazen_ui::views::connection_page(state, _cx)
    }
    let mut rt = Runtime::new(
        AppState::default(),
        slot_view,
        MIN,
        Env { system_dark: true, scale: 1.0 },
    );
    rt.frame(0.0);
    assert!(has_key(&rt, "db-object-host"), "the object view composition slot is stable");
}

#[test]
fn connection_page_must_compose_the_real_DatabaseObjectView_not_just_a_zero_sized_slot() {
    use std::cell::RefCell;
    use datazen_ui::views::connection_content_with_vm;
    use datazen_ui::views::content_view::ContentViewVm;
    use datazen_ui::views::database_object_view::DatabaseObjectVm;

    thread_local! {
        static OBJECT_VM: RefCell<Option<ContentViewVm>> = RefCell::new(None);
    }

    fn composed_view(state: &datazen_ui::state_def::AppState, _cx: &mut Cx<datazen_ui::state_def::AppState>) -> El {
        let vm = OBJECT_VM.with(|f| f.borrow().clone()).unwrap_or_default();
        connection_content_with_vm(state, rune::tok::theme(true), true, &vm, &mut Vec::new())
    }

    let identity = ObjectIdentity {
        db_session_id: "sess-1".to_owned(),
        database_type: Some("postgresql".to_owned()),
        database: "app".to_owned(),
        kind: ObjectKind::Function,
        name: "users_by_status".to_owned(),
        object_schema: Some("public".to_owned()),
        object_signature: None,
        object_target_schema: None,
        object_target_name: None,
    };
    let vm = ContentViewVm {
        object_vm: Some(DatabaseObjectVm {
            identity,
            state: ObjectViewState {
                ddl: "SELECT 1;".to_owned(),
                ..Default::default()
            },
        }),
        ..Default::default()
    };
    OBJECT_VM.with(|f| *f.borrow_mut() = Some(vm));
    let mut rt = Runtime::new(
        datazen_ui::state_def::AppState::default(),
        composed_view,
        DESIGN,
        Env { system_dark: true, scale: 1.0 },
    );
    rt.frame(0.0);
    assert!(has_key(&rt, "db-object-host"), "the slot key is preserved");
    assert!(has_key(&rt, "db-object-view"), "connection_page must compose the real DatabaseObjectView, not just a zero-sized slot");
    assert!(has_key(&rt, "db-object-ddl"), "the composed view shows the object DDL");
}

#[test]
fn long_message_text_wraps_inside_both_windows_and_palettes() {
    let mut scene = Scene::new(ObjectKind::Function, None);
    scene.state.loading = false;
    scene.state.message = "x".repeat(320);
    for size in [MIN, DESIGN] {
        for dark in [true, false] {
            let rt = runtime(Scene { dark, ..scene.clone() }, size);
            let rect = element(&rt, "db-object-message-text").rect;
            assert!(
                rect[0] >= -0.5 && rect[2] <= size.0 + 0.5,
                "db-object-message-text {:?} crosses window {:?} dark={dark}",
                rect,
                size
            );
            assert_inside(&rt, size, &format!("message wrap {size:?} dark={dark}"));
        }
    }
}
