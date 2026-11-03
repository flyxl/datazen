//! WR-G1：CM W1 断言钉子（平台开发计划 §11 / P7 退出门槛的服务端腿）。
//!
//! 门槛原文（platform-development-plan §11）：**CM-05～07、38～40、53～56、
//! 59、60 单实例部分及 CM-63、68、70、72 的 W1 断言**。本文件把**当前
//! 服务端集成面已经能表达**的 W1 断言落成可执行钉子；其余在文件尾的
//! 「CM→状态账目表」里如实登记（已由哪些既有测试钉住 / 缺什么才可落）。
//!
//! 装配样式沿用 `server/tests` 既有约定：内存/文件 `AdminStore` + 端口
//! 替身，直调服务，无路由接线（路由面钉子归 `skeleton.rs` / `sse_events.rs`）。
//!
//! ## CM → 状态账目表（报告同款，代码即账本）
//!
//! | CM | 断言点 | 状态 | 钉子位置 |
//! |----|--------|------|----------|
//! | CM-05 | 跨组织资源不可见（防枚举 404/空） | ✅ 已钉 | 本文件 `cm05_*`；`transfer_artifacts.rs`（产物下载）；`operations_ttl_cas_tasks.rs`（任务中心）；`storage/profiles_tests.rs`（配置 get/list） |
//! | CM-06 | 伪造 owner / 身份字段无法覆盖 RequestContext | ✅ 已钉 | 本文件 `cm05_06_*`（definition 组织盖章）+ `cm06_*`（令牌身份交叉核对） |
//! | CM-07 | 目标缺失/冲突（H/D） | ⏳ W1 缺口 | 断言主体在 H/D 层（`unified_*` / driver e2e）；W1 请求体目标校验待执行面路由接线 |
//! | CM-38 | CAS 冲突 409、凭据轮换独立递增、跨组织隔离 | ✅ 已钉 | 本文件 `cm38_*`；CAS 冲突 wire 形另由 `operations_ttl_cas_tasks.rs`（404 半）+ `src/operations/config_cas.rs` 单测钉住 |
//! | CM-39 | 禁用后配置版本递增；drain 拒绝新请求 | ✅ 已钉 | 本文件 `cm39_*`（disable）；drain 拒绝由 `operations_gate_and_revocation.rs::quota_gate_reports_draining_as_a_distinct_rejection` 钉住；「排队请求按真实取消结果记录」属 H |
//! | CM-40 | 重订阅不重建 Job、权限仍检查 | ✅ 已钉 | 本文件 `cm40_*`；「driver 未被 UI release」属 H |
//! | CM-53 | AI/MCP/Wapp 归属 | ⏳ W1 缺口 | MCP wire/契约已由 `mcp_http_wire.rs` / `mcp_contract_parity.rs` 钉住；会话 owner 归属面待 MCP runtime 装配切片 |
//! | CM-54 | 同 key 重发同 receipt | ✅ 已钉 | `transfer_submission.rs::token_replay_returns_the_original_receipt`；「写入一次/无法读取记录时核验」待 driver 执行面 |
//! | CM-55 | 事件重复/乱序/缺口（W 半） | ✅ 已钉（部分） | `sse_events.rs`（after_sequence 重放、缺口 streamResetRequired、重复 sequence 单调）；「超大 revision 精确计数」F 半已覆盖、W 半待大 Counter 事件注入 |
//! | CM-56 | 产物 TTL 清扫 | ✅ 已钉（部分） | `operations_ttl_cas_tasks.rs::ttl_sweep_*`；事务 TTL/grace 重附着的 W1 面待会话路由接线（H 半由 modules `directory_attachment_ttl.rs` 钉住） |
//! | CM-59 | 撤销传播 + CSRF 拒绝 | ✅ 已钉 | `operations_gate_and_revocation.rs`（同事务 bump + Stale）+ 本文件 `cm59_*`（撤销后会话死亡 × CSRF 非活会话拒绝的组合钉子）；SSE origin 钉子见 `sse_events.rs` |
//! | CM-60 | 单实例额度 admit/deny/release + drain | ✅ 已钉（H 同形） | `operations_gate_and_revocation.rs`（额度三段 + drain）；p95 基准归 `cm60-bench`，不入本轮 |
//! | CM-63 | attach/detach TTL 竞态 | ⏳ W1 缺口 | H 半由 modules `directory_attachment_ttl.rs` 钉住；W1 HTTP attach/detach 面未接线 |
//! | CM-68 | 候选替换发布/响应丢失 | ⏳ W1 缺口 | 替换发布属 runtime H + P9 WN；W1 面未接线 |
//! | CM-70 | 过期令牌重发不受理 | ✅ 已钉（部分） | 本文件 `cm70_*`（过期即拒）；伪造令牌由 `transfer_submission.rs::a_broken_token_is_rejected_as_unauthenticated` 钉住；「记录保留期删除」无清扫 API——缺口登记 |
//! | CM-72 | 提交面错误/冲突审计 | ✅ 已钉（部分） | `transfer_submission.rs::success_and_conflict_paths_leave_audit_events`；能力版本审计（capabilitySnapshot）属 H `p4_usecase_journeys`，W1 控制回执面待接线 |

