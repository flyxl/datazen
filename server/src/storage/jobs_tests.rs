//! `ServerJobs` 行为钉子：与内核 `InMemoryJobRepository` 同一用例语义的
//! 服务端腿（同一用例、两种绑定的第二组对齐断言）。
//!
//! 钉的语义：受理组织盖章（INV-01）、幂等重放与指纹冲突、planId 唯一消费、
//! CM-05 越组织不可见、claim fencing（generation/worker/租约）、CAS 版本
//! 冲突与终态清 claim、checkpoint 版本冲突、恢复扫描的候选口径。
//! 提交边界没有端口读取面，落库事实经管理库直查核验（同 crate 测试）。

use std::sync::Arc;

use datazen::jobs::domain::job::{
    Checkpoint, CommitBoundary, JobClaim, JobDefinition, JobFilter, JobState, RecoveryFilter,
    StageRecord,
};
use datazen::jobs::domain::repository::JobRepository;
use datazen::jobs::domain::time::{JobClock, SharedClock};
use datazen::shared::context::{OwnerRef, RequestContext};
use datazen::shared::error::PortError;
use datazen::shared::id::{
    BlockId, ClientInstanceId, Counter, ExecutionId, IdempotencyKey, JobId, JobStateVersion,
    OrganizationId, PrincipalId, RequestId, StageId, Timestamp, WorkerId,
};
use serde_json::json;

use super::jobs::ServerJobs;
use super::AdminStore;

const CLAIM_TTL_SECS: i64 = 60;

fn migrated_store() -> Arc<AdminStore> {
    let store = AdminStore::open_in_memory().expect("in-memory store");
    store.migrate().expect("migrations apply");
    Arc::new(store)
}

fn ctx(organization: &str, principal: &str) -> RequestContext {
    RequestContext {
        organization_id: OrganizationId::new(organization),
        principal_id: PrincipalId::new(principal),
        authentication_session_id: None,
        client_instance_id: ClientInstanceId::new("test-client"),
        request_id: RequestId::new("req-test"),
        delegation_id: None,
    }
}

/// 合法计划投影：三个版本字段必须存在且 major = 1（`project_frozen_plan`）。
fn prepare_payload() -> serde_json::Value {
    json!({"planVersion": 1, "handlerVersion": 1, "checkpointVersion": 1, "steps": ["load"]})
}

fn definition(job_id: &str, payload: serde_json::Value) -> JobDefinition {
    JobDefinition {
        job_id: JobId::new(job_id),
        // 调用方自报的组织值必须被 ctx 盖章覆盖（INV-01）——这里故意填错。
        organization_id: OrganizationId::new("caller-claims-this-org"),
        kind: "workflow".into(),
        owner: OwnerRef::WorkflowBlock {
            job_id: JobId::new(job_id),
            block_id: BlockId::new("b1"),
        },
        payload,
        created_at: Timestamp::new("2000-01-01T00:00:00Z"),
    }
}

struct Fixture {
    jobs: ServerJobs,
    clock: SharedClock,
}

fn fixture() -> Fixture {
    let clock = SharedClock::at("2026-01-01T00:00:00Z");
    let jobs = ServerJobs::new(
        migrated_store(),
        Arc::new(clock.clone()) as Arc<dyn JobClock>,
        CLAIM_TTL_SECS,
    );
    Fixture { jobs, clock }
}

async fn accepted(
    jobs: &ServerJobs,
    ctx: &RequestContext,
    job_id: &str,
) -> datazen::jobs::domain::job::JobRecord {
    jobs.accept(
        ctx,
        definition(job_id, prepare_payload()),
        &IdempotencyKey::new(format!("idem-{job_id}")),
    )
    .await
    .expect("accept succeeds")
}

#[tokio::test]
async fn accept_stamps_organization_from_context_not_caller() {
    let fixture = fixture();
    let context = ctx("org-a", "user-1");

    let record = accepted(&fixture.jobs, &context, "job-1").await;

    assert_eq!(record.definition.organization_id.as_str(), "org-a");
    assert_eq!(record.view.state, JobState::Queued);
    assert_eq!(record.state_version, JobStateVersion::new(1));
    assert_eq!(record.view.created_at.as_str(), "2026-01-01T00:00:00Z");
    // 计划正文冻结落库（内存实现同口径：调用方的 created_at 不作数）。
    assert_eq!(record.definition.payload, prepare_payload());
}

