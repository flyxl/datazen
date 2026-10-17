//! MySQL restore format: DDL can commit independently, so never promise atomicity.
use datazen_driver_api::{iter_driver_factories, require_resource_provider, restore_sql};
use datazen_driver_mysql::MysqlDriver;

#[test]
fn mysql_restore_preflights_whole_input_and_refuses_false_single_transaction() {
    let driver = MysqlDriver::new(false);
    let profile = restore_sql::IMPLICIT_COMMIT;
    let sql = "CREATE TABLE `restore_probe`(id int, value text); INSERT INTO `restore_probe` VALUES(1, 'a;b');";
    let statements = restore_sql::prepare(
        &driver,
        sql,
        profile,
        false,
        true,
        &datazen_driver_api::namespace::NamespaceTarget {
            database: Some("app".into()),
            catalog: None,
            schema: None,
            path: vec![],
        },
    )
    .unwrap();
    assert_eq!(statements.len(), 3);
    assert_eq!(statements[0], "DROP TABLE IF EXISTS `restore_probe`");
    assert!(restore_sql::prepare(
        &driver,
        sql,
        profile,
        true,
        false,
        &datazen_driver_api::namespace::NamespaceTarget {
            database: Some("app".into()),
            catalog: None,
            schema: None,
            path: vec![]
        }
    )
    .is_err());
    for unsupported in [
        "USE other",
        "SET autocommit=0",
        "/*! SET FOREIGN_KEY_CHECKS=0 */",
        "BEGIN",
        "LOAD DATA INFILE 'fixture' INTO TABLE t",
        "CREATE DATABASE other",
        "CREATE/**/DATABASE other",
    ] {
        assert!(restore_sql::prepare(
            &driver,
            &format!("{sql}{unsupported};"),
            profile,
            false,
            false,
            &datazen_driver_api::namespace::NamespaceTarget {
                database: Some("app".into()),
                catalog: None,
                schema: None,
                path: vec![]
            }
        )
        .is_err());
    }
    let factory = iter_driver_factories()
        .into_iter()
        .find(|factory| factory.driver_id() == "mysql")
        .unwrap();
    let provider = require_resource_provider(*factory).unwrap();
    assert_eq!(
        provider.capabilities().snapshot.confirmed[restore_sql::CAPABILITY_KEY],
        profile
    );
}

