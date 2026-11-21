//! T522: `ResultFindBar` — the exact find ids/copy, match/plan counts, option
//! dropdowns, match-case toggle, replace/write intents, close, blocked/error
//! states and rect geometry at both sizes in both palettes.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime, TextEdit};

use crate::i18n::t;
use crate::views::result_workspace::result_find_bar::{
    build, FindAction, FindHostPort, FindOperator, PlanItemVm, ResultFindBarVm, ResultFindQueryVm,
    BLOCKED_KEY, CLOSE_KEY, COLUMN_KEY, COUNT_KEY, ERROR_KEY, MATCH_CASE_KEY, NEEDLE_KEY,
    OPERATOR_KEY, PLAN_COUNT_KEY, REPLACE_ALL_KEY, REPLACE_NEXT_KEY, REPLACEMENT_KEY,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

#[derive(Clone)]
struct Scene {
    vm: ResultFindBarVm,
    actions: Rc<RefCell<Vec<FindAction>>>,
    needle: TextEdit,
    replacement: TextEdit,
    dark: bool,
}

fn plan() -> Vec<PlanItemVm> {
    vec![
        PlanItemVm { row: 0, column: "name".to_owned(), value: "acorp corp".to_owned() },
        PlanItemVm { row: 2, column: "name".to_owned(), value: "acorp again".to_owned() },
    ]
}

fn vm() -> ResultFindBarVm {
    ResultFindBarVm {
        columns: vec!["id".to_owned(), "name".to_owned()],
        query: ResultFindQueryVm {
            needle: "acme".to_owned(),
            column: None,
            operator: FindOperator::Contains,
            match_case: false,
        },
        replacement: "acorp".to_owned(),
        matched_rows: 2,
        total_rows: 3,
        plan: plan(),
        can_replace: true,
        replace_reason: None,
        busy: false,
        error: None,
        column_open: false,
        operator_open: false,
    }
}

fn scene(vm: ResultFindBarVm, dark: bool) -> Scene {
    let needle = TextEdit::new(vm.query.needle.clone());
    let replacement = TextEdit::new(vm.replacement.clone());
    Scene {
        vm,
        actions: Rc::new(RefCell::new(Vec::new())),
        needle,
        replacement,
        dark,
    }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let actions = s.actions.clone();
    let host = FindHostPort {
        on_action: Some(Rc::new(move |state: &mut Scene, action| {
            match &action {
                FindAction::SetNeedle(value) => state.vm.query.needle = value.clone(),
                FindAction::SetColumn(value) => {
                    state.vm.query.column = value.clone();
                    state.vm.column_open = false;
                }
                FindAction::SetOperator(value) => {
                    state.vm.query.operator = *value;
                    state.vm.operator_open = false;
                }
                FindAction::ToggleMatchCase => state.vm.query.match_case = !state.vm.query.match_case,
                FindAction::SetReplacement(value) => state.vm.replacement = value.clone(),
                FindAction::ToggleColumnOptions => state.vm.column_open = !state.vm.column_open,
                FindAction::ToggleOperatorOptions => state.vm.operator_open = !state.vm.operator_open,
                _ => {}
            }
            actions.borrow_mut().push(action);
        })),
        needle_edit: Some(Rc::new(|s: &mut Scene| &mut s.needle)),
        replacement_edit: Some(Rc::new(|s: &mut Scene| &mut s.replacement)),
    };
    col().w_full().h_full().child(build(&s.vm, s.dark, &host))
}

fn runtime(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn has_key(rt: &Runtime<Scene>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn element(rt: &Runtime<Scene>, key: &str) -> rune::ElementInfo {
    rt.elements().into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn text_of(rt: &Runtime<Scene>, key: &str) -> String {
    rt.elements().into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .and_then(|e| e.text)
        .unwrap_or_default()
}

fn click(rt: &mut Runtime<Scene>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out before click"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn assert_inside(rt: &Runtime<Scene>, size: (f32, f32), label: &str) {
    let outside: Vec<String> = rt.elements().iter()
        .filter(|e| {
            let r = e.rect;
            r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > size.0 + 0.5 || r[1] + r[3] > size.1 + 0.5
        })
        .map(|e| format!("{} {:?}", e.key.as_deref().unwrap_or(&e.path), e.rect))
        .collect();
    assert!(outside.is_empty(), "{label}: outside {outside:?}");
}

#[test]
fn operator_wire_ids_and_labels_are_exact() {
    assert_eq!(FindOperator::Contains.as_str(), "contains");
    assert_eq!(FindOperator::NotContains.as_str(), "notContains");
    assert_eq!(FindOperator::Prefix.as_str(), "prefix");
    assert_eq!(FindOperator::Suffix.as_str(), "suffix");
    for op in FindOperator::ALL {
        assert!(crate::i18n::has(op.label_key()), "{} has a locale entry", op.label_key());
    }
}

#[test]
fn find_bar_keys_and_exact_counts() {
    let rt = runtime(scene(vm(), true), DESIGN);
    for key in [
        "result-find-bar", NEEDLE_KEY, COLUMN_KEY, OPERATOR_KEY, MATCH_CASE_KEY, COUNT_KEY,
        CLOSE_KEY, REPLACEMENT_KEY, PLAN_COUNT_KEY, REPLACE_NEXT_KEY, REPLACE_ALL_KEY,
    ] {
        assert!(has_key(&rt, key), "{key} is mounted");
    }
    let expected = t("resultFind.matchCount").replace("{matched}", "2").replace("{total}", "3");
    assert_eq!(text_of(&rt, COUNT_KEY), expected);
    assert_eq!(text_of(&rt, PLAN_COUNT_KEY), t("resultFind.planCount").replace("{count}", "2"));
    let rendered: Vec<String> = rt.elements().iter().filter_map(|e| e.text.clone()).collect();
    assert!(rendered.iter().any(|x| x == t("resultFind.replaceNext")));
    assert!(rendered.iter().any(|x| x == t("resultFind.replaceAll")));
}

#[test]
fn toggles_close_and_replace_actions_route_through_the_host() {
    let mut rt = runtime(scene(vm(), true), DESIGN);
    click(&mut rt, MATCH_CASE_KEY);
    assert_eq!(*rt.state.actions.borrow(), [FindAction::ToggleMatchCase]);

    let mut rt = runtime(scene(vm(), true), DESIGN);
    click(&mut rt, CLOSE_KEY);
    assert_eq!(*rt.state.actions.borrow(), [FindAction::Close]);

    let mut rt = runtime(scene(vm(), true), DESIGN);
    click(&mut rt, REPLACE_NEXT_KEY);
    assert_eq!(*rt.state.actions.borrow(), [FindAction::ReplaceNext { item: plan()[0].clone() }]);

    let mut rt = runtime(scene(vm(), true), DESIGN);
    click(&mut rt, REPLACE_ALL_KEY);
    assert_eq!(*rt.state.actions.borrow(), [FindAction::ReplaceAll]);
}

#[test]
fn column_and_operator_options_emit_set_actions() {
    let mut rt = runtime(scene(vm(), true), DESIGN);
    click(&mut rt, COLUMN_KEY);
    assert_eq!(*rt.state.actions.borrow(), [FindAction::ToggleColumnOptions]);
    assert!(has_key(&rt, "result-find-column-option-name"));
    click(&mut rt, "result-find-column-option-name");
    assert_eq!(
        rt.state.actions.borrow().last(),
        Some(&FindAction::SetColumn(Some("name".to_owned())))
    );
    assert_eq!(rt.state.vm.query.column.as_deref(), Some("name"));

    let mut rt = runtime(scene(vm(), true), DESIGN);
    click(&mut rt, OPERATOR_KEY);
    click(&mut rt, "result-find-operator-option-prefix");
    assert_eq!(rt.state.actions.borrow().last(), Some(&FindAction::SetOperator(FindOperator::Prefix)));
    assert_eq!(rt.state.vm.query.operator, FindOperator::Prefix);
    assert!(rt.elements().iter().any(|e| e.text.as_deref() == Some(t("resultFind.operator.prefix"))));
}

#[test]
fn blocked_reason_and_error_are_pinned() {
    let mut blocked = vm();
    blocked.can_replace = false;
    blocked.replace_reason = Some(t("resultFind.safeMode").to_owned());
    let rt = runtime(scene(blocked, true), DESIGN);
    assert!(has_key(&rt, BLOCKED_KEY));
    assert_eq!(text_of(&rt, BLOCKED_KEY), t("resultFind.safeMode"));
    assert!(!has_key(&rt, REPLACEMENT_KEY), "Safe Mode hides the write row");
    assert!(!has_key(&rt, REPLACE_NEXT_KEY));

    let mut errored = vm();
    errored.error = Some("write failed".to_owned());
    let rt = runtime(scene(errored, false), DESIGN);
    assert!(has_key(&rt, ERROR_KEY));
    assert_eq!(text_of(&rt, ERROR_KEY), "write failed");
}

#[test]
fn busy_and_empty_plan_gate_the_write_controls() {
    let mut busy = vm();
    busy.busy = true;
    let rt = runtime(scene(busy, true), DESIGN);
    assert!(!element(&rt, REPLACE_NEXT_KEY).focusable, "busy disables Replace");
    assert!(!element(&rt, REPLACE_ALL_KEY).focusable, "busy disables Replace All");

    let mut empty = vm();
    empty.plan.clear();
    let rt = runtime(scene(empty, true), DESIGN);
    assert_eq!(text_of(&rt, PLAN_COUNT_KEY), t("resultFind.planCount").replace("{count}", "0"));
    assert!(!element(&rt, REPLACE_NEXT_KEY).focusable, "no plan disables Replace");
}

#[test]
fn geometry_fits_both_sizes_in_both_themes() {
    for size in [MIN, DESIGN] {
        for dark in [true, false] {
            let mut plan_vm = vm();
            plan_vm.column_open = true;
            let rt = runtime(scene(plan_vm, dark), size);
            assert_inside(&rt, size, &format!("open {size:?} dark={dark}"));

            let rt = runtime(scene(vm(), dark), size);
            assert_inside(&rt, size, &format!("closed {size:?} dark={dark}"));
        }
    }
}
