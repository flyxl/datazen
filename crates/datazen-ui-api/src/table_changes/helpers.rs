use super::types::{ColumnValues, PendingRowChange, TableChangeContext, WriteExpressionId};

fn nonblank(value: Option<&str>) -> bool {
    value.map(|value| !value.trim().is_empty()).unwrap_or(false)
}

pub fn is_complete_table_change_context(context: &TableChangeContext) -> bool {
    nonblank(context.connection_id.as_deref())
        && !context.db_session_id.trim().is_empty()
        && nonblank(context.driver_type.as_deref())
        && nonblank(context.database.as_deref())
        && !context.table.trim().is_empty()
}

pub fn table_change_context_key(context: &TableChangeContext) -> String {
    serde_json::json!([
        context.connection_id,
        context.db_session_id,
        context.driver_type,
        context.database,
        context.schema,
        context.table,
    ])
    .to_string()
}

pub fn draft_write_columns(values: &ColumnValues) -> Vec<String> {
    let mut columns: Vec<String> = values.keys().cloned().collect();
    columns.sort();
    columns
}

pub fn clone_pending_row_change(change: &PendingRowChange) -> PendingRowChange {
    change.clone()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde_json::json;

    use super::*;
    use crate::table_changes::types::RowIdentity;
    use crate::Value;

    fn context() -> TableChangeContext {
        TableChangeContext {
            connection_id: Some(" c ".to_owned()),
            db_session_id: " s ".to_owned(),
            driver_type: Some(" postgresql ".to_owned()),
            database: Some(" shop ".to_owned()),
            schema: None,
            table: " users ".to_owned(),
        }
    }

    #[test]
    fn completeness_requires_every_routing_field_except_schema() {
        assert!(is_complete_table_change_context(&context()));
        let mut missing_connection = context();
        missing_connection.connection_id = None;
        assert!(!is_complete_table_change_context(&missing_connection));
        let mut blank_driver = context();
        blank_driver.driver_type = Some("  ".to_owned());
        assert!(!is_complete_table_change_context(&blank_driver));
        let mut missing_database = context();
        missing_database.database = None;
        assert!(!is_complete_table_change_context(&missing_database));
        let mut blank_table = context();
        blank_table.table = String::new();
        assert!(!is_complete_table_change_context(&blank_table));
        let mut blank_session = context();
        blank_session.db_session_id = " ".to_owned();
        assert!(!is_complete_table_change_context(&blank_session));
    }

    #[test]
    fn context_key_is_json_stable_and_keeps_nulls() {
        let key = table_change_context_key(&context());
        assert_eq!(
            key,
            serde_json::json!([" c ", " s ", " postgresql ", " shop ", null, " users "])
                .to_string()
        );
    }

    #[test]
    fn draft_columns_are_sorted_names_of_present_values() {
        let mut values = ColumnValues::new();
        values.insert("z".to_owned(), json!(1));
        values.insert("a".to_owned(), Value::Null);
        assert_eq!(draft_write_columns(&values), ["a", "z"]);
        assert!(draft_write_columns(&ColumnValues::new()).is_empty());
    }

    #[test]
    fn clone_preserves_optional_keys_and_is_independent() {
        let mut expression_values = HashMap::new();
        expression_values.insert("col".to_owned(), WriteExpressionId::CurrentTimestamp);
        let change = PendingRowChange {
            row_index: None,
            row_identity: RowIdentity::new(),
            original_values: ColumnValues::new(),
            current_values: ColumnValues::new(),
            changed_columns: vec!["col".to_owned()],
            delete_marked: false,
            insert_marked: None,
            expression_values: Some(expression_values),
        };
        let cloned = clone_pending_row_change(&change);
        assert_eq!(cloned, change);
        assert_eq!(cloned.row_index, None);
        assert_eq!(cloned.insert_marked, None);
        assert!(cloned.expression_values.is_some());
    }
}
