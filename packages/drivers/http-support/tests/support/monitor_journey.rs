//! Real-driver slice of Monitor's scheduled read-only query execution.
//! No desktop scheduler, occurrence admission or duplicate-tick receipt is simulated.
#[path = "monitor_runtime.rs"]
mod runtime;
use datazen_driver_api::{resource::ResourceProvider, ConnectionConfig, DatabaseDriver};
use datazen_feature_core::{
    dashboard::schedule_grant::{MonitorGrant, MonitorScope},
    workflow::{resource_execution, WorkflowCommandStep},
};
use datazen_platform_api::{
    dto::execution::{EffectOutcome, ExecutionState},
    id::ConnectionId,
    target::{ExecutionTarget, NamespaceTarget},
};
use futures_util::FutureExt;
use std::sync::{atomic::Ordering, Arc};

fn integer(value: &serde_json::Value) -> Option<i64> {
    value.as_i64().or_else(|| value.as_str()?.parse().ok())
}

pub async fn run(
    mut config: ConnectionConfig,
    driver: Arc<dyn DatabaseDriver>,
    provider: Arc<dyn ResourceProvider>,
) {
    config.max_pool_size = 1;
    let observer = driver
        .connect(&config)
        .await
        .unwrap_or_else(|_| panic!("monitor fixture unavailable"));
    let table = format!("monitor_{}", uuid::Uuid::new_v4().simple());
    assert!(
        driver
            .execute(
                &observer,
                &format!("CREATE TABLE {table}(id INTEGER PRIMARY KEY)")
            )
            .await
            .is_ok(),
        "fixture creation failed"
    );
    assert!(
        driver
            .execute(&observer, &format!("INSERT INTO {table} VALUES (7), (8)"))
            .await
            .is_ok(),
        "fixture seed failed"
    );
    let outcome = std::panic::AssertUnwindSafe(async {
        let target = ExecutionTarget::new(
            ConnectionId::new("p"),
            NamespaceTarget::new(config.database.clone(), None, config.schema.clone(), vec![]),
        );
        let grant = MonitorGrant {
            id: "live-query-grant".into(),
            dashboard_id: "live-dashboard".into(),
            widget_id: "live-widget".into(),
            workflow_id: "live-workflow".into(),
            workflow_revision: "live-frozen-definition".into(),
            organization_id: "live-monitor-org".into(),
            service_principal: "monitor-service:live-query-grant".into(),
            approved_by: "fixture-owner".into(),
            scopes: vec![MonitorScope {
                target: target.clone(),
                config_revision: 1,
                command: "query".into(),
            }],
            actions: vec!["connect".into(), "query".into()],
            read_only: true,
            expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
            revocation_version: 1,
            revoked: false,
        };
        let host = runtime::host(config.clone(), driver.clone(), provider, grant);
        let sql = format!("SELECT COUNT(*) AS n, SUM(id) AS total FROM {table}");
        let step = WorkflowCommandStep::new(
            "scheduled-query",
            "query",
            Some("p".into()),
            serde_json::json!({"sql":sql,"database":config.database,"schema":config.schema}),
        );
        let result = resource_execution::execute(&host, &step, "p", None, None, true)
            .await
            .unwrap_or_else(|error| {
                use datazen_feature_core::workflow::error::WorkflowError;
                match error {
                    WorkflowError::SqlExecution { code, .. }
                    | WorkflowError::SqlUnconfirmed { code, .. }
                    | WorkflowError::SqlResultRead { code, .. } => {
                        panic!(
                        "scheduled query outcome code: {code}; opened={}, executed={}, closed={}",
                        host.lifecycle.opened.load(Ordering::SeqCst),
                        host.lifecycle.executions.load(Ordering::SeqCst),
                        host.lifecycle.closed.load(Ordering::SeqCst)
                    )
                    }
                    WorkflowError::Validation(_) => {
                        panic!("scheduled query validation failed")
                    }
                    other => panic!(
                        "scheduled query failed: {:?}",
                        std::mem::discriminant(&other)
                    ),
                }
            });
        let row = &result.data["results"][0]["rows"][0];
        assert_eq!(integer(&row[0]), Some(2));
        assert_eq!(integer(&row[1]), Some(15));
        let finished = host.finished.lock().unwrap();
        assert_eq!(finished.len(), 1);
        assert_eq!(finished[0].state, ExecutionState::Succeeded);
        assert_eq!(finished[0].effect_outcome, EffectOutcome::Completed);
        assert!(!finished[0].artifact_ids.is_empty());
        let artifact = finished[0].artifact_ids[0].clone();
        let provenance = finished[0].provenance.as_ref().unwrap();
        assert_eq!(provenance.principal_id, host.context.principal_id);
        assert_eq!(provenance.requested_target, target);
        drop(finished);
        // A separate pooled connection independently observes the fixture, without
        // relying on the ResourceProvider's result sink or Runtime result storage.
        let readback = driver
            .query(&observer, &sql)
            .await
            .unwrap_or_else(|_| panic!("independent monitor readback failed"));
        let independent = serde_json::to_value(&readback.rows[0]).unwrap();
        assert_eq!(integer(&independent[0]), Some(2));
        assert_eq!(integer(&independent[1]), Some(15));
        // Invalid SQL and a redirected namespace must fail before another physical
        // resource or SQL execution is dispatched.
        for sql in [
            format!("DELETE FROM {table}"),
            format!("SELECT * FROM foreign_scope.{table}"),
            "SELECT unknown_function(1)".into(),
        ] {
            let mut invalid = step.clone();
            invalid.input["sql"] = serde_json::json!(sql);
            assert!(
                resource_execution::execute(&host, &invalid, "p", None, None, true)
                    .await
                    .is_err()
            );
        }
        let mut redirected = step.clone();
        redirected.input["database"] = serde_json::json!("unapproved_database");
        assert!(
            resource_execution::execute(&host, &redirected, "p", None, None, true)
                .await
                .is_err()
        );
        host.policy.grant.write().unwrap().revoked = true;
        assert!(host
            .runtime
            .read_artifact_chunk(
                &host.context,
                artifact,
                datazen_platform_api::id::Counter::ZERO
            )
            .await
            .is_err());
        assert!(
            resource_execution::execute(&host, &step, "p", None, None, true)
                .await
                .is_err()
        );
        assert_eq!(host.lifecycle.opened.load(Ordering::SeqCst), 1);
        assert_eq!(host.lifecycle.executions.load(Ordering::SeqCst), 1);
        for _ in 0..250 {
            if host.lifecycle.closed.load(Ordering::SeqCst) == 1 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert_eq!(host.lifecycle.closed.load(Ordering::SeqCst), 1);
    })
    .catch_unwind()
    .await;
    assert!(
        driver
            .execute(&observer, &format!("DROP TABLE {table}"))
            .await
            .is_ok(),
        "monitor fixture cleanup failed"
    );
    assert!(driver.disconnect(observer).await.is_ok());
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
}
