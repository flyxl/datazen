//! The `truncated` criterion of the candidate list.
//!
//! Part of the DB-07 verification harness; the shared fixtures live in
//! the parent module.

use super::*;

// ─────────────────────────────────────────────────────────────────────────────
// C 组 · the `truncated` criterion
// ─────────────────────────────────────────────────────────────────────────────

fn rows(count: usize) -> Vec<Vec<Option<Value>>> {
    (0..count)
        .map(|index| vec![Some(Value::Integer(index as i64))])
        .collect()
}

#[tokio::test]
async fn exactly_the_limit_rows_is_not_truncated() {
    let driver = single_column_driver(rows(3));
    let mut request = request(&["customer_id"], vec![Value::Integer(7)]);
    request.limit = Some(3);

    let resolution = resolve_foreign_key_targets(&TestSource::new(driver), &request)
        .await
        .expect("the probe resolves");
    assert_eq!(resolution.target.candidate_limit, 3);
    assert_eq!(resolution.candidates.len(), 3);
    assert!(
        !resolution.truncated,
        "exactly the limit rows means nothing was cut off"
    );
}

#[tokio::test]
async fn one_row_over_the_limit_is_truncated() {
    let driver = single_column_driver(rows(4));
    let mut request = request(&["customer_id"], vec![Value::Integer(7)]);
    request.limit = Some(3);

    let resolution = resolve_foreign_key_targets(&TestSource::new(driver), &request)
        .await
        .expect("the probe resolves");
    assert_eq!(resolution.target.candidate_limit, 3);
    assert_eq!(resolution.candidates.len(), 3);
    assert!(
        resolution.truncated,
        "one row over the limit is a truncation"
    );
}

#[tokio::test]
async fn the_default_limit_uses_the_same_strict_criterion() {
    let driver = single_column_driver(rows(DEFAULT_CANDIDATE_LIMIT as usize));
    let resolution = resolve_foreign_key_targets(
        &TestSource::new(driver),
        &request(&["customer_id"], vec![Value::Integer(7)]),
    )
    .await
    .expect("the probe resolves");
    assert_eq!(resolution.target.candidate_limit, DEFAULT_CANDIDATE_LIMIT);
    assert!(!resolution.truncated);

    let driver = single_column_driver(rows(DEFAULT_CANDIDATE_LIMIT as usize + 1));
    let resolution = resolve_foreign_key_targets(
        &TestSource::new(driver),
        &request(&["customer_id"], vec![Value::Integer(7)]),
    )
    .await
    .expect("the probe resolves");
    assert!(resolution.truncated);
}

#[tokio::test]
async fn the_limit_is_clamped_into_one_to_two_hundred() {
    let driver = single_column_driver(Vec::new());

    let mut over = request(&["customer_id"], vec![Value::Integer(7)]);
    over.limit = Some(1_000);
    let resolution = resolve_foreign_key_targets(&TestSource::new(driver.clone()), &over)
        .await
        .expect("the probe resolves");
    assert_eq!(resolution.target.candidate_limit, MAX_CANDIDATE_LIMIT);

    let mut under = request(&["customer_id"], vec![Value::Integer(7)]);
    under.limit = Some(0);
    let resolution = resolve_foreign_key_targets(&TestSource::new(driver), &under)
        .await
        .expect("the probe resolves");
    assert_eq!(resolution.target.candidate_limit, 1);
}
