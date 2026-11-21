//! T548: `TableReadOnlyTip` — reason keys/copy, optional dismiss, empty state,
//! and rect geometry at both sizes in both palettes.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use crate::i18n::t;
use crate::views::table_read_only_tip::{
    build, reason_message_key, CellReadOnlyReason, TableReadOnlyTipAction, TableReadOnlyTipVm,
};

fn vm() -> TableReadOnlyTipVm {
    TableReadOnlyTipVm {
        reason: Some(CellReadOnlyReason::View),
        dismissible: true,
    }
}

#[derive(Clone)]
struct Scene {
    vm: TableReadOnlyTipVm,
    actions: Rc<RefCell<Vec<TableReadOnlyTipAction>>>,
    dark: bool,
}

fn scene(vm: TableReadOnlyTipVm, dark: bool) -> Scene {
    Scene { vm, actions: Rc::new(RefCell::new(Vec::new())), dark }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let actions = s.actions.clone();
    let callback = Rc::new(move |state: &mut Scene, action| {
        if action == TableReadOnlyTipAction::Dismiss {
            state.vm.dismissible = false;
        }
        actions.borrow_mut().push(action);
    });
    col().w_full().h_full().child(build(&s.vm, Some(callback), s.dark))
}

fn runtime(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
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
fn every_reason_maps_to_its_exact_i18n_key() {
    let reasons = [
        (CellReadOnlyReason::ConnectionReadOnly, "cellEditor.readOnly.connection"),
        (CellReadOnlyReason::DriverCapability, "cellEditor.readOnly.driverCapability"),
        (CellReadOnlyReason::View, "cellEditor.readOnly.view"),
        (CellReadOnlyReason::NoPrimaryKey, "cellEditor.readOnly.noPrimaryKey"),
        (CellReadOnlyReason::AmbiguousRowIdentity, "cellEditor.readOnly.ambiguousRowIdentity"),
        (CellReadOnlyReason::AutoIncrement, "cellEditor.readOnly.autoIncrement"),
        (CellReadOnlyReason::Generated, "cellEditor.readOnly.generated"),
        (CellReadOnlyReason::BinaryColumn, "cellEditor.readOnly.binaryColumn"),
        (CellReadOnlyReason::LargeValue, "cellEditor.readOnly.largeValue"),
        (CellReadOnlyReason::TypeNotEditable, "cellEditor.readOnly.typeNotEditable"),
    ];
    for (reason, key) in reasons {
        assert_eq!(reason_message_key(reason), key);
        assert!(!t(key).is_empty(), "the key resolves: {key}");
    }
}

#[test]
fn tip_keys_and_copy_are_pinned() {
    let rt = runtime(scene(vm(), true), (1440.0, 816.0));
    for key in ["table-read-only-tip", "table-read-only-reason", "table-read-only-tip-dismiss"] {
        assert!(has_key(&rt, key), "{key} is mounted");
    }
    let line = element(&rt, "table-read-only-tip");
    assert_eq!(line.role, "status");
    assert_eq!(
        element(&rt, "table-read-only-reason").text.as_deref(),
        Some(t("cellEditor.readOnly.view"))
    );
    let texts: Vec<String> = rt.elements().iter().filter_map(|e| e.text.clone()).collect();
    assert!(texts.iter().any(|x| x == t("tableData.readOnlyEditDisabled")), "the base copy is shown");
}

#[test]
fn dismiss_emits_the_host_intent_and_hides_the_affordance() {
    let mut rt = runtime(scene(vm(), true), (1440.0, 816.0));
    click(&mut rt, "table-read-only-tip-dismiss");
    assert_eq!(rt.state.actions.borrow().as_slice(), &[TableReadOnlyTipAction::Dismiss]);
    for _ in 0..3 {
        rt.invalidate();
        rt.frame(1.0 / 60.0);
    }
    assert!(!has_key(&rt, "table-read-only-tip-dismiss"));
}

#[test]
fn missing_reason_mounts_only_the_empty_marker() {
    let rt = runtime(scene(TableReadOnlyTipVm::default(), true), (1440.0, 816.0));
    assert!(has_key(&rt, "table-read-only-tip-empty"));
    assert!(!has_key(&rt, "table-read-only-tip"));
}

#[test]
fn geometry_fits_both_sizes_in_both_themes() {
    for size in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let mut state = vm();
            state.reason = Some(CellReadOnlyReason::AmbiguousRowIdentity);
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

// Reverse verification (mutation -> observed failure text -> restored):
// 1. Mapping `View` to the connection key made `every_reason_maps_to_its_exact_i18n_key`
//    fail on `cellEditor.readOnly.view`.
// 2. Always mounting the dismiss button made `dismiss_emits_the_host_intent_and_hides_the_affordance`
//    fail on `table-read-only-tip-dismiss`.