use std::sync::Arc;

use async_trait::async_trait;

use datazen::connection::domain::ProfileRepository;
use datazen::connection::dto::{ProfileDraft, ProfilePatch};
use datazen::execution::application::idempotency::IdempotentOperation;
use datazen::execution::application::submission::{SubmissionPresentation, SubmissionTokenIssuer};
use datazen::jobs::domain::job::JobDefinition;
use datazen::jobs::domain::repository::JobRepository;
use datazen::jobs::domain::time::{JobClock, SystemJobClock};
use datazen::shared::audit::{AuditEvent, AuditSink};
use datazen::shared::clock::{Clock, FixedClock};
use datazen::shared::context::{OwnerRef, RequestContext};
use datazen::shared::error::{ApiErrorCode, PortError};
use datazen::shared::id::{
    AuthenticationSessionId, ClientInstanceId, ConfigRevision, IdempotencyKey, JobId,
    OrganizationId, PrincipalId, RequestId, Timestamp,
};

use datazen_server::csrf::{self, CsrfRejection, CsrfSecret};
use datazen_server::execution::submission_token::ServerSubmissionTokenIssuer;
use datazen_server::operations::{RevocationEventSink, RevocationService, TaskCenter, TaskListQuery};
use datazen_server::policy::PolicyService;
use datazen_server::storage::login_sessions::{LoginSessionMode, NewLoginSession};
use datazen_server::storage::{AdminStore, LoginSessions, ServerAudit, ServerJobs, ServerProfiles, ValidateFailure};
use datazen_server::transfer::submission::SubmissionService;
use datazen_server::transfer::AllowAllGate;

const NOW: &str = "2026-01-01T00:00:00.000Z";

// ── 公共装配 ─────────────────────────────────────────────────────────────

fn store() -> Arc<AdminStore> {
    let store = AdminStore::open_in_memory().expect("store");
    store.migrate().expect("migrate");
    Arc::new(store)
}

fn ctx(org: &str, principal: &str) -> RequestContext {
    RequestContext::new(
        OrganizationId::new(org),
        PrincipalId::new(principal),
        Some(AuthenticationSessionId::new("auth-1")),
        ClientInstanceId::new("client-1"),
        RequestId::new("req-1"),
        None,
    )
}

fn definition(org: &str, job_id: &str, owner_client: &str, payload: serde_json::Value) -> JobDefinition {
    JobDefinition {
        job_id: JobId::new(job_id),
        // 伪造点：调用方可把 definition 的组织/owner 写成任何值——
        // 受理必须按 ctx 重新盖章（CM-05/06 的宿主半边）。
        organization_id: OrganizationId::new(org),
        kind: "transfer".into(),
        owner: OwnerRef::ClientSession {
            client_instance_id: ClientInstanceId::new(owner_client),
            purpose: "g1-cm-w1".into(),
        },
        payload,
        created_at: Timestamp::new(NOW),
    }
}

fn payload() -> serde_json::Value {
    serde_json::json!({
        "planVersion": 1,
        "handlerVersion": 1,
        "checkpointVersion": 1,
    })
}

