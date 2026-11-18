//! Independent audit for T025 (the SchemaDiffDeployPanel Rune port).
//!
//! Tester-written: the coder's module is treated as a black box and every
//! assertion below comes from re-reading
//! `packages/frontend/src/windows/schema-diff/SchemaDiffDeployPanel.tsx`,
//! its helpers in `commands/schemaDiff.ts` / `lib/schemaDiffConfirm.ts`, and
//! the React tests in `__tests__/SchemaDiffPanels.test.tsx`: the summary
//! box, the forced transaction control for SQLite rebuilds, the dialect
//! gate, the rollback/confirm journey that keeps the deploy button closed,
//! and the committed/failed receipts. Element evidence comes from
//! `Runtime::elements()` (runtime layout), never from a value the component
//! reports about itself; presses go through real pointer events.
//!
//! The pure evaluation (`deploy_readiness` and its helper copies) is
//! additionally mutation-proved out-of-tree (the scratch crate, runbook
//! §3.4); this file pins the same behaviour at the runtime level plus the
//! callback wiring.

use rune::{Cx, El, Env, Runtime, TextEdit};

use crate::views::schema_diff::schema_diff_deploy_panel::{
    DESTRUCTIVE_CONFIRM_TOKEN, DeployStatusVm, PlanRequirementKindVm, PlanRequirementVm,
    PlanStatementVm, RollbackCompletenessVm, SchemaDiffDeployPanel, SchemaDiffDeployResultVm,
    SchemaDiffPlanVm, StatementRiskVm, can_confirm_destructive_deploy, can_run_deploy,
    deploy_readiness, dialect_supports_transactional_ddl, plan_has_destructive,
};

fn statement(risk: StatementRiskVm, requires_tx: bool) -> PlanStatementVm {
    PlanStatementVm {
        sql: format!("ALTER TABLE users {}", match risk {
            StatementRiskVm::Additive => "ADD COLUMN email text",
            StatementRiskVm::Destructive => "DROP COLUMN email",
            StatementRiskVm::Rewrite => "ALTER COLUMN email TYPE text",
        }),
        risk,
        rollback_sql: None,
        summary: "s".to_owned(),
        requires_transaction: requires_tx,
    }
}

/// The React test's `samplePlan`: postgres, one additive statement, complete
/// rollback, no requirements.
fn sample_plan() -> SchemaDiffPlanVm {
    SchemaDiffPlanVm {
        tables: vec!["users".to_owned()],
        target_dialect: "postgres".to_owned(),
        statements: vec![statement(StatementRiskVm::Additive, false)],
        requirements: Vec::new(),
        rollback_completeness: RollbackCompletenessVm { complete: true, missing: Vec::new() },
    }
}

fn result(status: DeployStatusVm, executed: usize, total: usize, errors: &[&str]) -> SchemaDiffDeployResultVm {
    SchemaDiffDeployResultVm {
        status,
        executed_count: executed,
        statement_count: total,
        errors: errors.iter().map(|s| s.to_string()).collect(),
    }
}

// ---- the pure helpers (1:1 copies of the React layer) ----------------------

#[test]
fn destructive_detects_destructive_and_rewrite_risks() {
    let mut plan = sample_plan();
    assert!(!plan_has_destructive(&plan), "additive is not destructive");
    plan.statements[0].risk = StatementRiskVm::Destructive;
    assert!(plan_has_destructive(&plan));
    plan.statements[0].risk = StatementRiskVm::Rewrite;
    assert!(plan_has_destructive(&plan), "rewrite counts as destructive");
}

#[test]
fn transactional_ddl_is_postgres_and_sqlite_only() {
    for dialect in ["postgresql", "postgres", "sqlite", "PostgreSQL", "SQLite"] {
        assert!(dialect_supports_transactional_ddl(dialect), "{dialect}");
    }
    for dialect in ["mysql", "mariadb", "oracle", "sqlserver", ""] {
        assert!(!dialect_supports_transactional_ddl(dialect), "{dialect}");
    }
}

#[test]
fn the_confirm_token_is_deploy() {
    assert_eq!(DESTRUCTIVE_CONFIRM_TOKEN, "DEPLOY");
    // canConfirmDestructiveDeploy: non-destructive always passes; the token
    // must be typed exactly (trimmed).
    assert!(can_confirm_destructive_deploy("anything", false));
    assert!(can_confirm_destructive_deploy("DEPLOY", true));
    assert!(can_confirm_destructive_deploy("  DEPLOY  ", true), "trimmed");
    assert!(!can_confirm_destructive_deploy("deploy", true), "case-sensitive");
    assert!(!can_confirm_destructive_deploy("", true));
}

