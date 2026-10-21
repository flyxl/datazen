//! Independent re-verification of the DB-07 foreign-key navigation resolver.
//!
//! This file is a **verification harness**, not a deliverable: it re-derives
//! the behaviour the 07 分册 demands (the five `grid.fk.*` codes, the
//! `exists = false` success path, the `truncated` criterion and the wire DTO
//! shape) from the contract text instead of from the implementation's own
//! test names, so a mistake in the delivered tests cannot hide here.
//!
//! Nothing in the production tree is touched: the harness only uses the
//! public surface (`datazen_feature_core::services::fk_navigation` and
//! `...::services::ports`).

use std::collections::HashMap;
use std::sync::Arc;

use datazen_driver_api::mock_driver::{MockDriver, MockDriverOptions};
use datazen_driver_api::{
    ColumnSchema, ConnectionHandle, DatabaseDriver, ForeignKeyInfo, IndexInfo, TableSchema, Value,
};
use datazen_feature_core::services::fk_navigation::{
    find_fk, referenced_columns_are_unique, resolve_foreign_key_targets, FkErrorCode,
    FkNavigationError, ResolveForeignKeyTargetsRequest, DEFAULT_CANDIDATE_LIMIT,
    MAX_CANDIDATE_LIMIT,
};
use datazen_feature_core::services::ports::{
    FkSessionBinding, FkSessionError, ForeignKeyNavigationSource,
};

const DATABASE: &str = "app";

/// A value that must never appear in an error message.
const SENTINEL: &str = "SENTINEL-VALUE-42";

struct TestSource {
    driver: Arc<dyn DatabaseDriver>,
    failure: Option<FkSessionError>,
}

impl TestSource {
    fn new(driver: Arc<dyn DatabaseDriver>) -> Self {
        Self {
            driver,
            failure: None,
        }
    }
}

#[async_trait::async_trait]
impl ForeignKeyNavigationSource for TestSource {
    async fn bind_session(&self, _db_session_id: &str) -> Result<FkSessionBinding, FkSessionError> {
        if let Some(failure) = &self.failure {
            return Err(failure.clone());
        }
        Ok(FkSessionBinding {
            driver: Arc::clone(&self.driver),
            handle: ConnectionHandle {
                id: "handle-1".into(),
                pool_id: "pool-1".into(),
            },
            database: Some(DATABASE.to_string()),
            schema: None,
            guard: Box::new(()),
        })
    }
}

fn column(name: &str, data_type: &str, primary: bool) -> ColumnSchema {
    ColumnSchema {
        name: name.to_string(),
        data_type: data_type.to_string(),
        nullable: !primary,
        default_value: None,
        comment: None,
        is_primary_key: primary,
        is_auto_increment: false,
    }
}

fn foreign_key(
    name: &str,
    columns: &[&str],
    referenced_table: &str,
    referenced: &[&str],
) -> ForeignKeyInfo {
    ForeignKeyInfo {
        name: name.to_string(),
        columns: columns.iter().map(|v| v.to_string()).collect(),
        referenced_table: referenced_table.to_string(),
        referenced_columns: referenced.iter().map(|v| v.to_string()).collect(),
        on_update: "NO ACTION".into(),
        on_delete: "NO ACTION".into(),
        deferrability: Default::default(),
    }
}

fn schema(
    table_name: &str,
    columns: Vec<ColumnSchema>,
    primary_keys: Vec<String>,
    indexes: Vec<IndexInfo>,
    foreign_keys: Vec<ForeignKeyInfo>,
) -> TableSchema {
    TableSchema {
        table_name: table_name.to_string(),
        columns,
        primary_keys,
        indexes,
        foreign_keys,
        check_constraints: Vec::new(),
        table_options: Default::default(),
    }
}

fn index(name: &str, columns: &[&str], unique: bool) -> IndexInfo {
    IndexInfo {
        name: name.to_string(),
        columns: columns.iter().map(|v| v.to_string()).collect(),
        is_unique: unique,
        is_primary: false,
        index_type: "btree".into(),
    }
}

fn options_with(
    tables: HashMap<String, TableSchema>,
    query_rows: Vec<Vec<Option<Value>>>,
) -> MockDriverOptions {
    let mut by_database = HashMap::new();
    by_database.insert(DATABASE.to_string(), tables);
    MockDriverOptions {
        table_schemas_by_database: by_database,
        query_rows,
        ..Default::default()
    }
}

/// `orders(customer_id) → parents(id)`, both catalogued.
fn single_column_driver(query_rows: Vec<Vec<Option<Value>>>) -> Arc<MockDriver> {
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
    MockDriver::new("postgres", options_with(tables, query_rows))
}