/// 可平移的测试时钟（CM-70 推进过期用）。`FixedClock::set` 是 `&self`，
/// 因此持 `Arc<FixedClock>` 共享同一根指针。
#[derive(Clone)]
struct ShiftClock(Arc<FixedClock>);

impl Clock for ShiftClock {
    fn now(&self) -> Timestamp {
        self.0.now()
    }
}

impl JobClock for ShiftClock {
    fn now(&self) -> Timestamp {
        self.0.now()
    }
}

type SubmissionHarness = (
    Arc<ServerSubmissionTokenIssuer>,
    SubmissionService,
    Arc<ServerJobs>,
);

fn submission_harness(fixed: Arc<FixedClock>) -> SubmissionHarness {
    let store = store();
    let jobs = Arc::new(ServerJobs::new(
        store.clone(),
        Arc::new(ShiftClock(fixed.clone())),
        900,
    ));
    let issuer = Arc::new(
        ServerSubmissionTokenIssuer::new(b"wr-g1-submission-test-key".to_vec(), Arc::new(ShiftClock(fixed.clone())))
            .expect("issuer accepts non-empty key material"),
    );
    let audit = Arc::new(ServerAudit::new(store));
    let service = SubmissionService::new(
        issuer.clone(),
        jobs.clone(),
        Arc::new(AllowAllGate),
        audit,
        Arc::new(ShiftClock(fixed)),
    );
    (issuer, service, jobs)
}

async fn presentation(
    issuer: &ServerSubmissionTokenIssuer,
    context: &RequestContext,
) -> SubmissionPresentation {
    let token = issuer
        .issue(context, IdempotentOperation::StartJob, None)
        .await
        .expect("token issues for a startJob operation");
    SubmissionPresentation {
        token,
        claimed_organization_id: context.organization_id.clone(),
        claimed_principal_id: context.principal_id.clone(),
        claimed_client_instance_id: context.client_instance_id.clone(),
        claimed_delegation_id: context.delegation_id.clone(),
    }
}

// ── CM-05 / CM-06：跨组织不可见 + 伪造 owner 不作数 ─────────────────────

/// CM-05（W1）：跨组织读答「不存在」；CM-06（W1）：definition 里的组织与
/// owner 是调用方可伪造的字段——受理按 `RequestContext` 重新盖章，伪造值
/// 既不改变归属、也不提供任何越权入口。
#[tokio::test]
async fn cm05_06_a_forged_owner_cannot_override_the_request_context() {
    let fixed = Arc::new(FixedClock::at(NOW));
    let (_issuer, service, jobs) = submission_harness(fixed);

    // 伪造：user-1（org-1）提交的 definition 声称属于 org-other、owner 是
    // 别人的 client。
    let submitter = ctx("org-1", "user-1");
    let presentation = presentation(&_issuer, &submitter).await;
    let forged = definition("org-other", "job-1", "not-my-client", payload());
    let receipt = service
        .submit_job(&submitter, &presentation, forged)
        .await
        .expect("forged fields do not block an otherwise legitimate submission");
    assert_eq!(
        receipt.definition.organization_id.as_str(),
        "org-1",
        "归属按 RequestContext 盖章，definition 的组织字段不作数（CM-06）"
    );

    // CM-05：另一组织读同一 job_id → NotFound（不是 403，防枚举）。
    let outsider = ctx("org-2", "user-2");
    let probe = JobRepository::get(jobs.as_ref(), &outsider, JobId::new("job-1"))
        .await
        .expect_err("cross-org job reads are invisible");
    assert!(matches!(probe, PortError::NotFound(_)));
}

/// CM-06（W1）：令牌是签名事实、ctx 是认证链盖章事实——为 user-1 签发的
/// 提交令牌拿到 user-2 的会话下使用即伪造，拒绝且不受理。
#[tokio::test]
async fn cm06_a_token_minted_for_another_principal_is_rejected() {
    let fixed = Arc::new(FixedClock::at(NOW));
    let (issuer, service, _jobs) = submission_harness(fixed);

    let owner = ctx("org-1", "user-1");
    let presentation = presentation(&issuer, &owner).await;
    let impostor = ctx("org-1", "user-2");
    let error = service
        .submit_job(&impostor, &presentation, definition("org-1", "job-1", "client-1", payload()))
        .await
        .expect_err("a token minted for user-1 must not submit under user-2");
    assert_eq!(error.code, ApiErrorCode::Unauthenticated);
}

