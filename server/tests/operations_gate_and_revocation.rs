//! WR-D6 集成行为钉子（一）：QuotaGate 拒绝/放行 + 撤销传播同事务版本 bump。
//!
//! 初始化模式与 `storage` 既有测试一致：`AdminStore::open_in_memory` +
//! `migrate`（内存库、全迁移展开）。审计与事件用进程内替身记录调用，
//! 断言的是**服务语义**（拒绝码、版本序列、同事务性），不钉存储行。

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use datazen::execution::application::budget::{
    BudgetCoordinator, BudgetPurpose, BudgetRequest,
};
use datazen::shared::audit::{AuditEvent, AuditSink};
use datazen::shared::error::ApiErrorCode;
use datazen::shared::id::{ConnectionId, OrganizationId, PrincipalId, RequestId, Timestamp};
use datazen_server::execution::budget::ServerBudgetCoordinator;
use datazen_server::operations::{
    QuotaGate, QuotaRejectionReason, RevocationEventSink, RevocationService,
};
use datazen_server::policy::PolicyService;
use datazen_server::storage::login_sessions::{LoginSessionMode, NewLoginSession};
use datazen_server::storage::LoginSessions;
use datazen_server::storage::ValidateFailure;

const NOW: &str = "2026-01-01T00:00:00.000Z";

fn store() -> Arc<datazen_server::storage::AdminStore> {
    let store = datazen_server::storage::AdminStore::open_in_memory().expect("store");
    store.migrate().expect("migrate");
    Arc::new(store)
}

fn context() -> datazen::shared::context::RequestContext {
    datazen::shared::context::RequestContext::new(
        OrganizationId::new("org-a"),
        PrincipalId::new("user-1"),
        None,
        datazen::shared::id::ClientInstanceId::new("client-1"),
        RequestId::new("req-1"),
        None,
    )
}

fn request() -> BudgetRequest {
    BudgetRequest::new(
        OrganizationId::new("org-a"),
        ConnectionId::new("conn-1"),
        BudgetPurpose::UserInteractive,
    )
}

/// 审计替身：记录 action 词（配额拒绝必须落审计是 WR-D6 验收点）。
#[derive(Default, Clone)]
struct RecordingAudit {
    actions: std::sync::Arc<Mutex<Vec<String>>>,
}

impl RecordingAudit {
    fn contains(&self, action: &str) -> bool {
        self.actions.lock().expect("lock").iter().any(|a| a == action)
    }
}

impl AuditSink for RecordingAudit {
    fn record(&self, event: AuditEvent) -> Result<(), datazen::shared::error::PortError> {
        self.actions.lock().expect("lock").push(event.action);
        Ok(())
    }
}

/// 撤销事件替身：记录（组织，版本）调用。
#[derive(Default, Clone)]
struct RecordingEvents {
    calls: std::sync::Arc<Mutex<Vec<(String, i64)>>>,
}

#[async_trait]
impl RevocationEventSink for RecordingEvents {
    async fn publish_permission_revoked(
        &self,
        organization_id: &OrganizationId,
        _principal_id: Option<&str>,
        permission_version: i64,
    ) -> Result<Option<datazen::execution::application::EventSequence>, datazen::shared::error::PortError>
    {
        self.calls
            .lock()
            .expect("lock")
            .push((organization_id.as_str().to_string(), permission_version));
        Ok(Some(datazen::execution::application::EventSequence::new(1)))
    }
}

// ── QuotaGate ──

/// 拒绝路径：额度为零 → 拒绝 + 审计落 `quota_denied` + 429 语义。
#[tokio::test]
async fn quota_gate_denies_and_audits_when_the_budget_is_exhausted() {
    let audit = RecordingAudit::default();
    // node_limit = 0：任何申请都装不进节点账。
    let gate = QuotaGate::new(
        Arc::new(ServerBudgetCoordinator::new(
            0,
            0,
            OrganizationId::new("org-a"),
        )),
        Arc::new(audit.clone()),
    );
    let rejection = gate.gate(&context(), request()).await.expect_err("denied");
    assert_eq!(rejection.reason, QuotaRejectionReason::CapacityExhausted);
    assert_eq!(rejection.organization_id, "org-a");
    assert_eq!(rejection.connection_id, "conn-1");
    let api = rejection.to_api_error();
    assert_eq!(api.code, ApiErrorCode::QuotaExceeded);
    // 余量信息：端口 v1 快照是节点聚合（in_use=0 是排空前的账面值）。
    assert_eq!(rejection.snapshot_in_use, Some(0));
    assert!(audit.contains("quota_denied"), "拒绝必须落审计");
    // 消息脱敏：不带连接名原文。
    assert!(!api.message.contains("conn-1"), "拒绝文案不得携带连接名");
}

/// 排空拒绝：`QuotaExceeded` 端口事实 → `Draining`（不是容量耗尽）。
#[tokio::test]
async fn quota_gate_reports_draining_as_a_distinct_rejection() {
    let coordinator = ServerBudgetCoordinator::new(4, 4, OrganizationId::new("org-a"));
    let gate = QuotaGate::new(Arc::new(coordinator.clone()), Arc::new(RecordingAudit::default()));
    coordinator
        .drain(datazen::execution::application::budget::DrainScope::Organization(
            OrganizationId::new("org-a"),
        ))
        .await
        .expect("drain");
    let rejection = gate.gate(&context(), request()).await.expect_err("denied");
    assert_eq!(rejection.reason, QuotaRejectionReason::Draining);
}

