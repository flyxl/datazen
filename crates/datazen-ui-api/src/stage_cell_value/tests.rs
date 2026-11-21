use std::collections::BTreeMap;

use serde_json::json;

use super::*;

fn identity() -> RowIdentity {
    BTreeMap::from([("id".to_owned(), json!(1))])
}

fn change_with(columns: &[&str]) -> PendingRowChange {
    let mut original_values = ValueMap::new();
    let mut current_values = ValueMap::new();
    for column in columns {
        original_values.insert((*column).to_owned(), json!("old"));
        current_values.insert((*column).to_owned(), json!("new"));
    }
    PendingRowChange {
        row_index: Some(3),
        row_identity: identity(),
        original_values,
        current_values,
        changed_columns: columns.iter().map(|column| (*column).to_owned()).collect(),
        delete_marked: false,
        insert_marked: None,
        expression_values: None,
    }
}

fn cell_input(
    value: Option<Value>,
    existing: Option<PendingRowChange>,
    column: &str,
    original: Option<Value>,
) -> StageCellValueInput {
    StageCellValueInput {
        existing,
        column: column.to_owned(),
        value,
        original_value: original,
        row_identity: identity(),
        row_index: 4,
    }
}

#[test]
fn a_literal_write_stages_a_different_value() {
    let result = compute_stage_cell_value_result(&cell_input(
        Some(json!("new")),
        None,
        "name",
        Some(json!("old")),
    ));
    let change = result.change.expect_change();
    assert_eq!(change.changed_columns, ["name"]);
    assert_eq!(change.original_values["name"], json!("old"));
    assert_eq!(change.current_values["name"], json!("new"));
    assert_eq!(change.row_index, Some(4));
    assert_eq!(change.row_identity, identity());
    assert!(!change.delete_marked);
}

trait ExpectChange {
    fn expect_change(self) -> PendingRowChange;
}

impl ExpectChange for Option<PendingRowChange> {
    fn expect_change(self) -> PendingRowChange {
        match self {
            Some(change) => change,
            None => panic!("expected a staged change"),
        }
    }
}

#[test]
fn a_reverted_value_leaves_the_row_clean() {
    let existing = Some(change_with(&["name"]));
    let result = compute_stage_cell_value_result(&cell_input(
        Some(json!("old")),
        existing,
        "name",
        Some(json!("old")),
    ));
    assert_eq!(result.change, None);
    assert!(result.changed_columns.is_empty());
}

#[test]
fn sql_null_is_a_value_and_stays_distinct_from_an_empty_string() {
    let result = compute_stage_cell_value_result(&cell_input(
        Some(Value::Null),
        None,
        "name",
        Some(json!("")),
    ));
    let change = result.change.expect_change();
    assert_eq!(change.current_values["name"], Value::Null);
    assert_eq!(change.original_values["name"], json!(""));
    assert!(has_pending_column_change(Some(&change), "name"));
}

#[test]
fn undefined_removes_the_column_and_clears_a_lone_staged_change() {
    let result = compute_stage_cell_value_result(&cell_input(
        None,
        Some(change_with(&["name"])),
        "name",
        Some(json!("old")),
    ));
    assert_eq!(result.change, None);
    assert!(result.changed_columns.is_empty());
}

#[test]
fn undefined_keeps_the_row_when_other_columns_are_still_changed() {
    let result = compute_stage_cell_value_result(&cell_input(
        None,
        Some(change_with(&["name", "age"])),
        "name",
        Some(json!("old")),
    ));
    let change = result.change.expect_change();
    assert_eq!(change.changed_columns, ["age"]);
    assert!(!change.original_values.contains_key("name"));
    assert!(!change.current_values.contains_key("name"));
    assert_eq!(result.changed_columns, ["age"]);
}

#[test]
fn a_delete_marked_row_keeps_its_entry_after_every_revert() {
    let mut existing = change_with(&["name"]);
    existing.delete_marked = true;
    let result = compute_stage_cell_value_result(&cell_input(
        None,
        Some(existing),
        "name",
        Some(json!("old")),
    ));
    let change = result.change.expect_change();
    assert!(change.changed_columns.is_empty());
    assert!(change.delete_marked);
}

#[test]
fn a_literal_write_drops_a_staged_expression_on_that_column() {
    let mut existing = change_with(&["name"]);
    existing.expression_values = Some(BTreeMap::from([(
        "name".to_owned(),
        WriteExpressionId::CurrentTimestamp,
    )]));
    let result = compute_stage_cell_value_result(&cell_input(
        Some(json!("literal")),
        Some(existing),
        "name",
        Some(json!("old")),
    ));
    assert_eq!(result.change.expect_change().expression_values, None);
}

#[test]
fn an_expression_stages_a_column_with_an_original_placeholder() {
    let input = StageExpressionValueInput {
        existing: None,
        column: "updated_at".to_owned(),
        original_value: Some(Value::Null),
        row_identity: identity(),
        row_index: 9,
    };
    let result = compute_stage_expression_value_result(&input);
    let change = result.change.expect_change();
    assert_eq!(change.changed_columns, ["updated_at"]);
    assert_eq!(change.original_values["updated_at"], Value::Null);
    assert_eq!(change.current_values["updated_at"], Value::Null);
    assert_eq!(
        change.expression_values.map(|values| values["updated_at"]),
        Some(WriteExpressionId::CurrentTimestamp)
    );
    assert_eq!(
        WriteExpressionId::CurrentTimestamp.as_str(),
        "currentTimestamp"
    );
}

