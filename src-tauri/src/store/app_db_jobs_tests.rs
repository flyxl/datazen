use super::jobs::SqliteJobRepository;
use super::{AppDb, AppDbError, APP_DB_FILE, SCHEMA_VERSION};
use datazen_platform_api::context::RequestContext;
use datazen_platform_api::dto::execution::{EffectOutcome, ExecutionErrorCode};
use datazen_platform_api::dto::job::{
    Checkpoint, CommitBoundary, JobDefinition, JobDomainResult, JobProgress, JobRecoveryRequest,
    JobRecoveryResult, JobRecoveryVerdict, JobRecoveryVerification, JobResultCounter,
    JobResultItem, JobState, RecoveryFilter,
};
use datazen_platform_api::error::PortError;
use datazen_platform_api::id::{
    ArtifactId, AuthenticationSessionId, ClientInstanceId, Counter, IdempotencyKey, JobId,
    OrganizationId, PrincipalId, RequestId, StageId, Timestamp, WorkerId,
};
use datazen_platform_api::ports::job::JobRepository;
use datazen_runtime::budget::{BudgetConfig, BudgetLedger};
use datazen_runtime::job::{
    CancelToken, DesktopJobHost, FrozenPlan, JobClock, JobHandler, JobRecoveryVerifier, JobRuntime,
    RecoveryVerdict, SharedClock, StageOutcome, StageSpec, StageTerminal,
};
use rusqlite::Connection;
use serde_json::json;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

fn request_context() -> RequestContext {
    RequestContext::new(
        OrganizationId::new("org-local"),
        PrincipalId::new("principal-local"),
        Some(AuthenticationSessionId::new(
            "synthetic-session-token-never-persist",
        )),
        ClientInstanceId::new("client-local"),
        RequestId::new("request-local"),
        None,
    )
}

fn clock() -> Arc<SharedClock> {
    Arc::new(SharedClock::at("2026-01-01T00:00:00Z"))
}

fn repository(path: &Path, clock: Arc<SharedClock>, claim_ttl_secs: i64) -> SqliteJobRepository {
    let db = AppDb::open(path).expect("AppDb should open");
    let clock: Arc<dyn JobClock> = clock;
    SqliteJobRepository::new(db, clock, claim_ttl_secs)
}

fn definition(job_id: &str, plan_id: &str) -> JobDefinition {
    JobDefinition {
        job_id: JobId::new(job_id),
        kind: "schemaDiffApply".into(),
        owner: datazen_platform_api::OwnerRef::ClientSession {
            client_instance_id: ClientInstanceId::new("client-local"),
            purpose: "migration".into(),
        },
        payload: json!({
            "consumedPlanId": plan_id,
            "planId": plan_id,
            "planDigest": "sha256:abcd1234",
            "selectionRevision": 7,
            "planVersion": 1,
            "handlerVersion": 1,
            "checkpointVersion": 1,
            "recoveryTargets": [{
                "connectionId": "connection-local",
                "objectIds": ["obj-746172676574"],
            }],
            "targetBeforeFingerprint": "sha256:bef01234",
            "recoveryPolicy": "readOnlyVerifyV1",
            "useTransaction": true,
            "requireRollback": false,
            "profileRevisionDigest": "sha256:deadbeef",
        }),
        created_at: Timestamp::new("2025-12-31T23:59:00Z"),
    }
}

fn idempotency_key(value: &str) -> IdempotencyKey {
    IdempotencyKey::new(value)
}

fn legacy_v1_database(path: &Path, conflict_with_v2: bool) {
    let db_path = path.join(APP_DB_FILE);
    std::fs::create_dir_all(path).expect("temp directory should exist");
    let conn = Connection::open(db_path).expect("legacy database should open");
    conn.execute_batch(
        "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY NOT NULL, applied_at TEXT NOT NULL);
         INSERT INTO schema_migrations(version,applied_at) VALUES (1,'2025-01-01T00:00:00Z');
         CREATE TABLE workflows (
           id TEXT PRIMARY KEY NOT NULL, name TEXT NOT NULL, description TEXT NOT NULL DEFAULT '',
           visibility TEXT NOT NULL DEFAULT 'user', definition_yaml TEXT NOT NULL,
           updated_at TEXT NOT NULL, created_at TEXT NOT NULL
         );
         INSERT INTO workflows(id,name,definition_yaml,updated_at,created_at)
           VALUES ('legacy-workflow','Legacy','id: legacy-workflow','2025-01-01','2025-01-01');",
    )
    .expect("legacy v1 schema should be seeded");
    if conflict_with_v2 {
        conn.execute_batch("CREATE TABLE job_stages (legacy_column TEXT);")
            .expect("migration collision should be seeded");
    }
}

