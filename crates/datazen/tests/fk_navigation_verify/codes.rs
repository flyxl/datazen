//! The five `grid.fk.*` codes, one case each.
//!
//! Part of the DB-07 verification harness; the shared fixtures live in
//! the parent module.

use super::*;

// ─────────────────────────────────────────────────────────────────────────────
// B 组 · the five codes: prefix at position 0, no key value, concrete reason
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn shape_mismatch_is_reported_for_every_shape_fault() {
    // (a) the request's column count and value count disagree.
    let driver = single_column_driver(Vec::new());
    let error = resolve_foreign_key_targets(
        &TestSource::new(driver.clone()),
        &request(
            &["customer_id"],
            vec![Value::String(SENTINEL.into()), Value::Integer(2)],
        ),
    )
    .await
    .expect_err("two values for one column is a shape fault");
    assert_eq!(code_of(&error), Some("grid.fk.shapeMismatch"));
    let message = message_of(&error);
    assert!(
        message.starts_with("grid.fk.shapeMismatch"),
        "the prefix must sit at position 0: {message}"
    );
    assert!(
        !message.contains(SENTINEL),
        "the message must not echo a key value: {message}"
    );

    // (b) the request carries no column at all.
    let error =
        resolve_foreign_key_targets(&TestSource::new(driver.clone()), &request(&[], Vec::new()))
            .await
            .expect_err("an empty column list is a shape fault");
    assert_eq!(code_of(&error), Some("grid.fk.shapeMismatch"));
    assert!(message_of(&error).starts_with("grid.fk.shapeMismatch"));

    // (c) the request carries no value at all.
    let error = resolve_foreign_key_targets(
        &TestSource::new(driver.clone()),
        &request(&["customer_id"], Vec::new()),
    )
    .await
    .expect_err("an empty value list is a shape fault");
    assert_eq!(code_of(&error), Some("grid.fk.shapeMismatch"));
    assert!(message_of(&error).starts_with("grid.fk.shapeMismatch"));
}

