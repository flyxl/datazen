//! `JobRepository` 的服务端适配器（WR-D2 第三切片）。
//!
//! 同一 domain trait、另一个宿主的实现：语义与内核 `InMemoryJobRepository`
//! **逐方法对齐**（同一用例、两种绑定）——
//!
//! * **先持久化后取资源**（连接 §10.1）：`accept` 成功返回即已受理；
//! * **幂等受理**：同键同指纹 → 原 Job receipt；同键不同指纹 →
//!   `IdempotencyConflict`；apply 类 Job 的 `payload.consumedPlanId` 在受理
//!   事务内建立唯一消费（部分唯一索引兜底，失败也不释放消费资格）；
//! * **claim fencing**：generation/worker 匹配 + 租约未过期才放行写入；
//!   越组织的写入按 `StaleClaim` 拒绝（不是 `NotFound`），越组织的读取按
//!   「不存在」处理（CM-05，与 `ProfileRepository` 同口径）；
//! * **状态 CAS**：版本不匹配拒绝，不做覆盖写；终态清空 claim；
//! * **checkpoint** 以 `(organization_id, job_id, state_version)` 为键，
//!   重复版本冲突；**不保存** live session / lease / cursor（§4.2）；
//! * **恢复扫描**只产出待核验候选，绝不重放副作用阶段。
//!
//! 与内存实现的两处**已记录**形状差异（均由存储键形状决定，行为不变）：
//! 1. 内存实现的全量 `jobs` 表按 `job_id` 单键（进程内全局唯一）；SQLite 按
//!    persistence-model §3.6 的复合主键 `(organization_id, job_id)`——本适配器
//!    的「job 身份已被受理」检查按组织限定，跨组织撞 `job_id` 是两行而不是
//!    覆盖（UUID 现实下不可达，语义上也不存在覆盖入口）。
//! 2. `list` 的 `after` 游标：内存实现同样未消费该字段（端口文档允许游标
//!    不落盘），两腿一致，待分页切片一起接线。
//!
//! 异步边界：内核 trait 是 async，SQLite 是同步——每个 trait 方法把同步
//! 核心丢进 `spawn_blocking`，差异只允许出现在这一层。

use std::sync::Arc;

use async_trait::async_trait;
use rusqlite::{params, OptionalExtension, Row};

use datazen::execution::domain::EffectOutcome;
use datazen::jobs::domain::job::{
    Checkpoint, CommitBoundary, JobClaim, JobDefinition, JobFilter, JobProgress, JobRecord,
    JobState, JobView, RecoveryFilter, StageRecord,
};
use datazen::jobs::domain::plan::project_frozen_plan;
use datazen::jobs::domain::repository::JobRepository;
use datazen::jobs::domain::time::{after_seconds, JobClock};
use datazen::shared::context::{OwnerRef, RequestContext};
use datazen::shared::error::PortError;
use datazen::shared::hash::fnv1a64_hex;
use datazen::shared::id::{
    ArtifactId, Counter, ExecutionId, IdempotencyKey, JobId, JobStateVersion, OrganizationId,
    StageId, Timestamp, WorkerId,
};

use super::AdminStore;

/// 行投影的完整列清单（所有 SELECT 共用，顺序即 `record_from_row` 的下标）。
const JOB_COLUMNS: &str = "organization_id, job_id, kind, state, state_version, stage, \
     owner_json, payload_json, created_at, updated_at, execution_ids_json, artifact_ids_json, \
     effect_outcome, cancel_requested, pending_verification_reason, error, progress_json";

/// claim 列（`JobRowCore` 的读取面，紧跟 `JOB_COLUMNS` 之后）。
const CLAIM_COLUMNS: &str =
    "worker_id, claim_stage_id, claimed_at, claim_expires_at, claim_generation";

const STAGE_COLUMNS: &str =
    "stage_id, kind, claimed_by, execution_ids_json, started_at, finished_at";

#[derive(Clone)]
pub struct ServerJobs {
    store: Arc<AdminStore>,
    clock: Arc<dyn JobClock>,
    claim_ttl_secs: i64,
}

/// 一行的完整读取面：端口投影（`JobRecord`）+ claim 核心列。
struct JobDbRow {
    record: JobRecord,
    core: JobRowCore,
}

/// 行上与 claim 相关的核心列（`check_claim` / CAS / claim 的公共读取面）。
struct JobRowCore {
    organization_id: String,
    job_id: String,
    worker_id: Option<String>,
    claim_stage_id: Option<String>,
    claimed_at: Option<String>,
    claim_expires_at: Option<String>,
    claim_generation: i64,
}

impl ServerJobs {
    /// 在已迁移的管理库上构造。调用方负责先跑 `AdminStore::migrate`。
    ///
    /// `claim_ttl_secs` 与内核内存实现同义：租约时长由组装层给定，
    /// 仓储只负责按 `clock + ttl` 铸造与校验到期时间。
    pub fn new(store: Arc<AdminStore>, clock: Arc<dyn JobClock>, claim_ttl_secs: i64) -> Self {
        Self {
            store,
            clock,
            claim_ttl_secs,
        }
    }

