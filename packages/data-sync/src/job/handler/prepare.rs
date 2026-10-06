//! prepare 阶段实现：动态门闸 → 双端只读快照 → 完整 PK keyset 比较 → 冻结
//! ChangeSet Artifact → 释放快照资源（§5、§7）。
//!
//! 产物经 `host.store_artifact` 落成不可变 Artifact，planId + digest +
//! structure_fingerprint 一起返回；apply 阶段只凭 planId + selectionRevision
//! 回读，不依赖此处任何内存状态。

use std::sync::Arc;

use async_trait::async_trait;
use datazen_driver_api::{SyncKeyValue, Value};
use datazen_platform_api::dto::execution::{EffectOutcome, ExecutionErrorCode};
use datazen_platform_api::dto::job::JobProgress;
use datazen_platform_api::id::{ArtifactId, Counter, ExecutionId, StageId};
use datazen_runtime::job::{CancelToken, JobError, StageOutcome, StageTerminal};
use sha2::{Digest, Sha256};

use crate::compare::{compare_table_pages_to_sink, RowChangeSink, RowPageSource};
use crate::error::DataSyncError;
use crate::gate::check_table_gate;
use crate::model::{Row, RowChange};

use super::super::artifact::{
    artifact_digest, structure_fingerprint, ChangeBlock, ChangeSetArtifact, ColumnMeta,
    RelationIdentity, TableMeta,
};
use super::super::body::PrepareSpec;
use super::super::host::{DataSyncHost, EndpointSession, KeysetPageSource};
use super::{stage_failed, DataSyncHandler};

/// 将 Box<dyn KeysetPageSource> 适配为 RowPageSource，使 compare_table_pages_to_sink
/// 可直接使用 host 提供的读取器。
struct PageSourceAdapter(Box<dyn KeysetPageSource>);

#[async_trait]
impl RowPageSource for PageSourceAdapter {
    async fn next_page(
        &mut self,
        after_key: Option<&[Value]>,
        limit: u32,
    ) -> Result<Vec<Row>, DataSyncError> {
        self.0.next_page(after_key, limit).await
    }

    fn normalize_key(&self, key: &[Value]) -> Result<Vec<SyncKeyValue>, DataSyncError> {
        self.0.normalize_key(key)
    }
}

/// 把比较产物（RowChange）收集为不可变 ChangeBlock 的 sink。
struct BlockSink {
    relation: RelationIdentity,
    source_table: String,
    column_names: Vec<String>,
    pk_columns: Vec<String>,
    blocks: Vec<ChangeBlock>,
}

#[async_trait]
impl RowChangeSink for BlockSink {
    async fn push(&mut self, change: RowChange) -> Result<(), DataSyncError> {
        self.blocks.push(ChangeBlock {
            relation: self.relation.clone(),
            source_table: self.source_table.clone(),
            operation: change.operation,
            key: change.key,
            before: change.target_row,
            after: change.source_row,
            column_names: self.column_names.clone(),
            pk_columns: self.pk_columns.clone(),
            changed_columns: change.changed_columns,
        });
        Ok(())
    }

    async fn unchanged(&mut self) -> Result<(), DataSyncError> {
        Ok(())
    }
}
impl DataSyncHandler {
    /// prepare：门闸 → keyset 比较 → ChangeSet Artifact → 释放快照/会话。
    pub(super) async fn run_prepare(&self, cancel: &CancelToken) -> Result<StageOutcome, JobError> {
        let Self::Prepare { spec, host } = self else {
            return Err(JobError::PlanProjectionInvalid(
                "run_prepare on an apply handler".into(),
            ));
        };
        let source = match host.open_endpoint(&spec.source).await {
            Ok(s) => s,
            Err(e) => {
                return Ok(stage_failed(
                    host.as_ref(),
                    "prepare",
                    EffectOutcome::NotStarted,
                    ExecutionErrorCode::HostRejected,
                    e,
                    Vec::new(),
                    JobProgress::default(),
                ));
            }
        };
        let target = match host.open_endpoint(&spec.target).await {
            Ok(t) => t,
            Err(e) => {
                host.close_endpoint(source).await;
                return Ok(stage_failed(
                    host.as_ref(),
                    "prepare",
                    EffectOutcome::NotStarted,
                    ExecutionErrorCode::HostRejected,
                    e,
                    Vec::new(),
                    JobProgress::default(),
                ));
            }
        };

        let result = self
            .prepare_collect(spec, host, &source, &target, cancel)
            .await;

        host.close_endpoint(source).await;
        host.close_endpoint(target).await;

        match result {
            Ok((artifact, artifact_id, progress)) => Ok(StageOutcome {
                stage_id: StageId::new("prepare"),
                terminal: StageTerminal::Succeeded,
                progress,
                commit_boundaries: Vec::new(),
                execution_ids: vec![ExecutionId::new(format!(
                    "exec-prepare-{}",
                    artifact.plan_id
                ))],
                artifact_ids: vec![artifact_id],
                effect_outcome: EffectOutcome::Completed,
                error_code: None,
            }),
            Err((effect, code, err)) => Ok(stage_failed(
                host.as_ref(),
                "prepare",
                effect,
                code,
                err,
                Vec::new(),
                JobProgress::default(),
            )),
        }
    }

