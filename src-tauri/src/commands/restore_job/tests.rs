//! Host boundary journey: production upload, SQLite admission and desktop runtime wiring.
//! Provider behavior is synthetic; PG/MySQL driver journeys own real SQL evidence.
#[path = "provider_fixture.rs"]
mod provider_fixture;
use super::*;
use datazen_driver_api::{
    mock_driver::{MockDriver, MockDriverOptions},
    resource::ResourceProvider,
    DatabaseDriver, DatabaseDriverFactory,
};
use datazen_feature_core::restore_job::{RestoreGateway, RestorePlan};
use datazen_platform_api::{
    dto::{artifact::ArtifactLifecycle, execution::EffectOutcome},
    id::*,
    ports::{artifact::ArtifactStore, job::JobRepository},
    RequestContext,
};
use datazen_runtime::{
    budget::{BudgetConfig, BudgetLedger},
    job::{DesktopJobHost, SharedClock},
};
use sha2::{Digest, Sha256};
use std::sync::Mutex;

struct Factory {
    provider: Arc<provider_fixture::CountingProvider>,
    driver: Arc<MockDriver>,
}
impl DatabaseDriverFactory for Factory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        self.driver.clone()
    }
    fn driver_id(&self) -> &'static str {
        "restore-host-fixture"
    }
    fn resource_provider(&self) -> Option<Arc<dyn ResourceProvider>> {
        Some(self.provider.clone())
    }
}

async fn host() -> (
    tempfile::TempDir,
    Arc<crate::store::Store>,
    Arc<crate::platform::PlatformAdapter>,
    &'static Factory,
) {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(
        crate::store::Store::init_with_path(dir.path())
            .await
            .unwrap(),
    );
    let mut settings = store.get_settings().await;
    settings.safe_mode = false;
    store.save_settings(settings).await.unwrap();
    let config = serde_json::from_value(serde_json::json!({
        "id":"restore-profile", "name":"restore-host-fixture", "databaseType":"restore-host-fixture",
        "database":"app", "connectionTimeout":5, "maxPoolSize":1, "readOnly":false, "pinned":false
    })).unwrap();
    store.save_connection(config).await.unwrap();
    let provider = provider_fixture::CountingProvider::new("restore-host-fixture");
    let factory = Box::leak(Box::new(Factory {
        provider,
        driver: MockDriver::new("restore-host-fixture", MockDriverOptions::default()),
    }));
    let registry = Arc::new(datazen_runtime::driver_registry::DriverRegistry::new());
    registry
        .register_test_driver("restore-host-fixture", factory.driver.clone())
        .await;
    registry
        .register_test_factory("restore-host-fixture", factory)
        .await;
    let principal = crate::platform::identity::local_principal().unwrap();
    let identity = crate::platform::identity::DesktopIdentity::new_launch(
        crate::platform::identity::LOCAL_ORGANIZATION_ID,
        principal.as_str(),
        "restore-host-test",
    )
    .unwrap();
    let adapter = Arc::new(crate::platform::PlatformAdapter::with_identity(identity));
    adapter.inject(store.clone(), registry).unwrap();
    (dir, store, adapter, factory)
}
fn target() -> ExecutionTarget {
    ExecutionTarget::new(
        ConnectionId::new("restore-profile"),
        NamespaceTarget::database("app"),
    )
}
fn definition(ctx: &RequestContext, plan: &RestorePlan, id: &str) -> JobDefinition {
    JobDefinition {
        job_id: JobId::new(id),
        kind: RESTORE_APPLY_KIND.into(),
        owner: OwnerRef::ClientSession {
            client_instance_id: ctx.client_instance_id.clone(),
            purpose: RESTORE_APPLY_KIND.into(),
        },
        payload: plan.payload(),
        created_at: Timestamp::new("2026-10-19T00:00:00Z"),
    }
}

