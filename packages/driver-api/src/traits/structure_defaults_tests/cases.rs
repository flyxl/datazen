//! Tests for the structural defaults of the driver contract.
//!
//! The assertions here do not test dialect behaviour but the shape of the
//! contract itself: which methods carry a default, what that default returns and
//! which of them are documented. They live in their own file because they are a
//! self-contained block that must not push the contract itself apart.
//!
//! The parent module in `traits.rs` declares them with inline braces instead of a
//! bare `mod x;` declaration, because `clippy::items_after_test_module` only sees a
//! test module that owns an inner span: the lint requires
//! `item.span.hi() == test_mod.spans.inner_span.hi()`, and a declaration without
//! braces has a dummy inner span. That blindness is precisely how the contract
//! sitting after these tests came to be reported nowhere, so the braces are kept to
//! put the lint back in charge of this file's layout, and this file keeps the bulk
//! out of line so `traits.rs` stays readable. (There is no line-count ceiling on
//! it any more: file size is a coupling signal for code review, not a CI gate.)

use super::super::*;
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

/// The filter half of the contract: a driver that overrides nothing must keep
/// the behaviour it had before the filter contract existed — no dialect
/// operator, no `LIKE` escaping — while the whitelist it reports is the
/// shared set the host can render on its own.
mod filter_defaults {
    use super::*;
    use crate::filters::{FilterOperator, DIALECT_FILTER_OPERATORS, SHARED_FILTER_OPERATORS};
    use crate::mock_driver::{MockDriver, MockDriverOptions};

    fn mock() -> std::sync::Arc<MockDriver> {
        MockDriver::new("sqlite", MockDriverOptions::default())
    }

    #[test]
    fn default_supported_filter_operators_is_shared_set() {
        assert_eq!(mock().supported_filter_operators(), SHARED_FILTER_OPERATORS);
    }

    #[test]
    fn default_filter_operator_sql_is_none() {
        let driver = mock();
        for operator in DIALECT_FILTER_OPERATORS {
            assert!(
                driver
                    .filter_operator_sql(*operator, "\"name\"", "'^a'")
                    .is_none(),
                "{operator:?} must not be renderable without a driver implementation"
            );
        }
    }

    #[test]
    fn default_like_is_case_sensitive_flag_is_false() {
        assert!(!mock().like_is_case_insensitive());
    }

    #[test]
    fn default_like_escape_configuration_is_off() {
        let driver = mock();
        assert_eq!(driver.like_pattern_escape(), None);
        assert_eq!(driver.like_escape_clause('!'), None);
    }

    #[test]
    fn dialect_operators_are_absent_from_the_shared_whitelist() {
        for operator in DIALECT_FILTER_OPERATORS {
            assert!(
                !SHARED_FILTER_OPERATORS.contains(operator),
                "{operator:?} must be declared explicitly by a driver"
            );
        }
        assert_eq!(mock().supported_filter_operators().len(), 19);
        assert!(!mock()
            .supported_filter_operators()
            .contains(&FilterOperator::Regex));
    }
}

/// The browsing half of the contract: a driver that overrides nothing must
/// keep the behaviour it had before the count strategy and the default order
/// existed — an exact count, no estimate, the primary key (or the first
/// column) as the order.
/// The keyset half of the contract, as a **default**.
///
/// The driver used here does not implement the two keyset methods, so what
/// it answers is the trait's own default — not a mock's option, and not a
/// driver's choice. That is the whole point: a test that goes through
/// `MockDriver` cannot see the default at all, because the mock overrides
/// both methods, and a default that was flipped to `true` would leave every
/// suite green.
mod keyset_defaults {
    use super::*;
    use crate::traits::keyset;

    #[test]
    fn a_driver_that_says_nothing_cannot_seek() {
        let driver = StubDriver;
        assert!(
            !driver.supports_keyset_pagination(),
            "the capability must be off by default: a driver that did not opt in must keep reading pages by OFFSET"
        );
    }

    #[test]
    fn a_driver_that_says_nothing_has_no_condition() {
        let driver = StubDriver;
        assert_eq!(
            driver.keyset_predicate_sql(
                &[("id".to_string(), false)],
                &[("id".to_string(), crate::types::Value::Integer(42))],
            ),
            None,
            "a driver that did not opt in must never be asked for a condition"
        );
    }

    #[test]
    fn the_default_bodies_themselves_are_off() {
        // The two assertions above go through the trait; these go straight
        // to the bodies, so the default is pinned even if a driver starts
        // overriding it.
        let driver = StubDriver;
        assert!(!keyset::supports_keyset_pagination(&driver));
        assert_eq!(
            keyset::keyset_predicate_sql(
                &driver,
                &[("id".to_string(), false)],
                &[("id".to_string(), crate::types::Value::Integer(42))],
            ),
            None
        );
    }
}