    /// 比较主体：family 门闸 → 结构/PK 门闸 → 逐表 keyset 比较 → 块集合 → 指纹 → store。
    pub(super) async fn prepare_collect(
        &self,
        spec: &PrepareSpec,
        host: &Arc<dyn DataSyncHost>,
        source: &EndpointSession,
        target: &EndpointSession,
        cancel: &CancelToken,
    ) -> Result<
        (ChangeSetArtifact, ArtifactId, JobProgress),
        (EffectOutcome, ExecutionErrorCode, DataSyncError),
    > {
        // 家族一致
        if source.family != target.family {
            return Err((
                EffectOutcome::NotStarted,
                ExecutionErrorCode::HostRejected,
                DataSyncError::incompatible(format!(
                    "family mismatch: {} vs {} — 跨族请用 Data Transfer",
                    source.family, target.family
                )),
            ));
        }
        // 稳定读快照：不支持的基础设施时拒绝声称一致快照
        let snap_src = source
            .driver
            .begin_read_snapshot(&source.handle)
            .await
            .map_err(|e| {
                (
                    EffectOutcome::NotStarted,
                    ExecutionErrorCode::HostRejected,
                    DataSyncError::incompatible(format!(
                        "source does not support stable read snapshot: {e}"
                    )),
                )
            })?;
        let snap_tgt = match target.driver.begin_read_snapshot(&target.handle).await {
            Ok(s) => s,
            Err(e) => {
                let _ = source.driver.rollback(snap_src).await;
                return Err((
                    EffectOutcome::NotStarted,
                    ExecutionErrorCode::HostRejected,
                    DataSyncError::incompatible(format!(
                        "target does not support stable read snapshot: {e}"
                    )),
                ));
            }
        };
        let result = self
            .prepare_compare_all(spec, host, source, target, cancel)
            .await;
        let _ = source.driver.rollback(snap_src).await;
        let _ = target.driver.rollback(snap_tgt).await;
        result
    }