#[test]
fn v1_migration_preserves_legacy_workflows_and_failure_rolls_back_v2_ddl() {
    let temp = tempfile::tempdir().expect("temp directory");
    legacy_v1_database(temp.path(), false);

    let db = AppDb::open(temp.path()).expect("v1 database migrates");
    let workflow = db
        .get_workflow("legacy-workflow")
        .expect("legacy workflow remains readable");
    assert_eq!(workflow.name, "Legacy");
    let version: i32 = db
        .with_conn(|conn| {
            conn.query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .map_err(AppDbError::from)
        })
        .expect("migration version should be readable");
    assert_eq!(version, SCHEMA_VERSION);
    drop(db);

    let failed = tempfile::tempdir().expect("temp directory");
    legacy_v1_database(failed.path(), true);
    assert!(AppDb::open(failed.path()).is_err());
    let raw = Connection::open(failed.path().join(APP_DB_FILE)).expect("legacy db reopens");
    let version: i32 = raw
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .expect("migration version should remain readable");
    assert_eq!(version, 1, "failed v2 migration must not advance version");
    let created_jobs: i64 = raw
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='jobs'",
            [],
            |row| row.get(0),
        )
        .expect("schema catalog should be readable");
    assert_eq!(
        created_jobs, 0,
        "failed transaction must remove partial v2 DDL"
    );
}

