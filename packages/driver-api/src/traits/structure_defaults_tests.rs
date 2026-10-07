//! Tests for the structural defaults of the driver contract.
//!
//! The assertions here do not test dialect behaviour but the shape of the
//! contract itself: which methods carry a default, what that default returns and
//! which of them are documented. They live in their own file because they are a
//! self-contained block that must not push the contract itself apart.

use super::*;
use crate::ReuseDriver;
use std::sync::Arc;

struct StubDriver;

#[async_trait]
impl DatabaseDriver for StubDriver {
    fn driver_type(&self) -> DatabaseType {
        "stub".to_string()
    }

    async fn connect(&self, _config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
        Ok(ConnectionHandle {
            id: "conn".into(),
            pool_id: "pool".into(),
        })
    }

    async fn test_connection(&self, _config: &ConnectionConfig) -> Result<ServerInfo, DriverError> {
        Ok(ServerInfo {
            server_version: String::new(),
            server_type: self.driver_type(),
        })
    }

    async fn disconnect(&self, _handle: ConnectionHandle) -> Result<(), DriverError> {
        Ok(())
    }

    async fn get_databases(&self, _handle: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        Ok(vec![])
    }

    async fn get_tables(
        &self,
        _handle: &ConnectionHandle,
        _database: &str,
        _schema: Option<&str>,
    ) -> Result<Vec<TableInfo>, DriverError> {
        Ok(vec![])
    }

    async fn get_table_schema(
        &self,
        _handle: &ConnectionHandle,
        _table: &str,
        _database: &str,
        _schema: Option<&str>,
    ) -> Result<TableSchema, DriverError> {
        Ok(TableSchema {
            table_name: String::new(),
            columns: vec![],
            primary_keys: vec![],
            indexes: vec![],
            foreign_keys: vec![],
            check_constraints: vec![],
            table_options: TableOptions::default(),
        })
    }

    async fn query(
        &self,
        _handle: &ConnectionHandle,
        _sql: &str,
    ) -> Result<QueryResult, DriverError> {
        Ok(QueryResult {
            columns: vec![],
            rows: vec![],
            rows_affected: None,
            execution_time_ms: 0,
        })
    }

    async fn query_multi(
        &self,
        _handle: &ConnectionHandle,
        _sql: &str,
        _limit: Option<u32>,
    ) -> Result<MultiQueryResult, DriverError> {
        Ok(MultiQueryResult {
            results: vec![],
            total_time_ms: 0,
        })
    }

    async fn query_with_params(
        &self,
        _handle: &ConnectionHandle,
        _sql: &str,
        _params: &[Value],
    ) -> Result<QueryResult, DriverError> {
        Ok(QueryResult {
            columns: vec![],
            rows: vec![],
            rows_affected: None,
            execution_time_ms: 0,
        })
    }

    async fn execute(&self, _handle: &ConnectionHandle, _sql: &str) -> Result<u64, DriverError> {
        Ok(0)
    }

    async fn cancel_query(&self, _handle: &ConnectionHandle) -> Result<(), DriverError> {
        Ok(())
    }
}

fn sample_request() -> StructureChangeRequest {
    StructureChangeRequest {
        mode: StructureChangeMode::Alter,
        schema: Some("public".into()),
        table: "users".into(),
        original_columns: vec![],
        current_columns: vec![],
        original_indexes: vec![],
        current_indexes: vec![],
    }
}

#[tokio::test]
async fn default_ddl_atomicity_is_unknown() {
    let driver = StubDriver;
    assert_eq!(driver.ddl_atomicity(), DdlAtomicity::Unknown);
}

#[tokio::test]
async fn default_fk_catalog_visibility_fails_closed() {
    let driver = StubDriver;
    let handle = ConnectionHandle {
        id: "conn".into(),
        pool_id: "pool".into(),
    };

    assert!(!driver
        .has_complete_foreign_key_catalog_visibility(&handle)
        .await
        .expect("default visibility capability"));
}

#[tokio::test]
async fn default_read_snapshot_fails_closed() {
    let driver = StubDriver;
    let handle = ConnectionHandle {
        id: "conn".into(),
        pool_id: "pool".into(),
    };
    let err = driver.begin_read_snapshot(&handle).await.unwrap_err();
    assert!(
        matches!(err, DriverError::Unsupported(message) if message.contains("stable read snapshots"))
    );
}