#[tokio::test]
async fn desktop_restore_uploaded_input_reaches_durable_job_and_desktop_session_backend() {
    let (dir, store, adapter, factory) = host().await;
    let ctx = adapter.begin_call().context().clone();
    let artifacts = adapter.artifact_store().unwrap();
    let path = dir.path().join("native-selected.sql");
    let sql = "CREATE TABLE probe(id int); INSERT INTO probe VALUES(7);";
    tokio::fs::write(&path, sql).await.unwrap();
    let plan = input::upload_native_input(&ctx, artifacts.clone(), target(), 1, &path, &[])
        .await
        .unwrap();
    // No durable plan points back to the mutable native source.
    tokio::fs::write(&path, "synthetic replacement")
        .await
        .unwrap();
    assert_eq!(plan.input_bytes.get(), sql.len() as u64);
    assert_eq!(
        plan.input_sha256,
        format!("{:x}", Sha256::digest(sql.as_bytes()))
    );
    let metadata = artifacts
        .describe(&ctx, plan.artifact_id.clone())
        .await
        .unwrap();
    assert_eq!(metadata.lifecycle, ArtifactLifecycle::Complete);
    assert_eq!(
        metadata.target_fingerprint,
        RestorePlan::target_fingerprint(&target())
    );
    let mut outsider = ctx.clone();
    outsider.principal_id = PrincipalId::new("other-owner");
    assert!(artifacts
        .describe(&outsider, plan.artifact_id.clone())
        .await
        .is_err());
    assert!(artifacts
        .read_chunk(&outsider, plan.artifact_id.clone(), Counter::new(0))
        .await
        .is_err());
    outsider = ctx.clone();
    outsider.organization_id = OrganizationId::new("other-organization");
    assert!(artifacts
        .describe(&outsider, plan.artifact_id.clone())
        .await
        .is_err());
    let clock = Arc::new(SharedClock::at("2026-10-19T00:00:00Z"));
    let repo = Arc::new(crate::store::app_db::jobs::SqliteJobRepository::new(
        crate::store::app_db::AppDb::open(dir.path()).unwrap(),
        clock.clone(),
        300,
    ));
    let host = DesktopJobHost::new(
        repo.clone(),
        Arc::new(Mutex::new(BudgetLedger::new(BudgetConfig::team_default()))),
        clock.clone(),
    );
    let id = JobId::new("desktop-restore-job");
    host.accept(
        &ctx,
        definition(&ctx, &plan, id.as_str()),
        &IdempotencyKey::new("accept"),
    )
    .await
    .unwrap();
    assert!(host
        .accept(
            &ctx,
            definition(&ctx, &plan, "restore-replay"),
            &IdempotencyKey::new("replay")
        )
        .await
        .is_err());
    let gateway = Arc::new(super::super::restore_gateway::DesktopRestoreGateway {
        adapter: adapter.clone(),
        store: store.clone(),
        driver: factory.driver.clone(),
        profile: datazen_driver_api::restore_sql::IMPLICIT_COMMIT.into(),
    });
    assert!(gateway.validate(&ctx, &plan).await.is_ok());
    let endpoints = [EndpointRef {
        connection_id: target().connection_id,
        service_key: "host-fixture".into(),
        objects: vec!["*".into()],
        role: EndpointRole::TargetWriter,
    }];
    host.ensure_endpoint_services(&endpoints).unwrap();
    host.dispatch(
        &ctx,
        &id,
        &WorkerId::new("desktop-restore-worker"),
        &endpoints,
        Arc::new(RestoreApplyJob::new(
            ctx.clone(),
            plan.clone(),
            artifacts,
            gateway,
        )),
    )
    .await
    .unwrap();
    let details = host.details(&ctx, id.clone()).await.unwrap();
    assert_eq!(details.job.state, JobState::Succeeded);
    assert_eq!(details.job.effect_outcome, Some(EffectOutcome::Completed));
    assert_eq!(details.commit_boundaries.len(), 2);
    assert_eq!(factory.provider.acquires(), 1);
    assert_eq!(factory.provider.closes(), 1);
    let calls = factory.provider.calls.lock().unwrap();
    assert_eq!(
        calls
            .iter()
            .map(|(_, sql)| sql.as_str())
            .collect::<Vec<_>>(),
        vec![
            "SELECT 1",
            "CREATE TABLE probe(id int)",
            "INSERT INTO probe VALUES(7)"
        ]
    );
    for boundary in &details.commit_boundaries {
        assert!(calls
            .iter()
            .any(|(id, _)| boundary.evidence.contains(&format!("execution:{id}"))));
    }
    drop(calls);
    // Reopen the actual desktop repository after completion: evidence and
    // consumption survive a process-shaped storage lifetime, without replay.
    drop(host);
    drop(repo);
    let reopened = crate::store::app_db::jobs::SqliteJobRepository::new(
        crate::store::app_db::AppDb::open(dir.path()).unwrap(),
        clock,
        300,
    );
    assert_eq!(
        reopened.get(&ctx, id.clone()).await.unwrap().view.state,
        JobState::Succeeded
    );
    let persisted = datazen_runtime::job::JobRuntimeRepository::get_details(&reopened, &ctx, id)
        .await
        .unwrap();
    assert_eq!(persisted.commit_boundaries, details.commit_boundaries);
    assert_eq!(persisted.job.execution_ids, details.job.execution_ids);
    assert!(reopened
        .accept(
            &ctx,
            definition(&ctx, &plan, "after-restart-replay"),
            &IdempotencyKey::new("after-restart")
        )
        .await
        .is_err());
    adapter.stop_idle_reclaimer();
}

