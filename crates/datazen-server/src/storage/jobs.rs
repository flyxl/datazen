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

        Self::sync_get(&conn, &organization_id, &job_id)?.ok_or_else(|| PortError::NotFound(job_id))
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

        let mut sql =
            format!("SELECT {JOB_COLUMNS}, {CLAIM_COLUMNS} FROM jobs WHERE organization_id = ?1");
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
            let owner_json =
                serde_json::to_string(owner).map_err(|_| backend("owner serialization failed"))?;
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
                stage.finished_at.as_ref().map(|ts| ts.as_str().to_string()),
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

fn lock(store: &AdminStore) -> Result<std::sync::MutexGuard<'_, rusqlite::Connection>, PortError> {
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
        organization_id: record.definition.organization_id.as_str().to_string(),
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

// ─────────────────────────────────────────────────────────────────────────
// W8 刀3b：`JobRuntimeRepository`（schema_diff 域的 job 运行时面）。
//
// 桌面对照：`datazen-desktop/src/store/app_db/jobs/{runtime_ops,results,
// access,owner,writes}.rs`（`JOB_SCHEMA_V2` 的运行时方法族）。语义逐方法
// 对齐，形状差异四处（全部由两宿主的表形状决定，行为不变）：
//
// 1. **组织作用域**：桌面按 `organization_id + owner_principal_id` 过滤
//    ctx 作用域方法；服务端 jobs 家族的可见性口径是**组织级**（v3 表 +
//    jobs 域读面 + `TaskCenter` 同口径，CM-05），本实现按
//    `(organization_id, job_id)` 复合键过滤——claim 作用域方法的栅栏
//    仍是 worker + generation + 租约（`sync_check_claim`，越组织写按
//    `StaleClaim` 拒绝）。
// 2. **`error` 列 ≡ 桌面 `result_error_code`**：同一「终态错误码」语义
//    的两宿主列名。
// 3. **幂等回执不开表**：桌面 `job_idempotency_receipts` 的服务端对偶
//    是受理行自持的 `idem_scope_key`（v3 部分唯一索引 `uq_jobs_idem`），
//    `receipt_for` 直查受理行。桌面 `cancel_requested_at` /
//    `claim_client_instance_id` 两列不移植：被移植的读路径都不消费它们。
// 4. **checkpoint 是结构化列**（v3 `job_checkpoints` 的六列）而非桌面的
//    `checkpoint_json` blob——`latest_checkpoint` 从结构化列重建
//    `Checkpoint`（`save_checkpoint` 的逆投影，同一份列词汇）。
// ─────────────────────────────────────────────────────────────────────────

/// 桌面 codec.rs `safe_identifier` 的同款副本（跨 crate 不导入，逻辑钉死）。
fn safe_identifier(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 512
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:-".contains(&byte))
}

/// 桌面 writes.rs `safe_marker` 的同款副本（提交边界/证据词汇的校验）。
fn safe_marker(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 2048
        && !value.chars().any(char::is_control)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:-/#=".contains(&byte))
}

fn validate_code(value: &str) -> Result<(), PortError> {
    if safe_identifier(value) && value.len() <= 96 {
        Ok(())
    } else {
        Err(PortError::BackendUnavailable(
            "job result code is invalid".into(),
        ))
    }
}

fn validate_artifacts(artifacts: &[ArtifactId]) -> Result<(), PortError> {
    if artifacts.len() <= 4096
        && artifacts
            .iter()
            .all(|artifact| safe_identifier(artifact.as_str()))
    {
        Ok(())
    } else {
        Err(PortError::BackendUnavailable(
            "job artifact reference is invalid".into(),
        ))
    }
}

/// 枚举 → 列文本（EffectOutcome 等 camelCase 单元枚举的 serde 字面量）。
fn enum_text<T: serde::Serialize>(value: &T) -> Result<String, PortError> {
    serde_json::to_value(value)
        .map_err(|_| PortError::BackendUnavailable("job enum encoding failed".into()))?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| PortError::BackendUnavailable("job enum has invalid shape".into()))
}

fn verdict_text(verdict: datazen::jobs::domain::job::JobRecoveryVerdict) -> &'static str {
    match verdict {
        datazen::jobs::domain::job::JobRecoveryVerdict::NotExecuted => "notExecuted",
        datazen::jobs::domain::job::JobRecoveryVerdict::PendingVerification => "pendingVerification",
        datazen::jobs::domain::job::JobRecoveryVerdict::ResumeAfterVerify => "resumeAfterVerify",
        datazen::jobs::domain::job::JobRecoveryVerdict::Reject => "reject",
        datazen::jobs::domain::job::JobRecoveryVerdict::RequireManualReview => {
            "requireManualReview"
        }
    }
}

fn validate_boundary(boundary: &CommitBoundary) -> Result<(), PortError> {
    if !safe_marker(boundary.stage_id.as_str())
        || !safe_marker(&boundary.stable_target_fingerprint)
        || boundary
            .operation_id
            .as_deref()
            .is_some_and(|id| !safe_marker(id))
        || boundary
            .batch_id
            .as_deref()
            .is_some_and(|id| !safe_marker(id))
        || boundary
            .payload_digest
            .as_deref()
            .is_some_and(|id| !safe_marker(id))
        || boundary.evidence.iter().any(|item| !safe_marker(item))
    {
        return Err(PortError::BackendUnavailable(
            "commit boundary is outside the safe format".into(),
        ));
    }
    Ok(())
}

fn prune_expired_artifacts(conn: &rusqlite::Connection, now: &str) -> Result<(), PortError> {
    conn.execute("DELETE FROM job_artifact_refs WHERE expires_at <= ?1", params![now])
        .map(|_| ())
        .map_err(backend)
}