fn request(columns: &[&str], values: Vec<Value>) -> ResolveForeignKeyTargetsRequest {
    ResolveForeignKeyTargetsRequest {
        db_session_id: "session-1".into(),
        database: None,
        schema: None,
        table: "orders".into(),
        qualified_table: None,
        columns: columns.iter().map(|v| v.to_string()).collect(),
        constraint_name: None,
        values,
        limit: None,
    }
}

/// The `grid.fk.*` prefix of an error message, or `None` for a success.
fn code_of(error: &FkNavigationError) -> Option<&'static str> {
    match error {
        FkNavigationError::Coded { code, .. } => Some(code.as_str()),
        FkNavigationError::Session(_) => None,
    }
}

/// The whole message of a coded rejection.
fn message_of(error: &FkNavigationError) -> String {
    error.to_string()
}

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

// ─────────────────────────────────────────────────────────────────────────────
// §3.2 / §3.4 · the constraint lookup口径 is the same on both sides
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn the_constraint_lookup_prefers_the_name_and_falls_back_to_the_ordered_columns() {
    let foreign_keys = vec![
        foreign_key("fk_composite", &["a", "b"], "parents", &["x", "y"]),
        foreign_key("fk_single", &["a"], "parents", &["x"]),
    ];

    // The name wins.
    assert_eq!(
        find_fk(&foreign_keys, &["a".to_string()], Some("fk_composite")).map(|fk| fk.name.as_str()),
        Some("fk_composite")
    );
    // An unknown name falls back to the ordered columns.
    assert_eq!(
        find_fk(&foreign_keys, &["a".to_string()], Some("does_not_exist"))
            .map(|fk| fk.name.as_str()),
        Some("fk_single")
    );
    // No name at all: the ordered columns decide.
    assert_eq!(
        find_fk(&foreign_keys, &["a".to_string()], None).map(|fk| fk.name.as_str()),
        Some("fk_single")
    );
    // A different order is a different key.
    assert!(find_fk(&foreign_keys, &["b".to_string(), "a".to_string()], None).is_none());
    // A blank name is no name.
    assert_eq!(
        find_fk(&foreign_keys, &["a".to_string()], Some("   ")).map(|fk| fk.name.as_str()),
        Some("fk_single")
    );
}

#[test]
fn uniqueness_needs_an_exactly_covering_index() {
    let wide = schema(
        "parents",
        vec![
            column("x", "integer", false),
            column("y", "text", false),
            column("z", "text", false),
        ],
        Vec::new(),
        vec![index("parents_x_y_z", &["x", "y", "z"], true)],
        Vec::new(),
    );
    assert!(!referenced_columns_are_unique(
        &wide,
        &["x".to_string(), "y".to_string()]
    ));
    assert!(referenced_columns_are_unique(
        &wide,
        &["x".to_string(), "y".to_string(), "z".to_string()]
    ));
    let keyed = schema(
        "parents",
        vec![column("id", "integer", true)],
        vec!["id".to_string()],
        Vec::new(),
        Vec::new(),
    );
    assert!(referenced_columns_are_unique(&keyed, &["id".to_string()]));
    assert!(!referenced_columns_are_unique(&keyed, &[]));
}

