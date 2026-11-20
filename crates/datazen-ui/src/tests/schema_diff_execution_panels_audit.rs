//! Independent audit for T075 (the SchemaDiffExecutionPanels Rune port).
//!
//! Tester-written: every assertion comes from re-reading
//! `packages/frontend/src/windows/schema-diff/SchemaDiffExecutionPanels.tsx`
//! and the wizard tests: the compare step's three-pane row (loading
//! splash, rail + resizer + detail, the copy-summary journey, the
//! no-selection hint), the plan step's generating splash + actions slot +
//! tab-less review panel, and the deploy step's pinned review panel with
//! the hidden deploy button. Element evidence comes from
//! `Runtime::elements()`; presses go through real pointer events.

use std::collections::HashMap;

use rune::{col, Cx, El, Env, Runtime, TextEdit};

use crate::commands::schema_diff::wire::{
    ColumnTypeOverrideVm, RollbackCompletenessVm, SchemaDiffPlanVm, StatementRiskVm,
};
use crate::components::schema::schema_diff_panel::{ColumnDiffVm, TableSchemaDiffVm};
use crate::i18n::t;
use crate::views::schema_diff::schema_diff_execution_panels::{
    SchemaDiffClipboardFeedback, SchemaDiffExecutionPanels, SchemaDiffExecutionStep,
};

fn diff(table: &str) -> TableSchemaDiffVm {
    TableSchemaDiffVm {
        table: table.to_owned(),
        added: vec![ColumnDiffVm::new("email", "text", true, false)],
        ..Default::default()
    }
}

fn plan() -> SchemaDiffPlanVm {
    SchemaDiffPlanVm {
        table: "users".to_owned(),
        tables: vec!["users".to_owned()],
        source_dialect: "postgres".to_owned(),
        target_dialect: "postgres".to_owned(),
        same_dialect: true,
        statements: vec![crate::commands::schema_diff::wire::PlanStatementVm {
            sql: "SELECT 1".to_owned(),
            risk: StatementRiskVm::Additive,
            rollback_sql: None,
            summary: "noop".to_owned(),
            requires_transaction: None,
        }],
        warnings: Vec::new(),
        requirements: Vec::new(),
        rollback_completeness: RollbackCompletenessVm { complete: true, missing: Vec::new() },
        plan_id: None,
        type_suggestions: None,
    }
}

#[derive(Clone)]
struct Scene {
    step: SchemaDiffExecutionStep,
    loading: bool,
    diffs: Vec<TableSchemaDiffVm>,
    selected_table: Option<String>,
    table_has_diff: Option<HashMap<String, bool>>,
    clipboard_summary: bool,
    plan: Option<SchemaDiffPlanVm>,
    with_actions_slot: bool,
    selected_events: Vec<String>,
    copies: usize,
    confirm: TextEdit,
    dark: bool,
}

impl Scene {
    fn base() -> Self {
        Self {
            step: SchemaDiffExecutionStep::Compare,
            loading: false,
            diffs: vec![diff("users"), diff("orders")],
            selected_table: Some("users".to_owned()),
            table_has_diff: Some(HashMap::from([
                ("users".to_owned(), true),
                ("orders".to_owned(), false),
            ])),
            clipboard_summary: false,
            plan: Some(plan()),
            with_actions_slot: false,
            selected_events: Vec::new(),
            copies: 0,
            confirm: TextEdit::new(String::new()),
            dark: true,
        }
    }

    fn selected_diff(&self) -> Option<&TableSchemaDiffVm> {
        let selected = self.selected_table.as_deref()?;
        self.diffs.iter().find(|d| d.table == selected)
    }

    fn tables(&self) -> Vec<String> {
        self.diffs.iter().map(|d| d.table.clone()).collect()
    }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let tables = s.tables();
    let no_overrides: [ColumnTypeOverrideVm; 0] = [];
    let mut panels = SchemaDiffExecutionPanels::<Scene>::new(
        s.step,
        s.loading,
        &s.diffs,
        s.selected_diff(),
        &tables,
        s.selected_table.as_deref(),
        220.0,
        s.plan.as_ref(),
    )
    .table_has_diff(s.table_has_diff.as_ref())
    .clipboard_feedback(if s.clipboard_summary {
        Some(SchemaDiffClipboardFeedback::Summary)
    } else {
        None
    })
    .on_select_table(|s: &mut Scene, table| s.selected_events.push(table))
    .on_copy_summary(|s: &mut Scene| s.copies += 1)
    .review_inputs(
        false,
        true,
        "local (postgres)",
        true,
        false,
        &s.confirm,
        None,
    )
    .type_overrides(&no_overrides, None);
    if s.with_actions_slot {
        panels = panels.plan_actions(Some(
            col().key("host-plan-actions").child(
                rune::text("actions").key("host-plan-actions-text"),
            ),
        ));
    }
    panels.build(s.dark)
}

fn rt(scene: Scene) -> Runtime<Scene> {
    let mut rt = Runtime::new(
        scene,
        view,
        (960.0, 640.0),
        Env { system_dark: true, scale: 1.0 },
    );
    rt.frame(0.0);
    rt
}

fn maybe(rt: &Runtime<Scene>, key: &str) -> Option<rune::ElementInfo> {
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.key.as_deref() == Some(key))
}

fn text_of(rt: &Runtime<Scene>, key: &str) -> String {
    maybe(rt, key).and_then(|e| e.text).unwrap_or_default()
}

fn press(rt: &mut Runtime<Scene>, key: &str) {
    let point = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_move(point.0, point.1);
    rt.frame(0.0);
    rt.pointer_down(point.0, point.1);
    rt.pointer_up(point.0, point.1);
    rt.frame(0.0);
    rt.frame(0.0);
}