#[test]
fn can_run_deploy_gates_on_statements_rollback_and_token() {
    // statementCount === 0 blocks everything.
    assert!(!can_run_deploy(true, "DEPLOY", false, true, 0));
    // An incomplete rollback with requireRollback blocks.
    assert!(!can_run_deploy(true, "DEPLOY", true, false, 1));
    // Complete rollback + token passes.
    assert!(can_run_deploy(true, "DEPLOY", true, true, 1));
    // Wrong token blocks a destructive plan.
    assert!(!can_run_deploy(true, "wrong", false, false, 1));
}

#[test]
fn deploy_readiness_matches_reacts_derivation() {
    // The React test's baseline: postgres + tx + requireRollback -> enabled.
    let plan = sample_plan();
    let r = deploy_readiness(&plan, true, true, "", false);
    assert!(r.can_run);
    assert!(r.tx_supported);
    assert!(!r.transaction_required);
    assert!(r.transaction_enabled);

    // useTransaction=false drops transactionEnabled, and with
    // requireRollback the gate closes (the React rerender test).
    let r = deploy_readiness(&plan, false, true, "", false);
    assert!(!r.transaction_enabled);
    assert!(!r.can_run);

    // A mysql target cannot carry the transaction gate either.
    let mut mysql = sample_plan();
    mysql.target_dialect = "mysql".to_owned();
    let r = deploy_readiness(&mysql, true, true, "", false);
    assert!(!r.tx_supported);
    assert!(!r.can_run);

    // A delivered result closes the button regardless.
    let r = deploy_readiness(&plan, true, true, "", true);
    assert!(!r.can_run);

    // A SQLite table rebuild forces the transaction control and keeps the
    // button enabled even with useTransaction=false.
    let mut sqlite = sample_plan();
    sqlite.target_dialect = "sqlite".to_owned();
    sqlite.statements[0].requires_transaction = true;
    let r = deploy_readiness(&sqlite, false, false, "", false);
    assert!(r.transaction_required);
    assert!(r.transaction_enabled, "forced on");
    assert!(r.can_run, "the React SQLite-rebuild test enables deploy");

    // An Unsupported requirement makes the plan deployless; Backfill does not.
    let mut unsupported = sample_plan();
    unsupported.requirements = vec![PlanRequirementVm {
        kind: PlanRequirementKindVm::Unsupported,
        table: "users".to_owned(),
        column: "email".to_owned(),
        reason: "no way".to_owned(),
    }];
    assert!(!deploy_readiness(&unsupported, true, false, "", false).can_run);
    unsupported.requirements[0].kind = PlanRequirementKindVm::Backfill;
    assert!(deploy_readiness(&unsupported, true, false, "", false).can_run);

    // A destructive statement requires the token.
    let mut destructive = sample_plan();
    destructive.statements[0].risk = StatementRiskVm::Destructive;
    assert!(!deploy_readiness(&destructive, true, false, "", false).can_run);
    assert!(deploy_readiness(&destructive, true, false, "DEPLOY", false).can_run);
    // No statements at all: nothing to deploy.
    // A statement that requires a transaction on a dialect without
    // transactional DDL is undeployable (React's `!transactionRequired
    // || (txSupported && transactionEnabled)` clause).
    let mut stranded = sample_plan();
    stranded.target_dialect = "mysql".to_owned();
    stranded.statements[0].requires_transaction = true;
    let r = deploy_readiness(&stranded, false, false, "", false);
    assert!(r.transaction_required);
    assert!(!r.tx_supported);
    assert!(!r.can_run, "the rebuild cannot run without transactional DDL");

    let mut empty = sample_plan();
    empty.statements.clear();
    assert!(!deploy_readiness(&empty, true, false, "", false).can_run);
}

// ---- runtime evidence -------------------------------------------------------

#[derive(Clone)]
struct Scene {
    plan: SchemaDiffPlanVm,
    target_label: &'static str,
    use_transaction: bool,
    require_rollback: bool,
    confirm: TextEdit,
    deploying: bool,
    result: Option<SchemaDiffDeployResultVm>,
    dark: bool,
    tx_toggles: Vec<bool>,
    rb_toggles: Vec<bool>,
    deploys: usize,
}