/// 放行路径：许可入账 → 第二次拒绝 → 释放 → 再放行（INV-10 幂等释放）。
#[tokio::test]
async fn quota_gate_admits_then_releases_then_admits_again() {
    let gate = QuotaGate::new(
        Arc::new(ServerBudgetCoordinator::new(
            1,
            1,
            OrganizationId::new("org-a"),
        )),
        Arc::new(RecordingAudit::default()),
    );
    let admission = gate.gate(&context(), request()).await.expect("admitted");
    let second = gate.gate(&context(), request()).await;
    assert!(second.is_err(), "节点额度 1：第二个申请必须拒绝");
    gate.release(&admission.permit, datazen::execution::application::budget::ReleaseOutcome::Unused)
        .await
        .expect("release");
    gate.gate(&context(), request())
        .await
        .expect("released slot is reusable");
}

// ── RevocationService（同事务是验收核心） ──

fn policy(store: &Arc<datazen_server::storage::AdminStore>) -> PolicyService {
    PolicyService::new(store.clone(), b"test-server-secret-32-bytes-min!!".to_vec())
        .expect("policy")
}

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

/// 撤销传播：同事务 bump + 会话撤销 + 事件发布 + 版本校验助手判 Stale。
#[tokio::test]
async fn revocation_bumps_the_version_and_revokes_sessions_in_one_commit() {
    let store = store();
    let policy = policy(&store);
    policy
        .create_membership("org-a", "user-1", datazen_server::policy::Role::OrgMember, "sub-1")
        .expect("membership");
    policy
        .create_membership("org-a", "user-2", datazen_server::policy::Role::OrgMember, "sub-2")
        .expect("membership");
    let sessions = LoginSessions::new(store.clone());
    sessions
        .create(session_input("org-a", "user-1", "sess-1", 1))
        .expect("session");
    // user-2 的会话快照 1：传播后应判 Stale（授权面过期）。
    sessions
        .create(session_input("org-a", "user-2", "sess-2", 1))
        .expect("session");

    let events = RecordingEvents::default();
    let service = RevocationService::new(store.clone(), Arc::new(events.clone()), Arc::new(RecordingAudit::default()));
    let outcome = service
        .revoke_sessions_for_principal(
            &context(),
            &OrganizationId::new("org-a"),
            "user-1",
        )
        .await
        .expect("propagated");
    assert_eq!(outcome.commit.permission_version, 2, "首建为 2");
    assert_eq!(outcome.commit.revoked_sessions, 1, "只撤 user-1 的会话");
    assert!(outcome.published, "事件已发布");
    assert_eq!(
        events.calls.lock().expect("lock").as_slice(),
        [("org-a".to_string(), 2)]
    );

    // 版本递增与授权库一致（同一条账）。
    assert_eq!(policy.permission_version("org-a").expect("version"), 2);
    // user-1 的会话已撤销（peek 拒绝）。
    let rejected = sessions
        .peek("sess-1", Timestamp::new(NOW))
        .expect_err("revoked");
    assert!(matches!(rejected, ValidateFailure::Rejected(_)));
    // user-2 的会话存活，但版本快照已落后 → Stale。
    let check = service
        .check_session_permission(&sessions, "sess-2", Timestamp::new(NOW))
        .expect("check");
    assert_eq!(
        check,
        datazen_server::operations::PermissionCheck::Stale {
            session_version: 1,
            current_version: 2,
        }
    );
}

/// 事务性反例：mutation 失败 → 版本不递增、无事件、会话原样存活。
#[tokio::test]
async fn a_failed_revocation_mutation_leaves_the_version_untouched() {
    let store = store();
    let policy = policy(&store);
    policy
        .create_membership("org-a", "user-1", datazen_server::policy::Role::OrgMember, "sub-1")
        .expect("membership");
    let sessions = LoginSessions::new(store.clone());
    sessions
        .create(session_input("org-a", "user-1", "sess-1", 1))
        .expect("session");

    let events = RecordingEvents::default();
    let service = RevocationService::new(store.clone(), Arc::new(events.clone()), Arc::new(RecordingAudit::default()));
    // disable_principal 指向不存在的 membership → 事务回滚。
    let error = service
        .disable_principal(&context(), &OrganizationId::new("org-a"), "ghost")
        .await
        .expect_err("membership missing");
    assert_eq!(error.code, ApiErrorCode::ServiceUnavailable);
    assert_eq!(policy.permission_version("org-a").expect("version"), 1);
    assert!(
        events.calls.lock().expect("lock").is_empty(),
        "回滚不得发布事件"
    );
    // 会话原样存活（同事务撤销也随回滚丢失）。
    sessions
        .peek("sess-1", Timestamp::new(NOW))
        .expect("session survives the rollback");
}

/// Current 分支：快照等于当前版本 → 会话授权面未过期。
#[tokio::test]
async fn session_permission_check_reports_current_for_fresh_snapshots() {
    let store = store();
    let policy = policy(&store);
    policy
        .create_membership("org-a", "user-1", datazen_server::policy::Role::OrgMember, "sub-1")
        .expect("membership");
    let sessions = LoginSessions::new(store.clone());
    sessions
        .create(session_input("org-a", "user-1", "sess-1", 1))
        .expect("session");
    let service = RevocationService::new(
        store,
        Arc::new(RecordingEvents::default()),
        Arc::new(RecordingAudit::default()),
    );
    let check = service
        .check_session_permission(&sessions, "sess-1", Timestamp::new(NOW))
        .expect("check");
    assert_eq!(
        check,
        datazen_server::operations::PermissionCheck::Current { permission_version: 1 }
    );
}
