//! 冻结 Transfer 计划校验（handler 内复验，§10.1.1：版本、fingerprint、能力）。

use datazen_runtime::job::{FrozenPlan, JobError};

use crate::error::TransferError;
use crate::model::{TransferJob, TransferMode};

pub const HANDLER_VERSION: u64 = 1;
pub const PLAN_VERSION: u64 = 1;
pub const CHECKPOINT_VERSION: u64 = 1;

/// handler 捕获的冻结体：版本字段 + 不可变 TransferJob + 恢复政策。
#[derive(Debug, Clone)]
pub struct TransferFreezeBody {
    pub job: TransferJob,
    /// 列映射与顺序的稳定摘要（同 schema/列映射两个 fingerprint 思想）。
    pub mapping_fingerprint: String,
    /// `resumeAfterVerify` / `forbidAutoResume`。来自准备期源一致性证据裁决。
    pub recovery_policy: String,
    /// 结构阶段 IR DAG 是否存在（mode 含 Structure 时必填）。
    pub has_structure_ir: bool,
    /// 每个参与表的稳定完整 key 列。空 → 禁止自动续写。
    pub stable_key_columns: Vec<Vec<String>>,
    /// driver 是否能证明一致性快照（PG ordinary / MySQL InnoDB / 等价证明）。
    pub snapshot_proven: bool,
}

/// 失败时统一转 JobError：版本 → VersionIncompatible；其余 → 能力/计划无效。
pub fn validate_frozen_plan(
    plan: &FrozenPlan,
    body: &TransferFreezeBody,
    is_apply_handler: bool,
) -> Result<(), JobError> {
    // 未知 major 拒绝（runtime 已守卫一次，handler 必须再复验）。
    for got in [
        plan.plan_version,
        plan.handler_version,
        plan.checkpoint_version,
    ] {
        if got != 1 {
            return Err(JobError::VersionIncompatible { got, supported: 1 });
        }
    }
    let expected = if is_apply_handler {
        "dataTransferApply"
    } else {
        "dataTransferPrepare"
    };
    if plan.kind != expected {
        return Err(JobError::PlanProjectionInvalid(format!(
            "plan kind '{}' does not match handler kind '{expected}'",
            plan.kind
        )));
    }
    if is_apply_handler && plan.consumed_plan_id.is_none() {
        return Err(JobError::PlanProjectionInvalid(
            "apply plan requires consumedPlanId".into(),
        ));
    }
    if !is_apply_handler && plan.consumed_plan_id.is_some() {
        return Err(JobError::PlanProjectionInvalid(
            "prepare plan must not carry consumedPlanId".into(),
        ));
    }
    // 冻结字段守卫：目的地、写入模式、映射摘要非空、恢复政策合法、
    // 结构阶段 IR DAG 存在（Structure 模式）、稳定 key 与快照证据一致。
    body.job
        .validate_destination()
        .map_err(|e: TransferError| JobError::PlanProjectionInvalid(e.to_string()))?;
    if body.mapping_fingerprint.trim().is_empty() {
        return Err(JobError::PlanProjectionInvalid(
            "mapping fingerprint must be non-empty".into(),
        ));
    }
    if !matches!(
        body.recovery_policy.as_str(),
        "resumeAfterVerify" | "forbidAutoResume" | "requireManualReview"
    ) {
        return Err(JobError::PlanProjectionInvalid(format!(
            "unsupported recovery policy '{}'",
            body.recovery_policy
        )));
    }
    let needs_structure = matches!(
        body.job.mode,
        TransferMode::Structure | TransferMode::StructureAndData
    );
    if needs_structure && !body.has_structure_ir {
        return Err(JobError::PlanProjectionInvalid(
            "structure stage requires the frozen IR DAG".into(),
        ));
    }
    let needs_data = matches!(
        body.job.mode,
        TransferMode::Data | TransferMode::StructureAndData
    );
    if needs_data {
        let keys_ok = body.stable_key_columns.iter().all(|keys| !keys.is_empty())
            && body.stable_key_columns.len()
                >= body
                    .job
                    .tables
                    .iter()
                    .filter(|t| t.enabled)
                    .count()
                    .min(body.stable_key_columns.len());
        // 任何表缺完整稳定 key ⇒ 禁止声称 resumeAfterVerify；此时只允许 forbidAutoResume。
        let any_missing_key = body.stable_key_columns.iter().any(|keys| keys.is_empty())
            || body.stable_key_columns.len() < body.job.tables.iter().filter(|t| t.enabled).count();
        if body.recovery_policy == "resumeAfterVerify" && (any_missing_key || !body.snapshot_proven)
        {
            return Err(JobError::CapabilityUnsupported(format!(
                "recovery policy 'resumeAfterVerify' requires complete stable keys and a proven snapshot; table has keys={}, snapshotProven={}",
                !any_missing_key, body.snapshot_proven
            )));
        }
        let _ = keys_ok;
    }
    let _ = needs_data;
    Ok(())
}
