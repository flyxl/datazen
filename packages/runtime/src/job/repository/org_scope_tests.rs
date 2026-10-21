//! Job 持久化词汇的**组织隔离**契约测试。
//!
//! 落盘词汇（`JobDefinition` / `JobFilter`）带组织维度之后，仓储必须兑现三条：
//!
//! * 受理把 `RequestContext` 的组织盖章到落库的 `JobDefinition`（调用方自报不作数）；
//! * 列举 / 查询 / 恢复按组织过滤，跨组织一律按「**不存在**」处理（CM-05 口径），
//!   不返回「无权限」之类的错误，也不泄漏「存在但无权」；
//! * 幂等回执按 `(组织, 主体, 键)` 作用域：同一个键在另一个组织不是重放。
use super::*;

use datazen_platform_api::dto::job::{JobFilter, JobState};
use datazen_platform_api::id::{
    ClientInstanceId, IdempotencyKey, OrganizationId, PrincipalId, RequestId,
};
use datazen_platform_api::OwnerRef;

use crate::job::time::SharedClock;

fn organization_context(organization: &str) -> RequestContext {
    RequestContext::new(
        OrganizationId::new(organization),
        PrincipalId::new("user-a"),
        None,
        ClientInstanceId::new("client-1"),
        RequestId::new("req-1"),
        None,
    )
}

fn definition(job_id: &str, organization: &str) -> JobDefinition {
    JobDefinition {
        job_id: JobId::new(job_id),
        organization_id: OrganizationId::new(organization),
        kind: "schemaDiffPrepare".into(),
        owner: OwnerRef::ClientSession {
            client_instance_id: ClientInstanceId::new("client-1"),
            purpose: "migration".into(),
        },
        payload: serde_json::json!({
            "planVersion": 1,
            "handlerVersion": 1,
            "checkpointVersion": 1,
        }),
        created_at: Timestamp::new("2026-01-01T00:00:00Z"),
    }
}

fn filter(organization: &str) -> JobFilter {
    JobFilter {
        organization_id: OrganizationId::new(organization),
        states: Vec::new(),
        owner: None,
        after: None,
        limit: None,
    }
}

fn repository() -> InMemoryJobRepository {
    let clock = std::sync::Arc::new(SharedClock::at("2026-01-01T00:00:00Z"));
    InMemoryJobRepository::new(clock, 300)
}

#[tokio::test]
async fn accept_stamps_the_context_organization_onto_the_persisted_definition() {
    let repo = repository();
    let ctx = organization_context("org-a");
    // 调用方自报的组织必须被覆盖：落库归属只来自 adapter 构造的上下文（INV-01）。
    let record = repo
        .accept(
            &ctx,
            definition("job-a", "org-b"),
            &IdempotencyKey::new("key-1"),
        )
        .await
        .expect("accept");
    assert_eq!(
        record.definition.organization_id,
        OrganizationId::new("org-a"),
        "落库归属必须是受理上下文的组织"
    );
}

#[tokio::test]
async fn listing_never_crosses_the_organization_boundary() {
    let repo = repository();
    let org_a = organization_context("org-a");
    let org_b = organization_context("org-b");
    repo.accept(
        &org_a,
        definition("job-a", "org-a"),
        &IdempotencyKey::new("key-a"),
    )
    .await
    .expect("accept a");
    repo.accept(
        &org_b,
        definition("job-b", "org-b"),
        &IdempotencyKey::new("key-b"),
    )
    .await
    .expect("accept b");

    let listed = repo.list(&org_a, filter("org-a")).await.expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].view.job_id, JobId::new("job-a"));

    // 过滤器指向别的组织 → 空列表，不是错误，也不泄漏「存在但无权」。
    let crossed = repo.list(&org_a, filter("org-b")).await.expect("list");
    assert!(crossed.is_empty(), "跨组织的列举必须为空：{crossed:?}");
}

#[tokio::test]
async fn a_cross_organization_read_is_indistinguishable_from_absence() {
    let repo = repository();
    let org_a = organization_context("org-a");
    let org_b = organization_context("org-b");
    repo.accept(
        &org_a,
        definition("job-a", "org-a"),
        &IdempotencyKey::new("key-a"),
    )
    .await
    .expect("accept a");

    let error = repo
        .get(&org_b, JobId::new("job-a"))
        .await
        .expect_err("cross-organization read must fail");
    assert!(
        matches!(error, PortError::NotFound(_)),
        "跨组织访问按「不存在」处理，而不是「无权限」：{error:?}"
    );
}

