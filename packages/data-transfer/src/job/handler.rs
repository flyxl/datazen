//! DataTransferHandler：JobHandler 协议实现（§10.1.1）。
//!
//! - `validate_plan`：冻结语义复验（版本、fingerprint、能力、恢复政策）。
//! - `run_stage(prepare)`：源一致性证据 + 小规模采样 Artifact。
//! - `run_stage(apply)`：有界管道（结构阶段 + 数据阶段 + 外键阶段）。
//! - `verify_recovery`：§7 裁决表。

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use datazen_driver_api::{
    ConnectionHandle, DatabaseDriver, SyncSourceAdapter, SyncTargetAdapter, TableSchema, Value,
};
use datazen_platform_api::dto::execution::EffectOutcome;
use datazen_platform_api::dto::job::{Checkpoint, JobProgress};
use datazen_platform_api::id::{ArtifactId, StageId};

use async_trait::async_trait;

use datazen_runtime::job::{
    CancelToken, FrozenPlan, JobError, JobHandler, RecoveryVerdict, StageOutcome, StageSpec,
    StageTerminal,
};

use crate::error::TransferError;
use crate::model::{DdlPreviewItem, TableExecutionOutcome, TableInspectResult, TransferMode};
use crate::resume::{ResumeTableProgress, TransferResumeCheckpoint};
use crate::transfer::adapter_registry::SyncAdapterRegistry;

use crate::job::checkpoint::commit_boundary;
use crate::job::outcome::{absorb_progress, cancelled_stage, failed_stage, unbounded_stage};
use crate::job::pipeline::{execute_bounded_table, BoundedPipelineContext, PIPELINE_INITIAL_BYTES};
use crate::job::plan::{validate_frozen_plan, TransferFreezeBody};
use crate::job::recovery::verify_checkpoint;

/// 目标端点分类：数据库目标需要目标 driver/handle，SQL 文件目标不需要。
pub enum TransferEndpoints {
    Database {
        source_driver: Arc<dyn DatabaseDriver>,
        source_handle: ConnectionHandle,
        target_driver: Arc<dyn DatabaseDriver>,
        target_handle: ConnectionHandle,
        source_type: String,
        target_type: String,
    },
    SqlFile {
        source_driver: Arc<dyn DatabaseDriver>,
        source_handle: ConnectionHandle,
        source_type: String,
        target_type: String,
        destination: PathBuf,
        structure: Option<Vec<DdlPreviewItem>>,
        /// 源 IR 适配器：结构对象的方言中立渲染需要它；缺失时只允许基础建表。
        source_adapter: Option<Arc<dyn SyncSourceAdapter>>,
        /// 目标 IR 适配器：同上；SQL 文件目标的 DDL 落到文件，不需要目标连接。
        target_adapter: Option<Arc<dyn SyncTargetAdapter>>,
    },
}

/// 阶段形态：prepare 只跑 prepare stage；apply 按 mode 展开 structure/data/foreignKeys。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferStageProfile {
    Prepare,
    Apply,
}

/// 内存版 TransferResumeCheckpoint：单 job 内跟踪每张表的恢复进度。
pub struct InMemoryTransferCheckpoint {
    progress: HashMap<String, ResumeTableProgress>,
    invalidated: bool,
}

impl InMemoryTransferCheckpoint {
    pub fn new() -> Self {
        Self {
            progress: HashMap::new(),
            invalidated: false,
        }
    }
}

impl TransferResumeCheckpoint for InMemoryTransferCheckpoint {
    fn renew(&mut self) -> Result<(), TransferError> {
        Ok(())
    }

    fn prepare_table(
        &mut self,
        source_table: &str,
        target_table: &str,
        key_columns: &[String],
        chunk_size: u32,
        source_fingerprint: &str,
    ) -> Result<ResumeTableProgress, TransferError> {
        Ok(self
            .progress
            .entry(source_table.to_string())
            .or_insert_with(|| ResumeTableProgress {
                source_table: source_table.into(),
                target_table: target_table.into(),
                key_columns: key_columns.to_vec(),
                chunk_size,
                source_fingerprint: source_fingerprint.into(),
                cursor: None,
                rows_seen: 0,
            })
            .clone())
    }

    fn advance_table(
        &mut self,
        source_table: &str,
        cursor: Vec<Value>,
        rows_seen: u64,
    ) -> Result<(), TransferError> {
        if let Some(progress) = self.progress.get_mut(source_table) {
            progress.cursor = Some(cursor);
            progress.rows_seen = rows_seen;
            Ok(())
        } else {
            Err(TransferError::validation("missing checkpoint table"))
        }
    }