#[tokio::test]
async fn default_structure_capabilities_are_disabled() {
    let driver = StubDriver;
    let handle = ConnectionHandle {
        id: "conn".into(),
        pool_id: "pool".into(),
    };

    let caps = driver.structure_capabilities(&handle).await.unwrap();
    assert_eq!(caps.dialect_id, "stub");
    assert_eq!(caps.alter_strategy, AlterStrategy::None);
    assert!(!caps.create_table);
    assert!(!caps.add_column);
    assert!(!caps.drop_column);
    assert!(!caps.rename_column);
    assert!(!caps.alter_type);
    assert!(!caps.alter_nullability);
    assert!(!caps.alter_default);
    assert!(!caps.alter_primary_key);
    assert!(!caps.reorder_column);
    assert!(!caps.comment);
    assert!(!caps.create_index);
    assert!(!caps.drop_index);
    assert!(!caps.rebuild_index);
    assert!(!caps.index_type);
    assert!(!caps.index_include);
    assert!(!caps.index_filter);
    assert!(!caps.index_comment);
    assert!(caps.index_methods.is_empty());
}

#[tokio::test]
async fn default_plan_structure_changes_is_unsupported() {
    let driver = StubDriver;
    let handle = ConnectionHandle {
        id: "conn".into(),
        pool_id: "pool".into(),
    };
    let request = sample_request();

    let err = driver
        .plan_structure_changes(&handle, &request)
        .await
        .unwrap_err();
    assert!(
        matches!(err, DriverError::Unsupported(msg) if msg == "table structure planning is not supported by this driver")
    );
}

#[test]
fn stub_driver_sync_defaults() {
    let driver = StubDriver;
    assert_eq!(driver.sync_category(), SyncCategory::Sql);
    assert_eq!(driver.sync_family(), "stub");
}

#[tokio::test]
async fn reuse_driver_forwards_sync_taxonomy() {
    let inner: Arc<dyn DatabaseDriver> = Arc::new(StubDriver);
    let driver = ReuseDriver::new(inner, "reuse-stub");
    assert_eq!(driver.sync_category(), SyncCategory::Sql);
    assert_eq!(driver.sync_family(), "stub");
}

#[tokio::test]
async fn reuse_driver_forwards_structure_methods() {
    let inner: Arc<dyn DatabaseDriver> = Arc::new(StubDriver);
    let driver = ReuseDriver::new(inner, "reuse-stub");
    let handle = ConnectionHandle {
        id: "conn".into(),
        pool_id: "pool".into(),
    };

    let caps = driver.structure_capabilities(&handle).await.unwrap();
    assert_eq!(caps.dialect_id, "stub");
    assert_eq!(driver.driver_type(), "reuse-stub");

    let err = driver
        .plan_structure_changes(&handle, &sample_request())
        .await
        .unwrap_err();
    assert!(matches!(err, DriverError::Unsupported(_)));
}

#[tokio::test]
async fn get_columns_uses_effective_primary_keys_when_primary_keys_empty() {
    struct DriverWithColumnPkOnly;

    #[async_trait]
    impl DatabaseDriver for DriverWithColumnPkOnly {
        fn driver_type(&self) -> DatabaseType {
            "dummy".into()
        }

        async fn connect(&self, _: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
            unreachable!()
        }

        async fn test_connection(&self, _: &ConnectionConfig) -> Result<ServerInfo, DriverError> {
            unreachable!()
        }

        async fn disconnect(&self, _: ConnectionHandle) -> Result<(), DriverError> {
            Ok(())
        }

        async fn get_databases(&self, _: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
            Ok(vec![])
        }

        async fn get_tables(
            &self,
            _: &ConnectionHandle,
            _: &str,
            _: Option<&str>,
        ) -> Result<Vec<TableInfo>, DriverError> {
            Ok(vec![])
        }

        async fn get_table_schema(
            &self,
            _: &ConnectionHandle,
            _: &str,
            _: &str,
            _: Option<&str>,
        ) -> Result<TableSchema, DriverError> {
            Ok(TableSchema {
                table_name: "users".into(),
                columns: vec![ColumnSchema {
                    name: "id".into(),
                    data_type: "int".into(),
                    nullable: false,
                    default_value: None,
                    comment: None,
                    is_primary_key: true,
                    is_auto_increment: true,
                }],
                primary_keys: vec![], // intentionally empty to test fallback
                indexes: vec![],
                foreign_keys: vec![],
                check_constraints: vec![],
                table_options: TableOptions::default(),
            })
        }

        async fn query(&self, _: &ConnectionHandle, _: &str) -> Result<QueryResult, DriverError> {
            unreachable!()
        }

        async fn query_multi(
            &self,
            _: &ConnectionHandle,
            _: &str,
            _: Option<u32>,
        ) -> Result<MultiQueryResult, DriverError> {
            unreachable!()
        }

        async fn query_with_params(
            &self,
            _: &ConnectionHandle,
            _: &str,
            _: &[Value],
        ) -> Result<QueryResult, DriverError> {
            unreachable!()
        }

        async fn execute(&self, _: &ConnectionHandle, _: &str) -> Result<u64, DriverError> {
            unreachable!()
        }

        async fn cancel_query(&self, _: &ConnectionHandle) -> Result<(), DriverError> {
            Ok(())
        }
    }

    let driver = DriverWithColumnPkOnly;
    let handle = ConnectionHandle {
        id: "c".into(),
        pool_id: "p".into(),
    };
    let (_cols, pks) = driver
        .get_columns(&handle, "users", "app", None)
        .await
        .unwrap();
    assert_eq!(pks, vec!["id"]);
}
/// `has_schema_level` defaults to false, and the validator enforces the
/// capability/schema pairing in both directions.
mod validate_schema_target_tests {
    use super::*;

