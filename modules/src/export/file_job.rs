//! Durable read-only file jobs: output is published as an owned Artifact, never a client path.
//!
//! D6 item 5 切片 a 自 modules `file_job` 归位（行为用例留 modules
//! 集成测试——持有 datazen dev-dep，垫片保留先例）。
use super::error::ExportError as FeatureError;
use crate::execution::domain::{
    AbortReason, ArtifactRepository, ArtifactSpec, ChunkPayload, EffectOutcome, ExecutionErrorCode,
};
use crate::jobs::domain::job::Checkpoint;
use crate::jobs::{
    CancelToken, FrozenPlan, JobError, JobHandler, RecoveryVerdict, StageOutcome, StageSpec,
    StageTerminal,
};
use crate::shared::context::RequestContext;
use crate::shared::id::{ArtifactId, ExecutionId, StageId};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tokio::io::AsyncReadExt;

pub struct FileJobOutput {
    pub path: PathBuf,
    pub rows: u64,
}
#[async_trait::async_trait]
pub trait FileJobOperation: Send + Sync {
    /// The path is allocated and owned by the host, never deserialized from a request.
    async fn run(&self, cancel: &CancelToken) -> Result<FileJobOutput, FeatureError>;
}
pub struct FileArtifactJob {
    kind: String,
    context: RequestContext,
    artifacts: Arc<dyn ArtifactRepository>,
    operation: Arc<dyn FileJobOperation>,
    output: Mutex<Option<(ArtifactId, u64)>>,
}
impl FileArtifactJob {
    pub fn new(
        kind: &str,
        context: RequestContext,
        artifacts: Arc<dyn ArtifactRepository>,
        operation: Arc<dyn FileJobOperation>,
    ) -> Self {
        Self {
            kind: kind.into(),
            context,
            artifacts,
            operation,
            output: Mutex::new(None),
        }
    }
    pub fn output(&self) -> Option<(ArtifactId, u64)> {
        self.output.lock().ok()?.clone()
    }
    async fn publish(
        &self,
        output: FileJobOutput,
        cancel: &CancelToken,
    ) -> Result<ArtifactId, FeatureError> {
        let mut file = tokio::fs::File::open(&output.path).await?;
        let spec = ArtifactSpec::new(
            self.context.organization_id.clone(),
            self.context.principal_id.clone(),
            ExecutionId::new(uuid::Uuid::new_v4().to_string()),
            format!("file-job:{}", self.kind),
        );
        let (_, mut writer) = self
            .artifacts
            .create(&self.context, spec)
            .await
            .map_err(port_error)?;
        let result = async {
            let mut buffer = vec![0; 64 * 1024];
            loop {
                if cancel.is_cancelled() {
                    return Err(FeatureError::Validation("fileJobCancelled".into()));
                }
                let count = file.read(&mut buffer).await?;
                if count == 0 {
                    break;
                }
                self.artifacts
                    .append_chunk(&mut writer, ChunkPayload::new(buffer[..count].to_vec()))
                    .await
                    .map_err(port_error)?;
            }
            Ok::<(), FeatureError>(())
        }
        .await;
        if let Err(error) = result {
            let reason = if cancel.is_cancelled() {
                AbortReason::Cancelled
            } else {
                AbortReason::ExecutionFailed
            };
            let _ = self.artifacts.abort(writer, reason).await;
            return Err(error);
        }
        let artifact = self.artifacts.finalize(writer).await.map_err(port_error)?;
        *self
            .output
            .lock()
            .map_err(|_| FeatureError::Internal("file job result unavailable".into()))? =
            Some((artifact.clone(), output.rows));
        Ok(artifact)
    }
}
fn port_error(_: crate::shared::error::PortError) -> FeatureError {
    FeatureError::Internal("file artifact storage unavailable".into())
}
#[async_trait::async_trait]
impl JobHandler for FileArtifactJob {
    fn kind(&self) -> &str {
        &self.kind
    }
    fn handler_version(&self) -> u64 {
        1
    }
    fn validate_plan(&self, plan: &FrozenPlan) -> Result<Vec<StageSpec>, JobError> {
        if plan.kind != self.kind || plan.handler_version != 1 || plan.is_apply {
            return Err(JobError::PlanProjectionInvalid(
                "invalid file job plan".into(),
            ));
        }
        Ok(vec![StageSpec {
            stage_id: StageId::new("produceArtifact"),
            kind: self.kind.clone(),
            depends_on: vec![],
        }])
    }
    async fn run_stage(
        &self,
        spec: &StageSpec,
        cancel: &CancelToken,
    ) -> Result<StageOutcome, JobError> {
        let result = if cancel.is_cancelled() {
            Err(FeatureError::Validation("fileJobCancelled".into()))
        } else {
            match self.operation.run(cancel).await {
                Ok(output) => self.publish(output, cancel).await,
                Err(error) => Err(error),
            }
        };
        let (terminal, artifact_ids, error_code) = match result {
            Ok(artifact) => (StageTerminal::Succeeded, vec![artifact], None),
            Err(_) if cancel.is_cancelled() => (
                StageTerminal::Cancelled,
                vec![],
                Some(ExecutionErrorCode::Cancelled),
            ),
            Err(_) => (
                StageTerminal::Failed,
                vec![],
                Some(ExecutionErrorCode::HostRejected),
            ),
        };
        Ok(StageOutcome {
            stage_id: spec.stage_id.clone(),
            terminal,
            progress: Default::default(),
            commit_boundaries: vec![],
            execution_ids: vec![],
            artifact_ids,
            effect_outcome: EffectOutcome::NotStarted,
            error_code,
        })
    }
    fn verify_recovery(&self, _: &Checkpoint) -> RecoveryVerdict {
        RecoveryVerdict::Reject {
            reason: "file jobs are not automatically replayed".into(),
        }
    }
}
