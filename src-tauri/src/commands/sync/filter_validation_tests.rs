use super::filter_validation::{
    resolve_key_contracts, validate_filter_endpoints, validate_filter_schemas,
};
use crate::data_sync::SyncSourceFilter;
use crate::db::{ColumnSchema, TableSchema};
use crate::testing::mock_driver::{MockDriver, MockDriverOptions};
use datazen_driver_mysql::{MysqlDriver, MysqlSyncAdapter};
use datazen_driver_postgres::{PgSyncAdapter, PostgresDriver};

fn schema(key_types: &[(&str, &str)]) -> TableSchema {
    TableSchema {
        table_name: "events".into(),
        columns: key_types
            .iter()
            .map(|(name, data_type)| ColumnSchema {
                name: (*name).into(),
                data_type: (*data_type).into(),
                nullable: false,
                default_value: None,
                comment: None,
                is_primary_key: true,
                is_auto_increment: false,
            })
            .collect(),
        primary_keys: key_types.iter().map(|(name, _)| (*name).into()).collect(),
        indexes: vec![],
        foreign_keys: vec![],
        check_constraints: vec![],
        table_options: Default::default(),
    }
}

fn tuple_filter(start: [&str; 2], end: [&str; 2]) -> SyncSourceFilter {
    serde_json::from_value(serde_json::json!({
        "filters": [],
        "recordset": {
            "tupleRange": {
                "columns": ["tenant_id", "id"],
                "start": {"values": [start[0], start[1]]},
                "end": {"values": [end[0], end[1]]}
            }
        }
    }))
    .expect("valid tuple filter JSON")
}

fn keys() -> Vec<String> {
    vec!["tenant_id".into(), "id".into()]
}

fn sqlserver_filter_schema(filter_type: &str) -> TableSchema {
    TableSchema {
        table_name: "people".into(),
        columns: vec![
            ColumnSchema {
                name: "id".into(),
                data_type: "int".into(),
                nullable: false,
                default_value: None,
                comment: None,
                is_primary_key: true,
                is_auto_increment: false,
            },
            ColumnSchema {
                name: "name".into(),
                data_type: filter_type.into(),
                nullable: true,
                default_value: None,
                comment: None,
                is_primary_key: false,
                is_auto_increment: false,
            },
            ColumnSchema {
                name: "age".into(),
                data_type: "int".into(),
                nullable: false,
                default_value: None,
                comment: None,
                is_primary_key: false,
                is_auto_increment: false,
            },
        ],
        primary_keys: vec!["id".into()],
        indexes: vec![],
        foreign_keys: vec![],
        check_constraints: vec![],
        table_options: Default::default(),
    }
}

fn scalar_filter(column: &str, operator: &str, value: serde_json::Value) -> SyncSourceFilter {
    serde_json::from_value(serde_json::json!({
        "filters": [{"column": column, "operator": operator, "value": value}]
    }))
    .expect("valid scalar sync filter JSON")
}

fn validate_sqlserver_filter(
    filter: &SyncSourceFilter,
    source: &TableSchema,
    target: &TableSchema,
) -> Result<(), String> {
    let driver = MockDriver::new(
        "sqlserver",
        MockDriverOptions {
            parameterized_writes: true,
            ..MockDriverOptions::default()
        },
    );
    let adapter = PgSyncAdapter;
    let key_columns = vec!["id".into()];
    let (source_contracts, target_contracts) = resolve_key_contracts(
        &key_columns,
        &adapter,
        &adapter,
        source,
        target,
        "people",
        "people",
    )
    .map_err(|error| error.to_string())?;
    validate_filter_endpoints(
        filter,
        &key_columns,
        driver.as_ref(),
        driver.as_ref(),
        &adapter,
        &adapter,
        source,
        target,
        &source_contracts,
        &target_contracts,
        "people",
    )
    .map_err(|error| error.to_string())
}

#[test]
fn sqlserver_text_filter_is_rejected_when_collation_parity_is_unrepresented() {
    let source = sqlserver_filter_schema("nvarchar(64)");
    let target = source.clone();
    let filter = scalar_filter("name", "eq", serde_json::json!("Ada"));

    let error = validate_sqlserver_filter(&filter, &source, &target).unwrap_err();

    assert!(
        error.contains("default/per-column collation parity"),
        "{error}"
    );
}

#[test]
fn sqlserver_non_text_filter_is_safe_without_collation_metadata() {
    let source = sqlserver_filter_schema("nvarchar(64)");
    let target = source.clone();
    let filter = scalar_filter("age", "gte", serde_json::json!(18));

    validate_sqlserver_filter(&filter, &source, &target).unwrap();
}