/// The compare step's first-load splash: spinner + label, no rail.
#[test]
fn compare_loading_splash() {
    let mut scene = Scene::base();
    scene.loading = true;
    scene.diffs.clear();
    let runtime = rt(scene);
    assert_eq!(
        text_of(&runtime, "schema-diff-compare-loading-text"),
        t("schemaDiff.compare")
    );
    assert!(maybe(&runtime, "schema-diff-table-list").is_none());
}

/// The compare row: rail + resizer + detail, with the selected diff's
/// header, copy button and diff panel.
#[test]
fn compare_row_renders_rail_resizer_and_detail() {
    let runtime = rt(Scene::base());
    assert!(maybe(&runtime, "schema-diff-table-list").is_some(), "rail");
    assert!(maybe(&runtime, "schema-diff-table-list-resizer").is_some(), "resizer");
    let detail = maybe(&runtime, "schema-diff-detail-panel").expect("detail testid");
    assert!(detail.rect[2] > 300.0, "detail takes the remaining width");
    assert_eq!(text_of(&runtime, "schema-diff-detail-table"), "users");
    // The schema diff panel renders below the header.
    assert!(maybe(&runtime, "schema-diff-panel").is_some());
    // The rail's badges are threaded (users has a diff).
    assert!(maybe(&runtime, "schema-diff-table-row-users").is_some());
}

/// The copy-summary button: label swap on feedback, press fires the
/// callback.
#[test]
fn copy_summary_journey() {
    let mut runtime = rt(Scene::base());
    assert_eq!(
        text_under(&runtime, "schema-diff-copy-summary"),
        t("schemaDiff.copySummary")
    );
    press(&mut runtime, "schema-diff-copy-summary");
    assert_eq!(runtime.state.copies, 1);

    let mut scene = Scene::base();
    scene.clipboard_summary = true;
    let runtime = rt(scene);
    assert_eq!(
        text_under(&runtime, "schema-diff-copy-summary"),
        t("common.copied")
    );
}

/// The first text element inside a keyed element's subtree.
fn text_under(rt: &Runtime<Scene>, key: &str) -> String {
    let elements = rt.elements();
    let index = elements
        .iter()
        .position(|e| !e.exiting && e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"));
    if let Some(text) = &elements[index].text {
        return text.clone();
    }
    let mut frontier = vec![index];
    while let Some(current) = frontier.pop() {
        for (candidate, element) in elements.iter().enumerate() {
            if element.parent == Some(current) && !element.exiting {
                if let Some(text) = &element.text {
                    if !text.is_empty() {
                        return text.clone();
                    }
                }
                frontier.push(candidate);
            }
        }
    }
    String::new()
}

/// No selected table: the select-a-table hint, no header.
#[test]
fn no_selection_shows_the_hint() {
    let mut scene = Scene::base();
    scene.selected_table = None;
    let runtime = rt(scene);
    assert_eq!(
        text_of(&runtime, "schema-diff-detail-hint-text"),
        t("schemaDiff.selectTableHint")
    );
    assert!(maybe(&runtime, "schema-diff-detail-table").is_none());
    assert!(maybe(&runtime, "schema-diff-copy-summary").is_none());
}

/// Selecting a table through the rail reports the name (the threading).
#[test]
fn rail_selection_threads() {
    let mut runtime = rt(Scene::base());
    press(&mut runtime, "schema-diff-table-row-orders");
    assert_eq!(runtime.state.selected_events, ["orders".to_owned()]);
}

/// The plan step: generating splash while no plan, the actions slot, and
/// the tab-less review panel showing the plan content.
#[test]
fn plan_step_slot_and_review() {
    let mut scene = Scene::base();
    scene.step = SchemaDiffExecutionStep::Plan;
    scene.loading = true;
    scene.plan = None;
    let runtime = rt(scene);
    assert_eq!(
        text_of(&runtime, "schema-diff-plan-generating-text"),
        t("schemaDiff.generating")
    );

    let mut scene = Scene::base();
    scene.step = SchemaDiffExecutionStep::Plan;
    scene.with_actions_slot = true;
    let runtime = rt(scene);
    assert_eq!(text_of(&runtime, "host-plan-actions-text"), "actions");
    // The review panel is tab-less and shows the plan content.
    assert!(maybe(&runtime, "schema-diff-tabs-tablist").is_none(), "hideTabs");
    assert!(maybe(&runtime, "schema-diff-plan-panel").is_some());
    assert!(maybe(&runtime, "schema-diff-allow-destructive").is_some());
}

/// The deploy step: the review panel pinned to deploy, the deploy button
/// hidden, and `deploying` threaded from loading.
#[test]
fn deploy_step_pinned_review() {
    let mut scene = Scene::base();
    scene.step = SchemaDiffExecutionStep::Deploy;
    let runtime = rt(scene);
    assert!(maybe(&runtime, "schema-diff-deploy-panel").is_some());
    assert!(maybe(&runtime, "schema-diff-tabs-tablist").is_none(), "hideTabs");
    assert!(maybe(&runtime, "schema-diff-deploy").is_none(), "hideDeployButton");

    // loading threads to the panel's deploying state: the summary renders
    // and the result box stays absent.
    let mut scene = Scene::base();
    scene.step = SchemaDiffExecutionStep::Deploy;
    scene.loading = true;
    let runtime = rt(scene);
    assert!(maybe(&runtime, "schema-diff-deploy-panel-root-summary").is_some());
    assert!(maybe(&runtime, "schema-diff-deploy-result").is_none());
}

#[allow(dead_code)]
mod t075_debug {
}

