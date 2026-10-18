use super::*;
use crate::job::{InMemoryJobRepository, JobClock, SharedClock};
use datazen_platform_api::{
    dto::job::{JobDefinition, JobState, StageRecord},
    id::*,
    ports::job::JobRepository,
};
#[tokio::test]
async fn permit_admission_is_read_only_and_bound_to_persisted_claim_stage_client_and_blocks() {
    let time = Arc::new(SharedClock::at("2026-01-01T00:00:00Z"));
    let repo = Arc::new(InMemoryJobRepository::new(time.clone(), 30));
    let ctx = RequestContext::new(
        OrganizationId::new("org"),
        PrincipalId::new("user"),
        None,
        ClientInstanceId::new("client"),
        RequestId::new("request"),
        None,
    );
    let id = JobId::new("workflow");
    let stage = StageId::new("workflow");
    repo.accept(&ctx,JobDefinition { job_id:id.clone(),kind:"workflowRun".into(),owner:OwnerRef::Job {job_id:id.clone(),stage_id:stage.clone()},payload:serde_json::json!({"planVersion":1,"handlerVersion":2,"checkpointVersion":1,"workflowOwnerBlocks":["accepted"]}),created_at:time.now() },&IdempotencyKey::new("idem")).await.unwrap();
    let claim = repo
        .claim(&ctx, id.clone(), WorkerId::new("worker"))
        .await
        .unwrap();
    repo.record_stage(
        &ctx,
        &claim,
        StageRecord {
            job_id: id.clone(),
            stage_id: stage.clone(),
            kind: "workflowRun".into(),
            claimed_by: Some(claim.worker_id.clone()),
            execution_ids: vec![],
            started_at: Some(time.now()),
            finished_at: None,
        },
    )
    .await
    .unwrap();
    let permit = StageOwnerPermit::new(repo.clone(), ctx.clone(), claim.clone(), stage.clone());
    let owner = OwnerRef::WorkflowBlock {
        job_id: id.clone(),
        block_id: BlockId::new("accepted"),
    };
    let before = repo.get(&ctx, id.clone()).await.unwrap();
    permit.admit_owner(&ctx, &owner).await.unwrap();
    permit.verify_active().await.unwrap();
    assert_eq!(
        before,
        repo.get(&ctx, id.clone()).await.unwrap(),
        "admission must not mutate or renew"
    );
    for owner in [
        OwnerRef::WorkflowBlock {
            job_id: id.clone(),
            block_id: BlockId::new("forged"),
        },
        OwnerRef::WorkflowBlock {
            job_id: JobId::new("other"),
            block_id: BlockId::new("accepted"),
        },
        OwnerRef::Job {
            job_id: id.clone(),
            stage_id: StageId::new("other"),
        },
    ] {
        assert!(permit.admit_owner(&ctx, &owner).await.is_err());
    }
    let mut foreign = ctx.clone();
    foreign.client_instance_id = ClientInstanceId::new("foreign");
    assert!(permit.admit_owner(&foreign, &owner).await.is_err());
    time.set(claim.expires_at.as_str());
    assert!(permit.verify_active().await.is_err());
    time.set("2026-01-01T00:00:31Z");
    let next = repo
        .claim(&ctx, id.clone(), WorkerId::new("worker"))
        .await
        .unwrap();
    assert!(permit.verify_active().await.is_err());
    repo.record_stage(
        &ctx,
        &next,
        StageRecord {
            job_id: id.clone(),
            stage_id: StageId::new("next-stage"),
            kind: "query".into(),
            claimed_by: Some(next.worker_id.clone()),
            execution_ids: vec![],
            started_at: Some(time.now()),
            finished_at: None,
        },
    )
    .await
    .unwrap();
    let wrong_stage = StageOwnerPermit::new(repo.clone(), ctx.clone(), next.clone(), stage);
    assert!(wrong_stage.admit_owner(&ctx, &owner).await.is_err());
    let record = repo.get(&ctx, id).await.unwrap();
    repo.compare_and_set_state(&ctx, &next, record.state_version, JobState::Succeeded)
        .await
        .unwrap();
    assert!(wrong_stage.verify_active().await.is_err());
}

#[test]
fn workflow_v2_requires_frozen_block_whitelist_and_other_handler_versions_stay_closed() {
    let mut payload = serde_json::json!({"planVersion":1,"handlerVersion":2,"checkpointVersion":1});
    assert!(crate::job::project_frozen_plan("workflowRun", &payload).is_err());
    payload["workflowOwnerBlocks"] = serde_json::json!(["accepted"]);
    assert_eq!(
        crate::job::project_frozen_plan("workflowRun", &payload)
            .unwrap()
            .workflow_owner_blocks,
        Some(vec!["accepted".into()])
    );
    assert!(crate::job::project_frozen_plan("schemaDiffPrepare", &payload).is_err());
    payload["workflowOwnerBlocks"] = serde_json::json!(["same", "same"]);
    assert!(crate::job::project_frozen_plan("workflowRun", &payload).is_err());
    for (field, version) in [("planVersion", 2), ("checkpointVersion", 2)] {
        let mut unsupported = serde_json::json!({"planVersion":1,"handlerVersion":2,"checkpointVersion":1,"workflowOwnerBlocks":["accepted"]});
        unsupported[field] = serde_json::json!(version);
        assert!(crate::job::project_frozen_plan("workflowRun", &unsupported).is_err());
    }
    payload["workflowOwnerBlocks"] = serde_json::json!([]);
    payload["handlerVersion"] = serde_json::json!(3);
    assert!(crate::job::project_frozen_plan("workflowRun", &payload).is_err());
}
