//! T532: `StructurePlanPreview` — statement count/warnings/risk rendering,
//! empty/loading states, close action and rect geometry at both sizes in both
//! palettes. The close callback is recorded; no fake IPC.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use datazen_ui::i18n::t;
use datazen_ui::structure_editor::{PlanStatement, StatementRisk, StructureChangePlan};
use datazen_ui::views::structure_plan_preview::{
    build, StructurePlanAction, StructurePlanActionFn, StructurePlanPreviewVm, ROOT_KEY,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

#[derive(Clone)]
struct Scene {
    vm: StructurePlanPreviewVm,
    actions: Rc<RefCell<Vec<StructurePlanAction>>>,
    dark: bool,
}

fn stmt(risk: StatementRisk) -> PlanStatement {
    PlanStatement {
        sql: "ALTER TABLE users ADD COLUMN x int".to_owned(),
        summary: "add column x".to_owned(),
        risk,
    }
}

fn vm() -> StructurePlanPreviewVm {
    StructurePlanPreviewVm {
        plan: StructureChangePlan {
            statements: vec![
                stmt(StatementRisk::Additive),
                stmt(StatementRisk::Destructive),
                stmt(StatementRisk::Rewrite),
            ],
            warnings: Some(vec!["lossy".to_owned()]),
        },
        loading: false,
    }
}

fn scene(vm: StructurePlanPreviewVm, dark: bool) -> Scene {
    Scene { vm, actions: Rc::new(RefCell::new(Vec::new())), dark }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let actions = s.actions.clone();
    let on_action: Option<StructurePlanActionFn<Scene>> =
        Some(Rc::new(move |_st, action| actions.borrow_mut().push(action)));
    let mut layer = Vec::new();
    build(&s.vm, &on_action, s.dark, &mut layer);
    col().w_full().h_full().children(layer)
}

fn runtime(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn element(rt: &Runtime<Scene>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has_key(rt: &Runtime<Scene>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn text(rt: &Runtime<Scene>, key: &str) -> String {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).and_then(|e| e.text).unwrap_or_default()
}

fn click(rt: &mut Runtime<Scene>, key: &str) {
    rt.frame(0.0);
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
}

#[test]
fn helpers_follow_react_contracts() {
    let state = vm();
    assert!(state.has_warnings());
    assert_eq!(state.count_text(), format!("3 {}", t("structEditor.statements")));
    assert!(StructurePlanPreviewVm::risk_is_destructive(StatementRisk::Destructive));
    assert!(!StructurePlanPreviewVm::risk_is_destructive(StatementRisk::Additive));
    let empty = StructurePlanPreviewVm::default();
    assert!(!empty.has_warnings());
    assert_eq!(empty.count_text(), format!("0 {}", t("structEditor.statements")));
}

#[test]
fn header_warnings_and_statements_render() {
    let rt = runtime(scene(vm(), true), DESIGN);
    assert!(has_key(&rt, ROOT_KEY));
    assert_eq!(text(&rt, "struct-plan-title"), t("common.sqlPreview"));
    assert_eq!(text(&rt, "struct-plan-close"), t("common.close"));
    assert!(has_key(&rt, "struct-plan-warnings"));
    assert_eq!(text(&rt, "struct-plan-warnings-title"), t("structEditor.warnings"));
    assert_eq!(text(&rt, "struct-plan-warning-0"), "lossy");
    assert_eq!(text(&rt, "struct-plan-count-text"), "3 条语句");
    for i in 0..3 {
        assert!(has_key(&rt, &format!("struct-plan-statement-{i}")));
        assert!(has_key(&rt, &format!("struct-plan-sql-{i}")));
    }
    assert_eq!(text(&rt, "struct-plan-risk-text-1"), "destructive");
    assert_eq!(text(&rt, "struct-plan-summary-0"), "add column x");
}

#[test]
fn empty_loading_and_close_states() {
    let empty = runtime(scene(StructurePlanPreviewVm::default(), false), MIN);
    assert!(has_key(&empty, "struct-plan-empty"));
    assert_eq!(text(&empty, "struct-plan-empty"), t("structEditor.emptyPlan"));
    assert!(!has_key(&empty, "struct-plan-warnings"));

    let mut loading = StructurePlanPreviewVm::default();
    loading.loading = true;
    let rt = runtime(scene(loading, true), MIN);
    assert!(has_key(&rt, "struct-plan-loading"));
    assert_eq!(text(&rt, "struct-plan-loading-text"), t("common.loading"));

    let mut rt = runtime(scene(vm(), true), DESIGN);
    click(&mut rt, "struct-plan-close");
    assert_eq!(*rt.state.actions.borrow(), vec![StructurePlanAction::Close]);
}

#[test]
fn geometry_both_sizes_both_palettes() {
    for (w, h) in [MIN, DESIGN] {
        for dark in [true, false] {
            let rt = runtime(scene(vm(), dark), (w, h));
            for el in rt.elements().into_iter().filter(|e| !e.exiting) {
                let [x, y, ew, eh] = el.rect;
                assert!(x >= -0.5 && x + ew <= w + 0.5, "{w}x{h} dark={dark}: {:?} {:?} horizontal", el.key, el.rect);
                assert!(y >= -0.5 && y + eh <= h + 0.5, "{w}x{h} dark={dark}: {:?} {:?} vertical", el.key, el.rect);
            }
        }
    }
}