#[tokio::test]
async fn accept_receipt_plan_and_result_row_commit_atomically_and_replay_idempotently() {
    let temp = tempfile::tempdir().expect("temp directory");
    let clock = clock();
    let repo = repository(temp.path(), clock.clone(), 30);
    repo.db
        .with_conn(|conn| {
            conn.execute_batch(
                "CREATE TRIGGER reject_job_result_details BEFORE INSERT ON job_result_details
                 BEGIN SELECT RAISE(ABORT,'synthetic write failure'); END;",
            )?;
            Ok(())
        })
        .expect("failure trigger should be created");

    let ctx = request_context();
    let key = idempotency_key("synthetic-idempotency-token-marker");
    assert!(
        JobRepository::accept(&repo, &ctx, definition("job-atomic", "plan-atomic"), &key)
            .await
            .is_err()
    );
    repo.db
        .with_conn(|conn| {
            for table in ["jobs", "job_idempotency_receipts", "job_result_details"] {
                let count: i64 =
                    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                        row.get(0)
                    })?;
                assert_eq!(count, 0, "{table} must roll back with admission");
            }
            conn.execute_batch("DROP TRIGGER reject_job_result_details;")?;
            Ok(())
        })
        .expect("transaction rollback should leave a usable database");

    let accepted =
        JobRepository::accept(&repo, &ctx, definition("job-atomic", "plan-atomic"), &key)
            .await
            .expect("same request should succeed after injected failure is removed");
    let replay = JobRepository::accept(
        &repo,
        &ctx,
        definition("ignored-replay-id", "plan-atomic"),
        &key,
    )
    .await
    .expect("same idempotency key and payload returns receipt");
    assert_eq!(replay.view.job_id, accepted.view.job_id);

    let mut conflicting = definition("job-conflicting-replay", "plan-atomic");
    conflicting.payload["planDigest"] = json!("sha256:feedcafe");
    assert!(matches!(
        JobRepository::accept(&repo, &ctx, conflicting, &key).await,
        Err(PortError::IdempotencyConflict)
    ));
    assert!(matches!(
        JobRepository::accept(
            &repo,
            &ctx,
            definition("job-second-key", "plan-atomic"),
            &idempotency_key("another-key"),
        )
        .await,
        Err(PortError::PlanAlreadyConsumed(_))
    ));

    let details = datazen_runtime::job::JobRuntimeRepository::get_details(
        &repo,
        &ctx,
        accepted.view.job_id.clone(),
    )
    .await
    .expect("details should be queryable");
    assert_eq!(details.plan_id.as_deref(), Some("plan-atomic"));
    assert_eq!(
        details.target_before_fingerprint.as_deref(),
        Some("sha256:bef01234")
    );
    assert_eq!(details.recovery_targets.len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_independent_repositories_racing_for_one_plan_accept_only_one_job() {
    let temp = tempfile::tempdir().expect("temp directory");
    let clock = clock();
    let left = Arc::new(repository(temp.path(), clock.clone(), 30));
    let right = Arc::new(repository(temp.path(), clock, 30));
    let barrier = Arc::new(tokio::sync::Barrier::new(2));
    let ctx = request_context();
    let left_ctx = ctx.clone();
    let right_ctx = ctx;
    let left_barrier = barrier.clone();
    let right_barrier = barrier;
    let left_task = tokio::spawn(async move {
        left_barrier.wait().await;
        JobRepository::accept(
            left.as_ref(),
            &left_ctx,
            definition("job-race-left", "plan-race"),
            &idempotency_key("race-left"),
        )
        .await
    });
    let right_task = tokio::spawn(async move {
        right_barrier.wait().await;
        JobRepository::accept(
            right.as_ref(),
            &right_ctx,
            definition("job-race-right", "plan-race"),
            &idempotency_key("race-right"),
        )
        .await
    });
    let left_result = left_task.await.expect("left task joins");
    let right_result = right_task.await.expect("right task joins");
    assert_ne!(left_result.is_ok(), right_result.is_ok());
    let loser = if left_result.is_err() {
        left_result
    } else {
        right_result
    };
    assert!(matches!(loser, Err(PortError::PlanAlreadyConsumed(_))));
}

#[tokio::test]
async fn accept_then_restart_before_dispatch_is_durable_not_executed_and_never_replayed() {
    let temp = tempfile::tempdir().expect("temp directory");
    let clock = clock();
    let ctx = request_context();
    let id = JobId::new("job-crash-before-dispatch");
    let repo = Arc::new(repository(temp.path(), clock.clone(), 30));
    let host = DesktopJobHost::new(
        repo.clone(),
        Arc::new(Mutex::new(BudgetLedger::new(BudgetConfig::team_default()))),
        clock.clone(),
    );
    host.accept(
        &ctx,
        definition(id.as_str(), "plan-crash-before-dispatch"),
        &idempotency_key("crash-before-dispatch-key"),
    )
    .await
    .expect("durable admission completes before dispatch");
    drop(host);
    drop(repo);

    let reopened = Arc::new(repository(temp.path(), clock.clone(), 30));
    let host = DesktopJobHost::new(
        reopened.clone(),
        Arc::new(Mutex::new(BudgetLedger::new(BudgetConfig::team_default()))),
        clock,
    );
    let before_startup = host
        .recovery_candidates(&ctx, RecoveryFilter::default())
        .await
        .expect("accepted queue entry remains visible to recovery scan");
    assert_eq!(before_startup.len(), 1);
    assert_eq!(before_startup[0].view.state, JobState::Queued);

    let after_startup = host
        .mark_restart_candidates_for_verification(&ctx, RecoveryFilter::default())
        .await
        .expect("startup retires an undispatched queue entry without dispatching it");
    assert_eq!(after_startup.len(), 1);
    assert_eq!(after_startup[0].view.state, JobState::Failed);
    assert_eq!(
        after_startup[0].view.effect_outcome,
        Some(EffectOutcome::NotStarted)
    );
    assert_eq!(
        after_startup[0].view.error.as_deref(),
        Some("notDispatchedAfterRestart")
    );
    assert!(after_startup[0].view.pending_verification_reason.is_none());

    let details = host
        .details(&ctx, id.clone())
        .await
        .expect("not-executed verdict remains durable");
    assert_eq!(
        details.recovery.unwrap().verdict,
        JobRecoveryVerdict::NotExecuted
    );
    assert!(matches!(
        JobRepository::claim(
            reopened.as_ref(),
            &ctx,
            id,
            WorkerId::new("worker-after-restart"),
        )
        .await,
        Err(PortError::CasConflict { .. })
    ));
}

#[tokio::test]
async fn sqlite_runtime_reopens_unknown_boundary_receipt_and_verifies_without_replay() {
    let temp = tempfile::tempdir().expect("temp directory");
    let clock = clock();
    let repo = Arc::new(repository(temp.path(), clock.clone(), 30));
    let ctx = request_context();
    let id = JobId::new("job-reopen");
    JobRepository::accept(
        repo.as_ref(),
        &ctx,
        definition(id.as_str(), "plan-reopen"),
        &idempotency_key("reopen-key"),
    )
    .await
    .expect("job should be accepted");

    let calls = Arc::new(AtomicUsize::new(0));
    let handler = Arc::new(UnknownCommitHandler {
        calls: calls.clone(),
    });
    let mut handlers = datazen_runtime::job::HandlerRegistry::new();
    handlers.register(handler);
    let ledger = Arc::new(Mutex::new(BudgetLedger::new(BudgetConfig::team_default())));
    let clock_trait: Arc<dyn JobClock> = clock.clone();
    let runtime = JobRuntime::new(
        repo.clone(),
        Arc::new(handlers),
        ledger.clone(),
        clock_trait.clone(),
        0,
    );
    let result = runtime
        .run(&ctx, &id, &WorkerId::new("worker-local"), &[])
        .await
        .expect("runtime should persist the unknown commit outcome");
    assert_eq!(result.state, JobState::Failed);
    assert_eq!(result.effect_outcome, EffectOutcome::Unknown);
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    let details_before =
        datazen_runtime::job::JobRuntimeRepository::get_details(repo.as_ref(), &ctx, id.clone())
            .await
            .expect("details should be queryable before reopen");
    assert_eq!(details_before.commit_boundaries.len(), 1);
    assert_eq!(details_before.domain_results.len(), 1);
    assert_eq!(details_before.job.progress.unknown, Counter::new(1));
    assert_eq!(
        details_before.job.error.as_deref(),
        Some("unknownCommitBoundary")
    );
    assert_eq!(
        details_before.recovery.as_ref().map(|value| value.verdict),
        Some(JobRecoveryVerdict::PendingVerification)
    );
    let checkpoint =
        datazen_runtime::job::JobRuntimeRepository::latest_checkpoint(repo.as_ref(), &ctx, &id)
            .await
            .expect("checkpoint read should succeed");
    assert!(checkpoint.is_some());

    drop(runtime);
    drop(repo);
    let reopened = Arc::new(repository(temp.path(), clock.clone(), 30));
    let reopened_host = DesktopJobHost::new(reopened.clone(), ledger, clock_trait);
    let candidates = reopened_host
        .mark_restart_candidates_for_verification(&ctx, RecoveryFilter::default())
        .await
        .expect("restart scan should be read-only for terminal failed jobs");
    assert_eq!(candidates.len(), 1);
    let details = reopened_host
        .details(&ctx, id.clone())
        .await
        .expect("details should survive AppDb close and reopen");
    assert_eq!(details.commit_boundaries, details_before.commit_boundaries);
    assert_eq!(details.domain_results, details_before.domain_results);
    assert_eq!(
        details.job.artifact_ids,
        vec![ArtifactId::new("artifact-1")]
    );
    assert_eq!(
        details.target_before_fingerprint.as_deref(),
        Some("sha256:bef01234")
    );
    assert_eq!(details.recovery_targets.len(), 1);
    assert_eq!(details.job.progress.unknown, Counter::new(1));
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "reopen must not dispatch handler"
    );

    let verifier = Arc::new(ConfirmReadOnlyVerifier {
        calls: Arc::new(AtomicUsize::new(0)),
    });
    let verdict = reopened_host
        .verify_recovery(&ctx, id.clone(), verifier.clone())
        .await
        .expect("explicit verifier should persist the read-only verdict");
    assert_eq!(
        verdict.recovery.unwrap().verdict,
        JobRecoveryVerdict::ResumeAfterVerify
    );
    assert_eq!(verifier.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "verification must not replay apply"
    );
    assert_eq!(verdict.job.state, JobState::Failed);
}

