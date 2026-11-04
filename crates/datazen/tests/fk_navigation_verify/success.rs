//! `exists = false` is a successful answer, not an error.
//!
//! Part of the DB-07 verification harness; the shared fixtures live in
//! the parent module.

use super::*;

// ─────────────────────────────────────────────────────────────────────────────
// B 组 · `exists = false` is a success, not an error
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_missing_referenced_row_is_a_successful_answer() {
    let driver = single_column_driver(Vec::new());
    let resolution = resolve_foreign_key_targets(
        &TestSource::new(driver),
        &request(&["customer_id"], vec![Value::Integer(7)]),
    )
    .await
    .expect("a missing referenced row is a successful read");
    assert!(!resolution.exists);
    assert!(resolution.candidates.is_empty());
    assert!(!resolution.truncated);
    // The target itself is still resolved and reported.
    assert_eq!(resolution.target.table, "parents");
    assert_eq!(resolution.target.columns, vec!["id".to_string()]);
}