    // ── 同步核心（trait 实现只做 spawn_blocking 包装） ──

    /// 按 `(organization_id, job_id)` 读一行（含阶段）。跨组织读取天然落空。
    fn sync_get(
        conn: &rusqlite::Connection,
        organization_id: &str,
        job_id: &str,
    ) -> Result<Option<JobRecord>, PortError> {
        match Self::sync_get_db_row(conn, organization_id, job_id)? {
            None => Ok(None),
            Some(mut row) => {
                row.record.stages = row.core.stages(conn)?;
                Ok(Some(row.record))
            }
        }
    }

    fn sync_get_db_row(
        conn: &rusqlite::Connection,
        organization_id: &str,
        job_id: &str,
    ) -> Result<Option<JobDbRow>, PortError> {
        conn.query_row(
            &format!(
                "SELECT {JOB_COLUMNS}, {CLAIM_COLUMNS} FROM jobs
                 WHERE organization_id = ?1 AND job_id = ?2"
            ),
            params![organization_id, job_id],
            db_row_from_row,
        )
        .optional()
        .map_err(backend)
    }

    fn sync_accept(
        &self,
        ctx: &RequestContext,
        mut definition: JobDefinition,
        idem: &IdempotencyKey,
    ) -> Result<JobRecord, PortError> {
        // 组织盖章（INV-01）：调用方自报的归属不作数，落库归属来自 ctx。
        definition.organization_id = ctx.organization_id.clone();
        let plan =
            project_frozen_plan(&definition.kind, &definition.payload).map_err(PortError::from)?;
        let fingerprint = format!(
            "{}|{}",
            definition.kind,
            fnv1a64_hex(definition.payload.to_string().as_bytes())
        );
        let idem_scope_key = format!(
            "{}|{}|{}",
            ctx.organization_id.as_str(),
            ctx.principal_id.as_str(),
            idem.as_str()
        );
        let organization_id = ctx.organization_id.as_str().to_string();
        let job_id = definition.job_id.as_str().to_string();
        let kind = definition.kind.clone();
        let owner_json = serde_json::to_string(&definition.owner)
            .map_err(|_| backend("owner serialization failed"))?;
        let payload_json = definition.payload.to_string();

        let conn = lock(&self.store)?;
        let tx = conn.unchecked_transaction().map_err(backend)?;

        // 幂等重放：同键取回原 Job；同键不同指纹判冲突（先于一切写入路径）。
        let replayed: Option<(String, Option<String>)> = tx
            .query_row(
                "SELECT job_id, accept_fingerprint FROM jobs
                 WHERE organization_id = ?1 AND idem_scope_key = ?2",
                params![organization_id, idem_scope_key],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(backend)?;
        if let Some((existing_job_id, existing_fingerprint)) = replayed {
            if existing_fingerprint.as_deref() == Some(fingerprint.as_str()) {
                return Self::sync_get(&tx, &organization_id, &existing_job_id)?
                    .ok_or_else(|| PortError::NotFound(existing_job_id));
            }
            return Err(PortError::IdempotencyConflict);
        }

        // planId 唯一消费：同一计划在同一组织内只被一个 apply Job 消费。
        if let Some(consumed_plan_id) = &plan.consumed_plan_id {
            let taken: Option<String> = tx
                .query_row(
                    "SELECT job_id FROM jobs
                     WHERE organization_id = ?1 AND consumed_plan_id = ?2",
                    params![organization_id, consumed_plan_id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(backend)?;
            if taken.is_some() {
                return Err(PortError::PlanAlreadyConsumed(consumed_plan_id.clone()));
            }
        }

        // job 身份占用：同组织内重复受理同一 job_id 是数据损坏，不覆盖
        // （跨组织撞 id 是两行——见模块头的形状差异登记）。
        let identity_taken: bool = tx
            .query_row(
                "SELECT EXISTS (SELECT 1 FROM jobs WHERE organization_id = ?1 AND job_id = ?2)",
                params![organization_id, job_id],
                |row| row.get(0),
            )
            .map_err(backend)?;
        if identity_taken {
            return Err(backend("job identity is already admitted"));
        }

        let now = self.clock.now();
        // 受理行的 created_at 来自仓储时钟（与内存实现同口径：调用方带来的
        // definition.created_at 不作数）；progress 落全量 JSON（五类计数
        // 反序列化无缺省，空对象不是合法投影）。
        let progress_json = serde_json::to_string(&JobProgress::default())
            .map_err(|_| backend("progress serialization failed"))?;
        tx.execute(
            "INSERT INTO jobs (
               organization_id, job_id, kind, state, state_version, stage,
               owner_json, payload_json, created_at, updated_at,
               execution_ids_json, artifact_ids_json, effect_outcome,
               cancel_requested, pending_verification_reason, error, progress_json,
               worker_id, claim_stage_id, claimed_at, claim_expires_at, claim_generation,
               consumed_plan_id, idem_scope_key, accept_fingerprint
             ) VALUES (?1, ?2, ?3, 'queued', 1, NULL,
               ?4, ?5, ?6, ?6,
               '[]', '[]', NULL,
               0, NULL, NULL, ?7,
               NULL, NULL, NULL, NULL, 0,
               ?8, ?9, ?10)",
            params![
                organization_id,
                job_id,
                kind,
                owner_json,
                payload_json,
                now.as_str(),
                progress_json,
                plan.consumed_plan_id,
                idem_scope_key,
                fingerprint,
            ],
        )
        .map_err(backend)?;
        tx.commit().map_err(backend)?;

        Self::sync_get(&conn, &organization_id, &job_id)?
            .ok_or_else(|| PortError::NotFound(job_id))
    }

    fn sync_list(
        &self,
        ctx: &RequestContext,
        filter: JobFilter,
    ) -> Result<Vec<JobRecord>, PortError> {
        // 列举必须按组织隔离：filter 与 ctx 的组织不一致 → 空（CM-05）。
        if filter.organization_id != ctx.organization_id {
            return Ok(vec![]);
        }
        let organization_id = ctx.organization_id.as_str().to_string();
        let conn = lock(&self.store)?;

        let mut sql = format!(
            "SELECT {JOB_COLUMNS}, {CLAIM_COLUMNS} FROM jobs WHERE organization_id = ?1"
        );
        let mut bind: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(organization_id.clone())];
        if !filter.states.is_empty() {
            let placeholders = (0..filter.states.len())
                .map(|index| format!("?{}", bind.len() + 1 + index))
                .collect::<Vec<_>>()
                .join(", ");
            sql.push_str(&format!(" AND state IN ({placeholders})"));
            for state in &filter.states {
                bind.push(Box::new(state_name(*state).to_string()));
            }
        }
        if let Some(owner) = &filter.owner {
            let owner_json = serde_json::to_string(owner)
                .map_err(|_| backend("owner serialization failed"))?;
            let index = bind.len() + 1;
            sql.push_str(&format!(" AND owner_json = ?{index}"));
            bind.push(Box::new(owner_json));
        }
        sql.push_str(" ORDER BY created_at, job_id");

        let mut stmt = conn.prepare(&sql).map_err(backend)?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(bind.iter()), db_row_from_row)
            .map_err(backend)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(backend)?;
        let mut out = Vec::with_capacity(rows.len());
        for mut row in rows {
            row.record.stages = row.core.stages(&conn)?;
            out.push(row.record);
        }
        Ok(out)
    }