#[tokio::test]
async fn cancel_claim_fencing_and_durable_json_exclude_session_and_idempotency_secrets() {
    let temp = tempfile::tempdir().expect("temp directory");
    let clock = clock();
    let repo = Arc::new(repository(temp.path(), clock.clone(), 1));
    let ctx = request_context();
    let id = JobId::new("job-cancel");
    JobRepository::accept(
        repo.as_ref(),
        &ctx,
        definition(id.as_str(), "plan-cancel"),
        &idempotency_key("synthetic-idempotency-token-marker"),
    )
    .await
    .expect("job accepted");
    let host = DesktopJobHost::new(
        repo.clone(),
        Arc::new(Mutex::new(BudgetLedger::new(BudgetConfig::team_default()))),
        clock.clone(),
    );
    let cancelled = host
        .cancel(&ctx, &id)
        .await
        .expect("queued cancel succeeds");
    assert_eq!(cancelled.view.state, JobState::Cancelled);
    assert_eq!(
        cancelled.view.effect_outcome,
        Some(EffectOutcome::NotStarted)
    );

    let id2 = JobId::new("job-fence");
    JobRepository::accept(
        repo.as_ref(),
        &ctx,
        definition(id2.as_str(), "plan-fence"),
        &idempotency_key("fence-key"),
    )
    .await
    .expect("second job accepted");
    let stale = JobRepository::claim(
        repo.as_ref(),
        &ctx,
        id2.clone(),
        WorkerId::new("worker-old"),
    )
    .await
    .expect("first claim succeeds");
    clock.set(stale.expires_at.as_str());
    JobRepository::claim(
        repo.as_ref(),
        &ctx,
        id2.clone(),
        WorkerId::new("worker-new"),
    )
    .await
    .expect("expired claim can be fenced by a new generation");
    let stale_write = datazen_runtime::job::JobRuntimeRepository::record_progress(
        repo.as_ref(),
        &ctx,
        &stale,
        JobProgress {
            attempted: Counter::new(1),
            ..JobProgress::default()
        },
    )
    .await;
    assert!(matches!(stale_write, Err(PortError::StaleClaim)));

    let text = repo
        .db
        .with_conn(|conn| {
            let mut values = Vec::new();
            for query in [
                "SELECT plan_json FROM jobs",
                "SELECT owner_json FROM jobs",
                "SELECT idempotency_key_hash || request_digest || receipt_projection FROM job_idempotency_receipts",
                "SELECT boundary_json FROM job_commit_boundaries",
                "SELECT checkpoint_json FROM job_checkpoints",
                "SELECT recovery_json FROM job_result_details WHERE recovery_json IS NOT NULL",
                "SELECT result_json FROM job_domain_results",
                "SELECT result_error_code FROM jobs WHERE result_error_code IS NOT NULL",
            ] {
                let mut statement = conn.prepare(query)?;
                let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
                for row in rows {
                    values.push(row?);
                }
            }
            Ok(values.join("\n"))
        })
        .expect("persisted audit projection should be readable");
    assert!(!text.contains("synthetic-session-token-never-persist"));
    assert!(!text.contains("synthetic-idempotency-token-marker"));
    assert!(!text.contains("dbSessionId"));
    assert!(!text.to_ascii_lowercase().contains("password"));
}

