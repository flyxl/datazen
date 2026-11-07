//! 任务中心查询面（WR-D6）：list / detail / 取消登记。
//!
//! 全部是 **handler 级函数**（参数含组织身份——`RequestContext` 的组织盖
//! 章），不碰 router：HTTP 绑定归主线。组织隔离不在这里重复实现——
//! [`JobRepository`] 的适配器（`crate::storage::ServerJobs`）按 `ctx` 的
//! 组织过滤，跨组织读取答「不存在」（CM-05），本模块只把端口事实映射为
//! wire 语义。
//!
//! ## 分页
//!
//! 端口的 `JobFilter.after`（`Option<Counter>`）在内存实现与 SQLite 适配器
//! 里**都未消费**（`storage::jobs` 模块文档的已登记差异）。本面采用
//! 「limit+1 探测页界」的本域分页：`next_after` 目前只是「下一页存在」的
//! 信号位（页内条数），游标语义待端口消费 `after` 后重定义（缺口登记）。
//!
//! ## 取消的已登记缺口
//!
//! `JobRepository` 端口**没有取消语义**（取消属 ctx 作用域写：§10.1.1 的
//! 「取消请求是独立事实，不把 state 提前改成 cancelled」，端口未定义该
//! 写路径；`compare_and_set_state` 需要 worker 的 `JobClaim`，handler 侧
//! 不可达）。本模块落**最小本域实现** [`CancelRegistry`]：进程内登记
//! 「取消已请求」事实 + 读侧存在性/终态校验；worker 侧在阶段边界消费
//! [`TaskCenter::is_cancel_requested`] 是接线下一步。跨进程/重启持久的
//! 取消登记（`jobs` 表加列）随端口扩展补。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use datazen::jobs::domain::job::{JobFilter, JobRecord, JobState, RecoveryFilter};
use datazen::jobs::domain::repository::JobRepository;
use datazen::shared::context::{OwnerRef, RequestContext};
use datazen::shared::error::ApiError;
use datazen::shared::id::{Counter, JobId};
use serde::Serialize;

/// 单页上限的硬顶（分页参数的物理上限；超过按上限截取）。
pub const MAX_PAGE_LIMIT: u32 = 200;

/// list 的查询参数（组织维度来自 `ctx`，不接受调用方自报组织）。
#[derive(Debug, Clone, Default)]
pub struct TaskListQuery {
    /// 按状态过滤（空 = 全部状态）。
    pub states: Vec<JobState>,
    /// 按归属过滤（可选）。
    pub owner: Option<OwnerRef>,
    /// 游标位（上一页的 `next_after`）。
    pub after: Option<Counter>,
    /// 页大小（缺省/上限见 [`TaskCenter::new`] 与 [`MAX_PAGE_LIMIT`]）。
    pub limit: Option<u32>,
}

/// 一页任务视图。`Serialize` 供 HTTP handler 直接以 JSON 应答（`JobView`
/// 与 `Counter` 均已 serde 就绪；CM-01 计数器按十进制字符串穿线）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskListPage {
    pub jobs: Vec<datazen::jobs::domain::job::JobView>,
    /// 下一页游标位。端口消费 `after` 前，非 `None` 只表示「还有下一页」。
    pub next_after: Option<Counter>,
}

/// 取消登记的处置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CancelOutcome {
    /// 本次调用完成了登记。
    Registered,
    /// 该任务此前已被登记过（幂等）。
    AlreadyRequested,
    /// 任务已终态（succeeded/failed/cancelled）：取消无意义。
    AlreadyTerminal { state: JobState },
}

/// 一条取消登记。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CancelRecord {
    pub organization_id: String,
    pub job_id: String,
    pub requested_by: String,
    pub requested_at: String,
}

/// 进程内取消登记（最小本域实现；持久化是已登记缺口）。
#[derive(Default)]
pub struct CancelRegistry {
    entries: Mutex<HashMap<(String, String), CancelRecord>>,
}

