//! Real production Workflow Handler -> adapter -> retained resource journey.
use super::*;

struct PgFactory {
    driver: Arc<dyn DatabaseDriver>,
    provider: Arc<dyn datazen_driver_api::resource::ResourceProvider>,
}
impl datazen_driver_api::DatabaseDriverFactory for PgFactory {
    fn driver_id(&self) -> &'static str {
        "postgresql"
    }
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        self.driver.clone()
    }
    fn resource_provider(&self) -> Option<Arc<dyn datazen_driver_api::resource::ResourceProvider>> {
        Some(self.provider.clone())
    }
}
async fn state() -> crate::testing::app_state::TestAppState {
    let test =
        crate::testing::app_state::TestAppState::with_resource_provider(Default::default()).await;
    let driver: Arc<dyn DatabaseDriver> = test.mock.clone();
    let provider =
        Arc::new(crate::testing::resource_provider::TestResourceProvider::new(driver.clone()));
    test.registry
        .register_test_driver("postgresql", driver.clone())
        .await;
    test.registry
        .register_test_factory(
            "postgresql",
            Box::leak(Box::new(PgFactory { driver, provider })),
        )
        .await;
    let mut config = crate::testing::app_state::sample_postgres_config("wf-owner");
    config.database_type = "postgresql".into();
    test.store.save_connection(config).await.unwrap();
    test
}

#[tokio::test]
async fn production_handler_binds_job_and_block_owners_from_durable_claim_without_renewing() {
    let test = state().await;
    let definition: WorkflowDefinition = serde_yaml::from_str("id: wf-owner\nname: Owner journey\ndescription: ''\nversion: '2'\nconnection: wf-owner\ndatabase: app\nsteps:\n  - type: query\n    id: independent\n    sql: SELECT 1\nblocks:\n  - type: sessionBlock\n    id: retained\n    steps:\n      - type: query\n        id: one\n        sql: SELECT 2\n      - type: query\n        id: two\n        sql: SELECT 3\n").unwrap();
    let result = execute(&test.state, &definition, &serde_json::json!({}))
        .await
        .unwrap();
    assert!(result.success, "{:?}", result.error);
    for id in ["independent", "one", "two"] {
        assert!(
            result
                .steps
                .iter()
                .any(|step| step.step_id == id && step.status == StepStatus::Success),
            "the real production handler must execute each declared step"
        );
    }
    assert!(test.mock.query_calls() >= 3);
    let ctx = test
        .state
        .platform_require()
        .unwrap()
        .begin_call()
        .context()
        .clone();
    let jobs = test
        .state
        .desktop_job_host
        .list(&ctx, Default::default())
        .await
        .unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(
        jobs[0].view.state,
        datazen_platform_api::dto::job::JobState::Succeeded
    );
    assert_eq!(
        jobs[0].definition.payload["workflowOwnerBlocks"],
        serde_json::json!(["retained"])
    );
    assert_eq!(jobs[0].stages.len(), 1);
}

#[tokio::test]
async fn production_host_without_runtime_permit_refuses_before_resource_binding() {
    let test = state().await;
    let ctx = test
        .state
        .platform_require()
        .unwrap()
        .begin_call()
        .context()
        .clone();
    let host = Host {
        state: &test.state,
        proofs: Arc::new(Default::default()),
        evidence: Arc::new(std::sync::Mutex::new(Default::default())),
        context: ctx,
        revisions: Default::default(),
        permit: None,
        cancel: None,
        deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(30),
    };
    let target = datazen_feature_core::workflow::target_plan::ExecutionTarget {
        connection_id: "wf-owner".into(),
        database: Some("app".into()),
        source: datazen_feature_core::workflow::target_plan::TargetSource::Workflow,
    };
    assert!(host
        .open_session_block("forged", &target, host.deadline)
        .await
        .is_err());
    assert_eq!(
        test.mock.query_calls(),
        0,
        "missing permit must refuse before any SQL"
    );
}
