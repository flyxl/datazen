//! WR-D7 幂等提交的行为钉子（`server/tests` 既有样式：直调服务 + 内存/文件
//! `AdminStore` 假体装配，无网络、无路由接线）。
//!
//! 钉住的契约：
//!
//! 1. **令牌重放返回原 receipt**：同令牌 + 同 payload 二次提交 → 同一
//!    `JobRecord`（幂等键 = 令牌本身，指纹 = kind + payload 摘要，语义全部
//!    复用 `ServerJobs::accept`，本层不重造）。
//! 2. **指纹冲突如实上报**：同令牌 + 异 payload → `ApiErrorCode::IdempotencyConflict`。
//! 3. **闸门拒绝**：`PreSubmitGate` 拒绝 → wire 错误原样透传（429 词汇），
//!    且仓储无受理痕迹。
//! 4. **坏令牌拒绝**：签名/形状破坏 → `Unauthenticated`。
//! 5. **全链路审计**：成功与冲突路径都落 `audit_events` 行（文件库直查——
//!    集成测试与生产同一份 rusqlite 依赖，`AdminStore` 无公开行查询面，
//!    直查等价于运维的 `sqlite3` 探查）。

use std::sync::Arc;

use async_trait::async_trait;

use datazen::execution::application::idempotency::IdempotentOperation;
use datazen::execution::application::submission::{
    SubmissionPresentation, SubmissionTokenIssuer,
};
use datazen::execution::application::SubmissionToken;
use datazen::jobs::domain::job::JobDefinition;
use datazen::jobs::domain::repository::JobRepository;
use datazen::jobs::domain::time::JobClock;
use datazen::shared::clock::{Clock, FixedClock};
use datazen::shared::context::{OwnerRef, RequestContext};
use datazen::shared::error::{ApiError, ApiErrorCode, PortError};
use datazen::shared::id::{
    AuthenticationSessionId, ClientInstanceId, IdempotencyKey, JobId, OrganizationId, PrincipalId,
    RequestId, Timestamp,
};

use datazen_server::execution::submission_token::ServerSubmissionTokenIssuer;
use datazen_server::storage::{AdminStore, ServerAudit, ServerJobs};
use datazen_server::transfer::submission::SubmissionService;
use datazen_server::transfer::{AllowAllGate, PreSubmitGate};

/// 测试时钟：`Clock` 与 `JobClock` 同源（FixedClock 驱动，测试可推进）。
#[derive(Debug, Clone)]
struct TestClock(FixedClock);

impl Clock for TestClock {
    fn now(&self) -> Timestamp {
        self.0.now()
    }
}

impl JobClock for TestClock {
    fn now(&self) -> Timestamp {
        self.0.now()
    }
}

/// 文件库路径（审计回读需要第二只手直查；内存库做不到）。
fn admin_path(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "dz-transfer-submit-{tag}-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir.join("admin.sqlite")
}

type Harness = (
    Arc<ServerSubmissionTokenIssuer>,
    SubmissionService,
    Arc<ServerJobs>,
    Arc<ServerAudit>,
    std::path::PathBuf,
);

fn harness(tag: &str, gate: Arc<dyn PreSubmitGate>) -> Harness {
    let path = admin_path(tag);
    let store = Arc::new(AdminStore::open(&path).expect("file-backed admin store"));
    store.migrate().expect("migrations apply");
    let clock = Arc::new(TestClock(FixedClock::at("2026-01-01T00:00:00.000Z")));
    let jobs = Arc::new(ServerJobs::new(store.clone(), clock.clone(), 900));
    let issuer = Arc::new(
        ServerSubmissionTokenIssuer::new(b"wr-d7-submission-test-key".to_vec(), clock.clone())
            .expect("issuer accepts non-empty key material"),
    );
    let audit = Arc::new(ServerAudit::new(store));
    let service = SubmissionService::new(issuer.clone(), jobs.clone(), gate, audit.clone(), clock);
    (issuer, service, jobs, audit, path)
}

/// 直查 `audit_events`（append-only 的读半边；与 `storage::audit` 钉子测试
/// 同一断言面）。用读写连接打开：主连接仍以 WAL 持有文件，只读模式在
/// WAL 下可能拿不到 shm；读写打开与生产 `sqlite3` 探查同口径。
fn audit_actions(path: &std::path::Path) -> Vec<(String, String)> {
    let conn = rusqlite::Connection::open(path).expect("reopen admin store");
    let mut statement = conn
        .prepare("SELECT action, outcome FROM audit_events ORDER BY event_id")
        .expect("audit query prepares");
    let rows = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .expect("query map");
    rows.map(|row| row.expect("row")).collect()
}

fn ctx() -> RequestContext {
    RequestContext::new(
        OrganizationId::new("org-1"),
        PrincipalId::new("user-1"),
        Some(AuthenticationSessionId::new("auth-1")),
        ClientInstanceId::new("client-1"),
        RequestId::new("req-1"),
        None,
    )
}

fn definition(job_id: &str, payload: serde_json::Value) -> JobDefinition {
    JobDefinition {
        job_id: JobId::new(job_id),
        organization_id: OrganizationId::new("org-1"),
        kind: "transfer".into(),
        owner: OwnerRef::ClientSession {
            client_instance_id: ClientInstanceId::new("client-1"),
            purpose: "transfer-test".into(),
        },
        payload,
        created_at: Timestamp::new("2026-01-01T00:00:00.000Z"),
    }
}

