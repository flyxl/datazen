//! T501: `ExecutionStrategyAskModal` — the exact description/labels, both
//! strategy options with truncated previews, the three host actions and rect
//! geometry at both window sizes in both themes. Every choice is recorded as
//! an [`ExecutionStrategyAction`]; no fake IPC or script execution.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use datazen_ui::i18n::t;
use datazen_ui::views::query::execution_strategy_ask_modal::{
    build, ExecutionStrategyAction, ExecutionStrategyActionFn, ExecutionStrategyAskVm,
    StatementTargetVm, CANCEL_KEY, CURRENT_KEY, DESC_KEY, DIALOG_KEY, ENTIRE_KEY,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

#[derive(Clone)]
struct Scene {
    vm: ExecutionStrategyAskVm,
    actions: Rc<RefCell<Vec<ExecutionStrategyAction>>>,
    dark: bool,
}

fn vm() -> ExecutionStrategyAskVm {
    ExecutionStrategyAskVm {
        open: true,
        current_statement: Some(StatementTargetVm {
            sql: "SELECT * FROM users WHERE id = 1".to_owned(),
            from_line: 3,
            to_line: 5,
        }),
        statement_count: 4,
        entire_script: "SELECT 1;\nSELECT 2;".to_owned(),
    }
}

fn scene(vm: ExecutionStrategyAskVm, dark: bool) -> Scene {
    Scene {
        vm,
        actions: Rc::new(RefCell::new(Vec::new())),
        dark,
    }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let mut layer = Vec::new();
    let actions = s.actions.clone();
    let on_action: Option<ExecutionStrategyActionFn<Scene>> = Some(Rc::new(move |_s, action| {
        actions.borrow_mut().push(action);
    }));
    build(&s.vm, &on_action, s.dark, &mut layer);
    col().w_full().h_full().children(layer)
}

fn settle(rt: &mut Runtime<Scene>) {
    for _ in 0..120 {
        rt.frame(1.0 / 60.0);
    }
}

fn runtime(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    settle(&mut rt);
    rt
}

fn has_key(rt: &Runtime<Scene>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn text_of(rt: &Runtime<Scene>, key: &str) -> String {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .and_then(|e| e.text)
        .unwrap_or_default()
}

fn label_of(rt: &Runtime<Scene>, key: &str) -> String {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .and_then(|e| e.label)
        .unwrap_or_default()
}

fn click(rt: &mut Runtime<Scene>, key: &str) {
    settle(rt);
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(rt);
}

#[test]
fn description_and_option_labels_are_the_exact_react_copy() {
    let rt = runtime(scene(vm(), true), DESIGN);
    assert!(has_key(&rt, DIALOG_KEY), "the strategy modal mounts");
    assert_eq!(label_of(&rt, DIALOG_KEY), t("query.executionStrategy.askTitle"));
    assert_eq!(text_of(&rt, DESC_KEY), t("query.executionStrategy.askDesc"));
    assert_eq!(
        text_of(&rt, &format!("{CURRENT_KEY}-label")),
        t("query.executionStrategy.runCurrent")
            .replace("{from}", "3")
            .replace("{to}", "5")
    );
    assert_eq!(
        text_of(&rt, &format!("{ENTIRE_KEY}-label")),
        t("query.executionStrategy.runEntire").replace("{count}", "4")
    );
    assert_eq!(label_of(&rt, CANCEL_KEY), t("common.cancel"));
}

#[test]
fn option_cards_show_previews_and_truncate_at_120_characters() {
    let mut plan = vm();
    plan.entire_script = "x".repeat(130);
    let rt = runtime(scene(plan, false), MIN);
    assert_eq!(
        text_of(&rt, &format!("{CURRENT_KEY}-preview")),
        "SELECT * FROM users WHERE id = 1"
    );
    let expected = format!("{}…", "x".repeat(120));
    assert_eq!(
        text_of(&rt, &format!("{ENTIRE_KEY}-preview")),
        expected,
        "a long script is cut to 120 characters plus an ellipsis"
    );
}

#[test]
fn each_strategy_option_and_cancel_route_through_the_host() {
    let mut rt = runtime(scene(vm(), true), DESIGN);
    click(&mut rt, CURRENT_KEY);
    click(&mut rt, ENTIRE_KEY);
    click(&mut rt, CANCEL_KEY);
    assert_eq!(
        *rt.state.actions.borrow(),
        vec![
            ExecutionStrategyAction::ExecuteCurrent,
            ExecutionStrategyAction::ExecuteEntire,
            ExecutionStrategyAction::Cancel,
        ],
        "both choices and the cancel button emit their matching actions"
    );
}

#[test]
fn closed_modal_and_missing_current_card_mount_only_what_react_shows() {
    let mut closed = vm();
    closed.open = false;
    let rt = runtime(scene(closed, true), MIN);
    assert!(!has_key(&rt, DIALOG_KEY), "a closed modal mounts nothing");

    let mut no_current = vm();
    no_current.current_statement = None;
    let rt = runtime(scene(no_current, false), MIN);
    assert!(has_key(&rt, DIALOG_KEY));
    assert!(!has_key(&rt, CURRENT_KEY), "no current statement hides that card");
    assert!(has_key(&rt, ENTIRE_KEY), "the whole-script card always shows");
}

#[test]
fn geometry_stays_inside_both_windows_in_both_themes() {
    for (width, height) in [MIN, DESIGN] {
        for dark in [true, false] {
            let mut plan = vm();
            plan.entire_script = "x".repeat(130);
            let rt = runtime(scene(plan, dark), (width, height));
            for element in rt.elements().into_iter().filter(|e| !e.exiting) {
                let [x, y, w, h] = element.rect;
                assert!(
                    x >= -0.5 && x + w <= width + 0.5,
                    "{width}x{height} dark={dark}: {:?} {:?} overflows horizontally",
                    element.key,
                    element.rect
                );
                assert!(
                    y >= -0.5 && y + h <= height + 0.5,
                    "{width}x{height} dark={dark}: {:?} {:?} overflows vertically",
                    element.key,
                    element.rect
                );
            }
        }
    }
}
