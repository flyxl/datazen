//! Independent audit for T178 (the SchemaDiffPlanPanel Rune port).
//!
//! Tester-written: every assertion comes from re-reading
//! `packages/frontend/src/windows/schema-diff/SchemaDiffPlanPanel.tsx` and
//! the React tests (`__tests__/SchemaDiffPanels.test.tsx` — the empty-plan
//! distinction, the requirements section, the rollback status and the
//! type-suggestions journey): branch presence, exact labels, the derived
//! override input, and the callbacks. Element evidence comes from
//! `Runtime::elements()`; presses and typing go through real events.

use std::collections::HashMap;

use rune::{Cx, El, Env, Runtime, TextEdit};

use crate::commands::schema_diff::wire::{
    ColumnTypeOverrideVm, PlanRequirementKindVm, PlanRequirementVm, RollbackCompletenessVm,
    SchemaDiffPlanVm, StatementRiskVm, TypeSuggestionVm,
};
use crate::i18n::t;
use crate::ui::kit::kit;
use crate::views::schema_diff::schema_diff_plan_panel::SchemaDiffPlanPanel;

fn statement(risk: StatementRiskVm) -> crate::commands::schema_diff::wire::PlanStatementVm {
    crate::commands::schema_diff::wire::PlanStatementVm {
        sql: format!("-- {risk:?}"),
        risk,
        rollback_sql: None,
        summary: "summary".to_owned(),
        requires_transaction: None,
    }
}

fn plan() -> SchemaDiffPlanVm {
    SchemaDiffPlanVm {
        table: "users".to_owned(),
        tables: vec!["users".to_owned()],
        source_dialect: "postgres".to_owned(),
        target_dialect: "mysql".to_owned(),
        same_dialect: false,
        statements: vec![statement(StatementRiskVm::Additive)],
        warnings: Vec::new(),
        requirements: Vec::new(),
        rollback_completeness: RollbackCompletenessVm { complete: true, missing: Vec::new() },
        plan_id: None,
        type_suggestions: None,
    }
}

#[derive(Clone)]
struct Scene {
    plan: SchemaDiffPlanVm,
    allow_destructive: bool,
    include_indexes: bool,
    regenerating: bool,
    overrides: Vec<ColumnTypeOverrideVm>,
    edits: HashMap<String, TextEdit>,
    toggles_allow: usize,
    toggles_include: usize,
    regenerations: usize,
    applies: usize,
    dark: bool,
}