    /// 在快照内做结构门闸与逐表比较。
    pub(super) async fn prepare_compare_all(
        &self,
        spec: &PrepareSpec,
        host: &Arc<dyn DataSyncHost>,
        source: &EndpointSession,
        target: &EndpointSession,
        cancel: &CancelToken,
    ) -> Result<
        (ChangeSetArtifact, ArtifactId, JobProgress),
        (EffectOutcome, ExecutionErrorCode, DataSyncError),
    > {
        let pairs = host
            .mapped_table_schemas(source, target, &spec.mappings)
            .await
            .map_err(|e| {
                (
                    EffectOutcome::NotStarted,
                    ExecutionErrorCode::HostRejected,
                    e,
                )
            })?;
        for pair in &pairs {
            let verdict = check_table_gate(&source.family, &pair.source, &pair.target);
            if !verdict.is_compatible() {
                return Err((
                    EffectOutcome::NotStarted,
                    ExecutionErrorCode::HostRejected,
                    DataSyncError::incompatible(format!(
                        "table {}: {} — 结构不符请用 Schema Diff",
                        pair.source_table,
                        verdict.reason_text()
                    )),
                ));
            }
        }
        let mut blocks = Vec::new();
        let mut read_count = 0u64;
        let mut converted = 0u64;
        let mut table_meta = Vec::new();
        for pair in &pairs {
            let mapping = spec.mappings.iter().find(|m| {
                m.enabled
                    && m.target_table == pair.target_table
                    && m.source_table == pair.source_table
            });
            let Some(_mapping) = mapping else { continue };
            // 完整 PK 门闸：pk 列必须都在 columns 中出现
            let pk_indexes: Vec<usize> = pair
                .source
                .primary_keys
                .iter()
                .map(|pk| pair.source.columns.iter().position(|c| &c.name == pk))
                .collect::<Option<Vec<_>>>()
                .ok_or((
                    EffectOutcome::NotStarted,
                    ExecutionErrorCode::HostRejected,
                    DataSyncError::incompatible(format!(
                        "table {}: 缺少完整 PK — 请先用 Schema Diff 处理结构",
                        pair.source_table
                    )),
                ))?;
            if pk_indexes.is_empty() {
                return Err((
                    EffectOutcome::NotStarted,
                    ExecutionErrorCode::HostRejected,
                    DataSyncError::incompatible(format!(
                        "table {}: 缺少完整 PK — 请先用 Schema Diff 处理结构",
                        pair.source_table
                    )),
                ));
            }
            let column_names: Vec<String> =
                pair.source.columns.iter().map(|c| c.name.clone()).collect();
            let source_filter = spec.filters.get(&pair.source_table).cloned();
            let src_reader = host
                .table_reader(source, &pair.source_table, source_filter.as_ref())
                .await
                .map_err(|e| {
                    (
                        EffectOutcome::NotStarted,
                        ExecutionErrorCode::HostRejected,
                        e,
                    )
                })?;
            let tgt_reader = host
                .table_reader(target, &pair.target_table, None)
                .await
                .map_err(|e| {
                    (
                        EffectOutcome::NotStarted,
                        ExecutionErrorCode::HostRejected,
                        e,
                    )
                })?;
            let mut src = PageSourceAdapter(src_reader);
            let mut tgt = PageSourceAdapter(tgt_reader);
            let mut sink = BlockSink {
                relation: RelationIdentity {
                    database: target.database.clone(),
                    schema: target.schema.clone(),
                    table: pair.target_table.clone(),
                },
                source_table: pair.source_table.clone(),
                column_names: column_names.clone(),
                pk_columns: pair.source.primary_keys.clone(),
                blocks: Vec::new(),
            };
            // RowPageSource::next_page 的取消信号在调用时镜像 CancelToken
            let cancel_flag = Arc::new(std::sync::atomic::AtomicBool::new(cancel.is_cancelled()));
            let table_result = compare_table_pages_to_sink(
                &pair.source_table,
                &pair.target_table,
                &pk_indexes,
                &column_names,
                &spec.options,
                &mut src,
                &mut tgt,
                Some(cancel_flag),
                &mut sink,
            )
            .await
            .map_err(|e| {
                (
                    EffectOutcome::NotStarted,
                    ExecutionErrorCode::HostRejected,
                    e,
                )
            })?;
            read_count += (table_result.insert_count()
                + table_result.update_count()
                + table_result.delete_count()
                + table_result.unchanged_row_count()) as u64;
            converted += sink.blocks.len() as u64;
            blocks.extend(sink.blocks);
            table_meta.push(TableMeta {
                relation: RelationIdentity {
                    database: target.database.clone(),
                    schema: target.schema.clone(),
                    table: pair.target_table.clone(),
                },
                columns: pair
                    .source
                    .columns
                    .iter()
                    .map(|c| ColumnMeta {
                        name: c.name.clone(),
                        data_type: c.data_type.clone(),
                    })
                    .collect(),
                pk_columns: pair.source.primary_keys.clone(),
                unchanged_count: table_result.unchanged_row_count(),
                source_filter,
            });
        }
        // plan_id：host 显式指定则优先；缺省 handler 确定性生成（endpoint+mappings+时间）
        let plan_id = spec.plan_id.clone().unwrap_or_else(|| {
            let mut hasher = Sha256::new();
            hasher.update(spec.source.database.as_bytes());
            hasher.update(spec.target.database.as_bytes());
            for m in &spec.mappings {
                hasher.update(m.source_table.as_bytes());
                hasher.update(m.target_table.as_bytes());
            }
            hasher.update(host.now().as_bytes());
            format!("plan-{:x}", hasher.finalize())[..21].to_string()
        });
        let digest = artifact_digest(&blocks, &spec.source, &spec.target);
        let structure_fp = structure_fingerprint(pairs.iter().flat_map(|p| [&p.source, &p.target]));
        let artifact = ChangeSetArtifact {
            plan_id: plan_id.clone(),
            source: spec.source.clone(),
            target: spec.target.clone(),
            family: source.family.clone(),
            mappings: spec.mappings.clone(),
            blocks,
            table_meta,
            structure_fingerprint: structure_fp,
            digest,
            created_at: host.now(),
        };
        let guard = host.store_artifact(artifact.clone()).await.map_err(|e| {
            (
                EffectOutcome::NotStarted,
                ExecutionErrorCode::HostRejected,
                e,
            )
        })?;
        let artifact_id = ArtifactId::new(guard.artifact_id);
        let progress = JobProgress {
            read: Counter::new(read_count),
            converted: Counter::new(converted),
            attempted: Counter::new(0),
            committed: Counter::new(0),
            unknown: Counter::new(0),
        };
        Ok((artifact, artifact_id, progress))
    }
}
