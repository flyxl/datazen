use super::*;
use crate::testing::{app_state::TestAppState, mock_driver::MockDriverOptions};
use datazen_feature_core::dashboard::store::save_dashboard;
use datazen_feature_core::dashboard::types::*;
use datazen_platform_api::dto::job::{JobFilter, JobState};
use std::time::Duration;

// The shared Host mock defaults to the legacy test type "postgres"; this
// production authorization journey uses the actual profile type "postgresql".
struct ProfileFactory {
    driver: Arc<dyn datazen_driver_api::DatabaseDriver>,
    provider: Arc<dyn datazen_driver_api::resource::ResourceProvider>,
}
impl datazen_driver_api::DatabaseDriverFactory for ProfileFactory {
    fn create(&self) -> Arc<dyn datazen_driver_api::DatabaseDriver> {
        self.driver.clone()
    }
    fn driver_id(&self) -> &'static str {
        "postgresql"
    }
    fn resource_provider(&self) -> Option<Arc<dyn datazen_driver_api::resource::ResourceProvider>> {
        Some(self.provider.clone())
    }
}

#[tokio::test]
async fn production_timer_admits_actual_grant_revision_and_dispatches_runtime_once() {
    let test = TestAppState::with_resource_provider(MockDriverOptions {
        columns: vec![datazen_driver_api::ColumnSchema {
            name: "marker".into(),
            data_type: "INTEGER".into(),
            nullable: false,
            default_value: None,
            comment: None,
            is_primary_key: false,
            is_auto_increment: false,
        }],
        query_rows: vec![vec![Some(datazen_driver_api::Value::Integer(73129))]],
        ..Default::default()
    })
    .await;
    let provider = test
        .registry
        .resource_provider(&"postgres".to_owned())
        .await
        .unwrap();
    let driver: Arc<dyn datazen_driver_api::DatabaseDriver> = test.mock.clone();
    test.registry
        .register_test_driver("postgresql", driver.clone())
        .await;
    test.registry
        .register_test_factory(
            "postgresql",
            Box::leak(Box::new(ProfileFactory { driver, provider })),
        )
        .await;
    let mut profile = test.save_connection("timer-profile").await;
    profile.database_type = "postgresql".into();
    test.store.save_connection(profile).await.unwrap();
    let workflow: datazen_feature_core::workflow::model::WorkflowDefinition = serde_json::from_value(serde_json::json!({
        "id":"timer-workflow", "name":"timer", "description":"real grant revision",
        "connection":"timer-profile", "database":"app", "variables":[],
        "steps":[{"id":"read", "type":"command", "command":"query", "connection":"timer-profile",
            "input":{"database":"app", "sql":"SELECT 73129"}}]
    }))
    .unwrap();
    test.state
        .workflow_registry
        .save_workflow(&workflow)
        .await
        .unwrap();
    let now = chrono::Utc::now().to_rfc3339();
    let widget = DashboardWidget {
        id: "timer-widget".into(),
        title: "timer".into(),
        workflow_id: workflow.id.clone(),
        view_mode: ViewMode::Table,
        chart_config: None,
        layout: WidgetLayout {
            x: 0,
            y: 0,
            w: 4,
            h: 3,
        },
        refresh: RefreshPolicy {
            mode: RefreshMode::Interval,
            refresh_sec: Some(30),
        },
        alert: None,
        enabled: true,
    };
    save_dashboard(
        &*test.store.app_db(),
        Dashboard {
            id: "timer-dashboard".into(),
            name: "timer".into(),
            created_at: now.clone(),
            updated_at: now,
            layout: DashboardLayout {
                cols: 12,
                row_height: 80,
            },
            widgets: vec![widget.clone()],
            enabled: true,
            refresh_paused: false,
        },
    )
    .unwrap();
    let grant = crate::monitor::grants::authorize(&test.state, "timer-dashboard", &widget.id, 180)
        .await
        .unwrap();
    assert_eq!(
        grant.workflow_revision.len(),
        64,
        "production grant uses untagged SHA256"
    );
    // Avoid crossing a wall-clock occurrence slot while observing the timer.
    while chrono::Utc::now().timestamp().rem_euclid(30) > 25 {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let engine = &test.state.monitor_engine;
    // Same core startup helper used by production desktop setup. First receipt
    // must come from its real one-second timer; never call a manual tick here.
    start_core_on_desktop_runtime(&engine.core);
    let receipt = tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            let jobs = test
                .state
                .desktop_job_host
                .list(
                    &grant.context(),
                    JobFilter {
                        limit: Some(10),
                        ..Default::default()
                    },
                )
                .await
                .unwrap();
            if let Some(job) = jobs
                .into_iter()
                .find(|job| job.view.state == JobState::Succeeded)
            {
                break job;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("production timer produced a durable successful occurrence");
    assert_eq!(receipt.view.kind, "monitorOccurrence");
    assert_eq!(
        receipt.definition.payload["monitorOccurrence"]["workflowRevision"],
        grant.workflow_revision
    );
    let details = test
        .state
        .desktop_job_host
        .details(&grant.context(), receipt.view.job_id.clone())
        .await
        .unwrap();
    let run_id = &details.domain_results[0].items[0].item_id;
    let run = datazen_feature_core::dashboard::runs::get_run(
        &*test.store.app_db(),
        "timer-dashboard",
        &widget.id,
        run_id,
    )
    .unwrap();
    assert_eq!(run.status, WidgetRunStatus::Ok);
    assert_eq!(run.rows[0][0], 73129);
    let physical_queries = test.mock.query_calls();
    assert!(physical_queries > 0);
    // Duplicate-slot evidence is controlled only after the real timer receipt.
    if receipt.definition.payload["monitorOccurrence"]["scheduledAt"]
        .as_i64()
        .unwrap()
        / 30
        == chrono::Utc::now().timestamp() / 30
    {
        assert!(engine
            .core
            .tick_scheduled_widget("timer-dashboard", &widget)
            .await
            .is_err());
        assert_eq!(test.mock.query_calls(), physical_queries);
    } else {
        panic!("timer result crossed an occurrence slot boundary");
    }
    crate::monitor::grants::revoke(&test.state, "timer-dashboard", &widget.id).unwrap();
    assert!(engine
        .core
        .tick_scheduled_widget("timer-dashboard", &widget)
        .await
        .is_err());
    assert_eq!(test.mock.query_calls(), physical_queries);
    assert_eq!(
        test.state
            .desktop_job_host
            .list(&grant.context(), JobFilter::default())
            .await
            .unwrap()
            .len(),
        1
    );
}