fn read_boundaries(
    conn: &rusqlite::Connection,
    organization_id: &str,
    job_id: &JobId,
) -> Result<Vec<CommitBoundary>, PortError> {
    let mut stmt = conn
        .prepare(
            "SELECT boundary_json FROM job_commit_boundaries \
             WHERE organization_id = ?1 AND job_id = ?2 ORDER BY boundary_index",
        )
        .map_err(backend)?;
    let rows = stmt
        .query_map(params![organization_id, job_id.as_str()], |row| {
            row.get::<_, String>(0)
        })
        .map_err(backend)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(backend)?;
    rows.into_iter()
        .map(|json| serde_json::from_str(&json).map_err(|_| backend("boundary decode failed")))
        .collect()
}

fn read_domain_results(
    conn: &rusqlite::Connection,
    organization_id: &str,
    job_id: &JobId,
) -> Result<Vec<datazen::jobs::domain::job::JobDomainResult>, PortError> {
    let mut stmt = conn
        .prepare(
            "SELECT result_json FROM job_domain_results \
             WHERE organization_id = ?1 AND job_id = ?2 ORDER BY stage_id",
        )
        .map_err(backend)?;
    let rows = stmt
        .query_map(params![organization_id, job_id.as_str()], |row| {
            row.get::<_, String>(0)
        })
        .map_err(backend)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(backend)?;
    rows.into_iter()
        .map(|json| serde_json::from_str(&json).map_err(|_| backend("domain result decode failed")))
        .collect()
}