    fn token(&self) -> Option<String> {
        Some("in-memory-token".into())
    }

    fn has_table_progress(&self, source_table: &str) -> bool {
        self.progress.contains_key(source_table)
    }

    fn table_has_committed_chunks(&self, source_table: &str) -> bool {
        self.progress
            .get(source_table)
            .is_some_and(|p| p.rows_seen > 0)
    }

    fn is_invalidated(&self) -> bool {
        self.invalidated
    }

    fn invalidate(&mut self) {
        self.invalidated = true;
    }
}

/// Data Transfer 的 JobHandler 实现。kind = `dataTransferPrepare` / `dataTransferApply`。
pub struct DataTransferHandler {
    kind: &'static str,
    pub(super) freeze: TransferFreezeBody,
    pub(super) inspected: Vec<TableInspectResult>,
    pub(super) source_schemas: HashMap<String, TableSchema>,
    target_schemas: HashMap<String, TableSchema>,
    pub(super) endpoints: TransferEndpoints,
    registry: SyncAdapterRegistry,
    database_structure: Option<Vec<DdlPreviewItem>>,
}

impl DataTransferHandler {
    pub fn prepare(
        freeze: TransferFreezeBody,
        inspected: Vec<TableInspectResult>,
        source_schemas: HashMap<String, TableSchema>,
        endpoints: TransferEndpoints,
    ) -> Self {
        Self {
            kind: "dataTransferPrepare",
            freeze,
            inspected,
            source_schemas,
            target_schemas: HashMap::new(),
            endpoints,
            registry: SyncAdapterRegistry::new(),
            database_structure: None,
        }
    }

    pub fn apply(
        freeze: TransferFreezeBody,
        inspected: Vec<TableInspectResult>,
        source_schemas: HashMap<String, TableSchema>,
        target_schemas: HashMap<String, TableSchema>,
        endpoints: TransferEndpoints,
        database_structure: Option<Vec<DdlPreviewItem>>,
    ) -> Self {
        Self {
            kind: "dataTransferApply",
            freeze,
            inspected,
            source_schemas,
            target_schemas,
            endpoints,
            registry: SyncAdapterRegistry::new(),
            database_structure,
        }
    }

    /// 冻结体裁定的恢复政策（§7）。runtime 落 checkpoint 时写死的是它自己的默认值，
    /// 真正生效的裁决必须来自这份冻结体，所以核验前先把政策对齐回来。
    pub fn recovery_policy(&self) -> &str {
        &self.freeze.recovery_policy
    }

    /// 准备期是否证明了源端一致性快照。
    pub fn snapshot_proven(&self) -> bool {
        self.freeze.snapshot_proven
    }

    /// 取消事实只有一个来源：runtime 交给 `run_stage` 的那个 `CancelToken`。
    /// 阶段内取消由内核的阶段看守者持续把仓储里的 `cancel_requested` 翻译进这一位，
    /// 所以管道在批次边界读到的和阶段边界读到的是同一份事实，不再需要自建标志位。
    fn cancel_requested(&self, cancel: &CancelToken) -> bool {
        cancel.is_cancelled()
    }

    fn is_apply(&self) -> bool {
        self.kind == "dataTransferApply"
    }

    fn validate_shape(&self) -> Result<(), JobError> {
        for table in self.freeze.job.tables.iter().filter(|t| t.enabled) {
            if table.column_mappings.iter().all(|m| m.skip) {
                return Err(JobError::PlanProjectionInvalid(format!(
                    "table '{}' has no active column mappings",
                    table.source_table
                )));
            }
        }
        Ok(())
    }
}

#[async_trait]
impl JobHandler for DataTransferHandler {
    fn kind(&self) -> &str {
        self.kind
    }

    fn handler_version(&self) -> u64 {
        crate::job::plan::HANDLER_VERSION
    }