#[tokio::test]
async fn app_state_reopen_marks_interrupted_job_for_verification_without_dispatch() {
    use crate::db::registry::DriverRegistry;
    use crate::store::Store;
    use crate::transfer::adapter_registry::SyncAdapterRegistry;

    let _keyring = crate::testing::FileKeyringGuard::set();
    let temp = tempfile::tempdir().expect("temp directory");
    let first_store = Arc::new(
        Store::init_with_path(temp.path())
            .await
            .expect("first store"),
    );
    let first_state = crate::finish_app_state(
        first_store.clone(),
        Arc::new(DriverRegistry::new()),
        Arc::new(SyncAdapterRegistry::new()),
        None,
    );
    let ctx = RequestContext::new(
        OrganizationId::new("datazen-local"),
        PrincipalId::new("datazen-local-user"),
        None,
        ClientInstanceId::new("datazen-local-client"),
        RequestId::new("app-state-reopen"),
        None,
    );
    let job_id = JobId::new("job-app-state-reopen");
    first_state
        .desktop_job_host
        .accept(
            &ctx,
            definition(job_id.as_str(), "plan-app-state-reopen"),
            &idempotency_key("app-state-reopen-idempotency"),
        )
        .await
        .expect("durable accept");
    let repository = first_state.desktop_job_host.repository();
    JobRepository::claim(
        repository.as_ref(),
        &ctx,
        job_id.clone(),
        WorkerId::new("worker-before-app-reopen"),
    )
    .await
    .expect("simulate a running worker claim");
    drop(repository);
    drop(first_state);
    drop(first_store);

    let reopened_store = Arc::new(
        Store::init_with_path(temp.path())
            .await
            .expect("reopened store"),
    );
    let reopened_state = crate::finish_app_state(
        reopened_store,
        Arc::new(DriverRegistry::new()),
        Arc::new(SyncAdapterRegistry::new()),
        None,
    );
    crate::recover_desktop_jobs_at_startup(&reopened_state)
        .await
        .expect("startup recovery scan");
    let details = reopened_state
        .desktop_job_host
        .details(&ctx, job_id)
        .await
        .expect("durable details after AppState recreation");
    assert_eq!(details.job.state, JobState::Failed);
    assert_eq!(
        details.job.pending_verification_reason.as_deref(),
        Some("restartNeedsVerification")
    );
    assert_eq!(
        details.recovery.as_ref().map(|result| result.verdict),
        Some(JobRecoveryVerdict::PendingVerification)
    );
}