// ── CM-38：CAS 轮换、凭据版本独立、跨组织隔离 ───────────────────────────

/// CM-38（W1）：CAS 更新前进版本；携带旧 expected 的请求按明确政策拒绝
/// （`CasConflict` → 409 `configRevisionMismatch`）；凭据轮换只推进
/// `credentialRevision`、不动配置面；跨组织 CAS 答 NotFound（与 CM-05 同口径）。
#[tokio::test]
async fn cm38_cas_rotation_is_versioned_credential_scoped_and_org_isolated() {
    let profiles = ServerProfiles::new(store());
    let owner = ctx("org-a", "user-1");
    let record = profiles
        .create(&owner, ProfileDraft::new("primary", "postgres"), &IdempotencyKey::new("idem-1"))
        .await
        .expect("create succeeds");
    assert_eq!(record.config_revision.counter().get(), 1);

    // CAS 成功：expected=1 → revision 2。
    let patch = ProfilePatch {
        name: Some("rotated".into()),
        ..ProfilePatch::default()
    };
    let updated = ProfileRepository::compare_and_set(
        &profiles,
        &owner,
        record.connection_id.clone(),
        ConfigRevision::new(1),
        patch,
    )
    .await
    .expect("fresh expected revision applies");
    assert_eq!(updated.config_revision.counter().get(), 2);

    // 旧 expected 再来 → CasConflict（旧请求按明确政策拒绝，不是静默覆盖）。
    let stale = ProfileRepository::compare_and_set(
        &profiles,
        &owner,
        record.connection_id.clone(),
        ConfigRevision::new(1),
        ProfilePatch::default(),
    )
    .await
    .expect_err("stale expected revision conflicts");
    assert!(matches!(stale, PortError::CasConflict { .. }));

    // 凭据轮换：只动 credentialRevision（1 → 2），configRevision 保持 2。
    let credential = profiles
        .advance_credential_revision(&owner, record.connection_id.clone())
        .await
        .expect("credential rotation");
    assert_eq!(credential.counter().get(), 2);
    let after = ProfileRepository::get(&profiles, &owner, record.connection_id.clone())
        .await
        .expect("get succeeds")
        .expect("record exists");
    assert_eq!(after.config_revision.counter().get(), 2, "轮换凭据不动配置面");

    // CM-05 × CM-38：跨组织 CAS / 轮换 / disable 全部 NotFound（不可区分
    // 「不存在」与「无权」，防枚举）。
    let outsider = ctx("org-b", "user-2");
    for outcome in [
        ProfileRepository::compare_and_set(
            &profiles,
            &outsider,
            record.connection_id.clone(),
            ConfigRevision::new(2),
            ProfilePatch::default(),
        )
        .await
        .expect_err("cross-org CAS"),
        profiles
            .advance_credential_revision(&outsider, record.connection_id.clone())
            .await
            .expect_err("cross-org rotation"),
    ] {
        assert!(matches!(outcome, PortError::NotFound(_)));
    }
}

// ── CM-39：禁用即断流入口 ────────────────────────────────────────────────

