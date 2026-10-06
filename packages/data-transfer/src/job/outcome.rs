//! 阶段终态构造（§7）：失败与字节越界两条终态共用同一份「保留已确认边界」规则。
//!
//! 从 `handler.rs` 拆出：handler 已经贴着单文件规模上限，这三类终态没有 handler 状态。

use datazen_platform_api::dto::execution::{EffectOutcome, ExecutionErrorCode};
use datazen_platform_api::dto::job::{CommitBoundary, JobProgress};
use datazen_platform_api::id::Counter;
use datazen_runtime::job::{StageOutcome, StageSpec, StageTerminal};

use crate::job::pipeline::PIPELINE_INITIAL_BYTES;

/// 阶段失败：本阶段没有提交任何东西，所以 [`EffectOutcome::RolledBack`] 成立。
/// `StageOutcome` 不带失败正文（§10.1.1），细节只能落日志。
pub(crate) fn failed_stage(spec: &StageSpec, message: impl std::fmt::Display) -> StageOutcome {
    tracing::error!(
        stage = spec.stage_id.as_str(),
        reason = %message,
        "transfer stage failed"
    );
    StageOutcome {
        stage_id: spec.stage_id.clone(),
        terminal: StageTerminal::Failed,
        progress: JobProgress::default(),
        commit_boundaries: Vec::new(),
        execution_ids: Vec::new(),
        artifact_ids: Vec::new(),
        effect_outcome: EffectOutcome::RolledBack,
        error_code: Some(ExecutionErrorCode::SqlError),
    }
}

/// 字节账越界（§6.2）：显式失败，但按 §7 保留本阶段已确认的提交边界——已提交的那部分
/// 不回滚，也不许被记成 [`EffectOutcome::RolledBack`]。
pub(crate) fn unbounded_stage(
    spec: &StageSpec,
    boundaries: &[CommitBoundary],
    peak_bytes: usize,
) -> StageOutcome {
    tracing::error!(
        stage = spec.stage_id.as_str(),
        peak_bytes,
        budget_bytes = PIPELINE_INITIAL_BYTES,
        "pipeline buffer accounting exceeded its bound"
    );
    StageOutcome {
        stage_id: spec.stage_id.clone(),
        terminal: StageTerminal::Failed,
        progress: JobProgress::default(),
        commit_boundaries: boundaries.to_vec(),
        execution_ids: Vec::new(),
        artifact_ids: Vec::new(),
        effect_outcome: EffectOutcome::PartiallyApplied,
        error_code: Some(ExecutionErrorCode::SqlError),
    }
}

/// 用户取消（§7）：取消是既成事实，不是失败；已确认的提交边界照 §7 保留，
/// 什么都没提交时才是 [`EffectOutcome::NotStarted`]。
pub(crate) fn cancelled_stage(
    spec: &StageSpec,
    boundaries: &[CommitBoundary],
    message: impl std::fmt::Display,
) -> StageOutcome {
    tracing::info!(
        stage = spec.stage_id.as_str(),
        reason = %message,
        committed_boundaries = boundaries.len(),
        "transfer stage cancelled"
    );
    StageOutcome {
        stage_id: spec.stage_id.clone(),
        terminal: StageTerminal::Cancelled,
        progress: JobProgress::default(),
        commit_boundaries: boundaries.to_vec(),
        execution_ids: Vec::new(),
        artifact_ids: Vec::new(),
        effect_outcome: if boundaries.is_empty() {
            EffectOutcome::NotStarted
        } else {
            EffectOutcome::PartiallyApplied
        },
        error_code: None,
    }
}

/// 把一张表的管道结果并进阶段进度：五个计数器逐项饱和相加。
pub(crate) fn absorb_progress(progress: &mut JobProgress, delta: &JobProgress) {
    for (target, add) in [
        (&mut progress.read, delta.read.get()),
        (&mut progress.converted, delta.converted.get()),
        (&mut progress.attempted, delta.attempted.get()),
        (&mut progress.committed, delta.committed.get()),
        (&mut progress.unknown, delta.unknown.get()),
    ] {
        *target = Counter::new(target.get().saturating_add(add));
    }
}