impl ServerJobs {
    fn sync_cancel_poll(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
    ) -> Result<datazen::jobs::engine::memory_repository::CancelPollSnapshot, PortError> {
        let conn = lock(&self.store)?;
        let row = conn
            .query_row(
                "SELECT state, cancel_requested FROM jobs \
                 WHERE organization_id = ?1 AND job_id = ?2",
                params![ctx.organization_id.as_str(), job_id.as_str()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()
            .map_err(backend)?
            .ok_or_else(|| PortError::NotFound(job_id.as_str().into()))?;
        Ok(
            datazen::jobs::engine::memory_repository::CancelPollSnapshot {
                cancel_requested: row.1 != 0,
                terminal: matches!(row.0.as_str(), "succeeded" | "failed" | "cancelled"),
            },
        )
    }

    fn sync_request_cancel(&self, ctx: &RequestContext, job_id: &JobId) -> Result<JobRecord, PortError> {
        let now = self.clock.now();
        let organization_id = ctx.organization_id.as_str().to_string();
        let conn = lock(&self.store)?;
        let tx = conn.unchecked_transaction().map_err(backend)?;
        let changed = tx
            .execute(
                "UPDATE jobs SET cancel_requested = 1, updated_at = ?1 \
                 WHERE organization_id = ?2 AND job_id = ?3 AND state IN ('queued','running')",
                params![now.as_str(), organization_id, job_id.as_str()],
            )
            .map_err(backend)?;
        if changed == 0 {
            let exists = tx
                .query_row(
                    "SELECT 1 FROM jobs WHERE organization_id = ?1 AND job_id = ?2",
                    params![organization_id, job_id.as_str()],
                    |_| Ok(()),
                )
                .optional()
                .map_err(backend)?;
            if exists.is_none() {
                return Err(PortError::NotFound(job_id.as_str().into()));
            }
            return Err(PortError::CasConflict {
                entity: "job",
                id: job_id.as_str().into(),
            });
        }
        let record = Self::sync_get(&tx, &organization_id, job_id.as_str())?
            .ok_or_else(|| PortError::NotFound(job_id.as_str().into()))?;
        tx.commit().map_err(backend)?;
        Ok(record)
    }

    fn sync_confirm_cancelled(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
        outcome: EffectOutcome,
    ) -> Result<JobRecord, PortError> {
        let now = self.clock.now();
        let outcome_text = enum_text(&outcome)?;
        let organization_id = ctx.organization_id.as_str().to_string();
        let conn = lock(&self.store)?;
        let tx = conn.unchecked_transaction().map_err(backend)?;
        let changed = tx
            .execute(
                "UPDATE jobs SET state = 'cancelled', effect_outcome = ?1, \
                 state_version = state_version + 1, updated_at = ?2, \
                 worker_id = NULL, claim_stage_id = NULL, claim_expires_at = NULL, claimed_at = NULL \
                 WHERE organization_id = ?3 AND job_id = ?4 AND state = 'queued' \
                 AND cancel_requested = 1 AND worker_id IS NULL",
                params![outcome_text, now.as_str(), organization_id, job_id.as_str()],
            )
            .map_err(backend)?;
        if changed != 1 {
            return Err(not_found_or_conflict(&tx, &organization_id, job_id)?);
        }
        let record = Self::sync_get(&tx, &organization_id, job_id.as_str())?
            .ok_or_else(|| PortError::NotFound(job_id.as_str().into()))?;
        tx.commit().map_err(backend)?;
        Ok(record)
    }

    fn sync_mark_failed_unstarted(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
        reason_code: &str,
    ) -> Result<JobRecord, PortError> {
        validate_code(reason_code)?;
        let now = self.clock.now();
        let organization_id = ctx.organization_id.as_str().to_string();
        let conn = lock(&self.store)?;
        let tx = conn.unchecked_transaction().map_err(backend)?;
        let changed = tx
            .execute(
                "UPDATE jobs SET state = 'failed', effect_outcome = 'notStarted', error = ?1, \
                 pending_verification_reason = NULL, state_version = state_version + 1, \
                 updated_at = ?2 \
                 WHERE organization_id = ?3 AND job_id = ?4 AND state = 'queued' \
                 AND worker_id IS NULL",
                params![reason_code, now.as_str(), organization_id, job_id.as_str()],
            )
            .map_err(backend)?;
        if changed != 1 {
            return Err(not_found_or_conflict(&tx, &organization_id, job_id)?);
        }
        let recovery = datazen::jobs::domain::job::JobRecoveryResult {
            verdict: datazen::jobs::domain::job::JobRecoveryVerdict::NotExecuted,
            resume_through: None,
            reason_code: Some(reason_code.to_owned()),
        };
        tx.execute(
            "INSERT INTO job_result_details (organization_id, job_id, recovery_json) \
             VALUES (?1, ?2, ?3) \
             ON CONFLICT (organization_id, job_id) DO UPDATE SET recovery_json = excluded.recovery_json",
            params![
                organization_id,
                job_id.as_str(),
                serde_json::to_string(&recovery).map_err(|_| backend("recovery encode failed"))?
            ],
        )
        .map_err(backend)?;
        let record = Self::sync_get(&tx, &organization_id, job_id.as_str())?
            .ok_or_else(|| PortError::NotFound(job_id.as_str().into()))?;
        tx.commit().map_err(backend)?;
        Ok(record)
    }

    fn sync_mark_pending_verification(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
        reason_code: &str,
    ) -> Result<JobRecord, PortError> {
        validate_code(reason_code)?;
        let now = self.clock.now();
        let organization_id = ctx.organization_id.as_str().to_string();
        let conn = lock(&self.store)?;
        let tx = conn.unchecked_transaction().map_err(backend)?;
        let old_state = tx
            .query_row(
                "SELECT state, worker_id, effect_outcome FROM jobs \
                 WHERE organization_id = ?1 AND job_id = ?2",
                params![organization_id, job_id.as_str()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(backend)?
            .ok_or_else(|| PortError::NotFound(job_id.as_str().into()))?;
        let was_terminal = matches!(
            old_state.0.as_str(),
            "succeeded" | "failed" | "cancelled"
        );
        let outcome = if was_terminal {
            old_state.2.unwrap_or_else(|| "unknown".into())
        } else if old_state.1.is_some() || old_state.0 == "running" {
            "unknown".into()
        } else {
            "notStarted".into()
        };
        let state = if was_terminal {
            old_state.0
        } else {
            "failed".into()
        };
        let version_bump: i64 = if was_terminal { 0 } else { 1 };
        tx.execute(
            "UPDATE jobs SET state = ?1, effect_outcome = ?2, error = ?3, \
             pending_verification_reason = ?3, state_version = state_version + ?4, \
             updated_at = ?5, worker_id = NULL, claim_stage_id = NULL, \
             claim_expires_at = NULL, claimed_at = NULL \
             WHERE organization_id = ?6 AND job_id = ?7",
            params![
                state,
                outcome,
                reason_code,
                version_bump,
                now.as_str(),
                organization_id,
                job_id.as_str()
            ],
        )
        .map_err(backend)?;
        let recovery = datazen::jobs::domain::job::JobRecoveryResult {
            verdict: datazen::jobs::domain::job::JobRecoveryVerdict::PendingVerification,
            resume_through: None,
            reason_code: Some(reason_code.to_string()),
        };
        tx.execute(
            "INSERT INTO job_result_details (organization_id, job_id, recovery_json) \
             VALUES (?1, ?2, ?3) \
             ON CONFLICT (organization_id, job_id) DO UPDATE SET recovery_json = excluded.recovery_json",
            params![
                organization_id,
                job_id.as_str(),
                serde_json::to_string(&recovery).map_err(|_| backend("recovery encode failed"))?
            ],
        )
        .map_err(backend)?;
        let record = Self::sync_get(&tx, &organization_id, job_id.as_str())?
            .ok_or_else(|| PortError::NotFound(job_id.as_str().into()))?;
        tx.commit().map_err(backend)?;
        Ok(record)
    }

    fn sync_record_progress(
        &self,
        ctx: &RequestContext,
        claim: &JobClaim,
        progress: JobProgress,
    ) -> Result<(), PortError> {
        let now = self.clock.now();
        let organization_id = ctx.organization_id.as_str().to_string();
        let progress_json =
            serde_json::to_string(&progress).map_err(|_| backend("progress encode failed"))?;
        let conn = lock(&self.store)?;
        let tx = conn.unchecked_transaction().map_err(backend)?;
        Self::sync_check_claim(&tx, ctx, claim, now.as_str())?;
        tx.execute(
            "UPDATE jobs SET progress_json = ?1, updated_at = ?2 \
             WHERE organization_id = ?3 AND job_id = ?4",
            params![progress_json, now.as_str(), organization_id, claim.job_id.as_str()],
        )
        .map_err(backend)?;
        tx.commit().map_err(backend)?;
        Ok(())
    }

    /// 产物引用 TTL（桌面 `JOB_ARTIFACT_REFERENCE_TTL_SECS` 同款：30 天）。
    const JOB_ARTIFACT_REFERENCE_TTL_SECS: i64 = 30 * 24 * 60 * 60;

    fn insert_artifact_refs(
        tx: &rusqlite::Transaction<'_>,
        organization_id: &str,
        job_id: &str,
        artifacts: &[ArtifactId],
        now: &Timestamp,
        expires: &Timestamp,
    ) -> Result<(), PortError> {
        for artifact in artifacts {
            tx.execute(
                "INSERT INTO job_artifact_refs \
                 (organization_id, job_id, artifact_id, created_at, expires_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5) \
                 ON CONFLICT (organization_id, job_id, artifact_id) DO NOTHING",
                params![
                    organization_id,
                    job_id,
                    artifact.as_str(),
                    now.as_str(),
                    expires.as_str()
                ],
            )
            .map_err(backend)?;
        }
        tx.execute(
            "UPDATE jobs SET updated_at = ?1 WHERE organization_id = ?2 AND job_id = ?3",
            params![now.as_str(), organization_id, job_id],
        )
        .map_err(backend)?;
        Ok(())
    }

    fn sync_record_artifacts(
        &self,
        ctx: &RequestContext,
        claim: &JobClaim,
        artifacts: &[ArtifactId],
    ) -> Result<(), PortError> {
        validate_artifacts(artifacts)?;
        let now = self.clock.now();
        let expires = after_seconds(&now, Self::JOB_ARTIFACT_REFERENCE_TTL_SECS)
            .map_err(PortError::from)?;
        let organization_id = ctx.organization_id.as_str().to_string();
        let conn = lock(&self.store)?;
        let tx = conn.unchecked_transaction().map_err(backend)?;
        prune_expired_artifacts(&tx, now.as_str())?;
        Self::sync_check_claim(&tx, ctx, claim, now.as_str())?;
        Self::insert_artifact_refs(
            &tx,
            &organization_id,
            claim.job_id.as_str(),
            artifacts,
            &now,
            &expires,
        )?;
        tx.commit().map_err(backend)?;
        Ok(())
    }

    fn sync_append_external_artifacts(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
        artifacts: &[ArtifactId],
    ) -> Result<(), PortError> {
        validate_artifacts(artifacts)?;
        let now = self.clock.now();
        let expires = after_seconds(&now, Self::JOB_ARTIFACT_REFERENCE_TTL_SECS)
            .map_err(PortError::from)?;
        let organization_id = ctx.organization_id.as_str().to_string();
        let conn = lock(&self.store)?;
        let tx = conn.unchecked_transaction().map_err(backend)?;
        prune_expired_artifacts(&tx, now.as_str())?;
        let state = tx
            .query_row(
                "SELECT state FROM jobs WHERE organization_id = ?1 AND job_id = ?2",
                params![organization_id, job_id.as_str()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(backend)?
            .ok_or_else(|| PortError::NotFound(job_id.as_str().into()))?;
        if !matches!(state.as_str(), "succeeded" | "failed" | "cancelled") {
            return Err(PortError::CasConflict {
                entity: "job_artifact",
                id: job_id.as_str().into(),
            });
        }
        Self::insert_artifact_refs(
            &tx,
            &organization_id,
            job_id.as_str(),
            artifacts,
            &now,
            &expires,
        )?;
        tx.commit().map_err(backend)?;
        Ok(())
    }

    fn sync_record_domain_result(
        &self,
        ctx: &RequestContext,
        claim: &JobClaim,
        result: datazen::jobs::domain::job::JobDomainResult,
    ) -> Result<(), PortError> {
        datazen::jobs::runtime_repository::validate_job_domain_result(&result)?;
        let serialized =
            serde_json::to_string(&result).map_err(|_| backend("domain result encode failed"))?;
        if serialized.len() > 512 * 1024 {
            return Err(PortError::QuotaExceeded(
                "job result exceeds the local size limit".into(),
            ));
        }
        let now = self.clock.now();
        let organization_id = ctx.organization_id.as_str().to_string();
        let conn = lock(&self.store)?;
        let tx = conn.unchecked_transaction().map_err(backend)?;
        Self::sync_check_claim(&tx, ctx, claim, now.as_str())?;
        let active_stage = tx
            .query_row(
                "SELECT stage FROM jobs WHERE organization_id = ?1 AND job_id = ?2",
                params![organization_id, claim.job_id.as_str()],
                |row| row.get::<_, Option<String>>(0),
            )
            .map_err(backend)?;
        if active_stage.as_deref() != Some(result.stage_id.as_str()) {
            return Err(PortError::StaleClaim);
        }
        tx.execute(
            "INSERT INTO job_domain_results (organization_id, job_id, stage_id, result_json) \
             VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT (organization_id, job_id, stage_id) DO UPDATE SET result_json = excluded.result_json",
            params![
                organization_id,
                claim.job_id.as_str(),
                result.stage_id.as_str(),
                serialized
            ],
        )
        .map_err(backend)?;
        tx.execute(
            "UPDATE jobs SET updated_at = ?1 WHERE organization_id = ?2 AND job_id = ?3",
            params![now.as_str(), organization_id, claim.job_id.as_str()],
        )
        .map_err(backend)?;
        tx.commit().map_err(backend)?;
        Ok(())
    }

    fn sync_finish(
        &self,
        ctx: &RequestContext,
        claim: &JobClaim,
        expected: JobStateVersion,
        state: JobState,
        outcome: EffectOutcome,
        progress: JobProgress,
        error_code: Option<String>,
        pending_reason: Option<String>,
    ) -> Result<JobRecord, PortError> {
        if !state.is_terminal() {
            return Err(PortError::BackendUnavailable(
                "job finish requires terminal state".into(),
            ));
        }
        if let Some(code) = error_code.as_deref() {
            validate_code(code)?;
        }
        if let Some(code) = pending_reason.as_deref() {
            validate_code(code)?;
        }
        let now = self.clock.now();
        let state_text = state_name(state);
        let outcome_text = enum_text(&outcome)?;
        let progress_json =
            serde_json::to_string(&progress).map_err(|_| backend("progress encode failed"))?;
        let organization_id = ctx.organization_id.as_str().to_string();
        let conn = lock(&self.store)?;
        let tx = conn.unchecked_transaction().map_err(backend)?;
        Self::sync_check_claim(&tx, ctx, claim, now.as_str())?;
        let expected_version = i64::try_from(expected.get())
            .map_err(|_| PortError::BackendUnavailable("job version overflow".into()))?;
        let changed = tx
            .execute(
                "UPDATE jobs SET state = ?1, effect_outcome = ?2, progress_json = ?3, error = ?4, \
                 pending_verification_reason = ?5, state_version = state_version + 1, \
                 updated_at = ?6, worker_id = NULL, claim_stage_id = NULL, \
                 claim_expires_at = NULL, claimed_at = NULL \
                 WHERE organization_id = ?7 AND job_id = ?8 AND state_version = ?9",
                params![
                    state_text,
                    outcome_text,
                    progress_json,
                    error_code,
                    pending_reason,
                    now.as_str(),
                    organization_id,
                    claim.job_id.as_str(),
                    expected_version
                ],
            )
            .map_err(backend)?;
        if changed != 1 {
            return Err(PortError::CasConflict {
                entity: "job",
                id: claim.job_id.as_str().into(),
            });
        }
        if let Some(reason_code) = pending_reason.as_deref() {
            let recovery = datazen::jobs::domain::job::JobRecoveryResult {
                verdict: datazen::jobs::domain::job::JobRecoveryVerdict::PendingVerification,
                resume_through: None,
                reason_code: Some(reason_code.to_owned()),
            };
            tx.execute(
                "INSERT INTO job_result_details (organization_id, job_id, recovery_json) \
                 VALUES (?1, ?2, ?3) \
                 ON CONFLICT (organization_id, job_id) DO UPDATE SET recovery_json = excluded.recovery_json",
                params![
                    organization_id,
                    claim.job_id.as_str(),
                    serde_json::to_string(&recovery)
                        .map_err(|_| backend("recovery encode failed"))?
                ],
            )
            .map_err(backend)?;
        }
        let record = Self::sync_get(&tx, &organization_id, claim.job_id.as_str())?
            .ok_or_else(|| PortError::NotFound(claim.job_id.as_str().into()))?;
        tx.commit().map_err(backend)?;
        Ok(record)
    }

    fn sync_persist_recovery(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
        recovery: datazen::jobs::domain::job::JobRecoveryResult,
    ) -> Result<(), PortError> {
        if let Some(code) = recovery.reason_code.as_deref() {
            validate_code(code)?;
        }
        let now = self.clock.now();
        let organization_id = ctx.organization_id.as_str().to_string();
        let recovery_json = serde_json::to_string(&recovery)
            .map_err(|_| backend("recovery encode failed"))?;
        let conn = lock(&self.store)?;
        let tx = conn.unchecked_transaction().map_err(backend)?;
        let boundaries: i64 = tx
            .query_row(
                "SELECT COUNT(*) FROM job_commit_boundaries \
                 WHERE organization_id = ?1 AND job_id = ?2",
                params![organization_id, job_id.as_str()],
                |row| row.get(0),
            )
            .map_err(backend)?;
        if recovery
            .resume_through
            .is_some_and(|index| i64::try_from(index).unwrap_or(i64::MAX) > boundaries)
        {
            return Err(PortError::BackendUnavailable(
                "recovery boundary index is invalid".into(),
            ));
        }
        let changed = tx
            .execute(
                "INSERT INTO job_result_details (organization_id, job_id, recovery_json) \
                 SELECT organization_id, job_id, ?1 FROM jobs \
                 WHERE organization_id = ?2 AND job_id = ?3 \
                 ON CONFLICT (organization_id, job_id) DO UPDATE SET recovery_json = excluded.recovery_json",
                params![recovery_json, organization_id, job_id.as_str()],
            )
            .map_err(backend)?;
        if changed != 1 {
            return Err(PortError::NotFound(job_id.as_str().into()));
        }
        tx.execute(
            "UPDATE jobs SET pending_verification_reason = \
             CASE WHEN ?1 IN ('pendingVerification','reject','requireManualReview') \
             THEN ?2 ELSE pending_verification_reason END, \
             updated_at = ?3 WHERE organization_id = ?4 AND job_id = ?5",
            params![
                verdict_text(recovery.verdict),
                recovery.reason_code,
                now.as_str(),
                organization_id,
                job_id.as_str()
            ],
        )
        .map_err(backend)?;
        tx.commit().map_err(backend)?;
        Ok(())
    }

    fn sync_persist_recovery_verification(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
        expected: JobStateVersion,
        verification: datazen::jobs::domain::job::JobRecoveryVerification,
    ) -> Result<(), PortError> {
        let verification_valid = verification.domain_results.len() <= 64
            && verification.confirmed_boundaries.len() <= 4096
            && verification
                .result
                .reason_code
                .as_deref()
                .is_some_and(|code| {
                    datazen::jobs::runtime_repository::safe_result_marker(code)
                });
        if !verification_valid {
            return Err(PortError::BackendUnavailable(
                "recovery verification is outside the safe format".into(),
            ));
        }
        for boundary in &verification.confirmed_boundaries {
            if verification.result.verdict
                != datazen::jobs::domain::job::JobRecoveryVerdict::ResumeAfterVerify
            {
                return Err(PortError::BackendUnavailable(
                    "recovery verification is outside the safe format".into(),
                ));
            }
            validate_boundary(boundary)?;
        }
        for result in &verification.domain_results {
            datazen::jobs::runtime_repository::validate_job_domain_result(result)?;
        }
        let recovery_json = serde_json::to_string(&verification.result)
            .map_err(|_| backend("recovery encode failed"))?;
        let mut result_rows = Vec::new();
        for result in &verification.domain_results {
            let json = serde_json::to_string(result)
                .map_err(|_| backend("domain result encode failed"))?;
            if json.len() > 512 * 1024 {
                return Err(PortError::QuotaExceeded(
                    "job result exceeds the local size limit".into(),
                ));
            }
            result_rows.push((result.stage_id.as_str().to_string(), json));
        }
        let now = self.clock.now();
        let organization_id = ctx.organization_id.as_str().to_string();
        let conn = lock(&self.store)?;
        let tx = conn.unchecked_transaction().map_err(backend)?;
        let current = tx
            .query_row(
                "SELECT state, pending_verification_reason, state_version FROM jobs \
                 WHERE organization_id = ?1 AND job_id = ?2",
                params![organization_id, job_id.as_str()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(backend)?
            .ok_or_else(|| PortError::NotFound(job_id.as_str().into()))?;
        let persisted_recovery: Option<String> = tx
            .query_row(
                "SELECT recovery_json FROM job_result_details \
                 WHERE organization_id = ?1 AND job_id = ?2",
                params![organization_id, job_id.as_str()],
                |row| row.get(0),
            )
            .optional()
            .map_err(backend)?
            .flatten();
        let pending_verdict = persisted_recovery
            .as_deref()
            .and_then(|value| {
                serde_json::from_str::<datazen::jobs::domain::job::JobRecoveryResult>(value).ok()
            })
            .is_some_and(|result| {
                result.verdict
                    == datazen::jobs::domain::job::JobRecoveryVerdict::PendingVerification
            });
        if current.0 != "failed"
            || current.1.is_none()
            || current.2 != i64::try_from(expected.get()).unwrap_or(i64::MAX)
            || !pending_verdict
        {
            return Err(PortError::CasConflict {
                entity: "job_recovery",
                id: job_id.as_str().into(),
            });
        }
        let existing_boundaries: i64 = tx
            .query_row(
                "SELECT COUNT(*) FROM job_commit_boundaries \
                 WHERE organization_id = ?1 AND job_id = ?2",
                params![organization_id, job_id.as_str()],
                |row| row.get(0),
            )
            .map_err(backend)?;
        let boundary_total = existing_boundaries.saturating_add(
            i64::try_from(verification.confirmed_boundaries.len()).unwrap_or(i64::MAX),
        );
        if verification
            .result
            .resume_through
            .is_some_and(|index| i64::try_from(index).unwrap_or(i64::MAX) > boundary_total)
        {
            return Err(PortError::BackendUnavailable(
                "recovery boundary index is invalid".into(),
            ));
        }
        let mut sequence: i64 = tx
            .query_row(
                "SELECT COALESCE(MAX(boundary_index), 0) FROM job_commit_boundaries \
                 WHERE organization_id = ?1 AND job_id = ?2",
                params![organization_id, job_id.as_str()],
                |row| row.get(0),
            )
            .map_err(backend)?;
        for boundary in &verification.confirmed_boundaries {
            sequence = sequence.checked_add(1).ok_or_else(|| {
                PortError::BackendUnavailable("commit boundary sequence overflow".into())
            })?;
            let boundary_json = serde_json::to_string(boundary)
                .map_err(|_| backend("boundary encode failed"))?;
            tx.execute(
                "INSERT INTO job_commit_boundaries \
                 (organization_id, job_id, boundary_index, boundary_json) \
                 VALUES (?1, ?2, ?3, ?4)",
                params![organization_id, job_id.as_str(), sequence, boundary_json],
            )
            .map_err(backend)?;
        }
        for (stage_id, serialized) in &result_rows {
            tx.execute(
                "INSERT INTO job_domain_results (organization_id, job_id, stage_id, result_json) \
                 VALUES (?1, ?2, ?3, ?4) \
                 ON CONFLICT (organization_id, job_id, stage_id) DO UPDATE SET result_json = excluded.result_json",
                params![organization_id, job_id.as_str(), stage_id, serialized],
            )
            .map_err(backend)?;
        }
        tx.execute(
            "INSERT INTO job_result_details (organization_id, job_id, recovery_json) \
             VALUES (?1, ?2, ?3) \
             ON CONFLICT (organization_id, job_id) DO UPDATE SET recovery_json = excluded.recovery_json",
            params![organization_id, job_id.as_str(), recovery_json],
        )
        .map_err(backend)?;
        let is_resumable =
            verification.result.verdict == datazen::jobs::domain::job::JobRecoveryVerdict::ResumeAfterVerify;
        let pending_reason = if is_resumable {
            None
        } else {
            Some(
                verification
                    .result
                    .reason_code
                    .as_deref()
                    .unwrap_or("recoveryNeedsReview"),
            )
        };
        let changed = tx
            .execute(
                "UPDATE jobs SET pending_verification_reason = ?1, error = ?1, \
                 state_version = state_version + 1, updated_at = ?2 \
                 WHERE organization_id = ?3 AND job_id = ?4 AND state_version = ?5",
                params![
                    pending_reason,
                    now.as_str(),
                    organization_id,
                    job_id.as_str(),
                    current.2
                ],
            )
            .map_err(backend)?;
        if changed != 1 {
            return Err(PortError::CasConflict {
                entity: "job_recovery",
                id: job_id.as_str().into(),
            });
        }
        tx.commit().map_err(backend)?;
        Ok(())
    }

    fn sync_receipt_for(
        &self,
        ctx: &RequestContext,
        key: &IdempotencyKey,
    ) -> Result<Option<JobId>, PortError> {
        // 服务端形状差异：桌面查 `job_idempotency_receipts` 表，服务端受理行
        // 自持幂等（`idem_scope_key` = org|principal|key，与 sync_accept 同
        // 一铸造口径）。
        let idem_scope_key = format!(
            "{}|{}|{}",
            ctx.organization_id.as_str(),
            ctx.principal_id.as_str(),
            key.as_str()
        );
        let conn = lock(&self.store)?;
        let job_id = conn
            .query_row(
                "SELECT job_id FROM jobs WHERE organization_id = ?1 AND idem_scope_key = ?2",
                params![ctx.organization_id.as_str(), idem_scope_key],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(backend)?;
        match job_id {
            Some(id) => Ok(Some(JobId::new(&id))),
            None => Ok(None),
        }
    }

    fn sync_committed_boundaries(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
    ) -> Result<Vec<CommitBoundary>, PortError> {
        let conn = lock(&self.store)?;
        Self::sync_get(&conn, ctx.organization_id.as_str(), job_id.as_str())?
            .ok_or_else(|| PortError::NotFound(job_id.as_str().into()))?;
        read_boundaries(&conn, ctx.organization_id.as_str(), job_id)
    }

    fn sync_latest_checkpoint(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
    ) -> Result<Option<Checkpoint>, PortError> {
        let conn = lock(&self.store)?;
        Self::sync_get(&conn, ctx.organization_id.as_str(), job_id.as_str())?
            .ok_or_else(|| PortError::NotFound(job_id.as_str().into()))?;
        let row = conn
            .query_row(
                "SELECT checkpoint_version, stable_target_fingerprint, committed_json, \
                 verification_evidence_json, recovery_policy FROM job_checkpoints \
                 WHERE organization_id = ?1 AND job_id = ?2 \
                 ORDER BY checkpoint_version DESC LIMIT 1",
                params![ctx.organization_id.as_str(), job_id.as_str()],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                },
            )
            .optional()
            .map_err(backend)?;
        let Some((version, fingerprint, committed_json, evidence_json, policy)) = row else {
            return Ok(None);
        };
        Ok(Some(Checkpoint {
            job_id: job_id.clone(),
            state_version: JobStateVersion::new(
                u64::try_from(version).map_err(|_| backend("checkpoint version invalid"))?,
            ),
            stable_target_fingerprint: fingerprint,
            committed: serde_json::from_str(&committed_json)
                .map_err(|_| backend("checkpoint committed decode failed"))?,
            verification_evidence: serde_json::from_str(&evidence_json)
                .map_err(|_| backend("checkpoint evidence decode failed"))?,
            recovery_policy: policy,
        }))
    }

    fn sync_get_details(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
    ) -> Result<datazen::jobs::domain::job::JobDetails, PortError> {
        let now = self.clock.now();
        let organization_id = ctx.organization_id.as_str().to_string();
        let conn = lock(&self.store)?;
        prune_expired_artifacts(&conn, now.as_str())?;
        let record = Self::sync_get(&conn, &organization_id, job_id.as_str())?
            .ok_or_else(|| PortError::NotFound(job_id.as_str().into()))?;
        let result_row = conn
            .query_row(
                "SELECT plan_id, plan_digest, selection_revision, recovery_json \
                 FROM job_result_details WHERE organization_id = ?1 AND job_id = ?2",
                params![organization_id, job_id.as_str()],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<i64>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                    ))
                },
            )
            .optional()
            .map_err(backend)?;
        let (plan_id, plan_digest, selection_revision, recovery) =
            result_row.unwrap_or((None, None, None, None));
        let payload_plan_id = record
            .definition
            .payload
            .get("consumedPlanId")
            .or_else(|| record.definition.payload.get("planId"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        let payload_digest = record
            .definition
            .payload
            .get("planDigest")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        let payload_revision = record
            .definition
            .payload
            .get("selectionRevision")
            .and_then(serde_json::Value::as_u64);
        let committed = read_boundaries(&conn, &organization_id, job_id)?;
        let domain_results = read_domain_results(&conn, &organization_id, job_id)?;
        let recovery_targets = record
            .definition
            .payload
            .get("recoveryTargets")
            .cloned()
            .map(serde_json::from_value::<Vec<datazen::jobs::domain::job::JobRecoveryTarget>>)
            .transpose()
            .map_err(|_| backend("stored recovery targets are invalid"))?
            .unwrap_or_default();
        let target_before_fingerprint = record
            .definition
            .payload
            .get("targetBeforeFingerprint")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        let recovery_policy = record
            .definition
            .payload
            .get("recoveryPolicy")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        let recovery = recovery
            .map(|json| {
                serde_json::from_str::<datazen::jobs::domain::job::JobRecoveryResult>(&json)
                    .map_err(|_| backend("stored recovery is invalid"))
            })
            .transpose()?;
        let selection_revision = match selection_revision {
            Some(revision) => Some(
                u64::try_from(revision)
                    .map_err(|_| backend("stored plan revision is invalid"))?,
            ),
            None => payload_revision,
        };
        Ok(datazen::jobs::domain::job::JobDetails {
            job: record.view,
            state_version: record.state_version,
            plan_id: plan_id.or(payload_plan_id),
            plan_digest: plan_digest.or(payload_digest),
            selection_revision,
            commit_boundaries: committed,
            recovery,
            domain_results,
            recovery_targets,
            target_before_fingerprint,
            recovery_policy,
        })
    }

    fn sync_validate_stage_owner(
        &self,
        ctx: &RequestContext,
        claim: &JobClaim,
        stage: &StageId,
    ) -> Result<(), PortError> {
        let now = self.clock.now();
        let conn = lock(&self.store)?;
        conn.query_row(
            "SELECT 1 FROM jobs j JOIN job_stages s \
             ON s.organization_id = j.organization_id AND s.job_id = j.job_id \
             WHERE j.organization_id = ?1 AND j.job_id = ?2 \
             AND j.state = 'running' AND j.cancel_requested = 0 AND j.worker_id = ?3 \
             AND j.claim_generation = ?4 AND j.claim_expires_at > ?5 \
             AND j.stage = ?6 AND s.stage_id = ?6 AND s.claimed_by = ?3 \
             AND s.started_at IS NOT NULL AND s.finished_at IS NULL",
            params![
                ctx.organization_id.as_str(),
                claim.job_id.as_str(),
                claim.worker_id.as_str(),
                claim.claim_generation.get() as i64,
                now.as_str(),
                stage.as_str()
            ],
            |_| Ok(()),
        )
        .optional()
        .map_err(backend)?
        .ok_or(PortError::StaleClaim)
    }
}

fn not_found_or_conflict(
    conn: &rusqlite::Connection,
    organization_id: &str,
    job_id: &JobId,
) -> Result<PortError, PortError> {
    let exists = conn
        .query_row(
            "SELECT 1 FROM jobs WHERE organization_id = ?1 AND job_id = ?2",
            params![organization_id, job_id.as_str()],
            |_| Ok(()),
        )
        .optional()
        .map_err(backend)?;
    Ok(if exists.is_some() {
        PortError::CasConflict {
            entity: "job",
            id: job_id.as_str().into(),
        }
    } else {
        PortError::NotFound(job_id.as_str().into())
    })
}

#[async_trait]
impl datazen::jobs::runtime_repository::JobRuntimeRepository for ServerJobs {
    async fn validate_stage_owner(
        &self,
        ctx: &RequestContext,
        claim: &JobClaim,
        stage: &StageId,
    ) -> Result<(), PortError> {
        let this = self.clone();
        let ctx = ctx.clone();
        let claim = claim.clone();
        let stage = stage.clone();
        tokio::task::spawn_blocking(move || {
            this.sync_validate_stage_owner(&ctx, &claim, &stage)
        })
        .await
        .map_err(backend)?
    }

    async fn cancel_poll(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
    ) -> Result<datazen::jobs::engine::memory_repository::CancelPollSnapshot, PortError> {
        let this = self.clone();
        let ctx = ctx.clone();
        let job_id = job_id.clone();
        tokio::task::spawn_blocking(move || this.sync_cancel_poll(&ctx, &job_id))
            .await
            .map_err(backend)?
    }

    async fn request_cancel(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
    ) -> Result<JobRecord, PortError> {
        let this = self.clone();
        let ctx = ctx.clone();
        let job_id = job_id.clone();
        tokio::task::spawn_blocking(move || this.sync_request_cancel(&ctx, &job_id))
            .await
            .map_err(backend)?
    }

    async fn confirm_cancelled_not_started(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
        outcome: EffectOutcome,
    ) -> Result<JobRecord, PortError> {
        let this = self.clone();
        let ctx = ctx.clone();
        let job_id = job_id.clone();
        tokio::task::spawn_blocking(move || {
            this.sync_confirm_cancelled(&ctx, &job_id, outcome)
        })
        .await
        .map_err(backend)?
    }

    async fn mark_failed_unstarted(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
        reason_code: &str,
    ) -> Result<JobRecord, PortError> {
        let this = self.clone();
        let ctx = ctx.clone();
        let job_id = job_id.clone();
        let reason_code = reason_code.to_owned();
        tokio::task::spawn_blocking(move || {
            this.sync_mark_failed_unstarted(&ctx, &job_id, &reason_code)
        })
        .await
        .map_err(backend)?
    }

    async fn mark_pending_verification(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
        reason_code: &str,
    ) -> Result<JobRecord, PortError> {
        let this = self.clone();
        let ctx = ctx.clone();
        let job_id = job_id.clone();
        let reason_code = reason_code.to_owned();
        tokio::task::spawn_blocking(move || {
            this.sync_mark_pending_verification(&ctx, &job_id, &reason_code)
        })
        .await
        .map_err(backend)?
    }

    async fn record_progress(
        &self,
        ctx: &RequestContext,
        claim: &JobClaim,
        progress: JobProgress,
    ) -> Result<(), PortError> {
        let this = self.clone();
        let ctx = ctx.clone();
        let claim = claim.clone();
        tokio::task::spawn_blocking(move || this.sync_record_progress(&ctx, &claim, progress))
            .await
            .map_err(backend)?
    }

    async fn record_artifacts(
        &self,
        ctx: &RequestContext,
        claim: &JobClaim,
        artifacts: &[ArtifactId],
    ) -> Result<(), PortError> {
        let this = self.clone();
        let ctx = ctx.clone();
        let claim = claim.clone();
        let artifacts = artifacts.to_vec();
        tokio::task::spawn_blocking(move || {
            this.sync_record_artifacts(&ctx, &claim, &artifacts)
        })
        .await
        .map_err(backend)?
    }

    async fn record_domain_result(
        &self,
        ctx: &RequestContext,
        claim: &JobClaim,
        result: datazen::jobs::domain::job::JobDomainResult,
    ) -> Result<(), PortError> {
        let this = self.clone();
        let ctx = ctx.clone();
        let claim = claim.clone();
        tokio::task::spawn_blocking(move || this.sync_record_domain_result(&ctx, &claim, result))
            .await
            .map_err(backend)?
    }

    async fn append_external_artifacts(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
        artifacts: &[ArtifactId],
    ) -> Result<(), PortError> {
        let this = self.clone();
        let ctx = ctx.clone();
        let job_id = job_id.clone();
        let artifacts = artifacts.to_vec();
        tokio::task::spawn_blocking(move || {
            this.sync_append_external_artifacts(&ctx, &job_id, &artifacts)
        })
        .await
        .map_err(backend)?
    }

    async fn finish(
        &self,
        ctx: &RequestContext,
        claim: &JobClaim,
        expected: JobStateVersion,
        state: JobState,
        outcome: EffectOutcome,
        progress: JobProgress,
        error_code: Option<String>,
        pending_verification_reason: Option<String>,
    ) -> Result<JobRecord, PortError> {
        let this = self.clone();
        let ctx = ctx.clone();
        let claim = claim.clone();
        tokio::task::spawn_blocking(move || {
            this.sync_finish(
                &ctx,
                &claim,
                expected,
                state,
                outcome,
                progress,
                error_code,
                pending_verification_reason,
            )
        })
        .await
        .map_err(backend)?
    }

    async fn persist_recovery(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
        recovery: datazen::jobs::domain::job::JobRecoveryResult,
    ) -> Result<(), PortError> {
        let this = self.clone();
        let ctx = ctx.clone();
        let job_id = job_id.clone();
        tokio::task::spawn_blocking(move || this.sync_persist_recovery(&ctx, &job_id, recovery))
            .await
            .map_err(backend)?
    }

    async fn persist_recovery_verification(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
        expected: JobStateVersion,
        verification: datazen::jobs::domain::job::JobRecoveryVerification,
    ) -> Result<(), PortError> {
        let this = self.clone();
        let ctx = ctx.clone();
        let job_id = job_id.clone();
        tokio::task::spawn_blocking(move || {
            this.sync_persist_recovery_verification(&ctx, &job_id, expected, verification)
        })
        .await
        .map_err(backend)?
    }

    async fn receipt_for(
        &self,
        ctx: &RequestContext,
        key: &IdempotencyKey,
    ) -> Result<Option<JobId>, PortError> {
        let this = self.clone();
        let ctx = ctx.clone();
        let key = key.clone();
        tokio::task::spawn_blocking(move || this.sync_receipt_for(&ctx, &key))
            .await
            .map_err(backend)?
    }

    async fn committed_boundaries(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
    ) -> Result<Vec<CommitBoundary>, PortError> {
        let this = self.clone();
        let ctx = ctx.clone();
        let job_id = job_id.clone();
        tokio::task::spawn_blocking(move || this.sync_committed_boundaries(&ctx, &job_id))
            .await
            .map_err(backend)?
    }

    async fn latest_checkpoint(
        &self,
        ctx: &RequestContext,
        job_id: &JobId,
    ) -> Result<Option<Checkpoint>, PortError> {
        let this = self.clone();
        let ctx = ctx.clone();
        let job_id = job_id.clone();
        tokio::task::spawn_blocking(move || this.sync_latest_checkpoint(&ctx, &job_id))
            .await
            .map_err(backend)?
    }

    async fn get_details(
        &self,
        ctx: &RequestContext,
        job_id: JobId,
    ) -> Result<datazen::jobs::domain::job::JobDetails, PortError> {
        let this = self.clone();
        let ctx = ctx.clone();
        let job_id = job_id.clone();
        tokio::task::spawn_blocking(move || this.sync_get_details(&ctx, &job_id))
            .await
            .map_err(backend)?
    }
}