    /// claim fencing：行存在 + 行上组织与 ctx 一致 + generation/worker 匹配 +
    /// 租约未过期。越组织的行按 `StaleClaim` 拒绝（写入路径兜底，端口契约）。
    /// 返回整行读取面（CAS / 阶段登记复用 claim 列）。
    fn sync_check_claim(
        conn: &rusqlite::Connection,
        ctx: &RequestContext,
        claim: &JobClaim,
        now: &str,
    ) -> Result<JobDbRow, PortError> {
        // 行读取不带组织谓词：先区分「行不存在」与「行存在但别组织」。
        let row = conn
            .query_row(
                &format!("SELECT {JOB_COLUMNS}, {CLAIM_COLUMNS} FROM jobs WHERE job_id = ?1"),
                params![claim.job_id.as_str()],
                db_row_from_row,
            )
            .optional()
            .map_err(backend)?
            .ok_or_else(|| PortError::NotFound(claim.job_id.as_str().to_string()))?;
        if row.core.organization_id != ctx.organization_id.as_str() {
            return Err(PortError::StaleClaim);
        }
        let current_worker = row.core.worker_id.as_deref().ok_or(PortError::StaleClaim)?;
        if current_worker != claim.worker_id.as_str()
            || row.core.claim_generation != claim.claim_generation.get() as i64
        {
            return Err(PortError::StaleClaim);
        }
        let current_expires = row
            .core
            .claim_expires_at
            .as_deref()
            .ok_or(PortError::StaleClaim)?;
        if now > current_expires {
            return Err(PortError::StaleClaim);
        }
        Ok(row)
    }

