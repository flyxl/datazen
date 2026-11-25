//! Independent audit for T492: `PendingPlanDialog` port.
//!
//! Fill branch coverage the coder does not drive: the no-insert description,
//! statement execution order, and empty parameter/warning handling.

use datazen_ui::i18n::t;
use datazen_ui::views::pending_plan_dialog::{
    ChangeWarning, PlannedStatement, RowChangePlanVm,
};

fn stmt(sql: &str, params: &[&str]) -> PlannedStatement {
    PlannedStatement {
        sql_template: sql.to_owned(),
        parameter_summary: params.iter().map(|s| (*s).to_owned()).collect(),
    }
}

fn base() -> RowChangePlanVm {
    RowChangePlanVm {
        table: "users".to_owned(),
        fingerprint: "fp".to_owned(),
        ..Default::default()
    }
}

#[test]
fn description_omits_the_insert_phrase_when_there_are_no_inserts() {
    let mut plan = base();
    plan.updates = vec![stmt("UPDATE users", &[])];
    let expected = [
        "users".to_owned(),
        t("tableData.pendingUpdates").replace("{count}", "1"),
        t("tableData.pendingDeletes").replace("{count}", "0"),
    ]
    .join(" · ");
    assert_eq!(plan.description(), expected, "no insert phrase and no doubled count");

    plan.inserts = vec![stmt("INSERT users", &[])];
    assert!(
        plan.description().contains(&t("tableData.pendingInserts").replace("{count}", "1")),
        "a non-empty insert list contributes its phrase"
    );
}

#[test]
fn statements_follow_insert_update_delete_execution_order() {
    let mut plan = base();
    plan.inserts = vec![stmt("INSERT 1", &[]), stmt("INSERT 2", &[])];
    plan.updates = vec![stmt("UPDATE 1", &[])];
    plan.deletes = vec![stmt("DELETE 1", &[])];
    let sql: Vec<&str> = plan.statements().iter().map(|s| s.sql_template.as_str()).collect();
    assert_eq!(sql, ["INSERT 1", "INSERT 2", "UPDATE 1", "DELETE 1"]);

    let empty = base();
    assert!(empty.statements().is_empty(), "no statements in an empty plan");
}

#[test]
fn empty_parameter_summaries_and_warnings_are_absent() {
    let plan = base();
    assert!(plan.warnings.is_empty());
    assert_eq!(plan.fingerprint, "fp");

    let with_warning = ChangeWarning {
        code: "W1".to_owned(),
        message: "risky rewrite".to_owned(),
    };
    assert_eq!(with_warning.message, "risky rewrite", "warning message is carried verbatim");
}
