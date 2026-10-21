//! 桌面 SQLite 仓储的**组织隔离**契约测试。
//!
//! 桌���是单租户（当前所有请求上下文都来自同一个本地组织），因此这些用例**不改任何
//! 桌面行为**：它们只证明落库行带组织维度，且跨组织的列举/读取/恢复按「不存在」处理，
//! 不返回「无权限」，也不泄漏「存在但无权」（CM-05 口径，与 `ProfileRepository` 一致）。
use super::*;

fn organization_context(organization: &str) -> RequestContext {
    let mut ctx = request_context();
    ctx.organization_id = OrganizationId::new(organization);
    ctx
}

fn organization_filter(organization: &str) -> datazen_platform_api::dto::job::JobFilter {
    datazen_platform_api::dto::job::JobFilter {
        organization_id: OrganizationId::new(organization),
        states: Vec::new(),
        owner: None,
        after: None,
        limit: None,
    }
}

#[tokio::test]
async fn admission_stamps_the_context_organization_onto_the_persisted_row() {
    let temp = tempfile::tempdir().unwrap();
    let repo = repository(temp.path(), clock(), 30);
    let ctx = request_context();
    let id = JobId::new("job-org-stamp");
    // 调用方自报的组织必须被覆盖：落库归属只来自请求上下文（INV-01）。
    let mut forged = definition(id.as_str(), "plan-org-stamp");
    forged.organization_id = OrganizationId::new("forged-org");
    repo.accept(&ctx, forged, &idempotency_key("org-stamp"))
        .await
        .unwrap();
    let stored: String = repo
        .db
        .with_conn(|c| {
            Ok(c.query_row(
                "SELECT organization_id FROM jobs WHERE job_id='job-org-stamp'",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(
        stored,
        ctx.organization_id.as_str(),
        "落库行的组织必须是受理上下文的组织"
    );
    let record = repo.get(&ctx, id).await.unwrap();
    assert_eq!(record.definition.organization_id, ctx.organization_id);
}

#[tokio::test]
async fn listing_is_scoped_to_the_organization() {
    let temp = tempfile::tempdir().unwrap();
    let repo = repository(temp.path(), clock(), 30);
    let org_a = organization_context("org-a");
    let org_b = organization_context("org-b");
    repo.accept(
        &org_a,
        definition("job-org-a", "plan-org-a"),
        &idempotency_key("org-a"),
    )
    .await
    .unwrap();
    repo.accept(
        &org_b,
        definition("job-org-b", "plan-org-b"),
        &idempotency_key("org-b"),
    )
    .await
    .unwrap();

    let listed = repo
        .list(&org_a, organization_filter("org-a"))
        .await
        .unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].view.job_id, JobId::new("job-org-a"));

    // 过滤器指向别的组织 → 空列表，不是错误。
    let crossed = repo
        .list(&org_a, organization_filter("org-b"))
        .await
        .unwrap();
    assert!(crossed.is_empty(), "跨组织的列举必须为空：{crossed:?}");
}

#[tokio::test]
async fn a_cross_organization_read_is_indistinguishable_from_absence() {
    let temp = tempfile::tempdir().unwrap();
    let repo = repository(temp.path(), clock(), 30);
    let ctx = request_context();
    let id = JobId::new("job-org-read");
    repo.accept(
        &ctx,
        definition(id.as_str(), "plan-org-read"),
        &idempotency_key("org-read"),
    )
    .await
    .unwrap();

    // 主体相同、只有组织不同：因此「读不到」只能由组织隔离解释。
    let mut other = request_context();
    other.organization_id = OrganizationId::new("other-org");
    let error = repo
        .get(&other, id.clone())
        .await
        .expect_err("cross-organization read must fail");
    assert!(
        matches!(error, PortError::NotFound(_)),
        "跨组织访问按「不存在」处理，而不是「无权限」：{error:?}"
    );
}

#[tokio::test]
async fn the_recovery_scan_only_sees_the_current_organization() {
    let temp = tempfile::tempdir().unwrap();
    let repo = repository(temp.path(), clock(), 30);
    let org_a = organization_context("org-a");
    let org_b = organization_context("org-b");
    repo.accept(
        &org_a,
        definition("job-org-recover-a", "plan-org-recover-a"),
        &idempotency_key("org-recover-a"),
    )
    .await
    .unwrap();
    repo.accept(
        &org_b,
        definition("job-org-recover-b", "plan-org-recover-b"),
        &idempotency_key("org-recover-b"),
    )
    .await
    .unwrap();

    let recoverable = repo
        .list_recoverable(&org_b, RecoveryFilter::default())
        .await
        .unwrap();
    assert!(
        recoverable
            .iter()
            .all(|record| record.definition.organization_id == OrganizationId::new("org-b")),
        "恢复扫描不得产出别的组织的待办：{recoverable:?}"
    );
    assert_eq!(recoverable.len(), 1);
    assert_eq!(recoverable[0].view.job_id, JobId::new("job-org-recover-b"));
}
