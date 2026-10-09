//! §7 故障窗口决策：verifyRecovery 的纯判定核心。
//!
//! 规则（与 data-migration-jobs.md §7 表一一对应）：
//!
//! * 版本/能力变化 → Reject（不自动升级）；
//! * 所有已提交边界都已确认（verified_at 已写）→ ResumeAfterVerify；
//! * 任一边界带 `ddlResponseLost` 证据但未核验 → RequireManualReview（先只读比较）；
//! * 任一边界带 `commitAckLost` 证据但未核验 → RequireManualReview（只读比较后补边界或暂停）；
//! * 空 checkpoint 且恢复策略未声明 → RequireManualReview。

use datazen_platform_api::dto::job::Checkpoint;
use datazen_runtime::job::RecoveryVerdict;

/// checkpoint 证据标记（由 apply 阶段的 handler 在边界里写入）。
pub const EVIDENCE_DDL_RESPONSE_LOST: &str = "ddlResponseLost";
pub const EVIDENCE_COMMIT_ACK_LOST: &str = "commitAckLost";
pub const EVIDENCE_BOUNDARY_VERIFIED: &str = "boundaryVerified";
pub const EVIDENCE_VERSIONS_CHANGED: &str = "versionsChanged";
pub const EVIDENCE_CAPABILITY_CHANGED: &str = "capabilityChanged";

/// 纯函数：给定 checkpoint 判定恢复裁决。不能核验的副作用一律转人工核验，
/// **未知提交不自动重跑**。
pub fn decide_recovery(checkpoint: &Checkpoint) -> RecoveryVerdict {
    let evidence: Vec<&str> = checkpoint
        .verification_evidence
        .iter()
        .map(String::as_str)
        .collect();
    if evidence
        .iter()
        .any(|e| e.contains(EVIDENCE_VERSIONS_CHANGED))
    {
        return RecoveryVerdict::Reject {
            reason: "plan/credential versions changed; re-authorize and re-prepare".into(),
        };
    }
    if evidence
        .iter()
        .any(|e| e.contains(EVIDENCE_CAPABILITY_CHANGED))
    {
        return RecoveryVerdict::Reject {
            reason: "driver capability changed; re-authorize and re-prepare".into(),
        };
    }
    // 任一层证据含未核验的 DDL 响应丢失 / commit 应答丢失 → 只读核验后才可补边界或暂停。
    let any_ddl_lost = evidence
        .iter()
        .any(|e| e.contains(EVIDENCE_DDL_RESPONSE_LOST))
        || checkpoint.committed.iter().any(|b| {
            b.evidence
                .iter()
                .any(|e| e.contains(EVIDENCE_DDL_RESPONSE_LOST))
        });
    let any_ack_lost = evidence
        .iter()
        .any(|e| e.contains(EVIDENCE_COMMIT_ACK_LOST))
        || checkpoint.committed.iter().any(|b| {
            b.evidence
                .iter()
                .any(|e| e.contains(EVIDENCE_COMMIT_ACK_LOST))
        });
    if any_ddl_lost {
        return RecoveryVerdict::RequireManualReview {
            reason: "ddl response lost; run read-only before/after comparison before resuming"
                .into(),
        };
    }
    if any_ack_lost {
        return RecoveryVerdict::RequireManualReview {
            reason: "commit ack lost; run read-only comparison before filling boundary".into(),
        };
    }
    if checkpoint.committed.is_empty() {
        // 没有已确认边界：能否继续取决于恢复策略是否允许只读核验。
        if checkpoint.recovery_policy.contains("readOnlyVerify") {
            return RecoveryVerdict::ResumeAfterVerify { resume_through: 0 };
        }
        return RecoveryVerdict::RequireManualReview {
            reason: "no committed boundaries and recovery policy is not read-only-verifiable"
                .into(),
        };
    }
    let mut resume_through = 0usize;
    for boundary in &checkpoint.committed {
        let verified = boundary.verified_at.is_some()
            || boundary
                .evidence
                .iter()
                .any(|e| e.contains(EVIDENCE_BOUNDARY_VERIFIED));
        if !verified {
            return RecoveryVerdict::RequireManualReview {
                reason: format!(
                    "boundary {} lacks verification evidence",
                    boundary.operation_id.as_deref().unwrap_or("?")
                ),
            };
        }
        resume_through += 1;
    }
    RecoveryVerdict::ResumeAfterVerify { resume_through }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_platform_api::dto::job::{Checkpoint, CommitBoundary};
    use datazen_platform_api::id::{JobId, JobStateVersion, StageId, Timestamp};

    fn cp(policy: &str, evidence: Vec<&str>, committed: Vec<CommitBoundary>) -> Checkpoint {
        Checkpoint {
            job_id: JobId::new("j1"),
            state_version: JobStateVersion::new(1),
            stable_target_fingerprint: "fp".into(),
            committed,
            verification_evidence: evidence.into_iter().map(String::from).collect(),
            recovery_policy: policy.into(),
        }
    }

    fn boundary(op: &str, evidence: Vec<&str>, verified: bool) -> CommitBoundary {
        CommitBoundary {
            stage_id: StageId::new("apply"),
            stable_target_fingerprint: "fp".into(),
            committed_at: Timestamp::new("2026-01-01T00:00:00Z"),
            operation_id: Some(op.into()),
            batch_id: None,
            payload_digest: None,
            evidence: evidence.into_iter().map(String::from).collect(),
            verified_at: verified.then(|| Timestamp::new("2026-01-01T00:00:01Z")),
        }
    }

    #[test]
    fn versions_changed_rejects() {
        let c = cp("readOnlyVerify", vec!["versionsChanged"], vec![]);
        assert!(matches!(
            decide_recovery(&c),
            RecoveryVerdict::Reject { .. }
        ));
    }
    #[test]
    fn capability_changed_rejects() {
        let c = cp("readOnlyVerify", vec!["capabilityChanged"], vec![]);
        assert!(matches!(
            decide_recovery(&c),
            RecoveryVerdict::Reject { .. }
        ));
    }

    #[test]
    fn all_verified_resume() {
        let c = cp(
            "readOnlyVerify",
            vec![],
            vec![boundary("op-1", vec!["boundaryVerified"], true)],
        );
        assert!(matches!(
            decide_recovery(&c),
            RecoveryVerdict::ResumeAfterVerify { resume_through: 1 }
        ));
    }

    #[test]
    fn ddl_response_lost_requires_manual_review() {
        let c = cp(
            "readOnlyVerify",
            vec![],
            vec![boundary("op-2", vec!["ddlResponseLost"], false)],
        );
        assert!(matches!(
            decide_recovery(&c),
            RecoveryVerdict::RequireManualReview { .. }
        ));
    }

    #[test]
    fn commit_ack_lost_requires_manual_review() {
        let c = cp(
            "readOnlyVerify",
            vec![],
            vec![boundary("op-3", vec!["commitAckLost"], false)],
        );
        assert!(matches!(
            decide_recovery(&c),
            RecoveryVerdict::RequireManualReview { .. }
        ));
    }
}
