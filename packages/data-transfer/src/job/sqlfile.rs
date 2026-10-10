//! SQL 文件目标的 apply 阶段（CM-49）：无目标连接，只产出可复现的 SQL 产物。
//!
//! 产物规格（§6.3）：同一份映射与同一份源元数据必须落成同一份 SQL 文件，
//! 因此成功终态必须携带一条 artifact 提交边界（内容 sha256 + 行数 + 写入口）。

use datazen_platform_api::dto::execution::{EffectOutcome, ExecutionErrorCode};
use datazen_platform_api::dto::job::JobProgress;
use datazen_platform_api::id::ArtifactId;

use datazen_runtime::job::{CancelToken, JobError, StageOutcome, StageSpec, StageTerminal};

use crate::job::checkpoint::commit_boundary;
use crate::job::handler::{DataTransferHandler, TransferEndpoints};

impl DataTransferHandler {
    pub(crate) async fn run_sql_file(
        &self,
        spec: &StageSpec,
        cancel: &CancelToken,
    ) -> Result<StageOutcome, JobError> {
        match &self.endpoints {
            TransferEndpoints::SqlFile {
                source_driver,
                source_handle,
                destination,
                structure,
                source_adapter,
                target_adapter,
                ..
            } => {
                // SQL 文件目标没有目标连接；产物完整性由 writer finalize 计算。
                // 直接共享内核那一位：阶段内取消由内核看守者翻转，写入循环读的就是同一份事实。
                let atomic_cancel = cancel.flag();
                let result = crate::sql_file::execute_with_target(
                    source_driver.as_ref(),
                    source_driver.as_ref(),
                    source_adapter.as_deref(),
                    target_adapter.as_deref(),
                    source_handle,
                    &self.freeze.job,
                    &self.inspected,
                    &self.source_schemas,
                    destination.clone(),
                    Some(atomic_cancel),
                    structure.as_deref(),
                )
                .await;
                match result {
                    Ok(res) if res.cancelled || res.partial => {
                        let cancelled = res.cancelled;
                        tracing::warn!(
                            destination = destination.display().to_string(),
                            cancelled,
                            table_errors = ?res.tables.iter().filter_map(|table| table.error.as_deref()).collect::<Vec<_>>(),
                            "SQL file was not published because generation was incomplete"
                        );
                        Ok(StageOutcome {
                            stage_id: spec.stage_id.clone(),
                            terminal: if cancelled {
                                StageTerminal::Cancelled
                            } else {
                                StageTerminal::Failed
                            },
                            progress: JobProgress::default(),
                            commit_boundaries: Vec::new(),
                            execution_ids: Vec::new(),
                            artifact_ids: Vec::new(),
                            effect_outcome: EffectOutcome::RolledBack,
                            error_code: Some(if cancelled {
                                ExecutionErrorCode::Cancelled
                            } else {
                                ExecutionErrorCode::SqlError
                            }),
                        })
                    }
                    Ok(res) => {
                        let digest = artifact_digest(destination)?;
                        let boundary = commit_boundary(
                            spec.stage_id.as_str(),
                            "artifact",
                            &digest,
                            res.rows_inserted,
                            "artifact-writer-finalize",
                            "sha256:sql-file-spec",
                            None,
                        );
                        Ok(StageOutcome {
                            stage_id: spec.stage_id.clone(),
                            terminal: if res.cancelled {
                                StageTerminal::Cancelled
                            } else {
                                StageTerminal::Succeeded
                            },
                            progress: JobProgress::default(),
                            commit_boundaries: vec![boundary],
                            execution_ids: Vec::new(),
                            artifact_ids: vec![ArtifactId::new(format!("transfer-sql-{digest}"))],
                            effect_outcome: if res.cancelled {
                                EffectOutcome::PartiallyApplied
                            } else {
                                EffectOutcome::Completed
                            },
                            error_code: None,
                        })
                    }
                    Err(error) => {
                        tracing::error!(
                            destination = destination.display().to_string(),
                            error = error.to_string(),
                            "sql file artifact generation failed"
                        );
                        Ok(StageOutcome {
                            stage_id: spec.stage_id.clone(),
                            terminal: StageTerminal::Failed,
                            progress: JobProgress::default(),
                            commit_boundaries: Vec::new(),
                            execution_ids: Vec::new(),
                            artifact_ids: Vec::new(),
                            effect_outcome: EffectOutcome::RolledBack,
                            error_code: Some(ExecutionErrorCode::SqlError),
                        })
                    }
                }
            }
            TransferEndpoints::Database { .. } => Err(JobError::PlanProjectionInvalid(
                "sql_file stage requires SQL file endpoints".into(),
            )),
        }
    }
}

/// 产物指纹即 Artifact id：内容寻址让「同一规格 → 同一产物」可被下游核对。
fn artifact_digest(path: &std::path::Path) -> Result<String, JobError> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut file = std::fs::File::open(path)
        .map_err(|error| JobError::PlanProjectionInvalid(error.to_string()))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|error| JobError::PlanProjectionInvalid(error.to_string()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}