#[test]
fn sqlserver_text_null_check_is_safe_without_collation_metadata() {
    let source = sqlserver_filter_schema("nvarchar(64)");
    let target = source.clone();
    let filter = scalar_filter("name", "isNull", serde_json::Value::Null);

    validate_sqlserver_filter(&filter, &source, &target).unwrap();
}

#[test]
fn sqlserver_filter_rejects_unknown_alias_type_that_may_be_textual() {
    let source = sqlserver_filter_schema("[dbo].[PersonName]");
    let target = source.clone();
    let filter = scalar_filter("name", "like", serde_json::json!("A%"));

    let error = validate_sqlserver_filter(&filter, &source, &target).unwrap_err();

    assert!(error.contains("cannot verify whether this type uses text collation semantics"));
}

#[test]
fn test_tester_filter_schema_validation_checks_both_endpoints() {
    let filter = tuple_filter(["1", "10"], ["2", "20"]);
    let source = schema(&[("tenant_id", "INTEGER"), ("id", "BIGINT")]);
    validate_filter_schemas(&filter, &source, &source, "events", "events").unwrap();

    let mut target = source.clone();
    target.primary_keys.reverse();
    let error = validate_filter_schemas(&filter, &source, &target, "events", "archive_events")
        .unwrap_err()
        .to_string();
    assert!(error.contains("archive_events"));
}

#[test]
fn test_tester_key_contract_resolution_rejects_missing_and_incompatible_keys() {
    let source_adapter = PgSyncAdapter;
    let target_adapter = PgSyncAdapter;
    let source = schema(&[("tenant_id", "INTEGER"), ("id", "BIGINT")]);
    let target = source.clone();
    let key_columns = keys();
    let (source_contracts, target_contracts) = resolve_key_contracts(
        &key_columns,
        &source_adapter,
        &target_adapter,
        &source,
        &target,
        "events",
        "events",
    )
    .unwrap();
    assert_eq!(source_contracts, target_contracts);

    assert!(resolve_key_contracts(
        &["absent".into()],
        &source_adapter,
        &target_adapter,
        &source,
        &target,
        "events",
        "events",
    )
    .is_err());

    let incompatible_target = schema(&[("tenant_id", "INTEGER"), ("id", "TEXT")]);
    assert!(resolve_key_contracts(
        &key_columns,
        &source_adapter,
        &target_adapter,
        &source,
        &incompatible_target,
        "events",
        "archive_events",
    )
    .is_err());

    let missing_target = schema(&[("tenant_id", "INTEGER")]);
    assert!(resolve_key_contracts(
        &key_columns,
        &source_adapter,
        &target_adapter,
        &source,
        &missing_target,
        "events",
        "archive_events",
    )
    .is_err());
}

#[test]
fn test_tester_endpoint_validation_accepts_matching_postgres_range_and_rejects_bad_bounds() {
    let driver = PostgresDriver::new();
    let adapter = PgSyncAdapter;
    let source = schema(&[("tenant_id", "INTEGER"), ("id", "BIGINT")]);
    let target = source.clone();
    let key_columns = keys();
    let (source_contracts, target_contracts) = resolve_key_contracts(
        &key_columns,
        &adapter,
        &adapter,
        &source,
        &target,
        "events",
        "events",
    )
    .unwrap();
    let valid = tuple_filter(["1", "10"], ["2", "20"]);
    validate_filter_endpoints(
        &valid,
        &key_columns,
        &driver,
        &driver,
        &adapter,
        &adapter,
        &source,
        &target,
        &source_contracts,
        &target_contracts,
        "events",
    )
    .unwrap();

    let reversed = tuple_filter(["2", "10"], ["1", "20"]);
    assert!(validate_filter_endpoints(
        &reversed,
        &key_columns,
        &driver,
        &driver,
        &adapter,
        &adapter,
        &source,
        &target,
        &source_contracts,
        &target_contracts,
        "events",
    )
    .is_err());

    let invalid_type = tuple_filter(["not-an-integer", "10"], ["2", "20"]);
    assert!(validate_filter_endpoints(
        &invalid_type,
        &key_columns,
        &driver,
        &driver,
        &adapter,
        &adapter,
        &source,
        &target,
        &source_contracts,
        &target_contracts,
        "events",
    )
    .is_err());
}

