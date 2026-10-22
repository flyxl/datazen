//! Real PostgreSQL → Runtime Session/ExecutionGateway → shared production restore → JobRuntime.
//! The test host replaces desktop policy/storage wiring, never database execution.
#[path = "../../http-support/tests/support/migration_gate.rs"]
mod migration_gate;
mod restore_apply_support;
use datazen_driver_api::{DatabaseDriver, Value};
use datazen_feature_core::{restore_gateway::*, restore_job::*};
use datazen_platform_api::{
    context::OwnerRef,
    dto::{execution::EffectOutcome, job::*},
    id::*,
    ports::{artifact::*, job::JobRepository},
    target::*,
    RequestContext,
};
use datazen_runtime::{
    artifact::FileArtifactRepository,
    budget::{BudgetConfig, BudgetLedger},
    job::*,
};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
struct Authorization;
#[async_trait::async_trait]
impl RestoreAuthorization for Authorization {
    async fn validate(&self, _: &RequestContext, p: &RestorePlan) -> Result<(), RestoreRejection> {
        if p.config_revision.get() != 1 {
            return Err(RestoreRejection::NotAuthorized);
        }
        Ok(())
    }
}
#[tokio::test]
async fn artifact_restore_commits_through_runtime_and_rejects_client_formats_before_writes() {
    let Some(mut config) = migration_gate::require_config(
        "postgresql",
        "restore-apply-job",
        "postgresql",
        "restore-job",
        &[],
    ) else {
        return;
    };
    config.max_pool_size = 1;
    let driver = Arc::new(datazen_driver_postgres::PostgresDriver::new());
    let observer = driver
        .connect(&config)
        .await
        .unwrap_or_else(|_| panic!("fixture unavailable"));
    let schema = format!("restore_{}", uuid::Uuid::new_v4().simple());
    assert!(driver
        .execute(&observer, &format!("CREATE SCHEMA {schema}"))
        .await
        .is_ok());
    config.schema = Some(schema.clone());
    let target = ExecutionTarget::new(
        ConnectionId::new("p"),
        NamespaceTarget::new(config.database.clone(), None, Some(schema.clone()), vec![]),
    );
    let ctx = RequestContext::new(
        OrganizationId::new("org"),
        PrincipalId::new("alice"),
        None,
        ClientInstanceId::new("restore-test"),
        RequestId::new("restore-test"),
        None,
    );
    let lifecycle = Arc::new(restore_apply_support::Lifecycle::default());
    let runtime = restore_apply_support::runtime(config, lifecycle.clone());
    let gateway = Arc::new(RuntimeRestoreGateway {
        runtime,
        authorization: Arc::new(Authorization),
        driver: driver.clone(),
        profile: datazen_driver_api::restore_sql::TRANSACTIONAL.into(),
    });
    let dir = tempfile::tempdir().unwrap();
    let artifacts = Arc::new(FileArtifactRepository::open(dir.path().join("artifacts")).unwrap());
    let scripts = [
        format!("CREATE TABLE {schema}.items(id int PRIMARY KEY); INSERT INTO {schema}.items VALUES (7), (8);"),
        format!("CREATE TABLE {schema}.copy_probe(id int); COPY {schema}.copy_probe FROM stdin;\n1\n\\."),
        format!("CREATE TABLE {schema}.client_probe(id int); \\connect other"),
        format!("CREATE TABLE {schema}.digest_probe(id int);"),
        format!("CREATE TABLE {schema}.target_probe(id int);"),
        format!("CREATE TABLE {schema}.revision_probe(id int);"),
        format!("CREATE TABLE {schema}.rollback_probe(id int); INSERT INTO {schema}.missing_relation VALUES(1);"),
    ];
    for (index, script) in scripts.iter().enumerate() {
        let (artifact, mut writer) = artifacts
            .create(
                &ctx,
                ArtifactSpec::new(
                    ctx.organization_id.clone(),
                    ctx.principal_id.clone(),
                    ExecutionId::new(format!("upload-{index}")),
                    RestorePlan::target_fingerprint(&target),
                ),
            )
            .await
            .unwrap();
        artifacts
            .append_chunk(&mut writer, ChunkPayload::new(script.as_bytes().to_vec()))
            .await
            .unwrap();
        artifacts.finalize(writer).await.unwrap();
        let mut plan = RestorePlan {
            plan_id: format!("restore-plan-{index}"),
            target: target.clone(),
            config_revision: Counter::new(1),
            artifact_id: artifact,
            input_sha256: format!("{:x}", Sha256::digest(script.as_bytes())),
            input_bytes: Counter::new(script.len() as u64),
            single_transaction: true,
            overwrite: false,
        };
        match index {
            3 => plan.input_sha256 = "0".repeat(64),
            4 => plan.target.namespace.schema = Some("substituted".into()),
            5 => plan.config_revision = Counter::new(2),
            _ => {}
        }
        let handler = Arc::new(RestoreApplyJob::new(
            ctx.clone(),
            plan.clone(),
            artifacts.clone(),
            gateway.clone(),
        ));
        let clock = Arc::new(SharedClock::at("2026-10-18T00:00:00Z"));
        let repo = Arc::new(InMemoryJobRepository::new(clock.clone(), 300));
        let id = JobId::new(format!("restore-job-{index}"));
        let definition = JobDefinition {
            job_id: id.clone(),
            organization_id: ctx.organization_id.clone(),
            kind: RESTORE_APPLY_KIND.into(),
            owner: OwnerRef::ClientSession {
                client_instance_id: ctx.client_instance_id.clone(),
                purpose: "restore".into(),
            },
            payload: plan.payload(),
            created_at: Timestamp::new("2026-10-18T00:00:00Z"),
        };
        repo.accept(
            &ctx,
            definition.clone(),
            &IdempotencyKey::new(format!("accept-{index}")),
        )
        .await
        .unwrap();
        assert!(
            repo.accept(
                &ctx,
                JobDefinition {
                    job_id: JobId::new(format!("replay-{index}")),
                    ..definition
                },
                &IdempotencyKey::new(format!("replay-{index}"))
            )
            .await
            .is_err(),
            "accepted plan cannot be consumed twice"
        );
        let mut handlers = HandlerRegistry::new();
        handlers.register(handler);
        let ledger = Arc::new(Mutex::new(BudgetLedger::new(BudgetConfig::from_parts(
            8,
            [1, 1, 1, 0],
        ))));
        ledger
            .lock()
            .unwrap()
            .ensure_service(&ConnectionId::new("p"));
        let jobs = JobRuntime::new(repo.clone(), Arc::new(handlers), ledger, clock, 0);
        let result = jobs
            .run(
                &ctx,
                &id,
                &WorkerId::new("restore-worker"),
                &[EndpointRef {
                    connection_id: ConnectionId::new("p"),
                    service_key: "fixture".into(),
                    objects: vec![schema.clone()],
                    role: EndpointRole::TargetWriter,
                }],
            )
            .await
            .unwrap();
        let details = repo.get_details(&ctx, id.clone()).unwrap();
        let accepted = repo.get(&ctx, id.clone()).await.unwrap();
        assert_eq!(accepted.definition.payload, plan.payload());
        if index == 0 {
            assert_eq!(details.commit_boundaries.len(), 1);
            let boundary = &details.commit_boundaries[0];
            assert_eq!(
                boundary.stable_target_fingerprint,
                RestorePlan::target_fingerprint(&target)
            );
            assert_eq!(
                boundary.payload_digest.as_deref(),
                Some(plan.input_sha256.as_str())
            );
            assert!(boundary
                .evidence
                .iter()
                .any(|e| e.starts_with("execution:")));
            assert!(!details.job.execution_ids.is_empty());
            assert_eq!(result.state, JobState::Succeeded);
            assert_eq!(result.effect_outcome, EffectOutcome::Completed);
            assert_eq!(result.progress.committed.get(), 2);
        } else if index == 6 {
            assert!(details.commit_boundaries.is_empty());
            assert!(!details.job.execution_ids.is_empty());
            assert_eq!(result.state, JobState::Failed);
            assert_eq!(result.effect_outcome, EffectOutcome::RolledBack);
            assert_eq!(result.progress.committed.get(), 0);
            assert_eq!(result.progress.attempted.get(), 2);
        } else {
            assert!(details.commit_boundaries.is_empty());
            assert!(details.job.execution_ids.is_empty());
            assert_eq!(result.state, JobState::Failed);
            assert_eq!(result.effect_outcome, EffectOutcome::NotStarted);
            assert_eq!(result.progress.committed.get(), 0);
        }
    }
    assert_eq!(
        lifecycle.opened.load(std::sync::atomic::Ordering::SeqCst),
        2
    );
    assert_eq!(
        lifecycle.closed.load(std::sync::atomic::Ordering::SeqCst),
        2
    );
    let read = driver
        .query(
            &observer,
            &format!("SELECT id FROM {schema}.items ORDER BY id"),
        )
        .await
        .unwrap_or_else(|_| panic!("readback failed"));
    assert_eq!(read.rows.len(), 2);
    assert!(matches!(read.rows[0][0], Some(Value::Integer(7))));
    assert!(matches!(read.rows[1][0], Some(Value::Integer(8))));
    for name in [
        "copy_probe",
        "client_probe",
        "digest_probe",
        "target_probe",
        "revision_probe",
        "rollback_probe",
    ] {
        let read = driver
            .query(
                &observer,
                &format!("SELECT to_regclass('{schema}.{name}') IS NULL"),
            )
            .await
            .unwrap_or_else(|_| panic!("rejection readback failed"));
        assert!(matches!(read.rows[0][0], Some(Value::Bool(true))));
    }
    assert!(driver
        .execute(&observer, &format!("DROP SCHEMA {schema} CASCADE"))
        .await
        .is_ok());
    assert!(driver.disconnect(observer).await.is_ok());
}
