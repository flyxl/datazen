//! apply 阶段实现：结构指纹重验 → 选中子集核验 → 权限 → 事务作用域判定 →
//! 逐批 begin / before 重验 / execute / commit / CommitBoundary（§7、§10.1.1）。
//!
//! 终态语义：提交边界为空即整体回滚（RolledBack），有边界即部分提交
//! （PartiallyApplied）；生效范围不可判定时必须给 `Unknown`，绝不把
//! 「取消请求已送达驱动」当作「写入已回滚」的证据。

use std::sync::Arc;

use datazen_platform_api::dto::execution::{EffectOutcome, ExecutionErrorCode};
use datazen_platform_api::dto::job::{CommitBoundary, JobProgress};
use datazen_platform_api::id::{ArtifactId, Counter, ExecutionId, StageId, Timestamp};
use datazen_runtime::job::{CancelToken, JobError, StageOutcome, StageTerminal};
use sha2::{Digest, Sha256};

use crate::error::DataSyncError;
use crate::gate::check_table_gate;
use crate::model::{ChangeOperation, Row, RowChange, SyncOptions};

use super::super::artifact::{
    structure_fingerprint, ChangeBlock, ChangeSetArtifact, RelationIdentity,
};
use super::super::body::ApplySpec;
use super::super::host::{DataSyncHost, EndpointSession};
use super::{op_name, stage_failed, DataSyncHandler};

/// 一次冻结批次：同 relation + operation 的块按 options.batch_size 切分；
/// batchId/payload digest 由 plan 与块集合唯一决定，重复切分不变。
struct Batch {
    relation: RelationIdentity,
    source_table: String,
    operation: ChangeOperation,
    blocks: Vec<ChangeBlock>,
    batch_id: String,
    payload_digest: String,
}

fn build_batches(plan_id: &str, selection: &[ChangeBlock], options: &SyncOptions) -> Vec<Batch> {
    let mut groups: Vec<(String, ChangeOperation, Vec<ChangeBlock>)> = Vec::new();
    for block in selection {
        let key = format!(
            "{}/{}/{}",
            block.relation.database,
            block.relation.schema.clone().unwrap_or_default(),
            block.relation.table
        );
        match groups
            .iter_mut()
            .find(|(k, op, _)| k == &key && *op == block.operation)
        {
            Some((_, _, blocks)) => blocks.push(block.clone()),
            None => groups.push((key, block.operation, vec![block.clone()])),
        }
    }
    let mut out = Vec::new();
    for (_, operation, blocks) in groups {
        for (i, chunk) in blocks.chunks(options.batch_size as usize).enumerate() {
            let relation = chunk[0].relation.clone();
            let source_table = chunk[0].source_table.clone();
            let batch_id = format!(
                "{}:{}:{}:{}",
                plan_id,
                relation.table,
                op_name(operation),
                i
            );
            let mut hasher = Sha256::new();
            hasher.update(batch_id.as_bytes());
            for block in chunk {
                if let Ok(bytes) = serde_json::to_vec(block) {
                    hasher.update(bytes);
                }
            }
            let payload_digest = format!("{:x}", hasher.finalize());
            out.push(Batch {
                relation,
                source_table,
                operation,
                blocks: chunk.to_vec(),
                batch_id,
                payload_digest,
            });
        }
    }
    out
}
/// apply：重验 → 子集核验 → 权限 → 事务能力证明 → 逐批 begin/verify/execute/commit/boundary。
impl DataSyncHandler {
    pub(super) async fn run_apply(&self, cancel: &CancelToken) -> Result<StageOutcome, JobError> {
        let Self::Apply { spec, host } = self else {
            return Err(JobError::PlanProjectionInvalid(
                "run_apply on a prepare handler".into(),
            ));
        };
        let artifact = match host.load_artifact(&spec.plan_id).await {
            Ok(Some(a)) => a,
            Ok(None) => {
                return Ok(stage_failed(
                    "apply",
                    EffectOutcome::NotStarted,
                    ExecutionErrorCode::HostRejected,
                    DataSyncError::validation(format!("plan not found: {}", spec.plan_id)),
                    Vec::new(),
                    JobProgress::default(),
                ));
            }
            Err(e) => {
                return Ok(stage_failed(
                    "apply",
                    EffectOutcome::NotStarted,
                    ExecutionErrorCode::HostRejected,
                    e,
                    Vec::new(),
                    JobProgress::default(),
                ));
            }
        };
        let source = match host.open_endpoint(&artifact.source).await {
            Ok(s) => s,
            Err(e) => {
                return Ok(stage_failed(
                    "apply",
                    EffectOutcome::NotStarted,
                    ExecutionErrorCode::HostRejected,
                    e,
                    Vec::new(),
                    JobProgress::default(),
                ));
            }
        };
        let target = match host.open_endpoint(&artifact.target).await {
            Ok(t) => t,
            Err(e) => {
                host.close_endpoint(source).await;
                return Ok(stage_failed(
                    "apply",
                    EffectOutcome::NotStarted,
                    ExecutionErrorCode::HostRejected,
                    e,
                    Vec::new(),
                    JobProgress::default(),
                ));
            }
        };
        let result = self
            .apply_execute(spec, host, artifact, &source, &target, cancel)
            .await;
        host.close_endpoint(source).await;
        host.close_endpoint(target).await;
        match result {
            Ok((boundaries, progress)) => Ok(StageOutcome {
                stage_id: StageId::new("apply"),
                terminal: StageTerminal::Succeeded,
                progress,
                commit_boundaries: boundaries,
                execution_ids: vec![ExecutionId::new(format!("exec-apply-{}", spec.plan_id))],
                artifact_ids: vec![ArtifactId::new(format!("changeset-{}", spec.plan_id))],
                effect_outcome: EffectOutcome::Completed,
                error_code: None,
            }),
            Err(ApplyFailure {
                terminal,
                effect,
                code,
                err,
                boundaries,
                progress,
            }) => {
                let _ = err;
                Ok(StageOutcome {
                    stage_id: StageId::new("apply"),
                    terminal,
                    progress,
                    commit_boundaries: boundaries,
                    execution_ids: Vec::new(),
                    artifact_ids: vec![ArtifactId::new(format!("changeset-{}", spec.plan_id))],
                    effect_outcome: effect,
                    error_code: code.or(Some(ExecutionErrorCode::HostRejected)),
                })
            }
        }
    }

