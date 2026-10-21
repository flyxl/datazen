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

#[tokio::test]
async fn an_already_admitted_job_identity_is_refused_instead_of_overwritten() {
    // 受理一个**已被占用的 jobId** 不再静默覆盖既有行。
    // 覆盖不是无害的：它会把既有行连同它的落库归属一起顶掉。
    let repo = repository();
    let ctx = organization_context("org-a");
    repo.accept(
        &ctx,
        definition("job-a", "org-a"),
        &IdempotencyKey::new("key-a"),
    )
    .await
    .expect("accept a");

    // 另一个幂等键 + 同一个 jobId → 拒绝，不得覆盖。
    let error = repo
        .accept(
            &ctx,
            definition("job-a", "org-a"),
            &IdempotencyKey::new("key-b"),
        )
        .await
        .expect_err("an occupied job identity must be refused");
    assert!(
        matches!(error, PortError::BackendUnavailable(_)),
        "重复 jobId 按后端不可用拒绝（与 SQLite 主键冲突同口径）：{error:?}"
    );

    // 既有行原封不动：状态、归属与受理时刻都还是第一次那一条。
    let record = repo
        .get(&ctx, JobId::new("job-a"))
        .await
        .expect("read back");
    assert_eq!(record.view.job_id, JobId::new("job-a"));
    assert_eq!(record.view.state, JobState::Queued);
    assert_eq!(
        record.definition.organization_id,
        OrganizationId::new("org-a"),
        "被拒绝的受理不得改动既有行的落库归属"
    );
}

#[tokio::test]
async fn another_organization_cannot_take_over_an_admitted_job_identity() {
    // 跨组织的 jobId 顶替是「覆盖」语义最危险的形态：它同时抹掉受害组织的行。
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
        .accept(
            &org_b,
            definition("job-a", "org-b"),
            &IdempotencyKey::new("key-b"),
        )
        .await
        .expect_err("a foreign organization must not take over an admitted job identity");
    assert!(
        matches!(error, PortError::BackendUnavailable(_)),
        "跨组织顶替按后端不可用拒绝：{error:?}"
    );

    // 受害组织的行仍在，且仍属于它自己。
    let record = repo
        .get(&org_a, JobId::new("job-a"))
        .await
        .expect("read back");
    assert_eq!(
        record.definition.organization_id,
        OrganizationId::new("org-a"),
        "越组织的受理不得顶掉本组织的行"
    );
    // 顶替者自己也读不到受害组织的行。
    let error = repo
        .get(&org_b, JobId::new("job-a"))
        .await
        .expect_err("the takeover attempt must not make the row readable");
    assert!(
        matches!(error, PortError::NotFound(_)),
        "越组织读取按「不存在」处理：{error:?}"
    );
}

#[tokio::test]
async fn a_plan_consumed_in_one_organization_does_not_block_another() {
    // 已消费 planId 按 (组织, planId) 唯一，与 SQLite 的
    // `UNIQUE (organization_id, consumed_plan_id)` 同口径：
    // 同一个 planId 在另一个组织不是「已被消费」。
    let repo = repository();
    let org_a = organization_context("org-a");
    let org_b = organization_context("org-b");
    let apply = |job_id: &str, organization: &str| JobDefinition {
        job_id: JobId::new(job_id),
        organization_id: OrganizationId::new(organization),
        kind: "schemaDiffApply".into(),
        owner: OwnerRef::ClientSession {
            client_instance_id: ClientInstanceId::new("client-1"),
            purpose: "migration".into(),
        },
        payload: serde_json::json!({
            "consumedPlanId": "plan-shared",
            "planVersion": 1,
            "handlerVersion": 1,
            "checkpointVersion": 1,
        }),
        created_at: Timestamp::new("2026-01-01T00:00:00Z"),
    };

    repo.accept(
        &org_a,
        apply("job-a", "org-a"),
        &IdempotencyKey::new("key-a"),
    )
    .await
    .expect("accept a");

    // 另一个组织用同一个 planId 受理：不得被「已消费」挡住。
    let second = repo
        .accept(
            &org_b,
            apply("job-b", "org-b"),
            &IdempotencyKey::new("key-b"),
        )
        .await
        .expect("accept b");
    assert_eq!(
        second.definition.organization_id,
        OrganizationId::new("org-b"),
        "第二个受理落库在它自己的组织里"
    );

    // 但同一个组织内仍然只能消费一次。
    let error = repo
        .accept(
            &org_a,
            apply("job-c", "org-a"),
            &IdempotencyKey::new("key-c"),
        )
        .await
        .expect_err("the plan must stay consumed inside its organization");
    assert!(
        matches!(error, PortError::PlanAlreadyConsumed(_)),
        "同组织内 planId 仍只消费一次：{error:?}"
    );
}