impl Scene {
    fn base() -> Self {
        Self {
            plan: plan(),
            allow_destructive: false,
            include_indexes: true,
            regenerating: false,
            overrides: Vec::new(),
            edits: HashMap::new(),
            toggles_allow: 0,
            toggles_include: 0,
            regenerations: 0,
            applies: 0,
            dark: true,
        }
    }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    SchemaDiffPlanPanel::<Scene>::new(&s.plan, s.allow_destructive, s.include_indexes)
        .regenerating(s.regenerating)
        .type_overrides(&s.overrides)
        .override_edits(Some(&s.edits))
        .on_override_edit(|s: &mut Scene, table, column, initial| {
            s.edits
                .entry(format!("{table}.{column}"))
                .or_insert_with(|| TextEdit::new(initial.to_owned()))
        })
        .on_allow_destructive(|s: &mut Scene| s.toggles_allow += 1)
        .on_include_indexes(|s: &mut Scene| s.toggles_include += 1)
        .on_regenerate(|s: &mut Scene| s.regenerations += 1)
        .on_apply_type_overrides(|s: &mut Scene| s.applies += 1)
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

/// The first text element inside a keyed element's subtree (fields carry
/// their text on an inner element, not the keyed root).
fn text_under(rt: &Runtime<Scene>, key: &str) -> String {
    let elements = rt.elements();
    let index = elements
        .iter()
        .position(|e| !e.exiting && e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"));
    // The keyed element may itself carry the text (a textbox); otherwise
    // the first text-bearing element in its subtree.
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

fn press(rt: &mut Runtime<Scene>, key: &str) {
    let point = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_move(point.0, point.1);
    rt.frame(0.0);
    rt.pointer_down(point.0, point.1);
    rt.pointer_up(point.0, point.1);
    rt.frame(0.0);
    rt.frame(0.0);
}

/// The empty-plan line distinguishes clean plans from warning-only ones
/// (the React test).
#[test]
fn empty_plan_distinguishes_clean_from_warning_only() {
    let mut scene = Scene::base();
    scene.plan.statements.clear();
    let runtime = rt(scene.clone());
    assert_eq!(
        text_of(&runtime, "schema-diff-empty-plan-text"),
        t("schemaDiff.emptyPlanNoDiff")
    );

    scene.plan.warnings = vec!["Skipped DROP COLUMN old_col because destructive changes are disabled".to_owned()];
    let runtime = rt(scene);
    assert_eq!(
        text_of(&runtime, "schema-diff-empty-plan-text"),
        t("schemaDiff.emptyPlanSkipped")
    );
}

/// Backfill and unsupported requirements render above the statements (the
/// React test: title, target, hint, and `target: reason`).
#[test]
fn requirements_render_backfill_and_unsupported() {
    let mut scene = Scene::base();
    scene.plan.requirements = vec![
        PlanRequirementVm {
            kind: PlanRequirementKindVm::Backfill,
            table: "users".to_owned(),
            column: "status".to_owned(),
            reason: "Populate existing rows before enforcing NOT NULL.".to_owned(),
        },
        PlanRequirementVm {
            kind: PlanRequirementKindVm::Unsupported,
            table: "users".to_owned(),
            column: "meta".to_owned(),
            reason: "Operation is not supported by mysql".to_owned(),
        },
    ];
    let runtime = rt(scene);
    assert!(maybe(&runtime, "schema-diff-plan-requirements").is_some());
    // The backfill card: title, mono target, hint.
    assert_eq!(
        text_of(&runtime, "backfill-users.status-name"),
        "users.status"
    );
    let k = kit(runtime.state.dark);
    let card = maybe(&runtime, "backfill-users.status-Populate existing rows before enforcing NOT NULL.")
        .expect("backfill card");
    assert_eq!(card.fill, k.alpha(k.c.warning.fg, 0.10), "bg-yellow-500/10");
    // The unsupported banner: title + `target: reason`.
    assert_eq!(
        text_of(&runtime, "unsupported-users.meta-detail"),
        "users.meta: Operation is not supported by mysql"
    );
    // Both render ABOVE the statements list.
    let section = maybe(&runtime, "schema-diff-plan-requirements").expect("section");
    let statements = maybe(&runtime, "schema-diff-plan-statements").expect("statements");
    assert!(section.rect[1] < statements.rect[1], "requirements above statements");
}

/// The rollback status: partial (yellow, {count}) and available (green) —
/// the React test's two scenes.
#[test]
fn rollback_status_partial_and_available() {
    let mut scene = Scene::base();
    scene.plan.statements.push(statement(StatementRiskVm::Destructive));
    scene.plan.rollback_completeness =
        RollbackCompletenessVm { complete: false, missing: vec!["DROP INDEX".to_owned()] };
    let runtime = rt(scene);
    let expected = t("schemaDiff.rollback.partial").replace("{count}", "1");
    assert_eq!(text_of(&runtime, "schema-diff-rollback-status-text"), expected);
    let k = kit(runtime.state.dark);
    let line = maybe(&runtime, "schema-diff-rollback-status-text").expect("status text");
    assert_eq!(line.text_color, Some(k.c.warning.fg), "partial is yellow");

    let runtime = rt(Scene::base());
    assert_eq!(
        text_of(&runtime, "schema-diff-rollback-status-text"),
        t("schemaDiff.rollback.available")
    );
    let line = maybe(&runtime, "schema-diff-rollback-status-text").expect("status text");
    assert_eq!(line.text_color, Some(k.c.success.fg), "available is green");
}

/// The type-suggestions journey (the React test): the notice renders its
/// labels, the input carries the derived value, an edit lands in the
/// override slot, and Apply fires.
#[test]
fn type_suggestions_notice_journey() {
    let mut scene = Scene::base();
    scene.plan.type_suggestions = Some(vec![TypeSuggestionVm {
        table: "demo_customers".to_owned(),
        column: "region".to_owned(),
        source_type: "text".to_owned(),
        suggested_type: "VARCHAR(255)".to_owned(),
        current_type: "VARCHAR(255)".to_owned(),
        reason: "Indexed column; MySQL requires explicit key prefix length".to_owned(),
        is_key_or_indexed: true,
    }]);
    scene.edits.insert(
        "demo_customers.region".to_owned(),
        TextEdit::new("VARCHAR(255)".to_owned()),
    );
    let mut runtime = rt(scene);
    assert!(maybe(&runtime, "schema-diff-type-suggestions").is_some());
    assert_eq!(
        text_of(&runtime, "schema-diff-type-suggestions-title-text"),
        t("schemaDiff.typeSuggestions.title")
    );
    assert_eq!(
        text_of(&runtime, "schema-diff-type-suggestion-demo_customers.region-name"),
        "demo_customers.region"
    );
    assert_eq!(
        text_of(&runtime, "schema-diff-type-suggestion-demo_customers.region-source"),
        "text"
    );
    assert_eq!(
        text_of(&runtime, "schema-diff-type-suggestion-demo_customers-region-reason"),
        "Indexed column; MySQL requires explicit key prefix length"
    );
    // The derived input shows the current type.
    assert_eq!(
        text_under(&runtime, "type-suggestion-input-region"),
        "VARCHAR(255)"
    );
    // An override wins over the current type once the host mirrors it.
    runtime.update(|s, _| {
        s.edits.insert(
            "demo_customers.region".to_owned(),
            TextEdit::new("VARCHAR(64)".to_owned()),
        );
    });
    runtime.frame(0.0);
    assert_eq!(text_under(&runtime, "type-suggestion-input-region"), "VARCHAR(64)");
    // Apply fires the callback.
    press(&mut runtime, "schema-diff-apply-suggestions");
    assert_eq!(runtime.state.applies, 1);
}

/// The options row: both checkboxes toggle, Regenerate fires and disables
/// while regenerating.
#[test]
fn options_row_toggles_and_regenerates() {
    let mut runtime = rt(Scene::base());
    press(&mut runtime, "schema-diff-allow-destructive");
    press(&mut runtime, "schema-diff-include-indexes");
    press(&mut runtime, "schema-diff-regenerate");
    assert_eq!(runtime.state.toggles_allow, 1);
    assert_eq!(runtime.state.toggles_include, 1);
    assert_eq!(runtime.state.regenerations, 1);

    let mut scene = Scene::base();
    scene.regenerating = true;
    let mut runtime = rt(scene);
    press(&mut runtime, "schema-diff-regenerate");
    assert_eq!(runtime.state.regenerations, 0, "disabled while regenerating");
}

/// The cross-dialect note shows only for mixed dialects; the warnings box
/// renders its bullets; the risk chips colour by risk.
#[test]
fn note_warnings_and_risk_chips() {
    let mut scene = Scene::base();
    let runtime = rt(scene.clone());
    assert!(maybe(&runtime, "schema-diff-cross-dialect-note").is_some());
    assert_eq!(
        text_of(&runtime, "schema-diff-plan-dialects"),
        "postgres → mysql · 1 ".to_owned() + t("schemaDiff.statements")
    );

    scene.plan.same_dialect = true;
    let runtime = rt(scene);
    assert!(maybe(&runtime, "schema-diff-cross-dialect-note").is_none());

    let mut scene = Scene::base();
    scene.plan.warnings = vec!["w1".to_owned(), "w2".to_owned()];
    let runtime = rt(scene);
    assert_eq!(text_of(&runtime, "schema-diff-plan-warnings-title"), t("schemaDiff.warnings"));
    assert_eq!(text_of(&runtime, "schema-diff-plan-warning-w1"), "w1");

    // The statement card's risk chip: additive → success ink.
    let k = kit(runtime.state.dark);
    let chip = maybe(&runtime, "0-summary-risk").expect("risk chip");
    assert_eq!(chip.text_color, Some(k.c.success.fg));
    let card = maybe(&runtime, "0-summary").expect("statement card");
    assert_eq!(card.fill, k.c.layer_01, "bg-surface-alt");
}

/// The derived input value prefers the host override, then the current
/// type, over the suggested type (`??` chain).
#[test]
fn derived_value_prefers_override_then_current() {
    let mut scene = Scene::base();
    scene.plan.type_suggestions = Some(vec![TypeSuggestionVm {
        table: "t".to_owned(),
        column: "c".to_owned(),
        source_type: "text".to_owned(),
        suggested_type: "VARCHAR(255)".to_owned(),
        current_type: "VARCHAR(100)".to_owned(),
        reason: "r".to_owned(),
        is_key_or_indexed: false,
    }]);
    scene.plan.type_suggestions.as_mut().unwrap().push(TypeSuggestionVm {
        table: "t".to_owned(),
        column: "d".to_owned(),
        source_type: "text".to_owned(),
        suggested_type: "VARCHAR(255)".to_owned(),
        current_type: "VARCHAR(50)".to_owned(),
        reason: "r".to_owned(),
        is_key_or_indexed: false,
    });
    // No edit slot: the frozen branch shows each row's current type, not
    // the suggestion.
    let runtime = rt(scene.clone());
    assert_eq!(text_under(&runtime, "type-suggestion-input-c"), "VARCHAR(100)");
    assert_eq!(text_under(&runtime, "type-suggestion-input-d"), "VARCHAR(50)");
    // A host override for (t, c) wins for that row only.
    let mut scene = scene;
    scene.overrides = vec![ColumnTypeOverrideVm {
        table: "t".to_owned(),
        column: "c".to_owned(),
        target_type: "TEXT".to_owned(),
    }];
    let runtime = rt(scene);
    assert_eq!(text_under(&runtime, "type-suggestion-input-c"), "TEXT");
    assert_eq!(
        text_under(&runtime, "type-suggestion-input-d"),
        "VARCHAR(50)",
        "the other row keeps its current type"
    );
}

/// The Apply button stays inert while regenerating.
#[test]
fn apply_is_disabled_while_regenerating() {
    let mut scene = Scene::base();
    scene.plan.type_suggestions = Some(vec![TypeSuggestionVm {
        table: "t".to_owned(),
        column: "c".to_owned(),
        source_type: "text".to_owned(),
        suggested_type: "VARCHAR(255)".to_owned(),
        current_type: "VARCHAR(255)".to_owned(),
        reason: "r".to_owned(),
        is_key_or_indexed: false,
    }]);
    scene.regenerating = true;
    let mut runtime = rt(scene);
    press(&mut runtime, "schema-diff-apply-suggestions");
    assert_eq!(runtime.state.applies, 0, "disabled while regenerating");
}

/// A no-statements plan renders no rollback line (React's null branch).
#[test]
fn no_statements_means_no_rollback_line() {
    let mut scene = Scene::base();
    scene.plan.statements.clear();
    let runtime = rt(scene);
    assert!(maybe(&runtime, "schema-diff-rollback-status").is_none());
}
