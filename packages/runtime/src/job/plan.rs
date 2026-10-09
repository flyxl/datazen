//! 冻结计划投影：prepare/apply 分开受理（§2.1）、planId 唯一消费、版本守卫。

use serde_json::Value;

use crate::job::error::JobError;

/// apply 类 Job 的 kind 集合。只有这些 kind 消费 planId（§2.1）。
pub const APPLY_KINDS: [&str; 3] = ["schemaDiffApply", "dataSyncApply", "dataTransferApply"];

/// 当前运行时支持 freeze 计划的 planVersion major。
pub const SUPPORTED_PLAN_MAJOR: u64 = 1;

/// 从 `JobDefinition.payload` 提取的计划投影。运行时只读这个投影，不信任客户端正文以外内容。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrozenPlan {
    pub kind: String,
    pub is_apply: bool,
    /// apply 类必填；prepare 类必须为空（不写 consumedPlanId）。
    pub consumed_plan_id: Option<String>,
    pub plan_version: u64,
    pub handler_version: u64,
    pub checkpoint_version: u64,
    pub selection_revision: Option<u64>,
}

/// 解析并校验计划投影。规则：
///
/// * 三个版本字段必须存在且为整数；未知 major（ != [`SUPPORTED_PLAN_MAJOR`] ）一律拒绝，
///   不自动升级、不自动降级（§10.1.1「固定版本与授权不变才继续」）。
/// * apply 类必须携带非空 `consumedPlanId`，prepare 类不得携带。
/// * 缺字段或类型错误 → `PlanProjectionInvalid`。
pub fn project_frozen_plan(kind: &str, payload: &Value) -> Result<FrozenPlan, JobError> {
    let is_apply = APPLY_KINDS.contains(&kind);
    let consumed_plan_id = match payload.get("consumedPlanId") {
        None => None,
        Some(Value::String(s)) if !s.trim().is_empty() => Some(s.clone()),
        Some(_) => {
            return Err(JobError::PlanProjectionInvalid(
                "consumedPlanId must be a non-empty string".into(),
            ))
        }
    };
    if is_apply && consumed_plan_id.is_none() {
        return Err(JobError::PlanProjectionInvalid(
            "apply job requires consumedPlanId".into(),
        ));
    }
    if !is_apply && consumed_plan_id.is_some() {
        return Err(JobError::PlanProjectionInvalid(
            "prepare job must not carry consumedPlanId".into(),
        ));
    }
    let plan_version = required_u64(payload, "planVersion")?;
    let handler_version = required_u64(payload, "handlerVersion")?;
    let checkpoint_version = required_u64(payload, "checkpointVersion")?;
    for got in [plan_version, handler_version, checkpoint_version] {
        if got != SUPPORTED_PLAN_MAJOR {
            return Err(JobError::VersionIncompatible {
                got,
                supported: SUPPORTED_PLAN_MAJOR,
            });
        }
    }
    let selection_revision = match payload.get("selectionRevision") {
        None => None,
        Some(v) => Some(v.as_u64().ok_or_else(|| {
            JobError::PlanProjectionInvalid("selectionRevision must be u64".into())
        })?),
    };
    Ok(FrozenPlan {
        kind: kind.to_string(),
        is_apply,
        consumed_plan_id,
        plan_version,
        handler_version,
        checkpoint_version,
        selection_revision,
    })
}

fn required_u64(payload: &Value, field: &str) -> Result<u64, JobError> {
    payload
        .get(field)
        .and_then(|v| v.as_u64())
        .ok_or_else(|| JobError::PlanProjectionInvalid(format!("{field} must be a u64")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn apply_plan_requires_consumed_plan_id_and_supported_versions() {
        let payload = json!({
            "consumedPlanId": "plan-1",
            "planVersion": 1,
            "handlerVersion": 1,
            "checkpointVersion": 1,
            "selectionRevision": 3,
        });
        let plan = project_frozen_plan("schemaDiffApply", &payload).expect("project");
        assert!(plan.is_apply);
        assert_eq!(plan.consumed_plan_id.as_deref(), Some("plan-1"));
        assert_eq!(plan.selection_revision, Some(3));
    }

    #[test]
    fn prepare_plan_must_not_consume_a_plan_id() {
        let payload = json!({
            "consumedPlanId": "plan-1",
            "planVersion": 1,
            "handlerVersion": 1,
            "checkpointVersion": 1,
        });
        assert_eq!(
            project_frozen_plan("schemaDiffPrepare", &payload).unwrap_err(),
            JobError::PlanProjectionInvalid("prepare job must not carry consumedPlanId".into())
        );
    }

    #[test]
    fn unknown_major_version_is_rejected() {
        let payload = json!({
            "planVersion": 2,
            "handlerVersion": 1,
            "checkpointVersion": 1,
        });
        assert_eq!(
            project_frozen_plan("dataSyncPrepare", &payload).unwrap_err(),
            JobError::VersionIncompatible {
                got: 2,
                supported: 1
            }
        );
    }
}