fn payload() -> serde_json::Value {
    serde_json::json!({
        "planVersion": 1,
        "handlerVersion": 1,
        "checkpointVersion": 1,
    })
}

async fn presentation(
    issuer: &ServerSubmissionTokenIssuer,
    ctx: &RequestContext,
) -> SubmissionPresentation {
    let token = issuer
        .issue(ctx, IdempotentOperation::StartJob, None)
        .await
        .expect("token issues for a configuration-class operation");
    SubmissionPresentation {
        token,
        claimed_organization_id: ctx.organization_id.clone(),
        claimed_principal_id: ctx.principal_id.clone(),
        claimed_client_instance_id: ctx.client_instance_id.clone(),
        claimed_delegation_id: ctx.delegation_id.clone(),
    }
}

/// 拒绝一切的闸门（wire 词汇 = QuotaExceeded，模拟 WR-D6 额度拒绝形状）。
struct DenyAllGate;

#[async_trait]
impl PreSubmitGate for DenyAllGate {
    async fn check_submit(
        &self,
        _ctx: &RequestContext,
        _submission: &datazen::execution::application::submission::VerifiedSubmission,
    ) -> Result<(), ApiError> {
        Err(ApiError::new(
            ApiErrorCode::QuotaExceeded,
            "Organization job quota exhausted.",
        ))
    }
}

#[tokio::test]
async fn token_replay_returns_the_original_receipt() {
    let (issuer, service, _jobs, _audit, _path) = harness("replay", Arc::new(AllowAllGate));
    let ctx = ctx();
    let presentation = presentation(&issuer, &ctx).await;

    let first = service
        .submit_job(&ctx, &presentation, definition("job-1", payload()))
        .await
        .expect("first admission succeeds");
    let replay = service
        .submit_job(&ctx, &presentation, definition("job-1", payload()))
        .await
        .expect("same token + same payload replays the original receipt");

    assert_eq!(first, replay, "replay must return the original JobRecord");
    assert_eq!(first.definition.job_id.as_str(), "job-1");
}

#[tokio::test]
async fn fingerprint_conflict_is_reported_as_idempotency_conflict() {
    let (issuer, service, _jobs, _audit, _path) = harness("conflict", Arc::new(AllowAllGate));
    let ctx = ctx();
    let presentation = presentation(&issuer, &ctx).await;

    service
        .submit_job(&ctx, &presentation, definition("job-1", payload()))
        .await
        .expect("first admission succeeds");

    let conflicting = definition(
        "job-1",
        serde_json::json!({
            "planVersion": 1,
            "handlerVersion": 1,
            "checkpointVersion": 1,
            "note": "tampered payload",
        }),
    );
    let error = service
        .submit_job(&ctx, &presentation, conflicting)
        .await
        .expect_err("same key + different payload must conflict");
    assert_eq!(error.code, ApiErrorCode::IdempotencyConflict);
    assert_eq!(error.code.as_str(), "idempotencyConflict");
}

#[tokio::test]
async fn gate_denial_blocks_admission_and_keeps_the_repository_clean() {
    let (issuer, service, jobs, _audit, _path) = harness("gate", Arc::new(DenyAllGate));
    let ctx = ctx();
    let presentation = presentation(&issuer, &ctx).await;

    let error = service
        .submit_job(&ctx, &presentation, definition("job-1", payload()))
        .await
        .expect_err("denying gate blocks the submission");
    assert_eq!(error.code, ApiErrorCode::QuotaExceeded);

    // 仓储无受理痕迹：幂等受理发生在闸门之后。
    let probe = jobs
        .get(&ctx, JobId::new("job-1"))
        .await
        .expect_err("nothing was admitted");
    assert!(matches!(probe, PortError::NotFound(_)));
}

#[tokio::test]
async fn a_broken_token_is_rejected_as_unauthenticated() {
    let (issuer, service, _jobs, _audit, _path) = harness("token", Arc::new(AllowAllGate));
    let ctx = ctx();
    let mut presentation = presentation(&issuer, &ctx).await;
    presentation.token = SubmissionToken {
        idempotency_key: IdempotencyKey::new("not.a.valid.token"),
        expires_at: Timestamp::new("2026-01-02T00:00:00.000Z"),
    };

    let error = service
        .submit_job(&ctx, &presentation, definition("job-1", payload()))
        .await
        .expect_err("a forged token must fail verification");
    assert_eq!(error.code, ApiErrorCode::Unauthenticated);
}

#[tokio::test]
async fn success_and_conflict_paths_leave_audit_events() {
    let (issuer, service, _jobs, _audit, path) = harness("audit", Arc::new(AllowAllGate));
    let ctx = ctx();
    let presentation = presentation(&issuer, &ctx).await;
    service
        .submit_job(&ctx, &presentation, definition("job-1", payload()))
        .await
        .expect("admitted");

    // 同令牌异指纹再投（合法计划、不同正文）→ 冲突路径也出审计。
    let _ = service
        .submit_job(
            &ctx,
            &presentation,
            definition(
                "job-2",
                serde_json::json!({
                    "planVersion": 1,
                    "handlerVersion": 1,
                    "checkpointVersion": 1,
                    "steps": ["load"],
                }),
            ),
        )
        .await;

    let rows = audit_actions(&path);
    assert!(
        rows.iter()
            .any(|(action, outcome)| action == "job_submitted" && outcome == "succeeded"),
        "admission is audited: {rows:?}"
    );
    assert!(
        rows.iter()
            .any(|(action, outcome)| action == "job_submit_conflicted" && outcome == "failed"),
        "conflict is audited: {rows:?}"
    );
}
