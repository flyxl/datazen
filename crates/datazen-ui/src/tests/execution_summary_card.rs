//! T522: `ExecutionSummaryCard` — exact pills/labels, copy action + feedback,
//! absent rows/sql/status slots and rect geometry at both sizes in both themes.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use crate::i18n::t;
use crate::views::result_workspace::execution_summary_card::{
    build, ExecutionSummaryCardVm, SummaryAction, SummaryResultVm, COPIED_FEEDBACK_MS, COPY_KEY,
    ELAPSED_KEY, ROWS_KEY, SQL_KEY, STATUS_KEY, TITLE_KEY,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

#[derive(Clone)]
struct Scene {
    vm: ExecutionSummaryCardVm,
    actions: Rc<RefCell<Vec<SummaryAction>>>,
    with_status: bool,
    dark: bool,
}

fn result() -> SummaryResultVm {
    SummaryResultVm {
        sql: "SELECT * FROM users".to_owned(),
        rows_affected: Some(7),
        execution_time_ms: 42,
    }
}

fn scene(vm: ExecutionSummaryCardVm, with_status: bool, dark: bool) -> Scene {
    Scene {
        vm,
        actions: Rc::new(RefCell::new(Vec::new())),
        with_status,
        dark,
    }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let actions = s.actions.clone();
    let host = Some(Rc::new(move |state: &mut Scene, action| {
        actions.borrow_mut().push(action);
        if matches!(action, SummaryAction::CopySql { .. }) {
            state.vm.copied = true;
        }
        let _ = state;
    }) as Rc<dyn Fn(&mut Scene, SummaryAction) + 'static>);
    let status = s.with_status.then(|| {
        col()
            .key("status-node")
            .child(rune::text("ready").font(crate::ui::kit::kit(s.dark).ty.text_xsmall))
    });
    build(&s.vm, status, host, s.dark)
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

fn text_of(rt: &Runtime<Scene>, key: &str) -> String {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .and_then(|e| e.text)
        .unwrap_or_default()
}

fn all_texts(rt: &Runtime<Scene>) -> Vec<String> {
    rt.elements().iter().filter_map(|e| e.text.clone()).collect()
}

fn click(rt: &mut Runtime<Scene>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn assert_inside(rt: &Runtime<Scene>, size: (f32, f32), label: &str) {
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

#[test]
fn copy_feedback_deadline_is_the_react_constant() {
    assert_eq!(COPIED_FEEDBACK_MS, 2000);
}

#[test]
fn summary_values_are_exact() {
    let vm = ExecutionSummaryCardVm { result: result(), copied: false };
    assert_eq!(vm.rows_affected_text(), Some(t("query.rowsAffectedCount").replace("{count}", "7")));
    let rt = runtime(scene(vm, false, true), DESIGN);
    assert!(has_key(&rt, "execution-summary-card"));
    assert_eq!(text_of(&rt, TITLE_KEY), t("query.executionSuccess"));
    let texts = all_texts(&rt);
    assert!(texts.iter().any(|text| text.contains("7")), "rows-affected pill shows the count");
    assert!(texts.iter().any(|text| text.contains("42")), "elapsed pill shows the ms");
    assert_eq!(text_of(&rt, SQL_KEY), "SELECT * FROM users");
    assert!(has_key(&rt, COPY_KEY));
}

#[test]
fn copy_sends_the_sql_and_flips_the_feedback_label() {
    let vm = ExecutionSummaryCardVm { result: result(), copied: false };
    let mut rt = runtime(scene(vm, false, true), DESIGN);
    assert!(all_texts(&rt).iter().any(|text| text == "复制"), "un-copied label is React's literal");
    click(&mut rt, COPY_KEY);
    assert_eq!(
        *rt.state.actions.borrow(),
        [SummaryAction::CopySql { sql: "SELECT * FROM users".to_owned() }]
    );
    assert!(rt.state.vm.copied, "host sets the copied flag");
    assert_eq!(text_of(&rt, SQL_KEY), "SELECT * FROM users", "sql stays mounted");
}

#[test]
fn copied_flag_shows_the_copied_label() {
    let vm = ExecutionSummaryCardVm { result: result(), copied: true };
    let rt = runtime(scene(vm, false, false), DESIGN);
    assert!(all_texts(&rt).iter().any(|text| text == "已复制"), "copied label is React's literal");
}

#[test]
fn absent_rows_sql_and_status_slots() {
    let vm = ExecutionSummaryCardVm {
        result: SummaryResultVm { sql: String::new(), rows_affected: None, execution_time_ms: 9 },
        copied: false,
    };
    let rt = runtime(scene(vm, false, true), MIN);
    assert!(!has_key(&rt, ROWS_KEY), "null rowsAffected hides the pill");
    assert!(!has_key(&rt, SQL_KEY), "empty sql hides the code block");
    assert!(!has_key(&rt, COPY_KEY), "no code block => no copy button");
    assert!(has_key(&rt, ELAPSED_KEY), "elapsed is always shown");

    let rt = runtime(scene(ExecutionSummaryCardVm { result: result(), copied: false }, true, true), MIN);
    assert!(has_key(&rt, STATUS_KEY), "a provided statusBar is mounted");
}

#[test]
fn geometry_fits_both_sizes_in_both_themes() {
    for size in [MIN, DESIGN] {
        for dark in [true, false] {
            let vm = ExecutionSummaryCardVm {
                result: SummaryResultVm { sql: "SELECT * FROM a_very_long_table_name WHERE x = 1".to_owned(), rows_affected: Some(12), execution_time_ms: 987 },
                copied: false,
            };
            let rt = runtime(scene(vm, true, dark), size);
            assert_inside(&rt, size, &format!("{size:?} dark={dark}"));
        }
    }
}
