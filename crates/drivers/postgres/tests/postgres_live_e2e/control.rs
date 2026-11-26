use super::Fixture;
use datazen_driver_api::{
    async_trait,
    namespace::NamespaceTarget,
    resource::{
        AcquireResourceRequest, Baseline, BudgetPermit, BudgetPort, IdentityScope, ResourceError,
        ResourceProvider, ResourcePurpose, ResourceScope,
    },
    session::CloseDisposition,
    DatabaseDriver, DriverError, QueryExecutionId,
};
use datazen_driver_postgres::{PostgresDriver, PostgresResourceProvider};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Default)]
struct Ledger {
    data: Mutex<Vec<String>>,
    control: Mutex<Vec<String>>,
}

fn charge(permits: &Mutex<Vec<String>>, n: u32) -> BudgetPermit {
    let permit_id = uuid::Uuid::new_v4().to_string();
    permits
        .lock()
        .expect("budget instance")
        .push(permit_id.clone());
    BudgetPermit {
        permit_id,
        physical_connections: n,
    }
}

fn release(permits: &Mutex<Vec<String>>, permit: &BudgetPermit) {
    let mut guard = permits.lock().expect("budget instance");
    let index = guard
        .iter()
        .position(|id| id == &permit.permit_id)
        .expect("permit released only once");
    guard.swap_remove(index);
}

#[async_trait]
impl BudgetPort for Ledger {
    async fn acquire_physical_connections(&self, n: u32) -> Result<BudgetPermit, ResourceError> {
        Ok(charge(&self.data, n))
    }
    async fn release_physical_connections(
        &self,
        permit: &BudgetPermit,
    ) -> Result<(), ResourceError> {
        release(&self.data, permit);
        Ok(())
    }
    async fn acquire_control_connection(&self) -> Result<BudgetPermit, ResourceError> {
        Ok(charge(&self.control, 1))
    }
    async fn release_control_connection(&self, permit: &BudgetPermit) -> Result<(), ResourceError> {
        release(&self.control, permit);
        Ok(())
    }
}

#[tokio::test]
async fn control_channel_recovers_after_its_actual_anchor_database_is_removed() {
    let mut fixture = Fixture::open().await;
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let anchor = format!("dz_fixture_anchor_{suffix}");
    let data = format!("dz_fixture_business_{suffix}");
    for database in [&anchor, &data] {
        sqlx::query(&format!("CREATE DATABASE {database}"))
            .execute(&mut fixture.oracle)
            .await
            .expect("isolated real database");
    }
    let live_pids = async |oracle: &mut sqlx::PgConnection, name: &str| -> Vec<i32> {
        sqlx::query_scalar("SELECT pid FROM pg_stat_activity WHERE datname=$1 ORDER BY pid")
            .bind(name)
            .fetch_all(oracle)
            .await
            .expect("independent physical backend list")
    };
    let original_maintenance = live_pids(&mut fixture.oracle, "postgres").await;
    let mut profile = fixture.config.clone();
    profile.database = Some(anchor.clone());
    let driver = Arc::new(PostgresDriver::new());
    let provider = PostgresResourceProvider::new(driver.clone());
    let ledger = Arc::new(Ledger::default());
    let budget: Arc<dyn BudgetPort> = ledger.clone();
    let request = |name: &str| AcquireResourceRequest {
        connection_config: profile.clone(),
        target: NamespaceTarget::empty().with_database(name.to_owned()),
        scope: ResourceScope {
            purpose: ResourcePurpose::InteractiveQuery,
            max_physical_connections: 16,
            holds_open_transaction: false,
            pin_for_streaming: false,
        },
        identity_scope: IdentityScope::default(),
        baseline: Baseline::default(),
    };
    let owning = provider
        .acquire_resource(&request(&anchor), &budget)
        .await
        .expect("data + anchored control");
    let surviving = provider
        .acquire_resource(&request(&data), &budget)
        .await
        .expect("different live database shares registered control");
    let active_charge = ledger.control.lock().expect("control budget").clone();
    assert_eq!(active_charge.len(), 1);
    assert_eq!(ledger.data.lock().expect("user budgets").len(), 2);
    let anchor_pids = live_pids(&mut fixture.oracle, &anchor).await;
    let data_pids = live_pids(&mut fixture.oracle, &data).await;
    assert_eq!(anchor_pids.len(), 2);
    assert_eq!(data_pids.len(), 1);
    assert_eq!(
        provider
            .close_resource(&owning)
            .await
            .expect("close only first data owner"),
        CloseDisposition::Closed
    );
    assert_eq!(live_pids(&mut fixture.oracle, &anchor).await.len(), 1);
    assert_eq!(
        *ledger.control.lock().expect("controller still shared"),
        active_charge
    );
    sqlx::query(&format!("DROP DATABASE {anchor} WITH (FORCE)"))
        .execute(&mut fixture.oracle)
        .await
        .expect("close actual anchored control backend");

    let connection = provider
        .connection_handle(&surviving)
        .expect("unchanged data handle");
    let recovered = provider
        .observe_session(&surviving)
        .await
        .expect("same business session survives and control recovers");
    assert_eq!(
        recovered.context.namespace.database.as_deref(),
        Some(data.as_str())
    );
    let new_maintenance = live_pids(&mut fixture.oracle, "postgres").await;
    assert_eq!(
        new_maintenance
            .iter()
            .filter(|pid| !original_maintenance.contains(pid))
            .count(),
        1
    );
    assert_eq!(live_pids(&mut fixture.oracle, &data).await, data_pids);
    assert_eq!(
        *ledger
            .control
            .lock()
            .expect("no extra charge after recovery"),
        active_charge
    );

    let id = QueryExecutionId::new("cancel-after-losing-control-db");
    driver
        .prepare_query_execution(&connection, &id)
        .await
        .expect("real registered query on surviving data");
    let worker = {
        let driver = Arc::clone(&driver);
        let connection = connection.clone();
        let id = id.clone();
        tokio::spawn(async move {
            driver
                .query_stream_with_execution(
                    &connection,
                    &id,
                    "SELECT 1 FROM pg_sleep(30)",
                    None,
                    Arc::new(|_| {}),
                )
                .await
        })
    };
    tokio::time::timeout(Duration::from_secs(4),async{
        loop {
            let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE pid=$1 AND wait_event='PgSleep')")
                .bind(data_pids[0]).fetch_one(&mut fixture.oracle).await.expect("independent exact data PID");
            if waiting{break}
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }).await.expect("read starts on original data backend");
    driver
        .cancel_query_with_execution(&connection, &id)
        .await
        .expect("actual cancellation via replacement pool");
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(4), worker)
            .await
            .expect("driver promptly cancels")
            .expect("worker task"),
        Err(DriverError::QueryCancelled)
    ));

    assert_eq!(
        provider
            .close_resource(&surviving)
            .await
            .expect("last data connection closes shared control"),
        CloseDisposition::Closed
    );
    assert!(ledger
        .control
        .lock()
        .expect("control budget released")
        .is_empty());
    assert!(ledger
        .data
        .lock()
        .expect("both data budgets released")
        .is_empty());
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if live_pids(&mut fixture.oracle, "postgres").await == original_maintenance {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("server confirms maintenance backend physically gone");
    sqlx::query(&format!("DROP DATABASE {data}"))
        .execute(&mut fixture.oracle)
        .await
        .expect("no leaked data sessions");
}
