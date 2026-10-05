//! SchemaDiffHandler × JobRuntime 的 CM 用例覆盖（CM-41 / CM-42 / §4.2 故障窗口表）。

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use datazen_platform_api::context::RequestContext;
use datazen_platform_api::dto::execution::EffectOutcome;
use datazen_platform_api::dto::job::{Checkpoint, JobDefinition};
use datazen_platform_api::id::{
    ClientInstanceId, ConnectionId, IdempotencyKey, JobId, JobStateVersion, PrincipalId, RequestId,
    StageId, Timestamp, WorkerId,
};
use datazen_platform_api::ports::job::JobRepository;

use datazen_runtime::budget::{BudgetConfig, BudgetLedger};
use datazen_runtime::job::{
    CancelToken, EndpointRef, EndpointRole, HandlerRegistry, InMemoryJobRepository, JobHandler,
    JobRuntime,
    RecoveryVerdict, SharedClock, StageSpec, StageTerminal,
};

use datazen_schema_diff::job::{
    ApplyRequest, PlanStore, PrepareRequest, PreparedPlan, ReadOnlyVerdict, RecoveryPolicy,
    SchemaDiffFrozenPlan, SchemaDiffHandler, SchemaDiffJobBackend, SchemaDiffPlanError, StoredPlan,
};
use datazen_schema_diff::types::{
    DeployStatus, PlanStatement, RollbackCompleteness, SchemaDiffDeployResult, SchemaDiffPlan,
    StatementExecResult, StatementRisk,
};

// ------------------------------------------------------------ payload 夹具

fn apply_payload(plan_id: &str, selection_revision: u64) -> serde_json::Value {
    serde_json::json!({
        "consumedPlanId": plan_id,
        "planVersion": 1,
        "handlerVersion": 1,
        "checkpointVersion": 1,
        "selectionRevision": selection_revision,
    })
}

fn ctx() -> RequestContext {
    RequestContext::new(
        datazen_platform_api::id::OrganizationId::new("org-1"),
        PrincipalId::new("user-a"),
        None,
        ClientInstanceId::new("client-1"),
        RequestId::new("req-1"),
        None,
    )
}

fn definition(kind: &str, payload: serde_json::Value, job_id: &str) -> JobDefinition {
    JobDefinition {
        job_id: JobId::new(job_id),
        kind: kind.into(),
        owner: datazen_platform_api::OwnerRef::ClientSession {
            client_instance_id: ClientInstanceId::new("client-1"),
            purpose: "migration".into(),
        },
        payload,
        created_at: Timestamp::new("2026-01-01T00:00:00Z"),
    }
}

fn frozen_meta(plan_id: &str, revision: u64, expires_at: &str) -> SchemaDiffFrozenPlan {
    SchemaDiffFrozenPlan {
        plan_id: plan_id.into(),
        plan_version: 1,
        handler_version: 1,
        checkpoint_version: 1,
        selection_revision: revision,
        created_at: Timestamp::new("2026-01-01T00:00:00Z"),
        expires_at: Timestamp::new(expires_at),
        source_connection_id: "conn-src".into(),
        target_connection_id: "conn-tgt".into(),
        source_database: None,
        target_database: None,
        source_schema: None,
        target_schema: None,
        endpoint_evidence: vec!["svc-a/users".into()],
        capability_snapshot_hash: "cap-1".into(),
        schema_fingerprint: "fp-1".into(),
        mapping_fingerprint: "map-1".into(),
        consistency: "tableSnapshot".into(),
        transaction_scope: "nonAtomicDDL".into(),
        recovery_policy: RecoveryPolicy::ReadOnlyVerify,
        body_artifact_ids: vec!["artifact-plan-1".into()],
        body_digests: vec!["digest-1".into()],
        confirmed_actions: vec![],
    }
}

fn plan_with_statements(n: usize) -> SchemaDiffPlan {
    SchemaDiffPlan {
        plan_id: Some("plan-1".into()),
        table: "users".into(),
        tables: vec!["users".into()],
        source_dialect: "sqlite".into(),
        target_dialect: "sqlite".into(),
        same_dialect: true,
        statements: (0..n)
            .map(|i| PlanStatement {
                sql: format!("SQL{i}"),
                risk: StatementRisk::Additive,
                rollback_sql: None,
                summary: format!("stmt {i}"),
                requires_transaction: false,
            })
            .collect(),
        warnings: vec![],
        requirements: vec![],
        rollback_completeness: RollbackCompleteness {
            complete: true,
            missing: vec![],
        },
        type_suggestions: vec![],
        expected_target_schemas: vec![],
    }
}