#[tokio::test]
async fn same_key_same_payload_replays_the_original_job() {
    let fixture = fixture();
    let context = ctx("org-a", "user-1");
    let idem = IdempotencyKey::new("idem-1");

    let first = fixture
        .jobs
        .accept(&context, definition("job-1", prepare_payload()), &idem)
        .await
        .expect("first accept");
    let replay = fixture
        .jobs
        .accept(&context, definition("job-1", prepare_payload()), &idem)
        .await
        .expect("replay accept");

    assert_eq!(first, replay, "同键同指纹的重放返回原 Job receipt");
}

#[tokio::test]
async fn same_key_with_different_payload_is_an_idempotency_conflict() {
    let fixture = fixture();
    let context = ctx("org-a", "user-1");
    let idem = IdempotencyKey::new("idem-1");

    fixture
        .jobs
        .accept(&context, definition("job-1", prepare_payload()), &idem)
        .await
        .expect("first accept");

    let mut different = prepare_payload();
    different["steps"] = json!(["load", "verify"]);
    let outcome = fixture
        .jobs
        .accept(&context, definition("job-2", different), &idem)
        .await;

    assert!(matches!(outcome, Err(PortError::IdempotencyConflict)));
}

#[tokio::test]
async fn an_apply_job_consumes_its_plan_once_per_organization() {
    let fixture = fixture();
    let payload = json!({
        "planVersion": 1, "handlerVersion": 1, "checkpointVersion": 1,
        "consumedPlanId": "plan-1",
    });
    let apply = |job_id: &str| JobDefinition {
        kind: "dataSyncApply".into(),
        payload: payload.clone(),
        ..definition(job_id, payload.clone())
    };

    let org_a = ctx("org-a", "user-1");
    fixture
        .jobs
        .accept(&org_a, apply("job-1"), &IdempotencyKey::new("idem-1"))
        .await
        .expect("first consume");

    // 同组织第二次消费同一 planId → PlanAlreadyConsumed（失败也不释放资格）。
    let outcome = fixture
        .jobs
        .accept(&org_a, apply("job-2"), &IdempotencyKey::new("idem-2"))
        .await;
    assert!(matches!(outcome, Err(PortError::PlanAlreadyConsumed(id)) if id == "plan-1"));

    // 别组织消费同一 planId 是独立请求（消费按组织作用域）。
    let org_b = ctx("org-b", "user-2");
    let cross = fixture
        .jobs
        .accept(&org_b, apply("job-1"), &IdempotencyKey::new("idem-1"))
        .await
        .expect("other organization consumes its own plan");
    assert_eq!(cross.definition.organization_id.as_str(), "org-b");
}

#[tokio::test]
async fn cross_organization_reads_are_not_found_and_listings_are_empty() {
    let fixture = fixture();
    let org_a = ctx("org-a", "user-1");
    accepted(&fixture.jobs, &org_a, "job-1").await;

    let org_b = ctx("org-b", "user-2");
    let get = fixture.jobs.get(&org_b, JobId::new("job-1")).await;
    assert!(
        matches!(get, Err(PortError::NotFound(_))),
        "越组织读取按「不存在」处理"
    );

    // 过滤器带本组织的 id、ctx 是别组织 → 空列表（不是错误，也不泄漏存在性）。
    let filter = JobFilter {
        organization_id: OrganizationId::new("org-a"),
        states: vec![],
        owner: None,
        after: None,
        limit: None,
    };
    let listed = fixture
        .jobs
        .list(&org_b, filter)
        .await
        .expect("mismatched listing is empty, not an error");
    assert!(listed.is_empty());

    // 一致的过滤器看到本组织的行。
    let own_filter = JobFilter {
        organization_id: OrganizationId::new("org-a"),
        states: vec![JobState::Queued],
        owner: None,
        after: None,
        limit: None,
    };
    let listed = fixture
        .jobs
        .list(&org_a, own_filter)
        .await
        .expect("own listing");
    assert_eq!(listed.len(), 1);
}

