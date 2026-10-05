//! JobHandler 协议：validatePlan / runStage / verifyRecovery（§10.1.1）。
//!
//! 冻结语义：
//!
//! * handler 按 `kind + handlerVersion` 注册；`validate_plan` 校验冻结计划
//!   （版本、fingerprint、能力）并给出阶段序列；`run_stage` 执行一个阶段并返回
//!   已确认提交边界、executionIds、Artifact 引用与阶段进度；`verify_recovery`
//!   依据 checkpoint 决定续跑/拒绝/人工核验。
//! * handler **不**直接改 JobState、不自续租、在未知效果后不自动重试（§2.3）。
//! * handler 不申请未计数资源：预算由 [`crate::job::JobRuntime`] 在 claim 前全组预留。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use datazen_platform_api::dto::execution::{EffectOutcome, ExecutionErrorCode};
use datazen_platform_api::dto::job::{Checkpoint, CommitBoundary, JobProgress};
use datazen_platform_api::id::{ArtifactId, ExecutionId, StageId};

use crate::job::error::JobError;
use crate::job::plan::FrozenPlan;

/// 取消信号。快照式轮询：runtime 与 handler 都在阶段边界/批次边界检查。
#[derive(Debug, Clone)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

impl Default for CancelToken {
    fn default() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }
}

/// 阶段规格：handler 由 validate_plan 产出。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageSpec {
    pub stage_id: StageId,
    pub kind: String,
    pub depends_on: Vec<StageId>,
}

/// 阶段终态。不写成终 JobState：阶段可以多个，每个有自己的结局。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageTerminal {
    Succeeded,
    Failed,
    Cancelled,
    Unknown,
}

/// run_stage 的产出。handler 只汇报，不落库；runtime 负责把它写成
/// commit_boundary / checkpoint / CAS（§10.1.1「handler 不直接修改 Job 状态」）。
#[derive(Debug, Clone)]
pub struct StageOutcome {
    pub stage_id: StageId,
    pub terminal: StageTerminal,
    pub progress: JobProgress,
    pub commit_boundaries: Vec<CommitBoundary>,
    pub execution_ids: Vec<ExecutionId>,
    pub artifact_ids: Vec<ArtifactId>,
    pub effect_outcome: EffectOutcome,
    pub error_code: Option<ExecutionErrorCode>,
}

/// verify_recovery 的裁决（恢复决策表 §10.1.1）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryVerdict {
    /// 核验已通过：可补边界后继续。`resume_through` 是已确认边界在 checkpoint 中的位序。
    ResumeAfterVerify { resume_through: usize },
    /// 版本/能力/源变化：拒绝续跑，不自动升级。
    Reject { reason: String },
    /// 证据不足或未知提交：转人工核验，不自动重跑。
    RequireManualReview { reason: String },
}

/// 注册键：kind + handlerVersion。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct HandlerKey {
    kind: String,
    handler_version: u64,
}

/// JobHandler 注册表。未注册的 kind/version 一律拒绝，不静默回退到另一个版本。
#[derive(Default)]
pub struct HandlerRegistry {
    handlers: HashMap<HandlerKey, Arc<dyn JobHandler>>,
}

impl HandlerRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, handler: Arc<dyn JobHandler>) {
        self.handlers.insert(
            HandlerKey {
                kind: handler.kind().to_string(),
                handler_version: handler.handler_version(),
            },
            handler,
        );
    }

    pub fn resolve(&self, kind: &str, handler_version: u64) -> Option<Arc<dyn JobHandler>> {
        self.handlers
            .get(&HandlerKey {
                kind: kind.to_string(),
                handler_version: handler_version,
            })
            .cloned()
    }
}

/// JobHandler 协议（§10.1.1 冻结语义）。
#[async_trait::async_trait]
pub trait JobHandler: Send + Sync {
    fn kind(&self) -> &str;
    fn handler_version(&self) -> u64;

    /// 校验冻结计划的版本、指纹与能力；通过后返回阶段序列。
    /// handler 不得把「版本不兼容」伪装成通过——必须 Err。
    fn validate_plan(&self, plan: &FrozenPlan) -> Result<Vec<StageSpec>, JobError>;

    /// 执行一个阶段。通过 `cancel` 协作式取消；未知提交必须返回
    /// `StageTerminal::Unknown` + `EffectOutcome::Unknown`，不得自行改写为成功。
    async fn run_stage(
        &self,
        spec: &StageSpec,
        cancel: &CancelToken,
    ) -> Result<StageOutcome, JobError>;

    /// 依据 checkpoint 给出恢复裁决。不能核验的副作用 → RequireManualReview；
    /// 版本不兼容 → Reject；**未知提交不自动重跑**。
    fn verify_recovery(&self, checkpoint: &Checkpoint) -> RecoveryVerdict;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct EchoHandler;

    #[async_trait::async_trait]
    impl JobHandler for EchoHandler {
        fn kind(&self) -> &str {
            "schemaDiffApply"
        }
        fn handler_version(&self) -> u64 {
            1
        }
        fn validate_plan(&self, _plan: &FrozenPlan) -> Result<Vec<StageSpec>, JobError> {
            Ok(vec![StageSpec {
                stage_id: StageId::new("s1"),
                kind: "apply".into(),
                depends_on: vec![],
            }])
        }
        async fn run_stage(
            &self,
            spec: &StageSpec,
            _cancel: &CancelToken,
        ) -> Result<StageOutcome, JobError> {
            Ok(StageOutcome {
                stage_id: spec.stage_id.clone(),
                terminal: StageTerminal::Succeeded,
                progress: JobProgress::default(),
                commit_boundaries: vec![],
                execution_ids: vec![],
                artifact_ids: vec![],
                effect_outcome: EffectOutcome::Completed,
                error_code: None,
            })
        }
        fn verify_recovery(&self, _checkpoint: &Checkpoint) -> RecoveryVerdict {
            RecoveryVerdict::ResumeAfterVerify { resume_through: 0 }
        }
    }

    #[test]
    fn registry_resolves_by_kind_and_version() {
        let mut registry = HandlerRegistry::new();
        registry.register(Arc::new(EchoHandler));
        assert!(registry.resolve("schemaDiffApply", 1).is_some());
        assert!(registry.resolve("schemaDiffApply", 2).is_none());
        assert!(registry.resolve("dataSyncApply", 1).is_none());
    }
}
