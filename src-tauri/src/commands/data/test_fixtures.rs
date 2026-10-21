//! Fixtures shared by the change-plan suites.
//!
//! `tests.rs` and `insert_tests.rs` are sibling modules, so the helpers both
//! suites call live here; the bodies are unchanged apart from the visibility
//! needed for a sibling glob import.

use super::*;
use crate::db::Value;
use crate::testing::app_state::TestAppState;
use std::collections::BTreeMap;

pub(super) async fn with_parameterized_driver() -> TestAppState {
    let mut options = crate::testing::app_state::rich_mock_options();
    options.parameterized_writes = true;
    TestAppState::with_options(options).await
}

pub(super) fn table_context(connection_id: &str, db_session_id: &str) -> RowChangeTableContext {
    RowChangeTableContext {
        connection_id: connection_id.into(),
        db_session_id: db_session_id.into(),
        driver_type: "postgres".into(),
        database: Some("app".into()),
        schema: None,
        table: "users".into(),
    }
}

pub(super) fn update_change(id: i64, original: &str, current: &str) -> PendingRowChange {
    PendingRowChange {
        row_identity: BTreeMap::from([("id".into(), Some(Value::Integer(id)))]),
        original_values: BTreeMap::from([("name".into(), Some(Value::String(original.into())))]),
        current_values: BTreeMap::from([("name".into(), Some(Value::String(current.into())))]),
        changed_columns: vec!["name".into()],
        delete_marked: false,
        insert_marked: false,
    }
}

pub(super) fn delete_change(id: i64) -> PendingRowChange {
    PendingRowChange {
        row_identity: BTreeMap::from([("id".into(), Some(Value::Integer(id)))]),
        original_values: BTreeMap::new(),
        current_values: BTreeMap::new(),
        changed_columns: Vec::new(),
        delete_marked: true,
        insert_marked: false,
    }
}

pub(super) fn composite_update_change(
    tenant_id: i64,
    id: i64,
    next_id: Option<i64>,
) -> PendingRowChange {
    let mut current_values =
        BTreeMap::from([("name".into(), Some(Value::String("Updated".into())))]);
    let mut changed_columns = vec!["name".into()];
    if let Some(next_id) = next_id {
        current_values.insert("id".into(), Some(Value::Integer(next_id)));
        changed_columns.push("id".into());
    }
    PendingRowChange {
        row_identity: BTreeMap::from([
            ("tenant_id".into(), Some(Value::Integer(tenant_id))),
            ("id".into(), Some(Value::Integer(id))),
        ]),
        original_values: BTreeMap::new(),
        current_values,
        changed_columns,
        delete_marked: false,
        insert_marked: false,
    }
}

pub(super) fn assert_legacy_write_disabled(error: crate::commands::error::CommandError) {
    assert!(matches!(
        error,
        crate::commands::error::CommandError::Validation(message)
            if message.contains("Legacy row writes are disabled")
    ));
}

pub(super) fn insert_change(values: &[(&str, Option<Value>)]) -> PendingRowChange {
    PendingRowChange {
        row_identity: BTreeMap::new(),
        original_values: BTreeMap::new(),
        current_values: values
            .iter()
            .map(|(column, value)| ((*column).to_string(), value.clone()))
            .collect(),
        changed_columns: values
            .iter()
            .map(|(column, _)| (*column).to_string())
            .collect(),
        delete_marked: false,
        insert_marked: true,
    }
}

pub(super) fn insert_capable_options() -> crate::testing::mock_driver::MockDriverOptions {
    let mut options = crate::testing::app_state::rich_mock_options();
    options.execute_rows_affected = 1;
    options.parameterized_writes = true;
    options
}
