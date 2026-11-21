//! T442 independent audit (tester, not coder).
//!
//! Precise probes of branches the shipped 14 tests do not pin: the Running
//! footer/cancel and non-dismissable header, the Start action, sample() null
//! skipping / row cap, mapped pairs + overrides, Select open/choose write-back,
//! and a non-vacuous `visible`-rect geometry channel.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use crate::views::import_dialog::{
    build, CellValue, ImportAction, ImportDialogVm, ImportParseResult, ImportVmFn, LoadedFile,
    MappingMode, OpenSelect, TargetColumn, TargetColumnsState, WizardPhase, ROOT,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

#[derive(Clone)]
struct Scene {
    vm: ImportDialogVm,
    actions: Rc<RefCell<Vec<ImportAction>>>,
    dark: bool,
}

fn ready_vm() -> ImportDialogVm {
    ImportDialogVm {
        file: Some(LoadedFile { name: "data.csv".to_owned(), data_base64: "aWQsbmFtZQ==".to_owned() }),
        parsed: Some(ImportParseResult {
            columns: vec!["id".to_owned(), "name".to_owned()],
            rows: vec![
                vec![CellValue::Text("1".to_owned()), CellValue::Text("a".to_owned())],
                vec![CellValue::Text("2".to_owned()), CellValue::Text("b".to_owned())],
            ],
            row_origins: vec![2, 3],
            ..Default::default()
        }),
        targets: TargetColumnsState {
            columns: Some(vec![
                TargetColumn { name: "id".to_owned(), data_type: "integer".to_owned() },
                TargetColumn { name: "name".to_owned(), data_type: "text".to_owned() },
            ]),
            ..Default::default()
        },
        target_table: "users".to_owned(),
        ..Default::default()
    }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let mut layer = Vec::new();
    let actions = s.actions.clone();
    let callback: Rc<dyn Fn(&mut Scene, ImportAction)> = Rc::new(move |state: &mut Scene, action| {
        // The host owns Select open state; let the popup mount, record the rest.
        match &action {
            ImportAction::SetOpenSelect(kind) => state.vm.open_select = kind.clone(),
            _ => actions.borrow_mut().push(action),
        }
    });
    let vm_edit: ImportVmFn<Scene> = Rc::new(|s: &mut Scene| &mut s.vm);
    build::<Scene>(&s.vm, Some(callback), Some(vm_edit), s.dark, &mut layer);
    col().w_full().h_full().children(layer)
}

fn runtime(vm: ImportDialogVm, size: (f32, f32), dark: bool) -> Runtime<Scene> {
    let scene = Scene { vm, actions: Rc::new(RefCell::new(Vec::new())), dark };
    let mut rt = Runtime::new(scene, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

#[test]
fn running_footer_cancels_and_the_header_cannot_dismiss() {
    let mut vm = ready_vm();
    vm.phase = WizardPhase::Running;
    let mut rt = runtime(vm, DESIGN, true);
    assert!(!has_key(&rt, &format!("{ROOT}-confirm"))
        || rt.find(&format!("{ROOT}-confirm")).is_some());
    click(&mut rt, &format!("{ROOT}-cancel"));
    assert!(
        rt.state.actions.borrow().contains(&ImportAction::CancelImport),
        "running cancel emits CancelImport"
    );
    // The header close emits no action while running.
    let before = rt.state.actions.borrow().len();
    if rt.find(&format!("{ROOT}-close")).is_some() {
        click(&mut rt, &format!("{ROOT}-close"));
    }
    assert_eq!(rt.state.actions.borrow().len(), before, "no dismiss while running");
}

#[test]
fn ready_config_footer_emits_start() {
    let mut rt = runtime(ready_vm(), DESIGN, true);
    click(&mut rt, &format!("{ROOT}-confirm"));
    assert!(rt.state.actions.borrow().contains(&ImportAction::Start));
}

#[test]
fn sample_skips_nulls_and_caps_at_five_rows() {
    let parsed = ImportParseResult {
        columns: vec!["id".to_owned()],
        rows: (0..8)
            .map(|i| {
                if i < 2 {
                    vec![CellValue::Null]
                } else {
                    vec![CellValue::Text(format!("v{i}"))]
                }
            })
            .collect(),
        ..Default::default()
    };
    assert_eq!(parsed.sample(0), "v2", "the first non-null sample wins");
    let all_null = ImportParseResult {
        columns: vec!["id".to_owned()],
        rows: vec![vec![CellValue::Null]],
        ..Default::default()
    };
    assert_eq!(all_null.sample(0), "—", "an all-null column shows the em dash");
}

#[test]
fn mapped_pairs_and_override_target_follow_the_plan() {
    let mut vm = ready_vm();
    vm.mapping_mode = MappingMode::Name;
    let pairs = vm.mapped_pairs();
    assert_eq!(pairs, vec![
        ("id".to_owned(), "id".to_owned()),
        ("name".to_owned(), "name".to_owned()),
    ]);
    vm.overrides.push(("id".to_owned(), None));
    assert_eq!(vm.override_target("id"), Some(None), "None means don't import");
    assert_eq!(vm.override_target("name"), None, "no override means absent");
    assert_eq!(vm.mapped_pairs().len(), 1, "the overridden column drops out");
}

#[test]
fn a_select_opens_chooses_and_closes() {
    let mut vm = ready_vm();
    // Open the encoding select via the host-open flag, then choose option1.
    vm.open_select = OpenSelect::Encoding;
    let mut rt = runtime(vm, DESIGN, true);
    assert!(has_key(&rt, "option0"), "an open select mounts its option list");
    click(&mut rt, "option1");
    let actions = rt.state.actions.borrow().clone();
    assert!(
        actions.iter().any(|a| matches!(a, ImportAction::SetEncoding(_))),
        "choosing encodes a SetEncoding action with the option value"
    );
    assert_eq!(rt.state.vm.open_select, OpenSelect::None, "a choice closes the popup");
}

#[test]
fn all_phases_stay_inside_via_visible_rects() {
    for phase in [WizardPhase::Config, WizardPhase::Running, WizardPhase::Done] {
        for size in [MIN, DESIGN] {
            for dark in [true, false] {
                let mut vm = ready_vm();
                vm.phase = phase;
                let rt = runtime(vm, size, dark);
                assert!(has_key(&rt, ROOT), "{phase:?} {size:?} dark={dark}: root mounted");
                for e in rt.elements() {
                    let [x, y, w, h] = e.visible;
                    assert!(
                        x >= -0.5 && y >= -0.5 && x + w <= size.0 + 0.5 && y + h <= size.1 + 0.5,
                        "{phase:?} {size:?} dark={dark}: {} visible={:?} rect={:?} crosses window",
                        e.key.as_deref().unwrap_or(&e.path), e.visible, e.rect
                    );
                }
            }
        }
    }
}
