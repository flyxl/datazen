//! T492: `PendingPlanDialog` — the once-only count description, statement
//! order/preview keys, fingerprint, warnings, the close action and rect
//! geometry at both window sizes in both themes. The close callback is
//! recorded; no fake IPC or command runs.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use crate::i18n::t;
use crate::views::pending_plan_dialog::{
    build, ChangeWarning, PendingPlanAction, PendingPlanActionFn, PlannedStatement,
    RowChangePlanVm, CLOSE_KEY, DIALOG_KEY,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

#[derive(Clone)]
struct Scene {
    plan: Option<RowChangePlanVm>,
    open: bool,
    actions: Rc<RefCell<Vec<PendingPlanAction>>>,
    dark: bool,
}

fn statement(sql: &str, params: &[&str]) -> PlannedStatement {
    PlannedStatement {
        sql_template: sql.to_owned(),
        parameter_summary: params.iter().map(|p| (*p).to_owned()).collect(),
    }
}

fn plan() -> RowChangePlanVm {
    RowChangePlanVm {
        plan_id: "plan-1".to_owned(),
        fingerprint: "fingerprint-1".to_owned(),
        table: "users".to_owned(),
        inserts: vec![statement(
            "INSERT INTO \"users\" (\"code\") VALUES ($1)",
            &["INSERT code=\"e2e\""],
        )],
        updates: Vec::new(),
        deletes: Vec::new(),
        warnings: Vec::new(),
    }
}

fn scene(plan: Option<RowChangePlanVm>, open: bool, dark: bool) -> Scene {
    Scene {
        plan,
        open,
        actions: Rc::new(RefCell::new(Vec::new())),
        dark,
    }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let mut layer = Vec::new();
    let actions = s.actions.clone();
    let on_action: Option<PendingPlanActionFn<Scene>> = Some(Rc::new(move |_s, action| {
        actions.borrow_mut().push(action);
    }));
    build(s.plan.as_ref(), s.open, &on_action, s.dark, &mut layer);
    col().w_full().h_full().children(layer)
}

fn settle(rt: &mut Runtime<Scene>) {
    // Rune fades newly mounted modal panels in; let the entrance finish so its
    // footer button is opaque and takes pointer events.
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

fn texts(rt: &Runtime<Scene>) -> Vec<String> {
    rt.elements().iter().filter_map(|e| e.text.clone()).collect()
}

fn click(rt: &mut Runtime<Scene>, key: &str) {
    settle(rt);
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(rt);
}

fn description(inserts: usize, updates: usize, deletes: usize, table: &str) -> String {
    let mut parts = vec![table.to_owned()];
    if inserts > 0 {
        parts.push(t("tableData.pendingInserts").replace("{count}", &inserts.to_string()));
    }
    parts.push(t("tableData.pendingUpdates").replace("{count}", &updates.to_string()));
    parts.push(t("tableData.pendingDeletes").replace("{count}", &deletes.to_string()));
    parts.join(" · ")
}

#[test]
fn the_description_renders_every_count_exactly_once() {
    let rt = runtime(scene(Some(plan()), true, true), DESIGN);
    assert!(has_key(&rt, DIALOG_KEY), "the preview dialog mounts");
    let expected = description(1, 0, 0, "users");
    assert!(
        texts(&rt).iter().any(|text| text == &expected),
        "description is {expected:?}, not a doubled count; got {:?}",
        texts(&rt)
    );
    assert!(
        !texts(&rt).iter().any(|text| text.contains("1 1") || text.contains("0 0")),
        "no raw count is prefixed to the wording"
    );

    let mut full = plan();
    full.updates = vec![statement("UPDATE \"users\" SET \"name\" = $1", &["SET name=\"Bob\""])];
    full.deletes = vec![statement("DELETE FROM \"users\" WHERE \"id\" = 2", &[])];
    let rt = runtime(scene(Some(full), true, false), MIN);
    let expected = description(1, 1, 1, "users");
    assert!(
        texts(&rt).iter().any(|text| text == &expected),
        "all three kinds appear once each; got {:?}",
        texts(&rt)
    );
}

#[test]
fn fingerprint_and_statement_preview_pin_keys_and_order() {
    let mut plan = plan();
    plan.updates = vec![statement("UPDATE \"users\" SET \"name\" = $1", &["SET name=\"Bob\""])];
    plan.deletes = vec![statement("DELETE FROM \"users\" WHERE \"id\" = 2", &[])];
    let rt = runtime(scene(Some(plan), true, true), DESIGN);

    assert_eq!(text_of(&rt, "pending-plan-fingerprint-label"), t("tableData.previewFingerprint"));
    assert_eq!(text_of(&rt, "pending-plan-fingerprint"), "fingerprint-1");
    assert_eq!(text_of(&rt, "pending-plan-sql-label"), t("tableData.previewSql"));
    assert_eq!(
        text_of(&rt, "pending-plan-statement-sql-0"),
        "INSERT INTO \"users\" (\"code\") VALUES ($1)"
    );
    assert_eq!(
        text_of(&rt, "pending-plan-statement-sql-1"),
        "UPDATE \"users\" SET \"name\" = $1",
        "updates follow inserts"
    );
    assert_eq!(
        text_of(&rt, "pending-plan-statement-sql-2"),
        "DELETE FROM \"users\" WHERE \"id\" = 2",
        "deletes follow updates"
    );
    assert_eq!(
        text_of(&rt, "pending-plan-statement-params-0"),
        format!("{}: INSERT code=\"e2e\"", t("tableData.previewParameters"))
    );
    assert!(!has_key(&rt, "pending-plan-statement-params-2"), "no empty parameter line");
}

#[test]
fn no_changes_and_warnings_render_their_keys() {
    let mut empty = plan();
    empty.inserts.clear();
    let rt = runtime(scene(Some(empty), true, false), MIN);
    assert!(has_key(&rt, "pending-plan-no-changes"));
    assert_eq!(text_of(&rt, "pending-plan-no-changes"), t("tableData.noPendingChanges"));
    assert!(!has_key(&rt, "pending-plan-warnings"), "no warning block when there are none");

    let mut warned = plan();
    warned.warnings = vec![
        ChangeWarning { code: "W1".to_owned(), message: "first warning".to_owned() },
        ChangeWarning { code: "W2".to_owned(), message: "second warning".to_owned() },
    ];
    let rt = runtime(scene(Some(warned), true, true), DESIGN);
    assert!(has_key(&rt, "pending-plan-warnings"));
    assert_eq!(text_of(&rt, "pending-plan-warnings-label"), t("tableData.previewWarnings"));
    assert_eq!(text_of(&rt, "pending-plan-warning-0"), "first warning");
    assert_eq!(text_of(&rt, "pending-plan-warning-1"), "second warning");
}

#[test]
fn close_button_reaches_the_host_and_null_or_closed_mounts_nothing() {
    let mut rt = runtime(scene(Some(plan()), true, false), MIN);
    assert_eq!(label_of(&rt, CLOSE_KEY), t("common.close"));
    click(&mut rt, CLOSE_KEY);
    assert_eq!(
        *rt.state.actions.borrow(),
        vec![PendingPlanAction::Close],
        "the sole callback forwards Close"
    );

    let rt = runtime(scene(None, true, true), MIN);
    assert!(!has_key(&rt, DIALOG_KEY), "a null plan mounts nothing");
    let rt = runtime(scene(Some(plan()), false, true), MIN);
    assert!(!has_key(&rt, DIALOG_KEY), "a closed dialog mounts nothing");
}

#[test]
fn geometry_stays_inside_both_windows_in_both_themes() {
    for (width, height) in [MIN, DESIGN] {
        for dark in [true, false] {
            let mut plan = plan();
            plan.updates = vec![statement("UPDATE \"users\" SET \"name\" = $1", &["SET name=\"Bob\""])];
            plan.warnings = vec![ChangeWarning { code: "W1".to_owned(), message: "long warning".to_owned() }];
            let rt = runtime(scene(Some(plan), true, dark), (width, height));
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
