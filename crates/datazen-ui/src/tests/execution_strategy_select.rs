//! T512: `ExecutionStrategySelect` port — the React toolbar journey pinning
//! default/label/title, the four exact menu options with the `✓` on the active
//! choice, host selection actions, and rect geometry at both sizes in both palettes.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use crate::i18n::t;
use crate::views::query::query_editor_section::ExecutionStrategy;
use crate::views::query::toolbar::execution_strategy_select::{
    build, strategy_id, ExecutionStrategyAction, ExecutionStrategySelectVm,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

fn scene(vm: ExecutionStrategySelectVm, dark: bool) -> Scene {
    Scene { vm, actions: Rc::new(RefCell::new(Vec::new())), dark }
}

#[derive(Clone)]
struct Scene {
    vm: ExecutionStrategySelectVm,
    actions: Rc<RefCell<Vec<ExecutionStrategyAction>>>,
    dark: bool,
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let actions = s.actions.clone();
    let callback = Rc::new(move |state: &mut Scene, action| {
        match &action {
            ExecutionStrategyAction::ToggleMenu => state.vm.open = !state.vm.open,
            ExecutionStrategyAction::Select(strategy) => {
                state.vm.strategy = *strategy;
                state.vm.open = false;
            }
        }
        actions.borrow_mut().push(action);
    });
    let mounted = build(&s.vm, Some(callback), s.dark);
    col().w_full().h_full().child(mounted)
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
fn wire_ids_and_default_are_exact() {
    assert_eq!(strategy_id(ExecutionStrategy::CurrentStatement), "current_statement");
    assert_eq!(strategy_id(ExecutionStrategy::EntireScript), "entire_script");
    assert_eq!(strategy_id(ExecutionStrategy::LargestStatement), "largest_statement");
    assert_eq!(strategy_id(ExecutionStrategy::Ask), "ask");

    // Default() is React's `?? 'current_statement'` when the setting is unset.
    let vm = ExecutionStrategySelectVm::default();
    assert_eq!(vm.strategy, ExecutionStrategy::CurrentStatement);
    assert_eq!(vm.active_label_key(), "query.executionStrategy.currentStatement");
    assert_eq!(
        vm.title(),
        format!("{}: {}", t("query.executionStrategy.title"), t("query.executionStrategy.currentStatement"))
    );
}

#[test]
fn trigger_label_and_title_are_the_exact_locale_keys() {
    let rt = runtime(scene(ExecutionStrategySelectVm::default(), true), DESIGN);
    assert!(has_key(&rt, "execution-strategy-select"));
    assert!(has_key(&rt, "editor-execution-strategy-button"));
    assert_eq!(
        element(&rt, "editor-execution-strategy-button").label.as_deref(),
        Some(t("query.executionStrategy.currentStatement"))
    );
    assert!(!has_key(&rt, "execution-strategy-menu"), "menu is closed initially");
}

#[test]
fn pressing_the_trigger_emits_toggle_and_opens_all_four_exact_options() {
    let mut rt = runtime(scene(ExecutionStrategySelectVm::default(), true), DESIGN);
    click(&mut rt, "editor-execution-strategy-button");
    assert_eq!(rt.state.actions.borrow().first(), Some(&ExecutionStrategyAction::ToggleMenu));
    assert!(has_key(&rt, "execution-strategy-menu"));

    for (strategy, label_key) in [
        (ExecutionStrategy::CurrentStatement, "query.executionStrategy.currentStatement"),
        (ExecutionStrategy::EntireScript, "query.executionStrategy.entireScript"),
        (ExecutionStrategy::LargestStatement, "query.executionStrategy.largestStatement"),
        (ExecutionStrategy::Ask, "query.executionStrategy.ask"),
    ] {
        let key = format!("strategy-{}", strategy_id(strategy));
        assert!(has_key(&rt, &key), "{key} is mounted");
        let expected = if strategy == ExecutionStrategy::CurrentStatement {
            format!("✓ {}", t(label_key))
        } else {
            format!("   {}", t(label_key))
        };
        assert_eq!(element(&rt, &key).label.as_deref(), Some(expected.as_str()), "{key} check mark");
    }
}

#[test]
fn selecting_an_option_reports_the_host_intent_and_check_mark_moves_on_reopen() {
    let mut rt = runtime(scene(ExecutionStrategySelectVm::default(), true), DESIGN);
    click(&mut rt, "editor-execution-strategy-button");
    click(&mut rt, "strategy-entire_script");
    assert_eq!(
        rt.state.actions.borrow().iter().last(),
        Some(&ExecutionStrategyAction::Select(ExecutionStrategy::EntireScript)),
        "choosing whole script emits the Select intent"
    );
    assert_eq!(rt.state.vm.strategy, ExecutionStrategy::EntireScript, "host applies the choice");
    assert_eq!(
        rt.state.vm.title(),
        format!("{}: {}", t("query.executionStrategy.title"), t("query.executionStrategy.entireScript")),
        "the trigger reflects the new setting"
    );

    // Reopen: the check mark follows the host-updated strategy.
    click(&mut rt, "editor-execution-strategy-button");
    let current = element(&rt, "strategy-entire_script").label.unwrap_or_default();
    let other = element(&rt, "strategy-current_statement").label.unwrap_or_default();
    assert!(current.starts_with("✓ "), "whole script is now checked: {current}");
    assert!(other.starts_with("   "), "current statement is no longer checked: {other}");
}

#[test]
fn disabled_trigger_is_inert() {
    let vm = ExecutionStrategySelectVm { disabled: true, ..Default::default() };
    let mut rt = runtime(scene(vm, true), DESIGN);
    assert!(!element(&rt, "editor-execution-strategy-button").focusable, "disabled trigger");
    click(&mut rt, "editor-execution-strategy-button");
    assert!(rt.state.actions.borrow().is_empty(), "a disabled control emits no action");
}

#[test]
fn geometry_fits_both_sizes_in_both_themes() {
    for size in [MIN, DESIGN] {
        for dark in [true, false] {
            let mut vm = ExecutionStrategySelectVm {
                icon_only: true,
                ..Default::default()
            };
            let rt = runtime(scene(vm.clone(), dark), size);
            assert_inside(&rt, size, &format!("closed {size:?} dark={dark}"));

            vm.open = true;
            let rt = runtime(scene(vm, dark), size);
            assert_inside(&rt, size, &format!("open {size:?} dark={dark}"));
        }
    }
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
    assert!(outside.is_empty(), "{label}: outside {outside:?}");
}

// Reverse verification (mutation -> observed failure text -> restored):
// 1. Dropping the `✓ ` prefix on the active option made
//    `pressing_the_trigger_emits_toggle_and_opens_all_four_exact_options` fail
//    on `strategy-current_statement check mark`.
// 2. Using the wrong wire id for `entire_script` made the selection test fail
//    on `strategy-entire_script is laid out`.
