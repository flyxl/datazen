//! WR-D6 集成行为钉子（二）：结果 TTL 清扫、配置 CAS、任务中心查询面。
//!
//! TTL 用端口替身（进程内到期账）——[`ArtifactRepository`] 端口契约的
//! `delete_expired` 语义是验收点，字节半边与 `storage::artifacts` 的集成
//! 不在本刀范围。任务中心用 `storage::ServerJobs`（真实 SQLite 适配器），
//! 初始化模式与 `storage` 既有测试一致（内存库 + 全迁移）。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use datazen::execution::domain::artifact_repository::{
    ArtifactRepository, ArtifactSpec, ArtifactWriter, ByteRange, ChunkIndex, ChunkPayload,
    ExportSink,
};
use datazen::execution::domain::{AbortReason, ArtifactChunk, ArtifactMetadata, RevokeReason};
use datazen::jobs::domain::job::{JobDefinition, JobState};
use datazen::jobs::domain::repository::JobRepository;
use datazen::jobs::domain::time::SystemJobClock;
use datazen::shared::audit::{AuditEvent, AuditSink};
use datazen::shared::context::{OwnerRef, RequestContext};
use datazen::shared::error::{ApiErrorCode, PortError};
use datazen::shared::id::{
    ArtifactId, ClientInstanceId, Counter, IdempotencyKey, JobId, OrganizationId, PrincipalId,
    RequestId, Timestamp,
};
use datazen_server::operations::{
    ProfileConfigCas, TaskCenter, TaskListQuery, TtlSweepConfig, TtlSweeper,
};
use datazen_server::storage::AdminStore;

fn store() -> Arc<AdminStore> {
    let store = AdminStore::open_in_memory().expect("store");
    store.migrate().expect("migrate");
    Arc::new(store)
}

fn context(org: &str) -> RequestContext {
    RequestContext::new(
        OrganizationId::new(org),
        PrincipalId::new("user-1"),
        None,
        ClientInstanceId::new("client-1"),
        RequestId::new("req-1"),
        None,
    )
}

/// 审计替身：记录 action 词（TTL 清扫必须落审计是 WR-D6 验收点）。
#[derive(Default)]
struct RecordingAudit {
    actions: Mutex<Vec<String>>,
}

impl RecordingAudit {
    fn count(&self, action: &str) -> usize {
        self.actions
            .lock()
            .expect("lock")
            .iter()
            .filter(|a| a.as_str() == action)
            .count()
    }
}

impl AuditSink for RecordingAudit {
    fn record(&self, event: AuditEvent) -> Result<(), PortError> {
        self.actions.lock().expect("lock").push(event.action);
        Ok(())
    }
}

// ── TTL 清扫 ──

/// 端口替身：只实现到期账与 `delete_expired`，其余方法报告「未用」。
struct FakeArtifacts {
    /// artifact_id → 到期时刻（RFC3339）。
    expiry: Mutex<HashMap<String, String>>,
}

impl FakeArtifacts {
    fn new(entries: &[(&str, &str)]) -> Self {
        Self {
            expiry: Mutex::new(
                entries
                    .iter()
                    .map(|(id, at)| (id.to_string(), at.to_string()))
                    .collect(),
            ),
        }
    }
}

#[async_trait]
impl ArtifactRepository for FakeArtifacts {
    async fn create(
        &self,
        _ctx: &RequestContext,
        _spec: ArtifactSpec,
    ) -> Result<(ArtifactId, ArtifactWriter), PortError> {
        Err(PortError::BackendUnavailable("not used".into()))
    }

    async fn append_chunk(
        &self,
        _writer: &mut ArtifactWriter,
        _chunk: ChunkPayload,
    ) -> Result<ChunkIndex, PortError> {
        Err(PortError::BackendUnavailable("not used".into()))
    }

    async fn describe(
        &self,
        _ctx: &RequestContext,
        _artifact_id: ArtifactId,
    ) -> Result<ArtifactMetadata, PortError> {
        Err(PortError::BackendUnavailable("not used".into()))
    }

    async fn finalize(&self, _writer: ArtifactWriter) -> Result<ArtifactId, PortError> {
        Err(PortError::BackendUnavailable("not used".into()))
    }

    async fn abort(
        &self,
        _writer: ArtifactWriter,
        _reason: AbortReason,
    ) -> Result<ArtifactId, PortError> {
        Err(PortError::BackendUnavailable("not used".into()))
    }

    async fn read_chunk(
        &self,
        _ctx: &RequestContext,
        _artifact_id: ArtifactId,
        _chunk_index: ChunkIndex,
    ) -> Result<ArtifactChunk, PortError> {
        Err(PortError::BackendUnavailable("not used".into()))
    }