    fn sync_record_stage(
        &self,
        ctx: &RequestContext,
        claim: &JobClaim,
        stage: StageRecord,
    ) -> Result<(), PortError> {
        let now = self.clock.now();
        let organization_id = ctx.organization_id.as_str().to_string();
        let conn = lock(&self.store)?;
        let tx = conn.unchecked_transaction().map_err(backend)?;
        Self::sync_check_claim(&tx, ctx, claim, now.as_str())?;

        // 阶段 upsert（按 stage_id，主键冲突即更新——与内存实现的
        // `find + replace or push` 同口径）。
        let stage_execution_ids = serde_json::to_string(&stage.execution_ids)
            .map_err(|_| backend("stage execution ids serialization failed"))?;
        tx.execute(
            "INSERT INTO job_stages (
               organization_id, job_id, stage_id, kind, claimed_by,
               execution_ids_json, started_at, finished_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT (organization_id, job_id, stage_id) DO UPDATE SET
               kind = excluded.kind,
               claimed_by = excluded.claimed_by,
               execution_ids_json = excluded.execution_ids_json,
               started_at = excluded.started_at,
               finished_at = excluded.finished_at",
            params![
                organization_id,
                claim.job_id.as_str(),
                stage.stage_id.as_str(),
                stage.kind,
                stage
                    .claimed_by
                    .as_ref()
                    .map(|worker| worker.as_str().to_string()),
                stage_execution_ids,
                stage.started_at.as_ref().map(|ts| ts.as_str().to_string()),
                stage
                    .finished_at
                    .as_ref()
                    .map(|ts| ts.as_str().to_string()),
            ],
        )
        .map_err(backend)?;

        // 视图合并：execution_ids 取并集（与内存实现同口径），stage 置为当前阶段。
        let record = Self::sync_get_db_row(&tx, &organization_id, claim.job_id.as_str())?
            .ok_or_else(|| PortError::NotFound(claim.job_id.as_str().to_string()))?
            .record;
        let mut execution_ids = record.view.execution_ids.clone();
        for execution_id in &stage.execution_ids {
            if !execution_ids.contains(execution_id) {
                execution_ids.push(execution_id.clone());
            }
        }
        let execution_ids_json = serde_json::to_string(&execution_ids)
            .map_err(|_| backend("execution ids serialization failed"))?;
        tx.execute(
            "UPDATE jobs SET stage = ?3, execution_ids_json = ?4, updated_at = ?5
             WHERE organization_id = ?1 AND job_id = ?2",
            params![
                organization_id,
                claim.job_id.as_str(),
                stage.stage_id.as_str(),
                execution_ids_json,
                now.as_str(),
            ],
        )
        .map_err(backend)?;
        tx.commit().map_err(backend)?;
        Ok(())
    }

