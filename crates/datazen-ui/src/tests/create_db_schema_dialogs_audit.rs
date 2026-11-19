//! T266 independent audit (tester, not coder).
//!
//! Precise probes of branches the shipped 7 tests do not pin: the empty-name
//! guard preserving state, a failed result keeping the name for retry, the
//! schema spec's wrapper words, the confirm-disabled-while-running rule, Enter
//! submission, and a non-vacuous `visible`-rect geometry check. A temporary
//! probe also checks the real dialogs are composed into the T223 slot.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Key, KeyEvent, Mods, Runtime, TextEdit};

use crate::i18n::t;
use crate::views::create_database_dialog::{
    build, AdminCreateAction, AdminCreateSpec, AdminCreateState, CreateDialogKind,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

const DB_SPEC: AdminCreateSpec = AdminCreateSpec {
    key: "content-view-create-database-dialog",
    title_key: "common.createDatabase",
    name_label_key: "createDb.name",
    placeholder: "my_database",
    kind: CreateDialogKind::Database,
};

#[derive(Clone)]
struct AuditScene {
    form: AdminCreateState,
    actions: Rc<RefCell<Vec<AdminCreateAction>>>,
    spec: AdminCreateSpec,
    dark: bool,
}

impl Default for AuditScene {
    fn default() -> Self {
        Self {
            form: AdminCreateState::default(),
            actions: Rc::new(RefCell::new(Vec::new())),
            spec: DB_SPEC,
            dark: true,
        }
    }
}

fn view(s: &AuditScene, _cx: &mut Cx<AuditScene>) -> El {
    let mut layer = Vec::new();
    let actions = s.actions.clone();
    let callback: Rc<dyn Fn(&mut AuditScene, AdminCreateAction)> =
        Rc::new(move |_state: &mut AuditScene, action: AdminCreateAction| {
            actions.borrow_mut().push(action);
        });
    build::<AuditScene>(&s.form, s.spec, |st: &mut AuditScene| &mut st.form, Some(callback), s.dark, &mut layer);
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

#[test]
fn a_blank_name_never_begins_and_preserves_the_existing_error() {
    let mut form = AdminCreateState {
        name: TextEdit::new("   ".to_owned()),
        error: "already exists".to_owned(),
        ..Default::default()
    };
    assert_eq!(form.begin(), None, "whitespace-only names never begin");
    assert_eq!(form.error, "already exists", "a rejected press does not clear the error");
    assert!(!form.running);
}

#[test]
fn a_failed_result_keeps_the_name_for_a_retry() {
    let mut form = AdminCreateState::default();
    form.name = TextEdit::new("shop".to_owned());
    assert_eq!(form.begin(), Some("shop".to_owned()));
    assert!(form.apply_result(Err("already exists".to_owned()), CreateDialogKind::Database).is_empty());
    assert_eq!(form.name.text, "shop", "the name survives a failure so the user can retry");
    assert!(!form.running);
    form.reset();
    assert!(form.name.text.is_empty() && form.error.is_empty(), "reset clears name and error");
}

#[test]
fn the_schema_spec_pins_every_wrapper_word() {
    let spec = crate::views::create_schema_dialog::SPEC;
    assert_eq!(spec.key, "content-view-create-schema-dialog");
    assert_eq!(spec.title_key, "common.createSchema");
    assert_eq!(spec.name_label_key, "createSchema.name");
    assert_eq!(spec.placeholder, "my_schema");
    assert_eq!(spec.kind, CreateDialogKind::Schema);
    assert!(t(spec.title_key) != spec.title_key, "the schema title key resolves");
}

#[test]
fn confirm_is_disabled_while_running_even_with_a_name() {
    let mut scene = AuditScene::default();
    scene.form.name = TextEdit::new("db".to_owned());
    scene.form.running = true;
    let rt = runtime(scene, DESIGN);
    assert!(!element(&rt, "content-view-create-database-dialog-confirm").focusable,
        "an in-flight create disables confirm");
}

#[test]
fn enter_in_the_name_field_submits_the_trimmed_name() {
    let mut scene = AuditScene::default();
    scene.form.name = TextEdit::new("  analytics  ".to_owned());
    let mut rt = runtime(scene, DESIGN);
    let (x, y) = rt.find("content-view-create-database-dialog-name").unwrap_or_else(|| panic!("name field laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.key(KeyEvent { key: Key::Enter, mods: Mods::default() });
    rt.frame(1.0 / 60.0);
    assert_eq!(
        rt.state.actions.borrow().clone(),
        vec![AdminCreateAction::Execute { kind: CreateDialogKind::Database, name: "analytics".to_owned() }],
        "Enter is the same commit boundary as the confirm button"
    );
}

#[test]
fn both_dialogs_stay_inside_both_windows_via_visible_rects() {
    for spec in [
        DB_SPEC,
        crate::views::create_schema_dialog::SPEC,
    ] {
        for size in [MIN, DESIGN] {
            for dark in [true, false] {
                let scene = AuditScene {
                    form: AdminCreateState {
                        name: TextEdit::new("a".repeat(200)),
                        error: "a long failure message that wraps across the dialog body".to_owned(),
                        ..Default::default()
                    },
                    spec,
                    dark,
                    ..AuditScene::default()
                };
                let rt = runtime(scene, size);
                assert!(has_key(&rt, spec.key), "{} {size:?} dark={dark}: root mounted", spec.key);
                for e in rt.elements() {
                    let [x, y, w, h] = e.visible;
                    assert!(
                        x >= -0.5 && y >= -0.5 && x + w <= size.0 + 0.5 && y + h <= size.1 + 0.5,
                        "{} {size:?} dark={dark}: {} visible={:?} rect={:?} crosses window",
                        spec.key,
                        e.key.as_deref().unwrap_or(&e.path),
                        e.visible,
                        e.rect
                    );
                }
            }
        }
    }
}