    /// apply 的写入阶段主体。
    pub(super) async fn apply_execute(
        &self,
        spec: &ApplySpec,
        host: &Arc<dyn DataSyncHost>,
        artifact: ChangeSetArtifact,
        source: &EndpointSession,
        target: &EndpointSession,
        cancel: &CancelToken,
    ) -> Result<(Vec<CommitBoundary>, JobProgress), ApplyFailure> {
        // 1. 重验结构/PK（指纹 + 门闸）
        let pairs = host
            .mapped_table_schemas(source, target, &artifact.mappings)
            .await
            .map_err(|e| ApplyFailure::rejected(e))?;
        let current_fp = structure_fingerprint(pairs.iter().flat_map(|p| [&p.source, &p.target]));
        if current_fp != artifact.structure_fingerprint {
            return Err(ApplyFailure::rejected(DataSyncError::incompatible(
                "structure fingerprint changed — PlanStale，需要重新准备",
            )));
        }
        for pair in &pairs {
            let verdict = check_table_gate(&source.family, &pair.source, &pair.target);
            if !verdict.is_compatible() {
                return Err(ApplyFailure::rejected(DataSyncError::incompatible(
                    format!(
                        "table {}: {} — 门闸复验失败",
                        pair.source_table,
                        verdict.reason_text()
                    ),
                )));
            }
        }
        // 2. 权限
        host.check_permissions(target)
            .await
            .map_err(|e| ApplyFailure::rejected(e))?;
        // 3. 事务能力证明：无法证明批次事务支持则拒绝
        let scope = host
            .transaction_scope(target)
            .await
            .map_err(|e| ApplyFailure::rejected(e))?;
        if !scope.supports_batch_atomicity() {
            return Err(ApplyFailure::rejected(DataSyncError::incompatible(
                "target does not prove batch transaction support (NonAtomic scope)",
            )));
        }
        // 4. 选中子集核验
        let selection = host
            .selection(&spec.plan_id, spec.selection_revision)
            .await
            .map_err(|e| ApplyFailure::rejected(e))?;
        let subset_ok = selection.iter().all(|b| {
            artifact.blocks.iter().any(|ab| {
                ab.relation == b.relation
                    && ab.operation == b.operation
                    && crate::model::keys_equal(&ab.key, &b.key)
            })
        });
        if !subset_ok {
            return Err(ApplyFailure::rejected(DataSyncError::validation(
                "selection is not a subset of the frozen ChangeSet (客户端增加了行或改写了值)",
            )));
        }
        // 5. 固定目标 Lease 开执行器
        let mut executor = host
            .target_executor(target)
            .await
            .map_err(|e| ApplyFailure::rejected(e))?;
        let batches = build_batches(&spec.plan_id, &selection, &spec.options);
        let mut boundaries = Vec::new();
        let mut committed_rows = 0u64;
        let mut attempted = 0u64;
        let unknown_rows = 0u64;
        let mut statements_generated = 0u64;

        for batch in &batches {
            if cancel.is_cancelled() {
                // 提交前取消：回滚当前批（若已 begin 则未 commit 的部分由 rollback 处理）
                let _ = executor.rollback().await;
                return Err(ApplyFailure::cancelled(boundaries, committed_rows));
            }
            if let Err(e) = executor.begin().await {
                return Err(ApplyFailure::failed(
                    DataSyncError::outcome_unknown(format!("begin: {e}")),
                    boundaries,
                    committed_rows,
                    unknown_rows,
                ));
            }
            // before 证据重验
            let mut conflict: Option<String> = None;
            for block in &batch.blocks {
                match executor.read_by_key(&block.relation, &block.key).await {
                    Ok(None) if block.operation == ChangeOperation::Insert => {}
                    Ok(Some(current))
                        if block.operation != ChangeOperation::Insert
                            && option_rows_equal(&current, &block.before) => {}
                    Ok(_) => {
                        conflict = Some(format!(
                            "TargetConflictRows: target row at {:?} changed since review",
                            block.key
                        ));
                        break;
                    }
                    Err(e) if matches!(e, DataSyncError::Cancelled(_)) => {
                        let _ = executor.rollback().await;
                        return Err(ApplyFailure::cancelled(boundaries, committed_rows));
                    }
                    Err(e) if matches!(e, DataSyncError::OutcomeUnknown(_)) => {
                        let _ = executor.discard().await;
                        return Err(ApplyFailure::unknown(
                            boundaries,
                            committed_rows,
                            batch.blocks.len() as u64,
                        ));
                    }
                    Err(e) => {
                        let _ = executor.rollback().await;
                        return Err(ApplyFailure::failed(
                            e,
                            boundaries,
                            committed_rows,
                            unknown_rows,
                        ));
                    }
                }
            }
            if let Some(reason) = conflict {
                let _ = executor.rollback().await;
                return Err(ApplyFailure::failed_with_code(
                    DataSyncError::conflict(reason),
                    ExecutionErrorCode::SqlError,
                    boundaries,
                    committed_rows,
                ));
            }
            // 生成 SQL 并执行
            let changes: Vec<RowChange> = batch
                .blocks
                .iter()
                .map(|b| RowChange {
                    operation: b.operation,
                    key: b.key.clone(),
                    source_row: b.after.clone(),
                    target_row: b.before.clone(),
                    changed_columns: b.changed_columns.clone(),
                    selected: true,
                })
                .collect();
            let statements = host
                .generate_statements(
                    &batch.relation,
                    &batch.source_table,
                    &batch.relation.table,
                    changes,
                    &spec.options,
                )
                .await
                .map_err(ApplyFailure::rejected);
            let statements = match statements {
                Ok(s) => s,
                Err(f) => {
                    let _ = executor.rollback().await;
                    return Err(f);
                }
            };
            statements_generated += statements.len() as u64;
            let mut batch_conflict: Option<String> = None;
            let mut cancelled = false;
            let mut unknown = false;
            for stmt in &statements {
                attempted += 1;
                match executor.execute(stmt).await {
                    Ok(1) => {}
                    Ok(n) => {
                        batch_conflict = Some(format!(
                            "TargetConflictRows: expected 1 affected row, got {n}"
                        ));
                        break;
                    }
                    Err(e) if matches!(e, DataSyncError::Cancelled(_)) => {
                        cancelled = true;
                        break;
                    }
                    Err(e) if matches!(e, DataSyncError::OutcomeUnknown(_)) => {
                        unknown = true;
                        e_log_warn(e);
                        break;
                    }
                    Err(e) => {
                        let _ = executor.rollback().await;
                        return Err(ApplyFailure::failed(
                            e,
                            boundaries,
                            committed_rows,
                            unknown_rows,
                        ));
                    }
                }
            }
            if cancelled {
                let _ = executor.rollback().await;
                return Err(ApplyFailure::cancelled(boundaries, committed_rows));
            }
            if unknown {
                let _ = executor.discard().await;
                return Err(ApplyFailure::unknown(
                    boundaries,
                    committed_rows,
                    batch.blocks.len() as u64,
                ));
            }
            if let Some(reason) = batch_conflict {
                let _ = executor.rollback().await;
                return Err(ApplyFailure::failed_with_code(
                    DataSyncError::conflict(reason),
                    ExecutionErrorCode::SqlError,
                    boundaries,
                    committed_rows,
                ));
            }
            // commit
            match executor.commit().await {
                Ok(()) => {
                    committed_rows += batch.blocks.len() as u64;
                    let now = host.now();
                    boundaries.push(CommitBoundary {
                        stage_id: StageId::new("apply"),
                        stable_target_fingerprint: artifact.digest.clone(),
                        committed_at: Timestamp::new(now.clone()),
                        operation_id: None,
                        batch_id: Some(batch.batch_id.clone()),
                        payload_digest: Some(batch.payload_digest.clone()),
                        evidence: vec![
                            "target-batch-record".into(),
                            format!("operation={}", op_name(batch.operation)),
                            format!("affected={}", statements.len()),
                        ],
                        verified_at: Some(Timestamp::new(now)),
                    });
                }
                Err(e) if matches!(e, DataSyncError::OutcomeUnknown(_)) => {
                    let _ = executor.discard().await;
                    return Err(ApplyFailure::unknown(
                        boundaries,
                        committed_rows,
                        batch.blocks.len() as u64,
                    ));
                }
                Err(e) if matches!(e, DataSyncError::Cancelled(_)) => {
                    let _ = executor.rollback().await;
                    return Err(ApplyFailure::cancelled(boundaries, committed_rows));
                }
                Err(e) => {
                    let _ = executor.rollback().await;
                    return Err(ApplyFailure::failed(
                        e,
                        boundaries,
                        committed_rows,
                        unknown_rows,
                    ));
                }
            }
        }
        let progress = JobProgress {
            read: Counter::new(selection.len() as u64),
            converted: Counter::new(statements_generated),
            attempted: Counter::new(attempted),
            committed: Counter::new(committed_rows),
            unknown: Counter::new(unknown_rows),
        };
        Ok((boundaries, progress))
    }
}