    fn sync_record_commit_boundary(
        &self,
        ctx: &RequestContext,
        claim: &JobClaim,
        boundary: CommitBoundary,
    ) -> Result<(), PortError> {
        let now = self.clock.now();
        let organization_id = ctx.organization_id.as_str().to_string();
        let conn = lock(&self.store)?;
        let tx = conn.unchecked_transaction().map_err(backend)?;
        Self::sync_check_claim(&tx, ctx, claim, now.as_str())?;

        // 边界的 durable 投影：落库后不再只由 checkpoint 兜底（§3.6）。
        let boundary_json = serde_json::to_string(&boundary)
            .map_err(|_| backend("boundary serialization failed"))?;
        let next_index: i64 = tx
            .query_row(
                "SELECT COALESCE(MAX(boundary_index) + 1, 0) FROM job_commit_boundaries
                 WHERE organization_id = ?1 AND job_id = ?2",
                params![organization_id, claim.job_id.as_str()],
                |row| row.get(0),
            )
            .map_err(backend)?;
        tx.execute(
            "INSERT INTO job_commit_boundaries (
               organization_id, job_id, boundary_index, boundary_json
             ) VALUES (?1, ?2, ?3, ?4)",
            params![
                organization_id,
                claim.job_id.as_str(),
                next_index,
                boundary_json,
            ],
        )
        .map_err(backend)?;
        tx.execute(
            "UPDATE jobs SET updated_at = ?3
             WHERE organization_id = ?1 AND job_id = ?2",
            params![organization_id, claim.job_id.as_str(), now.as_str()],
        )
        .map_err(backend)?;
        tx.commit().map_err(backend)?;
        Ok(())
    }

    fn sync_compare_and_set_state(
        &self,
        ctx: &RequestContext,
        claim: &JobClaim,
        expected: JobStateVersion,
        next: JobState,
    ) -> Result<JobRecord, PortError> {
        let now = self.clock.now();
        let organization_id = ctx.organization_id.as_str().to_string();
        let conn = lock(&self.store)?;
        let tx = conn.unchecked_transaction().map_err(backend)?;
        let row = Self::sync_check_claim(&tx, ctx, claim, now.as_str())?;

        if row.record.state_version.get() != expected.get() {
            return Err(PortError::CasConflict {
                entity: "job",
                id: claim.job_id.as_str().to_string(),
            });
        }
        let next_version = row
            .record
            .state_version
            .get()
            .checked_add(1)
            .ok_or_else(|| backend("state version exhausted"))?;
        // 终态清空 claim（成对置 NULL 的 SQLite 等价物，§3.6 ck_jobs_claim）。
        let (worker, stage_id, claimed_at, expires, generation) = if next.is_terminal() {
            (None, None, None, None, 0i64)
        } else {
            (
                row.core.worker_id,
                row.core.claim_stage_id,
                row.core.claimed_at,
                row.core.claim_expires_at,
                row.core.claim_generation,
            )
        };
        tx.execute(
            "UPDATE jobs SET state = ?3, state_version = ?4, updated_at = ?5,
                    worker_id = ?6, claim_stage_id = ?7, claimed_at = ?8,
                    claim_expires_at = ?9, claim_generation = ?10
             WHERE organization_id = ?1 AND job_id = ?2 AND state_version = ?11",
            params![
                organization_id,
                claim.job_id.as_str(),
                state_name(next),
                next_version as i64,
                now.as_str(),
                worker,
                stage_id,
                claimed_at,
                expires,
                generation,
                row.record.state_version.get() as i64,
            ],
        )
        .map_err(backend)?;
        tx.commit().map_err(backend)?;

        Self::sync_get(&conn, &organization_id, claim.job_id.as_str())?
            .ok_or_else(|| PortError::NotFound(claim.job_id.as_str().to_string()))
    }

    fn sync_claim(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
        worker: &WorkerId,
    ) -> Result<JobClaim, PortError> {
        let now = self.clock.now();
        let organization_id = ctx.organization_id.as_str().to_string();
        let conn = lock(&self.store)?;
        let tx = conn.unchecked_transaction().map_err(backend)?;
        let row = Self::sync_get_db_row(&tx, &organization_id, job_id.as_str())?
            .ok_or_else(|| PortError::NotFound(job_id.as_str().to_string()))?;

        if row.record.view.state.is_terminal() {
            return Err(PortError::CasConflict {
                entity: "job",
                id: job_id.as_str().to_string(),
            });
        }
        // running 且租约未到期 → 别的 worker 在岗，拒绝抢占；Queued 或租约
        // 已过期允许接管（与内存实现同口径）。
        if row.record.view.state == JobState::Running {
            let current_expires = row
                .core
                .claim_expires_at
                .as_deref()
                .ok_or(PortError::StaleClaim)?;
            if now.as_str() <= current_expires {
                return Err(PortError::StaleClaim);
            }
        }
        let generation = row.core.claim_generation + 1;
        let expires_at = after_seconds(&now, self.claim_ttl_secs).map_err(PortError::from)?;

        // claim 的 stage_id：首阶段（按插入序）；尚无阶段时用占位 `*`
        // （与内存实现 `stages.first().unwrap_or("*")` 同口径）。
        let claim_stage_id: Option<String> = tx
            .query_row(
                "SELECT stage_id FROM job_stages
                 WHERE organization_id = ?1 AND job_id = ?2 ORDER BY rowid LIMIT 1",
                params![organization_id, job_id.as_str()],
                |row| row.get(0),
            )
            .optional()
            .map_err(backend)?;
        let claim_stage_id = claim_stage_id.unwrap_or_else(|| "*".to_string());

        tx.execute(
            "UPDATE jobs SET state = 'running', state_version = state_version + 1,
                    updated_at = ?3, worker_id = ?4, claim_stage_id = ?5,
                    claimed_at = ?6, claim_expires_at = ?7, claim_generation = ?8
             WHERE organization_id = ?1 AND job_id = ?2",
            params![
                organization_id,
                job_id.as_str(),
                now.as_str(),
                worker.as_str(),
                claim_stage_id,
                now.as_str(),
                expires_at.as_str(),
                generation,
            ],
        )
        .map_err(backend)?;
        tx.commit().map_err(backend)?;

        Ok(JobClaim {
            job_id: job_id.clone(),
            stage_id: StageId::new(&claim_stage_id),
            worker_id: worker.clone(),
            claimed_at: now.clone(),
            claim_generation: Counter::new(generation as u64),
            expires_at,
        })
    }

    fn sync_renew(&self, claim: &JobClaim) -> Result<JobClaim, PortError> {
        let now = self.clock.now();
        let conn = lock(&self.store)?;
        let tx = conn.unchecked_transaction().map_err(backend)?;
        // 续约只凭 claim 自身判定，不做组织比对（端口契约）。
        // 拒绝理由：行不存在、行上没有 claim、worker 不符、generation 不符、
        // 租约过期——与内存实现逐条一致。
        let row = conn
            .query_row(
                &format!("SELECT {CLAIM_COLUMNS} FROM jobs WHERE job_id = ?1"),
                params![claim.job_id.as_str()],
                |row| {
                    Ok(JobRowCore {
                        organization_id: String::new(),
                        job_id: String::new(),
                        worker_id: row.get(0)?,
                        claim_stage_id: row.get(1)?,
                        claimed_at: row.get(2)?,
                        claim_expires_at: row.get(3)?,
                        claim_generation: row.get(4)?,
                    })
                },
            )
            .optional()
            .map_err(backend)?
            .ok_or_else(|| PortError::NotFound(claim.job_id.as_str().to_string()))?;
        let current_worker = row.worker_id.as_deref().ok_or(PortError::StaleClaim)?;
        if current_worker != claim.worker_id.as_str()
            || row.claim_generation != claim.claim_generation.get() as i64
        {
            return Err(PortError::StaleClaim);
        }
        let current_expires = row
            .claim_expires_at
            .as_deref()
            .ok_or(PortError::StaleClaim)?;
        if now.as_str() > current_expires {
            return Err(PortError::StaleClaim);
        }
        let expires_at = after_seconds(&now, self.claim_ttl_secs).map_err(PortError::from)?;
        tx.execute(
            "UPDATE jobs SET claimed_at = ?3, claim_expires_at = ?4
             WHERE job_id = ?1 AND worker_id = ?2 AND claim_generation = ?5",
            params![
                claim.job_id.as_str(),
                claim.worker_id.as_str(),
                now.as_str(),
                expires_at.as_str(),
                claim.claim_generation.get() as i64,
            ],
        )
        .map_err(backend)?;
        tx.commit().map_err(backend)?;
        Ok(JobClaim {
            job_id: claim.job_id.clone(),
            stage_id: claim.stage_id.clone(),
            worker_id: claim.worker_id.clone(),
            claimed_at: now,
            claim_generation: claim.claim_generation.clone(),
            expires_at,
        })
    }

    fn sync_save_checkpoint(
        &self,
        ctx: &RequestContext,
        claim: &JobClaim,
        cp: Checkpoint,
    ) -> Result<(), PortError> {
        let now = self.clock.now();
        let organization_id = ctx.organization_id.as_str().to_string();
        let conn = lock(&self.store)?;
        let tx = conn.unchecked_transaction().map_err(backend)?;
        Self::sync_check_claim(&tx, ctx, claim, now.as_str())?;

        let committed_json = serde_json::to_string(&cp.committed)
            .map_err(|_| backend("committed serialization failed"))?;
        let evidence_json = serde_json::to_string(&cp.verification_evidence)
            .map_err(|_| backend("evidence serialization failed"))?;
        let inserted = tx
            .execute(
                "INSERT INTO job_checkpoints (
                   organization_id, job_id, checkpoint_version,
                   stable_target_fingerprint, committed_json,
                   verification_evidence_json, recovery_policy, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT (organization_id, job_id, checkpoint_version) DO NOTHING",
                params![
                    organization_id,
                    cp.job_id.as_str(),
                    cp.state_version.get() as i64,
                    cp.stable_target_fingerprint,
                    committed_json,
                    evidence_json,
                    cp.recovery_policy,
                    now.as_str(),
                ],
            )
            .map_err(backend)?;
        if inserted == 0 {
            return Err(PortError::CasConflict {
                entity: "job_checkpoint",
                id: cp.job_id.as_str().to_string(),
            });
        }
        tx.commit().map_err(backend)?;
        Ok(())
    }

    fn sync_list_recoverable(
        &self,
        ctx: &RequestContext,
        filter: RecoveryFilter,
    ) -> Result<Vec<JobRecord>, PortError> {
        let organization_id = ctx.organization_id.as_str().to_string();
        let conn = lock(&self.store)?;
        // 恢复扫描按 ctx 的组织限定；只产出待核验候选（queued/running/cancelled
        // 或带 pending_verification_reason），绝不重放副作用阶段。与内存实现
        // 同口径：kinds 空表示不过滤；older_than 两腿一致暂未消费。
        let mut sql = format!(
            "SELECT {JOB_COLUMNS}, {CLAIM_COLUMNS} FROM jobs
             WHERE organization_id = ?1
               AND (state IN ('queued', 'running', 'cancelled')
                    OR pending_verification_reason IS NOT NULL)"
        );
        let mut bind: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(organization_id.clone())];
        if !filter.kinds.is_empty() {
            let placeholders = (0..filter.kinds.len())
                .map(|index| format!("?{}", bind.len() + 1 + index))
                .collect::<Vec<_>>()
                .join(", ");
            sql.push_str(&format!(" AND kind IN ({placeholders})"));
            for kind in &filter.kinds {
                bind.push(Box::new(kind.clone()));
            }
        }
        sql.push_str(" ORDER BY created_at, job_id");

        let mut stmt = conn.prepare(&sql).map_err(backend)?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(bind.iter()), db_row_from_row)
            .map_err(backend)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(backend)?;
        let mut out = Vec::with_capacity(rows.len());
        for mut row in rows {
            row.record.stages = row.core.stages(&conn)?;
            out.push(row.record);
        }
        Ok(out)
    }
}

