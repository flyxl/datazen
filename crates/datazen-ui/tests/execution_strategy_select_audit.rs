//! T512 independent audit (tester, not coder).
//!
//! Precise probes of observables the shipped 6 tests do not pin: the shared
//! strategy enum order / per-strategy title, the menu + menuitem roles, and a
//! non-vacuous `visible`-rect geometry channel for the open dropdown.

use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use datazen_ui::i18n::t;
use datazen_ui::views::query::query_editor_section::ExecutionStrategy;
use datazen_ui::views::query::toolbar::execution_strategy_select::{
    all_strategies, build, strategy_id, ExecutionStrategyAction, ExecutionStrategySelectVm,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

#[derive(Clone)]
struct Scene {
    vm: ExecutionStrategySelectVm,
    dark: bool,
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let cb: Rc<dyn Fn(&mut Scene, ExecutionStrategyAction) + 'static> = Rc::new(|_s, _a| {});
    col().w_full().h_full().child(build(&s.vm, Some(cb), s.dark))
}

fn runtime(vm: ExecutionStrategySelectVm, size: (f32, f32), dark: bool) -> Runtime<Scene> {
    let mut rt = Runtime::new(Scene { vm, dark }, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

#[test]
fn strategy_order_and_title_follow_the_shared_enum() {
    assert_eq!(
        all_strategies(),
        [
            ExecutionStrategy::CurrentStatement,
            ExecutionStrategy::EntireScript,
            ExecutionStrategy::LargestStatement,
            ExecutionStrategy::Ask,
        ],
        "React STRATEGIES order must match the shared enum"
    );
    for strategy in all_strategies() {
        let vm = ExecutionStrategySelectVm { strategy, ..Default::default() };
        assert_eq!(vm.active_label_key(), strategy.label_key());
        assert_eq!(
            vm.title(),
            format!("{}: {}", t("query.executionStrategy.title"), t(strategy.label_key())),
            "the composed title tracks {}", strategy_id(strategy)
        );
    }
}

#[test]
fn menu_and_items_carry_their_semantic_roles() {
    let vm = ExecutionStrategySelectVm { open: true, ..Default::default() };
    let rt = runtime(vm, DESIGN, true);
    assert_eq!(element(&rt, "execution-strategy-menu").role, "menu");
    for strategy in all_strategies() {
        let key = format!("strategy-{}", strategy_id(strategy));
        assert_eq!(element(&rt, &key).role, "menuitem", "{key} is a menuitem");
        assert!(element(&rt, &key).focusable, "{key} is keyboard reachable");
    }
}

#[test]
fn open_dropdown_stays_inside_via_visible_rects() {
    for size in [MIN, DESIGN] {
        for dark in [true, false] {
            let vm = ExecutionStrategySelectVm {
                open: true,
                icon_only: true,
                ..Default::default()
            };
            let rt = runtime(vm, size, dark);
            assert!(rt.elements().iter().any(|e| e.key.as_deref() == Some("execution-strategy-select")));
            for e in rt.elements() {
                let [x, y, w, h] = e.visible;
                assert!(
                    x >= -0.5 && y >= -0.5 && x + w <= size.0 + 0.5 && y + h <= size.1 + 0.5,
                    "{size:?} dark={dark}: {} visible={:?} rect={:?} crosses window",
                    e.key.as_deref().unwrap_or(&e.path), e.visible, e.rect
                );
            }
        }
    }
}
