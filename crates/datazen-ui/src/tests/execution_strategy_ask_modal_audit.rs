//! Independent audit for T501: `ExecutionStrategyAskModal` port.
//!
//! Fill branch coverage the coder does not drive: preview boundaries, the
//! missing-current card, and zero statement counts.

use crate::i18n::t;
use crate::views::query::execution_strategy_ask_modal::{
    current_label, entire_label, preview, ExecutionStrategyAskVm, StatementTargetVm,
};

#[test]
fn preview_boundaries_match_the_react_slice_rule() {
    assert_eq!(preview(""), "", "empty SQL has no preview and no ellipsis");
    let exact = "x".repeat(120);
    assert_eq!(preview(&exact), exact, "exactly 120 chars is not truncated");
    let long = "y".repeat(121);
    assert_eq!(preview(&long), format!("{}…", "y".repeat(120)), "the 121st char becomes an ellipsis");
}

#[test]
fn missing_current_statement_yields_an_empty_label() {
    let mut vm = ExecutionStrategyAskVm {
        open: true,
        statement_count: 0,
        entire_script: "SELECT 1".to_owned(),
        ..Default::default()
    };
    assert!(vm.current_statement.is_none());
    assert_eq!(current_label(&vm), "", "React renders no current card/label");
    assert_eq!(
        entire_label(&vm),
        t("query.executionStrategy.runEntire").replace("{count}", "0")
    );

    vm.current_statement = Some(StatementTargetVm {
        sql: String::new(),
        from_line: 7,
        to_line: 9,
    });
    // An empty SQL preview is still a card; the label uses the line range.
    assert_eq!(
        current_label(&vm),
        t("query.executionStrategy.runCurrent")
            .replace("{from}", "7")
            .replace("{to}", "9")
    );
    assert_eq!(preview(""), "");
}