fn deploy_result(status: DeployStatus, ok: &[bool], error: &str) -> SchemaDiffDeployResult {
    SchemaDiffDeployResult {
        status,
        executed_count: ok.iter().filter(|x| **x).count(),
        statement_count: ok.len(),
        errors: if error.is_empty() {
            vec![]
        } else {
            vec![error.to_string()]
        },
        statement_results: ok
            .iter()
            .enumerate()
            .map(|(index, &ok)| StatementExecResult {
                index,
                sql: format!("SQL{index}"),
                ok,
                error: if ok { None } else { Some(error.into()) },
            })
            .collect(),
    }
}

// ---------------------------------------------------------- FakeBackend

struct FakeBackend {
    prepared: Option<(SchemaDiffFrozenPlan, SchemaDiffPlan, String)>,
    authorize_ok: bool,
    target_fingerprint: String,
    deploy_result: SchemaDiffDeployResult,
    verdict: ReadOnlyVerdict,
}

#[async_trait]
impl SchemaDiffJobBackend for FakeBackend {
    async fn prepare_plan(
        &self,
        _request: &PrepareRequest,
        _cancel: &CancelToken,
    ) -> Result<PreparedPlan, SchemaDiffPlanError> {
        let (meta, plan, digest) = self.prepared.clone().expect("prepared");
        Ok(PreparedPlan {
            meta,
            plan,
            body_digest: digest,
        })
    }

    async fn verify_authorization(
        &self,
        _plan_meta: &SchemaDiffFrozenPlan,
    ) -> Result<(), SchemaDiffPlanError> {
        if self.authorize_ok {
            Ok(())
        } else {
            Err(SchemaDiffPlanError::CapabilityChanged)
        }
    }

    async fn read_target_fingerprint(
        &self,
        _plan: &SchemaDiffPlan,
        _plan_meta: &SchemaDiffFrozenPlan,
    ) -> Result<String, SchemaDiffPlanError> {
        Ok(self.target_fingerprint.clone())
    }

    async fn deploy(
        &self,
        _plan: &SchemaDiffPlan,
        _request: &ApplyRequest,
        _cancel: &CancelToken,
    ) -> Result<SchemaDiffDeployResult, SchemaDiffPlanError> {
        Ok(self.deploy_result.clone())
    }

    async fn read_only_verify(
        &self,
        _plan_meta: &SchemaDiffFrozenPlan,
        _operation_id: &str,
    ) -> Result<ReadOnlyVerdict, SchemaDiffPlanError> {
        Ok(self.verdict.clone())
    }
}

// ------------------------------------------------------------------ helpers

fn store_with(meta: SchemaDiffFrozenPlan, plan: SchemaDiffPlan) -> Arc<PlanStore> {
    let store = Arc::new(PlanStore::new());
    store.insert(StoredPlan {
        meta,
        plan,
        body_digest: "digest".into(),
    });
    store
}

fn apply_request() -> ApplyRequest {
    ApplyRequest {
        target_db_session_id: "sess-t".into(),
        use_transaction: false,
        require_rollback: false,
        confirm_destructive: None,
        job_id: None,
        target_database: None,
        target_schema: None,
        profile: None,
    }
}

fn budget_ledger() -> Arc<Mutex<BudgetLedger>> {
    let config = BudgetConfig::new(
        datazen_platform_api::ports::budget::ServiceQuota::new(8, [1, 1, 1, 0]).expect("quota"),
    );
    Arc::new(Mutex::new(BudgetLedger::new(config)))
}

fn fake_ok() -> FakeBackend {
    FakeBackend {
        prepared: None,
        authorize_ok: true,
        target_fingerprint: "fp-1".into(),
        deploy_result: deploy_result(DeployStatus::Committed, &[true], ""),
        verdict: ReadOnlyVerdict::UniqueProofCommitted,
    }
}

// ------------------------------------------------------------- CM-41