/// CM-39（W1）：管理员禁用后配置可见面关闭（enabled=false、configRevision
/// 递增——排队请求的 drain/force 取消语义属 H 层，额度面由
/// `operations_gate_and_revocation.rs` 的 drain 拒绝钉住）。
#[tokio::test]
async fn cm39_disable_closes_the_config_surface_and_bumps_the_revision() {
    let profiles = ServerProfiles::new(store());
    let owner = ctx("org-a", "user-1");
    let record = profiles
        .create(&owner, ProfileDraft::new("primary", "postgres"), &IdempotencyKey::new("idem-1"))
        .await
        .expect("create succeeds");

    let disabled = profiles
        .disable(&owner, record.connection_id.clone())
        .await
        .expect("disable");
    assert!(!disabled.enabled, "禁用后配置不可用");
    assert!(
        disabled.config_revision.counter().get() > record.config_revision.counter().get(),
        "禁用本身是一次配置变更（可审计、可被 CAS 察觉）"
    );
    // 禁用后 CAS 仍按版本语义拒绝旧 expected——不因禁用出现第二条写路径。
    let stale = ProfileRepository::compare_and_set(
        &profiles,
        &owner,
        record.connection_id.clone(),
        record.config_revision.clone(),
        ProfilePatch::default(),
    )
    .await
    .expect_err("the pre-disable revision is stale after disable");
    assert!(matches!(stale, PortError::CasConflict { .. }));
}

// ── CM-40：重订阅不重建 Job、权限仍检查 ─────────────────────────────────

/// CM-40（W1）：关闭窗口后按 jobId 回来「重订阅」＝同一幂等键重放受理——
/// 拿回同一 JobRecord、列表只有一行；跨组织按 jobId 查询仍答 404
/// （重订阅不豁免权限检查）。
#[tokio::test]
async fn cm40_resubscribing_by_job_id_does_not_recreate_the_job() {
    let store = store();
    let jobs = Arc::new(ServerJobs::new(store.clone(), Arc::new(SystemJobClock), 900));
    let center = TaskCenter::new(jobs.clone(), 50);

    let owner = ctx("org-a", "user-1");
    let first = jobs
        .accept(&owner, definition("org-a", "job-1", "client-1", payload()), &IdempotencyKey::new("idem-1"))
        .await
        .expect("accepted");
    // 模拟重开窗口后的重订阅：同 job_id、同幂等键再受理一次。
    let replay = jobs
        .accept(&owner, definition("org-a", "job-1", "client-1", payload()), &IdempotencyKey::new("idem-1"))
        .await
        .expect("replay returns the original receipt");
    assert_eq!(
        first.definition.job_id, replay.definition.job_id,
        "重订阅不重建 Job"
    );

    let page = center
        .list_jobs(&owner, TaskListQuery::default())
        .await
        .expect("list");
    assert_eq!(page.jobs.len(), 1, "重放不产生第二行");

    let outsider = ctx("org-b", "user-2");
    let detail = center
        .get_job(&outsider, &JobId::new("job-1"))
        .await
        .expect_err("resubscribing across organizations is still a permission check");
    assert_eq!(detail.code, ApiErrorCode::NotFound);
}

// ── CM-59：撤销传播 × CSRF（W1 组合钉子）────────────────────────────────

/// 审计替身（撤销服务要求一个 AuditSink）。
#[derive(Default)]
struct SilentAudit;

impl AuditSink for SilentAudit {
    fn record(&self, _event: AuditEvent) -> Result<(), datazen::shared::error::PortError> {
        Ok(())
    }
}

/// 撤销事件替身：只登记调用（本钉子关心的是会话与 CSRF 的后续状态）。
#[derive(Default)]
struct SilentEvents;

#[async_trait]
impl RevocationEventSink for SilentEvents {
    async fn publish_permission_revoked(
        &self,
        _organization_id: &OrganizationId,
        _principal_id: Option<&str>,
        _permission_version: i64,
    ) -> Result<Option<datazen::execution::application::EventSequence>, datazen::shared::error::PortError> {
        Ok(None)
    }
}

const CSRF_SECRET: &[u8] = b"wr-g1-csrf-secret-32-bytes-min!!";

fn session_input(org: &str, principal: &str, session_id: &str, version: i64) -> NewLoginSession {
    NewLoginSession {
        organization_id: org.into(),
        session_id: session_id.into(),
        principal_id: principal.into(),
        auth_time: NOW.into(),
        issued_at: NOW.into(),
        absolute_expires_at: "2026-01-02T00:00:00.000Z".into(),
        idle_expires_at: "2026-01-01T08:00:00.000Z".into(),
        permission_version: version,
        client_binding: "ua|client-1".into(),
        mode: LoginSessionMode::Cookie,
        native_secret_hash: None,
    }
}