#[tokio::test]
async fn desktop_restore_empty_input_and_revoked_profile_fail_before_database_execution() {
    let (dir, store, adapter, factory) = host().await;
    let ctx = adapter.begin_call().context().clone();
    let artifacts = adapter.artifact_store().unwrap();
    let path = dir.path().join("empty.sql");
    tokio::fs::write(&path, "").await.unwrap();
    assert!(
        input::upload_native_input(&ctx, artifacts.clone(), target(), 1, &path, &[])
            .await
            .is_err()
    );
    tokio::fs::write(&path, "CREATE TABLE probe(id int);")
        .await
        .unwrap();
    let plan = input::upload_native_input(&ctx, artifacts, target(), 1, &path, &[])
        .await
        .unwrap();
    let gateway = super::super::restore_gateway::DesktopRestoreGateway {
        adapter: adapter.clone(),
        store: store.clone(),
        driver: factory.driver.clone(),
        profile: datazen_driver_api::restore_sql::IMPLICIT_COMMIT.into(),
    };
    assert!(gateway.validate(&ctx, &plan).await.is_ok());
    let mut config = store.platform_profile("restore-profile").await.unwrap().0;
    config.read_only = true;
    store.save_connection(config).await.unwrap();
    assert!(gateway.validate(&ctx, &plan).await.is_err());
    let mut current_plan = plan.clone();
    current_plan.config_revision = Counter::new(
        store
            .platform_profile("restore-profile")
            .await
            .unwrap()
            .1
            .config_revision,
    );
    // Current revision still cannot authorize a read-only target.
    assert!(gateway.validate(&ctx, &current_plan).await.is_err());
    let mut config = store.platform_profile("restore-profile").await.unwrap().0;
    config.read_only = false;
    store.save_connection(config).await.unwrap();
    current_plan.config_revision = Counter::new(
        store
            .platform_profile("restore-profile")
            .await
            .unwrap()
            .1
            .config_revision,
    );
    assert!(gateway.validate(&ctx, &current_plan).await.is_ok());
    let mut settings = store.get_settings().await;
    settings.safe_mode = true;
    store.save_settings(settings).await.unwrap();
    assert!(gateway.validate(&ctx, &current_plan).await.is_err());
    let clock = Arc::new(SharedClock::at("2026-10-19T00:00:00Z"));
    let repo = Arc::new(crate::store::app_db::jobs::SqliteJobRepository::new(
        crate::store::app_db::AppDb::open(dir.path()).unwrap(),
        clock.clone(),
        300,
    ));
    let host = DesktopJobHost::new(
        repo,
        Arc::new(Mutex::new(BudgetLedger::new(BudgetConfig::team_default()))),
        clock,
    );
    let job_id = JobId::new("restore-policy-revoked");
    host.accept(
        &ctx,
        definition(&ctx, &current_plan, job_id.as_str()),
        &IdempotencyKey::new("policy-revoked"),
    )
    .await
    .unwrap();
    let endpoints = [EndpointRef {
        connection_id: target().connection_id,
        service_key: "host-fixture".into(),
        objects: vec!["*".into()],
        role: EndpointRole::TargetWriter,
    }];
    host.ensure_endpoint_services(&endpoints).unwrap();
    host.dispatch(
        &ctx,
        &job_id,
        &WorkerId::new("desktop-restore-worker"),
        &endpoints,
        Arc::new(RestoreApplyJob::new(
            ctx.clone(),
            current_plan,
            adapter.artifact_store().unwrap(),
            Arc::new(gateway),
        )),
    )
    .await
    .unwrap();
    let details = host.details(&ctx, job_id).await.unwrap();
    assert_eq!(details.job.state, JobState::Failed);
    assert_eq!(details.job.effect_outcome, Some(EffectOutcome::NotStarted));
    assert!(details.job.execution_ids.is_empty());
    assert!(details.commit_boundaries.is_empty());
    assert_eq!(factory.provider.acquires(), 0);
    assert!(factory.provider.calls.lock().unwrap().is_empty());
    adapter.stop_idle_reclaimer();
}
