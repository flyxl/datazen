//! Tests for the shared executor's pure decision rules.
//!
//! These rules live in `application/command_runtime.rs` (D6 item 3 切片 b
//! 起从 executor 随「共享命令规则」归位)；the cases are kept in a sibling
//! file so the engine stays readable (the same split `ai/safety.rs` uses).

use crate::workflow::application::command_runtime::database_is_required;

#[test]
fn requires_a_database_for_an_unpinned_multi_db_connection() {
    // The reported failure: a PostgreSQL connection with no database, and a
    // step that names none. PG would fall back to its hardcoded "postgres".
    assert!(database_is_required(None, None, true));
}

#[test]
fn a_step_database_satisfies_the_rule() {
    assert!(!database_is_required(Some("datazen_demo"), None, true));
}

#[test]
fn a_connection_default_satisfies_the_rule() {
    assert!(!database_is_required(None, Some("datazen_demo"), true));
}

#[test]
fn a_single_database_driver_never_requires_one() {
    // Keeps existing workflows that legitimately omit the value working.
    assert!(!database_is_required(None, None, false));
}

#[test]
fn blank_values_count_as_unset() {
    assert!(database_is_required(Some("   "), Some("\t"), true));
    assert!(!database_is_required(Some(" real_db "), None, true));
    assert!(!database_is_required(None, Some(" real_db "), true));
}
