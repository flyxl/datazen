//! Independent audit for T526: `ResultWorkspace` port.
//!
//! Cover the chartability inference branches and the error/empty geometry the
//! coder's sweep does not drive, then pin exact React observable contracts.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use crate::i18n::t;
use crate::views::result_workspace::result_workspace::{
    build, can_render_result_chart, chart_data_point_actions, is_chartable_result, resolve_view,
    FallbackReason, ResultWorkspaceAction, ResultWorkspaceActionFn, ResultWorkspaceView,
    ResultWorkspaceVm, WorkspaceResultVm, EMPTY_KEY, ERROR_KEY, VIEW_CHART_KEY,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

#[derive(Clone)]
struct Scene {
    vm: ResultWorkspaceVm,
    actions: Rc<RefCell<Vec<ResultWorkspaceAction>>>,
    dark: bool,
}

fn result(data_type: &str, rows: &[&str]) -> WorkspaceResultVm {
    WorkspaceResultVm {
        columns: vec![("value".to_owned(), data_type.to_owned())],
        rows: rows.iter().map(|v| vec![(*v).to_owned()]).collect(),
    }
}

fn vm() -> ResultWorkspaceVm {
    ResultWorkspaceVm {
        result: Some(result("int4", &["1"])),
        view: ResultWorkspaceView::Table,
        ..Default::default()
    }
}

fn scene(vm: ResultWorkspaceVm, dark: bool) -> Scene {
    Scene { vm, actions: Rc::new(RefCell::new(Vec::new())), dark }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let mut layer = Vec::new();
    let actions = s.actions.clone();
    let on_action: Option<ResultWorkspaceActionFn<Scene>> =
        Some(Rc::new(move |_st, action| actions.borrow_mut().push(action)));
    build(&s.vm, &on_action, s.dark, &mut layer);
    col().w_full().h_full().children(layer)
}

fn runtime(vm: ResultWorkspaceVm, dark: bool, size: (f32, f32)) -> Runtime<Scene> {
    let mut rt = Runtime::new(scene(vm, dark), view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn element(rt: &Runtime<Scene>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("audit: {key} is laid out"))
}

fn has_key(rt: &Runtime<Scene>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

#[test]
fn infer_numeric_covers_every_declared_type_keyword() {
    for dt in ["int", "serial", "double", "numeric", "decimal", "real", "float", "money", "bigint", "smallint", "INT4", "BigInt"] {
        let r = result(dt, &["x"]);
        assert!(is_chartable_result(Some(&r)), "audit: {dt} must infer numeric by data type");
    }
    for dt in ["bool", "boolean", "timestamp", "date", "time", "text", "varchar", "category"] {
        let r = result(dt, &["x"]);
        assert!(!is_chartable_result(Some(&r)), "audit: {dt} must not infer numeric");
    }
}

#[test]
fn infer_numeric_falls_back_to_samples_only_when_all_are_numeric() {
    let numeric = result("text", &["1", "2.5", "-3", "+4", " 5 "]);
    assert!(is_chartable_result(Some(&numeric)), "audit: all numeric strings infer numeric");

    let mixed = result("text", &["1", "not-a-number"]);
    assert!(!is_chartable_result(Some(&mixed)), "audit: one non-numeric sample rejects numeric inference");

    let blank = result("text", &["   ", ""]);
    assert!(!is_chartable_result(Some(&blank)), "audit: blank samples reject numeric inference");

    let missing = WorkspaceResultVm {
        columns: vec![("value".to_owned(), "text".to_owned())],
        rows: vec![Vec::new(), Vec::new()],
    };
    assert!(!is_chartable_result(Some(&missing)), "audit: no samples infer nothing");
}

#[test]
fn chartability_samples_only_the_first_50_rows() {
    let mut r = result("text", &[]);
    for i in 0..60 {
        r.rows.push(vec![if i < 50 { "1".to_owned() } else { "not-numeric".to_owned() }]);
    }
    assert!(is_chartable_result(Some(&r)), "audit: rows past index 50 must not affect inference");
}

#[test]
fn resolution_is_independent_of_saved_chart_config() {
    let mut a = vm();
    a.view = ResultWorkspaceView::Chart;
    a.has_chart_config = false;
    let mut b = a.clone();
    b.has_chart_config = true;
    assert_eq!(resolve_view(&a), resolve_view(&b), "audit: chart config presence must not change resolution");
    assert!(can_render_result_chart(a.result.as_ref()));
}

#[test]
fn empty_result_chart_request_uses_empty_result_tooltip() {
    let mut s = vm();
    s.result = Some(result("int4", &[]));
    s.view = ResultWorkspaceView::Chart;
    assert_eq!(resolve_view(&s).fallback_reason, Some(FallbackReason::EmptyResult));
    let rt = runtime(s, false, MIN);
    assert!(!element(&rt, VIEW_CHART_KEY).focusable, "audit: chart is disabled for an empty result");
    assert_eq!(element(&rt, VIEW_CHART_KEY).help.as_deref(), Some(t("chart.emptyResultTooltip")));
}

#[test]
fn error_and_empty_branches_stay_in_bounds_at_both_sizes_and_palettes() {
    for (w, h) in [MIN, DESIGN] {
        for dark in [true, false] {
            for s in [
                { let mut s = vm(); s.error = "boom".to_owned(); s },
                { let mut s = vm(); s.result = None; s },
            ] {
                let rt = runtime(s.clone(), dark, (w, h));
                let expected = if s.error.is_empty() { EMPTY_KEY } else { ERROR_KEY };
                assert!(has_key(&rt, expected), "audit: {expected} laid out at {w}x{h} dark={dark}");
                for el in rt.elements().into_iter().filter(|e| !e.exiting) {
                    let [x, y, ew, eh] = el.rect;
                    assert!(x >= -0.5 && x + ew <= w + 0.5, "audit: {:?} horizontal overflow at {w}x{h} dark={dark}: {:?}", el.key, el.rect);
                    assert!(y >= -0.5 && y + eh <= h + 0.5, "audit: {:?} vertical overflow at {w}x{h} dark={dark}: {:?}", el.key, el.rect);
                }
            }
        }
    }
}

#[test]
fn chart_data_point_order_holds_for_boundary_rows() {
    for row in [0usize, 42, usize::MAX] {
        assert_eq!(
            chart_data_point_actions(row),
            [
                ResultWorkspaceAction::SetView(ResultWorkspaceView::Table),
                ResultWorkspaceAction::RowDetail(row),
            ],
            "audit: chart data point row {row} switches to table first"
        );
    }
}