#[tokio::test]
async fn claim_fences_worker_writes_by_generation_and_lease() {
    let fixture = fixture();
    let org_a = ctx("org-a", "user-1");
    accepted(&fixture.jobs, &org_a, "job-1").await;

    let claim = fixture
        .jobs
        .claim(&org_a, JobId::new("job-1"), WorkerId::new("w-1"))
        .await
        .expect("claim");
    assert_eq!(claim.claim_generation, Counter::new(1));
    assert_eq!(claim.expires_at.as_str(), "2026-01-01T00:01:00Z");

    // 别的 worker 拿着同代 claim 写 → StaleClaim（fencing 检查的是 claim
    // 三元组，不是 stage 的 claimed_by 数据字段）。
    let forged = JobClaim {
        worker_id: WorkerId::new("w-2"),
        ..claim.clone()
    };
    let stage = StageRecord {
        job_id: JobId::new("job-1"),
        stage_id: StageId::new("stage-1"),
        kind: "sql".into(),
        claimed_by: Some(WorkerId::new("w-2")),
        execution_ids: vec![],
        started_at: None,
        finished_at: None,
    };
    let outcome = fixture.jobs.record_stage(&org_a, &forged, stage).await;
    assert!(matches!(outcome, Err(PortError::StaleClaim)));

    // 租约过期后同一 worker 的写入也拒绝（fence 检查租约）。
    fixture.clock.set("2026-01-01T00:02:00Z");
    let stage = StageRecord {
        job_id: JobId::new("job-1"),
        stage_id: StageId::new("stage-1"),
        kind: "sql".into(),
        claimed_by: Some(WorkerId::new("w-1")),
        execution_ids: vec![],
        started_at: None,
        finished_at: None,
    };
    let outcome = fixture.jobs.record_stage(&org_a, &claim, stage).await;
    assert!(
        matches!(outcome, Err(PortError::StaleClaim)),
        "过期租约的写入一律拒绝"
    );
}

#[tokio::test]
async fn takeover_after_lease_expiry_bumps_the_generation() {
    let fixture = fixture();
    let org_a = ctx("org-a", "user-1");
    accepted(&fixture.jobs, &org_a, "job-1").await;

    let first = fixture
        .jobs
        .claim(&org_a, JobId::new("job-1"), WorkerId::new("w-1"))
        .await
        .expect("first claim");
    fixture.clock.set("2026-01-01T00:02:00Z");
    let second = fixture
        .jobs
        .claim(&org_a, JobId::new("job-1"), WorkerId::new("w-2"))
        .await
        .expect("takeover after expiry");
    assert_eq!(second.claim_generation, Counter::new(2));
    assert_ne!(second.worker_id, first.worker_id);

    // 旧代的写入仍然拒绝（ fencing 生效到新代）。
    let stage = StageRecord {
        job_id: JobId::new("job-1"),
        stage_id: StageId::new("stage-1"),
        kind: "sql".into(),
        claimed_by: Some(WorkerId::new("w-1")),
        execution_ids: vec![],
        started_at: None,
        finished_at: None,
    };
    let outcome = fixture.jobs.record_stage(&org_a, &first, stage).await;
    assert!(matches!(outcome, Err(PortError::StaleClaim)));
}

#[tokio::test]
async fn stage_writes_merge_execution_ids_and_persist_stage_order() {
    let fixture = fixture();
    let org_a = ctx("org-a", "user-1");
    accepted(&fixture.jobs, &org_a, "job-1").await;
    let claim = fixture
        .jobs
        .claim(&org_a, JobId::new("job-1"), WorkerId::new("w-1"))
        .await
        .expect("claim");

    fixture
        .jobs
        .record_stage(
            &org_a,
            &claim,
            StageRecord {
                job_id: JobId::new("job-1"),
                stage_id: StageId::new("stage-1"),
                kind: "sql".into(),
                claimed_by: Some(WorkerId::new("w-1")),
                execution_ids: vec![ExecutionId::new("exec-1")],
                started_at: Some(Timestamp::new("2026-01-01T00:00:01Z")),
                finished_at: Some(Timestamp::new("2026-01-01T00:00:02Z")),
            },
        )
        .await
        .expect("first stage");
    fixture
        .jobs
        .record_stage(
            &org_a,
            &claim,
            StageRecord {
                job_id: JobId::new("job-1"),
                stage_id: StageId::new("stage-2"),
                kind: "sql".into(),
                claimed_by: Some(WorkerId::new("w-1")),
                execution_ids: vec![ExecutionId::new("exec-2")],
                started_at: None,
                finished_at: None,
            },
        )
        .await
        .expect("second stage");

    let record = fixture
        .jobs
        .get(&org_a, JobId::new("job-1"))
        .await
        .expect("get");
    assert_eq!(record.stages.len(), 2);
    assert_eq!(record.stages[0].stage_id.as_str(), "stage-1");
    assert_eq!(record.stages[1].stage_id.as_str(), "stage-2");
    assert_eq!(record.view.execution_ids.len(), 2, "execution_ids 取并集");
    assert_eq!(record.view.stage.as_deref(), Some("stage-2"));
}