    struct SchemaLess;
    struct SchemaAware;

    macro_rules! stub_driver {
        ($name:ident, $schema_level:expr) => {
            #[async_trait]
            impl DatabaseDriver for $name {
                fn driver_type(&self) -> DatabaseType {
                    stringify!($name).to_lowercase()
                }
                fn has_schema_level(&self) -> bool {
                    $schema_level
                }
                async fn test_connection(
                    &self,
                    _: &ConnectionConfig,
                ) -> Result<ServerInfo, DriverError> {
                    unreachable!()
                }
                async fn connect(
                    &self,
                    _: &ConnectionConfig,
                ) -> Result<ConnectionHandle, DriverError> {
                    unreachable!()
                }
                async fn disconnect(&self, _: ConnectionHandle) -> Result<(), DriverError> {
                    unreachable!()
                }
                async fn get_databases(
                    &self,
                    _: &ConnectionHandle,
                ) -> Result<Vec<String>, DriverError> {
                    unreachable!()
                }
                async fn get_tables(
                    &self,
                    _: &ConnectionHandle,
                    _: &str,
                    _: Option<&str>,
                ) -> Result<Vec<TableInfo>, DriverError> {
                    unreachable!()
                }
                async fn get_table_schema(
                    &self,
                    _: &ConnectionHandle,
                    _: &str,
                    _: &str,
                    _: Option<&str>,
                ) -> Result<TableSchema, DriverError> {
                    unreachable!()
                }
                async fn query(
                    &self,
                    _: &ConnectionHandle,
                    _: &str,
                ) -> Result<QueryResult, DriverError> {
                    unreachable!()
                }
                async fn query_multi(
                    &self,
                    _: &ConnectionHandle,
                    _: &str,
                    _: Option<u32>,
                ) -> Result<MultiQueryResult, DriverError> {
                    unreachable!()
                }
                async fn query_with_params(
                    &self,
                    _: &ConnectionHandle,
                    _: &str,
                    _: &[Value],
                ) -> Result<QueryResult, DriverError> {
                    unreachable!()
                }
                async fn execute(&self, _: &ConnectionHandle, _: &str) -> Result<u64, DriverError> {
                    unreachable!()
                }
                async fn cancel_query(&self, _: &ConnectionHandle) -> Result<(), DriverError> {
                    unreachable!()
                }
            }
        };
    }

    stub_driver!(SchemaLess, false);
    stub_driver!(SchemaAware, true);

    #[test]
    fn schema_less_driver_accepts_only_none() {
        assert!(!SchemaLess.has_schema_level());
        assert!(validate_schema_target(&SchemaLess, "app", None, SchemaScope::AnySchema).is_ok());
        assert!(validate_schema_target(&SchemaLess, "app", None, SchemaScope::ExactSchema).is_ok());
        let err =
            validate_schema_target(&SchemaLess, "app", Some("public"), SchemaScope::AnySchema)
                .expect_err("schema-less driver must reject a schema");
        assert!(err.to_string().contains("no schema level"), "{err}");
        // Blank is treated as absent, not as a schema literally named "".
        assert!(
            validate_schema_target(&SchemaLess, "app", Some("  "), SchemaScope::AnySchema).is_ok()
        );
    }

    #[test]
    fn schema_aware_driver_requires_schema_only_when_resolving() {
        assert!(SchemaAware.has_schema_level());
        // Listing spans every schema, so None is legitimate here.
        assert!(validate_schema_target(&SchemaAware, "app", None, SchemaScope::AnySchema).is_ok());
        assert!(validate_schema_target(
            &SchemaAware,
            "app",
            Some("public"),
            SchemaScope::AnySchema
        )
        .is_ok());
        assert!(validate_schema_target(
            &SchemaAware,
            "app",
            Some("public"),
            SchemaScope::ExactSchema
        )
        .is_ok());
        let err = validate_schema_target(&SchemaAware, "app", None, SchemaScope::ExactSchema)
            .expect_err("schema-aware driver must require a schema when resolving one table");
        assert!(
            err.to_string().contains("explicit schema is required"),
            "{err}"
        );
        assert!(
            validate_schema_target(&SchemaAware, "app", Some(" "), SchemaScope::ExactSchema)
                .is_err()
        );
    }
}
