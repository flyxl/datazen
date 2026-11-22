use crate::{compute_stage_result, PendingRowChange};

pub const STAGE_EDIT_ACTIONS_SURFACE: [&str; 4] = [
    "computeStageResult",
    "computeUnstageDeleteResult",
    "StageEditResult",
    "STAGE_EDIT_ACTIONS_GAP",
];

pub const STAGE_EDIT_ACTIONS_GAP: [&str; 3] = [
    "the store action that feeds StageEditInput and applies StageEditResult",
    "removing the pendingChanges key when the returned change is null",
    "computed results are pure; the store owns the write",
];

pub fn compute_unstage_delete_result(change: &PendingRowChange) -> Option<PendingRowChange> {
    if change.changed_columns.is_empty() {
        return None;
    }
    let mut next = change.clone();
    next.delete_marked = false;
    Some(next)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde_json::json;

    use super::*;
    use crate::{RowIdentity, StageEditInput, ValueMap, WriteExpressionId};

    fn identity() -> RowIdentity {
        let mut identity = RowIdentity::new();
        identity.insert("id".to_owned(), json!(7));
        identity
    }

    fn stage_identity() -> ValueMap {
        let mut identity = ValueMap::new();
        identity.insert("id".to_owned(), json!(7));
        identity
    }

    fn marked_change(changed_columns: Vec<String>) -> PendingRowChange {
        let mut original_values = HashMap::new();
        original_values.insert("qty".to_owned(), json!(1));
        let mut current_values = HashMap::new();
        current_values.insert("qty".to_owned(), json!(2));
        let mut expression_values = HashMap::new();
        expression_values.insert("when".to_owned(), WriteExpressionId::CurrentTimestamp);
        PendingRowChange {
            row_index: Some(3),
            row_identity: identity(),
            original_values,
            current_values,
            changed_columns,
            delete_marked: true,
            insert_marked: Some(false),
            expression_values: Some(expression_values),
        }
    }

    #[test]
    fn delete_only_rows_leave_pending_changes() {
        let change = marked_change(Vec::new());
        assert_eq!(compute_unstage_delete_result(&change), None);
    }

    #[test]
    fn rows_with_column_changes_keep_them_and_lose_only_the_delete_mark() {
        let change = marked_change(vec!["qty".to_owned()]);
        let next = compute_unstage_delete_result(&change).expect("change survives");
        assert!(!next.delete_marked);
        assert_eq!(next.changed_columns, ["qty"]);
        assert_eq!(next.row_identity, change.row_identity);
        assert_eq!(next.original_values, change.original_values);
        assert_eq!(next.current_values, change.current_values);
        assert_eq!(next.row_index, change.row_index);
        assert_eq!(next.insert_marked, change.insert_marked);
        assert_eq!(next.expression_values, change.expression_values);
        assert!(change.delete_marked, "input is unchanged");
    }

    #[test]
    fn surface_and_gap_are_pinned_and_root_reachable() {
        assert_eq!(STAGE_EDIT_ACTIONS_SURFACE.len(), 4);
        assert!(STAGE_EDIT_ACTIONS_SURFACE.contains(&"computeStageResult"));
        assert!(STAGE_EDIT_ACTIONS_SURFACE.contains(&"computeUnstageDeleteResult"));
        assert_eq!(STAGE_EDIT_ACTIONS_GAP.len(), 3);
        assert_eq!(crate::STAGE_EDIT_ACTIONS_SURFACE.len(), 4);
        assert_eq!(crate::STAGE_EDIT_ACTIONS_GAP.len(), 3);
    }

    #[test]
    fn stage_result_is_reused_not_reimplemented() {
        let input = StageEditInput {
            existing: None,
            column: "qty".to_owned(),
            original_value: Some(json!(1)),
            current_value: Some(json!(2)),
            row_identity: stage_identity(),
            row_index: 4,
        };
        let result = compute_stage_result(&input);
        assert_eq!(result.changed_columns, ["qty"]);
        let change = result.change.expect("staged change exists");
        assert_eq!(change.row_index, Some(4));
        assert_eq!(change.current_values["qty"], json!(2));
        assert!(!change.delete_marked);
    }

    #[test]
    fn reverting_the_only_column_clears_the_change() {
        let input = StageEditInput {
            existing: None,
            column: "qty".to_owned(),
            original_value: Some(json!(1)),
            current_value: Some(json!(1)),
            row_identity: stage_identity(),
            row_index: 4,
        };
        let result = compute_stage_result(&input);
        assert!(result.change.is_none());
        assert!(result.changed_columns.is_empty());
    }
}