#[tokio::test]
async fn compare_and_set_rejects_stale_versions_and_clears_claim_on_terminal() {
    let fixture = fixture();
    let org_a = ctx("org-a", "user-1");
    accepted(&fixture.jobs, &org_a, "job-1").await;
    let claim = fixture
        .jobs
        .claim(&org_a, JobId::new("job-1"), WorkerId::new("w-1"))
        .await
        .expect("claim");

    // 版本落后 → CasConflict，不做覆盖写。
    let stale = fixture
        .jobs
        .compare_and_set_state(&org_a, &claim, JobStateVersion::new(1), JobState::Succeeded)
        .await;
    assert!(matches!(
        stale,
        Err(PortError::CasConflict { entity: "job", .. })
    ));

    // 正确版本推进到终态 → claim 清空，后续写入按 StaleClaim 拒绝。
    let finished = fixture
        .jobs
        .compare_and_set_state(&org_a, &claim, JobStateVersion::new(2), JobState::Succeeded)
        .await
        .expect("terminal CAS");
    assert_eq!(finished.view.state, JobState::Succeeded);
    assert_eq!(finished.state_version, JobStateVersion::new(3));

    let stage = StageRecord {
        job_id: JobId::new("job-1"),
        stage_id: StageId::new("stage-1"),
        kind: "sql".into(),
        claimed_by: Some(WorkerId::new("w-1")),
        execution_ids: vec![],
        started_at: None,
        finished_at: None,
    };
    let outcome = fixture.jobs.record_stage(&org_a, &claim, stage).await;
    assert!(
        matches!(outcome, Err(PortError::StaleClaim)),
        "终态后 claim 已清空"
    );
}

#[tokio::test]
async fn cross_organization_write_with_a_stolen_claim_is_stale_not_missing() {
    let fixture = fixture();
    let org_a = ctx("org-a", "user-1");
    accepted(&fixture.jobs, &org_a, "job-1").await;
    let claim = fixture
        .jobs
        .claim(&org_a, JobId::new("job-1"), WorkerId::new("w-1"))
        .await
        .expect("claim");

    // 别组织的 ctx 拿着（泄露的）claim 写 → 按 StaleClaim 拒绝（写入路径
    // 兜底语义，端口契约；不是 NotFound）。
    let org_b = ctx("org-b", "attacker");
    let stage = StageRecord {
        job_id: JobId::new("job-1"),
        stage_id: StageId::new("stage-1"),
        kind: "sql".into(),
        claimed_by: Some(WorkerId::new("w-1")),
        execution_ids: vec![],
        started_at: None,
        finished_at: None,
    };
    let outcome = fixture.jobs.record_stage(&org_b, &claim, stage).await;
    assert!(matches!(outcome, Err(PortError::StaleClaim)));
}

#[tokio::test]
async fn checkpoints_conflict_on_repeated_versions() {
    let fixture = fixture();
    let org_a = ctx("org-a", "user-1");
    accepted(&fixture.jobs, &org_a, "job-1").await;
    let claim = fixture
        .jobs
        .claim(&org_a, JobId::new("job-1"), WorkerId::new("w-1"))
        .await
        .expect("claim");

    let checkpoint = |version: u64| Checkpoint {
        job_id: JobId::new("job-1"),
        state_version: JobStateVersion::new(version),
        stable_target_fingerprint: "sha256:abc".into(),
        committed: vec![CommitBoundary {
            stage_id: StageId::new("stage-1"),
            stable_target_fingerprint: "sha256:abc".into(),
            committed_at: Timestamp::new("2026-01-01T00:00:01Z"),
            operation_id: None,
            batch_id: Some("batch-1".into()),
            payload_digest: Some("sha256:def".into()),
            evidence: vec!["target-ack".into()],
            verified_at: None,
        }],
        verification_evidence: vec!["pk-verified".into()],
        recovery_policy: "reauthorize-then-rewrite".into(),
    };

    fixture
        .jobs
        .save_checkpoint(&org_a, &claim, checkpoint(1))
        .await
        .expect("first checkpoint");
    let outcome = fixture
        .jobs
        .save_checkpoint(&org_a, &claim, checkpoint(1))
        .await;
    assert!(matches!(
        outcome,
        Err(PortError::CasConflict {
            entity: "job_checkpoint",
            ..
        })
    ));
}

