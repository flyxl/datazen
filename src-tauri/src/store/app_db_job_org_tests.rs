//! 桌面 SQLite 仓储的**组织隔离**契约测试。
//!
//! 桌面是单租户（当前所有请求上下文都来自同一个本地组织），因此这些用例**不改任何
//! 桌面行为**：它们只证明落库行带组织维度，且跨组织的列举/读取/恢复按「不存在」处理，
//! 不返回「无权限」，也不泄漏「存在但无权」（CM-05 口径，与 `ProfileRepository` 一致）。
//!
//! **写路径**（认领与阶段写入）不在上面的「读路径」口径里：认领按「不存在」处理，
//! 而持有别组织铸出的 claim 去写入则由 claim fencing 按 `StaleClaim` 拒绝。这两个形状
//! 必须与内存实现（`packages/runtime/src/job/repository/org_scope_tests.rs`）逐条一致，
//! 因此这里给 SQLite 侧补上同名同形状的用例。
use super::*;
use datazen_platform_api::dto::job::StageRecord;

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

/// 跨组织的**认领**按「不存在」处理。
///
/// 与内存实现的 `a_cross_organization_claim_is_rejected` 同名同形状：两个实现对同一
/// 场景必须给出同一种错误，否则调用方按错误形状分支时会在一侧走错。
#[tokio::test]
async fn a_cross_organization_claim_is_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let repo = repository(temp.path(), clock(), 30);
    let org_a = organization_context("org-a");
    let org_b = organization_context("org-b");
    repo.accept(
        &org_a,
        definition("job-org-claim", "plan-org-claim"),
        &idempotency_key("org-claim"),
    )
    .await
    .unwrap();

    // 主体相同、只有组织不同：因此「认领不到」只能由组织隔离解释。
    let error = repo
        .claim(
            &org_b,
            JobId::new("job-org-claim"),
            WorkerId::new("worker-1"),
        )
        .await
        .expect_err("cross-organization claim must fail");
    assert!(
        matches!(error, PortError::NotFound(_)),
        "跨组织认领按「不存在」处理，而不是「无权限」：{error:?}"
    );
}

/// 跨组织的**写入**由 claim fencing 兜底：按 `StaleClaim` 拒绝，**不是** `NotFound`。
///
/// 与内存实现的 `a_cross_organization_write_under_a_foreign_claim_is_rejected` 同名同形状。
/// 这里特意用「别的组织铸出的 claim」去写本组织的行——claim 本身不带组织维度，
/// 所以拒绝它的只能是写入路径上的组织比对（`check_claim`）。
#[tokio::test]
async fn a_cross_organization_write_under_a_foreign_claim_is_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let repo = repository(temp.path(), clock(), 30);
    let org_a = organization_context("org-a");
    let org_b = organization_context("org-b");
    repo.accept(
        &org_a,
        definition("job-org-write", "plan-org-write"),
        &idempotency_key("org-write"),
    )
    .await
    .unwrap();
    let claim = repo
        .claim(
            &org_a,
            JobId::new("job-org-write"),
            WorkerId::new("worker-1"),
        )
        .await
        .expect("claim in the owning organization");

    let error = repo
        .record_stage(
            &org_b,
            &claim,
            StageRecord {
                job_id: JobId::new("job-org-write"),
                stage_id: StageId::new("s1"),
                kind: "apply".into(),
                claimed_by: Some(WorkerId::new("worker-1")),
                execution_ids: Vec::new(),
                started_at: None,
                finished_at: None,
            },
        )
        .await
        .expect_err("cross-organization write must fail");
    assert!(
        matches!(error, PortError::StaleClaim),
        "越组织的写入按旧 claim 拒绝（不是 NotFound）：{error:?}"
    );
}

/// **续约路径本身没有组织维度**（SQLite 侧）。
///
/// 这是内存侧 `the_renewal_path_carries_no_organization_dimension` 的 SQLite 对照：
/// 端口契约里「越组织的续约只能按 `StaleClaim` 失败」是**推断**，不是代码强制的不变量。
///
/// 事实（实测，不是推断）：
///
/// * `renew` 的签名里没有 `RequestContext`，实现里也**没有任何组织比对**
///   （下面的 SQL 只按 `job_id` 定位行：`WHERE job_id=?1`）；
/// * `JobClaim` **不带组织字段**，所以「一次越组织的续约」在 API 上根本无法表达——
///   持有同一个 claim 的调用方与铸出它的调用方，在续约路径上**不可区分**；
/// * 因此续约唯一的拒绝理由是：行不存在（`NotFound`）、行上没有 claim、
///   worker 不匹配、generation 不匹配、租约已过期（都是 `StaleClaim`）。
///
/// 组织边界由**写入路径**兜底：所有写入都经 `check_claim`，那里比对 `ctx` 的组织，
/// 越组织写按 `StaleClaim` 拒绝（见上面那条用例）。
///
/// 这条用例把这些**事实**钉住：将来若给 claim 加上组织维度、或让续约自己比组织，
/// 这条用例会先变红，提示同步更新端口契约与文档里的因果链。
#[tokio::test]
async fn the_renewal_path_carries_no_organization_dimension() {
    let temp = tempfile::tempdir().unwrap();
    let repo = repository(temp.path(), clock(), 30);
    let org_a = organization_context("org-a");
    repo.accept(
        &org_a,
        definition("job-org-renew", "plan-org-renew"),
        &idempotency_key("org-renew"),
    )
    .await
    .unwrap();
    let claim = repo
        .claim(
            &org_a,
            JobId::new("job-org-renew"),
            WorkerId::new("worker-1"),
        )
        .await
        .expect("claim in the owning organization");

    // claim 不带组织：这份「从别处拿到的」续约凭证与铸出它的那份在续约路径上无法区分。
    let carried_elsewhere = datazen_platform_api::dto::job::JobClaim {
        job_id: claim.job_id.clone(),
        stage_id: claim.stage_id.clone(),
        worker_id: claim.worker_id.clone(),
        claimed_at: claim.claimed_at.clone(),
        claim_generation: claim.claim_generation,
        expires_at: claim.expires_at.clone(),
    };
    let renewed = repo
        .renew(&carried_elsewhere)
        .await
        .expect("续约只凭 claim 本身：worker 与 generation 对得上就成立");
    assert_eq!(renewed.job_id, claim.job_id);
    assert_eq!(renewed.claim_generation, claim.claim_generation);

    // 真正被拒的是**对不上**的 claim（这才是续约唯一的拒绝理由）。
    let wrong_worker = datazen_platform_api::dto::job::JobClaim {
        worker_id: WorkerId::new("not-the-owner"),
        ..carried_elsewhere
    };
    assert!(
        matches!(repo.renew(&wrong_worker).await, Err(PortError::StaleClaim)),
        "worker 对不上的续约按旧 claim 拒绝"
    );
}