    fn validate_plan(&self, plan: &FrozenPlan) -> Result<Vec<StageSpec>, JobError> {
        validate_frozen_plan(plan, &self.freeze, self.is_apply())?;
        self.validate_shape()?;
        // SQL 文件目标：apply 只发射一个 "apply" 阶段（结构+数据+产物由 sql_file 引擎统一完成）。
        if matches!(self.endpoints, TransferEndpoints::SqlFile { .. })
            && self.kind == "dataTransferApply"
        {
            return Ok(vec![StageSpec {
                stage_id: StageId::new("apply"),
                kind: "apply".into(),
                depends_on: vec![],
            }]);
        }
        let stages = match (self.kind, self.freeze.job.mode) {
            ("dataTransferPrepare", _) => vec![StageSpec {
                stage_id: StageId::new("prepare"),
                kind: "prepare".into(),
                depends_on: vec![],
            }],
            ("dataTransferApply", TransferMode::Structure) => vec![StageSpec {
                stage_id: StageId::new("structure"),
                kind: "structure".into(),
                depends_on: vec![],
            }],
            ("dataTransferApply", TransferMode::Data) => vec![StageSpec {
                stage_id: StageId::new("data"),
                kind: "data".into(),
                depends_on: vec![],
            }],
            ("dataTransferApply", TransferMode::StructureAndData) => vec![
                StageSpec {
                    stage_id: StageId::new("structure"),
                    kind: "structure".into(),
                    depends_on: vec![],
                },
                StageSpec {
                    stage_id: StageId::new("data"),
                    kind: "data".into(),
                    depends_on: vec![StageId::new("structure")],
                },
                StageSpec {
                    stage_id: StageId::new("foreignKeys"),
                    kind: "foreignKeys".into(),
                    depends_on: vec![StageId::new("data")],
                },
            ],
            _ => vec![],
        };

        Ok(stages)
    }

    async fn run_stage(
        &self,
        spec: &StageSpec,
        cancel: &CancelToken,
    ) -> Result<StageOutcome, JobError> {
        match (self.kind, spec.kind.as_str()) {
            ("dataTransferPrepare", "prepare") => self.run_prepare(cancel).await,
            ("dataTransferApply", "structure") => self.run_structure(spec, cancel).await,
            ("dataTransferApply", "data") => self.run_data(spec, cancel).await,
            ("dataTransferApply", "foreignKeys") => self.run_foreign_keys(spec, cancel).await,
            ("dataTransferApply", "apply") => self.run_sql_file(spec, cancel).await,
            _ => Err(JobError::PlanProjectionInvalid(format!(
                "stage kind '{}' does not match handler kind '{}'",
                spec.kind, self.kind
            ))),
        }
    }

    fn verify_recovery(&self, checkpoint: &Checkpoint) -> RecoveryVerdict {
        // 冻结体的政策优先：runtime 落 checkpoint 时把 `recovery_policy` 写死成
        // `resumeAfterVerify`，若照抄它就能绕过准备期「不许自动续写」的裁决。
        let mut aligned = checkpoint.clone();
        aligned.recovery_policy = self.freeze.recovery_policy.clone();
        verify_checkpoint(&aligned)
    }
}

impl DataTransferHandler {
    async fn run_prepare(&self, cancel: &CancelToken) -> Result<StageOutcome, JobError> {
        let mut evidence: Vec<String> = Vec::new();
        for table in self.freeze.job.tables.iter().filter(|t| t.enabled) {
            if self.cancel_requested(cancel) {
                return Ok(StageOutcome {
                    stage_id: StageId::new("prepare"),
                    terminal: StageTerminal::Cancelled,
                    progress: JobProgress::default(),
                    commit_boundaries: Vec::new(),
                    execution_ids: Vec::new(),
                    artifact_ids: Vec::new(),
                    effect_outcome: EffectOutcome::NotStarted,
                    error_code: None,
                });
            }
            let (keys, snapshot_proven) = match self.source_schemas.get(&table.source_table) {
                Some(schema) => (
                    schema.effective_primary_keys(),
                    schema.table_options.supports_consistent_snapshot == Some(true),
                ),
                None => {
                    let keys = self
                        .inspected
                        .iter()
                        .find(|t| t.source_table == table.source_table)
                        .map(|t| t.source_primary_keys.clone())
                        .unwrap_or_default();
                    (keys, false)
                }
            };
            let key_complete = !keys.is_empty();
            if !key_complete || !snapshot_proven {
                evidence.push(format!(
                    "table '{}': stableKeyComplete={key_complete}, snapshotProven={snapshot_proven}; recovery policy must forbid automatic resume",
                    table.source_table
                ));
            } else {
                evidence.push(format!(
                    "table '{}': stableKeyComplete=true, snapshotProven=true; resumeAfterVerify allowed",
                    table.source_table
                ));
            }
        }
        let artifact_id = format!("transfer-prepare-{}", uuid::Uuid::new_v4());
        Ok(StageOutcome {
            stage_id: StageId::new("prepare"),
            terminal: StageTerminal::Succeeded,
            progress: JobProgress::default(),
            commit_boundaries: Vec::new(),
            execution_ids: Vec::new(),
            artifact_ids: vec![ArtifactId::new(artifact_id)],
            effect_outcome: EffectOutcome::Completed,
            error_code: None,
        })
    }