#[derive(Default)]
struct Budget(std::sync::atomic::AtomicU32);
#[async_trait::async_trait]
impl datazen_driver_api::resource::BudgetPort for Budget {
    async fn acquire_control_connection(
        &self,
    ) -> Result<
        datazen_driver_api::resource::BudgetPermit,
        datazen_driver_api::resource::ResourceError,
    > {
        self.acquire_physical_connections(1).await
    }
    async fn acquire_physical_connections(
        &self,
        count: u32,
    ) -> Result<
        datazen_driver_api::resource::BudgetPermit,
        datazen_driver_api::resource::ResourceError,
    > {
        let id = self.0.fetch_add(count, std::sync::atomic::Ordering::SeqCst);
        Ok(datazen_driver_api::resource::BudgetPermit {
            permit_id: format!("restore-{id}"),
            physical_connections: count,
        })
    }
    async fn release_physical_connections(
        &self,
        permit: &datazen_driver_api::resource::BudgetPermit,
    ) -> Result<(), datazen_driver_api::resource::ResourceError> {
        self.0.fetch_sub(
            permit.physical_connections,
            std::sync::atomic::Ordering::SeqCst,
        );
        Ok(())
    }
}
#[derive(Default)]
struct Sink(std::sync::atomic::AtomicUsize);
#[async_trait::async_trait]
impl datazen_driver_api::resource::ResultSink for Sink {
    async fn write(
        &self,
        chunk: datazen_driver_api::resource::ResultChunk,
    ) -> Result<(), datazen_driver_api::resource::ResourceError> {
        self.0
            .fetch_add(chunk.rows.len(), std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }
    async fn write_output(
        &self,
        _: serde_json::Value,
    ) -> Result<(), datazen_driver_api::resource::ResourceError> {
        Ok(())
    }
    async fn complete(&self) -> Result<(), datazen_driver_api::resource::ResourceError> {
        Ok(())
    }
    async fn fail(&self, _: &str) -> Result<(), datazen_driver_api::resource::ResourceError> {
        Ok(())
    }
}

/// Explicit live journey; reads only process environment, and never prints errors or config.
#[tokio::test]
#[ignore = "requires a disposable TEST_MYSQL_DATABASE and TEST_MYSQL_HOST in process environment"]
async fn retained_resource_executes_restore_and_verifies_rows_before_closing() {
    use datazen_driver_api::resource::*;
    use datazen_driver_api::session::{CompletionStatus, ObservationConfidence};
    use std::sync::Arc;
    let required = |name: &str| {
        std::env::var(format!("TEST_MYSQL_{name}"))
            .unwrap_or_else(|_| panic!("missing live fixture environment"))
    };
    let config: datazen_driver_api::ConnectionConfig = serde_json::from_value(serde_json::json!({
        "id":"restore-live", "name":"restore-live", "databaseType":"mysql",
        "host":required("HOST"), "port":std::env::var("TEST_MYSQL_PORT").ok().and_then(|p|p.parse::<u16>().ok()).unwrap_or(3306),
        "database":required("DATABASE"), "schema":None::<String>,
        "username":required("USER"), "password":std::env::var("TEST_MYSQL_PASSWORD").ok(),
        "connectionTimeout":5, "maxPoolSize":1, "readOnly":false, "pinned":false
    })).unwrap_or_else(|_| panic!("invalid synthetic fixture configuration"));
    let factory = iter_driver_factories()
        .into_iter()
        .find(|f| f.driver_id() == "mysql")
        .unwrap();
    let driver = factory.create();
    let provider = require_resource_provider(*factory).unwrap();
    let budget = Arc::new(Budget::default());
    let port: Arc<dyn BudgetPort> = budget.clone();
    let handle = provider
        .acquire_resource(
            &AcquireResourceRequest {
                connection_config: config.clone(),
                identity_scope: IdentityScope::default(),
                target: datazen_driver_api::namespace::NamespaceTarget {
                    database: config.database.clone(),
                    catalog: None,
                    schema: config.schema.clone(),
                    path: vec![],
                },
                scope: ResourceScope {
                    purpose: ResourcePurpose::InteractiveQuery,
                    max_physical_connections: 4,
                    holds_open_transaction: false,
                    pin_for_streaming: false,
                },
                baseline: Baseline::default(),
            },
            &port,
        )
        .await
        .unwrap_or_else(|_| panic!("restore fixture resource acquisition failed"));
    let observation = provider
        .observe_session(&handle)
        .await
        .unwrap_or_else(|_| panic!("restore fixture observation failed"));
    assert_eq!(
        observation.context.confidence,
        ObservationConfidence::Confirmed
    );
    assert_eq!(observation.context.namespace.database, config.database);
    let table = format!("restore_probe_{}", uuid::Uuid::new_v4().simple());
    let script = format!("CREATE TABLE {table}(id int PRIMARY KEY); INSERT INTO {table} VALUES(7); INSERT INTO {table} VALUES(8);");
    let profile = provider.capabilities().snapshot.confirmed[restore_sql::CAPABILITY_KEY].clone();
    let statements = restore_sql::prepare(
        driver.as_ref(),
        &script,
        &profile,
        false,
        true,
        &datazen_driver_api::namespace::NamespaceTarget {
            database: Some("app".into()),
            catalog: None,
            schema: None,
            path: vec![],
        },
    )
    .unwrap();
    for (index, sql) in statements.into_iter().enumerate() {
        let result = provider
            .execute_on_resource(
                &handle,
                &datazen_driver_api::QueryExecutionId::new(format!("restore-{index}")),
                &CommandCall::new("execute", serde_json::json!({"sql":sql})),
                &Sink::default(),
            )
            .await;
        assert!(result.is_ok(), "retained restore statement failed");
        assert_eq!(
            result.unwrap().completion_status,
            CompletionStatus::Succeeded
        );
    }
    let verification_sink = Sink::default();
    let verified = provider
        .execute_on_resource(
            &handle,
            &datazen_driver_api::QueryExecutionId::new("restore-verify"),
            &CommandCall::new(
                "query",
                serde_json::json!({"sql":format!("SELECT id FROM {table} ORDER BY id")}),
            ),
            &verification_sink,
        )
        .await
        .unwrap_or_else(|_| panic!("restore verification failed"));
    assert_eq!(verified.completion_status, CompletionStatus::Succeeded);
    assert_eq!(
        verification_sink
            .0
            .load(std::sync::atomic::Ordering::SeqCst),
        2
    );
    let cleanup = provider
        .execute_on_resource(
            &handle,
            &datazen_driver_api::QueryExecutionId::new("restore-cleanup"),
            &CommandCall::new(
                "execute",
                serde_json::json!({"sql":format!("DROP TABLE {table}")}),
            ),
            &Sink::default(),
        )
        .await;
    assert!(cleanup.is_ok(), "restore fixture cleanup failed");
    assert!(provider.close_resource(&handle).await.is_ok());
    assert_eq!(budget.0.load(std::sync::atomic::Ordering::SeqCst), 0);
}

#[test]
fn frozen_restore_target_rejects_every_foreign_object_before_returning_any_script() {
    let driver = MysqlDriver::new(false);
    let target = datazen_driver_api::namespace::NamespaceTarget {
        database: Some("app".into()),
        catalog: None,
        schema: None,
        path: vec![],
    };
    let own =
        "CREATE TABLE app.probe(id int); INSERT INTO app.probe VALUES(1); DROP TABLE app.probe;";
    assert!(restore_sql::prepare(
        &driver,
        own,
        restore_sql::IMPLICIT_COMMIT,
        false,
        false,
        &target
    )
    .is_ok());
    for foreign in [
        "CREATE TABLE other.probe(id int)",
        "INSERT INTO other.probe VALUES(1)",
        "INSERT INTO other.app.probe VALUES(1)",
        "CREATE TABLE app.probe LIKE other.parent",
        "DROP TABLE app.probe, other.probe",
        "CREATE VIEW other.probe AS SELECT 1",
        "CREATE TABLE `app.other`.probe(id int)",
        "ALTER TABLE app.probe RENAME TO other.probe",
    ] {
        // A valid first statement must never cause execution before discovering
        // a later foreign target. The entire preflight returns no script.
        let script = format!("CREATE TABLE app.first_probe(id int); {foreign};");
        for overwrite in [false, true] {
            assert!(restore_sql::prepare(
                &driver,
                &script,
                restore_sql::IMPLICIT_COMMIT,
                false,
                overwrite,
                &target
            )
            .is_err());
        }
    }
}
