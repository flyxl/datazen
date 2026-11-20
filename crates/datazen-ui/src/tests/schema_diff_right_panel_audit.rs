//! Independent audit for T204 (the SchemaDiffRightPanel Rune port).
//!
//! Tester-written: every assertion comes from re-reading
//! `packages/frontend/src/windows/schema-diff/SchemaDiffRightPanel.tsx` and
//! the React tests (`__tests__/SchemaDiffPanels.test.tsx` — the tab switch
//! and the generate-plan empty state): the tab strip, the two tab bodies,
//! the threaded panels and the empty state. Element evidence comes from
//! `Runtime::elements()`; presses go through real pointer events.

use std::collections::HashMap;

use rune::{Cx, El, Env, Runtime, TextEdit};

use crate::commands::schema_diff::wire::{
    ColumnTypeOverrideVm, RollbackCompletenessVm, SchemaDiffPlanVm, StatementRiskVm,
};
use crate::i18n::t;
use crate::views::schema_diff::schema_diff_right_panel::{
    SchemaDiffRightPanel, SchemaDiffRightPanelTab,
};

fn plan() -> SchemaDiffPlanVm {
    crate::commands::schema_diff::wire::PlanStatementVm {
        sql: "SELECT 1".to_owned(),
        risk: StatementRiskVm::Additive,
        rollback_sql: None,
        summary: "noop".to_owned(),
        requires_transaction: None,
    };
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
    active_tab: SchemaDiffRightPanelTab,
    plan: Option<SchemaDiffPlanVm>,
    hide_tabs: bool,
    confirm: TextEdit,
    tab_changes: Vec<SchemaDiffRightPanelTab>,
    deploys: usize,
    dark: bool,
}

impl Scene {
    fn base() -> Self {
        Self {
            active_tab: SchemaDiffRightPanelTab::Plan,
            plan: Some(plan()),
            hide_tabs: false,
            confirm: TextEdit::new(String::new()),
            tab_changes: Vec::new(),
            deploys: 0,
            dark: true,
        }
    }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    SchemaDiffRightPanel::<Scene>::new(
        s.active_tab,
        s.plan.as_ref(),
        false,
        true,
        "local (postgres)",
        true,
        false,
        &s.confirm,
        false,
        None,
    )
    .hide_tabs(s.hide_tabs)
    .on_tab_change(|s: &mut Scene, tab| s.tab_changes.push(tab))
    .on_confirm_edit(|s: &mut Scene| &mut s.confirm)
    .on_deploy(|s: &mut Scene| s.deploys += 1)
    .build(s.dark)
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

/// The React test's core: the plan tab shows the plan body, and clicking
/// the deploy tab reports the switch.
#[test]
fn tab_strip_switches_to_deploy() {
    let mut runtime = rt(Scene::base());
    assert!(maybe(&runtime, "schema-diff-right-panel").is_some());
    assert!(maybe(&runtime, "schema-diff-plan-panel").is_some(), "plan body wrapper");
    assert!(
        maybe(&runtime, "schema-diff-allow-destructive").is_some(),
        "the plan panel's checkbox is threaded through"
    );
    // The tabs carry their labels (the atom sets the accessible label on
    // the tab element).
    let plan_tab = maybe(&runtime, "schema-diff-tabs-tab-plan").expect("plan tab");
    assert_eq!(plan_tab.label.as_deref(), Some(t("schemaDiff.stepPlan")));
    let deploy_tab = maybe(&runtime, "schema-diff-tabs-tab-deploy").expect("deploy tab");
    assert_eq!(deploy_tab.label.as_deref(), Some(t("schemaDiff.stepReview")));
    press(&mut runtime, "schema-diff-tabs-tab-deploy");
    assert_eq!(runtime.state.tab_changes, [SchemaDiffRightPanelTab::Deploy]);
}

/// The deploy tab shows the deploy panel body (the T025 builder threaded).
#[test]
fn deploy_tab_shows_the_deploy_panel() {
    let mut scene = Scene::base();
    scene.active_tab = SchemaDiffRightPanelTab::Deploy;
    let mut runtime = rt(scene);
    assert!(maybe(&runtime, "schema-diff-deploy-panel").is_some(), "deploy body wrapper");
    // The deploy panel's summary box renders the threaded target label.
    assert!(maybe(&runtime, "schema-diff-deploy-panel-root-summary-target-value").is_some());
    // The threaded deploy callback fires through the panel's button.
    press(&mut runtime, "schema-diff-deploy");
    assert_eq!(runtime.state.deploys, 1);
}

/// No plan: the tab body shows the generate-plan empty state (the React
/// test's "asks the user to generate a plan").
#[test]
fn no_plan_shows_the_generate_plan_hint() {
    let mut scene = Scene::base();
    scene.plan = None;
    let runtime = rt(scene);
    assert!(maybe(&runtime, "schema-diff-plan-panel").is_some());
    assert_eq!(
        text_of(&runtime, "schema-diff-right-panel-empty-text"),
        t("schemaDiff.generatePlan")
    );
    assert!(maybe(&runtime, "schema-diff-allow-destructive").is_none());

    // The deploy tab's empty state is the same hint.
    let mut scene = Scene::base();
    scene.plan = None;
    scene.active_tab = SchemaDiffRightPanelTab::Deploy;
    let runtime = rt(scene);
    assert!(maybe(&runtime, "schema-diff-deploy-panel").is_some());
    assert_eq!(
        text_of(&runtime, "schema-diff-right-panel-empty-text"),
        t("schemaDiff.generatePlan")
    );
}

/// `hideTabs` (the wizard's step mode) drops the tab strip.
#[test]
fn hide_tabs_drops_the_strip() {
    let mut scene = Scene::base();
    scene.hide_tabs = true;
    let runtime = rt(scene);
    assert!(maybe(&runtime, "schema-diff-tabs-tablist").is_none());
    assert!(maybe(&runtime, "schema-diff-plan-panel").is_some(), "body stays");
}

/// The empty state keeps React's min-h-[8rem] footprint.
#[test]
fn empty_state_keeps_the_min_height() {
    let mut scene = Scene::base();
    scene.plan = None;
    let runtime = rt(scene);
    let empty = maybe(&runtime, "schema-diff-right-panel-empty").expect("empty state");
    assert!(empty.rect[3] >= 95.0, "min-h-[8rem] = 96pt, got {}", empty.rect[3]);
}
