//! The browse mode of the resolver (DB-20): the in-cell selector asks for
//! what the target table **offers**, not for one looked-up row.
//!
//! These cases pin the three facts that keep the selector honest: a browse
//! request needs no key values, its candidate list is the target table's own
//! rows (cut off by the same `limit + 1` sentinel the probe uses), and the
//! constraint rules — which constraint, which columns — still apply.

use super::*;

/// A browse request for `orders.customer_id`, with whatever values the
/// caller pretends the cell holds (they are ignored).
fn browse_request(
    columns: &[&str],
    values: Vec<Value>,
    limit: Option<u32>,
) -> ResolveForeignKeyTargetsRequest {
    ResolveForeignKeyTargetsRequest {
        browse: true,
        limit,
        ..request(columns, values)
    }
}

#[tokio::test]
async fn browse_lists_the_target_rows_without_key_values() {
    // The selector opens before the user has picked anything, so the request
    // carries **no** values — and that must not read as a shape fault.
    let target = schema(
        "parents",
        vec![column("id", "integer", true)],
        vec!["id".to_string()],
        Vec::new(),
        Vec::new(),
    );
    let driver = driver_with(
        foreign_key("orders_parent_fk", &["customer_id"], "parents", &["id"]),
        target,
        vec![vec![Some(Value::Integer(1))], vec![Some(Value::Integer(2))]],
    );
    let source = TestSource::new(driver);

    let resolution =
        resolve_foreign_key_targets(&source, &browse_request(&["customer_id"], Vec::new(), None))
            .await
            .expect("a browse request needs no values");

    assert!(resolution.exists, "the target offers rows");
    assert!(!resolution.truncated);
    let rows: Vec<Vec<serde_json::Value>> =
        resolution.candidates.iter().map(|row| cells(row)).collect();
    assert_eq!(
        rows,
        vec![vec![serde_json::json!(1)], vec![serde_json::json!(2)],],
        "the candidates are the target's own rows"
    );
}

#[tokio::test]
async fn browse_truncates_with_the_same_sentinel_as_the_probe() {
    // Two rows in the answer for a limit of one: the first is the candidate,
    // the second is the truncation sentinel — the selector learns "there are
    // more" without a COUNT(*).
    let target = schema(
        "parents",
        vec![column("id", "integer", true)],
        vec!["id".to_string()],
        Vec::new(),
        Vec::new(),
    );
    let driver = driver_with(
        foreign_key("orders_parent_fk", &["customer_id"], "parents", &["id"]),
        target,
        vec![
            vec![Some(Value::Integer(1))],
            vec![Some(Value::Integer(2))],
            vec![Some(Value::Integer(3))],
        ],
    );
    let source = TestSource::new(driver);

    let resolution = resolve_foreign_key_targets(
        &source,
        &browse_request(&["customer_id"], Vec::new(), Some(2)),
    )
    .await
    .expect("the request resolves");

    assert_eq!(resolution.candidates.len(), 2, "the cap holds");
    assert!(resolution.truncated, "the third row proves the cut");
    assert_eq!(resolution.target.candidate_limit, 2);
}

#[tokio::test]
async fn browse_still_refuses_an_unknown_constraint() {
    // Browse relaxes the *values*, never the *rules*: the columns still have
    // to name a constraint the source table declares.
    let target = schema(
        "parents",
        vec![column("id", "integer", true)],
        vec!["id".to_string()],
        Vec::new(),
        Vec::new(),
    );
    let driver = driver_with(
        foreign_key("orders_parent_fk", &["customer_id"], "parents", &["id"]),
        target,
        Vec::new(),
    );
    let source = TestSource::new(driver);

    let error = resolve_foreign_key_targets(&source, &browse_request(&["nope"], Vec::new(), None))
        .await
        .expect_err("no constraint covers that column");

    let message = error.to_string();
    assert!(
        message.starts_with("grid.fk.unknownConstraint"),
        "the refusal is classified: {message}"
    );
}

#[tokio::test]
async fn browse_reports_an_empty_target_as_not_exists() {
    // An empty target is a successful answer, not an error: the selector
    // renders its empty state from `exists = false`.
    let target = schema(
        "parents",
        vec![column("id", "integer", true)],
        vec!["id".to_string()],
        Vec::new(),
        Vec::new(),
    );
    let driver = driver_with(
        foreign_key("orders_parent_fk", &["customer_id"], "parents", &["id"]),
        target,
        Vec::new(),
    );
    let source = TestSource::new(driver);

    let resolution =
        resolve_foreign_key_targets(&source, &browse_request(&["customer_id"], Vec::new(), None))
            .await
            .expect("an empty target is not an error");

    assert!(!resolution.exists);
    assert!(resolution.candidates.is_empty());
    assert!(!resolution.truncated);
}

#[tokio::test]
async fn probe_mode_still_requires_values() {
    // The relaxation belongs to browse only: a probe request without values
    // keeps its shape fault, so the jump cannot silently lose its conditions.
    let target = schema(
        "parents",
        vec![column("id", "integer", true)],
        vec!["id".to_string()],
        Vec::new(),
        Vec::new(),
    );
    let driver = driver_with(
        foreign_key("orders_parent_fk", &["customer_id"], "parents", &["id"]),
        target,
        Vec::new(),
    );
    let source = TestSource::new(driver);

    let mut probe_without_values = request(&["customer_id"], Vec::new());
    probe_without_values.browse = false;
    let error = resolve_foreign_key_targets(&source, &probe_without_values)
        .await
        .expect_err("the probe needs its values");

    let message = error.to_string();
    assert!(
        message.starts_with("grid.fk.shapeMismatch"),
        "the probe keeps its rules: {message}"
    );
}