impl JobRowCore {
    /// 该 job 的阶段列表（按插入序，`rowid` 承载内存实现的 `Vec` 顺序）。
    fn stages(&self, conn: &rusqlite::Connection) -> Result<Vec<StageRecord>, PortError> {
        stages_from_db(conn, &self.organization_id, &self.job_id)
    }
}

fn backend(source: impl std::fmt::Display) -> PortError {
    PortError::BackendUnavailable(source.to_string())
}

fn lock(
    store: &AdminStore,
) -> Result<std::sync::MutexGuard<'_, rusqlite::Connection>, PortError> {
    store
        .lock_for_adapter()
        .map_err(|source| PortError::BackendUnavailable(source.to_string()))
}

fn state_name(state: JobState) -> &'static str {
    match state {
        JobState::Queued => "queued",
        JobState::Running => "running",
        JobState::Succeeded => "succeeded",
        JobState::Failed => "failed",
        JobState::Cancelled => "cancelled",
    }
}

fn state_from_name(name: &str) -> Option<JobState> {
    match name {
        "queued" => Some(JobState::Queued),
        "running" => Some(JobState::Running),
        "succeeded" => Some(JobState::Succeeded),
        "failed" => Some(JobState::Failed),
        "cancelled" => Some(JobState::Cancelled),
        _ => None,
    }
}

