//! T266: CreateDatabaseDialog + CreateSchemaDialog ports — keys, validation,
//! host actions, the shared form state machine and two-size/two-palette geometry.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime, TextEdit};

use crate::i18n::t;
use crate::views::create_database_dialog::{
    build, AdminCreateAction, AdminCreateSpec, AdminCreateState, CreateDialogKind,
};
use crate::views::create_schema_dialog::build_schema;

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
struct Scene {
    form: AdminCreateState,
    actions: Rc<RefCell<Vec<AdminCreateAction>>>,
    kind: CreateDialogKind,
    dark: bool,
}

impl Scene {
    fn new(kind: CreateDialogKind, name: &str, dark: bool) -> Self {
        Self {
            form: AdminCreateState {
                name: TextEdit::new(name.to_owned()),
                ..Default::default()
            },
            actions: Rc::new(RefCell::new(Vec::new())),
            kind,
            dark,
        }
    }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let mut layer = Vec::new();
    let actions = s.actions.clone();
    let callback: Rc<dyn Fn(&mut Scene, AdminCreateAction)> =
        Rc::new(move |state: &mut Scene, action: AdminCreateAction| {
            actions.borrow_mut().push(action);
        });
    match s.kind {
        CreateDialogKind::Database => build::<Scene>(
            &s.form,
            DB_SPEC,
            |state: &mut Scene| &mut state.form,
            Some(callback),
            s.dark,
            &mut layer,
        ),
        CreateDialogKind::Schema => build_schema::<Scene>(
            &s.form,
            |state: &mut Scene| &mut state.form,
            Some(callback),
            s.dark,
            &mut layer,
        ),
    }
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

// --- 1. keys and i18n --------------------------------------------------------

#[test]
fn database_dialog_pins_its_slot_keys_and_labels() {
    let rt = runtime(Scene::new(CreateDialogKind::Database, "", true), DESIGN);
    for suffix in ["", "-title", "-close", "-body", "-context", "-name", "-name-label", "-cancel", "-confirm"] {
        let key = format!("content-view-create-database-dialog{suffix}");
        assert!(has_key(&rt, &key), "{key} is mounted");
    }
    assert_eq!(text_of(&rt, "content-view-create-database-dialog-title"), t("common.createDatabase"));
    assert_eq!(text_of(&rt, "content-view-create-database-dialog-name-label"), t("createDb.name"));
    assert_eq!(
        element(&rt, "content-view-create-database-dialog-confirm").label.as_deref(),
        Some(t("common.createDatabase"))
    );
    assert!(!element(&rt, "content-view-create-database-dialog-confirm").focusable, "empty name disables confirm");
}

#[test]
fn schema_dialog_pins_its_own_words_and_keys() {
    let rt = runtime(Scene::new(CreateDialogKind::Schema, "", true), DESIGN);
    assert_eq!(text_of(&rt, "content-view-create-schema-dialog-title"), t("common.createSchema"));
    assert_eq!(text_of(&rt, "content-view-create-schema-dialog-name-label"), t("createSchema.name"));
    assert_eq!(crate::views::create_schema_dialog::SPEC.placeholder, "my_schema");
    assert_eq!(crate::views::create_schema_dialog::SPEC.kind, CreateDialogKind::Schema);
}

// --- 2. validation and host actions -----------------------------------------

#[test]
fn confirm_sends_the_trimmed_name_only_when_valid() {
    let mut rt = runtime(Scene::new(CreateDialogKind::Database, "  analytics  ", true), DESIGN);
    click(&mut rt, "content-view-create-database-dialog-confirm");
    let actions = rt.state.actions.borrow().clone();
    assert_eq!(
        actions,
        vec![AdminCreateAction::Execute {
            kind: CreateDialogKind::Database,
            name: "analytics".to_owned(),
        }],
        "the host gets the trimmed name, no IPC here"
    );

    let mut rt = runtime(Scene::new(CreateDialogKind::Schema, "  ", true), DESIGN);
    click(&mut rt, "content-view-create-schema-dialog-confirm");
    assert!(rt.state.actions.borrow().is_empty(), "a blank name never submits");
}

#[test]
fn cancel_and_header_close_emit_close() {
    let mut rt = runtime(Scene::new(CreateDialogKind::Database, "db", true), DESIGN);
    click(&mut rt, "content-view-create-database-dialog-cancel");
    assert_eq!(rt.state.actions.borrow().clone(), vec![AdminCreateAction::Close]);
    let mut rt = runtime(Scene::new(CreateDialogKind::Schema, "s", true), DESIGN);
    click(&mut rt, "content-view-create-schema-dialog-close");
    assert_eq!(rt.state.actions.borrow().clone(), vec![AdminCreateAction::Close]);
}

#[test]
fn the_form_state_machine_guards_and_reports_results() {
    let mut form = AdminCreateState::default();
    assert_eq!(form.begin(), None, "empty name is rejected");
    form.name = TextEdit::new("  shop  ".to_owned());
    assert_eq!(form.begin(), Some("shop".to_owned()));
    assert!(form.running);

    let actions = form.apply_result(Ok(()), CreateDialogKind::Database);
    assert_eq!(
        actions,
        vec![
            AdminCreateAction::Created { kind: CreateDialogKind::Database, name: "shop".to_owned() },
            AdminCreateAction::Close,
        ]
    );
    assert!(form.name.text.is_empty(), "success resets the form");

    form.name = TextEdit::new("s".to_owned());
    let _ = form.begin();
    let actions = form.apply_result(Err("already exists".to_owned()), CreateDialogKind::Schema);
    assert!(actions.is_empty(), "a failure only shows the error");
    assert_eq!(form.error, "already exists");
    assert!(!form.running);
}

#[test]
fn an_error_renders_under_its_key() {
    let scene = Scene {
        form: AdminCreateState {
            name: TextEdit::new("db".to_owned()),
            error: "already exists".to_owned(),
            ..Default::default()
        },
        ..Scene::new(CreateDialogKind::Database, "", true)
    };
    let rt = runtime(scene, DESIGN);
    assert!(has_key(&rt, "content-view-create-database-dialog-error"));
}

// --- 3. geometry -------------------------------------------------------------

#[test]
fn both_dialogs_fit_both_window_sizes_and_both_palettes() {
    for kind in [CreateDialogKind::Database, CreateDialogKind::Schema] {
        let prefix = match kind {
            CreateDialogKind::Database => "content-view-create-database-dialog",
            CreateDialogKind::Schema => "content-view-create-schema-dialog",
        };
        for size in [MIN, DESIGN] {
            for dark in [true, false] {
                let scene = Scene {
                    form: AdminCreateState {
                        name: TextEdit::new("a".repeat(120)),
                        error: "a long failure message that wraps in the narrow dialog body".to_owned(),
                        ..Default::default()
                    },
                    ..Scene::new(kind, "", dark)
                };
                let rt = runtime(scene, size);
                assert!(has_key(&rt, prefix), "{kind:?} {size:?} dark={dark}: root mounted");
                assert_inside(&rt, size, &format!("{kind:?} {size:?} dark={dark}"));
            }
        }
    }
}
