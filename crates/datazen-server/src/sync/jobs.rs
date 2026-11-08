//! Data Sync job 的读回与取消（W8 刀3e-3；桌面 `commands/sync/jobs.rs`
//! 读命令 + `job_api.rs` 读面的 server 对应物）。
//!
//! 受理/派发（prepare / apply 两类 durable Job）随刀3e-4 落地；本刀的
//! 读面对**空而真实**的状态工作：JobHost 按 ctx 组织过滤，kind 过滤
//! （`dataSyncPrepare` / `dataSyncApply`——共享层 `job::body` 的两个
//! KIND 常量）在服务层做（`JobFilter` 无 kind 字段，schema_diff 同款
//! 后置过滤）。
//!
//! 读回语义（桌面 IPC 契约逐条对齐）：
//!
//! - `list` → `Vec<JobDetails>`（桌面 `list_data_sync_jobs` 逐 job 折
//!   details——UI 要进度与阶段，不只 JobView）。
//! - `get` → `JobView`（Failed 态的 `error` 由 [`SyncRuntimeState`] 的
//!   宿主侧消息覆写——durable 记录只落安全原因码）。
//! - `details` → `JobDetails`（同上覆写）。
//! - `preview` → `SyncComparisonPreview`（plan_store 的行级复核投影；
//!   durable 计划受理后投影仍在物化时，60s 窗口内等待生产者——桌面
//!   `preview_for_durable_job` 同款，不发布陈旧的「计划不可用」）。
//! - `cancel` → `{ cancelled: bool }`（未知 id / 他类 job → false 幂等
//!   no-op；桌面 `cancel_durable_job` 的 NotFound 腿在服务端是 false
//!   而非「预受理取消记忆」——服务端 job id 是受理时铸造的，客户端
//!   无法在受理前指名一个还不存在的 id）。

use std::sync::Arc;
use std::time::Duration;

use datazen::jobs::domain::job::{JobDetails, JobFilter, JobState, JobView};
use datazen::jobs::JobHost;
use datazen::migration::data_sync::job::body::{APPLY_KIND, PREPARE_KIND};
use datazen::migration::data_sync::plan_store::{
    preview_for_plan, SyncComparisonPreview, SyncPlanRepository,
};
use datazen::shared::context::RequestContext;
use datazen::shared::error::PortError;
use datazen::shared::id::JobId;
use datazen::shared::scope::OrganizationScope;

use super::runtime_state::SyncRuntimeState;

/// sync job 读面错误（路由层按桶映射 wire 码）。
#[derive(Debug, thiserror::Error)]
pub enum SyncJobError {
    /// 未知 id / 他类 job（→ 404；统一 404 纪律，不区分「不存在」与
    /// 「不可见」）。
    #[error("Data Sync job '{0}' was not found")]
    NotFound(String),
    /// 语义校验失败（→ 400 `invalidArgument`；桌面 `CommandError::Validation`
    /// 的服务端对应物）。
    #[error("{0}")]
    Validation(String),
    /// job 仓储故障（→ 503 `serviceUnavailable`）。
    #[error("job store unavailable: {0}")]
    Unavailable(String),
}

fn port_error(error: PortError) -> SyncJobError {
    match error {
        PortError::NotFound(id) => SyncJobError::NotFound(id),
        other => SyncJobError::Unavailable(other.to_string()),
    }
}

/// 桌面同款等待窗口：复核投影物化的上限（超出后如实回答「仍在准备，
/// 稍后重读」——不是错误，是诚实占位）。
const PREVIEW_WAIT: Duration = Duration::from_secs(60);
/// 等待轮询间隔（桌面同款）。
const PREVIEW_POLL: Duration = Duration::from_millis(50);

/// Data Sync job 服务（JobHost + 计划注册表 + 运行态的装配组合）。
#[derive(Clone)]
pub struct SyncJobs {
    host: Arc<JobHost>,
    plans: Arc<dyn SyncPlanRepository>,
    runtime: SyncRuntimeState,
}

impl SyncJobs {
    pub fn new(
        host: Arc<JobHost>,
        plans: Arc<dyn SyncPlanRepository>,
        runtime: SyncRuntimeState,
    ) -> Self {
        Self {
            host,
            plans,
            runtime,
        }
    }

    fn is_sync_kind(kind: &str) -> bool {
        matches!(kind, PREPARE_KIND | APPLY_KIND)
    }

