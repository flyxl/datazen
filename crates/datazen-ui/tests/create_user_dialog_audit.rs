//! T286 independent audit (tester, not coder).
//!
//! Precise probes of branches the shipped 12 tests do not pin: select-all
//! toggling off, a failed grant keeping the form, success-text substitution,
//! reset preserving host facts, guards clearing a prior error, the no-grant-step
//! branch, and non-vacuous `visible`-rect geometry. Temporary probes check the
//! grant step's database Select default and openability.

use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime, TextEdit};

use datazen_ui::i18n::t;
use datazen_ui::views::create_user_dialog::{
    build, CreateUserAction, CreateUserState, CreateUserStep,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);
const ROOT: &str = "content-view-create-user-dialog";

#[derive(Clone)]
struct AuditScene {
    form: CreateUserState,
    dark: bool,
}

impl Default for AuditScene {
    fn default() -> Self {
        Self { form: CreateUserState::default(), dark: true }
    }
}

fn view(s: &AuditScene, _cx: &mut Cx<AuditScene>) -> El {
    let mut layer = Vec::new();
    let actions: Rc<dyn Fn(&mut AuditScene, CreateUserAction)> = Rc::new(|_s, _a| {});
    build::<AuditScene>(
        &s.form,
        Some("Local · postgres"),
        Some(Rc::new(|st: &mut AuditScene| &mut st.form)),
        Some(actions),
        s.dark,
        &mut layer,
        None,
    );
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

fn grant_scene() -> AuditScene {
    let mut scene = AuditScene::default();
    scene.form.step = CreateUserStep::Grant;
    scene.form.created_username = "alice".to_owned();
    scene.form.show_grant_step = true;
    scene.form.privileges = vec!["SELECT".to_owned(), "INSERT".to_owned()];
    scene.form.selected_privileges = vec!["SELECT".to_owned()];
    scene
}

#[test]
fn select_all_toggles_back_to_empty_when_everything_is_selected() {
    let mut form = CreateUserState::default();
    form.privileges = vec!["SELECT".to_owned(), "INSERT".to_owned()];
    form.select_all_privileges();
    assert_eq!(form.selected_privileges, form.privileges);
    form.select_all_privileges();
    assert!(form.selected_privileges.is_empty(), "a second select-all deselects everything");
}

#[test]
fn a_failed_grant_keeps_the_form_for_a_retry() {
    let mut form = CreateUserState::default();
    form.created_username = "alice".to_owned();
    form.target_database = "db1".to_owned();
    form.privileges = vec!["SELECT".to_owned()];
    form.selected_privileges = vec!["SELECT".to_owned()];
    let _ = form.begin_grant();
    assert!(form.apply_grant(Err("permission denied".to_owned())).is_empty());
    assert_eq!(form.error, "permission denied");
    assert_eq!(form.created_username, "alice", "the created user survives for a retry");
    assert_eq!(form.target_database, "db1");
    assert!(!form.running);
}

#[test]
fn success_text_substitutes_the_created_username() {
    let mut form = CreateUserState::default();
    form.created_username = "alice".to_owned();
    assert_eq!(form.success_text(), t("createUser.success").replace("{name}", "alice"));
}

#[test]
fn reset_preserves_host_supplied_facts_but_clears_the_form() {
    let mut form = CreateUserState::default();
    form.username = TextEdit::new("u".to_owned());
    form.password = TextEdit::new("p".to_owned());
    form.error = "boom".to_owned();
    form.privileges = vec!["SELECT".to_owned()];
    form.databases = vec!["db1".to_owned()];
    form.show_grant_option = true;
    form.show_grant_step = true;
    form.reset();
    assert!(form.username.text.is_empty() && form.password.text.is_empty() && form.error.is_empty());
    assert_eq!(form.privileges, vec!["SELECT".to_owned()]);
    assert_eq!(form.databases, vec!["db1".to_owned()]);
    assert!(form.show_grant_option && form.show_grant_step);
}

#[test]
fn guards_clear_a_prior_error_before_retrying() {
    let mut form = CreateUserState::default();
    form.username = TextEdit::new("u".to_owned());
    form.error = "stale".to_owned();
    assert!(form.begin_create().is_some());
    assert!(form.error.is_empty(), "create clears the stale error");
    form.error = "stale".to_owned();
    form.privileges = vec!["SELECT".to_owned()];
    form.selected_privileges = vec!["SELECT".to_owned()];
    assert!(form.begin_grant().is_some());
    assert!(form.error.is_empty(), "grant clears the stale error");
}

#[test]
fn a_grant_step_without_a_grant_schema_shows_only_the_success_banner() {
    let mut scene = grant_scene();
    scene.form.show_grant_step = false;
    let rt = runtime(scene, DESIGN);
    assert!(has_key(&rt, &format!("{ROOT}-success")));
    assert!(!has_key(&rt, &format!("{ROOT}-database")), "no database select without a grant schema");
    assert!(!has_key(&rt, &format!("{ROOT}-privileges")));
}

#[test]
fn both_steps_stay_inside_both_windows_via_visible_rects() {
    for step in [CreateUserStep::Create, CreateUserStep::Grant] {
        for size in [MIN, DESIGN] {
            for dark in [true, false] {
                let mut scene = grant_scene();
                scene.form.step = step;
                scene.form.error = "a long failure message that wraps across the dialog body".to_owned();
                scene.dark = dark;
                let rt = runtime(scene, size);
                assert!(has_key(&rt, ROOT), "{step:?} {size:?} dark={dark}: root mounted");
                for e in rt.elements() {
                    let [x, y, w, h] = e.visible;
                    assert!(
                        x >= -0.5 && y >= -0.5 && x + w <= size.0 + 0.5 && y + h <= size.1 + 0.5,
                        "{step:?} {size:?} dark={dark}: {} visible={:?} rect={:?} crosses window",
                        e.key.as_deref().unwrap_or(&e.path), e.visible, e.rect
                    );
                }
            }
        }
    }
}