impl CancelRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// 登记/查询幂等：重复登记返回 `AlreadyRequested`。
    fn register(&self, ctx: &RequestContext, job_id: &JobId) -> CancelOutcome {
        let mut entries = self.entries.lock().expect("cancel registry lock");
        let key = (
            ctx.organization_id.as_str().to_string(),
            job_id.as_str().to_string(),
        );
        if entries.contains_key(&key) {
            return CancelOutcome::AlreadyRequested;
        }
        entries.insert(
            key,
            CancelRecord {
                organization_id: ctx.organization_id.as_str().to_string(),
                job_id: job_id.as_str().to_string(),
                requested_by: ctx.principal_id.as_str().to_string(),
                requested_at: super::now_string(),
            },
        );
        CancelOutcome::Registered
    }

    /// worker 侧消费：该任务是否已被请求取消。
    pub fn is_requested(&self, organization_id: &str, job_id: &str) -> bool {
        self.entries
            .lock()
            .expect("cancel registry lock")
            .contains_key(&(organization_id.to_string(), job_id.to_string()))
    }

    /// 运维侧消费：组织的全部取消登记。
    pub fn list(&self, organization_id: &str) -> Vec<CancelRecord> {
        let entries = self.entries.lock().expect("cancel registry lock");
        let mut records: Vec<CancelRecord> = entries
            .iter()
            .filter(|((org, _), _)| org == organization_id)
            .map(|(_, record)| record.clone())
            .collect();
        records.sort_by(|a, b| a.requested_at.cmp(&b.requested_at));
        records
    }
}

/// 任务中心查询面。`J` 是 [`JobRepository`] 端口实现（装配注入
/// `crate::storage::ServerJobs`）。
pub struct TaskCenter<J: JobRepository> {
    jobs: Arc<J>,
    cancels: CancelRegistry,
    default_page_limit: u32,
}

impl<J: JobRepository> TaskCenter<J> {
    /// `default_page_limit` 是查询未显式给 `limit` 时的页大小（建议 50；
    /// 物理上限 [`MAX_PAGE_LIMIT`]）。
    pub fn new(jobs: Arc<J>, default_page_limit: u32) -> Self {
        Self {
            jobs,
            cancels: CancelRegistry::new(),
            default_page_limit: default_page_limit.clamp(1, MAX_PAGE_LIMIT),
        }
    }

    /// list jobs：组织隔离 + 分页。
    pub async fn list_jobs(
        &self,
        ctx: &RequestContext,
        query: TaskListQuery,
    ) -> Result<TaskListPage, ApiError> {
        let limit = query
            .limit
            .unwrap_or(self.default_page_limit)
            .clamp(1, MAX_PAGE_LIMIT);
        // limit+1 探测页界：端口未消费 after，多取一条是本域分页的探测手段。
        let filter = JobFilter {
            organization_id: ctx.organization_id.clone(),
            states: query.states,
            owner: query.owner,
            after: query.after,
            limit: Some(limit.saturating_add(1)),
        };
        let mut records: Vec<JobRecord> = self
            .jobs
            .list(ctx, filter)
            .await
            .map_err(|e| super::port_error_to_api_error(&e))?;
        let has_more = records.len() > limit as usize;
        records.truncate(limit as usize);
        let next_after = has_more.then(|| Counter::new(records.len() as u64));
        Ok(TaskListPage {
            jobs: records.into_iter().map(|record| record.view).collect(),
            next_after,
        })
    }

    /// get job 详情：视图 + 定义 + 阶段 + 状态版本。
    ///
    /// 检查点/提交边界的**读**端口未定义（`save_checkpoint` 只写）——
    /// 恢复核验走 [`Self::list_recovery_candidates`]（端口既有的恢复扫描
    /// 读取面），明细级 checkpoint 查询随端口读扩展补（缺口登记）。
    pub async fn get_job(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
    ) -> Result<JobRecord, ApiError> {
        self.jobs
            .get(ctx, job_id.clone())
            .await
            .map_err(|e| super::port_error_to_api_error(&e))
    }

    /// 恢复候选（待核验）查询：检查点/提交边界的读取入口。
    pub async fn list_recovery_candidates(
        &self,
        ctx: &RequestContext,
        filter: RecoveryFilter,
    ) -> Result<Vec<JobRecord>, ApiError> {
        self.jobs
            .list_recoverable(ctx, filter)
            .await
            .map_err(|e| super::port_error_to_api_error(&e))
    }

    /// 取消登记：先按组织读取（隔离 + 存在性 + 终态判定），再登记。
    pub async fn request_cancel(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
    ) -> Result<CancelOutcome, ApiError> {
        let record = self.get_job(ctx, job_id).await?;
        if record.view.state.is_terminal() {
            return Ok(CancelOutcome::AlreadyTerminal {
                state: record.view.state,
            });
        }
        Ok(self.cancels.register(ctx, job_id))
    }

    /// worker 侧消费：该任务是否已被请求取消。
    pub fn is_cancel_requested(&self, organization_id: &str, job_id: &str) -> bool {
        self.cancels.is_requested(organization_id, job_id)
    }

    /// 运维侧消费：组织的全部取消登记。
    pub fn cancel_list(&self, organization_id: &str) -> Vec<CancelRecord> {
        self.cancels.list(organization_id)
    }
}
