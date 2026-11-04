//! Persistent claim authority is separate from Job read visibility.
use super::*;
use datazen::jobs::domain::job::StageRecord;
use datazen::jobs::JobRuntimeRepository;

fn stage(claim: &datazen::jobs::domain::job::JobClaim, id: &str) -> StageRecord {
    StageRecord {
        job_id: claim.job_id.clone(),
        stage_id: StageId::new(id),
        kind: "query".into(),
        claimed_by: Some(claim.worker_id.clone()),
        execution_ids: vec![],
        started_at: Some(claim.claimed_at.clone()),
        finished_at: None,
    }
}

#[tokio::test]
async fn persistent_owner_check_is_read_only_and_rejects_every_stale_identity_dimension() {
    let temp = tempfile::tempdir().unwrap();
    let time = clock();
    let repo = repository(temp.path(), time.clone(), 30);
    let ctx = request_context();
    let id = JobId::new("job-owner");
    repo.accept(
        &ctx,
        definition(id.as_str(), "plan-owner"),
        &idempotency_key("owner"),
    )
    .await
    .unwrap();
    let claim = repo
        .claim(&ctx, id.clone(), WorkerId::new("worker"))
        .await
        .unwrap();
    let stage_id = StageId::new("stage");
    assert!(repo
        .validate_stage_owner(&ctx, &claim, &stage_id)
        .await
        .is_err());
    repo.record_stage(&ctx, &claim, stage(&claim, "stage"))
        .await
        .unwrap();
    let before = repo.get(&ctx, id.clone()).await.unwrap();
    for _ in 0..2 {
        repo.validate_stage_owner(&ctx, &claim, &stage_id)
            .await
            .unwrap();
    }
    assert_eq!(before, repo.get(&ctx, id.clone()).await.unwrap());
    let expiry: String = repo
        .db
        .with_conn(|c| {
            Ok(c.query_row(
                "SELECT claim_expires_at FROM jobs WHERE job_id='job-owner'",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(
        expiry,
        claim.expires_at.as_str(),
        "admission must not renew"
    );
    for dimension in ["client", "principal", "organization"] {
        let mut other = ctx.clone();
        match dimension {
            "client" => other.client_instance_id = ClientInstanceId::new("other"),
            "principal" => other.principal_id = PrincipalId::new("other"),
            _ => other.organization_id = OrganizationId::new("other"),
        }
        assert!(repo
            .validate_stage_owner(&other, &claim, &stage_id)
            .await
            .is_err());
    }
    assert!(repo
        .validate_stage_owner(&ctx, &claim, &StageId::new("other-stage"))
        .await
        .is_err());
    let mut forged = claim.clone();
    forged.job_id = JobId::new("missing");
    assert!(repo
        .validate_stage_owner(&ctx, &forged, &stage_id)
        .await
        .is_err());
    forged = claim.clone();
    forged.worker_id = WorkerId::new("forged");
    assert!(repo
        .validate_stage_owner(&ctx, &forged, &stage_id)
        .await
        .is_err());
    forged = claim.clone();
    forged.claim_generation = Counter::new(claim.claim_generation.get() + 1);
    assert!(repo
        .validate_stage_owner(&ctx, &forged, &stage_id)
        .await
        .is_err());
    forged = claim.clone();
    forged.expires_at = Timestamp::new("2099-01-01T00:00:00Z");
    assert!(repo
        .validate_stage_owner(&ctx, &forged, &stage_id)
        .await
        .is_err());
    time.set("2026-01-01T00:00:01Z");
    let renewed = repo.renew(&claim).await.unwrap();
    assert!(
        repo.validate_stage_owner(&ctx, &claim, &stage_id)
            .await
            .is_err(),
        "renewal must invalidate old expiry"
    );
    repo.validate_stage_owner(&ctx, &renewed, &stage_id)
        .await
        .unwrap();
    let claim = renewed;
    time.set(claim.expires_at.as_str());
    assert!(repo
        .validate_stage_owner(&ctx, &claim, &stage_id)
        .await
        .is_err());
    let next = repo
        .claim(&ctx, id.clone(), WorkerId::new("worker"))
        .await
        .unwrap();
    assert!(repo
        .validate_stage_owner(&ctx, &claim, &stage_id)
        .await
        .is_err());
    repo.record_stage(&ctx, &next, stage(&next, "stage"))
        .await
        .unwrap();
    repo.validate_stage_owner(&ctx, &next, &stage_id)
        .await
        .unwrap();
    let mut ended = stage(&next, "stage");
    ended.finished_at = Some(time.now());
    repo.record_stage(&ctx, &next, ended).await.unwrap();
    assert!(repo
        .validate_stage_owner(&ctx, &next, &stage_id)
        .await
        .is_err());
    repo.record_stage(&ctx, &next, stage(&next, "stage"))
        .await
        .unwrap();
    let record = repo.get(&ctx, id).await.unwrap();
    repo.compare_and_set_state(&ctx, &next, record.state_version, JobState::Succeeded)
        .await
        .unwrap();
    assert!(repo
        .validate_stage_owner(&ctx, &next, &stage_id)
        .await
        .is_err());
}

#[tokio::test]
async fn v3_running_claims_migrate_with_null_instance_and_remain_unusable() {
    let temp = tempfile::tempdir().unwrap();
    let time = clock();
    let ctx = request_context();
    let repo = repository(temp.path(), time.clone(), 30);
    let id = JobId::new("job-legacy-owner");
    repo.accept(
        &ctx,
        definition(id.as_str(), "plan-legacy-owner"),
        &idempotency_key("legacy-owner"),
    )
    .await
    .unwrap();
    let claim = repo
        .claim(&ctx, id.clone(), WorkerId::new("worker"))
        .await
        .unwrap();
    repo.record_stage(&ctx, &claim, stage(&claim, "stage"))
        .await
        .unwrap();
    // 回退到 **v3**：v4 加了 `claim_client_instance_id` 列，v5 之后还有别的迁移，
    // 因此这里按「≥4 全删」回退，而不是只删 v4 那一条（否则 MAX(version) 仍 ≥4，
    // v4 迁移不会重跑，列就再也加不回来）。
    repo.db.with_conn(|c| Ok(c.execute_batch("ALTER TABLE jobs DROP COLUMN claim_client_instance_id; DELETE FROM schema_migrations WHERE version>=4;")?)).unwrap();
    drop(repo);
    let reopened = repository(temp.path(), time, 30);
    let client: Option<String> = reopened
        .db
        .with_conn(|c| {
            Ok(c.query_row(
                "SELECT claim_client_instance_id FROM jobs WHERE job_id='job-legacy-owner'",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert!(client.is_none());
    assert_eq!(
        reopened.get(&ctx, id).await.unwrap().view.state,
        JobState::Running
    );
    assert!(reopened
        .validate_stage_owner(&ctx, &claim, &StageId::new("stage"))
        .await
        .is_err());
}
