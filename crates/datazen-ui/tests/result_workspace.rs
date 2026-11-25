//! T526: `ResultWorkspace` — view resolution/chartability helpers, error and
//! empty branches, toolbar view switching, host actions and rect geometry at
//! both window sizes in both themes. Every command is a recorded
//! [`ResultWorkspaceAction`]; no fake IPC or clipboard write.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use datazen_ui::i18n::t;
use datazen_ui::views::result_workspace::result_workspace::{
    build, can_render_result_chart, chart_data_point_actions, is_chartable_result, resolve_view,
    FallbackReason, ResultWorkspaceAction, ResultWorkspaceActionFn, ResultWorkspaceView,
    ResultWorkspaceVm, WorkspaceResultVm, CHART_SLOT_KEY, EMPTY_KEY, ERROR_KEY, FIND_SLOT_KEY,
    ROOT_KEY, SUMMARY_SLOT_KEY, TABLE_SLOT_KEY, VIEW_CHART_KEY, VIEW_TABLE_KEY,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

#[derive(Clone)]
struct Scene {
    vm: ResultWorkspaceVm,
    actions: Rc<RefCell<Vec<ResultWorkspaceAction>>>,
    dark: bool,
}

fn chartable() -> WorkspaceResultVm {
    WorkspaceResultVm {
        columns: vec![("label".to_owned(), "text".to_owned()), ("amount".to_owned(), "int4".to_owned())],
        rows: vec![vec!["one".to_owned(), "1".to_owned()]],
    }
}

fn vm() -> ResultWorkspaceVm {
    ResultWorkspaceVm {
        result: Some(chartable()),
        view: ResultWorkspaceView::Table,
        has_chart_config: true,
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

fn click(rt: &mut Runtime<Scene>, key: &str) {
    rt.frame(0.0);
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
}

#[test]
fn view_resolution_and_chartability_match_react() {
    let mut state = vm();
    assert_eq!(resolve_view(&state).view, ResultWorkspaceView::Table);
    assert!(resolve_view(&state).chart_available);
    state.view = ResultWorkspaceView::Chart;
    assert_eq!(resolve_view(&state).view, ResultWorkspaceView::Chart);
    assert!(can_render_result_chart(state.result.as_ref()));

    let mut empty = vm();
    empty.result = Some(WorkspaceResultVm { rows: Vec::new(), ..chartable() });
    empty.view = ResultWorkspaceView::Chart;
    let resolution = resolve_view(&empty);
    assert_eq!(resolution.view, ResultWorkspaceView::Table);
    assert!(!resolution.chart_available);
    assert_eq!(resolution.fallback_reason, Some(FallbackReason::EmptyResult));
    assert!(!is_chartable_result(empty.result.as_ref()));

    let mut text_only = vm();
    text_only.result = Some(WorkspaceResultVm {
        columns: vec![("a".to_owned(), "text".to_owned()), ("b".to_owned(), "category".to_owned())],
        rows: vec![vec!["one".to_owned(), "x".to_owned()]],
    });
    text_only.view = ResultWorkspaceView::Chart;
    let resolution = resolve_view(&text_only);
    assert_eq!(resolution.view, ResultWorkspaceView::Table);
    assert_eq!(resolution.fallback_reason, Some(FallbackReason::NotChartable));

    let mut none = vm();
    none.result = None;
    none.view = ResultWorkspaceView::Chart;
    assert_eq!(resolve_view(&none).fallback_reason, Some(FallbackReason::EmptyResult));
}

#[test]
fn error_and_empty_branches_render_their_contracts() {
    let mut failed = vm();
    failed.error = "query failed".to_owned();
    let rt = runtime(scene(failed, true), MIN);
    assert!(has_key(&rt, ERROR_KEY));
    assert_eq!(element(&rt, "result-workspace-error-text").text.as_deref(), Some("query failed"));
    assert_eq!(element(&rt, ERROR_KEY).role, "alert");
    assert!(!has_key(&rt, ROOT_KEY));

    let mut empty = vm();
    empty.result = None;
    let rt = runtime(scene(empty, false), DESIGN);
    assert!(has_key(&rt, EMPTY_KEY));
    assert_eq!(element(&rt, EMPTY_KEY).role, "status");
    assert_eq!(element(&rt, "result-workspace-empty-text").text.as_deref(), Some(t("sqlFile.noResults")));
    assert!(!has_key(&rt, VIEW_TABLE_KEY));
}

#[test]
fn toolbar_switches_views_and_respects_chart_availability() {
    let mut rt = runtime(scene(vm(), true), DESIGN);
    assert!(has_key(&rt, ROOT_KEY));
    assert!(has_key(&rt, TABLE_SLOT_KEY));
    assert!(has_key(&rt, SUMMARY_SLOT_KEY));
    assert!(has_key(&rt, FIND_SLOT_KEY));
    assert!(!has_key(&rt, CHART_SLOT_KEY));
    assert_eq!(element(&rt, VIEW_TABLE_KEY).label.as_deref(), Some(t("chart.viewTable")));
    assert_eq!(element(&rt, VIEW_CHART_KEY).label.as_deref(), Some(t("chart.viewChart")));
    assert!(element(&rt, VIEW_CHART_KEY).focusable, "a chartable result enables the chart button");
    click(&mut rt, VIEW_CHART_KEY);
    assert_eq!(*rt.state.actions.borrow(), vec![ResultWorkspaceAction::SetView(ResultWorkspaceView::Chart)]);

    let mut text_only = vm();
    text_only.result = Some(WorkspaceResultVm {
        columns: vec![("a".to_owned(), "text".to_owned())],
        rows: vec![vec!["x".to_owned()]],
    });
    let rt = runtime(scene(text_only, false), MIN);
    assert!(!element(&rt, VIEW_CHART_KEY).focusable, "a non-chartable result disables the chart button");
    assert_eq!(element(&rt, VIEW_CHART_KEY).help.as_deref(), Some(t("chart.notChartableTooltip")));
}

#[test]
fn chart_data_point_order_and_geometry() {
    assert_eq!(
        chart_data_point_actions(1),
        [
            ResultWorkspaceAction::SetView(ResultWorkspaceView::Table),
            ResultWorkspaceAction::RowDetail(1),
        ],
        "chart data point switches to table before forwarding the row"
    );

    for (width, height) in [MIN, DESIGN] {
        for dark in [true, false] {
            for view in [ResultWorkspaceView::Table, ResultWorkspaceView::Chart] {
                let mut state = vm();
                state.view = view;
                let rt = runtime(scene(state, dark), (width, height));
                for el in rt.elements().into_iter().filter(|e| !e.exiting) {
                    let [x, y, ew, eh] = el.rect;
                    assert!(x >= -0.5 && x + ew <= width + 0.5, "{width}x{height} dark={dark}: {:?} {:?} horizontal", el.key, el.rect);
                    assert!(y >= -0.5 && y + eh <= height + 0.5, "{width}x{height} dark={dark}: {:?} {:?} vertical", el.key, el.rect);
                }
            }
        }
    }
}