fn option_rows_equal(current: &Row, before: &Option<Row>) -> bool {
    match before {
        Some(b) => crate::model::rows_equal(current, b),
        None => false,
    }
}

fn e_log_warn(e: DataSyncError) {
    tracing::warn!(error = %e, "data-sync executor returned unknown outcome");
}

pub(super) struct ApplyFailure {
    terminal: StageTerminal,
    effect: EffectOutcome,
    code: Option<ExecutionErrorCode>,
    err: DataSyncError,
    boundaries: Vec<CommitBoundary>,
    progress: JobProgress,
}

impl ApplyFailure {
    fn rejected(err: DataSyncError) -> Self {
        Self {
            terminal: StageTerminal::Failed,
            effect: EffectOutcome::NotStarted,
            code: Some(ExecutionErrorCode::HostRejected),
            err,
            boundaries: Vec::new(),
            progress: JobProgress::default(),
        }
    }

    fn failed(
        err: DataSyncError,
        boundaries: Vec<CommitBoundary>,
        committed_rows: u64,
        unknown_rows: u64,
    ) -> Self {
        let effect = if boundaries.is_empty() {
            EffectOutcome::RolledBack
        } else {
            EffectOutcome::PartiallyApplied
        };
        Self {
            terminal: StageTerminal::Failed,
            effect,
            code: Some(ExecutionErrorCode::SqlError),
            err,
            boundaries,
            progress: JobProgress {
                committed: Counter::new(committed_rows),
                unknown: Counter::new(unknown_rows),
                ..JobProgress::default()
            },
        }
    }