#[test]
fn an_expression_preserves_existing_fields_and_other_expressions() {
    let mut existing = change_with(&["name", "other"]);
    existing.insert_marked = Some(true);
    existing.expression_values = Some(BTreeMap::from([(
        "other".to_owned(),
        WriteExpressionId::CurrentTimestamp,
    )]));
    let input = StageExpressionValueInput {
        existing: Some(existing),
        column: "name".to_owned(),
        original_value: Some(json!("old")),
        row_identity: identity(),
        row_index: 2,
    };
    let change = compute_stage_expression_value_result(&input)
        .change
        .expect_change();
    assert_eq!(change.changed_columns, ["name", "other"]);
    assert_eq!(change.current_values["name"], json!("new"));
    assert_eq!(change.insert_marked, Some(true));
    let expressions = change.expression_values.expect_expressions();
    assert_eq!(expressions.len(), 2);
    assert_eq!(expressions["name"], WriteExpressionId::CurrentTimestamp);
    assert_eq!(expressions["other"], WriteExpressionId::CurrentTimestamp);
}

trait ExpectExpressions {
    fn expect_expressions(self) -> BTreeMap<String, WriteExpressionId>;
}

impl ExpectExpressions for Option<BTreeMap<String, WriteExpressionId>> {
    fn expect_expressions(self) -> BTreeMap<String, WriteExpressionId> {
        match self {
            Some(values) => values,
            None => panic!("expected expression values"),
        }
    }
}

#[test]
fn a_revert_keeps_expressions_of_other_columns() {
    let mut existing = change_with(&["name", "other"]);
    existing.expression_values = Some(BTreeMap::from([
        ("name".to_owned(), WriteExpressionId::CurrentTimestamp),
        ("other".to_owned(), WriteExpressionId::CurrentTimestamp),
    ]));
    let result = compute_stage_cell_value_result(&cell_input(
        None,
        Some(existing),
        "name",
        Some(json!("old")),
    ));
    let change = result.change.expect_change();
    let expressions = change.expression_values.expect_expressions();
    assert_eq!(expressions.len(), 1);
    assert!(expressions.contains_key("other"));
}

#[test]
fn restored_value_comes_from_the_original_map_only_for_changed_columns() {
    assert_eq!(restored_cell_value(None, "name"), None);
    let change = change_with(&["name"]);
    assert_eq!(restored_cell_value(Some(&change), "age"), None);
    assert_eq!(
        restored_cell_value(Some(&change), "name"),
        Some(json!("old"))
    );
    let mut missing = change_with(&["name"]);
    missing.original_values.remove("name");
    assert_eq!(restored_cell_value(Some(&missing), "name"), None);
}

#[test]
fn pending_change_checks_membership_expression_then_value_equality() {
    assert!(!has_pending_column_change(None, "name"));
    let change = change_with(&["name"]);
    assert!(!has_pending_column_change(Some(&change), "age"));
    assert!(has_pending_column_change(Some(&change), "name"));

    let mut equal = change.clone();
    equal.current_values.insert("name".to_owned(), json!("old"));
    assert!(!has_pending_column_change(Some(&equal), "name"));

    let mut expressed = equal;
    expressed.expression_values = Some(BTreeMap::from([(
        "name".to_owned(),
        WriteExpressionId::CurrentTimestamp,
    )]));
    assert!(has_pending_column_change(Some(&expressed), "name"));
}

#[test]
fn values_equal_uses_the_canonical_json_form() {
    assert!(values_equal(None, None));
    assert!(values_equal(Some(&Value::Null), None));
    assert!(values_equal(
        Some(&json!({"b": 2, "a": 1})),
        Some(&json!({"a": 1, "b": 2}))
    ));
    assert!(!values_equal(Some(&json!(1)), Some(&json!("1"))));
    assert!(!values_equal(Some(&json!("")), Some(&Value::Null)));
}

#[test]
fn stage_edit_rebuilds_the_change_without_carrying_an_expression() {
    let mut existing = change_with(&["name"]);
    existing.expression_values = Some(BTreeMap::from([(
        "name".to_owned(),
        WriteExpressionId::CurrentTimestamp,
    )]));
    existing.insert_marked = Some(true);
    let result = compute_stage_result(&StageEditInput {
        existing: Some(existing),
        column: "name".to_owned(),
        original_value: Some(json!("old")),
        current_value: Some(json!("newer")),
        row_identity: identity(),
        row_index: 7,
    });
    let change = result.change.expect_change();
    assert_eq!(change.row_index, Some(7));
    assert_eq!(change.current_values["name"], json!("newer"));
    assert_eq!(change.expression_values, None);
    assert_eq!(change.insert_marked, None);
}

#[test]
fn gap_and_surface_are_pinned_and_root_reexports_match() {
    assert_eq!(STAGE_CELL_VALUE_SURFACE.len(), 9);
    assert!(STAGE_CELL_VALUE_SURFACE.contains(&"computeStageCellValueResult"));
    assert_eq!(STAGE_CELL_VALUE_GAP.len(), 3);
    assert_eq!(crate::STAGE_CELL_VALUE_SURFACE.len(), 9);
    assert_eq!(crate::STAGE_CELL_VALUE_GAP.len(), 3);
    let _: Option<crate::PendingRowChange> = Some(change_with(&[]));
}
