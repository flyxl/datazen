//! A composite key is located as one group.
//!
//! Part of the DB-07 verification harness; the shared fixtures live in
//! the parent module.

use super::*;

// ─────────────────────────────────────────────────────────────────────────────
// C 组 · a composite key is located as one group
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_composite_key_pairs_positionally() {
    let mut tables = HashMap::new();
    tables.insert(
        "orders".to_string(),
        schema(
            "orders",
            vec![
                column("id", "integer", true),
                column("a", "integer", false),
                column("b", "text", false),
            ],
            vec!["id".to_string()],
            Vec::new(),
            vec![foreign_key(
                "orders_parent_fk",
                &["a", "b"],
                "parents",
                &["x", "y"],
            )],
        ),
    );
    tables.insert(
        "parents".to_string(),
        schema(
            "parents",
            vec![column("x", "integer", true), column("y", "text", true)],
            vec!["x".to_string(), "y".to_string()],
            Vec::new(),
            Vec::new(),
        ),
    );
    let driver = MockDriver::new(
        "postgres",
        options_with(
            tables,
            vec![vec![
                Some(Value::Integer(1)),
                Some(Value::String("k".into())),
            ]],
        ),
    );

    let resolution = resolve_foreign_key_targets(
        &TestSource::new(driver),
        &request(
            &["a", "b"],
            vec![Value::Integer(1), Value::String("k".into())],
        ),
    )
    .await
    .expect("a composite foreign key resolves");
    assert_eq!(
        resolution.target.columns,
        vec!["x".to_string(), "y".to_string()]
    );
    assert_eq!(
        resolution.target.data_types,
        vec!["integer".to_string(), "text".to_string()]
    );
    assert_eq!(resolution.candidates[0].len(), 2);
    assert!(resolution.exists);
}