    fn failed_with_code(
        err: DataSyncError,
        code: ExecutionErrorCode,
        boundaries: Vec<CommitBoundary>,
        committed_rows: u64,
    ) -> Self {
        let effect = if boundaries.is_empty() {
            EffectOutcome::RolledBack
        } else {
            EffectOutcome::PartiallyApplied
        };
        Self {
            terminal: StageTerminal::Failed,
            effect,
            code: Some(code),
            err,
            boundaries,
            progress: JobProgress {
                committed: Counter::new(committed_rows),
                ..JobProgress::default()
            },
        }
    }

    fn unknown(boundaries: Vec<CommitBoundary>, committed_rows: u64, unknown_rows: u64) -> Self {
        Self {
            terminal: StageTerminal::Unknown,
            effect: EffectOutcome::Unknown,
            code: Some(ExecutionErrorCode::ResourceLost),
            err: DataSyncError::outcome_unknown("commit outcome unknown"),
            boundaries,
            progress: JobProgress {
                committed: Counter::new(committed_rows),
                unknown: Counter::new(unknown_rows),
                ..JobProgress::default()
            },
        }
    }

    fn cancelled(boundaries: Vec<CommitBoundary>, committed_rows: u64) -> Self {
        let effect = if boundaries.is_empty() {
            EffectOutcome::RolledBack
        } else {
            EffectOutcome::PartiallyApplied
        };
        Self {
            terminal: StageTerminal::Cancelled,
            effect,
            code: Some(ExecutionErrorCode::Cancelled),
            err: DataSyncError::cancelled("cancelled"),
            boundaries,
            progress: JobProgress {
                committed: Counter::new(committed_rows),
                ..JobProgress::default()
            },
        }
    }
}