// ─────────────────────────────────────────────────────────────────────────────
// D 组 · the wire shape of the DTOs
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn the_resolution_serializes_to_the_documented_camel_case_shape() {
    let driver = single_column_driver(vec![vec![Some(Value::Integer(7))]]);
    let resolution = resolve_foreign_key_targets(
        &TestSource::new(driver),
        &request(&["customer_id"], vec![Value::Integer(7)]),
    )
    .await
    .expect("the probe resolves");

    let json = serde_json::to_value(&resolution).expect("the resolution serializes");
    let target = json["target"].as_object().expect("the target is an object");

    // The key set of the target, field by field.
    let mut keys: Vec<&str> = target.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec![
            "candidateLimit",
            "columns",
            "dataTypes",
            "database",
            "primaryKeys",
            "schema",
            "table",
            "unique",
        ]
    );

    let mut top: Vec<&str> = json
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect();
    top.sort_unstable();
    assert_eq!(top, vec!["candidates", "exists", "target", "truncated"]);

    assert_eq!(target["database"], serde_json::json!(DATABASE));
    assert_eq!(target["table"], serde_json::json!("parents"));
    assert_eq!(target["columns"], serde_json::json!(["id"]));
    assert_eq!(target["dataTypes"], serde_json::json!(["integer"]));
    assert_eq!(target["primaryKeys"], serde_json::json!(["id"]));
    assert_eq!(target["unique"], serde_json::json!(true));
    assert_eq!(target["candidateLimit"], serde_json::json!(50));
    assert_eq!(json["exists"], serde_json::json!(true));
    assert_eq!(json["truncated"], serde_json::json!(false));
    assert_eq!(json["candidates"], serde_json::json!([[7]]));

    // The **declaration** order is what a struct serialization puts on the
    // wire (`serde_json::Value` sorts its keys, so the order is read off the
    // serialized text instead).
    let text = serde_json::to_string(&resolution).expect("the resolution serializes");
    let target_start = text.find("\"target\":").expect("the target is present");
    let target_end = text.find("},").expect("the target object closes");
    let target_text = &text[target_start..target_end];
    for key in [
        "\"database\"",
        "\"schema\"",
        "\"table\"",
        "\"columns\"",
        "\"dataTypes\"",
        "\"primaryKeys\"",
        "\"unique\"",
        "\"candidateLimit\"",
    ] {
        assert!(
            target_text.contains(key),
            "the target must carry {key}: {target_text}"
        );
    }
    let mut previous = 0usize;
    for key in [
        "\"database\"",
        "\"schema\"",
        "\"table\"",
        "\"columns\"",
        "\"dataTypes\"",
        "\"primaryKeys\"",
        "\"unique\"",
        "\"candidateLimit\"",
    ] {
        let position = target_text.find(key).unwrap_or_else(|| {
            panic!("the serialized target must carry {key}: {target_text}");
        });
        assert!(
            position >= previous,
            "the target fields must keep their declaration order: {target_text}"
        );
        previous = position;
    }

    // The top level: `target`, `exists`, `candidates`, `truncated`.
    let tail = &text[target_end..];
    let mut previous = 0usize;
    for key in ["\"exists\"", "\"candidates\"", "\"truncated\""] {
        let position = tail.find(key).unwrap_or_else(|| {
            panic!("the serialized resolution must carry {key}: {tail}");
        });
        assert!(
            position >= previous,
            "the top-level fields must keep their declaration order: {tail}"
        );
        previous = position;
    }
}

#[test]
fn the_request_deserializes_from_the_documented_camel_case_shape() {
    let json = serde_json::json!({
        "dbSessionId": "sess-1",
        "database": "app",
        "schema": "public",
        "table": "orders",
        "qualifiedTable": "public.orders",
        "columns": ["customer_id"],
        "constraintName": "orders_customer_fk",
        "values": [7],
        "limit": 10,
    });
    let request: ResolveForeignKeyTargetsRequest =
        serde_json::from_value(json).expect("the request deserializes");
    assert_eq!(request.db_session_id, "sess-1");
    assert_eq!(request.database.as_deref(), Some("app"));
    assert_eq!(request.schema.as_deref(), Some("public"));
    assert_eq!(request.table, "orders");
    assert_eq!(request.qualified_table.as_deref(), Some("public.orders"));
    assert_eq!(request.columns, vec!["customer_id".to_string()]);
    assert_eq!(
        request.constraint_name.as_deref(),
        Some("orders_customer_fk")
    );
    assert_eq!(request.values.len(), 1);
    assert_eq!(request.limit, Some(10));

    // Every optional field may be omitted.
    let minimal = serde_json::json!({
        "dbSessionId": "sess-1",
        "table": "orders",
        "columns": ["customer_id"],
        "values": [7],
    });
    let request: ResolveForeignKeyTargetsRequest =
        serde_json::from_value(minimal).expect("the minimal request deserializes");
    assert!(request.database.is_none());
    assert!(request.schema.is_none());
    assert!(request.qualified_table.is_none());
    assert!(request.constraint_name.is_none());
    assert!(request.limit.is_none());
}

#[test]
fn a_json_null_value_reaches_the_resolver_as_a_null_value() {
    // The wire carries a plain `null`; the untagged `Value` enum must decode
    // it into the `Null` variant, or the NULL guard would never fire.
    let json = serde_json::json!({
        "dbSessionId": "sess-1",
        "table": "orders",
        "columns": ["customer_id"],
        "values": [null],
    });
    let request: ResolveForeignKeyTargetsRequest =
        serde_json::from_value(json).expect("the request deserializes");
    assert!(matches!(request.values[0], Value::Null));
}