mod browsing_defaults {
    use super::*;
    use crate::mock_driver::{MockDriver, MockDriverOptions};
    use crate::types::CountStrategy;
    use std::sync::Arc;

    fn mock(options: MockDriverOptions) -> Arc<MockDriver> {
        MockDriver::new("sqlite", options)
    }

    fn column(name: &str, is_primary_key: bool) -> ColumnSchema {
        ColumnSchema {
            name: name.to_string(),
            data_type: "text".to_string(),
            nullable: true,
            default_value: None,
            comment: None,
            is_primary_key,
            is_auto_increment: false,
        }
    }

    #[test]
    fn count_strategy_defaults_to_exact_when_skip_count_query_is_false() {
        let driver = mock(MockDriverOptions::default());
        assert_eq!(driver.count_strategy(), CountStrategy::Exact);
    }

    #[test]
    fn count_strategy_defaults_to_unsupported_when_skip_count_query_is_true() {
        // The compatibility argument of the whole contract: a driver that
        // skips the count keeps skipping it, and every other driver keeps
        // counting exactly.
        let driver = mock(MockDriverOptions {
            skip_count_query: true,
            ..Default::default()
        });
        assert!(driver.skip_count_query());
        assert_eq!(driver.count_strategy(), CountStrategy::Unsupported);
    }

    #[test]
    fn count_estimate_sql_defaults_to_none() {
        let driver = mock(MockDriverOptions::default());
        assert_eq!(driver.count_estimate_sql("app", None, "orders"), None);
        assert_eq!(
            driver.count_estimate_sql("app", Some("public"), "orders"),
            None
        );
    }

    #[test]
    fn default_order_columns_prefers_the_primary_keys_argument() {
        // The primary keys arrive normalised, so a column without the flag
        // still orders by the real key.
        let driver = mock(MockDriverOptions::default());
        let columns = vec![column("col_a", false), column("col_b", false)];
        assert_eq!(
            driver.default_order_columns(&columns, &["id".to_string()]),
            vec!["id".to_string()]
        );
    }

    #[test]
    fn default_order_columns_falls_back_to_the_first_column() {
        let driver = mock(MockDriverOptions::default());
        let columns = vec![column("col_a", false), column("col_b", false)];
        assert_eq!(
            driver.default_order_columns(&columns, &[]),
            vec!["col_a".to_string()]
        );
    }

    #[test]
    fn default_order_is_empty_when_columns_are_empty() {
        let driver = mock(MockDriverOptions::default());
        assert!(driver.default_order_columns(&[], &[]).is_empty());
    }

    #[test]
    fn allow_first_column_fallback_order_defaults_to_true() {
        assert!(mock(MockDriverOptions::default()).allow_first_column_fallback_order());
    }

    #[test]
    fn allow_first_column_fallback_order_can_be_switched_off() {
        // The switch the host's gate reads: a driver that forbids the
        // fallback has to be able to say so.
        let driver = mock(MockDriverOptions {
            allow_first_column_fallback_order: Some(false),
            ..Default::default()
        });
        assert!(!driver.allow_first_column_fallback_order());
    }

    /// The wrapper must not lose the wrapped driver's rules: a reuse alias
    /// that quietly fell back to the trait default would drop the estimate
    /// and the default order of the engine it stands in for.
    ///
    /// The expectations are **written out**, not compared with the inner
    /// driver's own answers. A comparison of "wrapper == inner" proves
    /// nothing here: the inner driver answers the *trait defaults* unless it
    /// is told otherwise, so a wrapper that did not forward at all would
    /// still answer the same thing. The inner driver therefore overrides
    /// every method of the contract, and the wrapper has to carry those
    /// overrides over.
    #[test]
    fn reuse_driver_forwards_browsing_strategy_methods() {
        use crate::ReuseDriver;

        let inner = mock(MockDriverOptions {
            skip_count_query: false,
            count_strategy: Some(CountStrategy::Estimated),
            count_estimate_rows: Some(4242),
            default_order_columns: Some(vec!["wrapped_order".to_string()]),
            allow_first_column_fallback_order: Some(false),
            ..Default::default()
        });
        let wrapper = ReuseDriver::new(inner, "alias");

        // Every answer below differs from the trait default, so a wrapper
        // that stopped forwarding would answer something else and the test
        // would fail.
        assert_eq!(wrapper.count_strategy(), CountStrategy::Estimated);
        assert_eq!(
            wrapper.count_estimate_sql("app", Some("public"), "orders"),
            Some("SELECT estimated_row_count FROM \"orders\"".to_string())
        );
        assert_eq!(
            wrapper.default_order_columns(&[], &[]),
            vec!["wrapped_order".to_string()]
        );
        assert!(!wrapper.allow_first_column_fallback_order());
    }
}