#[tokio::test]
async fn cm41_expired_plan_is_rejected_before_any_effect() {
    let store = store_with(
        frozen_meta("plan-old", 1, "2000-01-01T00:00:00Z"),
        plan_with_statements(1),
    );
    let handler = SchemaDiffHandler::for_apply(
        Arc::new(fake_ok()),
        store,
        "plan-old".into(),
        1,
        apply_request(),
    );
    let plan = datazen_runtime::job::FrozenPlan {
        kind: "schemaDiffApply".into(),
        is_apply: true,
        consumed_plan_id: Some("plan-old".into()),
        plan_version: 1,
        handler_version: 1,
        checkpoint_version: 1,
        selection_revision: Some(1),
    };
    let err = handler.validate_plan(&plan).expect_err("expired");
    assert!(err.to_string().contains("expired"), "{err}");
}

#[tokio::test]
async fn cm41_selection_revision_mismatch_is_rejected() {
    let store = store_with(
        frozen_meta("plan-1", 3, "2999-01-01T00:00:00Z"),
        plan_with_statements(1),
    );
    let handler = SchemaDiffHandler::for_apply(
        Arc::new(fake_ok()),
        store,
        "plan-1".into(),
        2,
        apply_request(),
    );
    let plan = datazen_runtime::job::FrozenPlan {
        kind: "schemaDiffApply".into(),
        is_apply: true,
        consumed_plan_id: Some("plan-1".into()),
        plan_version: 1,
        handler_version: 1,
        checkpoint_version: 1,
        selection_revision: Some(2),
    };
    let err = handler.validate_plan(&plan).expect_err("stale revision");
    assert!(err.to_string().contains("selection revision"), "{err}");
}

#[tokio::test]
async fn cm41_plan_id_is_consumed_once() {
    let (repo, clock) = repo_clock();
    let repo = Arc::new(repo);
    let c = ctx();
    repo.accept(
        &c,
        definition("schemaDiffApply", apply_payload("plan-once", 1), "job-1"),
        &IdempotencyKey::new("k1"),
    )
    .await
    .expect("accept");
    let err = repo
        .accept(
            &c,
            definition("schemaDiffApply", apply_payload("plan-once", 1), "job-2"),
            &IdempotencyKey::new("k2"),
        )
        .await
        .expect_err("plan consumed");
    assert!(
        matches!(err, datazen_platform_api::error::PortError::PlanAlreadyConsumed(_)),
        "{err:?}"
    );
    let _ = clock;
}

#[tokio::test]
async fn cm41_self_cover_endpoint_overlap_is_rejected() {
    let (repo, clock) = repo_clock();
    let repo = Arc::new(repo);
    let c = ctx();
    let store = store_with(
        frozen_meta("plan-sc", 1, "2999-01-01T00:00:00Z"),
        plan_with_statements(1),
    );
    repo.accept(
        &c,
        definition("schemaDiffApply", apply_payload("plan-sc", 1), "job-sc"),
        &IdempotencyKey::new("k-sc"),
    )
    .await
    .expect("accept");
    let ledger = budget_ledger();
    ledger.lock().expect("lock").ensure_service(&ConnectionId::new("conn-src"));
    ledger.lock().expect("lock").ensure_service(&ConnectionId::new("conn-tgt"));
    let handler = SchemaDiffHandler::for_apply(
        Arc::new(fake_ok()),
        store,
        "plan-sc".into(),
        1,
        apply_request(),
    );
    let mut handlers = HandlerRegistry::new();
    handlers.register(Arc::new(handler));
    let runtime = JobRuntime::new(repo, Arc::new(handlers), ledger, clock.clone(), 0);
    let endpoints = vec![
        EndpointRef {
            connection_id: ConnectionId::new("conn-src"),
            service_key: "svc-1".into(),
            objects: vec!["users".into()],
            role: EndpointRole::SourceReader,
        },
        EndpointRef {
            connection_id: ConnectionId::new("conn-tgt"),
            service_key: "svc-1".into(),
            objects: vec!["users".into()],
            role: EndpointRole::TargetWriter,
        },
    ];
    let err = runtime
        .run(&c, &JobId::new("job-sc"), &WorkerId::new("w1"), &endpoints)
        .await
        .expect_err("overlap");
    assert!(
        err.to_string().contains("both read and written"),
        "{err:?}"
    );
}