    /// `listDataSyncJobs` → `Vec<JobDetails>`（桌面逐 job 折 details 同款）。
    pub async fn list(&self, ctx: &RequestContext) -> Result<Vec<JobDetails>, SyncJobError> {
        let records = self
            .host
            .list(
                ctx,
                JobFilter {
                    organization_id: ctx.organization_id.clone(),
                    states: Vec::new(),
                    owner: None,
                    after: None,
                    limit: Some(100),
                },
            )
            .await
            .map_err(port_error)?;
        let mut details = Vec::new();
        for record in records {
            if !Self::is_sync_kind(&record.view.kind) {
                continue;
            }
            details.push(self.details(ctx, record.view.job_id.as_str()).await?);
        }
        Ok(details)
    }

    /// `getDataSyncJob` → `JobView`（Failed 态宿主侧消息覆写）。
    pub async fn get(&self, ctx: &RequestContext, job_id: &str) -> Result<JobView, SyncJobError> {
        let record = self
            .host
            .get(ctx, JobId::new(job_id.to_owned()))
            .await
            .map_err(port_error)?;
        if !Self::is_sync_kind(&record.view.kind) {
            return Err(SyncJobError::NotFound(job_id.to_owned()));
        }
        let mut view = record.view;
        if view.state == JobState::Failed {
            if let Some(message) = self.runtime.safe_failure_of(job_id) {
                view.error = Some(message);
            }
        }
        Ok(view)
    }

    /// `getDataSyncJobDetails` → `JobDetails`（同上覆写）。
    pub async fn details(
        &self,
        ctx: &RequestContext,
        job_id: &str,
    ) -> Result<JobDetails, SyncJobError> {
        let mut details = self
            .host
            .details(ctx, JobId::new(job_id.to_owned()))
            .await
            .map_err(port_error)?;
        if !Self::is_sync_kind(&details.job.kind) {
            return Err(SyncJobError::NotFound(job_id.to_owned()));
        }
        if details.job.state == JobState::Failed {
            if let Some(message) = self.runtime.safe_failure_of(job_id) {
                details.job.error = Some(message);
            }
        }
        Ok(details)
    }

    /// `cancelDataSync` → `{ cancelled }`（未知/他类 → false；终态 → true；
    /// 在途 → 登记取消意图回 true。桌面 `cancel_durable_job` 同语义减去
    /// 预受理取消记忆——服务端 id 受理时铸造，无「受理前取消」窗口）。
    pub async fn cancel(&self, ctx: &RequestContext, job_id: &str) -> Result<bool, SyncJobError> {
        let id = JobId::new(job_id.to_owned());
        match self.host.get(ctx, id.clone()).await {
            Err(PortError::NotFound(_)) => return Ok(false),
            Err(error) => return Err(port_error(error)),
            Ok(record) if !Self::is_sync_kind(&record.view.kind) => return Ok(false),
            Ok(record) if record.view.state.is_terminal() => return Ok(true),
            Ok(_) => {}
        }
        match self.host.cancel(ctx, &id).await {
            Ok(record) => {
                Ok(record.view.cancel_requested || record.view.state == JobState::Cancelled)
            }
            Err(PortError::NotFound(_)) => Ok(false),
            Err(error) => Err(port_error(error)),
        }
    }

    /// `getDataSyncJobPreview` → `SyncComparisonPreview`。
    ///
    /// 仅 prepare 类 job 有计划回执；durable 计划已受理但复核投影仍在
    /// 物化时，60s 窗口内轮询等待（桌面 `preview_for_durable_job` 同款，
    /// 超时如实回答「仍在准备，稍后重读」）。
    pub async fn preview(
        &self,
        ctx: &RequestContext,
        scope: &OrganizationScope,
        job_id: &str,
    ) -> Result<SyncComparisonPreview, SyncJobError> {
        let details = self.details(ctx, job_id).await?;
        if details.job.kind != PREPARE_KIND {
            return Err(SyncJobError::Validation(
                "job is not a Data Sync prepare job".into(),
            ));
        }
        let Some(plan_id) = details.plan_id else {
            return Err(SyncJobError::Validation(
                "Data Sync prepare job has no durable plan receipt".into(),
            ));
        };
        let deadline = tokio::time::Instant::now() + PREVIEW_WAIT;
        while self.runtime.preview_pending(job_id) {
            if tokio::time::Instant::now() >= deadline {
                return Err(SyncJobError::Validation(
                    "Data Sync review projection is still being prepared; retry loading the preview"
                        .into(),
                ));
            }
            tokio::time::sleep(PREVIEW_POLL).await;
        }
        preview_for_plan(self.plans.as_ref(), scope, &plan_id).map_err(SyncJobError::Validation)
    }
}
