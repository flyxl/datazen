use super::Fixture;
use std::sync::{Arc, Mutex};

use datazen_driver_api::namespace::NamespaceTarget;
use datazen_driver_api::resource::{
    AcquireResourceRequest, Baseline, BudgetPermit, BudgetPort, IdentityScope, ResourceError,
    ResourceProvider, ResourcePurpose, ResourceScope,
};
use datazen_driver_api::session::{
    CloseDisposition, ObservationConfidence, TransactionOptions, TransactionState,
};
use datazen_driver_api::DatabaseDriver;
use datazen_driver_postgres::{PostgresDriver, PostgresResourceProvider};

#[derive(Default)]
struct Budget(Mutex<Vec<String>>);

#[async_trait::async_trait]
impl BudgetPort for Budget {
    async fn acquire_physical_connections(&self, n: u32) -> Result<BudgetPermit, ResourceError> {
        let permit_id = uuid::Uuid::new_v4().to_string();
        self.0.lock().unwrap().push(permit_id.clone());
        Ok(BudgetPermit {
            permit_id,
            physical_connections: n,
        })
    }

    async fn release_physical_connections(
        &self,
        permit: &BudgetPermit,
    ) -> Result<(), ResourceError> {
        let mut active = self.0.lock().unwrap();
        let index = active
            .iter()
            .position(|id| id == &permit.permit_id)
            .expect("only release an actual permit");
        active.swap_remove(index);
        Ok(())
    }
}

#[tokio::test]
async fn leased_session_and_pinned_transaction_report_only_live_postgres_facts() {
    let mut fixture = Fixture::open().await;
    let name = format!("dz_obs_{}", uuid::Uuid::new_v4().simple());
    let table = format!("{name}.live_rows");
    sqlx::raw_sql(&format!("CREATE SCHEMA {name};CREATE TABLE {table}(id integer PRIMARY KEY);INSERT INTO {table} VALUES(1);"))
        .execute(&mut fixture.oracle).await.expect("independent schema and data");
    let who: (String, String) =
        sqlx::query_as("SELECT current_database()::text, current_user::text")
            .fetch_one(&mut fixture.oracle)
            .await
            .expect("independent session identity");
    let driver = Arc::new(PostgresDriver::new());
    let provider = PostgresResourceProvider::new(driver.clone());
    let ledger = Arc::new(Budget::default());
    let budget: Arc<dyn BudgetPort> = ledger.clone();
    let request = AcquireResourceRequest {
        connection_config: fixture.config.clone(),
        target: NamespaceTarget::empty().with_database(who.0.clone()),
        scope: ResourceScope {
            purpose: ResourcePurpose::MetadataInspection,
            max_physical_connections: 16,
            holds_open_transaction: false,
            pin_for_streaming: false,
        },
        identity_scope: IdentityScope::default(),
        baseline: Baseline::default(),
    };
    let handle = provider
        .acquire_resource(&request, &budget)
        .await
        .expect("leased real resource");
    let leased = provider
        .observe_session(&handle)
        .await
        .expect("actual leased session");
    assert_eq!(
        leased.context.namespace.database.as_deref(),
        Some(who.0.as_str())
    );
    assert_eq!(
        leased.context.effective_identity.as_deref(),
        Some(who.1.as_str())
    );
    assert_eq!(leased.context.confidence, ObservationConfidence::Partial);
    assert!(leased.context.namespace.schema.is_none() && leased.context.search_path.is_empty());

    provider
        .begin_transaction(
            &handle,
            &TransactionOptions {
                isolation_level: None,
                read_only: None,
                defers_commit: false,
            },
        )
        .await
        .expect("link a real pinned transaction");
    let session = provider
        .connection_handle(&handle)
        .expect("same underlying driver handle");
    driver
        .execute(
            &session,
            &format!("SET LOCAL search_path TO \"{name}\", public"),
        )
        .await
        .expect("set real transaction search path");
    assert_eq!(
        driver
            .execute(&session, "INSERT INTO live_rows VALUES(2)")
            .await
            .expect("live unqualified transaction write"),
        1
    );
    let pinned = provider
        .observe_session(&handle)
        .await
        .expect("actual pinned session");
    assert_eq!(
        pinned.context.namespace.database.as_deref(),
        Some(who.0.as_str())
    );
    assert_eq!(
        pinned.context.effective_identity.as_deref(),
        Some(who.1.as_str())
    );
    assert_eq!(
        pinned.context.namespace.schema.as_deref(),
        Some(name.as_str())
    );
    assert_eq!(
        pinned.context.search_path,
        [name.clone(), "public".to_owned()]
    );
    assert_eq!(pinned.context.confidence, ObservationConfidence::Confirmed);
    assert_eq!(pinned.context.transaction_state, TransactionState::Active);
    assert_eq!(pinned.context.autocommit, Some(false));
    assert_eq!(pinned.handles.len(), 1);
    let sql = format!("SELECT id FROM {table} ORDER BY id");
    let independently_visible = async |oracle: &mut sqlx::PgConnection| {
        sqlx::query_scalar::<_, i32>(&sql)
            .fetch_all(oracle)
            .await
            .expect("independent rows")
    };
    assert_eq!(
        independently_visible(&mut fixture.oracle).await,
        [1],
        "uncommitted row must not escape"
    );
    provider
        .close_handle(&handle, &pinned.handles[0])
        .await
        .expect("real rollback on handle close");
    assert_eq!(
        independently_visible(&mut fixture.oracle).await,
        [1],
        "closed transaction must roll back"
    );
    assert!(provider.commit_transaction(&handle).await.is_err());
    let after = provider
        .observe_session(&handle)
        .await
        .expect("still usable leased pool");
    assert_eq!(after.context.confidence, ObservationConfidence::Partial);
    assert!(after.handles.is_empty());
    assert_eq!(
        provider
            .close_resource(&handle)
            .await
            .expect("release real pool"),
        CloseDisposition::Closed
    );
    assert!(ledger.0.lock().unwrap().is_empty());
    sqlx::query(&format!("DROP SCHEMA {name} CASCADE"))
        .execute(&mut fixture.oracle)
        .await
        .expect("remove observation fixture");
}