// ------------------------------------------------------------- CM-42

fn apply_stage_spec() -> StageSpec {
    StageSpec {
        stage_id: StageId::new("apply"),
        kind: "apply".into(),
        depends_on: vec![],
    }
}

#[tokio::test]
async fn cm42_non_transactional_partial_success_keeps_confirmed_boundaries() {
    let store = store_with(
        frozen_meta("plan-p", 1, "2999-01-01T00:00:00Z"),
        plan_with_statements(3),
    );
    let mut backend = fake_ok();
    backend.deploy_result = deploy_result(DeployStatus::Mixed, &[true, false, true], "boom");
    let handler = SchemaDiffHandler::for_apply(
        Arc::new(backend),
        store,
        "plan-p".into(),
        1,
        apply_request(),
    );
    let out = handler
        .run_stage(&apply_stage_spec(), &CancelToken::new())
        .await
        .expect("stage");
    assert_eq!(out.terminal, StageTerminal::Failed);
    assert_eq!(out.effect_outcome, EffectOutcome::PartiallyApplied);
    assert_eq!(out.commit_boundaries.len(), 2, "仅已确认的操作有边界");
    assert!(out
        .commit_boundaries
        .iter()
        .all(|b| b.operation_id.as_deref().map(|s| s.starts_with("op-")).unwrap_or(false)));
}

#[tokio::test]
async fn cm42_unknown_operation_needs_read_only_verification() {
    let store = store_with(
        frozen_meta("plan-u", 1, "2999-01-01T00:00:00Z"),
        plan_with_statements(2),
    );
    let mut backend = fake_ok();
    backend.deploy_result = deploy_result(DeployStatus::Unknown, &[true, false], "ddl response lost");
    let handler = SchemaDiffHandler::for_apply(
        Arc::new(backend),
        store,
        "plan-u".into(),
        1,
        apply_request(),
    );
    let out = handler
        .run_stage(&apply_stage_spec(), &CancelToken::new())
        .await
        .expect("stage");
    assert_eq!(out.terminal, StageTerminal::Unknown);
    assert_eq!(out.effect_outcome, EffectOutcome::Unknown);
    // D2：Unknown 分支的边界证据必须写入 ddlResponseLost，使 decide_recovery 必走人工核验。
    assert!(out
        .commit_boundaries
        .iter()
        .all(|b| b.evidence.iter().any(|e| e == "ddlResponseLost")));
    assert!(out
        .execution_ids
        .iter()
        .all(|id| id.as_str().starts_with("exec-")));
    // §7：未知 operation 必须走只读核验，不能直接当 notStarted/重放。
    let checkpoint = Checkpoint {
        job_id: JobId::new("job-u"),
        state_version: JobStateVersion::new(1),
        stable_target_fingerprint: "fp-1".into(),
        committed: out.commit_boundaries.clone(),
        verification_evidence: vec!["ddlResponseLost".into()],
        recovery_policy: "readOnlyVerify".into(),
    };
    let verdict = handler.verify_recovery(&checkpoint);
    assert!(
        matches!(verdict, RecoveryVerdict::RequireManualReview { .. }),
        "{verdict:?}"
    );
}

// ---------------------------------------------------- §4.2 故障窗口表

#[tokio::test]
async fn s42_whole_transaction_rollback_is_explicitly_retryable() {
    let store = store_with(
        frozen_meta("plan-rb", 1, "2999-01-01T00:00:00Z"),
        plan_with_statements(2),
    );
    let mut backend = fake_ok();
    backend.deploy_result = deploy_result(DeployStatus::RolledBack, &[false, false], "tx failed");
    let handler = SchemaDiffHandler::for_apply(
        Arc::new(backend),
        store,
        "plan-rb".into(),
        1,
        apply_request(),
    );
    let out = handler
        .run_stage(&apply_stage_spec(), &CancelToken::new())
        .await
        .expect("stage");
    assert_eq!(out.terminal, StageTerminal::Failed);
    assert_eq!(out.effect_outcome, EffectOutcome::RolledBack);
    assert!(out.commit_boundaries.is_empty(), "整组回滚 ⇒ 无已提交边界");
}