#[tokio::test]
async fn the_recovery_scan_only_sees_the_current_organization() {
    let repo = repository();
    let org_a = organization_context("org-a");
    let org_b = organization_context("org-b");
    repo.accept(
        &org_a,
        definition("job-a", "org-a"),
        &IdempotencyKey::new("key-a"),
    )
    .await
    .expect("accept a");
    repo.accept(
        &org_b,
        definition("job-b", "org-b"),
        &IdempotencyKey::new("key-b"),
    )
    .await
    .expect("accept b");

    let recoverable = repo
        .list_recoverable(&org_b, RecoveryFilter::default())
        .await
        .expect("recoverable");
    assert!(
        recoverable
            .iter()
            .all(|record| record.definition.organization_id == OrganizationId::new("org-b")),
        "恢复扫描不得产出别的组织的待办：{recoverable:?}"
    );
    assert_eq!(recoverable.len(), 1);
    assert_eq!(recoverable[0].view.job_id, JobId::new("job-b"));
}

#[tokio::test]
async fn the_same_idempotency_key_in_another_organization_is_not_a_replay() {
    let repo = repository();
    let org_a = organization_context("org-a");
    let org_b = organization_context("org-b");
    let first = repo
        .accept(
            &org_a,
            definition("job-a", "org-a"),
            &IdempotencyKey::new("shared-key"),
        )
        .await
        .expect("accept a");
    let second = repo
        .accept(
            &org_b,
            definition("job-b", "org-b"),
            &IdempotencyKey::new("shared-key"),
        )
        .await
        .expect("accept b");
    assert_ne!(
        first.view.job_id, second.view.job_id,
        "幂等只在同一组织内成立：跨组织复用同一个键不是重放"
    );
    assert_eq!(
        second.definition.organization_id,
        OrganizationId::new("org-b"),
        "第二个受理落库在它自己的组织里"
    );
}

#[tokio::test]
async fn a_cross_organization_claim_is_rejected() {
    let repo = repository();
    let org_a = organization_context("org-a");
    let org_b = organization_context("org-b");
    repo.accept(
        &org_a,
        definition("job-a", "org-a"),
        &IdempotencyKey::new("key-a"),
    )
    .await
    .expect("accept a");

    let error = repo
        .claim(&org_b, JobId::new("job-a"), WorkerId::new("w-1"))
        .await
        .expect_err("cross-organization claim must fail");
    assert!(
        matches!(error, PortError::NotFound(_)),
        "跨组织认领按「不存在」处理：{error:?}"
    );
}

#[tokio::test]
async fn a_cross_organization_write_under_a_foreign_claim_is_rejected() {
    let repo = repository();
    let org_a = organization_context("org-a");
    let org_b = organization_context("org-b");
    repo.accept(
        &org_a,
        definition("job-a", "org-a"),
        &IdempotencyKey::new("key-a"),
    )
    .await
    .expect("accept a");
    let claim = repo
        .claim(&org_a, JobId::new("job-a"), WorkerId::new("w-1"))
        .await
        .expect("claim in the owning organization");

    let error = repo
        .record_stage(
            &org_b,
            &claim,
            StageRecord {
                job_id: JobId::new("job-a"),
                stage_id: StageId::new("s1"),
                kind: "apply".into(),
                claimed_by: Some(WorkerId::new("w-1")),
                execution_ids: Vec::new(),
                started_at: None,
                finished_at: None,
            },
        )
        .await
        .expect_err("cross-organization write must fail");
    assert!(
        matches!(error, PortError::StaleClaim),
        "越组织的写入按旧 claim 拒绝：{error:?}"
    );
}

#[tokio::test]
async fn the_state_of_a_job_is_isolated_from_another_organization() {
    let repo = repository();
    let org_a = organization_context("org-a");
    let org_b = organization_context("org-b");
    repo.accept(
        &org_a,
        definition("job-a", "org-a"),
        &IdempotencyKey::new("key-a"),
    )
    .await
    .expect("accept a");

    // 取消意图也按组织过滤：别组织的取消不得改到本组织的行上。
    let error = repo
        .request_cancel(&org_b, &JobId::new("job-a"))
        .expect_err("cross-organization cancel must fail");
    assert!(matches!(error, PortError::NotFound(_)), "{error:?}");
    let record = repo
        .get(&org_a, JobId::new("job-a"))
        .await
        .expect("read back");
    assert_eq!(
        record.view.state,
        JobState::Queued,
        "越组织的取消不得改动本组织的 job"
    );
    assert!(!record.view.cancel_requested);
}
