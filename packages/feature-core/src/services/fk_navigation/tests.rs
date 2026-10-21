//! Unit tests for the foreign-key navigation resolver (DB-07).

use std::collections::HashMap;
use std::sync::Arc;

use datazen_driver_api::mock_driver::{MockDriver, MockDriverOptions};
use datazen_driver_api::{
    ColumnSchema, ConnectionHandle, DatabaseDriver, ForeignKeyInfo, IndexInfo, TableSchema, Value,
};

use super::*;
use crate::services::ports::{FkSessionBinding, FkSessionError, ForeignKeyNavigationSource};

const DATABASE: &str = "app";

/// A source port that lends one session to the resolver.
struct TestSource {
    driver: Arc<dyn DatabaseDriver>,
    database: Option<String>,
    schema: Option<String>,
    failure: Option<FkSessionError>,
}

impl TestSource {
    fn new(driver: Arc<dyn DatabaseDriver>) -> Self {
        Self {
            driver,
            database: Some(DATABASE.to_string()),
            schema: None,
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
            database: self.database.clone(),
            schema: self.schema.clone(),
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
        columns: columns.iter().map(|value| value.to_string()).collect(),
        referenced_table: referenced_table.to_string(),
        referenced_columns: referenced.iter().map(|value| value.to_string()).collect(),
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
        columns: columns.iter().map(|value| value.to_string()).collect(),
        is_unique: unique,
        is_primary: false,
        index_type: "btree".into(),
    }
}

/// `Value` has no `PartialEq`, so cells are compared through their JSON shape.
fn cells(row: &[Option<Value>]) -> Vec<serde_json::Value> {
    row.iter()
        .map(|cell| serde_json::to_value(cell).unwrap_or(serde_json::Value::Null))
        .collect()
}

/// Build a mock driver whose catalog holds `orders` (with `fk`) and `parents`.
fn driver_with(
    fk: ForeignKeyInfo,
    target: TableSchema,
    query_rows: Vec<Vec<Option<Value>>>,
) -> Arc<MockDriver> {
    let source = schema(
        "orders",
        vec![
            column("id", "integer", true),
            column("customer_id", "integer", false),
        ],
        vec!["id".to_string()],
        Vec::new(),
        vec![fk],
    );
    let mut tables = HashMap::new();
    tables.insert("orders".to_string(), source);
    tables.insert("parents".to_string(), target);
    MockDriver::new("postgres", options_with(tables, query_rows))
}

/// The mock options for a catalog that holds the given tables.
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

fn request(columns: &[&str], values: Vec<Value>) -> ResolveFkTargetsRequest {
    ResolveFkTargetsRequest {
        db_session_id: "session-1".into(),
        database: None,
        schema: None,
        table: "orders".into(),
        qualified_table: None,
        columns: columns.iter().map(|value| value.to_string()).collect(),
        constraint_name: None,
        values,
        limit: None,
    }
}

fn single_column_target() -> TableSchema {
    schema(
        "parents",
        vec![column("id", "integer", true)],
        vec!["id".to_string()],
        Vec::new(),
        Vec::new(),
    )
}

#[tokio::test]
async fn resolve_fk_targets_single_column_maps_values_to_referenced_columns() {
    let driver = driver_with(
        foreign_key("orders_customer_fk", &["customer_id"], "parents", &["id"]),
        single_column_target(),
        vec![vec![Some(Value::Integer(7))]],
    );
    let source = TestSource::new(driver);

    let resolution =
        resolve_foreign_key_targets(&source, &request(&["customer_id"], vec![Value::Integer(7)]))
            .await
            .expect("single column foreign key resolves");

    assert_eq!(resolution.target.columns, vec!["id".to_string()]);
    assert_eq!(resolution.target.data_types, vec!["integer".to_string()]);
    assert_eq!(resolution.target.table, "parents");
    assert_eq!(resolution.target.database, DATABASE);
    assert!(resolution.target.unique);
    assert!(resolution.exists);
    assert_eq!(cells(&resolution.candidates[0]), vec![serde_json::json!(7)]);
    assert!(!resolution.truncated);
}

#[tokio::test]
async fn resolve_fk_targets_composite_pairs_columns_positionally() {
    let target = schema(
        "parents",
        vec![column("x", "integer", true), column("y", "text", true)],
        vec!["x".to_string(), "y".to_string()],
        Vec::new(),
        Vec::new(),
    );
    let driver = driver_with(
        foreign_key("orders_parent_fk", &["a", "b"], "parents", &["x", "y"]),
        target,
        vec![vec![
            Some(Value::Integer(1)),
            Some(Value::String("k".into())),
        ]],
    );
    let source = TestSource::new(driver);

    let resolution = resolve_foreign_key_targets(
        &source,
        &request(
            &["a", "b"],
            vec![Value::Integer(1), Value::String("k".into())],
        ),
    )
    .await
    .expect("composite foreign key resolves");

    assert_eq!(
        resolution.target.columns,
        vec!["x".to_string(), "y".to_string()]
    );
    assert_eq!(
        resolution.target.data_types,
        vec!["integer".to_string(), "text".to_string()]
    );
    assert!(resolution.target.unique);
    assert!(resolution.exists);
    // The candidate row keeps the pairing order of the referenced columns.
    assert_eq!(
        cells(&resolution.candidates[0]),
        vec![serde_json::json!(1), serde_json::json!("k")]
    );
}

#[tokio::test]
async fn resolve_fk_targets_rejects_length_mismatch_with_shape_mismatch_prefix() {
    let driver = driver_with(
        foreign_key("orders_customer_fk", &["customer_id"], "parents", &["id"]),
        single_column_target(),
        Vec::new(),
    );
    let source = TestSource::new(driver);

    let error = resolve_foreign_key_targets(
        &source,
        &request(&["customer_id"], vec![Value::Integer(7), Value::Integer(8)]),
    )
    .await
    .expect_err("mismatched lengths are refused");

    assert!(
        error.to_string().starts_with("grid.fk.shapeMismatch"),
        "unexpected message: {error}"
    );
}

#[tokio::test]
async fn resolve_fk_targets_rejects_null_component_with_value_null_prefix() {
    let target = schema(
        "parents",
        vec![column("x", "integer", true), column("y", "text", true)],
        vec!["x".to_string(), "y".to_string()],
        Vec::new(),
        Vec::new(),
    );
    let driver = driver_with(
        foreign_key("orders_parent_fk", &["a", "b"], "parents", &["x", "y"]),
        target,
        Vec::new(),
    );
    let source = TestSource::new(driver);

    let error = resolve_foreign_key_targets(
        &source,
        &request(&["a", "b"], vec![Value::Integer(1), Value::Null]),
    )
    .await
    .expect_err("a NULL member makes the group unlocatable");

    assert!(
        error.to_string().starts_with("grid.fk.valueNull"),
        "unexpected message: {error}"
    );
    // The message names the column but never echoes the key value.
    assert!(error.to_string().contains("'b'"));
}

#[tokio::test]
async fn resolve_fk_targets_unknown_constraint_prefix() {
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
    let source = TestSource::new(driver);

    let error =
        resolve_foreign_key_targets(&source, &request(&["customer_id"], vec![Value::Integer(7)]))
            .await
            .expect_err("a source table without foreign keys cannot be navigated");

    assert!(
        error.to_string().starts_with("grid.fk.unknownConstraint"),
        "unexpected message: {error}"
    );
}

#[tokio::test]
async fn resolve_fk_targets_missing_target_row_is_not_an_error() {
    let driver = driver_with(
        foreign_key("orders_customer_fk", &["customer_id"], "parents", &["id"]),
        single_column_target(),
        Vec::new(),
    );
    let source = TestSource::new(driver);

    let resolution =
        resolve_foreign_key_targets(&source, &request(&["customer_id"], vec![Value::Integer(7)]))
            .await
            .expect("a missing referenced row is a successful read");

    assert!(!resolution.exists);
    assert!(resolution.candidates.is_empty());
    assert!(!resolution.truncated);
}

#[tokio::test]
async fn resolve_fk_targets_unresolved_metadata_uses_target_unresolved_prefix() {
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
    // The catalog holds no `parents`: the target cannot be resolved.
    let driver = MockDriver::new("postgres", options_with(tables, Vec::new()));
    let source = TestSource::new(driver);

    let error =
        resolve_foreign_key_targets(&source, &request(&["customer_id"], vec![Value::Integer(7)]))
            .await
            .expect_err("a target table without metadata cannot be navigated");

    assert!(
        error.to_string().starts_with("grid.fk.targetUnresolved"),
        "unexpected message: {error}"
    );
    assert!(
        !error.to_string().contains('7'),
        "the message must not echo the key value: {error}"
    );
}

#[tokio::test]
async fn resolve_fk_targets_probe_failure_uses_probe_failed_prefix() {
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
    tables.insert("parents".to_string(), single_column_target());
    let mut options = options_with(tables, Vec::new());
    options.query_error = Some("probe exploded".into());
    let driver = MockDriver::new("postgres", options);
    let source = TestSource::new(driver);

    let error =
        resolve_foreign_key_targets(&source, &request(&["customer_id"], vec![Value::Integer(7)]))
            .await
            .expect_err("a failing probe is reported");

    assert!(
        error.to_string().starts_with("grid.fk.probeFailed"),
        "unexpected message: {error}"
    );
}

#[tokio::test]
async fn resolve_fk_targets_flags_non_unique_referenced_columns() {
    let target = schema(
        "parents",
        vec![column("id", "integer", false)],
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );
    let driver = driver_with(
        foreign_key("orders_customer_fk", &["customer_id"], "parents", &["id"]),
        target,
        Vec::new(),
    );
    let source = TestSource::new(driver);

    let resolution =
        resolve_foreign_key_targets(&source, &request(&["customer_id"], vec![Value::Integer(7)]))
            .await
            .expect("a non-unique target still resolves");

    assert!(!resolution.target.unique);
    assert!(resolution.target.primary_keys.is_empty());
}

#[tokio::test]
async fn resolve_fk_targets_treats_exactly_covering_unique_index_as_unique() {
    let target = schema(
        "parents",
        vec![column("x", "integer", false), column("y", "text", false)],
        Vec::new(),
        vec![index("parents_x_y_key", &["x", "y"], true)],
        Vec::new(),
    );
    let driver = driver_with(
        foreign_key("orders_parent_fk", &["a", "b"], "parents", &["x", "y"]),
        target,
        Vec::new(),
    );
    let source = TestSource::new(driver);

    let resolution = resolve_foreign_key_targets(
        &source,
        &request(
            &["a", "b"],
            vec![Value::Integer(1), Value::String("k".into())],
        ),
    )
    .await
    .expect("a covered unique index still resolves");

    assert!(resolution.target.unique);
}

#[tokio::test]
async fn resolve_fk_targets_rejects_wider_unique_index_as_unique() {
    let target = schema(
        "parents",
        vec![
            column("x", "integer", false),
            column("y", "text", false),
            column("z", "text", false),
        ],
        Vec::new(),
        vec![index("parents_x_y_z_key", &["x", "y", "z"], true)],
        Vec::new(),
    );
    let driver = driver_with(
        foreign_key("orders_parent_fk", &["a", "b"], "parents", &["x", "y"]),
        target,
        Vec::new(),
    );
    let source = TestSource::new(driver);

    let resolution = resolve_foreign_key_targets(
        &source,
        &request(
            &["a", "b"],
            vec![Value::Integer(1), Value::String("k".into())],
        ),
    )
    .await
    .expect("a wider unique index still resolves");

    assert!(
        !resolution.target.unique,
        "a unique index on (x, y, z) does not make (x, y) unique"
    );
}

#[tokio::test]
async fn resolve_fk_targets_caps_candidates_and_sets_truncated() {
    let rows: Vec<Vec<Option<Value>>> = (0..201)
        .map(|index| vec![Some(Value::Integer(index))])
        .collect();
    let driver = driver_with(
        foreign_key("orders_customer_fk", &["customer_id"], "parents", &["id"]),
        single_column_target(),
        rows,
    );
    let source = TestSource::new(driver);
    let mut request = request(&["customer_id"], vec![Value::Integer(7)]);
    request.limit = Some(1_000);

    let resolution = resolve_foreign_key_targets(&source, &request)
        .await
        .expect("an over-sized limit still resolves");

    assert_eq!(resolution.target.candidate_limit, MAX_CANDIDATE_LIMIT);
    assert_eq!(resolution.candidates.len(), MAX_CANDIDATE_LIMIT as usize);
    assert!(resolution.truncated);
}

#[tokio::test]
async fn resolve_fk_targets_probe_uses_limit_without_count() {
    let driver = driver_with(
        foreign_key("orders_customer_fk", &["customer_id"], "parents", &["id"]),
        single_column_target(),
        vec![vec![Some(Value::Integer(7))]],
    );
    let source = TestSource::new(driver.clone());

    let resolution =
        resolve_foreign_key_targets(&source, &request(&["customer_id"], vec![Value::Integer(7)]))
            .await
            .expect("the probe resolves");

    assert!(resolution.exists);
    // One statement only: the existence probe must not count the whole table.
    assert_eq!(
        driver.query_calls(),
        1,
        "the probe must issue exactly one statement"
    );
}

#[tokio::test]
async fn resolve_fk_targets_qualified_referenced_table_falls_back_to_split_schema() {
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
    tables.insert("parents".to_string(), single_column_target());
    let mut options = options_with(tables, Vec::new());
    options.has_schema_level = true;
    options.default_schema = Some("public");
    let driver = MockDriver::new("postgres", options);
    let source = TestSource::new(driver);

    let resolution =
        resolve_foreign_key_targets(&source, &request(&["customer_id"], vec![Value::Integer(7)]))
            .await
            .expect("a qualified referenced table falls back to the split schema");

    assert_eq!(resolution.target.table, "parents");
    assert_eq!(resolution.target.schema.as_deref(), Some("sales"));
}

#[tokio::test]
async fn resolve_fk_targets_rejects_a_constraint_with_mismatched_sides() {
    let mut tables = HashMap::new();
    tables.insert(
        "orders".to_string(),
        schema(
            "orders",
            vec![column("id", "integer", true), column("a", "integer", false)],
            vec!["id".to_string()],
            Vec::new(),
            vec![foreign_key(
                "orders_broken_fk",
                &["a"],
                "parents",
                &["x", "y"],
            )],
        ),
    );
    let driver = MockDriver::new("postgres", options_with(tables, Vec::new()));
    let source = TestSource::new(driver);

    let error = resolve_foreign_key_targets(&source, &request(&["a"], vec![Value::Integer(7)]))
        .await
        .expect_err("a constraint with two sides of different length is refused");

    assert!(
        error.to_string().starts_with("grid.fk.shapeMismatch"),
        "unexpected message: {error}"
    );
}

#[tokio::test]
async fn resolve_fk_targets_reports_an_unbound_session_without_a_grid_code() {
    let source = TestSource {
        driver: MockDriver::new("postgres", MockDriverOptions::default()),
        database: None,
        schema: None,
        failure: Some(FkSessionError::NotFound("DB session is unavailable".into())),
    };

    let error =
        resolve_foreign_key_targets(&source, &request(&["customer_id"], vec![Value::Integer(7)]))
            .await
            .expect_err("a missing session is reported");

    assert!(
        !error.to_string().starts_with("grid."),
        "a session failure is not a navigation rule failure: {error}"
    );
}

#[test]
fn find_fk_prefers_the_constraint_name_and_falls_back_to_the_ordered_columns() {
    let foreign_keys = vec![
        foreign_key("fk_a", &["a", "b"], "parents", &["x", "y"]),
        foreign_key("fk_b", &["a"], "parents", &["x"]),
    ];

    assert_eq!(
        find_fk(&foreign_keys, &["a".to_string()], Some("fk_a")).map(|fk| fk.name.as_str()),
        Some("fk_a")
    );
    assert_eq!(
        find_fk(&foreign_keys, &["a".to_string()], None).map(|fk| fk.name.as_str()),
        Some("fk_b")
    );
    // The pairing is positional: a different order does not match.
    assert!(find_fk(&foreign_keys, &["b".to_string(), "a".to_string()], None).is_none());
}