// ─────────────────────────────────────────────────────────────────────────────
// FK-D · the target name must be the driver's normalized name, and the
// namespace must be the one the reference names — a schema-qualified name
// cannot survive the driver's own qualification step.
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn the_target_table_name_is_the_driver_normalized_name_not_the_raw_reference() {
    // PostgreSQL reports `referenced_table` as "{schema}.{table}" and its
    // `TableSchema.table_name` is the **bare** name. The resolver has to
    // report the driver's normalized name: the probe addresses the table by
    // it, and `FROM "public.parents"` would be qualified into
    // `"public"."public.parents"`, a relation that cannot exist.
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
                "public.parents",
                &["id"],
            )],
        ),
    );
    // The driver resolves the qualified reference and reports the bare name.
    tables.insert(
        "public.parents".to_string(),
        schema(
            "parents",
            vec![column("id", "integer", true)],
            vec!["id".to_string()],
            Vec::new(),
            Vec::new(),
        ),
    );
    let driver = MockDriver::new("postgres", options_with(tables, Vec::new()));

    let resolution = resolve_foreign_key_targets(
        &TestSource::new(driver),
        &request(&["customer_id"], vec![Value::Integer(7)]),
    )
    .await
    .expect("the target resolves");

    // The catalogued (driver-normalized) name is "parents".
    assert_eq!(resolution.target.table, "parents");
    // And the namespace is the one the reference names.
    assert_eq!(resolution.target.schema.as_deref(), Some("public"));
}

#[tokio::test]
async fn the_target_namespace_is_the_referenced_one_not_the_source_tables() {
    // The source table lives in `public`, the target in `sales`: the target's
    // namespace is the one the reference names, not the source table's.
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
                "sales.parents",
                &["id"],
            )],
        ),
    );
    tables.insert(
        "sales.parents".to_string(),
        schema(
            "parents",
            vec![column("id", "integer", true)],
            vec!["id".to_string()],
            Vec::new(),
            Vec::new(),
        ),
    );
    let mut options = options_with(tables, Vec::new());
    options.has_schema_level = true;
    options.default_schema = Some("public");
    let driver = MockDriver::new("postgres", options);

    let resolution = resolve_foreign_key_targets(
        &TestSource::new(driver),
        &request(&["customer_id"], vec![Value::Integer(7)]),
    )
    .await
    .expect("the target resolves");

    assert_eq!(resolution.target.table, "parents");
    assert_eq!(resolution.target.schema.as_deref(), Some("sales"));
}

#[test]
fn a_schema_qualified_target_name_cannot_survive_the_drivers_qualification() {
    use datazen_driver_api::sql_target::{qualify_sql_with, QualifierQuote};
    use sqlparser::dialect::PostgreSqlDialect;

    // The probe SQL the resolver builds for a PostgreSQL target whose
    // `referenced_table` is schema-qualified: the table name is quoted as one
    // identifier, so the dot stays inside the quotes.
    let sql = r#"SELECT "id" FROM "public.customers" WHERE "id" = 7 LIMIT 51"#;
    let out = qualify_sql_with(
        &PostgreSqlDialect {},
        QualifierQuote::DoubleQuote,
        &["public"],
        sql,
    );

    // The driver's own qualification step turns the quoted dotted name into a
    // two-part relation that cannot exist in the catalog.
    assert!(
        out.sql.contains(r#""public"."public.customers""#),
        "the qualified name is re-qualified: {}",
        out.sql
    );

    // The bare name is what the driver can actually resolve.
    let bare = r#"SELECT "id" FROM "customers" WHERE "id" = 7 LIMIT 51"#;
    let out = qualify_sql_with(
        &PostgreSqlDialect {},
        QualifierQuote::DoubleQuote,
        &["public"],
        bare,
    );
    assert!(
        out.sql.contains(r#"FROM "public"."customers""#),
        "{}",
        out.sql
    );
}

#[test]
fn the_probe_sql_of_a_qualified_target_quotes_the_dotted_name_as_one_identifier() {
    use datazen_driver_api::dialect::LegacySqlDialect;
    use datazen_feature_core::services::query_executor::QueryExecutor;

    let driver = MockDriver::new("postgres", MockDriverOptions::default());
    let dialect = LegacySqlDialect::new(driver.as_ref());
    let sql = QueryExecutor::build_select_sql(
        "public.customers",
        &[column("id", "integer", true)],
        Some(vec![datazen_driver_api::filters::FilterCondition {
            column: "id".into(),
            operator: datazen_driver_api::filters::FilterOperator::Eq,
            value: Value::Integer(7),
        }]),
        None,
        &dialect,
        51,
        0,
        Some("and"),
    )
    .expect("the probe SQL is expressible");

    assert!(
        sql.contains(r#"FROM "public.customers""#),
        "the dotted name is one quoted identifier: {sql}"
    );
}