/// 列 0..=16 是端口投影（`record_from_row`），列 17..=21 是 claim 核心列。
fn db_row_from_row(row: &Row<'_>) -> rusqlite::Result<JobDbRow> {
    let record = record_from_row(row)?;
    let core = JobRowCore {
        organization_id: record
            .definition
            .organization_id
            .as_str()
            .to_string(),
        job_id: record.view.job_id.as_str().to_string(),
        worker_id: row.get(17)?,
        claim_stage_id: row.get(18)?,
        claimed_at: row.get(19)?,
        claim_expires_at: row.get(20)?,
        claim_generation: row.get(21)?,
    };
    Ok(JobDbRow { record, core })
}

fn record_from_row(row: &Row<'_>) -> rusqlite::Result<JobRecord> {
    let organization_id: String = row.get(0)?;
    let job_id: String = row.get(1)?;
    let kind: String = row.get(2)?;
    let state_name: String = row.get(3)?;
    let state_version: i64 = row.get(4)?;
    let stage: Option<String> = row.get(5)?;
    let owner_json: String = row.get(6)?;
    let payload_json: String = row.get(7)?;
    let created_at: String = row.get(8)?;
    let updated_at: String = row.get(9)?;
    let execution_ids_json: String = row.get(10)?;
    let artifact_ids_json: String = row.get(11)?;
    let effect_outcome: Option<String> = row.get(12)?;
    let cancel_requested: i64 = row.get(13)?;
    let pending_verification_reason: Option<String> = row.get(14)?;
    let error: Option<String> = row.get(15)?;
    let progress_json: String = row.get(16)?;

    let invalid = |column: usize, name: &str| {
        rusqlite::Error::FromSqlConversionFailure(
            column,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                name.to_string(),
            )),
        )
    };

    let state = state_from_name(&state_name).ok_or_else(|| invalid(3, "job state"))?;
    let owner: OwnerRef =
        serde_json::from_str(&owner_json).map_err(|_| invalid(6, "owner json"))?;
    let payload: serde_json::Value =
        serde_json::from_str(&payload_json).map_err(|_| invalid(7, "payload json"))?;
    let execution_ids: Vec<ExecutionId> =
        serde_json::from_str(&execution_ids_json).map_err(|_| invalid(10, "execution ids"))?;
    let artifact_ids: Vec<ArtifactId> =
        serde_json::from_str(&artifact_ids_json).map_err(|_| invalid(11, "artifact ids"))?;
    // EffectOutcome 是 camelCase 单元枚举（"notStarted"/"completed"/…）：
    // 列存变体名，读取时按 JSON 字符串字面量还原。
    let effect_outcome = match effect_outcome.as_deref() {
        None => None,
        Some(name) => Some(
            serde_json::from_str::<EffectOutcome>(&format!("\"{name}\""))
                .map_err(|_| invalid(12, "effect outcome"))?,
        ),
    };
    let progress: JobProgress =
        serde_json::from_str(&progress_json).map_err(|_| invalid(16, "progress"))?;

    Ok(JobRecord {
        view: JobView {
            job_id: JobId::new(&job_id),
            kind: kind.clone(),
            state,
            stage,
            execution_ids,
            artifact_ids,
            created_at: Timestamp::new(&created_at),
            updated_at: Timestamp::new(&updated_at),
            effect_outcome,
            cancel_requested: cancel_requested != 0,
            pending_verification_reason,
            error,
            progress,
        },
        definition: JobDefinition {
            job_id: JobId::new(&job_id),
            organization_id: OrganizationId::new(&organization_id),
            kind,
            owner,
            payload,
            created_at: Timestamp::new(&created_at),
        },
        stages: Vec::new(),
        state_version: JobStateVersion::new(state_version as u64),
    })
}

