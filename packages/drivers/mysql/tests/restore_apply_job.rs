//! Real MySQL → Runtime Session/ExecutionGateway → shared production restore → JobRuntime.
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
    artifact::FileArtifactStore,
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
async fn artifact_restore_records_implicit_commits_and_rejects_client_formats_before_writes() {
    let Some(mut config) =
        migration_gate::require_config("mysql", "restore-apply-job", "mysql", "restore-job", &[])
    else {
        return;
    };
    config.max_pool_size = 1;
    let driver = Arc::new(datazen_driver_mysql::MysqlDriver::new(false));
    let observer = driver
        .connect(&config)
        .await
        .unwrap_or_else(|_| panic!("fixture unavailable"));
    let schema = format!("restore_{}", uuid::Uuid::new_v4().simple());
    let target = ExecutionTarget::new(
        ConnectionId::new("p"),
        NamespaceTarget::new(config.database.clone(), None, None, vec![]),
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
        profile: datazen_driver_api::restore_sql::IMPLICIT_COMMIT.into(),
    });
    let dir = tempfile::tempdir().unwrap();
    let artifacts = Arc::new(FileArtifactStore::open(dir.path().join("artifacts")).unwrap());
    // Every name is unique within the disposable database. No cross-DDL/DML
    // atomicity is claimed: each confirmed nontransactional statement is a boundary.
    let scripts = [
        format!("CREATE TABLE {schema}_items(id int PRIMARY KEY); INSERT INTO {schema}_items VALUES (7), (8);"),
        format!("CREATE TABLE {schema}_load_probe(id int); LOAD DATA INFILE 'fixture' INTO TABLE {schema}_load_probe;"),
        format!("CREATE TABLE {schema}_client_probe(id int); SOURCE fixture.sql;"),
        format!("CREATE TABLE {schema}_failure_probe(id int); INSERT INTO {schema}_missing_relation VALUES(1); CREATE TABLE {schema}_after_failure(id int);"),
        format!("CREATE TABLE {schema}_digest_probe(id int);"),
        format!("CREATE TABLE {schema}_target_probe(id int);"),
        format!("CREATE TABLE {schema}_revision_probe(id int);"),
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
            single_transaction: false,
            overwrite: false,
        };
        match index {
            4 => plan.input_sha256 = "0".repeat(64),
            5 => plan.target.namespace.database = Some("substituted".into()),
            6 => plan.config_revision = Counter::new(2),
            _ => {}
        }
        let opened_before = lifecycle.opened.load(std::sync::atomic::Ordering::SeqCst);
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
        let details = repo.get_details(id.clone()).unwrap();
        let accepted = repo.get(&ctx, id.clone()).await.unwrap();
        assert_eq!(accepted.definition.payload, plan.payload());
        if index == 0 || index == 3 {
            let confirmed = if index == 0 { 2 } else { 1 };
            assert_eq!(details.commit_boundaries.len(), confirmed);
            for (offset, boundary) in details.commit_boundaries.iter().enumerate() {
                let statement = scripts[index].split(';').nth(offset).unwrap().trim();
                assert_eq!(
                    boundary.stable_target_fingerprint,
                    RestorePlan::target_fingerprint(&target)
                );
                assert_eq!(
                    boundary.operation_id,
                    Some(format!("restore-statement-{}", offset + 1))
                );
                assert_eq!(
                    boundary.payload_digest,
                    Some(format!("{:x}", Sha256::digest(statement.as_bytes())))
                );
                assert_eq!(boundary.evidence.len(), 1);
                assert!(details
                    .job
                    .execution_ids
                    .iter()
                    .any(|id| boundary.evidence[0] == format!("execution:{}", id.as_str())));
            }
            assert!(!details.job.execution_ids.is_empty());
            assert_eq!(result.progress.committed.get(), confirmed as u64);
            assert_eq!(result.progress.attempted.get(), 2);
            if index == 0 {
                assert_eq!(result.state, JobState::Succeeded);
                assert_eq!(result.effect_outcome, EffectOutcome::Completed);
            } else {
                assert_eq!(result.state, JobState::Failed);
                // The real provider returns an error without per-statement effect
                // proof. Production preserves Unknown, alongside confirmed DDL.
                assert_eq!(result.effect_outcome, EffectOutcome::Unknown);
            }
        } else {
            assert!(details.commit_boundaries.is_empty());
            assert!(details.job.execution_ids.is_empty());
            assert_eq!(result.state, JobState::Failed);
            assert_eq!(result.effect_outcome, EffectOutcome::NotStarted);
            assert_eq!(result.progress.committed.get(), 0);
            assert_eq!(result.progress.attempted.get(), 0);
            assert_eq!(
                lifecycle.opened.load(std::sync::atomic::Ordering::SeqCst),
                opened_before,
                "rejected artifact/target/revision must not acquire a Runtime resource"
            );
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
            &format!("SELECT id FROM {schema}_items ORDER BY id"),
        )
        .await
        .unwrap_or_else(|_| panic!("readback failed"));
    assert_eq!(read.rows.len(), 2);
    assert!(matches!(read.rows[0][0], Some(Value::Integer(7))));
    assert!(matches!(read.rows[1][0], Some(Value::Integer(8))));
    for name in [
        "load_probe",
        "client_probe",
        "after_failure",
        "digest_probe",
        "target_probe",
        "revision_probe",
    ] {
        let read = driver.query(&observer, &format!(
            "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = DATABASE() AND table_name = '{schema}_{name}'"
        )).await.unwrap_or_else(|_| panic!("rejection readback failed"));
        assert!(matches!(read.rows[0][0], Some(Value::Integer(0))));
    }
    // The failed restore's CREATE TABLE really committed and survived resource
    // close. Independent pooled readback proves this was never a rollback.
    let read = driver
        .query(
            &observer,
            &format!("SELECT COUNT(*) FROM {schema}_failure_probe"),
        )
        .await
        .unwrap_or_else(|_| panic!("implicit DDL commit readback failed"));
    assert!(matches!(read.rows[0][0], Some(Value::Integer(0))));
    for name in ["items", "failure_probe"] {
        assert!(driver
            .execute(&observer, &format!("DROP TABLE {schema}_{name}"))
            .await
            .is_ok());
    }
    assert!(driver.disconnect(observer).await.is_ok());
}