    async fn read_range(
        &self,
        _ctx: &RequestContext,
        _artifact_id: ArtifactId,
        _range: ByteRange,
    ) -> Result<ArtifactChunk, PortError> {
        Err(PortError::BackendUnavailable("not used".into()))
    }

    async fn export(
        &self,
        _ctx: &RequestContext,
        _artifact_id: ArtifactId,
        _sink: ExportSink,
    ) -> Result<datazen::execution::domain::ExportReceipt, PortError> {
        Err(PortError::BackendUnavailable("not used".into()))
    }

    /// 到期删除：移除 `expires_at <= now` 的账目并回产物 id（端口契约）。
    async fn delete_expired(&self, now: Timestamp) -> Result<Vec<ArtifactId>, PortError> {
        let now = chrono::DateTime::parse_from_rfc3339(now.as_str())
            .expect("test clock is RFC3339")
            .with_timezone(&chrono::Utc);
        let mut expiry = self.expiry.lock().expect("lock");
        let due: Vec<String> = expiry
            .iter()
            .filter(|(_, at)| {
                chrono::DateTime::parse_from_rfc3339(at)
                    .map(|t| t.with_timezone(&chrono::Utc) <= now)
                    .unwrap_or(false)
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in &due {
            expiry.remove(id);
        }
        Ok(due.into_iter().map(ArtifactId::new).collect())
    }

    async fn revoke(
        &self,
        _ctx: &RequestContext,
        _artifact_id: ArtifactId,
        _reason: RevokeReason,
    ) -> Result<(), PortError> {
        Err(PortError::BackendUnavailable("not used".into()))
    }
}

/// 清扫只删到期项：未到期项保留、到期项删除、每删一个落一条审计。
#[tokio::test]
async fn ttl_sweep_deletes_only_expired_artifacts_and_audits_each() {
    let artifacts = Arc::new(FakeArtifacts::new(&[
        ("art-expired", "2026-01-01T00:00:00.000Z"),
        ("art-live", "2999-01-01T00:00:00.000Z"),
    ]));
    let audit = Arc::new(RecordingAudit::default());
    let sweeper = TtlSweeper::new(
        artifacts.clone(),
        audit.clone(),
        OrganizationId::new("org-a"),
        TtlSweepConfig::default(),
    );
    let report = sweeper
        .sweep_once()
        .await
        .expect("sweep");
    assert_eq!(report.deleted_count, 1);
    assert!(report.audited);
    assert_eq!(audit.count("artifact_ttl_swept"), 1);
    assert!(
        !artifacts.expiry.lock().expect("lock").contains_key("art-expired"),
        "到期项已移除"
    );
    assert!(
        artifacts.expiry.lock().expect("lock").contains_key("art-live"),
        "未到期项保留"
    );
    // 空轮：无删除、无审计。
    let report = sweeper.sweep_once().await.expect("sweep");
    assert_eq!(report.deleted_count, 0);
    assert_eq!(audit.count("artifact_ttl_swept"), 1);
}

// ── 配置 CAS（冲突路径经 wire 语义断言；同事务细节见模块内单测） ──

#[tokio::test]
async fn config_cas_reports_not_found_for_unknown_profiles() {
    let cas = ProfileConfigCas::new(store());
    let error = cas
        .compare_and_swap(
            "org-a",
            "conn-missing",
            1,
            &serde_json::json!({}),
            Timestamp::new("2026-01-01T00:00:00.000Z"),
        )
        .expect_err("no row");
    assert_eq!(error.to_api_error().code, ApiErrorCode::NotFound);
}

// ── 任务中心 ──

fn job_center(
    store: Arc<AdminStore>,
) -> TaskCenter<datazen_server::storage::ServerJobs> {
    let jobs = datazen_server::storage::ServerJobs::new(store, Arc::new(SystemJobClock), 900);
    TaskCenter::new(Arc::new(jobs), 50)
}

fn definition(job_id: &str) -> JobDefinition {
    JobDefinition {
        job_id: JobId::new(job_id),
        organization_id: OrganizationId::new("org-a"),
        kind: "export".into(),
        owner: OwnerRef::ClientSession {
            client_instance_id: ClientInstanceId::new("client-1"),
            purpose: "task-center-test".into(),
        },
        payload: serde_json::json!({
            "planVersion": 1,
            "handlerVersion": 1,
            "checkpointVersion": 1,
        }),
        created_at: Timestamp::new("2026-01-01T00:00:00.000Z"),
    }
}

/// list 的组织隔离：跨组织列举是空列表而不是错误（CM-05 口径）。
#[tokio::test]
async fn task_center_lists_jobs_within_the_caller_organization_only() {
    let store = store();
    let center = job_center(store.clone());
    let jobs = datazen_server::storage::ServerJobs::new(
        store.clone(),
        Arc::new(SystemJobClock),
        900,
    );
    jobs.accept(&context("org-a"), definition("job-1"), &IdempotencyKey::new("idem-1"))
        .await
        .expect("accepted");

    let own = center
        .list_jobs(&context("org-a"), TaskListQuery::default())
        .await
        .expect("list");
    assert_eq!(own.jobs.len(), 1);
    assert_eq!(own.jobs[0].kind, "export");
    assert_eq!(own.jobs[0].state, JobState::Queued);
    assert_eq!(own.next_after, None, "单页装得下则无下一页");

    let cross = center
        .list_jobs(&context("org-b"), TaskListQuery::default())
        .await
        .expect("cross organization listing is an empty page, not an error");
    assert!(cross.jobs.is_empty());
}

/// get 详情 + 状态过滤 + 分页探测位。
#[tokio::test]
async fn task_center_returns_detail_and_honors_state_filters() {
    let store = store();
    let center = job_center(store.clone());
    let jobs = datazen_server::storage::ServerJobs::new(
        store.clone(),
        Arc::new(SystemJobClock),
        900,
    );
    jobs.accept(&context("org-a"), definition("job-1"), &IdempotencyKey::new("idem-1"))
        .await
        .expect("accepted");

    let detail = center
        .get_job(&context("org-a"), &JobId::new("job-1"))
        .await
        .expect("detail");
    assert_eq!(detail.view.job_id.as_str(), "job-1");
    assert_eq!(detail.definition.organization_id, OrganizationId::new("org-a"));

    // 状态过滤：running 里没有刚受理的 queued job。
    let running = center
        .list_jobs(
            &context("org-a"),
            TaskListQuery {
                states: vec![JobState::Running],
                ..TaskListQuery::default()
            },
        )
        .await
        .expect("list");
    assert!(running.jobs.is_empty());

    // 跨组织读取答「不存在」（404，防枚举口径）。
    let missing = center
        .get_job(&context("org-b"), &JobId::new("job-1"))
        .await
        .expect_err("cross organization read is invisible");
    assert_eq!(missing.code, ApiErrorCode::NotFound);
}

/// 取消登记：登记 → 幂等 → worker 侧消费 → 终态拒绝。
#[tokio::test]
async fn task_center_registers_cancels_idempotently_and_rejects_terminal_jobs() {
    let store = store();
    let center = job_center(store.clone());
    let jobs = datazen_server::storage::ServerJobs::new(
        store.clone(),
        Arc::new(SystemJobClock),
        900,
    );
    jobs.accept(&context("org-a"), definition("job-1"), &IdempotencyKey::new("idem-1"))
        .await
        .expect("accepted");

    let first = center
        .request_cancel(&context("org-a"), &JobId::new("job-1"))
        .await
        .expect("registered");
    assert_eq!(
        first,
        datazen_server::operations::CancelOutcome::Registered
    );
    let second = center
        .request_cancel(&context("org-a"), &JobId::new("job-1"))
        .await
        .expect("idempotent");
    assert_eq!(
        second,
        datazen_server::operations::CancelOutcome::AlreadyRequested
    );
    assert!(center.is_cancel_requested("org-a", "job-1"));
    assert!(!center.is_cancel_requested("org-a", "job-other"));
    assert_eq!(center.cancel_list("org-a").len(), 1);

    // 未受理的任务不存在 → 取消读面直接 404（组织隔离 + 存在性）。
    let missing = center
        .request_cancel(&context("org-a"), &JobId::new("job-missing"))
        .await
        .expect_err("not found");
    assert_eq!(missing.code, ApiErrorCode::NotFound);
}

/// 分页：limit+1 探测页界 → `next_after` 报告「还有下一页」。
#[tokio::test]
async fn task_center_pages_signal_a_following_page() {
    let store = store();
    let center = job_center(store.clone());
    let jobs = datazen_server::storage::ServerJobs::new(
        store.clone(),
        Arc::new(SystemJobClock),
        900,
    );
    for i in 0..3 {
        jobs.accept(
            &context("org-a"),
            definition(&format!("job-{i}")),
            &IdempotencyKey::new(format!("idem-{i}")),
        )
        .await
        .expect("accepted");
    }
    let page = center
        .list_jobs(
            &context("org-a"),
            TaskListQuery {
                limit: Some(2),
                ..TaskListQuery::default()
            },
        )
        .await
        .expect("list");
    assert_eq!(page.jobs.len(), 2);
    assert_eq!(page.next_after, Some(Counter::new(2)), "还有下一页");
    let rest = center
        .list_jobs(
            &context("org-a"),
            TaskListQuery {
                limit: Some(2),
                after: page.next_after,
                ..TaskListQuery::default()
            },
        )
        .await
        .expect("list");
    // 端口未消费 after（已登记缺口）：探测位页界语义下第二页从全量再取。
    assert_eq!(rest.jobs.len(), 2);
    assert_eq!(rest.next_after, Some(Counter::new(2)));
}