#[test]
fn test_tester_endpoint_validation_fails_closed_when_sql_ordering_differs() {
    let source_driver = PostgresDriver::new();
    let target_driver = MysqlDriver::new(false);
    let source_adapter = PgSyncAdapter;
    let target_adapter = MysqlSyncAdapter { is_mariadb: false };
    let source = schema(&[("tenant_id", "VARCHAR(20)"), ("id", "VARCHAR(20)")]);
    let target = source.clone();
    let key_columns = keys();
    let (source_contracts, target_contracts) = resolve_key_contracts(
        &key_columns,
        &source_adapter,
        &target_adapter,
        &source,
        &target,
        "events",
        "events",
    )
    .unwrap();
    let filter = tuple_filter(["a", "10"], ["b", "20"]);
    let error = validate_filter_endpoints(
        &filter,
        &key_columns,
        &source_driver,
        &target_driver,
        &source_adapter,
        &target_adapter,
        &source,
        &target,
        &source_contracts,
        &target_contracts,
        "events",
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("ordering expressions"));
}

#[test]
fn test_tester_key_contract_resolver_reports_driver_rejection() {
    let adapter = PgSyncAdapter;
    let unsupported = schema(&[("tenant_id", "JSONB"), ("id", "BIGINT")]);
    let keys = keys();
    let error = resolve_key_contracts(
        &keys,
        &adapter,
        &adapter,
        &unsupported,
        &unsupported,
        "events",
        "events",
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("tenant_id"));
}

#[test]
fn test_tester_endpoint_validation_rejects_cross_dialect_ordering_expressions() {
    let source_driver = PostgresDriver::new();
    let target_driver = MysqlDriver::new(false);
    let source_adapter = PgSyncAdapter;
    let target_adapter = MysqlSyncAdapter { is_mariadb: false };
    let source = schema(&[("tenant_id", "INTEGER"), ("id", "BIGINT")]);
    let target = source.clone();
    let key_columns = keys();
    let (source_contracts, target_contracts) = resolve_key_contracts(
        &key_columns,
        &source_adapter,
        &target_adapter,
        &source,
        &target,
        "events",
        "events",
    )
    .unwrap();
    let filter = tuple_filter(["1", "10"], ["2", "20"]);
    assert!(validate_filter_endpoints(
        &filter,
        &key_columns,
        &source_driver,
        &target_driver,
        &source_adapter,
        &target_adapter,
        &source,
        &target,
        &source_contracts,
        &target_contracts,
        "events",
    )
    .is_err());
}

#[test]
fn test_tester_endpoint_validation_reports_unavailable_parameter_binding() {
    let driver = crate::testing::mock_driver::MockDriver::new(
        "postgresql",
        crate::testing::mock_driver::MockDriverOptions::default(),
    );
    let adapter = PgSyncAdapter;
    let source = schema(&[("tenant_id", "INTEGER"), ("id", "BIGINT")]);
    let keys = keys();
    let (source_contracts, target_contracts) = resolve_key_contracts(
        &keys, &adapter, &adapter, &source, &source, "events", "events",
    )
    .unwrap();
    let filter = tuple_filter(["1", "10"], ["2", "20"]);
    let error = validate_filter_endpoints(
        &filter,
        &keys,
        driver.as_ref(),
        driver.as_ref(),
        &adapter,
        &adapter,
        &source,
        &source,
        &source_contracts,
        &target_contracts,
        "events",
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("cannot execute sync filter"));
}

#[test]
fn test_tester_endpoint_validation_rejects_unmapped_tuple_columns() {
    let incompatible_source = schema(&[("tenant_id", "INTEGER"), ("id", "BIGINT")]);
    let key_columns = keys();
    let invalid_filter: SyncSourceFilter = serde_json::from_value(serde_json::json!({
        "recordset": {"tupleRange": {
            "columns": ["wrong", "id"],
            "start": {"values": ["1", "10"]}
        }}
    }))
    .unwrap();
    let source_driver = PostgresDriver::new();
    let target_driver = PostgresDriver::new();
    let source_adapter = PgSyncAdapter;
    let target_adapter = PgSyncAdapter;
    let (source_contracts, target_contracts) = resolve_key_contracts(
        &key_columns,
        &source_adapter,
        &target_adapter,
        &incompatible_source,
        &incompatible_source,
        "events",
        "events",
    )
    .unwrap();
    assert!(validate_filter_endpoints(
        &invalid_filter,
        &key_columns,
        &source_driver,
        &target_driver,
        &source_adapter,
        &target_adapter,
        &incompatible_source,
        &incompatible_source,
        &source_contracts,
        &target_contracts,
        "events",
    )
    .is_err());
}
