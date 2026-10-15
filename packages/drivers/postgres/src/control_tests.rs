//! Destructive live regressions: run only against a disposable PostgreSQL instance.
use super::*;
use async_trait::async_trait;
use std::sync::atomic::{AtomicU32, Ordering};

#[derive(Default)]
struct Ledger {
    charged: AtomicU32,
    released: AtomicU32,
}
#[async_trait]
impl BudgetPort for Ledger {
    async fn acquire_physical_connections(
        &self,
        count: u32,
    ) -> Result<BudgetPermit, datazen_driver_api::resource::ResourceError> {
        let id = self.charged.fetch_add(count, Ordering::SeqCst);
        Ok(BudgetPermit {
            permit_id: id.to_string(),
            physical_connections: count,
        })
    }
    async fn release_physical_connections(
        &self,
        permit: &BudgetPermit,
    ) -> Result<(), datazen_driver_api::resource::ResourceError> {
        self.released
            .fetch_add(permit.physical_connections, Ordering::SeqCst);
        Ok(())
    }
}
fn config() -> ConnectionConfig {
    // Only process variables; never read local credential files.
    serde_json::from_value(serde_json::json!({
        "id":"control-live", "name":"control live", "databaseType":"postgresql",
        "host":std::env::var("TEST_PG_HOST").unwrap_or_else(|_| "127.0.0.1".into()),
        "port":std::env::var("TEST_PG_PORT").expect("disposable test port").parse::<u16>().unwrap(),
        "username":std::env::var("TEST_PG_USER").expect("disposable test user"),
        "database":"postgres", "sslMode":"disable", "connectionTimeout":1,
        "maxPoolSize":1
    }))
    .expect("test config")
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL: TEST_PG_PORT and TEST_PG_USER"]
async fn live_shared_control_races_last_close_and_dropped_anchor_recovery() {
    let mut anchor = config();
    let admin = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(build_pg_options(&anchor).unwrap())
        .await
        .expect("admin connect");
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let first = format!("ctl_a_{suffix}");
    let second = format!("ctl_b_{suffix}");
    for db in [&first, &second] {
        sqlx::query(&format!("CREATE DATABASE {db}"))
            .execute(&admin)
            .await
            .expect("create database");
    }
    anchor.database = Some(first.clone());
    let mut target = anchor.clone();
    target.database = Some(second.clone());
    let driver = Arc::new(PostgresDriver::new());
    let ledger = Arc::new(Ledger::default());
    let budget: Arc<dyn BudgetPort> = ledger.clone();
    let (a, b) = tokio::join!(
        driver.connect_with_control(&anchor, &anchor, Some(budget.clone())),
        driver.open_fixed_session_with_control(&target, &anchor, Some(budget.clone()))
    );
    let a = a.expect("first resource");
    let b = b.expect("second resource");
    assert_eq!(
        ledger.charged.load(Ordering::SeqCst),
        1,
        "one shared control charge"
    );
    assert_eq!(driver.control_pools.read().await.len(), 1);
    let live_sockets: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_catalog.pg_stat_activity WHERE datname = ANY($1)",
    )
    .bind(vec![first.clone(), second.clone()])
    .fetch_one(&admin)
    .await
    .unwrap();
    assert_eq!(
        live_sockets, 3,
        "two data backends plus one shared control backend"
    );
    let old = driver.control_pool_for(&b).await.expect("shared pool");
    let pid_a: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&old)
        .await
        .unwrap();
    let pid_b: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&driver.control_pool_for(&a).await.unwrap())
        .await
        .unwrap();
    assert_eq!(
        pid_a, pid_b,
        "cross-database resources use one physical control backend"
    );
    driver.disconnect_impl(a).await.unwrap();
    assert_eq!(ledger.released.load(Ordering::SeqCst), 0);
    let execution = datazen_driver_api::QueryExecutionId::new("prepared-before-recovery");
    driver
        .prepare_query_execution_impl(&b, &execution)
        .await
        .unwrap();
    sqlx::query(&format!("DROP DATABASE {first} WITH (FORCE)"))
        .execute(&admin)
        .await
        .expect("drop anchor");
    let recovered = driver
        .control_pool_for(&b)
        .await
        .expect("recovery through maintenance database");
    assert!(old.is_closed(), "replacement waits for old pool close");
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&recovered)
        .await
        .unwrap();
    assert_eq!(database, "postgres");
    driver
        .read_fixed_session(&b)
        .await
        .expect("cross-database observation after recovery");
    driver
        .cancel_query_with_execution_impl(&b, &execution)
        .await
        .expect("prepared cancellation resolves replacement control pool");
    assert_eq!(
        ledger.charged.load(Ordering::SeqCst),
        1,
        "recovery retains the original permit"
    );
    let held = recovered.acquire().await.unwrap();
    let closer = driver.clone();
    let closing_handle = b.clone();
    let closing = tokio::spawn(async move { closer.disconnect_impl(closing_handle).await });
    tokio::time::timeout(Duration::from_secs(2), async {
        while !recovered.is_closed() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("last close reaches control pool");
    closing.abort();
    let _ = closing.await;
    assert_eq!(
        ledger.released.load(Ordering::SeqCst),
        0,
        "cancelled close retains permit"
    );
    assert_eq!(
        driver.control_pools.read().await.len(),
        1,
        "cancelled close retains registration"
    );
    drop(held);
    driver.disconnect_impl(b.clone()).await.unwrap();
    driver.disconnect_impl(b).await.unwrap();
    assert_eq!(
        ledger.released.load(Ordering::SeqCst),
        1,
        "last confirmed close releases exactly once"
    );
    assert!(recovered.is_closed());
    assert!(driver.control_pools.read().await.is_empty());
    sqlx::query(&format!("DROP DATABASE {second}"))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL: TEST_PG_PORT and TEST_PG_USER"]
async fn live_failed_control_connect_releases_charge_and_registration() {
    let mut missing = config();
    missing.database = Some("missing_control_database".into());
    // A denied maintenance database forces every candidate to fail without data sockets.
    missing.username = Some("missing_control_role".into());
    let driver = PostgresDriver::new();
    let ledger = Arc::new(Ledger::default());
    assert!(driver
        .acquire_control_pool("failed", &missing, &missing, Some(ledger.clone()))
        .await
        .is_err());
    assert_eq!(ledger.charged.load(Ordering::SeqCst), 1);
    assert_eq!(ledger.released.load(Ordering::SeqCst), 1);
    assert!(driver.control_pools.read().await.is_empty());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL; temporarily revokes PUBLIC maintenance CONNECT"]
async fn live_account_without_maintenance_access_uses_known_business_database() {
    let mut target = config();
    let admin = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(build_pg_options(&target).unwrap())
        .await
        .unwrap();
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let role = format!("ctl_user_{suffix}");
    let database = format!("ctl_db_{suffix}");
    sqlx::query(&format!("CREATE ROLE {role} LOGIN"))
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query(&format!("CREATE DATABASE {database} OWNER {role}"))
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query("REVOKE CONNECT ON DATABASE postgres FROM PUBLIC")
        .execute(&admin)
        .await
        .unwrap();
    target.username = Some(role.clone());
    target.database = Some(database.clone());
    let mut anchor = target.clone();
    anchor.database = Some("missing_profile_anchor".into());
    let ledger = Arc::new(Ledger::default());
    let driver = PostgresDriver::new();
    let result = driver
        .connect_with_control(&target, &anchor, Some(ledger.clone()))
        .await;
    // Restore global maintenance ACL before assertions, including failed opens.
    sqlx::query("GRANT CONNECT ON DATABASE postgres TO PUBLIC")
        .execute(&admin)
        .await
        .unwrap();
    let handle = result.expect("no extra maintenance CONNECT privilege required");
    let pool = driver.control_pool_for(&handle).await.unwrap();
    let actual: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(actual, database);
    assert_eq!(ledger.charged.load(Ordering::SeqCst), 1);
    driver.disconnect_impl(handle).await.unwrap();
    assert_eq!(ledger.released.load(Ordering::SeqCst), 1);
    sqlx::query(&format!("DROP DATABASE {database}"))
        .execute(&admin)
        .await
        .unwrap();
    sqlx::query(&format!("DROP ROLE {role}"))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL: TEST_PG_PORT and TEST_PG_USER"]
async fn live_abandoned_admission_retries_cleanup_before_releasing() {
    use datazen_driver_api::resource::{
        AcquireResourceRequest, Baseline, IdentityScope, ResourceProvider, ResourcePurpose,
        ResourceScope,
    };
    struct BlockingBudget {
        ledger: Ledger,
        entered: tokio::sync::Notify,
        proceed: tokio::sync::Notify,
        failed_control_release: std::sync::atomic::AtomicBool,
        failed_data_release: std::sync::atomic::AtomicBool,
    }
    #[async_trait]
    impl BudgetPort for BlockingBudget {
        async fn acquire_physical_connections(
            &self,
            count: u32,
        ) -> Result<BudgetPermit, datazen_driver_api::resource::ResourceError> {
            self.ledger.acquire_physical_connections(count).await
        }
        async fn acquire_control_connection(
            &self,
        ) -> Result<BudgetPermit, datazen_driver_api::resource::ResourceError> {
            self.entered.notify_one();
            self.proceed.notified().await;
            self.ledger.acquire_physical_connections(1).await
        }
        async fn release_control_connection(
            &self,
            permit: &BudgetPermit,
        ) -> Result<(), datazen_driver_api::resource::ResourceError> {
            if !self.failed_control_release.swap(true, Ordering::SeqCst) {
                return Err(datazen_driver_api::resource::ResourceError::Driver(
                    DriverError::SessionLost,
                ));
            }
            self.ledger.release_physical_connections(permit).await
        }
        async fn release_physical_connections(
            &self,
            permit: &BudgetPermit,
        ) -> Result<(), datazen_driver_api::resource::ResourceError> {
            if !self.failed_data_release.swap(true, Ordering::SeqCst) {
                return Err(datazen_driver_api::resource::ResourceError::Driver(
                    DriverError::SessionLost,
                ));
            }
            self.ledger.release_physical_connections(permit).await
        }
    }
    let driver = Arc::new(PostgresDriver::new());
    let provider = Arc::new(crate::PostgresResourceProvider::new(driver.clone()));
    let budget = Arc::new(BlockingBudget {
        ledger: Ledger::default(),
        entered: tokio::sync::Notify::new(),
        proceed: tokio::sync::Notify::new(),
        failed_control_release: std::sync::atomic::AtomicBool::new(false),
        failed_data_release: std::sync::atomic::AtomicBool::new(false),
    });
    let request = AcquireResourceRequest {
        connection_config: config(),
        target: datazen_driver_api::namespace::NamespaceTarget::empty(),
        scope: ResourceScope {
            purpose: ResourcePurpose::InteractiveQuery,
            max_physical_connections: 1,
            holds_open_transaction: false,
            pin_for_streaming: false,
        },
        identity_scope: IdentityScope::default(),
        baseline: Baseline::new(Vec::new()),
    };
    let task_budget: Arc<dyn BudgetPort> = budget.clone();
    let acquisition =
        tokio::spawn(async move { provider.acquire_resource(&request, &task_budget).await });
    tokio::time::timeout(Duration::from_secs(3), budget.entered.notified())
        .await
        .expect("admission reaches control budget");
    assert_eq!(budget.ledger.charged.load(Ordering::SeqCst), 1);
    acquisition.abort();
    let _ = acquisition.await;
    budget.proceed.notify_one();
    tokio::time::timeout(Duration::from_secs(3), async {
        while budget.ledger.released.load(Ordering::SeqCst) != 2 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("abandoned admission eventually confirms and releases both sockets");
    assert_eq!(budget.ledger.charged.load(Ordering::SeqCst), 2);
    assert!(budget.failed_control_release.load(Ordering::SeqCst));
    assert!(budget.failed_data_release.load(Ordering::SeqCst));
    assert!(driver.pools.read().await.is_empty());
    assert!(driver.control_pools.read().await.is_empty());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL: TEST_PG_PORT and TEST_PG_USER"]
async fn live_failed_initial_control_release_remains_recoverable() {
    struct FailFirstRelease {
        ledger: Ledger,
        failed: std::sync::atomic::AtomicBool,
    }
    #[async_trait]
    impl BudgetPort for FailFirstRelease {
        async fn acquire_physical_connections(
            &self,
            count: u32,
        ) -> Result<BudgetPermit, datazen_driver_api::resource::ResourceError> {
            self.ledger.acquire_physical_connections(count).await
        }
        async fn release_physical_connections(
            &self,
            permit: &BudgetPermit,
        ) -> Result<(), datazen_driver_api::resource::ResourceError> {
            if !self.failed.swap(true, Ordering::SeqCst) {
                return Err(datazen_driver_api::resource::ResourceError::Driver(
                    DriverError::SessionLost,
                ));
            }
            self.ledger.release_physical_connections(permit).await
        }
    }
    let mut missing = config();
    missing.username = Some("missing_control_role".into());
    let driver = PostgresDriver::new();
    let budget = Arc::new(FailFirstRelease {
        ledger: Ledger::default(),
        failed: std::sync::atomic::AtomicBool::new(false),
    });
    assert!(driver
        .acquire_control_pool("failed", &missing, &missing, Some(budget.clone()))
        .await
        .is_err());
    // Public connect failure returns no handle, so only a fresh acquisition can recover this identity.
    assert!(driver
        .acquire_control_pool("retry", &missing, &missing, Some(budget.clone()))
        .await
        .is_err());
    assert_eq!(
        budget.ledger.charged.load(Ordering::SeqCst),
        2,
        "fresh admission starts only after retained close succeeds"
    );
    assert_eq!(
        budget.ledger.released.load(Ordering::SeqCst),
        2,
        "old and freshly failed control attempts both release once"
    );
    assert!(driver.control_pools.read().await.is_empty());
}