#[tokio::test]
async fn s42_ddl_response_lost_enters_unknown_and_read_only_verify() {
    let store = store_with(
        frozen_meta("plan-lost", 1, "2999-01-01T00:00:00Z"),
        plan_with_statements(1),
    );
    let mut backend = fake_ok();
    backend.deploy_result = deploy_result(DeployStatus::Unknown, &[false], "conn lost");
    let handler = SchemaDiffHandler::for_apply(
        Arc::new(backend),
        store,
        "plan-lost".into(),
        1,
        apply_request(),
    );
    let out = handler
        .run_stage(&apply_stage_spec(), &CancelToken::new())
        .await
        .expect("stage");
    assert_eq!(out.terminal, StageTerminal::Unknown);
    assert_eq!(out.effect_outcome, EffectOutcome::Unknown);
    let checkpoint = Checkpoint {
        job_id: JobId::new("job-lost"),
        state_version: JobStateVersion::new(1),
        stable_target_fingerprint: "fp-1".into(),
        committed: out.commit_boundaries.clone(),
        verification_evidence: vec!["ddlResponseLost".into()],
        recovery_policy: "readOnlyVerify".into(),
    };
    let verdict = handler.verify_recovery(&checkpoint);
    assert!(matches!(
        verdict,
        RecoveryVerdict::RequireManualReview { .. }
    ));
}

#[tokio::test]
async fn s42_cancel_stops_new_operations_and_keeps_actual_terminal() {
    let store = store_with(
        frozen_meta("plan-c", 1, "2999-01-01T00:00:00Z"),
        plan_with_statements(3),
    );
    let mut backend = fake_ok();
    backend.deploy_result = deploy_result(DeployStatus::Cancelled, &[true, false, false], "cancelled");
    let handler = SchemaDiffHandler::for_apply(
        Arc::new(backend),
        store,
        "plan-c".into(),
        1,
        apply_request(),
    );
    let out = handler
        .run_stage(&apply_stage_spec(), &CancelToken::new())
        .await
        .expect("stage");
    assert_eq!(out.terminal, StageTerminal::Cancelled);
    assert_eq!(out.effect_outcome, EffectOutcome::PartiallyApplied);
    assert_eq!(out.commit_boundaries.len(), 1, "只保留已确认之 operation");
}

// ---------------------------------------------------------- prepare 路径

#[tokio::test]
async fn prepare_stage_stores_plan_and_reports_progress() {
    let store = Arc::new(PlanStore::new());
    let prepared = PreparedPlan {
        meta: frozen_meta("plan-new", 1, "2999-01-01T00:00:00Z"),
        plan: plan_with_statements(2),
        body_digest: "digest".into(),
    };
    let backend = Arc::new(FakeBackend {
        prepared: Some((prepared.meta, prepared.plan, prepared.body_digest)),
        authorize_ok: true,
        target_fingerprint: "fp-1".into(),
        deploy_result: deploy_result(DeployStatus::Committed, &[true, true], ""),
        verdict: ReadOnlyVerdict::UniqueProofCommitted,
    });
    let handler = SchemaDiffHandler::for_prepare(
        backend,
        store.clone(),
        PrepareRequest::Table {
            source_db_session_id: "s1".into(),
            target_db_session_id: "t1".into(),
            table_names: vec!["users".into()],
            target_table_names: vec!["users".into()],
            target_only_table_names: vec![],
            source_schema: None,
            target_schema: None,
            allow_destructive: false,
            include_indexes: None,
            type_overrides: vec![],
        },
    );
    let out = handler
        .run_stage(
            &StageSpec {
                stage_id: StageId::new("prepare"),
                kind: "prepare".into(),
                depends_on: vec![],
            },
            &CancelToken::new(),
        )
        .await
        .expect("prepare");
    assert_eq!(out.terminal, StageTerminal::Succeeded);
    assert_eq!(out.effect_outcome, EffectOutcome::Completed);
    assert_eq!(out.progress.read.get(), 1);
    assert_eq!(out.progress.converted.get(), 2);
    assert!(store.get("plan-new").is_some(), "计划已入 PlanStore");
}

fn repo_clock() -> (InMemoryJobRepository, Arc<SharedClock>) {
    let clock = Arc::new(SharedClock::at("2026-01-01T00:00:00Z"));
    let repo = InMemoryJobRepository::new(clock.clone(), 300);
    (repo, clock)
}