#[tokio::test]
async fn commit_boundaries_are_durable_in_submission_order() {
    let store = migrated_store();
    let clock = SharedClock::at("2026-01-01T00:00:00Z");
    let jobs = ServerJobs::new(store.clone(), Arc::new(clock.clone()), CLAIM_TTL_SECS);
    let org_a = ctx("org-a", "user-1");
    jobs.accept(
        &org_a,
        definition("job-1", prepare_payload()),
        &IdempotencyKey::new("idem-1"),
    )
    .await
    .expect("accept");
    let claim = jobs
        .claim(&org_a, JobId::new("job-1"), WorkerId::new("w-1"))
        .await
        .expect("claim");

    let boundary = |digest: &str| CommitBoundary {
        stage_id: StageId::new("stage-1"),
        stable_target_fingerprint: "sha256:abc".into(),
        committed_at: clock.now(),
        operation_id: None,
        batch_id: Some(digest.to_string()),
        payload_digest: None,
        evidence: vec![],
        verified_at: None,
    };
    jobs.record_commit_boundary(&org_a, &claim, boundary("batch-1"))
        .await
        .expect("first boundary");
    jobs.record_commit_boundary(&org_a, &claim, boundary("batch-2"))
        .await
        .expect("second boundary");

    // 端口没有边界读取面（内存实现的 committed_boundaries 是 inherent 方法）；
    // durable 事实经管理库直查核验。
    let conn = store.lock_for_adapter().expect("lock");
    let digests: Vec<String> = {
        let mut stmt = conn
            .prepare(
                "SELECT boundary_json FROM job_commit_boundaries
                 WHERE organization_id = 'org-a' AND job_id = 'job-1' ORDER BY boundary_index",
            )
            .expect("prepare");
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .expect("query");
        rows.map(|row| row.expect("row"))
            .map(|json| {
                serde_json::from_str::<CommitBoundary>(&json)
                    .expect("boundary json")
                    .batch_id
                    .expect("batch id")
            })
            .collect()
    };
    assert_eq!(digests, vec!["batch-1".to_string(), "batch-2".to_string()]);
}

#[tokio::test]
async fn recoverable_scan_lists_pending_states_only() {
    let fixture = fixture();
    let org_a = ctx("org-a", "user-1");
    accepted(&fixture.jobs, &org_a, "job-queued").await;
    accepted(&fixture.jobs, &org_a, "job-running").await;
    accepted(&fixture.jobs, &org_a, "job-done").await;

    let org_b = ctx("org-b", "user-2");
    accepted(&fixture.jobs, &org_b, "job-b1").await;

    // running 需要先 claim（终态除外，claim 即 running）。
    let claim = fixture
        .jobs
        .claim(&org_a, JobId::new("job-running"), WorkerId::new("w-1"))
        .await
        .expect("claim");
    let _ = claim;
    // 终态 job 退出候选。
    let done_claim = fixture
        .jobs
        .claim(&org_a, JobId::new("job-done"), WorkerId::new("w-1"))
        .await
        .expect("claim done");
    fixture
        .jobs
        .compare_and_set_state(
            &org_a,
            &done_claim,
            JobStateVersion::new(2),
            JobState::Succeeded,
        )
        .await
        .expect("finish");

    let filter = RecoveryFilter {
        kinds: vec![],
        older_than: None,
        limit: None,
    };
    let recoverable = fixture
        .jobs
        .list_recoverable(&org_a, filter.clone())
        .await
        .expect("scan");
    let ids: Vec<String> = recoverable
        .iter()
        .map(|record| record.view.job_id.as_str().to_string())
        .collect();
    assert_eq!(
        ids,
        vec!["job-queued".to_string(), "job-running".to_string()]
    );

    // 别组织的待办不出现在本组织的扫描里。
    let other = fixture
        .jobs
        .list_recoverable(&org_b, filter)
        .await
        .expect("scan b");
    assert_eq!(other.len(), 1);
    assert_eq!(other[0].view.job_id.as_str(), "job-b1");
}

#[tokio::test]
async fn renew_extends_the_lease_without_bumping_the_generation() {
    let fixture = fixture();
    let org_a = ctx("org-a", "user-1");
    accepted(&fixture.jobs, &org_a, "job-1").await;
    let claim = fixture
        .jobs
        .claim(&org_a, JobId::new("job-1"), WorkerId::new("w-1"))
        .await
        .expect("claim");

    fixture.clock.set("2026-01-01T00:00:30Z");
    let renewed = fixture.jobs.renew(&claim).await.expect("renew");
    assert_eq!(
        renewed.claim_generation, claim.claim_generation,
        "续约不变 generation"
    );
    assert_eq!(renewed.expires_at.as_str(), "2026-01-01T00:01:30Z");
    assert_eq!(renewed.claimed_at.as_str(), "2026-01-01T00:00:30Z");

    // 别的 worker 的续约请求 → StaleClaim。
    let intruder = JobClaim {
        worker_id: WorkerId::new("w-2"),
        ..claim.clone()
    };
    let outcome = fixture.jobs.renew(&intruder).await;
    assert!(matches!(outcome, Err(PortError::StaleClaim)));
}