fn stages_from_db(
    conn: &rusqlite::Connection,
    organization_id: &str,
    job_id: &str,
) -> Result<Vec<StageRecord>, PortError> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {STAGE_COLUMNS} FROM job_stages
             WHERE organization_id = ?1 AND job_id = ?2 ORDER BY rowid"
        ))
        .map_err(backend)?;
    let rows = stmt
        .query_map(params![organization_id, job_id], |row| {
            let stage_id: String = row.get(0)?;
            let kind: String = row.get(1)?;
            let claimed_by: Option<String> = row.get(2)?;
            let execution_ids_json: String = row.get(3)?;
            let started_at: Option<String> = row.get(4)?;
            let finished_at: Option<String> = row.get(5)?;
            let execution_ids: Vec<ExecutionId> = serde_json::from_str(&execution_ids_json)
                .map_err(|_| {
                    rusqlite::Error::FromSqlConversionFailure(
                        3,
                        rusqlite::types::Type::Text,
                        Box::new(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "stage execution ids",
                        )),
                    )
                })?;
            Ok(StageRecord {
                job_id: JobId::new(job_id),
                stage_id: StageId::new(&stage_id),
                kind,
                claimed_by: claimed_by.map(WorkerId::new),
                execution_ids,
                started_at: started_at.map(|ts| Timestamp::new(&ts)),
                finished_at: finished_at.map(|ts| Timestamp::new(&ts)),
            })
        })
        .map_err(backend)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(backend)?;
    Ok(rows)
}

#[async_trait]
impl JobRepository for ServerJobs {
    async fn accept(
        &self,
        ctx: &RequestContext,
        definition: JobDefinition,
        idem: &IdempotencyKey,
    ) -> Result<JobRecord, PortError> {
        let this = self.clone();
        let ctx = ctx.clone();
        let idem = idem.clone();
        tokio::task::spawn_blocking(move || this.sync_accept(&ctx, definition, &idem))
            .await
            .map_err(backend)?
    }

    async fn get(&self, ctx: &RequestContext, job_id: JobId) -> Result<JobRecord, PortError> {
        let this = self.clone();
        let ctx = ctx.clone();
        tokio::task::spawn_blocking(move || {
            let conn = lock(&this.store)?;
            Self::sync_get(&conn, ctx.organization_id.as_str(), job_id.as_str())?
                .ok_or_else(|| PortError::NotFound(job_id.as_str().to_string()))
        })
        .await
        .map_err(backend)?
    }

    async fn list(
        &self,
        ctx: &RequestContext,
        filter: JobFilter,
    ) -> Result<Vec<JobRecord>, PortError> {
        let this = self.clone();
        let ctx = ctx.clone();
        tokio::task::spawn_blocking(move || this.sync_list(&ctx, filter))
            .await
            .map_err(backend)?
    }

    async fn record_stage(
        &self,
        ctx: &RequestContext,
        claim: &JobClaim,
        stage: StageRecord,
    ) -> Result<(), PortError> {
        let this = self.clone();
        let ctx = ctx.clone();
        let claim = claim.clone();
        tokio::task::spawn_blocking(move || this.sync_record_stage(&ctx, &claim, stage))
            .await
            .map_err(backend)?
    }

    async fn record_commit_boundary(
        &self,
        ctx: &RequestContext,
        claim: &JobClaim,
        boundary: CommitBoundary,
    ) -> Result<(), PortError> {
        let this = self.clone();
        let ctx = ctx.clone();
        let claim = claim.clone();
        tokio::task::spawn_blocking(move || {
            this.sync_record_commit_boundary(&ctx, &claim, boundary)
        })
        .await
        .map_err(backend)?
    }

    async fn compare_and_set_state(
        &self,
        ctx: &RequestContext,
        claim: &JobClaim,
        expected: JobStateVersion,
        next: JobState,
    ) -> Result<JobRecord, PortError> {
        let this = self.clone();
        let ctx = ctx.clone();
        let claim = claim.clone();
        tokio::task::spawn_blocking(move || {
            this.sync_compare_and_set_state(&ctx, &claim, expected, next)
        })
        .await
        .map_err(backend)?
    }

    async fn claim(
        &self,
        ctx: &RequestContext,
        job_id: JobId,
        worker: WorkerId,
    ) -> Result<JobClaim, PortError> {
        let this = self.clone();
        let ctx = ctx.clone();
        tokio::task::spawn_blocking(move || this.sync_claim(&ctx, &job_id, &worker))
            .await
            .map_err(backend)?
    }

    async fn renew(&self, claim: &JobClaim) -> Result<JobClaim, PortError> {
        let this = self.clone();
        let claim = claim.clone();
        tokio::task::spawn_blocking(move || this.sync_renew(&claim))
            .await
            .map_err(backend)?
    }

    async fn save_checkpoint(
        &self,
        ctx: &RequestContext,
        claim: &JobClaim,
        cp: Checkpoint,
    ) -> Result<(), PortError> {
        let this = self.clone();
        let ctx = ctx.clone();
        let claim = claim.clone();
        tokio::task::spawn_blocking(move || this.sync_save_checkpoint(&ctx, &claim, cp))
            .await
            .map_err(backend)?
    }

    async fn list_recoverable(
        &self,
        ctx: &RequestContext,
        filter: RecoveryFilter,
    ) -> Result<Vec<JobRecord>, PortError> {
        let this = self.clone();
        let ctx = ctx.clone();
        tokio::task::spawn_blocking(move || this.sync_list_recoverable(&ctx, filter))
            .await
            .map_err(backend)?
    }
}
