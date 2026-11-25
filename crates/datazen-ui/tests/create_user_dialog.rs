//! T286: CreateUserDialog port — two-step keys, validation, host actions and
//! slot-mount geometry at both window sizes and both palettes.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime, TextEdit, Tx};

use datazen_ui::i18n::t;
use datazen_ui::views::create_user_dialog::{
    build, CreateUserAction, CreateUserState, CreateUserStep,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);
const ROOT: &str = "content-view-create-user-dialog";

#[derive(Clone)]
struct Scene {
    form: CreateUserState,
    actions: Rc<RefCell<Vec<CreateUserAction>>>,
    dark: bool,
}

impl Scene {
    fn new(username: &str, password: &str, dark: bool) -> Self {
        Self {
            form: CreateUserState {
                username: TextEdit::new(username.to_owned()),
                password: TextEdit::new(password.to_owned()),
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
    let callback: Rc<dyn Fn(&mut Scene, CreateUserAction)> =
        Rc::new(move |state: &mut Scene, action: CreateUserAction| {
            actions.borrow_mut().push(action);
            let _ = state;
        });
    build::<Scene>(
        &s.form,
        Some("Local · postgres"),
        Some(Rc::new(|state: &mut Scene| &mut state.form)),
        Some(callback),
        s.dark,
        &mut layer,
        None,
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

// --- 1. create step ----------------------------------------------------------

#[test]
fn create_step_pins_username_password_and_footer_keys() {
    let rt = runtime(Scene::new("alice", "", true), DESIGN);
    for suffix in ["", "-title", "-close", "-body", "-context", "-username", "-username-label", "-password", "-password-label", "-cancel", "-confirm"] {
        assert!(has_key(&rt, &format!("{ROOT}{suffix}")), "{suffix} is mounted");
    }
    assert_eq!(text_of(&rt, &format!("{ROOT}-title")), t("common.createUser"));
    assert_eq!(element(&rt, &format!("{ROOT}-confirm")).label.as_deref(), Some(t("common.createUser")));
    assert!(element(&rt, &format!("{ROOT}-confirm")).focusable, "a username enables confirm");
    assert!(!has_key(&rt, &format!("{ROOT}-skip")), "skip belongs to the grant step");
}

#[test]
fn an_empty_username_disables_confirm_and_emits_no_command() {
    let mut rt = runtime(Scene::new("  ", "", true), DESIGN);
    assert!(!element(&rt, &format!("{ROOT}-confirm")).focusable);
    click(&mut rt, &format!("{ROOT}-confirm"));
    assert!(rt.state.actions.borrow().is_empty(), "a blank username never submits");

    let mut form = CreateUserState::default();
    assert_eq!(form.begin_create(), None);
}

#[test]
fn confirm_sends_the_trimmed_username_and_optional_password() {
    let mut rt = runtime(Scene::new("  alice  ", "secret", true), DESIGN);
    click(&mut rt, &format!("{ROOT}-confirm"));
    assert_eq!(
        rt.state.actions.borrow().clone(),
        vec![CreateUserAction::Create {
            username: "alice".to_owned(),
            password: "secret".to_owned(),
        }]
    );
}

#[test]
fn enter_in_the_username_field_is_the_same_commit_boundary() {
    let mut rt = runtime(Scene::new("bob", "", true), DESIGN);
    let (x, y) = rt.find(&format!("{ROOT}-username")).unwrap_or_else(|| panic!("username laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.key(rune::KeyEvent { key: rune::Key::Enter, mods: rune::Mods::default() });
    rt.frame(1.0 / 60.0);
    assert_eq!(
        rt.state.actions.borrow().clone(),
        vec![CreateUserAction::Create { username: "bob".to_owned(), password: String::new() }]
    );
}

// --- 2. two-step state machine ----------------------------------------------

#[test]
fn a_successful_create_advances_to_the_grant_step() {
    let mut form = CreateUserState::default();
    form.username = TextEdit::new("alice".to_owned());
    let action = form.begin_create().expect("valid username");
    assert!(form.running);
    assert!(form.apply_create(Ok(()), "alice").is_empty());
    assert_eq!(form.step, CreateUserStep::Grant);
    assert_eq!(form.created_username, "alice");
    assert!(!form.running);
    assert_eq!(action, CreateUserAction::Create { username: "alice".to_owned(), password: String::new() });
}

#[test]
fn a_failed_create_keeps_the_form_for_a_retry() {
    let mut form = CreateUserState::default();
    form.username = TextEdit::new("alice".to_owned());
    let _ = form.begin_create();
    assert!(form.apply_create(Err("already exists".to_owned()), "alice").is_empty());
    assert_eq!(form.error, "already exists");
    assert_eq!(form.step, CreateUserStep::Create);
    assert_eq!(form.username.text, "alice");
}

#[test]
fn grant_requires_privileges_and_reports_success() {
    let mut form = CreateUserState::default();
    assert_eq!(form.begin_grant(), None, "no privileges -> no grant");
    form.privileges = vec!["SELECT".to_owned(), "INSERT".to_owned()];
    form.created_username = "alice".to_owned();
    form.select_all_privileges();
    assert_eq!(form.selected_privileges, form.privileges);
    form.toggle_privilege("INSERT");
    assert_eq!(form.selected_privileges, vec!["SELECT".to_owned()]);
    let grant = form.begin_grant().expect("one privilege selected");
    assert_eq!(
        grant,
        CreateUserAction::Grant {
            username: "alice".to_owned(),
            database: String::new(),
            privileges: vec!["SELECT".to_owned()],
            grant_option: false,
        }
    );
    assert_eq!(
        form.apply_grant(Ok(())),
        vec![
            CreateUserAction::Created { username: "alice".to_owned() },
            CreateUserAction::Close,
        ]
    );
    assert!(form.selected_privileges.is_empty(), "success resets the form");
}

#[test]
fn skip_grant_reports_the_created_user() {
    let mut form = CreateUserState::default();
    form.created_username = "alice".to_owned();
    assert_eq!(
        form.skip_grant(),
        vec![CreateUserAction::Skip { username: "alice".to_owned() }, CreateUserAction::Close]
    );
}

// --- 3. grant step rendering -------------------------------------------------

#[test]
fn grant_step_shows_success_database_privileges_and_grant_option() {
    let mut scene = Scene::new("alice", "", true);
    scene.form.step = CreateUserStep::Grant;
    scene.form.created_username = "alice".to_owned();
    scene.form.privileges = vec!["SELECT".to_owned(), "INSERT".to_owned()];
    scene.form.selected_privileges = vec!["SELECT".to_owned()];
    scene.form.show_grant_step = true;
    scene.form.show_grant_option = true;
    let rt = runtime(scene, DESIGN);
    assert_eq!(text_of(&rt, &format!("{ROOT}-title")), t("common.grantPrivileges"));
    assert!(text_of(&rt, &format!("{ROOT}-success")).contains("alice"));
    assert!(has_key(&rt, &format!("{ROOT}-database-trigger")), "the Select base keys its trigger");
    assert!(has_key(&rt, &format!("{ROOT}-privileges")));
    assert!(has_key(&rt, &format!("{ROOT}-privilege-0")));
    assert!(has_key(&rt, &format!("{ROOT}-select-all")));
    assert!(has_key(&rt, &format!("{ROOT}-grant-option")));
    assert!(has_key(&rt, &format!("{ROOT}-skip")));
    assert!(element(&rt, &format!("{ROOT}-confirm")).focusable, "a selected privilege enables grant");
}

#[test]
fn grant_confirm_without_privileges_is_disabled() {
    let mut scene = Scene::new("alice", "", true);
    scene.form.step = CreateUserStep::Grant;
    scene.form.created_username = "alice".to_owned();
    let rt = runtime(scene, DESIGN);
    assert!(!element(&rt, &format!("{ROOT}-confirm")).focusable);
    assert!(!has_key(&rt, &format!("{ROOT}-privileges")), "no definition -> no privilege rows");
}

// --- 4. slot mount geometry --------------------------------------------------

#[test]
fn create_step_and_grant_step_fit_both_sizes_and_both_palettes() {
    let mut grant = Scene::new("alice", "secret", true);
    grant.form.step = CreateUserStep::Grant;
    grant.form.created_username = "alice".to_owned();
    grant.form.privileges = vec!["SELECT".to_owned()];
    grant.form.selected_privileges = vec!["SELECT".to_owned()];
    grant.form.show_grant_step = true;

    for (name, scene) in [("create", Scene::new("alice", "secret", true)), ("grant", grant)] {
        for size in [MIN, DESIGN] {
            for dark in [true, false] {
                let rt = runtime(Scene { dark, ..scene.clone() }, size);
                assert!(has_key(&rt, ROOT), "{name} {size:?} dark={dark}: root mounted");
                assert_inside(&rt, size, &format!("{name} {size:?} dark={dark}"));
            }
        }
    }
}

#[test]
fn real_create_user_dialog_is_composed_into_the_content_view_slot() {
    use datazen_ui::views::content_view_dialogs::{
        build as build_slot, DialogAction, DialogConnectionCtx, DialogsVm,
    };

    #[derive(Clone)]
    struct SlotProbe {
        vm: DialogsVm,
        actions: Rc<RefCell<Vec<DialogAction>>>,
    }

    fn slot_view(s: &SlotProbe, _cx: &mut Cx<SlotProbe>) -> El {
        let mut layer = Vec::new();
        let callback: Rc<dyn Fn(&mut SlotProbe, DialogAction)> =
            Rc::new(move |state: &mut SlotProbe, action: DialogAction| {
                state.actions.borrow_mut().push(action);
            });
        build_slot::<SlotProbe>(&s.vm, false, &mut layer, &callback);
        col().w_full().h_full().children(layer)
    }

    let mut vm = DialogsVm {
        conn_ctx: Some(DialogConnectionCtx {
            db_session_id: "sess-1".to_owned(),
            connection_id: "cfg-1".to_owned(),
            connection_name: "Local".to_owned(),
            database_type: "postgres".to_owned(),
        }),
        ..Default::default()
    };
    vm.create_user_open = true;
    let mut rt = Runtime::new(
        SlotProbe { vm, actions: Rc::new(RefCell::new(Vec::new())) },
        slot_view,
        MIN,
        Env { system_dark: false, scale: 1.0 },
    );
    rt.frame(0.0);
    assert!(has_key(&rt, &format!("{ROOT}-username")), "the real dialog's username field mounts in the slot");
    assert!(has_key(&rt, &format!("{ROOT}-confirm")));
    click(&mut rt, &format!("{ROOT}-confirm"));
    assert!(rt.state.actions.borrow().contains(&DialogAction::UserCreated));
}

#[test]
fn React_s_default_database_selection_shows_All_databases() {
    let mut scene = Scene::new("alice", "", true);
    scene.form.step = CreateUserStep::Grant;
    scene.form.created_username = "alice".to_owned();
    scene.form.show_grant_step = true;
    scene.form.databases = vec!["shop".to_owned(), "analytics".to_owned()];
    let mut rt = runtime(scene, DESIGN);
    click(&mut rt, &format!("{ROOT}-database-trigger"));
    assert!(
        has_key(&rt, "option0") && element(&rt, "option0").item_selected,
        "React's default database selection shows All databases"
    );
}

#[test]
fn the_database_select_must_open_its_option_list_like_React() {
    let mut scene = Scene::new("alice", "", true);
    scene.form.step = CreateUserStep::Grant;
    scene.form.created_username = "alice".to_owned();
    scene.form.show_grant_step = true;
    scene.form.databases = vec!["shop".to_owned(), "analytics".to_owned()];
    let mut rt = runtime(scene, DESIGN);

    click(&mut rt, &format!("{ROOT}-database-trigger"));
    assert!(rt.state.form.database_select_open, "the trigger writes the open state");
    assert!(has_key(&rt, "option0"), "the database select must open its option list like React");
    assert!(has_key(&rt, "option2"), "all database options are mounted");

    // Selecting a database writes back and closes the list.
    click(&mut rt, "option1");
    assert_eq!(rt.state.form.target_database, "shop");
    assert!(!rt.state.form.database_select_open, "a choice closes the listbox");
}