    async fn run_structure(
        &self,
        spec: &StageSpec,
        cancel: &CancelToken,
    ) -> Result<StageOutcome, JobError> {
        match &self.endpoints {
            TransferEndpoints::Database {
                target_driver,
                target_handle,
                ..
            } => {
                let selected: HashSet<String> = self
                    .freeze
                    .job
                    .tables
                    .iter()
                    .filter(|t| t.enabled)
                    .map(|t| t.source_table.clone())
                    .collect();
                let plan = self.database_structure.as_deref().unwrap_or(&[]);
                // 共享内核那一位：阶段内取消由内核看守者翻转，结构阶段在每个 DDL 之前读的就是它。
                let atomic_cancel = cancel.flag();
                let results = crate::structure::execute_database_structure_plan(
                    target_driver.as_ref(),
                    target_handle,
                    plan,
                    &selected,
                    crate::structure::DatabaseStructurePhase::Prepare,
                    Some(atomic_cancel),
                    None,
                )
                .await;
                let mut boundaries = Vec::new();
                for item in plan.iter().filter(|item| {
                    selected.contains(&item.source_table)
                        && item.kind != crate::model::DdlPreviewKind::ForeignKey
                }) {
                    let committed = results.iter().any(|r| {
                        r.source_table == item.source_table
                            && r.outcome == Some(TableExecutionOutcome::Committed)
                    });
                    if committed {
                        boundaries.push(commit_boundary(
                            spec.stage_id.as_str(),
                            &item.source_table,
                            &{
                                use sha2::{Digest, Sha256};
                                let mut h = Sha256::new();
                                h.update(item.ddl.as_bytes());
                                format!("{:x}", h.finalize())
                            },
                            0,
                            "target-ddl-ack",
                            "sha256:structure",
                            None,
                        ));
                    }
                }
                let succeeded = results
                    .iter()
                    .all(|r| r.outcome == Some(TableExecutionOutcome::Committed));
                Ok(StageOutcome {
                    stage_id: spec.stage_id.clone(),
                    terminal: if succeeded {
                        StageTerminal::Succeeded
                    } else {
                        StageTerminal::Failed
                    },
                    progress: JobProgress::default(),
                    commit_boundaries: boundaries,
                    execution_ids: Vec::new(),
                    artifact_ids: Vec::new(),
                    effect_outcome: if succeeded {
                        EffectOutcome::Completed
                    } else {
                        EffectOutcome::PartiallyApplied
                    },
                    error_code: None,
                })
            }
            TransferEndpoints::SqlFile { .. } => Ok(StageOutcome {
                stage_id: spec.stage_id.clone(),
                terminal: StageTerminal::Succeeded,
                progress: JobProgress::default(),
                commit_boundaries: Vec::new(),
                execution_ids: Vec::new(),
                artifact_ids: Vec::new(),
                effect_outcome: EffectOutcome::Completed,
                error_code: None,
            }),
        }
    }

    async fn run_foreign_keys(
        &self,
        spec: &StageSpec,
        _cancel: &CancelToken,
    ) -> Result<StageOutcome, JobError> {
        match &self.endpoints {
            TransferEndpoints::Database {
                target_driver,
                target_handle,
                ..
            } => {
                let selected: HashSet<String> = self
                    .freeze
                    .job
                    .tables
                    .iter()
                    .filter(|t| t.enabled)
                    .map(|t| t.source_table.clone())
                    .collect();
                let plan = self.database_structure.as_deref().unwrap_or(&[]);
                let results = crate::structure::execute_database_structure_plan(
                    target_driver.as_ref(),
                    target_handle,
                    plan,
                    &selected,
                    crate::structure::DatabaseStructurePhase::ForeignKeys,
                    None,
                    None,
                )
                .await;
                let succeeded = results
                    .iter()
                    .all(|r| r.outcome == Some(TableExecutionOutcome::Committed));
                Ok(StageOutcome {
                    stage_id: spec.stage_id.clone(),
                    terminal: if succeeded {
                        StageTerminal::Succeeded
                    } else {
                        StageTerminal::Failed
                    },
                    progress: JobProgress::default(),
                    commit_boundaries: Vec::new(),
                    execution_ids: Vec::new(),
                    artifact_ids: Vec::new(),
                    effect_outcome: if succeeded {
                        EffectOutcome::Completed
                    } else {
                        EffectOutcome::PartiallyApplied
                    },
                    error_code: None,
                })
            }
            TransferEndpoints::SqlFile { .. } => Ok(StageOutcome {
                stage_id: spec.stage_id.clone(),
                terminal: StageTerminal::Succeeded,
                progress: JobProgress::default(),
                commit_boundaries: Vec::new(),
                execution_ids: Vec::new(),
                artifact_ids: Vec::new(),
                effect_outcome: EffectOutcome::Completed,
                error_code: None,
            }),
        }
    }
}