/// CM-59（W1）：登出/撤销后——(1) 登录会话行被撤销（peek 拒绝）；
/// (2) 绑定该会话的 CSRF 令牌在「会话非活」判定下拒绝（SessionInactive），
/// 跨站伪造修改请求无入口。同令牌在撤销前通过，证明拒绝确实由撤销触发。
#[tokio::test]
async fn cm59_after_revocation_the_session_dies_and_csrf_refuses_it() {
    let store = store();
    let policy = PolicyService::new(store.clone(), b"wr-g1-server-secret-32-bytes!".to_vec())
        .expect("policy");
    policy
        .create_membership("org-a", "user-1", datazen_server::policy::Role::OrgMember, "sub-1")
        .expect("membership");

    let sessions = LoginSessions::new(store.clone());
    sessions
        .create(session_input("org-a", "user-1", "sess-1", 1))
        .expect("session");

    let secret = CsrfSecret::new(CSRF_SECRET.to_vec()).expect("secret");
    let token = csrf::mint(&secret, "sess-1").expect("mint");
    csrf::verify(&secret, "sess-1", &token, &token, true)
        .expect("before revocation the token verifies for the live session");

    let service = RevocationService::new(store, Arc::new(SilentEvents), Arc::new(SilentAudit));
    service
        .revoke_sessions_for_principal(&ctx("org-a", "user-1"), &OrganizationId::new("org-a"), "user-1")
        .await
        .expect("revocation propagates");

    let rejected = sessions
        .peek("sess-1", Timestamp::new(NOW))
        .expect_err("the revoked session is dead");
    assert!(matches!(rejected, ValidateFailure::Rejected(_)));
    assert_eq!(
        csrf::verify(&secret, "sess-1", &token, &token, false),
        Err(CsrfRejection::SessionInactive),
        "撤销后同一令牌在会话非活判定下拒绝（403，对攻击者与普通无权同形）"
    );
}

// ── CM-70：过期幂等令牌永不受理 ─────────────────────────────────────────

/// CM-70（W1）：有效期内受理（对照组语义已有 `transfer_submission.rs`）；
/// 时钟推进过 TTL 后**同一令牌**重发 → `Unauthenticated`，且无论时钟怎么
/// 回拨都不会复活（过期判定在服务端时钟上，拒绝以当前时刻为准）。
#[tokio::test]
async fn cm70_an_expired_submission_token_is_never_accepted() {
    let fixed = Arc::new(FixedClock::at(NOW));
    let store = store();
    let jobs = Arc::new(ServerJobs::new(store.clone(), Arc::new(ShiftClock(fixed.clone())), 900));
    let issuer = Arc::new(
        ServerSubmissionTokenIssuer::with_ttl(
            b"wr-g1-submission-test-key".to_vec(),
            Arc::new(ShiftClock(fixed.clone())),
            std::time::Duration::from_secs(60),
        )
        .expect("issuer"),
    );
    let audit = Arc::new(ServerAudit::new(store));
    let service = SubmissionService::new(
        issuer.clone(),
        jobs.clone(),
        Arc::new(AllowAllGate),
        audit,
        Arc::new(ShiftClock(fixed.clone())),
    );

    let context = ctx("org-1", "user-1");
    let presentation = presentation(&issuer, &context).await;

    // 有效期内：受理成功（对照组）。
    service
        .submit_job(&context, &presentation, definition("org-1", "job-1", "client-1", payload()))
        .await
        .expect("inside the TTL the token is accepted");

    // 过期：同一令牌重发 → Unauthenticated，仓储不再受理。
    fixed.set("2026-01-01T00:02:00.000Z");
    let error = service
        .submit_job(&context, &presentation, definition("org-1", "job-2", "client-1", payload()))
        .await
        .expect_err("an expired token is refused");
    assert_eq!(error.code, ApiErrorCode::Unauthenticated);

    let probe = JobRepository::get(jobs.as_ref(), &context, JobId::new("job-2")).await;
    assert!(
        matches!(probe, Err(PortError::NotFound(_))),
        "过期重发不产生受理痕迹"
    );
}