struct UnknownCommitHandler {
    calls: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl JobHandler for UnknownCommitHandler {
    fn kind(&self) -> &str {
        "schemaDiffApply"
    }

    fn handler_version(&self) -> u64 {
        1
    }

    fn validate_plan(
        &self,
        _plan: &FrozenPlan,
    ) -> Result<Vec<StageSpec>, datazen_runtime::job::JobError> {
        Ok(vec![StageSpec {
            stage_id: StageId::new("apply-stage"),
            kind: "apply".into(),
            depends_on: Vec::new(),
        }])
    }

    async fn run_stage(
        &self,
        spec: &StageSpec,
        _cancel: &CancelToken,
    ) -> Result<StageOutcome, datazen_runtime::job::JobError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let boundary = CommitBoundary {
            stage_id: spec.stage_id.clone(),
            stable_target_fingerprint: "sha256:target1234".into(),
            committed_at: Timestamp::new("2026-01-01T00:00:01Z"),
            operation_id: Some("operation-1".into()),
            batch_id: None,
            payload_digest: Some("sha256:payload1234".into()),
            evidence: vec!["confirmed-operation-1".into()],
            verified_at: None,
        };
        Ok(StageOutcome {
            stage_id: spec.stage_id.clone(),
            terminal: StageTerminal::Unknown,
            progress: JobProgress {
                attempted: Counter::new(1),
                unknown: Counter::new(1),
                ..JobProgress::default()
            },
            commit_boundaries: vec![boundary],
            execution_ids: Vec::new(),
            artifact_ids: vec![ArtifactId::new("artifact-1")],
            effect_outcome: EffectOutcome::Unknown,
            error_code: Some(ExecutionErrorCode::ProtocolError),
        })
    }

    fn verify_recovery(&self, _checkpoint: &Checkpoint) -> RecoveryVerdict {
        RecoveryVerdict::RequireManualReview {
            reason: "readOnlyVerifierRequired".into(),
        }
    }

    fn durable_result(&self, outcome: &StageOutcome) -> Option<JobDomainResult> {
        Some(JobDomainResult {
            stage_id: outcome.stage_id.clone(),
            result_code: "schemaDiffApply".into(),
            outcome_code: "unknown".into(),
            counters: vec![JobResultCounter {
                code: "attempted".into(),
                value: Counter::new(1),
            }],
            items: vec![JobResultItem {
                item_id: "obj-746172676574".into(),
                outcome_code: "unknown".into(),
                reason_code: Some("commitUnknown".into()),
            }],
            artifact_ids: outcome.artifact_ids.clone(),
        })
    }
}

struct ConfirmReadOnlyVerifier {
    calls: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl JobRecoveryVerifier for ConfirmReadOnlyVerifier {
    fn kind(&self) -> &str {
        "schemaDiffApply"
    }

    async fn verify(
        &self,
        _ctx: &RequestContext,
        request: &JobRecoveryRequest,
    ) -> Result<JobRecoveryVerification, PortError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert_eq!(request.details.plan_id.as_deref(), Some("plan-reopen"));
        assert_eq!(
            request.details.target_before_fingerprint.as_deref(),
            Some("sha256:bef01234")
        );
        assert_eq!(request.details.recovery_targets.len(), 1);
        assert!(request.checkpoint.is_some());
        Ok(JobRecoveryVerification {
            result: JobRecoveryResult {
                verdict: JobRecoveryVerdict::ResumeAfterVerify,
                resume_through: Some(1),
                reason_code: None,
            },
            confirmed_boundaries: Vec::new(),
            domain_results: Vec::new(),
        })
    }
}