#[tokio::test]
async fn shape_mismatch_is_reported_when_the_constraint_names_other_columns() {
    // The constraint is identified by name, but the request addresses the
    // columns in another order: the pairing is positional, so the two sides
    // do not line up.
    let mut tables = HashMap::new();
    tables.insert(
        "orders".to_string(),
        schema(
            "orders",
            vec![
                column("id", "integer", true),
                column("a", "integer", false),
                column("b", "integer", false),
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
            vec![column("x", "integer", true), column("y", "integer", true)],
            vec!["x".to_string(), "y".to_string()],
            Vec::new(),
            Vec::new(),
        ),
    );
    let driver = MockDriver::new("postgres", options_with(tables, Vec::new()));
    let mut request = request(&["b", "a"], vec![Value::Integer(1), Value::Integer(2)]);
    request.constraint_name = Some("orders_parent_fk".into());

    let error = resolve_foreign_key_targets(&TestSource::new(driver), &request)
        .await
        .expect_err("a different column order cannot be paired positionally");
    assert_eq!(code_of(&error), Some("grid.fk.shapeMismatch"));
    let message = message_of(&error);
    assert!(message.starts_with("grid.fk.shapeMismatch"));
    // The message says which constraint and which columns are involved.
    assert!(
        message.contains("orders_parent_fk"),
        "the message must name the constraint: {message}"
    );
}

#[tokio::test]
async fn shape_mismatch_is_reported_when_the_target_lacks_the_referenced_column() {
    // The target table exists and is readable, but it has no `id` column.
    let mut tables = HashMap::new();
    tables.insert(
        "orders".to_string(),
        schema(
            "orders",
            vec![
                column("id", "integer", true),
                column("customer_id", "integer", false),
            ],
            vec!["id".to_string()],
            Vec::new(),
            vec![foreign_key(
                "orders_customer_fk",
                &["customer_id"],
                "parents",
                &["id"],
            )],
        ),
    );
    tables.insert(
        "parents".to_string(),
        schema(
            "parents",
            vec![column("other", "integer", true)],
            vec!["other".to_string()],
            Vec::new(),
            Vec::new(),
        ),
    );
    let driver = MockDriver::new("postgres", options_with(tables, Vec::new()));

    let error = resolve_foreign_key_targets(
        &TestSource::new(driver),
        &request(&["customer_id"], vec![Value::String(SENTINEL.into())]),
    )
    .await
    .expect_err("a target without the referenced column cannot be navigated");
    assert_eq!(code_of(&error), Some("grid.fk.shapeMismatch"));
    let message = message_of(&error);
    assert!(message.starts_with("grid.fk.shapeMismatch"));
    assert!(
        message.contains("id") && message.contains("parents"),
        "the message must name the table and the missing column: {message}"
    );
    assert!(
        !message.contains(SENTINEL),
        "the message must not echo a key value: {message}"
    );
}

#[tokio::test]
async fn unknown_constraint_is_reported_when_no_constraint_matches() {
    let mut tables = HashMap::new();
    tables.insert(
        "orders".to_string(),
        schema(
            "orders",
            vec![column("id", "integer", true)],
            vec!["id".to_string()],
            Vec::new(),
            Vec::new(),
        ),
    );
    let driver = MockDriver::new("postgres", options_with(tables, Vec::new()));

    let error = resolve_foreign_key_targets(
        &TestSource::new(driver),
        &request(&["customer_id"], vec![Value::String(SENTINEL.into())]),
    )
    .await
    .expect_err("a source table without foreign keys cannot be navigated");
    assert_eq!(code_of(&error), Some("grid.fk.unknownConstraint"));
    let message = message_of(&error);
    assert!(
        message.starts_with("grid.fk.unknownConstraint"),
        "the prefix must sit at position 0: {message}"
    );
    assert!(
        message.contains("orders"),
        "the message must name the source table: {message}"
    );
    assert!(
        !message.contains(SENTINEL),
        "the message must not echo a key value: {message}"
    );
}

#[tokio::test]
async fn unknown_constraint_is_reported_when_the_source_fk_metadata_is_unreadable() {
    // A driver with a schema level but no default schema makes every
    // `get_table_schema` call fail: the source table's foreign keys cannot be
    // read at all. Per the adjudicated boundary that is `unknownConstraint`
    // (there is no sixth code), and the message must say "unreadable" rather
    // than "there is no such constraint".
    let mut tables = HashMap::new();
    tables.insert(
        "orders".to_string(),
        schema(
            "orders",
            vec![
                column("id", "integer", true),
                column("customer_id", "integer", false),
            ],
            vec!["id".to_string()],
            Vec::new(),
            vec![foreign_key(
                "orders_customer_fk",
                &["customer_id"],
                "parents",
                &["id"],
            )],
        ),
    );
    let mut options = options_with(tables, Vec::new());
    options.has_schema_level = true;
    options.default_schema = None;
    let driver = MockDriver::new("postgres", options);

    let error = resolve_foreign_key_targets(
        &TestSource::new(driver),
        &request(&["customer_id"], vec![Value::String(SENTINEL.into())]),
    )
    .await
    .expect_err("unreadable source metadata cannot be navigated");
    assert_eq!(code_of(&error), Some("grid.fk.unknownConstraint"));
    let message = message_of(&error);
    assert!(message.starts_with("grid.fk.unknownConstraint"));
    assert!(
        message.contains("orders"),
        "the message must name the source table: {message}"
    );
    assert!(
        !message.contains(SENTINEL),
        "the message must not echo a key value: {message}"
    );
}

#[tokio::test]
async fn value_null_is_reported_and_names_the_column() {
    let driver = single_column_driver(Vec::new());
    let error = resolve_foreign_key_targets(
        &TestSource::new(driver),
        &request(&["customer_id"], vec![Value::Null]),
    )
    .await
    .expect_err("a NULL member makes the group unlocatable");
    assert_eq!(code_of(&error), Some("grid.fk.valueNull"));
    let message = message_of(&error);
    assert!(
        message.starts_with("grid.fk.valueNull"),
        "the prefix must sit at position 0: {message}"
    );
    assert!(
        message.contains("customer_id"),
        "the message must name the NULL column: {message}"
    );
}

#[tokio::test]
async fn value_null_is_reported_for_any_member_of_a_composite_key() {
    let mut tables = HashMap::new();
    tables.insert(
        "orders".to_string(),
        schema(
            "orders",
            vec![
                column("id", "integer", true),
                column("a", "integer", false),
                column("b", "integer", false),
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
            vec![column("x", "integer", true), column("y", "integer", true)],
            vec!["x".to_string(), "y".to_string()],
            Vec::new(),
            Vec::new(),
        ),
    );
    let driver = MockDriver::new("postgres", options_with(tables, Vec::new()));

    // The NULL is the **second** member: the whole group is unlocatable.
    let error = resolve_foreign_key_targets(
        &TestSource::new(driver),
        &request(&["a", "b"], vec![Value::Integer(1), Value::Null]),
    )
    .await
    .expect_err("one NULL member makes the whole group unlocatable");
    assert_eq!(code_of(&error), Some("grid.fk.valueNull"));
    let message = message_of(&error);
    assert!(message.starts_with("grid.fk.valueNull"));
    assert!(
        message.contains("'b'"),
        "the message must name the NULL member: {message}"
    );
}

#[tokio::test]
async fn target_unresolved_is_reported_when_the_target_metadata_is_unavailable() {
    // The catalog holds `orders` but no `parents`: the target metadata cannot
    // be read (or comes back empty) on both resolution steps.
    let mut tables = HashMap::new();
    tables.insert(
        "orders".to_string(),
        schema(
            "orders",
            vec![
                column("id", "integer", true),
                column("customer_id", "integer", false),
            ],
            vec!["id".to_string()],
            Vec::new(),
            vec![foreign_key(
                "orders_customer_fk",
                &["customer_id"],
                "parents",
                &["id"],
            )],
        ),
    );
    let driver = MockDriver::new("postgres", options_with(tables, Vec::new()));

    let error = resolve_foreign_key_targets(
        &TestSource::new(driver),
        &request(&["customer_id"], vec![Value::String(SENTINEL.into())]),
    )
    .await
    .expect_err("a target without metadata cannot be navigated");
    assert_eq!(code_of(&error), Some("grid.fk.targetUnresolved"));
    let message = message_of(&error);
    assert!(
        message.starts_with("grid.fk.targetUnresolved"),
        "the prefix must sit at position 0: {message}"
    );
    assert!(
        message.contains("parents"),
        "the message must name the referenced table: {message}"
    );
    assert!(
        !message.contains(SENTINEL),
        "the message must not echo a key value: {message}"
    );
}

#[tokio::test]
async fn probe_failed_is_reported_when_the_metadata_is_readable_but_the_select_fails() {
    let mut tables = HashMap::new();
    tables.insert(
        "orders".to_string(),
        schema(
            "orders",
            vec![
                column("id", "integer", true),
                column("customer_id", "integer", false),
            ],
            vec!["id".to_string()],
            Vec::new(),
            vec![foreign_key(
                "orders_customer_fk",
                &["customer_id"],
                "parents",
                &["id"],
            )],
        ),
    );
    tables.insert(
        "parents".to_string(),
        schema(
            "parents",
            vec![column("id", "integer", true)],
            vec!["id".to_string()],
            Vec::new(),
            Vec::new(),
        ),
    );
    let mut options = options_with(tables, Vec::new());
    options.query_error = Some("probe exploded".into());
    let driver = MockDriver::new("postgres", options);

    let error = resolve_foreign_key_targets(
        &TestSource::new(driver),
        &request(&["customer_id"], vec![Value::String(SENTINEL.into())]),
    )
    .await
    .expect_err("a failing probe is reported");
    assert_eq!(code_of(&error), Some("grid.fk.probeFailed"));
    let message = message_of(&error);
    assert!(
        message.starts_with("grid.fk.probeFailed"),
        "the prefix must sit at position 0: {message}"
    );
    assert!(
        message.contains("parents"),
        "the message must name the probed table: {message}"
    );
}

#[tokio::test]
async fn the_five_codes_are_exactly_the_five_the_contract_registers() {
    // The closed set of 00-contracts §8.2 for the `grid.fk.*` domain.
    let all = [
        FkErrorCode::ShapeMismatch,
        FkErrorCode::UnknownConstraint,
        FkErrorCode::ValueNull,
        FkErrorCode::TargetUnresolved,
        FkErrorCode::ProbeFailed,
    ];
    let names: Vec<&str> = all.iter().map(|code| code.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "grid.fk.shapeMismatch",
            "grid.fk.unknownConstraint",
            "grid.fk.valueNull",
            "grid.fk.targetUnresolved",
            "grid.fk.probeFailed",
        ]
    );
    // Every code renders as `<code>: <reason>` — the prefix is the head.
    for code in all {
        let error = FkNavigationError::Coded {
            code,
            message: "because".to_string(),
        };
        assert_eq!(error.to_string(), format!("{code}: because"));
    }
}

#[tokio::test]
async fn a_session_failure_carries_no_grid_code() {
    let source = TestSource {
        driver: MockDriver::new("postgres", MockDriverOptions::default()),
        failure: Some(FkSessionError::NotFound("DB session is unavailable".into())),
    };
    let error =
        resolve_foreign_key_targets(&source, &request(&["customer_id"], vec![Value::Integer(7)]))
            .await
            .expect_err("a missing session is reported");
    assert_eq!(code_of(&error), None);
    assert!(!error.to_string().starts_with("grid."));
}
