//! Unit tests for the reverse foreign-key catalog (DB-20).

use std::collections::HashMap;
use std::sync::Arc;

use datazen_driver_api::mock_driver::{MockDriver, MockDriverOptions};
use datazen_driver_api::{
    ColumnSchema, ConnectionHandle, DatabaseDriver, ForeignKeyInfo, TableInfo, TableSchema,
};

use super::*;
use crate::services::fk_navigation::FkErrorCode;
use crate::services::ports::{FkSessionBinding, FkSessionError, ForeignKeyNavigationSource};

const DATABASE: &str = "app";

/// A source port that lends one session to the catalog scan.
struct TestSource {
    driver: Arc<dyn DatabaseDriver>,
    database: Option<String>,
}

impl TestSource {
    fn new(driver: Arc<dyn DatabaseDriver>) -> Self {
        Self {
            driver,
            database: Some(DATABASE.to_string()),
        }
    }
}

#[async_trait::async_trait]
impl ForeignKeyNavigationSource for TestSource {
    async fn bind_session(&self, _db_session_id: &str) -> Result<FkSessionBinding, FkSessionError> {
        Ok(FkSessionBinding {
            driver: Arc::clone(&self.driver),
            handle: ConnectionHandle {
                id: "handle-1".into(),
                pool_id: "pool-1".into(),
            },
            database: self.database.clone(),
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
        columns: columns.iter().map(|value| value.to_string()).collect(),
        referenced_table: referenced_table.to_string(),
        referenced_columns: referenced.iter().map(|value| value.to_string()).collect(),
        on_update: "NO ACTION".into(),
        on_delete: "NO ACTION".into(),
        deferrability: Default::default(),
    }
}

fn table_info(name: &str) -> TableInfo {
    TableInfo {
        name: name.to_string(),
        schema: None,
        table_type: datazen_driver_api::TableType::Table,
        row_count: None,
    }
}

fn schema(
    name: &str,
    columns: Vec<ColumnSchema>,
    primary_keys: Vec<String>,
    foreign_keys: Vec<ForeignKeyInfo>,
) -> TableSchema {
    TableSchema {
        table_name: name.to_string(),
        columns,
        primary_keys,
        indexes: Vec::new(),
        foreign_keys,
        check_constraints: Vec::new(),
        table_options: Default::default(),
    }
}

/// A catalog whose source is `customers` and whose referencing tables carry
/// the constraints the test asks for. Every referencing table gets the same
/// two columns (`id`, `customer_id`), so only the constraints differ.
fn driver_with(
    referencing: Vec<(String, Vec<ForeignKeyInfo>)>,
    source_columns: Vec<ColumnSchema>,
) -> Arc<MockDriver> {
    let source = schema(
        "customers",
        source_columns,
        vec!["id".to_string()],
        Vec::new(),
    );
    let mut tables: Vec<TableInfo> = vec![table_info("customers")];
    let mut schemas: HashMap<String, TableSchema> = HashMap::new();
    schemas.insert("customers".to_string(), source);
    for (name, foreign_keys) in referencing {
        tables.push(table_info(&name));
        schemas.insert(
            name.clone(),
            schema(
                &name,
                vec![
                    column("id", "integer", true),
                    column("customer_id", "integer", false),
                ],
                vec!["id".to_string()],
                foreign_keys,
            ),
        );
    }
    let mut by_database: HashMap<String, Vec<TableInfo>> = HashMap::new();
    by_database.insert(DATABASE.to_string(), tables);
    let mut schemas_by_database: HashMap<String, HashMap<String, TableSchema>> = HashMap::new();
    schemas_by_database.insert(DATABASE.to_string(), schemas);
    MockDriver::new(
        "postgres",
        MockDriverOptions {
            tables_by_database: by_database,
            table_schemas_by_database: schemas_by_database,
            ..Default::default()
        },
    )
}

fn request(table: &str) -> ListInboundForeignKeysRequest {
    ListInboundForeignKeysRequest {
        db_session_id: "session-1".into(),
        database: None,
        schema: None,
        table: table.to_string(),
        qualified_table: None,
    }
}

#[tokio::test]
async fn the_catalog_lists_the_constraints_that_reference_the_table() {
    let driver = driver_with(
        vec![
            (
                "orders".to_string(),
                vec![foreign_key(
                    "orders_customer_fk",
                    &["customer_id"],
                    "customers",
                    &["id"],
                )],
            ),
            (
                "audits".to_string(),
                vec![foreign_key(
                    "audits_customer_fk",
                    &["customer_id"],
                    "customers",
                    &["id"],
                )],
            ),
        ],
        vec![column("id", "integer", true)],
    );
    let source = TestSource::new(driver);

    let catalog = list_inbound_foreign_keys(&source, &request("customers"))
        .await
        .expect("the catalog is readable");

    assert_eq!(catalog.table, "customers");
    assert!(!catalog.truncated);
    let names: Vec<&str> = catalog
        .references
        .iter()
        .map(|entry| entry.table.as_str())
        .collect();
    assert_eq!(names, vec!["audits", "orders"], "ordered by table name");
    let orders = &catalog.references[1];
    assert_eq!(orders.constraint_name, "orders_customer_fk");
    assert_eq!(orders.columns, vec!["customer_id".to_string()]);
    assert_eq!(orders.referenced_columns, vec!["id".to_string()]);
}

#[tokio::test]
async fn the_scan_is_bounded_and_says_so() {
    // 250 referencing tables: the cap stops the scan at 200 and the flag
    // says the answer is partial — a catalog that silently stopped would
    // read as "nothing references this table".
    let referencing: Vec<(String, Vec<ForeignKeyInfo>)> = (0..250)
        .map(|index| {
            (
                format!("child_{index:03}"),
                vec![foreign_key("fk", &["customer_id"], "customers", &["id"])],
            )
        })
        .collect();
    let driver = driver_with(referencing, vec![column("id", "integer", true)]);
    let source = TestSource::new(driver);

    let catalog = list_inbound_foreign_keys(&source, &request("customers"))
        .await
        .expect("the catalog is readable");

    assert_eq!(catalog.scanned_tables, MAX_INBOUND_SCAN_TABLES);
    assert!(catalog.truncated);
    assert_eq!(catalog.references.len(), MAX_INBOUND_SCAN_TABLES);
}

#[tokio::test]
async fn the_source_itself_is_not_a_referencing_table() {
    // A self-referencing table is a jump the forward path already handles;
    // the reverse catalog answers "who points **here**" — and the source is
    // skipped even when it carries a constraint against itself.
    let mut by_database: HashMap<String, Vec<TableInfo>> = HashMap::new();
    by_database.insert(DATABASE.to_string(), vec![table_info("customers")]);
    let self_referencing = schema(
        "customers",
        vec![
            column("id", "integer", true),
            column("parent_id", "integer", false),
        ],
        vec!["id".to_string()],
        vec![foreign_key(
            "customers_parent_fk",
            &["parent_id"],
            "customers",
            &["id"],
        )],
    );
    let mut schemas: HashMap<String, TableSchema> = HashMap::new();
    schemas.insert("customers".to_string(), self_referencing);
    let mut schemas_by_database: HashMap<String, HashMap<String, TableSchema>> = HashMap::new();
    schemas_by_database.insert(DATABASE.to_string(), schemas);
    let driver = MockDriver::new(
        "postgres",
        MockDriverOptions {
            tables_by_database: by_database,
            table_schemas_by_database: schemas_by_database,
            ..Default::default()
        },
    );
    let source = TestSource::new(driver as Arc<dyn DatabaseDriver>);

    let catalog = list_inbound_foreign_keys(&source, &request("customers"))
        .await
        .expect("the catalog is readable");

    assert!(
        catalog.references.is_empty(),
        "no entry but the source itself"
    );
}

#[tokio::test]
async fn a_constraint_whose_referenced_columns_do_not_exist_is_dropped() {
    // The referencing table names `customers.zzz`; the source has no `zzz`.
    // A filter built on it could not render, so the entry is not offered.
    let driver = driver_with(
        vec![(
            "orders".to_string(),
            vec![foreign_key(
                "orders_customer_fk",
                &["customer_id"],
                "customers",
                &["zzz"],
            )],
        )],
        vec![column("id", "integer", true)],
    );
    let source = TestSource::new(driver);

    let catalog = list_inbound_foreign_keys(&source, &request("customers"))
        .await
        .expect("the catalog is readable");

    assert!(catalog.references.is_empty());
}

#[tokio::test]
async fn a_qualified_reference_matches_by_its_last_segment() {
    // PostgreSQL reports the reference as `sales.customers`; the request
    // names `customers`. Both normalize to the same table.
    let driver = driver_with(
        vec![(
            "orders".to_string(),
            vec![foreign_key(
                "orders_customer_fk",
                &["customer_id"],
                "sales.customers",
                &["id"],
            )],
        )],
        vec![column("id", "integer", true)],
    );
    let source = TestSource::new(driver);

    let catalog = list_inbound_foreign_keys(&source, &request("customers"))
        .await
        .expect("the catalog is readable");

    assert_eq!(catalog.references.len(), 1);
    assert_eq!(catalog.references[0].table, "orders");
}

#[tokio::test]
async fn an_empty_table_name_is_refused() {
    let driver = driver_with(Vec::new(), vec![column("id", "integer", true)]);
    let source = TestSource::new(driver);

    let error = list_inbound_foreign_keys(&source, &request("   "))
        .await
        .expect_err("an empty name cannot be scanned");

    match error {
        FkNavigationError::Coded { code, .. } => assert_eq!(code, FkErrorCode::ShapeMismatch),
        other => panic!("expected a coded rejection, got {other}"),
    }
}

/// The wire shape the front end consumes: camelCase, with the constraint and
/// the scan facts the UI needs to explain itself.
#[test]
fn the_catalog_type_is_serializable() {
    let catalog = InboundForeignKeyCatalog {
        table: "customers".into(),
        database: DATABASE.into(),
        references: vec![InboundForeignKeyRef {
            table: "orders".into(),
            schema: None,
            constraint_name: "fk".into(),
            columns: vec!["customer_id".into()],
            referenced_columns: vec!["id".into()],
        }],
        scanned_tables: 1,
        truncated: false,
    };
    let json = serde_json::to_value(&catalog).expect("the catalog serializes");
    assert_eq!(json["references"][0]["constraintName"], "fk");
    assert_eq!(
        json["references"][0]["referencedColumns"],
        serde_json::json!(["id"])
    );
    assert_eq!(json["scannedTables"], 1);
}
