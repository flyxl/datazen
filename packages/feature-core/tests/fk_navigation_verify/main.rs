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
        browse: false,
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

// The cases live in submodules so that no single file outgrows the
// repository's 800-line ceiling. Every assertion is the one this harness
// was written with.
mod codes;
mod composite;
mod lookup;
mod success;
mod target_name;
mod truncated;
mod wire;