impl Scene {
    fn base() -> Self {
        Self {
            plan: sample_plan(),
            target_label: "demo_db",
            use_transaction: true,
            require_rollback: false,
            confirm: TextEdit::new(String::new()),
            deploying: false,
            result: None,
            dark: true,
            tx_toggles: Vec::new(),
            rb_toggles: Vec::new(),
            deploys: 0,
        }
    }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    SchemaDiffDeployPanel::<Scene>::new(
        "schema-diff-deploy-panel",
        &s.plan,
        s.target_label,
        s.use_transaction,
        s.require_rollback,
        &s.confirm,
        s.deploying,
        s.result.as_ref(),
    )
    .on_use_transaction(|s: &mut Scene| {
        let next = !s.use_transaction;
        s.tx_toggles.push(next);
        s.use_transaction = next;
    })
    .on_require_rollback(|s: &mut Scene| {
        let next = !s.require_rollback;
        s.rb_toggles.push(next);
        s.require_rollback = next;
    })
    .on_confirm_edit(|s: &mut Scene| &mut s.confirm)
    .on_deploy(|s: &mut Scene| {
        s.deploys += 1;
    })
    .build(s.dark)
}

fn rt(scene: Scene) -> Runtime<Scene> {
    let mut rt = Runtime::new(
        scene,
        view,
        (960.0, 640.0),
        Env {
            system_dark: true,
            scale: 1.0,
        },
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
    let point = rt
        .find(key)
        .unwrap_or_else(|| panic!("{key} is laid out and pressable"));
    rt.pointer_down(point.0, point.1);
    rt.pointer_up(point.0, point.1);
    rt.frame(0.0);
    rt.frame(0.0);
}

fn deploy_enabled(rt: &Runtime<Scene>) -> bool {
    maybe(rt, "schema-diff-deploy").map(|e| e.focusable).unwrap_or(false)
}

#[test]
fn the_summary_box_shows_target_tables_and_count() {
    let rt = rt(Scene::base());
    assert!(maybe(&rt, "schema-diff-deploy-panel-summary").is_some());
    assert!(text_of(&rt, "schema-diff-deploy-panel-summary-target").contains("demo_db"));
    assert!(text_of(&rt, "schema-diff-deploy-panel-summary-tables").contains("users"));
    assert!(text_of(&rt, "schema-diff-deploy-panel-summary-statements").contains("1"));
}

#[test]
fn a_committed_result_renders_the_receipt_without_errors() {
    // React test: status 'committed', 4/4, no errors list.
    let mut scene = Scene::base();
    scene.result = Some(result(DeployStatusVm::Committed, 4, 4, &[]));
    let rt = rt(scene);
    let status = text_of(&rt, "schema-diff-deploy-status");
    assert!(status.contains("committed"), "wire string verbatim: {status}");
    assert!(text_of(&rt, "schema-diff-deploy-count").contains("4/4"));
    assert!(maybe(&rt, "schema-diff-deploy-errors").is_none());
}

#[test]
fn a_failed_result_lists_its_errors() {
    let mut scene = Scene::base();
    scene.result = Some(result(
        DeployStatusVm::Failed,
        1,
        4,
        &["Query failed: Multiple primary key defined"],
    ));
    let rt = rt(scene);
    assert!(text_of(&rt, "schema-diff-deploy-status").contains("failed"));
    assert!(text_of(&rt, "schema-diff-deploy-count").contains("1/4"));
    let errors = maybe(&rt, "schema-diff-deploy-errors").expect("the errors list");
    assert!(errors.text.unwrap_or_default().contains("Multiple primary key defined"));
}

#[test]
fn the_deploy_button_blocks_and_reopens_across_the_journey() {
    // The React rerender journey: enabled -> tx off (with rollback required)
    // disabled -> mysql disabled -> back enabled -> result disabled.
    let mut scene = Scene::base();
    scene.require_rollback = true;
    let baseline = rt(scene.clone());
    assert!(deploy_enabled(&baseline), "baseline enabled");

    scene.use_transaction = false;
    let tx_off = rt(scene.clone());
    assert!(!deploy_enabled(&tx_off), "transaction off closes the gate");

    scene.plan.target_dialect = "mysql".to_owned();
    let mysql = rt(scene.clone());
    assert!(!deploy_enabled(&mysql), "mysql closes the gate");

    scene.plan.target_dialect = "postgres".to_owned();
    scene.use_transaction = true;
    let restored = rt(scene.clone());
    assert!(deploy_enabled(&restored), "back to the baseline");

    scene.result = Some(result(DeployStatusVm::Unknown, 1, 1, &["COMMIT outcome unknown"]));
    let with_result = rt(scene);
    assert!(!deploy_enabled(&with_result), "a result closes the button");
    assert!(text_of(&with_result, "schema-diff-deploy-status").contains("unknown"));
}

#[test]
fn a_sqlite_rebuild_forces_the_transaction_control() {
    // React test: the checkbox is checked, disabled and shows the required
    // hint; deploy stays enabled.
    let mut scene = Scene::base();
    scene.plan.target_dialect = "sqlite".to_owned();
    scene.plan.statements[0].requires_transaction = true;
    scene.use_transaction = false;
    scene.require_rollback = false;
    let rt = rt(scene);
    let checkbox = maybe(&rt, "schema-diff-deploy-panel-transaction").expect("the checkbox");
    assert!(!checkbox.focusable, "disabled while forced");
    assert!(
        maybe(&rt, "schema-diff-deploy-panel-required-hint").is_some(),
        "the (transactionRequired) hint shows"
    );
    assert!(deploy_enabled(&rt), "deploy enabled for the reviewed rebuild");
}

#[test]
fn an_unsupported_dialect_shows_the_unsupported_hint() {
    let mut scene = Scene::base();
    scene.plan.target_dialect = "mysql".to_owned();
    let rt = rt(scene);
    assert!(
        maybe(&rt, "schema-diff-deploy-panel-unsupported-hint").is_some(),
        "the (txUnsupported) hint shows for mysql"
    );
    assert!(maybe(&rt, "schema-diff-deploy-panel-required-hint").is_none());
}

#[test]
fn the_destructive_journey_forwards_edits_and_presses() {
    // The React tester journey: destructive plan + rollback required.
    let mut scene = Scene::base();
    scene.plan.statements[0].risk = StatementRiskVm::Destructive;
    scene.require_rollback = true;
    let untokenced = rt(scene.clone());
    assert!(!deploy_enabled(&untokenced), "no token yet");
    assert!(maybe(&untokenced, "schema-diff-deploy-panel-confirm-input").is_some());

    // The confirm field carries the DEPLOY placeholder; the host types it.
    scene.confirm = TextEdit::new(DESTRUCTIVE_CONFIRM_TOKEN.to_owned());
    scene.use_transaction = false; // React's rerender keeps it closed…
    let tx_off = rt(scene.clone());
    assert!(!deploy_enabled(&tx_off), "tx off still closes the gate");

    scene.use_transaction = true;
    let mut rt = rt(scene.clone());
    assert!(deploy_enabled(&rt), "token + tx opens the gate");

    press(&mut rt, "schema-diff-deploy");
    assert_eq!(rt.state.deploys, 1, "the deploy action fired once");

    // The checkbox toggles forward the flipped values.
    press(&mut rt, "schema-diff-deploy-panel-transaction");
    assert_eq!(rt.state.tx_toggles.last(), Some(&false), "forwarded false");
    press(&mut rt, "schema-diff-deploy-panel-rollback");
    assert_eq!(rt.state.rb_toggles.last(), Some(&false), "forwarded false");
}

#[test]
fn the_incomplete_rollback_banner_only_while_required_and_incomplete() {
    let mut scene = Scene::base();
    scene.require_rollback = true;
    scene.plan.rollback_completeness = RollbackCompletenessVm {
        complete: false,
        missing: vec!["DROP COLUMN email".to_owned()],
    };
    let required = rt(scene.clone());
    let banner = maybe(&required, "schema-diff-deploy-panel-rollback-incomplete").expect("the banner");
    assert!(banner.text.unwrap_or_default().contains("DROP COLUMN email"));
    assert!(!deploy_enabled(&required), "incomplete rollback blocks");

    // Not required: the banner is gone (and the gate no longer blocks).
    scene.require_rollback = false;
    let optional = rt(scene);
    assert!(maybe(&optional, "schema-diff-deploy-panel-rollback-incomplete").is_none());
}

#[test]
fn the_deploying_state_relabels_and_blocks_the_button() {
    let mut scene = Scene::base();
    scene.deploying = true;
    let rt = rt(scene);
    let label = text_of(&rt, "schema-diff-deploy");
    assert!(label.contains("正在部署"), "t('schemaDiff.deploying'): {label}");
    assert!(!deploy_enabled(&rt), "deploying blocks the button");
}