impl DataTransferHandler {
    async fn run_data(
        &self,
        spec: &StageSpec,
        cancel: &CancelToken,
    ) -> Result<StageOutcome, JobError> {
        match &self.endpoints {
            TransferEndpoints::Database {
                source_driver,
                source_handle,
                target_driver,
                target_handle,
                source_type,
                target_type,
            } => {
                if let Err(err) = self.registry.ensure_pair(source_type, target_type) {
                    return Ok(failed_stage(spec, err));
                }
                let pairing =
                    match crate::pairing::enforce_transfer_pairing(source_type, target_type) {
                        Ok(pairing) => pairing,
                        Err(err) => return Ok(failed_stage(spec, err.to_string())),
                    };
                let mut table_ir_types: HashMap<
                    String,
                    HashMap<String, crate::transfer::ir::IRType>,
                > = HashMap::new();
                let adapters_src = self.registry.get_source(source_type);
                let adapters_tgt = self.registry.get_target(target_type);
                let is_ir = matches!(pairing, crate::transfer::pairing::SyncPairing::Ir);
                if is_ir {
                    match (&adapters_src, &adapters_tgt) {
                        (Some(src_source), Some(_tgt_target)) => {
                            for t in self.inspected.iter().filter(|t| t.enabled) {
                                if let Some(schema) = self.source_schemas.get(&t.source_table) {
                                    let ir = crate::structure::source_schema_to_target_ir(
                                        src_source.as_ref(),
                                        schema,
                                        None,
                                        &t.target_table,
                                    );
                                    table_ir_types.insert(
                                        t.source_table.clone(),
                                        crate::structure::column_ir_types_by_source(&ir),
                                    );
                                }
                            }
                        }
                        _ => {
                            return Ok(failed_stage(
                                spec,
                                "cross-family execute requires IR sync adapters",
                            ))
                        }
                    }
                }
                let mut all_boundaries = Vec::new();
                let mut progress = JobProgress::default();
                let mut any_failed = false;
                let mut any_cancelled = false;
                let mut checkpoint = InMemoryTransferCheckpoint::new();
                for table in self
                    .inspected
                    .iter()
                    .filter(|t| crate::structure::table_eligible_for_data(t, &self.freeze.job))
                {
                    if self.cancel_requested(cancel) {
                        any_cancelled = true;
                        break;
                    }
                    let Some(src_schema) = self.source_schemas.get(&table.source_table).cloned()
                    else {
                        continue;
                    };
                    let tgt_schema = match self.target_schemas.get(&table.target_table) {
                        Some(s) => s.clone(),
                        None => match target_driver
                            .get_table_schema(
                                target_handle,
                                &table.target_table,
                                &self
                                    .freeze
                                    .job
                                    .database_target()
                                    .map_err(|e| JobError::PlanProjectionInvalid(e.to_string()))?
                                    .database,
                                self.freeze
                                    .job
                                    .database_target()
                                    .map_err(|e| JobError::PlanProjectionInvalid(e.to_string()))?
                                    .normalized_schema(),
                            )
                            .await
                        {
                            Ok(s) => s,
                            Err(_) => continue,
                        },
                    };
                    let columns: Vec<&crate::model::ColumnMapping> =
                        table.column_mappings.iter().filter(|m| !m.skip).collect();
                    let mapping = self
                        .freeze
                        .job
                        .tables
                        .iter()
                        .find(|m| m.source_table == table.source_table);
                    let src_family_quote = source_driver.quote_char();
                    let source_scope = match crate::recordset::build_source_scope(
                        &src_schema,
                        mapping.and_then(|m| m.source_filter.as_ref()),
                        mapping.and_then(|m| m.recordset.as_ref()),
                        src_family_quote,
                        source_type,
                        |index, data_type| {
                            source_driver
                                .parameter_placeholder(index, data_type)
                                .map_err(|e| TransferError::unsupported(e.to_string()))
                        },
                        |column| {
                            src_schema
                                .columns
                                .iter()
                                .find(|c| c.name == column)
                                .map(|c| c.data_type.clone())
                        },
                    ) {
                        Ok(scope) => scope,
                        Err(error) => return Ok(failed_stage(spec, error.to_string())),
                    };
                    let formatter = if is_ir {
                        match adapters_tgt.as_deref() {
                            Some(tgt_target) => crate::execute::ValueFormatter::Ir {
                                tgt_adapter: tgt_target,
                                source_column_ir_types: &table_ir_types,
                            },
                            None => {
                                return Ok(failed_stage(
                                    spec,
                                    "missing target adapter for IR formatter",
                                ))
                            }
                        }
                    } else {
                        crate::execute::ValueFormatter::SameFamily
                    };
                    let formatter_ref: &crate::execute::ValueFormatter<'_> = &formatter;
                    let mut context = BoundedPipelineContext {
                        job: &self.freeze.job,
                        table,
                        source_schema: &src_schema,
                        target_schema: &tgt_schema,
                        source_driver: source_driver.as_ref(),
                        source_handle,
                        target_driver: target_driver.as_ref(),
                        target_handle,
                        source_scope: &source_scope,
                        source_table_ref: &table.source_table,
                        target_table_ref: &table.target_table,
                        source_quote: src_family_quote,
                        target_type,
                        columns: &columns,
                        formatter: formatter_ref,
                        cancelled: Some(cancel.flag()),
                        write_started: None,
                        checkpoint: &mut checkpoint,
                    };
                    match execute_bounded_table(&mut context).await {
                        Ok(o) => {
                            all_boundaries.extend(o.boundaries);
                            absorb_progress(&mut progress, &o.progress);
                            // §6.2：缓冲不得越过 8 MiB 预算；越界说明字节账失真，
                            // 必须显式失败，但已确认的提交边界按 §7 保留。
                            if o.max_buffer_bytes > PIPELINE_INITIAL_BYTES {
                                return Ok(unbounded_stage(
                                    spec,
                                    &all_boundaries,
                                    o.max_buffer_bytes,
                                ));
                            }
                            tracing::debug!(
                                source_table = table.source_table.as_str(),
                                peak_bytes = o.max_buffer_bytes,
                                "bounded pipeline peak buffer"
                            );
                            if !o.result.result.success {
                                any_failed = true;
                            }
                            if o.result.cancelled {
                                any_cancelled = true;
                            }
                        }
                        Err(error) => {
                            // 取消是既成事实：管道用 Cancelled 错误上抛时，不能降级成失败
                            // 态——§7 要求已确认的那部分保留在边界里。
                            if matches!(error, TransferError::Cancelled(_)) {
                                return Ok(cancelled_stage(
                                    spec,
                                    &all_boundaries,
                                    error.to_string(),
                                ));
                            }
                            // 阶段级错误直接给出失败态，无需再累加 any_failed。
                            return Ok(failed_stage(spec, error.to_string()));
                        }
                    }
                }
                Ok(StageOutcome {
                    stage_id: spec.stage_id.clone(),
                    // 取消优先于失败：取消是用户事实，§7 要求保留已确认的那部分，
                    // 运行时按「cancelled + 有边界」判成 PartiallyApplied。
                    terminal: if any_cancelled {
                        StageTerminal::Cancelled
                    } else if any_failed {
                        StageTerminal::Failed
                    } else {
                        StageTerminal::Succeeded
                    },
                    progress,
                    commit_boundaries: all_boundaries,
                    execution_ids: Vec::new(),
                    artifact_ids: Vec::new(),
                    effect_outcome: if any_failed || any_cancelled {
                        EffectOutcome::PartiallyApplied
                    } else {
                        EffectOutcome::Completed
                    },
                    error_code: None,
                })
            }
            TransferEndpoints::SqlFile { .. } => Ok(StageOutcome {
                stage_id: spec.stage_id.clone(),
                terminal: StageTerminal::Succeeded,
                progress: JobProgress::default(),
                commit_boundaries: Vec::new(),
                execution_ids: Vec::new(),
                artifact_ids: Vec::new(),
                effect_outcome: EffectOutcome::Completed,
                error_code: None,
            }),
        }
    }
}
