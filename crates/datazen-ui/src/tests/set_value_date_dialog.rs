//! T530: `SetValueDateDialog` — keys, canonical draft, apply/cancel host
//! actions and rect geometry at both sizes in both palettes.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, row, Cx, El, Env, TextEdit, Runtime};

use crate::i18n::t;
use crate::views::set_value_date_dialog::{
    build, PickerKind, SetValueDateAction, SetValueDateDialogVm,
};

fn vm() -> SetValueDateDialogVm {
    SetValueDateDialogVm {
        open: true,
        column_name: "created_at".to_owned(),
        column_type: Some("date".to_owned()),
        kind: PickerKind::Date,
        draft: TextEdit::new("2026-01-01".to_owned()),
    }
}

#[derive(Clone)]
struct Scene {
    vm: SetValueDateDialogVm,
    actions: Rc<RefCell<Vec<SetValueDateAction>>>,
    dark: bool,
}

fn scene(vm: SetValueDateDialogVm, dark: bool) -> Scene {
    Scene { vm, actions: Rc::new(RefCell::new(Vec::new())), dark }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let actions = s.actions.clone();
    let callback = Rc::new(move |state: &mut Scene, action| {
        if let SetValueDateAction::Apply(value) = &action {
            state.vm.draft.replace_text(value.clone());
        }
        actions.borrow_mut().push(action);
    });
    let edit: Rc<dyn for<'b> Fn(&'b mut Scene) -> &'b mut TextEdit> =
        Rc::new(|state| &mut state.vm.draft);
    let mut layer = Vec::new();
    build(&s.vm, Some(edit), Some(callback), s.dark, &mut layer);
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

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

#[test]
fn closed_dialog_mounts_nothing() {
    let mut state = vm();
    state.open = false;
    let rt = runtime(scene(state, true), (1440.0, 816.0));
    assert!(!has_key(&rt, "set-value-date-dialog"));
    assert!(!has_key(&rt, "set-value-date-picker"));
}

#[test]
fn dialog_names_column_and_shows_picker_intent() {
    let rt = runtime(scene(vm(), true), (1440.0, 816.0));
    for key in [
        "set-value-date-dialog",
        "set-value-date-dialog-title",
        "set-value-date-picker",
        "set-value-date-picker-input",
        "set-value-date-cancel",
        "set-value-date-apply",
    ] {
        assert!(has_key(&rt, key), "{key} is mounted");
    }
    assert_eq!(element(&rt, "set-value-date-dialog-title").text.as_deref(), Some("created_at"));
    assert_eq!(element(&rt, "set-value-date-apply").label.as_deref(), Some(t("common.confirm")));
    assert_eq!(element(&rt, "set-value-date-cancel").label.as_deref(), Some(t("common.cancel")));
    assert_eq!(
        vm().description(),
        format!("{} · date", t("dataTable.pickDate")),
        "the column type is shown in the description"
    );
}

#[test]
fn applying_sends_the_edited_canonical_value() {
    let mut rt = runtime(scene(vm(), true), (1440.0, 816.0));
    rt.state.vm.draft = TextEdit::new(String::new());
    rt.invalidate();
    rt.frame(1.0 / 60.0);
    let (x, y) = rt.find("set-value-date-picker-input").unwrap_or_else(|| panic!("picker input"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.paste("2026-03-08");
    rt.frame(0.0);
    click(&mut rt, "set-value-date-apply");
    assert_eq!(
        rt.state.actions.borrow().first(),
        Some(&SetValueDateAction::Apply("2026-03-08".to_owned())),
        "apply chooses the edited value without an IPC command"
    );
}

#[test]
fn cancel_only_closes() {
    let mut rt = runtime(scene(vm(), true), (1440.0, 816.0));
    click(&mut rt, "set-value-date-cancel");
    assert_eq!(rt.state.actions.borrow().as_slice(), &[SetValueDateAction::Close]);
}

#[test]
fn kind_and_clear_value_are_ported() {
    let mut state = vm();
    state.kind = PickerKind::from_str("datetime");
    state.column_type = Some("timestamp".to_owned());
    state.draft = TextEdit::new(String::new());
    assert_eq!(state.kind, PickerKind::DateTime);
    assert_eq!(state.description(), format!("{} · timestamp", t("dataTable.pickDate")));
    let mut rt = runtime(scene(state, true), (1440.0, 816.0));
    click(&mut rt, "set-value-date-apply");
    assert_eq!(
        rt.state.actions.borrow().first(),
        Some(&SetValueDateAction::Apply(String::new())),
        "a cleared picker applies SQL NULL as an empty canonical string"
    );
}

#[test]
fn geometry_fits_both_sizes_in_both_themes() {
    for size in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            for state in [vm(), {
                let mut state = vm();
                state.kind = PickerKind::DateTime;
                state.column_type = None;
                state
            }] {
                let rt = runtime(scene(state, dark), size);
                let outside: Vec<String> = rt.elements().iter()
                    .filter(|e| {
                        let r = e.rect;
                        r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > size.0 + 0.5 || r[1] + r[3] > size.1 + 0.5
                    })
                    .map(|e| format!("{} {:?}", e.key.as_deref().unwrap_or(&e.path), e.rect))
                    .collect();
                assert!(outside.is_empty(), "{size:?} dark={dark}: outside {outside:?}");
            }
        }
    }
}

#[test]
fn empty_type_and_case_insensitive_kind_are_ported() {
    let mut state = vm();
    state.column_type = Some(String::new());
    assert_eq!(state.description(), t("dataTable.pickDate"));
    assert_eq!(PickerKind::from_str("date"), PickerKind::Date);
    assert_eq!(PickerKind::from_str("DATE"), PickerKind::Date);
    assert_eq!(PickerKind::from_str("DaTeTiMe"), PickerKind::DateTime);
    assert_eq!(PickerKind::from_str("other"), PickerKind::Date);
}

#[test]
fn confirm_applies_then_closes_in_order() {
    let mut rt = runtime(scene(vm(), true), (1440.0, 816.0));
    click(&mut rt, "set-value-date-apply");
    assert_eq!(
        rt.state.actions.borrow().as_slice(),
        &[SetValueDateAction::Apply("2026-01-01".to_owned()), SetValueDateAction::Close],
        "React confirm calls onClose after onApply"
    );
}
